//! Apply explicit frozen target instructions in one Nautilus account.
use crate::catalog::{NativeMarketData, load_catalog};
pub use crate::fee_models::FillFeeFailure;
use anyhow::{Result, ensure};
use bigdecimal::{BigDecimal, ToPrimitive};
use contracts::{DbCounter, DecimalValue, SchemaV1, science::*};
use nautilus_analysis::{
    analyzer::PortfolioAnalyzer,
    statistics::{cagr::CAGR, max_drawdown::MaxDrawdown},
};
use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
};
use nautilus_common::actor::DataActorNative;
use nautilus_common::{
    actor::DataActor,
    logging::logger::LoggerConfig,
    timer::{TimeEvent, TimeEventCallback},
};
use nautilus_core::DurationNanos;
use nautilus_execution::models::{
    fill::{DefaultFillModel, FillModelHandle},
    latency::{LatencyModelHandle, StaticLatencyModel},
};
use nautilus_model::{
    data::{Bar, BarType, DataBatch},
    enums::{AccountType, BookType, OmsType, OrderSide, OrderStatus},
    events::{
        OrderDenied, OrderFilled, OrderRejected, PositionChanged, PositionClosed, PositionOpened,
    },
    identifiers::{ClientOrderId, InstrumentId, StrategyId, Venue},
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

#[derive(Default)]
pub(crate) struct ReplayStatus {
    pub(crate) consumed: usize,
    pub(crate) failure: Option<&'static str>,
    callback_error: Option<anyhow::Error>,
    order_rejection: Option<String>,
    order_denial: Option<String>,
    submitted_after_ns: u64,
    settled_replay_ns: Option<u64>,
    spot_fills: Vec<OrderFilled>,
}

impl ReplayStatus {
    fn replay_terminal(&self, data_complete: bool, expected_targets: usize) -> bool {
        self.failure.is_none()
            && data_complete && self.consumed == expected_targets
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
            let detail = if code == "NATIVE_ORDER_DENIED" {
                self.order_denial.as_deref()
            } else {
                self.order_rejection.as_deref()
            };
            let error = match (code, detail) {
                ("NATIVE_ORDER_REJECTED", Some(reason)) => anyhow::anyhow!("{code}: {reason}"),
                ("NATIVE_ORDER_DENIED", Some(reason)) => anyhow::anyhow!("{code}: {reason}"),
                _ => anyhow::anyhow!(code),
            };
            return Err(error.context("NATIVE_TARGET_REPLAY_FAILED"));
        }
        native_result
    }
}

pub(crate) struct TargetReplay {
    core: StrategyCore,
    instruments: Vec<InstrumentAny>,
    bar_types: Vec<BarType>,
    latest: BTreeMap<InstrumentId, Bar>,
    points: Vec<NativeTargetPointV1>,
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
    status: Rc<RefCell<ReplayStatus>>,
    replay_bar_ends: Option<BTreeMap<InstrumentId, u64>>,
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
    fn on_order_denied(&mut self, event: OrderDenied) {
        let mut status = self.status.borrow_mut();
        status.order_denial = Some(event.reason.to_string());
        status.failure = Some("NATIVE_ORDER_DENIED");
    }
    fn on_order_rejected(&mut self, event: OrderRejected) {
        let mut status = self.status.borrow_mut();
        status.order_rejection = Some(event.reason.to_string());
        status.failure = Some("NATIVE_ORDER_REJECTED");
    }

    fn on_order_filled(&mut self, event: &OrderFilled) {
        let now = event.ts_event.as_u64();

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
    fn submit_target_order(&mut self, order: OrderAny) -> Result<()> {
        if self.wake_order_arrivals && self.latency_ns > 0 {
            let arrival = self
                .clock()
                .timestamp_ns()
                .as_u64()
                .checked_add(self.latency_ns)
                .ok_or_else(|| anyhow::anyhow!("NATIVE_LATENCY_TIME_RANGE"))?;
            let name = format!("TARGET-INSERT:{}", order.client_order_id());
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
        self.submit_order(order, None, None, None)
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
        if !self.awaiting_settlement || !self.orders_settled() {
            self.finish_replay_tail();
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
        self.finish_replay_tail();
    }

    fn finish_replay_tail(&self) {
        let Some(ends) = &self.replay_bar_ends else {
            return;
        };
        let mut status = self.status.borrow_mut();
        let data_complete = ends
            .iter()
            .all(|(id, end)| self.latest.get(id).map(|bar| bar.ts_init.as_u64()) == Some(*end));
        let finished =
            status.replay_terminal(data_complete, self.points.len()) && self.orders_settled();
        if finished {
            status
                .settled_replay_ns
                .get_or_insert_with(|| self.clock().timestamp_ns().as_u64());
        }
        drop(status);
        if finished {
            // The native engine stops at this settled timestamp, not after an
            // unrelated periodic snapshot sequence across a data-empty tail.
            self.shutdown_system(Some("TARGET_REPLAY_SETTLED".into()));
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
                    ready &= observed.ts_init.as_u64() == point.asof_ns.get();
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
        let point = point.clone();


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

                let quantity = instrument.try_make_qty_from_decimal(amount, Some(true))?;
                instrument.try_normalize_qty(quantity)?;
                let order = self.order().market(
                    id,
                    side,
                    quantity,
                    None,
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
        for kind in self.bar_types.clone() {
            self.subscribe_bars(kind, None, None);
        }

        Ok(())
    }

    fn on_bar(&mut self, bar: &Bar) -> Result<()> {
        let result = self.apply_bar(bar, bar.ts_init.as_u64());
        if let Err(error) = result {
            // Native actor callbacks may log rather than propagate. Keep an explicit
            // in-process failure observation that the outer adapter MUST inspect.
            self.status.borrow_mut().record_callback_error(error);
            return Err(anyhow::anyhow!("NATIVE_TARGET_REPLAY_FAILED"));
        }
        self.finish_replay_tail();
        Ok(())
    }
}

pub(crate) fn execution_market(
    data: &NativeMarketData,
    settings: &NativeSimulationSettingsV1,
) -> Result<Currency> {
    domain::portfolio::simulation_settings(settings)?;
    let currency = execution_market_terms(data, settings)?;
    // Historical replay always authenticates its independently frozen catalog
    // fee source. Unsupported instrument/account terms retain their specific
    // error instead of being obscured by a secondary source mismatch.
    crate::fee_models::catalog_fees::validate_market(data, settings)?;
    Ok(currency)
}

/// Source-independent native terms; callers must also validate their fee owner.
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
                InstrumentAny::CurrencyPair(_) | InstrumentAny::Equity(_)
            ) && instrument.venue() == venue
                && instrument.quote_currency() == currency
                && instrument.settlement_currency() == currency
                && !instrument.is_inverse()
                && !instrument.has_expiration(),
            "SIMULATION_MARKET_UNSUPPORTED"
        );
        domain::catalogs::execution_account(
            if matches!(instrument, InstrumentAny::CurrencyPair(_)) {
                "CurrencyPair"
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

    let mut total = point.cash_weight.as_decimal().clone();
    let mut gross = BigDecimal::from(0);
    for (target, instrument) in point.targets.iter().zip(instruments) {
        ensure!(
            target.instrument_id == instrument.id().to_string()
                && target.currency == settings.base_currency,
            "SIMULATION_TARGET_IDENTITY"
        );
        if settings.account_kind == NativeAccountKind::Cash {
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
    _settings: &NativeSimulationSettingsV1,
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
    let period = 252;
    crate::performance::register_tail_risk(&mut analyzer, period);
    analyzer.register_statistic(Arc::new(CAGR::new(Some(period))));

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
    run_with_strategy_and_spot(root, request, None)
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

    run_with_strategy_and_spot(
        root,
        request,
        Some(crate::spot_cash_runtime::SpotReplayInputs {
            dataset_revision_id,
            closed_rows,
            observed_start: None,
        }),
    )
}

/// Explicit observed-inventory runner. Empty points mean no submitted orders,
/// never repeated target weights or liquidation. Only this wrapper permits it.
pub(crate) fn run_observed_spot(
    root: &Path,
    request: &NativeSimulationRequestV1,
    dataset_revision_id: contracts::Id,
    closed_rows: Vec<crate::spot_cash_capture::ClosedBarSourceRow>,
    initial_state: &contracts::observed_spot::NativeObservedSpotStartV1,
) -> Result<NativeSimulationResultV1> {
    domain::observed_spot::starting_balances(
        initial_state,
        &request.settings,
        request.selection.event_start_ns,
    )?;

    run_with_strategy_and_spot(
        root,
        request,
        Some(crate::spot_cash_runtime::SpotReplayInputs {
            dataset_revision_id,
            closed_rows,
            observed_start: Some(initial_state.clone()),
        }),
    )
}

fn run_with_strategy_and_spot(
    root: &Path,
    request: &NativeSimulationRequestV1,
    mut spot_inputs: Option<crate::spot_cash_runtime::SpotReplayInputs>,
) -> Result<NativeSimulationResultV1> {
    ensure!(
        spot_inputs.is_none()
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
    let wake_order_arrivals = scoped_replay || is_spot;
    let mut market = if !request.target_points.is_empty()
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
    let projected_bar_types = market
        .series
        .iter()
        .filter(|series| {
            series.bar_type.aggregation_source()
                == nautilus_model::enums::AggregationSource::Internal
        })
        .map(|series| {
            serde_json::json!({
                "source_type": series.bar_type.to_string(),
                "runtime_type": crate::catalog::runtime_bar_type(series.bar_type).to_string(),
                "source_event_clock": "RIGHT_OPEN_INTERVAL_START",
                "runtime_event_clock": "THEORETICAL_INTERVAL_CLOSE",
                "availability_clock": "UNCHANGED_SOURCE_TS_INIT",
                "external_means": "SUPPLIED_FROM_OUTSIDE_THIS_BACKTEST_RUNTIME",
            })
        })
        .collect::<Vec<_>>();
    let projected_bar_evidence = (!projected_bar_types.is_empty())
        .then(|| serde_json::to_string(&projected_bar_types))
        .transpose()?;
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
    let first_event = market
        .series
        .iter()
        .flat_map(|series| &series.bars)
        .map(|bar| bar.ts_init)
        .min()
        .ok_or_else(|| anyhow::anyhow!("CATALOG_EMPTY_SELECTION"))?;
    let bar_ends = market.series.iter().flat_map(|series| &series.bars).fold(
        BTreeMap::<InstrumentId, u64>::new(),
        |mut ends, bar| {
            let time = bar.ts_init.as_u64();
            ends.entry(bar.bar_type.instrument_id())
                .and_modify(|end| *end = (*end).max(time))
                .or_insert(time);
            ends
        },
    );
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
        bar_types: market
            .series
            .iter()
            .map(|s| crate::catalog::runtime_bar_type(s.bar_type))
            .collect(),
        latest: BTreeMap::new(),
        points: request.target_points.clone(),
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
        // Every selected series must deliver its own final real BAR before
        // shutdown, even when the markets' observation clocks differ.
        replay_bar_ends: wake_order_arrivals.then(|| bar_ends.clone()),
        status: status.clone(),
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
    let fill_fees = crate::fee_models::FillFeeObserver::default();
    let live_progress = crate::live_progress::LiveProgress::attach();
    let result = (|| -> Result<NativeSimulationResultV1> {
        let settings = &request.settings;

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
        let fill_model = FillModelHandle::new(default_fill);
        {
            let fee = if is_spot {
                crate::spot_cash_runtime::fee_model(request, &market)?
            } else {
                crate::fee_models::catalog_fees::fee_model(
                    &market,
                    settings,
                    engine.kernel().clock.clone(),
                )?
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
                                Money::from_str_exact(&format!(
                                    "{} {}",
                                    balance.amount.as_decimal().to_plain_string(),
                                    balance.currency
                                ))
                                .map_err(anyhow::Error::msg)
                            })
                            .collect::<Result<Vec<_>>>()?
                    } else {
                        vec![
                            Money::from_str_exact(&format!(
                                "{} {}",
                                settings.starting_capital.as_decimal().to_plain_string(),
                                settings.base_currency
                            ))
                            .map_err(anyhow::Error::msg)?,
                        ]
                    })
                    .default_leverage(native_decimal(&settings.leverage)?)
                    // Every fee model can fail at the actual fill, including a
                    // delayed fill after a frozen maker/taker policy has expired.
                    // Preserve that original cause before native shutdown or the
                    // generic unsettled-order checks can obscure it.
                    .fee_model(fill_fees.wrap(fee))
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
        }
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
        for series in &mut market.series {
            // Native replay merges these streams by receive time and retains
            // insertion priority on ties, matching the original series order.
            // Scoped selections may legitimately have an empty asset series.
            if !series.bars.is_empty() {
                // Frozen source evidence keeps INTERNAL/RightOpen semantics.
                // The consuming native engine requires explicitly external
                // delivery with a closed event label; no quote/trade is invented.
                let bars = std::mem::take(&mut series.bars)
                    .into_iter()
                    .map(crate::catalog::runtime_bar)
                    .collect::<Result<Vec<_>>>()?;
                engine.add_data_batch(DataBatch::Bar(bars.into()), None, true, true)?;
            }
        }

        // Event bounds select BARs, not their later permitted receive clocks.
        // Scoped orders may also need arrival callbacks after the data cutoff;
        // their original expiry bounds execution, while settled-final-BAR
        // shutdown still prevents manufacturing an empty account-return tail.
        // Final latency alerts must run with the strategy active: end() stops
        // it before draining commands and therefore cannot consume their callbacks.
        let replay_end = scoped_end.or(spot_end.map(|end| end.get())).map(Into::into);
        let native_result = engine.run(None, replay_end, None, false);
        // Original callback errors retain precedence over native fee failure.
        status.borrow_mut().check_callback_error()?;

        fill_fees
            .check()
            .map_err(|error| error.context("NATIVE_TARGET_REPLAY_FAILED"))?;
        status.borrow_mut().finish_run(native_result)?;

        let observed = status.borrow();
        let expected_targets = request.target_points.len();
        ensure!(
            observed.consumed == expected_targets,
            "NATIVE_TARGETS_NOT_CONSUMED: consumed={} expected={}",
            observed.consumed,
            expected_targets
        );
        let native = engine.get_result();
        ensure!(
            native.summary.get("orders.open").map(String::as_str) == Some("0")
                && native.summary.get("orders.inflight").map(String::as_str) == Some("0"),
            "NATIVE_ORDERS_NOT_SETTLED"
        );
        let mut spot_cash_report = if let Some(capture) = spot_capture.take() {
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
                    daily.get_performance_stats_portfolio_returns()
                },
            ),
            (NativeStatisticGroup::General, native.stats_general),
        ] {
            for (key, value) in values {
                let mut entry = statistic(group, key, None, value);
                if group == NativeStatisticGroup::Returns && !fully_priced {
                    entry.value = None;
                    entry.reason_code = Some("NATIVE_STATISTIC_UNAVAILABLE".into());
                }
                statistics.push(entry);
            }
        }
        if let Some(benchmark) = &request.settings.price_benchmark {
            if let Some(report) = &mut spot_cash_report {
                let days = report
                    .daily_returns
                    .days
                    .iter()
                    .map(|d| {
                        (
                            d.day_start_ns.get(),
                            d.day_end_ns.get(),
                            if d.reason.is_none() { d.value } else { None },
                        )
                    })
                    .collect::<Vec<_>>();
                report
                    .statistics
                    .extend(crate::performance::versus_catalog_price(
                        root,
                        benchmark,
                        &request.selection,
                        &report.daily_returns.report_currency,
                        usize::from(report.daily_returns.annualization_days),
                        &days,
                    ));
                report
                    .statistics
                    .sort_by(|a, b| a.native_key.cmp(&b.native_key));
            } else {
                let accounts = engine.kernel().cache.borrow().accounts_all_owned();
                let snapshots = engine
                    .kernel()
                    .portfolio
                    .borrow()
                    .snapshots(&accounts[0].id());
                let boundaries: std::collections::BTreeSet<_> =
                    snapshots.iter().map(|s| s.ts_event.as_u64()).collect();
                let days = daily
                    .portfolio_returns()
                    .iter()
                    .map(|(time, value)| {
                        let start = time.as_u64();
                        let end = start.saturating_add(86_400_000_000_000);
                        (
                            start,
                            end,
                            (fully_priced
                                && boundaries.contains(&start)
                                && boundaries.contains(&end))
                            .then_some(*value),
                        )
                    })
                    // Partial start/end calendar days are not part of the
                    // complete UTC daily comparison window.
                    .filter(|(start, end, _)| {
                        *start >= request.selection.event_start_ns.get()
                            && *end <= request.selection.event_end_ns.get()
                    })
                    .collect::<Vec<_>>();
                statistics.extend(crate::performance::versus_catalog_price(
                    root,
                    benchmark,
                    &request.selection,
                    &request.settings.base_currency,
                    252,
                    &days,
                ));
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
        let mut summary: BTreeMap<String, String> = native.summary.into_iter().collect();
        if let Some(evidence) = projected_bar_evidence {
            summary.insert("qz.source_runtime_bar_projection".into(), evidence);
        }
        Ok(NativeSimulationResultV1 {
            settled_replay_ns: observed
                .settled_replay_ns
                .map(DbCounter::new)
                .transpose()
                .map_err(anyhow::Error::msg)?,
            spot_cash_report,
            schema_version: SchemaV1,
            native_version: crate::NAUTILUS_VERSION.into(),
            iterations: count(native.iterations)?,
            events: count(native.total_events)?,
            orders: count(native.total_orders)?,
            positions: count(native.total_positions)?,
            consumed_target_points: count(observed.consumed)?,
            summary,
            statistics,
            returns_kind: NativeReturnsKind::PortfolioDaily,
            returns_status,
            returns_reason,
            returns,
            canonical_result: engine.get_canonical_result()?.as_value().clone(),
        })
    })();
    drop(live_progress);
    engine.dispose();
    result
}

#[cfg(test)]
mod replay_error_tests {
    use super::*;
    use nautilus_model::types::{Price, Quantity};
    #[test]
    fn native_order_rejection_retains_closed_reason_under_original_outer_code() {
        let mut status = ReplayStatus {
            failure: Some("NATIVE_ORDER_REJECTED"),
            order_rejection: Some("Market HYPERLIQUID is CLOSED".into()),
            ..ReplayStatus::default()
        };
        let error = status
            .finish_run(Err(anyhow::anyhow!("native shutdown")))
            .unwrap_err();
        assert_eq!(error.to_string(), "NATIVE_TARGET_REPLAY_FAILED");
        assert_eq!(
            format!("{error:#}"),
            "NATIVE_TARGET_REPLAY_FAILED: NATIVE_ORDER_REJECTED: Market HYPERLIQUID is CLOSED"
        );
    }

    #[test]
    fn rejection_detail_cannot_replace_an_original_callback_chain() {
        let mut status = ReplayStatus::default();
        status
            .record_callback_error(anyhow::anyhow!("original callback cause").context("apply bar"));
        status.failure = Some("NATIVE_ORDER_REJECTED");
        status.order_rejection = Some("Market HYPERLIQUID is CLOSED".into());
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
                anyhow::bail!("NATIVE_FILL_FEE_FAILED")
            }
        }
        let instrument =
            InstrumentAny::CurrencyPair(nautilus_model::instruments::stubs::audusd_sim());
        let order = OrderTestBuilder::new(OrderType::Market)
            .instrument_id(instrument.id())
            .side(OrderSide::Buy)
            .quantity(Quantity::from("10.000000"))
            .build();
        let fees = crate::fee_models::FillFeeObserver::default();
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
        assert_eq!(fees.failure().unwrap().reason, "NATIVE_FILL_FEE_FAILED");
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
    fn replay_is_terminal_only_after_all_data_and_targets_are_consumed() {
        let mut status = ReplayStatus::default();
        assert!(!status.replay_terminal(false, 1));
        assert!(!status.replay_terminal(true, 1));
        assert_eq!(status.consumed, 0);
        status.failure = Some("NATIVE_TARGET_REPLAY_FAILED");
        assert!(!status.replay_terminal(false, 1));
        status.failure = None;
        status.consumed = 1;
        assert!(!status.replay_terminal(false, 1));
        assert!(status.replay_terminal(true, 1));
    }
}
