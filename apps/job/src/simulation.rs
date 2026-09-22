//! Replay frozen targets in one native Nautilus account. This is never live execution.
use crate::catalog::{load_catalog, NativeMarketData};
use anyhow::{ensure, Result};
use bigdecimal::{BigDecimal, ToPrimitive};
use contracts::{science::*, DbCounter, DecimalValue, SchemaV1};
use nautilus_analysis::{
    analyzer::{PortfolioAnalyzer, Statistic},
    statistics::{
        returns_volatility::ReturnsVolatility, sharpe_ratio::SharpeRatio,
        sortino_ratio::SortinoRatio,
    },
};
use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
};
use nautilus_common::{actor::DataActor, logging::logger::LoggerConfig};
use nautilus_execution::models::{
    fill::{DefaultFillModel, FillModelHandle},
    latency::{LatencyModelHandle, StaticLatencyModel},
};
use nautilus_model::{
    data::{Bar, BarType, Data, InstrumentClose},
    enums::{AccountType, BookType, OmsType, OrderSide, OrderStatus},
    events::{OrderDenied, OrderFilled, OrderRejected},
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
};

const MAX_TARGET_ORDERS: usize = 65_536;

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
struct ReplayStatus {
    consumed: usize,
    failure: Option<&'static str>,
    submitted_after_ns: u64,
    study_infeasible: bool,
    frames: Vec<NativePortfolioStudyFrameV1>,
}

pub(crate) struct StudyInput {
    pub cutoff_ns: DbCounter,
    pub input: contracts::portfolio::AllocationInputV1,
    pub slippage: Vec<NativePortfolioSlippageReferenceV1>,
    pub bar_notionals: Vec<contracts::execution::NativeBarNotionalV1>,
    pub liquidity_maximum_age: Option<u32>,
}

struct TargetReplay {
    core: StrategyCore,
    instruments: Vec<InstrumentAny>,
    bar_types: Vec<BarType>,
    latest: BTreeMap<InstrumentId, Bar>,
    points: Vec<NativeTargetPointV1>,
    study_inputs: Vec<StudyInput>,
    settings: NativeSimulationSettingsV1,
    currency: Currency,
    venue: Venue,
    tolerance: Decimal,
    latency_ns: u64,
    reduction_ids: BTreeSet<ClientOrderId>,
    deferred_orders: Vec<OrderAny>,
    active_expiry_ns: u64,
    settlement_events: BTreeMap<InstrumentId, InstrumentClose>,
    status: Rc<RefCell<ReplayStatus>>,
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
    fn on_order_rejected(&mut self, _event: OrderRejected) {
        self.status.borrow_mut().failure = Some("NATIVE_ORDER_REJECTED");
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
        if now <= self.status.borrow().submitted_after_ns || now >= self.active_expiry_ns {
            self.status.borrow_mut().failure = Some("NATIVE_NONCAUSAL_OR_EXPIRED_FILL");
            return;
        }
        if self.reduction_ids.contains(&event.client_order_id)
            && self
                .cache()
                .order(&event.client_order_id)
                .is_some_and(|order| order.status() == OrderStatus::Filled)
        {
            self.reduction_ids.remove(&event.client_order_id);
            if self.reduction_ids.is_empty() && self.submit_deferred(now).is_err() {
                self.status.borrow_mut().failure = Some("NATIVE_DEFERRED_TARGET_FAILED");
            }
        }
    }
});

impl TargetReplay {
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
        self.status.borrow_mut().submitted_after_ns = now;
        for order in std::mem::take(&mut self.deferred_orders) {
            self.submit_order(order, None, None, None)?;
            ensure!(
                self.status.borrow().failure.is_none(),
                "NATIVE_ORDER_NOT_ACCEPTED"
            );
        }
        self.status.borrow_mut().consumed += 1;
        Ok(())
    }

    fn apply_bar(&mut self, bar: &Bar) -> Result<()> {
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
                bar.ts_init.as_u64() < self.active_expiry_ns,
                "SIMULATION_REDUCTION_EXPIRED"
            );
            return Ok(());
        }
        let next = self.status.borrow().consumed;
        let Some(mut point) = self.points.get(next).cloned() else {
            return Ok(());
        };
        // Every asset must have a completed price for the same event time. No
        // forward fill, partial cross-section, or hidden look-ahead to later bars.
        if self.latest.len() != self.instruments.len()
            || self
                .latest
                .values()
                .any(|other| other.ts_event != bar.ts_event)
        {
            return Ok(());
        }
        let now = self
            .latest
            .values()
            .map(|v| v.ts_init.as_u64())
            .max()
            .ok_or_else(|| anyhow::anyhow!("SIMULATION_MISSING_PRICE"))?;
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
        ensure!(
            self.cache().orders_open_count(None, None, None, None, None) == 0
                && self
                    .cache()
                    .orders_inflight_count(None, None, None, None, None)
                    == 0,
            "SIMULATION_UNSETTLED_PREVIOUS_TARGET"
        );
        let equity = self
            .portfolio()
            .equity(&self.venue, None)
            .get(&self.currency)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("SIMULATION_EQUITY_UNAVAILABLE"))?
            .as_decimal();
        ensure!(equity > Decimal::ZERO, "SIMULATION_NONPOSITIVE_EQUITY");
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
                let price = self.latest[&instrument.id()].close.as_decimal();
                let notional = checked(
                    checked(
                        self.portfolio()
                            .net_position(&instrument.id())
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
        let mut reductions = Vec::<OrderAny>::new();
        let mut increases = Vec::<OrderAny>::new();
        for (instrument, target) in self.instruments.iter().zip(&point.targets) {
            let id = instrument.id();
            let price = self
                .latest
                .get(&id)
                .ok_or_else(|| anyhow::anyhow!("SIMULATION_MISSING_PRICE"))?
                .close
                .as_decimal();
            let unit_value = checked(price.checked_mul(instrument.multiplier().as_decimal()))?;
            ensure!(unit_value > Decimal::ZERO, "SIMULATION_INVALID_UNIT_VALUE");
            let desired = checked(
                checked(native_decimal(&target.weight)?.checked_mul(equity))?
                    .checked_div(unit_value),
            )?;
            let current = self.portfolio().net_position(&id);
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
                let order = self.order().market(
                    id,
                    side,
                    quantity,
                    None,
                    Some(reduce_only),
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
            self.submit_order(order, None, None, None)?;
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
        if let Err(error) = self.apply_bar(bar) {
            // Native actor callbacks may log rather than propagate. Keep an explicit
            // in-process failure observation that the outer adapter MUST inspect.
            self.status
                .borrow_mut()
                .failure
                .get_or_insert("NATIVE_TARGET_REPLAY_FAILED");
            return Err(error);
        }
        Ok(())
    }
}

pub(crate) fn execution_market(
    data: &NativeMarketData,
    settings: &NativeSimulationSettingsV1,
) -> Result<Currency> {
    domain::portfolio::simulation_settings(settings)?;
    crate::prediction::validate_market(data, settings)?;
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
        let rate = fees
            .remove(instrument.id().to_string().as_str())
            .ok_or_else(|| anyhow::anyhow!("SIMULATION_FEE_MISSING"))?;
        if !matches!(instrument, InstrumentAny::BinaryOption(_)) {
            ensure!(
                native_decimal(&rate.maker)? == instrument.maker_fee()
                    && native_decimal(&rate.taker)? == instrument.taker_fee(),
                "SIMULATION_FEE_SOURCE_MISMATCH"
            );
        }
    }
    Ok(currency)
}

fn validate_settings(
    data: &NativeMarketData,
    request: &NativeSimulationRequestV1,
) -> Result<Currency> {
    let settings = &request.settings;
    let currency = execution_market(data, settings)?;
    let instruments = data
        .series
        .iter()
        .map(|s| s.instrument.clone())
        .collect::<Vec<_>>();
    ensure!(
        (1..=10_000).contains(&request.target_points.len())
            && request
                .target_points
                .len()
                .checked_mul(data.series.len())
                .is_some_and(|n| n <= MAX_TARGET_ORDERS),
        "SIMULATION_TARGET_COUNT_LIMIT"
    );
    let span = request.selection.event_end_ns.get() - request.selection.event_start_ns.get();
    ensure!(
        span / (u64::from(settings.snapshot_interval_ms) * 1_000_000) <= 1_000_000,
        "SIMULATION_SNAPSHOT_COUNT_LIMIT"
    );
    for (index, point) in request.target_points.iter().enumerate() {
        ensure!(
            point.asof_ns < point.valid_until_ns
                && point.asof_ns >= request.selection.event_start_ns
                && point.asof_ns < request.selection.event_end_ns
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
    ensure!(
        (total - BigDecimal::from(1)).abs() <= *settings.exposure_tolerance.as_decimal()
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
) -> Result<PortfolioAnalyzer> {
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
    let mut analyzer = PortfolioAnalyzer::default();
    let period = domain::prediction::portfolio_annualization_days(&settings.fee_model);
    if domain::prediction::uses_native_fee(&settings.fee_model) {
        use std::sync::Arc;
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
    Ok(analyzer)
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
    ensure!(
        study_inputs.is_empty() || study_inputs.len() == request.target_points.len(),
        "STUDY_FRAME_COUNT"
    );
    let (fill, latency) = domain::portfolio::simulation_models(&request.settings)?;
    let market = load_catalog(root, &request.selection)?;
    let currency = validate_settings(&market, request)?;
    let closes = crate::prediction::close_events(root, &market, &request.selection)?;
    let settlement_events = closes
        .iter()
        .map(|c| (c.instrument_id, *c))
        .collect::<BTreeMap<_, _>>();
    let settled_ids = settlement_events.keys().copied().collect::<Vec<_>>();
    let venue = market.series[0].instrument.venue();
    let status = Rc::new(RefCell::new(ReplayStatus::default()));
    let strategy = TargetReplay {
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
        settings: request.settings.clone(),
        currency,
        venue,
        tolerance: native_decimal(&request.settings.exposure_tolerance)?,
        latency_ns: latency.base_latency_ns.get() + latency.insert_latency_ns.get(),
        reduction_ids: BTreeSet::new(),
        deferred_orders: Vec::new(),
        active_expiry_ns: 0,
        settlement_events,
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
        ..BacktestEngineConfig::default()
    };
    let mut engine = BacktestEngine::new(config)?;
    let result = (|| -> Result<Option<NativeSimulationResultV1>> {
        let settings = &request.settings;
        engine.add_venue(
            SimulatedVenueConfig::builder()
                .venue(venue)
                .oms_type(OmsType::Netting)
                .account_type(match settings.account_kind {
                    NativeAccountKind::Cash => AccountType::Cash,
                    NativeAccountKind::Margin => AccountType::Margin,
                })
                .book_type(BookType::L1_MBP)
                .base_currency(currency)
                .starting_balances(vec![Money::from_str(&format!(
                    "{} {}",
                    settings.starting_capital.as_decimal().to_plain_string(),
                    settings.base_currency
                ))
                .map_err(anyhow::Error::msg)?])
                .default_leverage(native_decimal(&settings.leverage)?)
                .fee_model(crate::prediction::fee_model(settings))
                .fill_model(FillModelHandle::new(DefaultFillModel::new(
                    fill.prob_fill_on_limit
                        .as_decimal()
                        .to_f64()
                        .ok_or_else(|| anyhow::anyhow!("FILL_PROBABILITY_RANGE"))?,
                    fill.prob_slippage
                        .as_decimal()
                        .to_f64()
                        .ok_or_else(|| anyhow::anyhow!("SLIPPAGE_PROBABILITY_RANGE"))?,
                    Some(fill.random_seed.get()),
                )?))
                .latency_model(LatencyModelHandle::new(StaticLatencyModel::new(
                    latency.base_latency_ns.get().into(),
                    latency.insert_latency_ns.get().into(),
                    latency.update_latency_ns.get().into(),
                    latency.cancel_latency_ns.get().into(),
                )))
                .bar_execution(true)
                .liquidity_consumption(true)
                .build()?,
        )?;
        let mut events = Vec::with_capacity(market.rows);
        for series in market.series {
            engine.add_instrument(&series.instrument)?;
            events.extend(series.bars.into_iter().map(Data::Bar));
        }
        events.extend(closes.into_iter().map(Data::InstrumentClose));
        engine.add_strategy(strategy)?;
        engine.add_data(events, None, true, true)?;
        engine.run(None, None, None, false)?;
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
        ensure!(observed.failure.is_none(), "NATIVE_TARGET_REPLAY_FAILED");
        if observed.study_infeasible {
            return Ok(None);
        }
        ensure!(
            observed.consumed == request.target_points.len(),
            "NATIVE_TARGETS_NOT_CONSUMED"
        );
        let native = engine.get_result();
        ensure!(
            native.summary.get("orders.open").map(String::as_str) == Some("0")
                && native.summary.get("orders.inflight").map(String::as_str) == Some("0"),
            "NATIVE_ORDERS_NOT_SETTLED"
        );
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
        let daily = portfolio_return_analysis(&engine, &request.settings)?;
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
        } else if returns.iter().any(|point| point.value.is_none()) {
            (
                contracts::evidence::MetricStatus::Failed,
                Some("NATIVE_RETURN_UNAVAILABLE".into()),
            )
        } else {
            (contracts::evidence::MetricStatus::Ok, None)
        };
        Ok(Some(NativeSimulationResultV1 {
            schema_version: SchemaV1,
            native_version: "0.63.0".into(),
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
            canonical_result: serde_json::from_slice(&engine.get_canonical_result()?.to_bytes()?)?,
        }))
    })();
    engine.dispose();
    let frames = std::mem::take(&mut status.borrow_mut().frames);
    Ok((result?, frames))
}
