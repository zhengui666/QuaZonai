//! Apply frozen targets in one native Nautilus simulated account, in replay or Paper.
use crate::catalog::{load_catalog, NativeBarSeries, NativeMarketData};
pub use crate::prediction::FillFeeFailure;
use anyhow::{ensure, Result};
use bigdecimal::{BigDecimal, ToPrimitive};
use contracts::{science::*, DbCounter, DecimalValue, SchemaV1};
use nautilus_analysis::{
    analyzer::{PortfolioAnalyzer, Statistic},
    statistics::{
        max_drawdown::MaxDrawdown, returns_volatility::ReturnsVolatility,
        sharpe_ratio::SharpeRatio, sortino_ratio::SortinoRatio,
    },
};
use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
};
use nautilus_common::{
    actor::DataActor,
    logging::logger::LoggerConfig,
    timer::{TimeEvent, TimeEventCallback},
};
use nautilus_core::DurationNanos;
#[cfg(feature = "native-paper")]
use nautilus_execution::models::fee::MakerTakerFeeModel;
use nautilus_execution::models::{
    fill::{DefaultFillModel, FillModelHandle},
    latency::{LatencyModelHandle, StaticLatencyModel},
};
use nautilus_model::{
    accounts::{Account, AccountAny},
    data::{Bar, BarType, CustomData, DataBatch, DataType, InstrumentClose, InstrumentStatus},
    enums::{
        AccountType, BookType, MarketStatusAction, OmsType, OrderSide, OrderStatus, TimeInForce,
    },
    events::{
        OrderCanceled, OrderDenied, OrderFilled, OrderRejected, PositionChanged, PositionClosed,
        PositionOpened,
    },
    fees::MakerTakerFeeSchedule,
    identifiers::{AccountId, ClientId, ClientOrderId, InstrumentId, StrategyId, Venue},
    instruments::{Instrument, InstrumentAny},
    orders::{Order, OrderAny},
    types::{Currency, Money},
};
use nautilus_portfolio::config::PortfolioConfig;
use nautilus_trading::{
    nautilus_strategy,
    strategy::{Strategy, StrategyConfig, StrategyCore},
};
use rust_decimal::Decimal;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::Path,
    rc::Rc,
    str::FromStr,
    sync::Arc,
};

pub(crate) mod calendar_returns;

fn native_decimal(value: &DecimalValue) -> Result<Decimal> {
    Decimal::from_str(&value.as_decimal().to_plain_string())
        .map_err(|_| anyhow::anyhow!("NATIVE_DECIMAL_RANGE"))
}
fn checked(value: Option<Decimal>) -> Result<Decimal> {
    value.ok_or_else(|| anyhow::anyhow!("NATIVE_QUANTITY_RANGE"))
}
fn count(value: usize) -> Result<DbCounter> {
    DbCounter::new(u64::try_from(value)?).map_err(anyhow::Error::msg)
}

fn recorded_liquidity(settings: &NativeSimulationSettingsV1) -> bool {
    matches!(
        settings.fill_model,
        contracts::portfolio::NativeModelRefV1::PolymarketV2RecordedLiquidity { .. }
    )
}

fn settlement_batches(
    closes: impl IntoIterator<Item = InstrumentClose>,
) -> impl Iterator<Item = DataBatch> {
    closes.into_iter().flat_map(|close| {
        // This companion derives only from the verified close and preserves both clocks.
        // Close trading before the native close handler can iterate resting orders.
        let status = InstrumentStatus::new(
            close.instrument_id,
            MarketStatusAction::Close,
            close.ts_event,
            close.ts_init,
            None,
            None,
            Some(false),
            None,
            None,
        );
        // Keep each status/close pair in original source order, including ties
        // between instruments. Grouping all statuses first would reorder ties.
        [
            DataBatch::InstrumentStatus(vec![status].into()),
            DataBatch::InstrumentClose(vec![close].into()),
        ]
    })
}

#[cfg(test)]
#[path = "../tests/support/polymarket.rs"]
mod replay_fixture;

#[derive(Default)]
pub(crate) struct ReplayStatus {
    pub(crate) consumed: usize,
    pub(crate) failure: Option<&'static str>,
    callback_error: Option<anyhow::Error>,
    order_rejection: Option<String>,
    submitted_after_ns: u64,
    study_infeasible: bool,
    settled_replay_ns: Option<u64>,
    frames: Vec<NativePortfolioStudyFrameV1>,
    spot_fills: Vec<OrderFilled>,
}

impl ReplayStatus {
    fn recorded_replay_terminal(&self, data_complete: bool, expected_targets: usize) -> bool {
        self.failure.is_none()
            && (self.study_infeasible || (data_complete && self.consumed == expected_targets))
    }

    fn record_callback_error(&mut self, error: anyhow::Error) {
        if self.failure.is_none() {
            self.failure = Some("NATIVE_TARGET_REPLAY_FAILED");
            self.callback_error = Some(error);
        }
    }

    pub(crate) fn check_callback_error(&mut self) -> Result<()> {
        if let Some(error) = self.callback_error.take() {
            return Err(error.context("NATIVE_TARGET_REPLAY_FAILED"));
        }
        Ok(())
    }

    fn finish_run(&mut self, native_result: Result<()>) -> Result<()> {
        self.check_callback_error()?;
        if let Some(code) = self.failure {
            let error = match (code, self.order_rejection.as_deref()) {
                ("NATIVE_ORDER_REJECTED", Some(reason)) => anyhow::anyhow!("{code}: {reason}"),
                _ => anyhow::anyhow!(code),
            };
            return Err(error.context("NATIVE_TARGET_REPLAY_FAILED"));
        }
        native_result
    }
}

pub(crate) struct StudyInput {
    pub cutoff_ns: DbCounter,
    pub input: contracts::portfolio::AllocationInputV1,
    pub slippage: Vec<NativePortfolioSlippageReferenceV1>,
    pub bar_notionals: Vec<contracts::execution::NativeBarNotionalV1>,
    pub liquidity_maximum_age: Option<u32>,
}

pub(crate) struct CatalogReplayFrame {
    pub instruments: Vec<InstrumentAny>,
    pub point: NativeTargetPointV1,
    pub bar_end_ns: u64,
}

pub(crate) struct CatalogReplayInbox {
    pub pending: Option<CatalogReplayFrame>,
    pub consumed: usize,
    gross_exposure: Decimal,
    bar_end_ns: u64,
}

pub(crate) struct TargetReplay {
    core: StrategyCore,
    instruments: Vec<InstrumentAny>,
    bar_types: Vec<BarType>,
    latest: BTreeMap<InstrumentId, Bar>,
    points: Vec<NativeTargetPointV1>,
    study_inputs: Vec<StudyInput>,
    strategy_constraints: Option<(
        contracts::portfolio::PortfolioConstraintsV1,
        String,
        DecimalValue,
    )>,
    settings: NativeSimulationSettingsV1,
    spot_cash: Option<crate::spot_cash_capture::SpotCashCaptureHandle>,
    currency: Currency,
    venue: Venue,
    tolerance: Decimal,
    latency_ns: u64,
    wake_order_arrivals: bool,
    reduction_ids: BTreeSet<ClientOrderId>,
    deferred_orders: Vec<OrderAny>,
    active_expiry_ns: u64,
    awaiting_settlement: bool,
    outstanding_orders: BTreeSet<ClientOrderId>,
    settlement_events: BTreeMap<InstrumentId, InstrumentClose>,
    status: Rc<RefCell<ReplayStatus>>,
    recorded_bar_ends: Option<BTreeMap<InstrumentId, u64>>,
    paper: Option<PaperClock>,
    capital_exit_gate: Option<(crate::capital_exit_gate::CapitalExitGate, DbCounter)>,
    capital_exit_source_pending: bool,
    catalog: Option<Rc<RefCell<CatalogReplayInbox>>>,
}

struct PaperClock {
    execution_client_id: ClientId,
    started_ns: u64,
    fresh_account: Option<contracts::strategy_portfolio::FreshPaperCashV1>,
    installed_fee_policy: Option<PaperMakerTakerPolicy>,
}

/// The Paper host supplies the policy from the model installed in its execution
/// client. A target's requested rates are never treated as their own source.
pub(crate) struct PaperMakerTakerPolicy {
    venue: Venue,
    schedule: MakerTakerFeeSchedule,
}

impl PaperMakerTakerPolicy {
    #[cfg(feature = "native-paper")]
    pub(crate) fn from_model(venue: Venue, model: &MakerTakerFeeModel) -> Self {
        Self {
            venue,
            schedule: model.schedule().clone(),
        }
    }
}

impl fmt::Debug for TargetReplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TargetReplay")
            .field("instruments", &self.instruments.len())
            .field("target_points", &self.points.len())
            .finish_non_exhaustive()
    }
}

nautilus_strategy!(TargetReplay, {
    fn on_order_denied(&mut self, _event: OrderDenied) {
        self.status.borrow_mut().failure = Some("NATIVE_ORDER_DENIED");
    }
    fn on_order_rejected(&mut self, event: OrderRejected) {
        let mut status = self.status.borrow_mut();
        status.order_rejection = Some(event.reason.to_string());
        status.failure = Some("NATIVE_ORDER_REJECTED");
    }
    fn on_order_canceled(&mut self, event: &OrderCanceled) {
        if !recorded_liquidity(&self.settings) {
            return;
        }
        self.outstanding_orders.remove(&event.client_order_id);
        if self.reduction_ids.remove(&event.client_order_id) {
            // A partial IOC reduction did not release the originally requested
            // margin. Abandon its dependent increases, never assume a full exit.
            self.deferred_orders.clear();
            if self.reduction_ids.is_empty() {
                self.status.borrow_mut().consumed += 1;
            }
        }
        self.resume_after_settlement(event.ts_init.as_u64());
    }
    fn on_order_filled(&mut self, event: &OrderFilled) {
        let now = event.ts_event.as_u64();
        if crate::prediction::is_native_settlement_order(event.client_order_id.as_str()) {
            let matches = self
                .settlement_events
                .get(&event.instrument_id)
                .is_some_and(|close| {
                    now >= close.ts_init.as_u64() && event.last_px == close.close_price
                });
            if !matches {
                self.status.borrow_mut().failure = Some("NATIVE_SETTLEMENT_SOURCE_MISMATCH");
            }
            return;
        }
        // Native BAR execution can extrapolate a remainder beyond the instrument
        // bounds. Reject that research result; never clamp or replace its fill.
        let valid_price = self
            .cache()
            .instrument(&event.instrument_id)
            .is_some_and(|instrument| {
                instrument.ts_init() <= event.ts_init
                    && event.last_px.precision == instrument.price_precision()
                    && instrument.try_normalize_price(event.last_px).is_ok()
                    && instrument
                        .min_price()
                        .is_none_or(|price| event.last_px >= price)
                    && instrument
                        .max_price()
                        .is_none_or(|price| event.last_px <= price)
            });
        if !valid_price {
            self.status.borrow_mut().failure = Some("NATIVE_FILL_OUTSIDE_INSTRUMENT");
            return;
        }
        // Original owner-issued reductions belong to the same native strategy
        // and account. They do not reactivate or inherit the expired target's
        // deferred buy instructions; Nautilus has already booked the original fill.
        if self.capital_exit_gate.as_ref().is_some_and(|(gate, _)| {
            gate.issued_order_ids()
                .iter()
                .any(|id| id == event.client_order_id.as_str())
        }) {
            return;
        }
        if now <= self.status.borrow().submitted_after_ns || now >= self.active_expiry_ns {
            self.status.borrow_mut().failure = Some("NATIVE_NONCAUSAL_OR_EXPIRED_FILL");
            return;
        }
        if self.spot_cash.is_some() {
            self.status.borrow_mut().spot_fills.push(event.clone());
        }
        let fully_filled = self
            .cache()
            .order(&event.client_order_id)
            .is_some_and(|order| order.status() == OrderStatus::Filled);
        if fully_filled {
            self.outstanding_orders.remove(&event.client_order_id);
        }
        if self.reduction_ids.contains(&event.client_order_id) && fully_filled {
            self.reduction_ids.remove(&event.client_order_id);
            if self.reduction_ids.is_empty() {
                if let Err(error) = self.submit_deferred(now) {
                    self.status
                        .borrow_mut()
                        .record_callback_error(error.context("NATIVE_DEFERRED_TARGET_FAILED"));
                }
            }
        }
    }
    fn on_position_opened(&mut self, event: PositionOpened) {
        self.resume_after_settlement(event.ts_init.as_u64());
    }
    fn on_position_changed(&mut self, event: PositionChanged) {
        self.resume_after_settlement(event.ts_init.as_u64());
    }
    fn on_position_closed(&mut self, event: PositionClosed) {
        self.resume_after_settlement(event.ts_init.as_u64());
    }
});

impl TargetReplay {
    #[cfg(feature = "native-paper")]
    pub(crate) fn require_strategy_constraints(
        &mut self,
        constraints: contracts::portfolio::PortfolioConstraintsV1,
        base_currency: String,
        tolerance: DecimalValue,
    ) {
        self.strategy_constraints = Some((constraints, base_currency, tolerance));
    }

    #[cfg(feature = "native-paper")]
    pub(crate) fn require_fresh_paper_account(
        &mut self,
        expected: contracts::strategy_portfolio::FreshPaperCashV1,
    ) -> Result<()> {
        let paper = self
            .paper
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("PAPER_ACCOUNT_MODE"))?;
        ensure!(
            expected.base_currency == self.settings.base_currency
                && expected.starting_capital == self.settings.starting_capital,
            "PAPER_ACCOUNT_SETTINGS"
        );
        paper.fresh_account = Some(expected);
        Ok(())
    }

    fn verify_fresh_paper_account(&self) -> Result<()> {
        let Some(expected) = self
            .paper
            .as_ref()
            .and_then(|paper| paper.fresh_account.as_ref())
        else {
            return Ok(());
        };
        let id = AccountId::new_checked(&expected.account_id)?;
        let account = self
            .cache()
            .account(&id)
            .ok_or_else(|| anyhow::anyhow!("PAPER_INITIAL_ACCOUNT_UNAVAILABLE"))?;
        let amount = native_decimal(&expected.starting_capital)?;
        ensure!(
            matches!(&account, AccountAny::Margin(margin) if margin.default_leverage == Decimal::ONE && margin.leverages.values().all(|v| *v == Decimal::ONE)),
            "PAPER_INITIAL_ACCOUNT_LEVERAGE"
        );
        ensure!(
            self.trader_id()
                .is_some_and(|id| id.to_string() == expected.trader_id)
                && account.id() == id
                && account.account_type() == AccountType::Margin
                && account.base_currency() == Some(self.currency)
                && account.currencies() == vec![self.currency]
                && account
                    .balance_total(Some(self.currency))
                    .is_some_and(|v| v.as_decimal() == amount)
                && account
                    .balance_free(Some(self.currency))
                    .is_some_and(|v| v.as_decimal() == amount)
                && account
                    .balance_locked(Some(self.currency))
                    .is_some_and(|v| v.as_decimal() == Decimal::ZERO)
                && self
                    .cache()
                    .positions_open(None, None, None, Some(&id), None)
                    .is_empty()
                && self
                    .cache()
                    .orders_open(None, None, None, Some(&id), None)
                    .is_empty()
                && self
                    .cache()
                    .orders_inflight(None, None, None, Some(&id), None)
                    .is_empty(),
            "PAPER_INITIAL_ACCOUNT_MISMATCH"
        );
        Ok(())
    }

    /// Host must install this before registering the strategy. All target submit
    /// paths (initial, deferred and position callbacks) share this exact gate.
    #[cfg(feature = "native-paper")]
    pub(crate) fn require_capital_exit_gate(
        &mut self,
        gate: crate::capital_exit_gate::CapitalExitGate,
        claim: &contracts::strategy_portfolio::HandoffClaimViewV2,
    ) -> Result<()> {
        let epoch = gate.capture_target_claim(claim)?;
        self.capital_exit_gate = Some((gate, epoch));
        Ok(())
    }

    #[cfg(feature = "native-paper")]
    pub(crate) fn await_capital_exit_source(&mut self) {
        self.capital_exit_source_pending = true;
    }

    #[cfg(feature = "native-paper")]
    pub(crate) fn authenticated_capital_exit_source(&mut self) {
        self.capital_exit_source_pending = false;
    }

    fn target_fenced(&self) -> bool {
        self.capital_exit_source_pending
            || self
                .capital_exit_gate
                .as_ref()
                .is_some_and(|(gate, _)| gate.control().is_some() || gate.recovery_required())
    }

    fn submit_target_order(&mut self, order: OrderAny) -> Result<()> {
        if let Some((gate, epoch)) = &self.capital_exit_gate {
            gate.check_target(*epoch, &order)?;
        }
        let client_id = if let Some(paper) = &self.paper {
            ensure!(
                self.clock().timestamp_ns().as_u64() < self.active_expiry_ns,
                "SIMULATION_TARGET_EXPIRED"
            );
            Some(paper.execution_client_id)
        } else {
            None
        };
        if self.paper.is_none() && self.wake_order_arrivals && self.latency_ns > 0 {
            let arrival = self
                .clock()
                .timestamp_ns()
                .as_u64()
                .checked_add(self.latency_ns)
                .ok_or_else(|| anyhow::anyhow!("NATIVE_LATENCY_TIME_RANGE"))?;
            let name = format!("RECORDED-INSERT:{}", order.client_order_id());
            let callback: Rc<dyn Fn(TimeEvent)> = Rc::new(|_| {});
            // Static latency queues the command but does not wake the engine.
            // Let the official timer loop settle it at arrival, not a later BAR
            // or account snapshot. This adds no market data or synthetic fill.
            self.clock().set_time_alert_ns(
                &name,
                arrival.into(),
                Some(TimeEventCallback::from(callback)),
                None,
            )?;
        }
        self.submit_order(order, None, client_id, None)
    }

    fn start_paper(&mut self) -> Result<()> {
        self.verify_fresh_paper_account()?;
        let now = self.clock().timestamp_ns().as_u64();
        let point = &self.points[0];
        ensure!(
            now < point.valid_until_ns.get(),
            "SIMULATION_TARGET_EXPIRED"
        );
        // LiveNode connects its clients before starting strategies. The native
        // cache, populated by those clients, is the authoritative instrument
        // source; no historical catalog is loaded or replayed for Paper.
        let instruments = point
            .targets
            .iter()
            .map(|target| {
                let id = InstrumentId::from_str(&target.instrument_id)?;
                self.cache()
                    .instrument(&id)
                    .ok_or_else(|| anyhow::anyhow!("PAPER_INSTRUMENT_UNAVAILABLE"))
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            instruments
                .iter()
                .all(|instrument| instrument.ts_init().as_u64() <= now),
            "SIMULATION_BASELINE_FROM_FUTURE"
        );
        validate_paper_instruments(
            &self.settings,
            &instruments,
            &self.bar_types,
            point,
            self.paper
                .as_ref()
                .and_then(|paper| paper.installed_fee_policy.as_ref()),
        )?;
        self.instruments = instruments;
        self.paper.as_mut().expect("Paper mode checked").started_ns = now;
        Ok(())
    }

    fn accept_new_paper_bar(&self, bar: &Bar) -> Result<bool> {
        let Some(paper) = &self.paper else {
            return Ok(true);
        };
        ensure!(
            self.bar_types.contains(&bar.bar_type),
            "PAPER_BAR_TYPE_MISMATCH"
        );
        let now = self.clock().timestamp_ns().as_u64();
        ensure!(
            bar.ts_event <= bar.ts_init && bar.ts_init.as_u64() <= now,
            "PAPER_BAR_FROM_FUTURE"
        );
        // Only callbacks for newly completed native bars enter this check.
        // Position-event settlement resumption retains the existing cached bar.
        Ok(bar.ts_event.as_u64() > paper.started_ns
            && self
                .latest
                .get(&bar.bar_type.instrument_id())
                .is_none_or(|previous| previous.ts_event < bar.ts_event))
    }

    fn orders_settled(&self) -> bool {
        // Initialized commands have not reached the native open/inflight indexes.
        self.outstanding_orders.is_empty()
            && self.cache().orders_open_count(None, None, None, None, None) == 0
            && self
                .cache()
                .orders_inflight_count(None, None, None, None, None)
                == 0
    }

    fn resume_after_settlement(&mut self, receipt_ns: u64) {
        if self.target_fenced() {
            return;
        }
        if !self.awaiting_settlement || !self.orders_settled() {
            self.finish_recorded_tail();
            return;
        }
        // Native position events follow order fills and the Portfolio's higher-
        // priority handler refreshes net positions before this strategy sees them.
        self.awaiting_settlement = false;
        if let Some(bar) = self.latest.values().max_by_key(|bar| bar.ts_init).copied() {
            if let Err(error) = self.apply_bar(&bar, receipt_ns) {
                self.status.borrow_mut().record_callback_error(error);
            }
        }
        self.finish_recorded_tail();
    }

    fn finish_recorded_tail(&self) {
        let Some(ends) = &self.recorded_bar_ends else {
            return;
        };
        let mut status = self.status.borrow_mut();
        let data_complete = ends
            .iter()
            .all(|(id, end)| self.latest.get(id).map(|bar| bar.ts_init.as_u64()) == Some(*end));
        let finished = status.recorded_replay_terminal(data_complete, self.points.len())
            && self.orders_settled();
        if finished && !status.study_infeasible {
            status
                .settled_replay_ns
                .get_or_insert_with(|| self.clock().timestamp_ns().as_u64());
        }
        drop(status);
        if finished {
            // The native engine stops at this settled timestamp, not after an
            // unrelated periodic snapshot sequence across a data-empty tail.
            self.shutdown_system(Some("RECORDED_REPLAY_SETTLED".into()));
        }
    }

    fn execution_price(
        &self,
        instrument_id: &InstrumentId,
        snapshot: Option<&crate::spot_cash_capture::SpotCashDecisionSnapshot>,
    ) -> Result<Decimal> {
        let bar = self
            .latest
            .get(instrument_id)
            .ok_or_else(|| anyhow::anyhow!("SIMULATION_MISSING_PRICE"))?;
        if let Some(snapshot) = snapshot {
            let policy = self
                .settings
                .multi_currency_spot_cash
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("SPOT_EXECUTION_POLICY_MISSING"))?;
            crate::spot_cash_runtime::execution_price(snapshot, policy, bar)
        } else {
            Ok(bar.close.as_decimal())
        }
    }

    fn submit_deferred(&mut self, now: u64) -> Result<()> {
        if self.target_fenced() {
            return Ok(());
        }
        let now = if self.paper.is_some() {
            self.clock().timestamp_ns().as_u64()
        } else {
            now
        };
        ensure!(
            self.reduction_ids.is_empty() && self.status.borrow().failure.is_none(),
            "SIMULATION_REDUCTIONS_NOT_CONFIRMED"
        );
        ensure!(
            now.checked_add(self.latency_ns)
                .is_some_and(|n| n < self.active_expiry_ns),
            "SIMULATION_TARGET_EXPIRES_BEFORE_INSERT"
        );
        if let Some(capture) = &self.spot_cash {
            let snapshot = capture.decision_snapshot()?;
            let free = crate::spot_cash_runtime::native_balance(&snapshot, self.currency, true)?;
            let mut requested = Decimal::ZERO;
            for order in &self.deferred_orders {
                ensure!(
                    order.order_side() == OrderSide::Buy,
                    "SPOT_INCREASE_MUST_BUY"
                );
                let price = self.execution_price(&order.instrument_id(), Some(&snapshot))?;
                requested = checked(
                    requested
                        .checked_add(checked(order.quantity().as_decimal().checked_mul(price))?),
                )?;
            }
            ensure!(requested <= free, "SPOT_INSUFFICIENT_NATIVE_QUOTE_CASH");
        }
        self.status.borrow_mut().submitted_after_ns = now;
        for order in std::mem::take(&mut self.deferred_orders) {
            self.outstanding_orders.insert(order.client_order_id());
            self.submit_target_order(order)?;
            ensure!(
                self.status.borrow().failure.is_none(),
                "NATIVE_ORDER_NOT_ACCEPTED"
            );
        }
        self.status.borrow_mut().consumed += 1;
        Ok(())
    }

    fn apply_bar(&mut self, bar: &Bar, receipt_ns: u64) -> Result<()> {
        let receipt_ns = if self.paper.is_some() {
            self.clock().timestamp_ns().as_u64()
        } else {
            receipt_ns
        };
        if self.status.borrow().study_infeasible {
            return Ok(());
        }
        ensure!(
            self.status.borrow().failure.is_none(),
            "SIMULATION_ALREADY_FAILED"
        );
        self.latest.insert(bar.bar_type.instrument_id(), *bar);
        if !self.reduction_ids.is_empty() {
            ensure!(
                receipt_ns < self.active_expiry_ns,
                "SIMULATION_REDUCTION_EXPIRED"
            );
            self.awaiting_settlement = true;
            return Ok(());
        }
        let next = self.status.borrow().consumed;
        let Some(point) = self.points.get(next) else {
            return Ok(());
        };
        if self.catalog.is_some() && receipt_ns < point.asof_ns.get() {
            return Ok(());
        }
        let now = if let Some(scope) = &point.rebalance_instrument_ids {
            // Only genuine observations at this decision's receive clock can
            // authorize orders. Wait for the entire same-clock scope, not an
            // unrelated instrument's first BAR or a synchronized cross-section.
            let mut ready = true;
            for id in scope {
                let id = InstrumentId::from_str(id)?;
                if let Some(observed) = self.latest.get(&id) {
                    ensure!(
                        observed.ts_init.as_u64() <= point.asof_ns.get(),
                        "SIMULATION_SCOPE_BAR_MISSING"
                    );
                    ensure!(
                        observed.ts_event <= observed.ts_init,
                        "SIMULATION_SCOPE_BAR_FROM_FUTURE"
                    );
                    ready &= if let Some(inbox) = &self.catalog {
                        observed.ts_event.as_u64() == inbox.borrow().bar_end_ns
                    } else {
                        observed.ts_init.as_u64() == point.asof_ns.get()
                    };
                } else {
                    ready = false;
                }
            }
            if !ready {
                return Ok(());
            }
            receipt_ns.max(point.asof_ns.get())
        } else {
            // Legacy frames require every asset's completed price at the same
            // event time. Keep their original synchronized semantics unchanged.
            if self.latest.len() != self.instruments.len()
                || self
                    .latest
                    .values()
                    .any(|other| other.ts_event != bar.ts_event)
            {
                return Ok(());
            }
            self.latest
                .values()
                .map(|v| v.ts_init.as_u64())
                .max()
                .ok_or_else(|| anyhow::anyhow!("SIMULATION_MISSING_PRICE"))?
                .max(receipt_ns)
        };
        if now < point.asof_ns.get() {
            return Ok(());
        }
        ensure!(
            now < point.valid_until_ns.get(),
            "SIMULATION_TARGET_EXPIRED"
        );
        ensure!(
            now.checked_add(self.latency_ns)
                .is_some_and(|n| n < point.valid_until_ns.get()),
            "SIMULATION_TARGET_EXPIRES_BEFORE_INSERT"
        );
        ensure!(
            self.points
                .get(next + 1)
                .is_none_or(|p| p.asof_ns.get() > now),
            "SIMULATION_TARGETS_COALESCED"
        );
        if !self.orders_settled() {
            // Nautilus delivers BAR data before draining queued execution events.
            // Wait for its position callback and settled cache before a new
            // target; never manufacture fills or calculate from pending equity.
            self.awaiting_settlement = true;
            return Ok(());
        }
        self.awaiting_settlement = false;
        if next == 0 {
            self.verify_fresh_paper_account()?;
        }
        let spot_snapshot = self
            .spot_cash
            .as_ref()
            .map(|capture| capture.decision_snapshot())
            .transpose()?;
        let equity = if let Some(snapshot) = &spot_snapshot {
            native_decimal(&snapshot.total)?
        } else {
            self.portfolio()
                .equity(&self.venue, None)
                .get(&self.currency)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("SIMULATION_EQUITY_UNAVAILABLE"))?
                .as_decimal()
        };
        ensure!(equity > Decimal::ZERO, "SIMULATION_NONPOSITIVE_EQUITY");
        // Most BARs only establish readiness. Own the target only once the
        // complete decision can use settled positions and native equity.
        let mut point = point.clone();
        if let Some(inbox) = &self.catalog {
            let ids = self
                .instruments
                .iter()
                .map(Instrument::id)
                .collect::<BTreeSet<_>>();
            let mut residual = Decimal::ZERO;
            for (id, last) in &self.latest {
                if ids.contains(id) {
                    continue;
                }
                let quantity = self.portfolio().net_position(id);
                ensure!(quantity >= Decimal::ZERO, "CATALOG_ENSEMBLE_SHORT_POSITION");
                residual = checked(
                    residual.checked_add(checked(quantity.checked_mul(last.close.as_decimal()))?),
                )?;
            }
            let allowance = checked(
                inbox
                    .borrow()
                    .gross_exposure
                    .checked_sub(checked(residual.checked_div(equity))?),
            )?
            .max(Decimal::ZERO);
            let requested = point
                .targets
                .iter()
                .try_fold(Decimal::ZERO, |sum, target| {
                    checked(sum.checked_add(native_decimal(&target.weight)?))
                })?;
            if requested > allowance {
                let scale = checked(allowance.checked_div(requested))?;
                for target in &mut point.targets {
                    target.weight = checked(native_decimal(&target.weight)?.checked_mul(scale))?
                        .round_dp_with_strategy(18, rust_decimal::RoundingStrategy::ToZero)
                        .to_string()
                        .parse()
                        .map_err(anyhow::Error::msg)?;
                }
            }
            let allocated = point
                .targets
                .iter()
                .try_fold(Decimal::ZERO, |sum, target| {
                    checked(sum.checked_add(native_decimal(&target.weight)?))
                })?;
            point.cash_weight = checked(Decimal::ONE.checked_sub(allocated))?
                .to_string()
                .parse()
                .map_err(anyhow::Error::msg)?;
            validate_point(&self.settings, &point, &self.instruments)?;
        }
        if let Some(planned) = self.study_inputs.get(next) {
            let mut input = planned.input.clone();
            input.forecasts.decision_asof_ns = DbCounter::new(now).map_err(anyhow::Error::msg)?;
            if let Some(age) = planned.liquidity_maximum_age {
                domain::portfolio::bar_liquidity_age(
                    &planned.bar_notionals,
                    &input.base_currency,
                    age,
                    input.forecasts.decision_asof_ns,
                )?;
            }
            input.capital_assumption = equity.to_string().parse().map_err(anyhow::Error::msg)?;
            let mut cash = Decimal::ONE;
            for (asset, instrument) in input.assets.iter_mut().zip(&self.instruments) {
                let price = self.execution_price(&instrument.id(), spot_snapshot.as_ref())?;
                let notional = checked(
                    checked(
                        crate::spot_cash_runtime::inventory_quantity(
                            spot_snapshot.as_ref(),
                            instrument,
                            || self.portfolio().net_position(&instrument.id()),
                        )?
                        .checked_mul(price),
                    )?
                    .checked_mul(instrument.multiplier().as_decimal()),
                )?;
                let weight = checked(notional.checked_div(equity))?.round_dp(18);
                cash = checked(cash.checked_sub(weight))?;
                asset.current_weight = weight.to_string().parse().map_err(anyhow::Error::msg)?;
            }
            input.current_cash_weight = cash.to_string().parse().map_err(anyhow::Error::msg)?;
            let allocation = crate::allocate(&input)?;
            domain::portfolio::allocation_result(&input, &allocation)?;
            let targets = allocation.targets.clone();
            let cash = allocation.cash_weight.clone();
            self.status
                .borrow_mut()
                .frames
                .push(NativePortfolioStudyFrameV1 {
                    cutoff_ns: planned.cutoff_ns,
                    input,
                    allocation,
                    slippage_references: planned.slippage.clone(),
                    bar_notionals: planned.bar_notionals.clone(),
                });
            let (Some(targets), Some(cash)) = (targets, cash) else {
                self.status.borrow_mut().study_infeasible = true;
                return Ok(());
            };
            point.targets = targets;
            point.cash_weight = cash;
            point.asof_ns = DbCounter::new(now).map_err(anyhow::Error::msg)?;
            validate_point(&self.settings, &point, &self.instruments)?;
        }
        if let Some((constraints, currency, tolerance)) = &self.strategy_constraints {
            let current = self
                .instruments
                .iter()
                .map(|instrument| {
                    let price = self.execution_price(&instrument.id(), spot_snapshot.as_ref())?;
                    let notional = checked(
                        checked(
                            crate::spot_cash_runtime::inventory_quantity(
                                spot_snapshot.as_ref(),
                                instrument,
                                || self.portfolio().net_position(&instrument.id()),
                            )?
                            .checked_mul(price),
                        )?
                        .checked_mul(instrument.multiplier().as_decimal()),
                    )?;
                    checked(notional.checked_div(equity))?
                        .round_dp(18)
                        .to_string()
                        .parse()
                        .map_err(anyhow::Error::msg)
                })
                .collect::<Result<Vec<DecimalValue>>>()?;
            domain::execution::strategy::target_bounds(
                constraints,
                currency,
                tolerance,
                &point,
                &current,
            )?;
        }
        let mut reductions = Vec::<OrderAny>::new();
        let mut increases = Vec::<OrderAny>::new();
        for (instrument, target) in self.instruments.iter().zip(&point.targets) {
            let id = instrument.id();
            if point
                .rebalance_instrument_ids
                .as_ref()
                .is_some_and(|scope| !scope.contains(&target.instrument_id))
            {
                // Residual holdings remain in the native account at its last
                // real marks. Never submit against another market's stale BAR.
                continue;
            }
            let current = crate::spot_cash_runtime::inventory_quantity(
                spot_snapshot.as_ref(),
                instrument,
                || self.portfolio().net_position(&id),
            )?;
            // A cash-only valuation and unchanged zero exposure require no
            // asset mark. Any sizing of a held or requested exposure does.
            if spot_snapshot.is_some() && current.is_zero() && !target.weight.is_positive() {
                continue;
            }
            let price = self.execution_price(&id, spot_snapshot.as_ref())?;
            let unit_value = checked(price.checked_mul(instrument.multiplier().as_decimal()))?;
            ensure!(unit_value > Decimal::ZERO, "SIMULATION_INVALID_UNIT_VALUE");
            let desired = checked(
                checked(native_decimal(&target.weight)?.checked_mul(equity))?
                    .checked_div(unit_value),
            )?;
            let delta = checked(desired.checked_sub(current))?;
            if delta.is_zero() {
                continue;
            }
            let increment = instrument.size_increment().as_decimal();
            ensure!(
                increment > Decimal::ZERO,
                "SIMULATION_INVALID_SIZE_INCREMENT"
            );
            let lots = checked(delta.abs().checked_div(increment))?.trunc();
            let units = checked(lots.checked_mul(increment))?;
            let residue = checked(delta.abs().checked_sub(units))?;
            let residual_weight =
                checked(checked(residue.checked_mul(unit_value))?.checked_div(equity))?;
            ensure!(
                residual_weight <= self.tolerance,
                "SIMULATION_LOT_ROUNDING_EXCEEDS_TOLERANCE"
            );
            if units.is_zero() {
                continue;
            }
            let side = if delta < Decimal::ZERO {
                OrderSide::Sell
            } else {
                OrderSide::Buy
            };
            if let Some(snapshot) = &spot_snapshot {
                if side == OrderSide::Sell {
                    let base = instrument
                        .base_currency()
                        .ok_or_else(|| anyhow::anyhow!("SPOT_BASE_CURRENCY_MISSING"))?;
                    ensure!(
                        units <= crate::spot_cash_runtime::native_balance(snapshot, base, true)?,
                        "SPOT_INSUFFICIENT_NATIVE_BASE_CASH"
                    );
                }
            }
            let reducing = if (current > Decimal::ZERO && delta < Decimal::ZERO)
                || (current < Decimal::ZERO && delta > Decimal::ZERO)
            {
                units.min(current.abs())
            } else {
                Decimal::ZERO
            };
            for (amount, reduce_only) in [
                (reducing, true),
                (checked(units.checked_sub(reducing))?, false),
            ] {
                if amount.is_zero() {
                    continue;
                }
                crate::prediction::order_notional(
                    instrument,
                    now,
                    checked(amount.checked_mul(unit_value))?,
                )?;
                if let InstrumentAny::BinaryOption(binary) = instrument {
                    ensure!(
                        now.checked_add(self.latency_ns)
                            .is_some_and(|at| at < binary.expiration_ns.as_u64()),
                        "POLYMARKET_TARGET_EXPIRES_BEFORE_INSERT"
                    );
                }
                let quantity = instrument.try_make_qty_from_decimal(amount, Some(true))?;
                instrument.try_normalize_qty(quantity)?;
                let time_in_force = recorded_liquidity(&self.settings).then_some(TimeInForce::Ioc);
                let order = self.order().market(
                    id,
                    side,
                    quantity,
                    time_in_force,
                    // Native reduce-only is a position restriction. A CASH
                    // base-currency balance can predate this strategy without
                    // any position; its real free inventory was checked above.
                    // Keep sell-before-buy sequencing without inventing a
                    // position or an opening fill for observed spot inventory.
                    Some(reduce_only && spot_snapshot.is_none()),
                    None,
                    None,
                    None,
                    None,
                    None,
                );
                if reduce_only {
                    reductions.push(order);
                } else {
                    increases.push(order);
                }
            }
        }
        // Sending a reducing order does NOT release native margin. Wait for all
        // native reducing fills before submitting increases, including short covers.
        // Both phases retain quantities computed from this one equity snapshot.
        self.active_expiry_ns = point.valid_until_ns.get();
        self.deferred_orders = increases;
        self.reduction_ids = reductions.iter().map(Order::client_order_id).collect();
        self.status.borrow_mut().submitted_after_ns = now;
        if reductions.is_empty() {
            return self.submit_deferred(now);
        }
        for order in reductions {
            self.outstanding_orders.insert(order.client_order_id());
            self.submit_target_order(order)?;
            ensure!(
                self.status.borrow().failure.is_none(),
                "NATIVE_ORDER_NOT_ACCEPTED"
            );
        }
        Ok(())
    }
}

impl DataActor for TargetReplay {
    fn on_start(&mut self) -> Result<()> {
        if self.paper.is_some() {
            if let Err(error) = self.start_paper() {
                self.status.borrow_mut().record_callback_error(error);
                return Err(anyhow::anyhow!("PAPER_TARGET_START_FAILED"));
            }
        }
        for kind in self.bar_types.clone() {
            self.subscribe_bars(kind, None, None);
        }
        if self.catalog.is_some() {
            self.subscribe_data(
                DataType::new(crate::catalog_ensemble::EVENT_TYPE, None, None),
                None,
                None,
            );
        }
        Ok(())
    }
    fn on_data(&mut self, _data: &CustomData) -> Result<()> {
        let result = (|| -> Result<()> {
            let Some(inbox) = self.catalog.clone() else {
                return Ok(());
            };
            let pending = inbox.borrow_mut().pending.take();
            if let Some(frame) = pending {
                ensure!(
                    self.orders_settled(),
                    "CATALOG_ENSEMBLE_PREVIOUS_ORDERS_PENDING"
                );
                let mut status = self.status.borrow_mut();
                status.check_callback_error()?;
                inbox.borrow_mut().consumed += status.consumed;
                status.consumed = 0;
                drop(status);
                self.instruments = frame.instruments;
                inbox.borrow_mut().bar_end_ns = frame.bar_end_ns;
                self.bar_types = frame
                    .point
                    .targets
                    .iter()
                    .map(|target| {
                        BarType::from_str(&format!(
                            "{}-1-MINUTE-LAST-EXTERNAL",
                            target.instrument_id
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                for kind in self.bar_types.clone() {
                    self.subscribe_bars(kind, None, None);
                    let observed = self.cache().bar(&kind).ok_or_else(|| {
                        anyhow::anyhow!("CATALOG_ENSEMBLE_NATIVE_BAR_NOT_DELIVERED")
                    })?;
                    ensure!(
                        observed.ts_event.as_u64() == frame.bar_end_ns
                            && observed.ts_init.as_u64() <= frame.point.asof_ns.get(),
                        "CATALOG_ENSEMBLE_NATIVE_BAR_NOT_CURRENT"
                    );
                    self.latest.insert(kind.instrument_id(), observed);
                }
                self.points = vec![frame.point];
                if self.instruments.is_empty() {
                    self.status.borrow_mut().consumed = 1;
                    return Ok(());
                }
            }
            if let Some(bar) = self.latest.values().max_by_key(|bar| bar.ts_init).copied() {
                let now = self.clock().timestamp_ns().as_u64();
                if self
                    .points
                    .first()
                    .is_some_and(|point| now >= point.asof_ns.get())
                {
                    self.apply_bar(&bar, now)?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.status.borrow_mut().record_callback_error(error);
            return Err(anyhow::anyhow!("NATIVE_TARGET_REPLAY_FAILED"));
        }
        Ok(())
    }
    fn on_bar(&mut self, bar: &Bar) -> Result<()> {
        if self.target_fenced() {
            return Ok(());
        }
        let result = self.accept_new_paper_bar(bar).and_then(|accepted| {
            if accepted {
                self.apply_bar(bar, bar.ts_init.as_u64())
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            // Native actor callbacks may log rather than propagate. Keep an explicit
            // in-process failure observation that the outer adapter MUST inspect.
            self.status.borrow_mut().record_callback_error(error);
            return Err(anyhow::anyhow!("NATIVE_TARGET_REPLAY_FAILED"));
        }
        self.finish_recorded_tail();
        Ok(())
    }
}

/// A sparse target stream uses the same sizing, fills, reductions and native
/// account as ordinary replay. Only currently observed instruments are replaced.
pub(crate) fn catalog_target_strategy(
    settings: NativeSimulationSettingsV1,
    gross_exposure: Decimal,
) -> Result<(
    TargetReplay,
    Rc<RefCell<ReplayStatus>>,
    Rc<RefCell<CatalogReplayInbox>>,
)> {
    let (_, latency) = domain::portfolio::simulation_models(&settings)?;
    let latency_ns = latency
        .base_latency_ns
        .get()
        .checked_add(latency.insert_latency_ns.get())
        .ok_or_else(|| anyhow::anyhow!("CATALOG_ENSEMBLE_LATENCY"))?;
    ensure!(latency_ns > 0, "CATALOG_ENSEMBLE_POSITIVE_LATENCY");
    let status = Rc::new(RefCell::new(ReplayStatus::default()));
    let inbox = Rc::new(RefCell::new(CatalogReplayInbox {
        pending: None,
        consumed: 0,
        gross_exposure,
        bar_end_ns: 0,
    }));
    let strategy = TargetReplay {
        core: StrategyCore::new_checked(
            StrategyConfig::builder()
                .strategy_id(StrategyId::from("CATALOG-001"))
                .order_id_tag("001".into())
                .oms_type(OmsType::Netting)
                .log_events(false)
                .log_commands(false)
                .build()?,
        )?,
        instruments: Vec::new(),
        bar_types: Vec::new(),
        latest: BTreeMap::new(),
        points: Vec::new(),
        study_inputs: Vec::new(),
        strategy_constraints: None,
        currency: Currency::from_str(&settings.base_currency)?,
        venue: Venue::from("POLYMARKET"),
        tolerance: native_decimal(&settings.exposure_tolerance)?,
        settings,
        spot_cash: None,
        latency_ns,
        wake_order_arrivals: true,
        reduction_ids: BTreeSet::new(),
        deferred_orders: Vec::new(),
        active_expiry_ns: 0,
        awaiting_settlement: false,
        outstanding_orders: BTreeSet::new(),
        settlement_events: BTreeMap::new(),
        status: status.clone(),
        recorded_bar_ends: None,
        paper: None,
        capital_exit_gate: None,
        capital_exit_source_pending: false,
        catalog: Some(inbox.clone()),
    };
    Ok((strategy, status, inbox))
}

pub(crate) fn catalog_simulation_result(
    engine: &BacktestEngine,
    settings: &NativeSimulationSettingsV1,
    start_ns: u64,
    end_ns: u64,
    consumed: usize,
    fees: &crate::recorded_fees::RecordedFeeModel,
) -> Result<NativeSimulationResultV1> {
    let native = engine.get_result();
    ensure!(
        native.summary.get("orders.open").map(String::as_str) == Some("0")
            && native.summary.get("orders.inflight").map(String::as_str) == Some("0"),
        "NATIVE_ORDERS_NOT_SETTLED"
    );
    let statistic = |group, native_key, currency, value: f64| NativeStatisticV1 {
        group,
        native_key,
        currency,
        value: value.is_finite().then_some(value),
        reason_code: (!value.is_finite()).then(|| "NATIVE_STATISTIC_UNAVAILABLE".into()),
    };
    let mut statistics = Vec::new();
    for (currency, values) in native.stats_pnls {
        for (key, value) in values {
            statistics.push(statistic(
                NativeStatisticGroup::Pnl,
                key,
                Some(currency.clone()),
                value,
            ));
        }
    }
    let (daily, fully_priced) = portfolio_return_analysis(engine, settings)?;
    statistics.extend(calendar_returns::statistics(
        start_ns,
        end_ns,
        fully_priced,
        daily
            .portfolio_returns()
            .iter()
            .map(|(time, value)| (time.as_u64(), *value)),
    ));
    for (group, values) in [
        (
            NativeStatisticGroup::Returns,
            daily.get_performance_stats_returns(),
        ),
        (NativeStatisticGroup::General, native.stats_general),
    ] {
        for (key, value) in values {
            statistics.push(statistic(group, key, None, value));
        }
    }
    statistics.sort_by(|a, b| (&a.native_key, &a.currency).cmp(&(&b.native_key, &b.currency)));
    let returns = daily
        .portfolio_returns()
        .iter()
        .map(|(time, &value)| {
            Ok(NativeReturnV1 {
                timestamp_ns: DbCounter::new(time.as_u64()).map_err(anyhow::Error::msg)?,
                value: value.is_finite().then_some(value),
                reason_code: (!value.is_finite()).then(|| "NATIVE_RETURN_UNAVAILABLE".into()),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let (returns_status, returns_reason) = if returns.is_empty() {
        (
            contracts::evidence::MetricStatus::InsufficientData,
            Some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE".into()),
        )
    } else if returns.iter().any(|row| row.value.is_none()) {
        (
            contracts::evidence::MetricStatus::Failed,
            Some("NATIVE_RETURN_UNAVAILABLE".into()),
        )
    } else {
        (contracts::evidence::MetricStatus::Ok, None)
    };
    Ok(NativeSimulationResultV1 {
        settled_replay_ns: Some(
            DbCounter::new(engine.kernel().clock.borrow().timestamp_ns().as_u64())
                .map_err(anyhow::Error::msg)?,
        ),
        spot_cash_report: None,
        recorded_fee_replay: Some(fees.report()),
        schema_version: SchemaV1,
        native_version: "0.67.0".into(),
        iterations: count(native.iterations)?,
        events: count(native.total_events)?,
        orders: count(native.total_orders)?,
        positions: count(native.total_positions)?,
        consumed_target_points: count(consumed)?,
        summary: native.summary.into_iter().collect(),
        statistics,
        returns_kind: NativeReturnsKind::PortfolioDaily,
        returns_status,
        returns_reason,
        returns,
        canonical_result: engine.get_canonical_result()?.as_value().clone(),
    })
}

/// Construct one target for a native Paper node. Instrument definitions may be
/// absent until the data client connects; on_start always reloads them from cache.
/// Sandbox execution uses its live clock and immediate runtime latency. The
/// historical latency model remains a validated setting, never a Paper delay.
#[cfg(feature = "native-paper")]
pub(crate) fn paper_target_strategy(
    settings: NativeSimulationSettingsV1,
    instruments: Vec<InstrumentAny>,
    bar_types: Vec<BarType>,
    point: NativeTargetPointV1,
    strategy_id: StrategyId,
    client_id: ClientId,
    installed_fee_policy: Option<PaperMakerTakerPolicy>,
) -> Result<(TargetReplay, Rc<RefCell<ReplayStatus>>)> {
    domain::portfolio::simulation_settings(&settings)?;
    ensure!(
        point.rebalance_instrument_ids.is_none(),
        "TARGET_SCOPE_BACKTEST_ONLY"
    );
    ensure!(
        !matches!(
            settings.fee_model,
            contracts::portfolio::NativeModelRefV1::PolymarketV2RecordedFees { .. }
        ),
        "RECORDED_FEES_BACKTEST_ONLY"
    );
    ensure!(
        !point.targets.is_empty()
            && point.targets.len() == bar_types.len()
            && point.asof_ns < point.valid_until_ns,
        "SIMULATION_TARGET_INVALID"
    );
    let mut ids = BTreeSet::new();
    for (target, kind) in point.targets.iter().zip(&bar_types) {
        ensure!(
            kind.is_internally_aggregated()
                && !kind.is_composite()
                && kind.spec().is_time_aggregated()
                && target.instrument_id == kind.instrument_id().to_string()
                && ids.insert(kind.instrument_id()),
            "PAPER_BAR_TYPE_MISMATCH"
        );
    }
    validate_installed_paper_fees(&settings, installed_fee_policy.as_ref())?;
    if !instruments.is_empty() {
        validate_paper_instruments(
            &settings,
            &instruments,
            &bar_types,
            &point,
            installed_fee_policy.as_ref(),
        )?;
    }
    let currency = Currency::from_str(&settings.base_currency)?;
    let venue = bar_types[0].instrument_id().venue;
    let tolerance = native_decimal(&settings.exposure_tolerance)?;
    let status = Rc::new(RefCell::new(ReplayStatus::default()));
    let strategy = TargetReplay {
        core: StrategyCore::new_checked(
            StrategyConfig::builder()
                .strategy_id(strategy_id)
                .order_id_tag(strategy_id.get_tag().to_owned())
                .oms_type(OmsType::Netting)
                .log_events(false)
                .log_commands(false)
                .build()?,
        )?,
        instruments,
        bar_types,
        latest: BTreeMap::new(),
        points: vec![point],
        study_inputs: Vec::new(),
        settings,
        spot_cash: None,
        currency,
        venue,
        tolerance,
        latency_ns: 0,
        wake_order_arrivals: false,
        reduction_ids: BTreeSet::new(),
        deferred_orders: Vec::new(),
        active_expiry_ns: 0,
        awaiting_settlement: false,
        outstanding_orders: BTreeSet::new(),
        settlement_events: BTreeMap::new(),
        recorded_bar_ends: None,
        status: status.clone(),
        strategy_constraints: None,
        capital_exit_gate: None,
        capital_exit_source_pending: false,
        catalog: None,
        paper: Some(PaperClock {
            execution_client_id: client_id,
            started_ns: 0,
            fresh_account: None,
            installed_fee_policy,
        }),
    };
    Ok((strategy, status))
}

/// Reuse the same target policy with the original frozen latency for an official
/// event-time streaming Paper simulation. The live Sandbox constructor is unchanged.
#[cfg(feature = "native-paper")]
pub(crate) fn streaming_paper_target_strategy(
    settings: NativeSimulationSettingsV1,
    instruments: Vec<InstrumentAny>,
    bar_types: Vec<BarType>,
    point: NativeTargetPointV1,
    strategy_id: StrategyId,
    client_id: ClientId,
) -> Result<(TargetReplay, Rc<RefCell<ReplayStatus>>)> {
    let (_, latency) = domain::portfolio::simulation_models(&settings)?;
    let insertion_delay = latency
        .base_latency_ns
        .get()
        .checked_add(latency.insert_latency_ns.get())
        .ok_or_else(|| anyhow::anyhow!("PAPER_STREAM_LATENCY_RANGE"))?;
    let (mut strategy, status) = paper_target_strategy(
        settings,
        instruments,
        bar_types,
        point,
        strategy_id,
        client_id,
        None,
    )?;
    strategy.latency_ns = insertion_delay;
    Ok((strategy, status))
}

fn validate_paper_instruments(
    settings: &NativeSimulationSettingsV1,
    instruments: &[InstrumentAny],
    bar_types: &[BarType],
    point: &NativeTargetPointV1,
    installed_fee_policy: Option<&PaperMakerTakerPolicy>,
) -> Result<()> {
    domain::portfolio::simulation_settings(settings)?;
    ensure!(
        point.rebalance_instrument_ids.is_none(),
        "TARGET_SCOPE_BACKTEST_ONLY"
    );
    validate_point(settings, point, instruments)?;
    // Reuse the historical metadata predicates without inventing market data.
    // Empty bars are validation input only and never enter the Paper data engine.
    let metadata = NativeMarketData {
        series: instruments
            .iter()
            .zip(bar_types)
            .map(|(instrument, &bar_type)| NativeBarSeries {
                instrument: instrument.clone(),
                instrument_updates: Vec::new(),
                bar_type,
                bars: Vec::new(),
            })
            .collect(),
        rows: 0,
        execution_fees: None,
        fee_selection: None,
    };
    validate_installed_paper_fees(settings, installed_fee_policy)?;
    if matches!(
        settings.fee_model,
        contracts::portfolio::NativeModelRefV1::NautilusMakerTaker { .. }
    ) {
        ensure!(
            instruments
                .iter()
                .all(|instrument| !matches!(instrument, InstrumentAny::BinaryOption(_))),
            "POLYMARKET_NATIVE_FEE_MODEL_REQUIRED"
        );
    } else {
        // Polymarket keeps its original native schedule/price/lifetime checks.
        crate::prediction::validate_market(&metadata, settings)?;
    }
    execution_market_terms(&metadata, settings)?;
    Ok(())
}

fn validate_installed_paper_fees(
    settings: &NativeSimulationSettingsV1,
    installed_fee_policy: Option<&PaperMakerTakerPolicy>,
) -> Result<()> {
    if matches!(
        settings.fee_model,
        contracts::portfolio::NativeModelRefV1::NautilusMakerTaker { .. }
    ) {
        let installed = installed_fee_policy
            .ok_or_else(|| anyhow::anyhow!("PAPER_MAKER_TAKER_POLICY_REQUIRED"))?;
        for rate in &settings.fee_rates {
            let id = InstrumentId::from_str(&rate.instrument_id)?;
            let actual = installed.schedule.rates_for(id);
            ensure!(
                id.venue == installed.venue
                    && native_decimal(&rate.maker)? == actual.maker
                    && native_decimal(&rate.taker)? == actual.taker,
                "PAPER_INSTALLED_FEE_POLICY_MISMATCH"
            );
        }
    } else {
        ensure!(
            installed_fee_policy.is_none(),
            "PAPER_FEE_POLICY_MODEL_MISMATCH"
        );
    }
    Ok(())
}

pub(crate) fn execution_market(
    data: &NativeMarketData,
    settings: &NativeSimulationSettingsV1,
) -> Result<Currency> {
    domain::portfolio::simulation_settings(settings)?;
    // Historical replay always authenticates its independently frozen catalog
    // fee source before sharing the instrument/account predicates with Paper.
    crate::prediction::validate_market(data, settings)?;
    execution_market_terms(data, settings)
}

/// Source-independent native terms; each caller validates its own fee owner first.
fn execution_market_terms(
    data: &NativeMarketData,
    settings: &NativeSimulationSettingsV1,
) -> Result<Currency> {
    let currency = Currency::from_str(&settings.base_currency)?;
    ensure!(
        settings.fee_rates.len() == data.series.len(),
        "SIMULATION_FEE_COUNT_MISMATCH"
    );
    let mut fees = BTreeMap::new();
    for rate in &settings.fee_rates {
        ensure!(
            fees.insert(rate.instrument_id.as_str(), rate).is_none(),
            "SIMULATION_DUPLICATE_FEE"
        );
    }
    let venue = data.series[0].instrument.venue();
    for series in &data.series {
        let instrument = &series.instrument;
        ensure!(
            matches!(
                instrument,
                InstrumentAny::CurrencyPair(_)
                    | InstrumentAny::Equity(_)
                    | InstrumentAny::BinaryOption(_)
            ) && instrument.venue() == venue
                && instrument.quote_currency() == currency
                && instrument.settlement_currency() == currency
                && !instrument.is_inverse()
                && (!instrument.has_expiration()
                    || matches!(instrument, InstrumentAny::BinaryOption(_))),
            "SIMULATION_MARKET_UNSUPPORTED"
        );
        domain::catalogs::execution_account(
            if matches!(instrument, InstrumentAny::CurrencyPair(_)) {
                "CurrencyPair"
            } else if matches!(instrument, InstrumentAny::BinaryOption(_)) {
                "BinaryOption"
            } else {
                "Equity"
            },
            settings.account_kind,
        )?;
        fees.remove(instrument.id().to_string().as_str())
            .ok_or_else(|| anyhow::anyhow!("SIMULATION_FEE_MISSING"))?;
    }
    Ok(currency)
}

fn validate_settings(
    data: &NativeMarketData,
    request: &NativeSimulationRequestV1,
) -> Result<Currency> {
    // The official BacktestEngine cannot update its matching engines in place.
    // Keep the history for data/planning; replaying a stale tick or replacing
    // the matching engine would invalidate native order and position state.
    ensure!(
        data.series
            .iter()
            .all(|series| series.instrument_updates.is_empty()),
        "SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED"
    );
    let currency = execution_market(data, &request.settings)?;
    validate_settings_with_currency(data, request, currency, false)
}

fn validate_settings_with_currency(
    data: &NativeMarketData,
    request: &NativeSimulationRequestV1,
    currency: Currency,
    allow_observed_hold: bool,
) -> Result<Currency> {
    ensure!(
        data.series
            .iter()
            .all(|series| series.instrument_updates.is_empty()),
        "SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED"
    );
    let settings = &request.settings;
    let instruments = data
        .series
        .iter()
        .map(|s| s.instrument.clone())
        .collect::<Vec<_>>();
    ensure!(
        !request.target_points.is_empty() || allow_observed_hold,
        "SIMULATION_TARGET_COUNT_LIMIT"
    );
    for (index, point) in request.target_points.iter().enumerate() {
        ensure!(
            point.asof_ns < point.valid_until_ns
                && point.asof_ns >= request.selection.event_start_ns
                // Targets are decisions on the receive clock. BAR event bounds
                // select rows; delayed availability can be later than event_end.
                && point.asof_ns <= request.selection.decision_cutoff_ns
                && (index == 0 || request.target_points[index - 1].asof_ns < point.asof_ns)
                && point.targets.len() == data.series.len(),
            "SIMULATION_TARGET_INVALID"
        );
        validate_point(settings, point, &instruments)?;
    }
    Ok(currency)
}

fn validate_point(
    settings: &NativeSimulationSettingsV1,
    point: &NativeTargetPointV1,
    instruments: &[InstrumentAny],
) -> Result<()> {
    ensure!(
        point.targets.len() == instruments.len(),
        "SIMULATION_TARGET_IDENTITY"
    );
    domain::execution::strategy::target_scope(point)?;
    crate::prediction::target_window(instruments, point.asof_ns.get(), point.valid_until_ns.get())?;
    let mut total = point.cash_weight.as_decimal().clone();
    let mut gross = BigDecimal::from(0);
    for (target, instrument) in point.targets.iter().zip(instruments) {
        ensure!(
            target.instrument_id == instrument.id().to_string()
                && target.currency == settings.base_currency,
            "SIMULATION_TARGET_IDENTITY"
        );
        if settings.account_kind == NativeAccountKind::Cash
            || matches!(
                settings.fee_model,
                contracts::portfolio::NativeModelRefV1::PolymarketV2RecordedFees { .. }
            )
        {
            ensure!(
                target.weight.is_nonnegative() && point.cash_weight.is_nonnegative(),
                "CASH_SHORTING_UNSUPPORTED"
            );
        }
        total += target.weight.as_decimal();
        gross += target.weight.as_decimal().abs();
    }
    let exact_scope_sum = point.rebalance_instrument_ids.is_none() || total == BigDecimal::from(1);
    ensure!(
        exact_scope_sum
            && (total - BigDecimal::from(1)).abs() <= *settings.exposure_tolerance.as_decimal()
            && gross <= settings.leverage.as_decimal() + settings.exposure_tolerance.as_decimal(),
        "SIMULATION_CAPITAL_OR_LEVERAGE"
    );
    Ok(())
}

// The engine's preferred returns may fall back to per-position returns. Build
// the native snapshot-only analyzer so that fallback cannot qualify a portfolio.
fn portfolio_return_analysis(
    engine: &BacktestEngine,
    settings: &NativeSimulationSettingsV1,
) -> Result<(PortfolioAnalyzer, bool)> {
    let accounts = engine.kernel().cache.borrow().accounts_all_owned();
    ensure!(accounts.len() == 1, "SIMULATION_ACCOUNT_COUNT_MISMATCH");
    let account_ids = accounts
        .iter()
        .map(|account| account.id())
        .collect::<Vec<_>>();
    let snapshots = engine
        .kernel()
        .portfolio
        .borrow()
        .snapshots(&account_ids[0]);
    let fully_priced = snapshots
        .iter()
        .all(|snapshot| snapshot.unpriced_instruments.is_empty());
    let mut analyzer = PortfolioAnalyzer::default();
    analyzer.register_statistic(Arc::new(MaxDrawdown::new()));
    let period = domain::prediction::portfolio_annualization_days(&settings.fee_model);
    if domain::prediction::uses_native_fee(&settings.fee_model) {
        let replacements: [(Statistic, Statistic); 3] = [
            (
                Arc::new(ReturnsVolatility::new(None)),
                Arc::new(ReturnsVolatility::new(Some(period))),
            ),
            (
                Arc::new(SharpeRatio::new(None)),
                Arc::new(SharpeRatio::new(Some(period))),
            ),
            (
                Arc::new(SortinoRatio::new(None)),
                Arc::new(SortinoRatio::new(Some(period))),
            ),
        ];
        for (original, replacement) in replacements {
            analyzer.deregister_statistic(&original);
            analyzer.register_statistic(replacement);
        }
    }
    analyzer.set_portfolio_returns_from_snapshots(&account_ids, &snapshots);
    Ok((analyzer, fully_priced))
}

/// Explicit managed/CLI simulation dispatch. The ID names app.dataset_revisions,
/// and the source proof stays in the same registered catalog snapshot.
pub fn simulate_explicit(
    root: &Path,
    request: &NativeSimulationRequestV1,
    dataset_revision_id: Option<contracts::Id>,
) -> Result<NativeSimulationResultV1> {
    if request.settings.multi_currency_spot_cash.is_none() {
        return simulate(root, request);
    }
    let dataset_revision_id =
        dataset_revision_id.ok_or_else(|| anyhow::anyhow!("SPOT_DATASET_REVISION_ID_REQUIRED"))?;
    domain::spot_cash::execution_settings(&request.settings)?;
    #[cfg(feature = "hyperliquid-offline")]
    {
        let source = crate::spot_cash_source::load_closed_rows(root, &request.selection)?;
        let mut result =
            simulate_spot_cash_candidate(root, request, dataset_revision_id, source.rows)?;
        result
            .spot_cash_report
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("SPOT_NATIVE_REPORT_MISSING"))?
            .source_evidence = Some(source.evidence);
        Ok(result)
    }
    #[cfg(not(feature = "hyperliquid-offline"))]
    {
        let _ = dataset_revision_id;
        Err(anyhow::anyhow!(
            "SPOT_SOURCE_CAPABILITY_NOT_BUILT: hyperliquid-offline"
        ))
    }
}

/// The only filesystem path is a trusted registered read-only runtime mount.
pub fn simulate(
    root: &Path,
    request: &NativeSimulationRequestV1,
) -> Result<NativeSimulationResultV1> {
    run(root, request, Vec::new())?
        .0
        .ok_or_else(|| anyhow::anyhow!("SIMULATION_RESULT_MISSING"))
}

pub(crate) fn run(
    root: &Path,
    request: &NativeSimulationRequestV1,
    study_inputs: Vec<StudyInput>,
) -> Result<(
    Option<NativeSimulationResultV1>,
    Vec<NativePortfolioStudyFrameV1>,
)> {
    run_with_strategy(root, request, study_inputs, None)
}

pub fn simulate_strategy(
    root: &Path,
    request: &NativeSimulationRequestV1,
    mandate: &contracts::strategy_portfolio::StrategyMandateContentV1,
) -> Result<NativeSimulationResultV1> {
    run_with_strategy(
        root,
        request,
        Vec::new(),
        Some((
            mandate.constraints.clone(),
            mandate.base_currency.clone(),
            mandate.exposure_tolerance.clone(),
        )),
    )?
    .0
    .ok_or_else(|| anyhow::anyhow!("SIMULATION_RESULT_MISSING"))
}

/// Explicit research-candidate entry. Source rows must come from the trusted
/// frozen-data adapter; existing CLI/managed entrypoints do not select it yet.
pub fn simulate_spot_cash_candidate(
    root: &Path,
    request: &NativeSimulationRequestV1,
    dataset_revision_id: contracts::Id,
    closed_rows: Vec<crate::spot_cash_capture::ClosedBarSourceRow>,
) -> Result<NativeSimulationResultV1> {
    domain::spot_cash::execution_settings(&request.settings)?;
    ensure!(
        request.settlements.is_empty(),
        "SPOT_SETTLEMENT_INPUT_UNSUPPORTED"
    );
    run_with_strategy_and_spot(
        root,
        request,
        Vec::new(),
        None,
        Some(crate::spot_cash_runtime::SpotReplayInputs {
            dataset_revision_id,
            closed_rows,
            observed_start: None,
        }),
    )?
    .0
    .ok_or_else(|| anyhow::anyhow!("SIMULATION_RESULT_MISSING"))
}

/// Explicit observed-inventory runner. Empty points mean no submitted orders,
/// never repeated target weights or liquidation. Only this wrapper permits it.
pub(crate) fn run_observed_spot(
    root: &Path,
    request: &NativeSimulationRequestV1,
    dataset_revision_id: contracts::Id,
    closed_rows: Vec<crate::spot_cash_capture::ClosedBarSourceRow>,
    initial_state: &contracts::observed_spot::NativeObservedSpotStartV1,
    study_inputs: Vec<StudyInput>,
) -> Result<(
    Option<NativeSimulationResultV1>,
    Vec<NativePortfolioStudyFrameV1>,
)> {
    domain::observed_spot::starting_balances(
        initial_state,
        &request.settings,
        request.selection.event_start_ns,
    )?;
    ensure!(
        request.settlements.is_empty(),
        "SPOT_SETTLEMENT_INPUT_UNSUPPORTED"
    );
    run_with_strategy_and_spot(
        root,
        request,
        study_inputs,
        None,
        Some(crate::spot_cash_runtime::SpotReplayInputs {
            dataset_revision_id,
            closed_rows,
            observed_start: Some(initial_state.clone()),
        }),
    )
}

fn run_with_strategy(
    root: &Path,
    request: &NativeSimulationRequestV1,
    study_inputs: Vec<StudyInput>,
    strategy_constraints: Option<(
        contracts::portfolio::PortfolioConstraintsV1,
        String,
        DecimalValue,
    )>,
) -> Result<(
    Option<NativeSimulationResultV1>,
    Vec<NativePortfolioStudyFrameV1>,
)> {
    run_with_strategy_and_spot(root, request, study_inputs, strategy_constraints, None)
}

fn run_with_strategy_and_spot(
    root: &Path,
    request: &NativeSimulationRequestV1,
    study_inputs: Vec<StudyInput>,
    strategy_constraints: Option<(
        contracts::portfolio::PortfolioConstraintsV1,
        String,
        DecimalValue,
    )>,
    mut spot_inputs: Option<crate::spot_cash_runtime::SpotReplayInputs>,
) -> Result<(
    Option<NativeSimulationResultV1>,
    Vec<NativePortfolioStudyFrameV1>,
)> {
    ensure!(
        study_inputs.is_empty() || study_inputs.len() == request.target_points.len(),
        "STUDY_FRAME_COUNT"
    );
    ensure!(
        (study_inputs.is_empty() && strategy_constraints.is_none() && spot_inputs.is_none())
            || request
                .target_points
                .iter()
                .all(|point| point.rebalance_instrument_ids.is_none()),
        "TARGET_SCOPE_BACKTEST_ONLY"
    );
    let (fill, latency) = domain::portfolio::simulation_models(&request.settings)?;
    let observed_balances = spot_inputs
        .as_ref()
        .and_then(|input| input.observed_start.as_ref())
        .map(|start| {
            domain::observed_spot::starting_balances(
                start,
                &request.settings,
                request.selection.event_start_ns,
            )
        })
        .transpose()?;
    let is_spot = spot_inputs.is_some();
    let scoped_end = domain::execution::strategy::scoped_replay_end(request);
    let scoped_replay = scoped_end.is_some();
    let spot_end = is_spot
        .then(|| crate::spot_cash_runtime::execution_horizon(request))
        .transpose()?;
    let wake_order_arrivals = recorded_liquidity(&request.settings) || scoped_replay || is_spot;
    let market = if !request.target_points.is_empty()
        && request
            .target_points
            .iter()
            .all(|point| point.rebalance_instrument_ids.is_some())
    {
        crate::catalog::load_scoped_catalog(root, &request.selection)?
    } else {
        load_catalog(root, &request.selection)?
    };
    let currency = if is_spot {
        domain::spot_cash::execution_settings(&request.settings)?;
        let currency = Currency::from_str(&request.settings.base_currency)?;
        validate_settings_with_currency(&market, request, currency, observed_balances.is_some())?
    } else {
        validate_settings(&market, request)?
    };
    let closes =
        crate::prediction::close_events(root, &market, &request.selection, &request.settlements)?;
    let settlement_events = closes
        .iter()
        .map(|c| (c.instrument_id, *c))
        .collect::<BTreeMap<_, _>>();
    let settled_ids = settlement_events.keys().copied().collect::<Vec<_>>();
    let venue = market.series[0].instrument.venue();
    if let Some(balances) = &observed_balances {
        let allowed: BTreeSet<_> = market
            .series
            .iter()
            .filter_map(|s| s.instrument.base_currency())
            .map(|currency| currency.to_string())
            .chain(std::iter::once(request.settings.base_currency.clone()))
            .collect();
        ensure!(
            balances
                .iter()
                .all(|balance| allowed.contains(&balance.currency)),
            "OBSERVED_SPOT_UNVALUED_CURRENCY"
        );
    }
    let status = Rc::new(RefCell::new(ReplayStatus::default()));
    let mut strategy = TargetReplay {
        core: StrategyCore::new_checked(
            StrategyConfig::builder()
                .strategy_id(StrategyId::from("TARGET-001"))
                .order_id_tag("001".into())
                .oms_type(OmsType::Netting)
                .log_events(false)
                .log_commands(false)
                .build()?,
        )?,
        instruments: market.series.iter().map(|s| s.instrument.clone()).collect(),
        bar_types: market.series.iter().map(|s| s.bar_type).collect(),
        latest: BTreeMap::new(),
        points: request.target_points.clone(),
        study_inputs,
        strategy_constraints,
        settings: request.settings.clone(),
        spot_cash: None,
        currency,
        venue,
        tolerance: native_decimal(&request.settings.exposure_tolerance)?,
        latency_ns: latency.base_latency_ns.get() + latency.insert_latency_ns.get(),
        wake_order_arrivals,
        reduction_ids: BTreeSet::new(),
        deferred_orders: Vec::new(),
        active_expiry_ns: 0,
        awaiting_settlement: false,
        outstanding_orders: BTreeSet::new(),
        settlement_events,
        // Every selected series must deliver its own final real BAR before
        // shutdown, even when the markets' observation clocks differ.
        recorded_bar_ends: wake_order_arrivals.then(|| {
            market.series.iter().flat_map(|series| &series.bars).fold(
                BTreeMap::<InstrumentId, u64>::new(),
                |mut ends, bar| {
                    let time = bar.ts_init.as_u64();
                    ends.entry(bar.bar_type.instrument_id())
                        .and_modify(|end| *end = (*end).max(time))
                        .or_insert(time);
                    ends
                },
            )
        }),
        status: status.clone(),
        capital_exit_gate: None,
        capital_exit_source_pending: false,
        paper: None,
        catalog: None,
    };
    let config = BacktestEngineConfig {
        portfolio: Some(
            PortfolioConfig::builder()
                .snapshot_interval_ms(u64::from(request.settings.snapshot_interval_ms))
                .build()?,
        ),
        // The kernel initializes its own logger from this nested config. The
        // engine flag alone does not protect the JSON stdout transport. Native
        // bypass retains shutdown-on-error capture without exposing raw messages.
        logging: LoggerConfig::builder()
            .bypass_logging(true)
            .is_colored(false)
            .build()?,
        shutdown_on_error: true,
        bypass_logging: true,
        // Final output and daily-return analysis remain explicit below. The
        // native end-of-run log does not need another portfolio analysis.
        run_analysis: false,
        ..BacktestEngineConfig::default()
    };
    let mut engine = BacktestEngine::new(config)?;
    let live_progress = crate::live_progress::LiveProgress::attach();
    let result = (|| -> Result<Option<NativeSimulationResultV1>> {
        let settings = &request.settings;
        let fill_fees = crate::prediction::FillFeeObserver::default();
        let recorded_fees = if matches!(
            settings.fee_model,
            contracts::portfolio::NativeModelRefV1::PolymarketV2RecordedFees { .. }
        ) {
            Some(crate::recorded_fees::RecordedFeeModel::load(
                root,
                &market,
                engine.kernel().clock.clone(),
            )?)
        } else {
            None
        };
        let fee = if is_spot {
            crate::spot_cash_runtime::fee_model(request, &market)?
        } else if let Some(model) = &recorded_fees {
            model.handle()
        } else {
            crate::prediction::fee_model(settings, &market, engine.kernel().clock.clone())?
        };
        let default_fill = DefaultFillModel::new(
            fill.prob_fill_on_limit
                .as_decimal()
                .to_f64()
                .ok_or_else(|| anyhow::anyhow!("FILL_PROBABILITY_RANGE"))?,
            fill.prob_slippage
                .as_decimal()
                .to_f64()
                .ok_or_else(|| anyhow::anyhow!("SLIPPAGE_PROBABILITY_RANGE"))?,
            Some(fill.random_seed.get()),
        )?;
        let mut liquidity_errors = None;
        let fill_model = if recorded_liquidity(settings) {
            let model = crate::recorded_fees::RecordedLiquidityModel::new(
                recorded_fees
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("RECORDED_LIQUIDITY_FEES_REQUIRED"))?
                    .clone(),
                default_fill,
            );
            liquidity_errors = Some(model.observer());
            FillModelHandle::new(model)
        } else {
            FillModelHandle::new(default_fill)
        };
        engine.add_venue(
            SimulatedVenueConfig::builder()
                .venue(venue)
                .oms_type(OmsType::Netting)
                .account_type(match settings.account_kind {
                    NativeAccountKind::Cash => AccountType::Cash,
                    NativeAccountKind::Margin => AccountType::Margin,
                })
                .book_type(BookType::L1_MBP)
                .maybe_base_currency((!is_spot).then_some(currency))
                .allow_cash_borrowing(false)
                .starting_balances(if let Some(balances) = &observed_balances {
                    balances
                        .iter()
                        .map(|balance| {
                            Money::from_str(&format!(
                                "{} {}",
                                balance.amount.as_decimal().to_plain_string(),
                                balance.currency
                            ))
                            .map_err(anyhow::Error::msg)
                        })
                        .collect::<Result<Vec<_>>>()?
                } else {
                    vec![Money::from_str(&format!(
                        "{} {}",
                        settings.starting_capital.as_decimal().to_plain_string(),
                        settings.base_currency
                    ))
                    .map_err(anyhow::Error::msg)?]
                })
                .default_leverage(native_decimal(&settings.leverage)?)
                .fee_model(
                    if domain::prediction::uses_native_fee(&settings.fee_model)
                        || recorded_fees.is_some()
                    {
                        fill_fees.wrap(fee)
                    } else {
                        fee
                    },
                )
                .fill_model(fill_model)
                .latency_model(LatencyModelHandle::new(StaticLatencyModel::new(
                    DurationNanos::new(latency.base_latency_ns.get()),
                    DurationNanos::new(latency.insert_latency_ns.get()),
                    DurationNanos::new(latency.update_latency_ns.get()),
                    DurationNanos::new(latency.cancel_latency_ns.get()),
                )))
                .bar_execution(true)
                .liquidity_consumption(true)
                .build()?,
        )?;
        let first_event = market
            .series
            .iter()
            .flat_map(|series| &series.bars)
            .map(|bar| bar.ts_init)
            .chain(closes.iter().map(|close| close.ts_init))
            .min()
            .ok_or_else(|| anyhow::anyhow!("CATALOG_EMPTY_SELECTION"))?;
        ensure!(
            market
                .series
                .iter()
                .all(|series| series.instrument.ts_init() <= first_event),
            "SIMULATION_BASELINE_FROM_FUTURE"
        );
        for series in &market.series {
            engine.add_instrument(&series.instrument)?;
        }
        let mut spot_capture = if let Some(input) = spot_inputs.take() {
            ensure!(closes.is_empty(), "SPOT_EXTERNAL_SETTLEMENT_UNSUPPORTED");
            let capture = crate::spot_cash_capture::Capture::prepare(
                &engine,
                settings,
                input.dataset_revision_id,
                &market,
                input.closed_rows,
                None,
                Some(crate::spot_cash_runtime::execution_horizon(request)?),
            )?;
            if let Some(balances) = &observed_balances {
                capture.expect_observed_balances(balances.clone())?;
            }
            strategy.spot_cash = Some(capture.handle(&engine)?);
            Some(capture)
        } else {
            None
        };
        engine.add_strategy(strategy)?;
        for series in market.series {
            // Native replay merges these streams by receive time and retains
            // insertion priority on ties, matching the original series order.
            // Scoped selections may legitimately have an empty asset series.
            if !series.bars.is_empty() {
                engine.add_data_batch(DataBatch::Bar(series.bars.into()), None, true, true)?;
            }
        }
        for batch in settlement_batches(closes) {
            engine.add_data_batch(batch, None, true, true)?;
        }
        // Event bounds select BARs, not their later permitted receive clocks.
        // Scoped orders may also need arrival callbacks after the data cutoff;
        // their original expiry bounds execution, while settled-final-BAR
        // shutdown still prevents manufacturing an empty account-return tail.
        let replay_end = scoped_end
            .or(spot_end.map(|end| end.get()))
            .or_else(|| {
                recorded_liquidity(&request.settings)
                    .then(|| request.selection.event_end_ns.get() - 1)
            })
            .map(Into::into);
        let native_result = engine.run(None, replay_end, None, false);
        // Original callback errors retain precedence over native fee failure.
        status.borrow_mut().check_callback_error()?;
        if let Some(observer) = liquidity_errors {
            observer
                .check()
                .map_err(|error| error.context("NATIVE_TARGET_REPLAY_FAILED"))?;
        }
        fill_fees
            .check()
            .map_err(|error| error.context("NATIVE_TARGET_REPLAY_FAILED"))?;
        status.borrow_mut().finish_run(native_result)?;
        for instrument_id in &settled_ids {
            ensure!(
                engine
                    .kernel()
                    .cache
                    .borrow()
                    .positions_open(None, Some(instrument_id), None, None, None)
                    .is_empty(),
                "POLYMARKET_NATIVE_SETTLEMENT_INCOMPLETE"
            );
        }
        let observed = status.borrow();
        if observed.study_infeasible {
            return Ok(None);
        }
        ensure!(
            observed.consumed == request.target_points.len(),
            "NATIVE_TARGETS_NOT_CONSUMED: consumed={} expected={}",
            observed.consumed,
            request.target_points.len()
        );
        let native = engine.get_result();
        ensure!(
            native.summary.get("orders.open").map(String::as_str) == Some("0")
                && native.summary.get("orders.inflight").map(String::as_str) == Some("0"),
            "NATIVE_ORDERS_NOT_SETTLED"
        );
        let spot_cash_report = if let Some(capture) = spot_capture.take() {
            let tape = capture.finish(&engine)?;
            Some(crate::spot_cash_runtime::finish_report(
                &engine,
                request,
                tape,
                &observed.spot_fills,
            )?)
        } else {
            None
        };
        let mut statistics = Vec::new();
        let statistic = |group, native_key, currency, value: f64| NativeStatisticV1 {
            group,
            native_key,
            currency,
            value: value.is_finite().then_some(value),
            reason_code: (!value.is_finite()).then(|| "NATIVE_STATISTIC_UNAVAILABLE".into()),
        };
        for (currency, values) in native.stats_pnls {
            for (key, value) in values {
                statistics.push(statistic(
                    NativeStatisticGroup::Pnl,
                    key,
                    Some(currency.clone()),
                    value,
                ));
            }
        }
        let (daily, fully_priced) = if is_spot {
            (PortfolioAnalyzer::default(), false)
        } else {
            portfolio_return_analysis(&engine, &request.settings)?
        };
        if !is_spot {
            statistics.extend(calendar_returns::statistics(
                request.selection.event_start_ns.get(),
                request.selection.event_end_ns.get(),
                fully_priced,
                daily
                    .portfolio_returns()
                    .iter()
                    .map(|(time, value)| (time.as_u64(), *value)),
            ));
        }
        for (group, values) in [
            (
                NativeStatisticGroup::Returns,
                if is_spot {
                    Default::default()
                } else {
                    daily.get_performance_stats_returns()
                },
            ),
            (NativeStatisticGroup::General, native.stats_general),
        ] {
            for (key, value) in values {
                statistics.push(statistic(group, key, None, value));
            }
        }
        statistics.sort_by(|a, b| (&a.native_key, &a.currency).cmp(&(&b.native_key, &b.currency)));
        let returns = daily
            .portfolio_returns()
            .iter()
            .map(|(time, &value)| {
                Ok(NativeReturnV1 {
                    timestamp_ns: DbCounter::new(time.as_u64()).map_err(anyhow::Error::msg)?,
                    value: value.is_finite().then_some(value),
                    reason_code: (!value.is_finite()).then(|| "NATIVE_RETURN_UNAVAILABLE".into()),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let (returns_status, returns_reason) = if returns.is_empty() {
            (
                contracts::evidence::MetricStatus::InsufficientData,
                Some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE".into()),
            )
        } else if returns.iter().any(|point| point.value.is_none()) {
            (
                contracts::evidence::MetricStatus::Failed,
                Some("NATIVE_RETURN_UNAVAILABLE".into()),
            )
        } else {
            (contracts::evidence::MetricStatus::Ok, None)
        };
        Ok(Some(NativeSimulationResultV1 {
            settled_replay_ns: observed
                .settled_replay_ns
                .map(DbCounter::new)
                .transpose()
                .map_err(anyhow::Error::msg)?,
            spot_cash_report,
            recorded_fee_replay: recorded_fees
                .as_ref()
                .map(crate::recorded_fees::RecordedFeeModel::report),
            schema_version: SchemaV1,
            native_version: "0.67.0".into(),
            iterations: count(native.iterations)?,
            events: count(native.total_events)?,
            orders: count(native.total_orders)?,
            positions: count(native.total_positions)?,
            consumed_target_points: count(observed.consumed)?,
            summary: native.summary.into_iter().collect(),
            statistics,
            returns_kind: NativeReturnsKind::PortfolioDaily,
            returns_status,
            returns_reason,
            returns,
            canonical_result: engine.get_canonical_result()?.as_value().clone(),
        }))
    })();
    drop(live_progress);
    engine.dispose();
    let frames = std::mem::take(&mut status.borrow_mut().frames);
    Ok((result?, frames))
}

#[cfg(test)]
mod replay_error_tests {
    use super::*;
    use nautilus_backtest::data_iterator::BacktestDataIterator;
    use nautilus_model::{
        data::{Data, QuoteTick},
        enums::InstrumentCloseType,
        events::OrderFilled,
        types::{Price, Quantity},
    };

    #[test]
    fn typed_replay_keeps_receive_clock_and_original_same_clock_priority() {
        let [first, second] = replay_fixture::IDS.map(InstrumentId::from);
        let bar = |id: InstrumentId, event: u64, received: u64| {
            let kind = BarType::from_str(&format!("{id}-1-MINUTE-LAST-EXTERNAL")).unwrap();
            let price = Price::from("0.5000");
            Bar::new_checked(
                kind,
                price,
                price,
                price,
                price,
                Quantity::from("1.000000"),
                event.into(),
                received.into(),
            )
            .unwrap()
        };
        let close = |id, event: u64| {
            InstrumentClose::new(
                id,
                Price::from("1.0000"),
                InstrumentCloseType::ContractExpired,
                event.into(),
                20_u64.into(),
            )
        };
        let mut iterator = BacktestDataIterator::new();
        // Deliberately out-of-order input and late events ensure receive clocks
        // drive sorting; an event clock or global instrument sort is incorrect.
        iterator.add_data_batch(
            "first",
            DataBatch::Bar(vec![bar(first, 10, 20), bar(first, 10, 10)].into()),
            true,
        );
        iterator.add_data_batch(
            "second",
            DataBatch::Bar(vec![bar(second, 5, 20), bar(second, 15, 15)].into()),
            true,
        );
        for (index, batch) in settlement_batches([close(second, 19), close(first, 18)]).enumerate()
        {
            iterator.add_data_batch(&format!("settlement-{index}"), batch, true);
        }
        let mut actual = Vec::new();
        while let Some(data) = iterator.next() {
            actual.push(match data {
                Data::Bar(bar) => (
                    "bar",
                    bar.bar_type.instrument_id(),
                    bar.ts_event.as_u64(),
                    bar.ts_init.as_u64(),
                ),
                Data::InstrumentStatus(status) => {
                    assert_eq!(status.action, MarketStatusAction::Close);
                    assert_eq!(status.is_trading, Some(false));
                    (
                        "status",
                        status.instrument_id,
                        status.ts_event.as_u64(),
                        status.ts_init.as_u64(),
                    )
                }
                Data::InstrumentClose(close) => (
                    "close",
                    close.instrument_id,
                    close.ts_event.as_u64(),
                    close.ts_init.as_u64(),
                ),
                _ => panic!("unexpected replay data"),
            });
        }
        assert_eq!(
            actual,
            vec![
                ("bar", first, 10, 10),
                ("bar", second, 15, 15),
                ("bar", first, 10, 20),
                ("bar", second, 5, 20),
                ("status", second, 19, 20),
                ("close", second, 19, 20),
                ("status", first, 18, 20),
                ("close", first, 18, 20),
            ]
        );
        assert!(iterator.is_done());
    }

    struct CloseProbe {
        core: StrategyCore,
        instrument_id: InstrumentId,
        submitted: bool,
        fills: Rc<RefCell<Vec<OrderFilled>>>,
    }
    impl fmt::Debug for CloseProbe {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("CloseProbe").finish_non_exhaustive()
        }
    }
    nautilus_strategy!(CloseProbe, {
        fn on_order_filled(&mut self, event: &OrderFilled) {
            self.fills.borrow_mut().push(event.clone());
        }
    });
    impl DataActor for CloseProbe {
        fn on_start(&mut self) -> Result<()> {
            self.subscribe_quotes(self.instrument_id, None, None);
            Ok(())
        }
        fn on_quote(&mut self, _quote: &QuoteTick) -> Result<()> {
            if !self.submitted {
                self.submitted = true;
                let order = self.order().limit(
                    self.instrument_id,
                    OrderSide::Buy,
                    Quantity::from("10.000000"),
                    Price::from("0.5000"),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(ClientOrderId::from("RESTING-BEFORE-CLOSE")),
                );
                self.submit_order(order, None, None, None)?;
            }
            Ok(())
        }
    }

    #[test]
    fn original_early_close_cancels_resting_remainder_before_another_native_fill() {
        let original = replay_fixture::instruments("0", 18 * replay_fixture::STEP).remove(0);
        let id = original.id();
        let mut engine = BacktestEngine::new(BacktestEngineConfig {
            logging: LoggerConfig::builder()
                .bypass_logging(true)
                .build()
                .unwrap(),
            shutdown_on_error: true,
            bypass_logging: true,
            run_analysis: false,
            ..Default::default()
        })
        .unwrap();
        engine
            .add_venue(
                SimulatedVenueConfig::builder()
                    .venue(original.venue())
                    .oms_type(OmsType::Netting)
                    .account_type(AccountType::Cash)
                    .book_type(BookType::L1_MBP)
                    .base_currency(original.quote_currency())
                    .starting_balances(vec![Money::from("1000 pUSD")])
                    .fee_model(nautilus_execution::models::fee::FeeModelHandle::new(
                        nautilus_polymarket::models::PolymarketFeeModel,
                    ))
                    .fill_model(FillModelHandle::new(
                        DefaultFillModel::new(1.0, 0.0, Some(7)).unwrap(),
                    ))
                    .bar_execution(false)
                    .liquidity_consumption(true)
                    .build()
                    .unwrap(),
            )
            .unwrap();
        engine.add_instrument(&original).unwrap();
        let fills = Rc::new(RefCell::new(Vec::new()));
        engine
            .add_strategy(CloseProbe {
                core: StrategyCore::new(StrategyConfig {
                    strategy_id: Some(StrategyId::from("CLOSE-PROBE-001")),
                    ..Default::default()
                }),
                instrument_id: id,
                submitted: false,
                fills: fills.clone(),
            })
            .unwrap();
        let quote = |at: u64, price: &str| {
            QuoteTick::new(
                id,
                Price::from(price),
                Price::from(price),
                Quantity::from("2.000000"),
                Quantity::from("2.000000"),
                at.into(),
                at.into(),
            )
        };
        engine
            .add_data_batch(
                DataBatch::Quote(
                    vec![
                        quote(5 * replay_fixture::STEP, "0.5000"),
                        quote(10 * replay_fixture::STEP, "0.2000"),
                    ]
                    .into(),
                ),
                None,
                true,
                true,
            )
            .unwrap();
        for batch in settlement_batches([InstrumentClose::new(
            id,
            Price::from("1.0000"),
            InstrumentCloseType::ContractExpired,
            (8 * replay_fixture::STEP).into(),
            (9 * replay_fixture::STEP).into(),
        )]) {
            engine.add_data_batch(batch, None, true, true).unwrap();
        }
        engine.run(None, None, None, false).unwrap();
        let cache = engine.kernel().cache.borrow();
        let order = cache
            .order(&ClientOrderId::from("RESTING-BEFORE-CLOSE"))
            .unwrap();
        assert_eq!(order.status(), OrderStatus::Canceled);
        assert_eq!(order.filled_qty(), Quantity::from("2.000000"));
        assert_eq!(order.leaves_qty(), Quantity::from("8.000000"));
        drop(order);
        assert!(cache
            .positions_open(None, Some(&id), None, None, None)
            .is_empty());
        let account = cache.account_for_venue(&id.venue).unwrap();
        assert_eq!(
            account.balance_total(None).unwrap(),
            Money::from("1001 pUSD")
        );
        assert_eq!(
            account.balance_locked(None).unwrap(),
            Money::zero(original.quote_currency())
        );
        drop(account);
        drop(cache);
        let observed = fills.borrow();
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0].last_qty, Quantity::from("2.000000"));
        assert_eq!(observed[1].ts_event.as_u64(), 9 * replay_fixture::STEP);
        assert_eq!(observed[1].last_px, Price::from("1.0000"));
        assert!(crate::prediction::is_native_settlement_order(
            observed[1].client_order_id.as_str()
        ));
        assert_eq!(
            observed[1].commission,
            Some(Money::zero(original.quote_currency()))
        );
        drop(observed);
        let result = engine.get_result();
        for key in ["positions.open", "orders.open", "orders.inflight"] {
            assert_eq!(result.summary[key], "0");
        }
        engine.dispose();
    }

    #[test]
    fn native_order_rejection_retains_closed_reason_under_original_outer_code() {
        let mut status = ReplayStatus {
            failure: Some("NATIVE_ORDER_REJECTED"),
            order_rejection: Some("Market POLYMARKET is CLOSED".into()),
            ..ReplayStatus::default()
        };
        let error = status
            .finish_run(Err(anyhow::anyhow!("native shutdown")))
            .unwrap_err();
        assert_eq!(error.to_string(), "NATIVE_TARGET_REPLAY_FAILED");
        assert_eq!(
            format!("{error:#}"),
            "NATIVE_TARGET_REPLAY_FAILED: NATIVE_ORDER_REJECTED: Market POLYMARKET is CLOSED"
        );
    }

    #[test]
    fn rejection_detail_cannot_replace_an_original_callback_chain() {
        let mut status = ReplayStatus::default();
        status
            .record_callback_error(anyhow::anyhow!("original callback cause").context("apply bar"));
        status.failure = Some("NATIVE_ORDER_REJECTED");
        status.order_rejection = Some("Market POLYMARKET is CLOSED".into());
        let error = status.finish_run(Ok(())).unwrap_err();
        assert_eq!(
            format!("{error:#}"),
            "NATIVE_TARGET_REPLAY_FAILED: apply bar: original callback cause"
        );
    }

    #[test]
    fn original_callback_chain_has_priority_over_an_observed_native_fee_failure() {
        use nautilus_execution::models::fee::{FeeModel, FeeModelHandle};
        use nautilus_model::{enums::OrderType, orders::builder::OrderTestBuilder};
        struct FailedFee;
        impl FeeModel for FailedFee {
            fn get_commission(
                &self,
                _: &OrderAny,
                _: Quantity,
                _: Price,
                _: &InstrumentAny,
            ) -> Result<Money> {
                anyhow::bail!("POLYMARKET_FILL_FEE_EXCEEDS_PLANNING_BOUND")
            }
        }
        let instrument = replay_fixture::instruments("0.05", 18 * replay_fixture::STEP).remove(0);
        let order = OrderTestBuilder::new(OrderType::Market)
            .instrument_id(instrument.id())
            .side(OrderSide::Buy)
            .quantity(Quantity::from("10.000000"))
            .build();
        let fees = crate::prediction::FillFeeObserver::default();
        fees.wrap(FeeModelHandle::new(FailedFee))
            .get_commission(
                &order,
                Quantity::from("0.000480"),
                Price::from("0.4000"),
                &instrument,
            )
            .unwrap_err();
        let mut status = ReplayStatus::default();
        status
            .record_callback_error(anyhow::anyhow!("original callback cause").context("apply bar"));
        let result = (|| -> Result<()> {
            status.check_callback_error()?;
            fees.check()?;
            status.finish_run(Err(anyhow::anyhow!("native shutdown")))
        })();
        assert_eq!(
            format!("{:#}", result.unwrap_err()),
            "NATIVE_TARGET_REPLAY_FAILED: apply bar: original callback cause"
        );
        assert_eq!(
            fees.failure().unwrap().reason,
            "POLYMARKET_FILL_FEE_EXCEEDS_PLANNING_BOUND"
        );
    }

    #[test]
    fn rejection_detail_is_not_added_to_denied_or_native_only_errors() {
        let mut status = ReplayStatus {
            failure: Some("NATIVE_ORDER_DENIED"),
            order_rejection: Some("unrelated reason".into()),
            ..ReplayStatus::default()
        };
        assert_eq!(
            format!("{:#}", status.finish_run(Ok(())).unwrap_err()),
            "NATIVE_TARGET_REPLAY_FAILED: NATIVE_ORDER_DENIED"
        );
        status.failure = None;
        assert_eq!(
            status
                .finish_run(Err(anyhow::anyhow!("native failure")))
                .unwrap_err()
                .to_string(),
            "native failure"
        );
    }

    #[test]
    fn original_callback_chain_survives_native_shutdown_error() {
        let mut status = ReplayStatus::default();
        status.record_callback_error(anyhow::anyhow!("underlying cause").context("apply bar"));
        status.record_callback_error(anyhow::anyhow!("later callback"));
        let error = status
            .finish_run(Err(anyhow::anyhow!("native shutdown")))
            .unwrap_err();
        assert_eq!(error.to_string(), "NATIVE_TARGET_REPLAY_FAILED");
        assert_eq!(
            format!("{error:#}"),
            "NATIVE_TARGET_REPLAY_FAILED: apply bar: underlying cause"
        );
    }

    #[test]
    fn event_failure_retains_specific_reason() {
        let mut status = ReplayStatus {
            failure: Some("NATIVE_ORDER_DENIED"),
            ..ReplayStatus::default()
        };
        status.record_callback_error(anyhow::anyhow!("SIMULATION_ALREADY_FAILED"));
        let error = status.finish_run(Ok(())).unwrap_err();
        assert_eq!(
            format!("{error:#}"),
            "NATIVE_TARGET_REPLAY_FAILED: NATIVE_ORDER_DENIED"
        );
    }

    #[test]
    fn native_run_error_or_success_is_preserved_without_actor_failure() {
        let mut status = ReplayStatus::default();
        assert!(status.finish_run(Ok(())).is_ok());
        let error = status
            .finish_run(Err(anyhow::anyhow!("native failure")))
            .unwrap_err();
        assert_eq!(error.to_string(), "native failure");
    }

    #[test]
    fn recorded_infeasible_study_is_terminal_without_consuming_future_targets() {
        let mut status = ReplayStatus::default();
        assert!(!status.recorded_replay_terminal(false, 1));
        assert!(!status.recorded_replay_terminal(true, 1));
        status.study_infeasible = true;
        assert!(status.recorded_replay_terminal(false, 1));
        assert_eq!(status.consumed, 0);
        status.failure = Some("NATIVE_TARGET_REPLAY_FAILED");
        assert!(!status.recorded_replay_terminal(false, 1));
        status.failure = None;
        status.study_infeasible = false;
        status.consumed = 1;
        assert!(!status.recorded_replay_terminal(false, 1));
        assert!(status.recorded_replay_terminal(true, 1));
    }
}
