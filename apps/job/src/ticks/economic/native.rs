//! Official-engine adapter for an explicit cohort-mark simulation, never a source BAR catalog.
use anyhow::{ensure, Context, Result};
use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
    modules::{
        AccountAdjustmentOutcome, ExchangeContext, SimulationModule, SimulationModuleHandle,
        SimulationModuleResult,
    },
};
use nautilus_common::{
    actor::DataActor,
    cache::config::CacheConfig,
    logging::logger::LoggerConfig,
    msgbus::{get_message_bus, set_message_bus, MessageBus},
};
use nautilus_core::UnixNanos;
use nautilus_execution::models::{
    fee::{FeeModel, FeeModelHandle},
    fill::{DefaultFillModel, FillModelHandle},
};
use nautilus_model::{
    accounts::{
        margin_model::{MarginModel, MarginModelHandle},
        Account,
    },
    data::{Bar, BarSpecification, BarType, Data, FundingRateUpdate, OrderBookDeltas, QuoteTick},
    enums::{
        AccountType, AggregationSource, BarAggregation, BookType, OmsType, OrderSide, PriceType,
        TimeInForce,
    },
    events::{OrderDenied, OrderFilled, OrderRejected},
    identifiers::{InstrumentId, StrategyId, Venue},
    instruments::{Instrument, InstrumentAny},
    orderbook::OrderBook,
    orders::OrderAny,
    position::Position,
    types::{
        fixed::{check_fixed_raw_i128, check_fixed_raw_u128},
        Currency, Money, Price, Quantity,
    },
};
use nautilus_trading::{
    nautilus_strategy,
    strategy::{Strategy, StrategyConfig, StrategyCore},
};
use rust_decimal::Decimal;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    rc::Rc,
};

fn arithmetic(value: Option<Decimal>) -> Result<Decimal> {
    value.context("TICK_ECONOMIC_NATIVE_DECIMAL_RANGE")
}

#[allow(
    clippy::useless_conversion,
    reason = "Native fixed-point raw widths depend on upstream high-precision feature"
)]
fn encode_book(snapshot: &mut OrderBookDeltas, instrument: &InstrumentAny) -> Result<()> {
    let price_precision = instrument.price_precision();
    let size_precision = instrument.size_precision();
    for delta in &mut snapshot.deltas {
        if delta.order.price.precision != price_precision {
            check_fixed_raw_i128(i128::from(delta.order.price.raw()), price_precision).with_context(
                || {
                    format!(
                        "TICK_ECONOMIC_BOOK_PRICE_PRECISION_UNSUPPORTED: instrument_id={} received_ns={} value={} required_precision={price_precision}",
                        delta.instrument_id, delta.ts_init, delta.order.price
                    )
                },
            )?;
            delta.order.price = Price::from_raw_checked(delta.order.price.raw(), price_precision)?;
        }
        if delta.order.size.precision != size_precision {
            check_fixed_raw_u128(u128::from(delta.order.size.raw()), size_precision).with_context(
                || {
                    format!(
                        "TICK_ECONOMIC_BOOK_SIZE_PRECISION_UNSUPPORTED: instrument_id={} received_ns={} value={} required_precision={size_precision}",
                        delta.instrument_id, delta.ts_init, delta.order.size
                    )
                },
            )?;
            delta.order.size = Quantity::from_raw_checked(delta.order.size.raw(), size_precision)?;
        }
    }
    Ok(())
}

pub(super) fn affordable_depth_quantity(
    instrument: &InstrumentAny,
    levels: impl IntoIterator<Item = (Decimal, Decimal)>,
    maximum: Decimal,
    mut closing: Decimal,
    mut budget: Decimal,
    fee_fraction: Decimal,
) -> Result<Decimal> {
    let precision = u32::from(instrument.quote_currency().precision);
    let rounding_unit = Decimal::new(1, precision);
    let mut remaining = maximum;
    let mut quantity = Decimal::ZERO;
    for (price, size) in levels {
        if remaining.is_zero() {
            break;
        }
        let mut take = remaining.min(size);
        let reducing = take.min(closing);
        quantity += reducing;
        remaining -= reducing;
        closing -= reducing;
        take -= reducing;
        let unit = arithmetic(price.checked_mul(instrument.multiplier().as_decimal()))?;
        let notional = arithmetic(take.checked_mul(unit))?;
        let fee = arithmetic(notional.checked_mul(fee_fraction))?;
        let cost = arithmetic(
            notional
                .round_dp_with_strategy(
                    precision,
                    rust_decimal::RoundingStrategy::ToPositiveInfinity,
                )
                .checked_add(fee.round_dp_with_strategy(
                    precision,
                    rust_decimal::RoundingStrategy::ToPositiveInfinity,
                )),
        )?;
        if cost > budget {
            // Bound both native Money roundings without iterating individual lot increments.
            let reserve = if fee_fraction.is_zero() {
                rounding_unit
            } else {
                rounding_unit * Decimal::TWO
            };
            let allowed = arithmetic(
                (budget - reserve)
                    .max(Decimal::ZERO)
                    .checked_div(arithmetic(unit.checked_mul(Decimal::ONE + fee_fraction))?),
            )?;
            return arithmetic(quantity.checked_add(take.min(allowed)));
        }
        budget -= cost;
        quantity += take;
        remaining -= take;
    }
    Ok(quantity)
}

fn closing_limit(
    book: &OrderBook,
    instrument: &InstrumentAny,
    side: OrderSide,
    quantity: Quantity,
) -> Result<Price> {
    let levels = if side == OrderSide::Buy {
        book.asks_as_map(None)
    } else {
        book.bids_as_map(None)
    };
    let mut remaining = quantity.as_decimal();
    let mut last = None;
    for (price, size) in levels {
        last = Some(price);
        remaining -= remaining.min(size);
        if remaining.is_zero() {
            break;
        }
    }
    let increment = instrument.price_increment().as_decimal();
    let rounding = if side == OrderSide::Buy {
        rust_decimal::RoundingStrategy::ToPositiveInfinity
    } else {
        rust_decimal::RoundingStrategy::ToNegativeInfinity
    };
    let price = arithmetic(
        last.context("TICK_ECONOMIC_CLOSING_SIDE_MISSING")?
            .checked_div(increment),
    )?
    .round_dp_with_strategy(0, rounding)
        * increment;
    instrument.try_make_price_from_decimal(price)
}

#[derive(Debug)]
struct ScenarioFee(Decimal);
impl FeeModel for ScenarioFee {
    fn get_commission(
        &self,
        _order: &OrderAny,
        quantity: Quantity,
        price: Price,
        instrument: &InstrumentAny,
    ) -> Result<Money> {
        let notional = arithmetic(quantity.as_decimal().checked_mul(price.as_decimal()))?;
        let notional = arithmetic(notional.checked_mul(instrument.multiplier().as_decimal()))?;
        let amount = arithmetic(notional.checked_mul(self.0))?;
        let currency = if matches!(instrument, InstrumentAny::CurrencyPair(_)) {
            instrument.quote_currency()
        } else {
            instrument.settlement_currency()
        };
        Ok(Money::from_decimal(amount, currency)?)
    }
}

#[derive(Debug)]
struct ScenarioMargin {
    initial: Decimal,
    maintenance: Decimal,
}
impl ScenarioMargin {
    fn amount(
        &self,
        instrument: &dyn Instrument,
        quantity: Quantity,
        price: Price,
        fraction: Decimal,
    ) -> Result<Money> {
        let notional = arithmetic(quantity.as_decimal().checked_mul(price.as_decimal()))?;
        let notional = arithmetic(notional.checked_mul(instrument.multiplier().as_decimal()))?;
        Ok(Money::from_decimal(
            arithmetic(notional.checked_mul(fraction))?,
            instrument.settlement_currency(),
        )?)
    }
}
impl MarginModel for ScenarioMargin {
    fn name(&self) -> &'static str {
        "QZ_FIXED_FRACTION_COHORT_SCENARIO"
    }
    fn calculate_initial_margin(
        &self,
        instrument: &dyn Instrument,
        quantity: Quantity,
        price: Price,
        _leverage: Decimal,
        _inverse: Option<bool>,
    ) -> Result<Money> {
        self.amount(instrument, quantity, price, self.initial)
    }
    fn calculate_maintenance_margin(
        &self,
        instrument: &dyn Instrument,
        quantity: Quantity,
        price: Price,
        _leverage: Decimal,
        _inverse: Option<bool>,
    ) -> Result<Money> {
        self.amount(instrument, quantity, price, self.maintenance)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct NativeSummary {
    pub submitted: u64,
    pub fills: u64,
    pub rejected: u64,
    pub skipped_cash: u64,
    pub skipped_minimum: u64,
    pub commissions: BTreeMap<String, Decimal>,
    pub adjustments_applied: u64,
    pub adjustment_totals: BTreeMap<String, Decimal>,
    pub nominal_settlement_usdc: Decimal,
}
#[derive(Default)]
struct State {
    targets: BTreeMap<InstrumentId, Decimal>,
    capacities: BTreeMap<InstrumentId, Decimal>,
    inventory_targets: BTreeSet<InstrumentId>,
    summary: NativeSummary,
    failure: Option<String>,
}
struct CohortStrategy {
    core: StrategyCore,
    instruments: BTreeMap<InstrumentId, InstrumentAny>,
    bars: Vec<BarType>,
    venue: Venue,
    cash: bool,
    synthetic_quote_ioc: bool,
    observed_l2: bool,
    cost_fraction: Decimal,
    state: Rc<RefCell<State>>,
}
impl fmt::Debug for CohortStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CohortSimulationStrategy")
            .finish_non_exhaustive()
    }
}
nautilus_strategy!(CohortStrategy, {
    fn on_order_denied(&mut self, _event: OrderDenied) {
        self.state.borrow_mut().summary.rejected += 1;
    }
    fn on_order_rejected(&mut self, _event: OrderRejected) {
        self.state.borrow_mut().summary.rejected += 1;
    }
    fn on_order_filled(&mut self, event: &OrderFilled) {
        let mut state = self.state.borrow_mut();
        state.summary.fills += 1;
        let Some(commission) = event.commission else {
            state.failure = Some("TICK_ECONOMIC_COMMISSION_MISSING".into());
            return;
        };
        let total = state
            .summary
            .commissions
            .entry(commission.currency.code.to_string())
            .or_default();
        match total.checked_add(commission.as_decimal()) {
            Some(value) => *total = value,
            None => state.failure = Some("TICK_ECONOMIC_FEE_OVERFLOW".into()),
        }
    }
});
impl DataActor for CohortStrategy {
    fn on_start(&mut self) -> Result<()> {
        if self.observed_l2 {
            for id in self.instruments.keys().copied().collect::<Vec<_>>() {
                self.subscribe_book_deltas(id, BookType::L2_MBP, None, None, true, None);
            }
        } else {
            for bar in self.bars.clone() {
                self.subscribe_bars(bar, None, None);
            }
        }
        Ok(())
    }
    fn on_bar(&mut self, bar: &Bar) -> Result<()> {
        self.execute(bar.instrument_id(), bar.close, None)
    }
    fn on_book_deltas(&mut self, deltas: &OrderBookDeltas) -> Result<()> {
        if !self
            .state
            .borrow()
            .targets
            .contains_key(&deltas.instrument_id)
        {
            return Ok(());
        }
        let book = self.cache().order_book(&deltas.instrument_id);
        if let Some(book) = book {
            if let Some(reference) = book.best_ask_price().or_else(|| book.best_bid_price()) {
                self.execute(deltas.instrument_id, reference, Some(&book))?;
            }
        }
        Ok(())
    }
}
impl CohortStrategy {
    fn execute(
        &mut self,
        id: InstrumentId,
        reference: Price,
        book: Option<&OrderBook>,
    ) -> Result<()> {
        let Some(target) = self.state.borrow_mut().targets.remove(&id) else {
            return Ok(());
        };
        let instrument = self
            .instruments
            .get(&id)
            .context("TICK_ECONOMIC_INSTRUMENT_UNKNOWN")?;
        let current = if self.cash && self.state.borrow().inventory_targets.contains(&id) {
            let currency = instrument
                .base_currency()
                .context("TICK_ECONOMIC_SPOT_BASE_MISSING")?;
            self.cache()
                .account_for_venue(&self.venue)
                .and_then(|a| a.balance_total(Some(currency)))
                .map_or(Decimal::ZERO, |v| v.as_decimal())
        } else {
            self.portfolio().net_position(&id)
        };
        let difference = arithmetic(target.checked_sub(current))?;
        if difference.is_zero() {
            return Ok(());
        }
        let side = if difference > Decimal::ZERO {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let one_sided = book
            .is_some_and(|book| book.best_bid_price().is_none() || book.best_ask_price().is_none());
        if one_sided
            && (current.is_zero() || current.is_sign_negative() == difference.is_sign_negative())
        {
            return Ok(());
        }
        let price = match book {
            Some(book) => if side == OrderSide::Buy {
                book.best_ask_price()
            } else {
                book.best_bid_price()
            }
            .context("TICK_ECONOMIC_EXECUTION_SIDE_MISSING")?,
            None => reference,
        };
        let capacity = self
            .state
            .borrow()
            .capacities
            .get(&id)
            .copied()
            .context("TICK_ECONOMIC_CAPACITY_MISSING")?;
        let mut amount = if self.synthetic_quote_ioc && !self.observed_l2 {
            difference.abs()
        } else {
            difference.abs().min(capacity)
        };
        if one_sided {
            amount = amount.min(current.abs());
        }
        let mut cash_constrained = false;
        if self.cash {
            let currency = if side == OrderSide::Buy {
                instrument.quote_currency()
            } else {
                instrument.base_currency().unwrap()
            };
            let free = self
                .cache()
                .account_for_venue(&self.venue)
                .and_then(|a| a.balance_free(Some(currency)))
                .map_or(Decimal::ZERO, |v| v.as_decimal());
            let affordable = if side == OrderSide::Buy {
                let unit = arithmetic(
                    arithmetic(
                        price
                            .as_decimal()
                            .checked_mul(instrument.multiplier().as_decimal()),
                    )?
                    .checked_mul(arithmetic(Decimal::ONE.checked_add(self.cost_fraction))?),
                )?;
                let budget = free.max(Decimal::ZERO);
                let affordable = arithmetic(budget.checked_div(unit))?;
                if self.observed_l2 {
                    let book = book.context("TICK_ECONOMIC_CASH_BOOK_MISSING")?;
                    affordable_depth_quantity(
                        instrument,
                        book.asks(None)
                            .map(|level| (level.price.value.as_decimal(), level.size_decimal())),
                        amount.min(affordable),
                        Decimal::ZERO,
                        budget,
                        self.cost_fraction,
                    )?
                } else {
                    affordable
                }
            } else {
                free.max(Decimal::ZERO)
            };
            cash_constrained = affordable < amount;
            amount = amount.min(affordable);
        }
        let step = instrument.size_increment().as_decimal();
        amount = arithmetic(
            arithmetic(amount.checked_div(step))?
                .trunc()
                .checked_mul(step),
        )?;
        let quantity = instrument.try_make_qty_from_decimal(amount, Some(true))?;
        if quantity.as_decimal() <= Decimal::ZERO {
            let mut state = self.state.borrow_mut();
            if cash_constrained {
                state.summary.skipped_cash += 1;
            } else {
                state.summary.skipped_minimum += 1;
            }
            return Ok(());
        }
        let notional = arithmetic(
            arithmetic(quantity.as_decimal().checked_mul(price.as_decimal()))?
                .checked_mul(instrument.multiplier().as_decimal()),
        )?;
        if instrument
            .min_quantity()
            .is_some_and(|minimum| quantity < minimum)
            || instrument
                .min_notional()
                .is_some_and(|minimum| notional < minimum.as_decimal())
        {
            self.state.borrow_mut().summary.skipped_minimum += 1;
            return Ok(());
        }
        let order = if one_sided {
            let limit = closing_limit(book.unwrap(), instrument, side, quantity)?;
            self.order().limit(
                id,
                side,
                quantity,
                limit,
                Some(TimeInForce::Ioc),
                None,
                Some(false),
                Some(!self.cash),
                Some(false),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
        } else {
            self.order().market(
                id,
                side,
                quantity,
                Some(TimeInForce::Ioc),
                Some(false),
                None,
                None,
                None,
                None,
                None,
            )
        };
        self.state.borrow_mut().summary.submitted += 1;
        self.submit_order(order, None, None, None)
    }
}

#[derive(Default)]
struct AdjustmentState {
    queue: VecDeque<(u64, Money)>,
    pending: Vec<Money>,
    applied_totals: BTreeMap<String, Decimal>,
    pending_funding: usize,
    pending_nominal: Decimal,
    nominal_settlement_usdc: Decimal,
    applied: u64,
    failure: Option<String>,
}
#[derive(Clone, Default)]
struct Adjustments {
    state: Rc<RefCell<AdjustmentState>>,
    nominal_settlement: bool,
}
impl SimulationModule for Adjustments {
    fn pre_process(&self, _data: &Data) -> Result<()> {
        Ok(())
    }
    fn process(&self, now: UnixNanos, ctx: &ExchangeContext) -> Result<SimulationModuleResult> {
        let mut state = self.state.borrow_mut();
        ensure!(
            state.pending.is_empty(),
            "TICK_ECONOMIC_ADJUSTMENT_UNACKNOWLEDGED"
        );
        let mut batch = Vec::new();
        while state
            .queue
            .front()
            .is_some_and(|(at, _)| *at <= now.as_u64())
        {
            batch.push(state.queue.pop_front().unwrap().1);
        }
        state.pending_funding = batch.len();
        state.pending_nominal = Decimal::ZERO;
        let account = ctx
            .cache
            .account_for_venue(&ctx.venue)
            .context("TICK_ECONOMIC_ADJUSTMENT_ACCOUNT_MISSING")?;
        if self.nominal_settlement {
            let nominal = account
                .balance_total(Some(Currency::USD()))
                .context("TICK_ECONOMIC_NOMINAL_BALANCE_MISSING")?;
            if nominal.as_decimal() != Decimal::ZERO {
                // Explicit quote-unit settlement model; native USD PnL has already rounded to cents.
                batch.push(-nominal);
                batch.push(Money::from_decimal(nominal.as_decimal(), Currency::USDC())?);
                state.pending_nominal = nominal.as_decimal();
            }
        }
        if batch.is_empty() {
            return Ok(SimulationModuleResult::NotReady);
        }
        // Native module batches apply sequentially. Preflight known failures, then make any
        // unexpected partial application terminal instead of returning a successful account.
        let mut totals = BTreeMap::new();
        for amount in &batch {
            let initial = account
                .balance(Some(amount.currency))
                .context("TICK_ECONOMIC_ADJUSTMENT_CURRENCY_MISSING")?;
            let entry = totals
                .entry(amount.currency.code.to_string())
                .or_insert((initial.total, initial.free));
            entry.0 = entry
                .0
                .checked_add(*amount)
                .context("TICK_ECONOMIC_ADJUSTMENT_TOTAL_OVERFLOW")?;
            entry.1 = entry
                .1
                .checked_add(*amount)
                .context("TICK_ECONOMIC_ADJUSTMENT_FREE_OVERFLOW")?;
            ensure!(
                self.nominal_settlement
                    || (entry.0.as_decimal() >= Decimal::ZERO
                        && entry.1.as_decimal() >= Decimal::ZERO),
                "TICK_ECONOMIC_CASH_ADJUSTMENT_OVERDRAFT"
            );
        }
        state.pending = batch.clone();
        Ok(SimulationModuleResult::Completed(batch))
    }
    fn acknowledge(&self, outcomes: &[AccountAdjustmentOutcome]) -> Result<()> {
        let mut state = self.state.borrow_mut();
        ensure!(
            state.pending.len() == outcomes.len(),
            "TICK_ECONOMIC_ADJUSTMENT_ACK_MISMATCH"
        );
        if outcomes
            .iter()
            .any(|outcome| !matches!(outcome, AccountAdjustmentOutcome::Applied))
        {
            state.failure = Some("TICK_ECONOMIC_ADJUSTMENT_FAILED".into());
            anyhow::bail!("TICK_ECONOMIC_ADJUSTMENT_FAILED");
        }
        let pending = std::mem::take(&mut state.pending);
        for amount in pending.into_iter().take(state.pending_funding) {
            let total = state
                .applied_totals
                .entry(amount.currency.code.to_string())
                .or_default();
            *total = arithmetic(total.checked_add(amount.as_decimal()))?;
        }
        state.applied += state.pending_funding as u64;
        state.nominal_settlement_usdc = arithmetic(
            state
                .nominal_settlement_usdc
                .checked_add(state.pending_nominal),
        )?;
        state.pending_funding = 0;
        state.pending_nominal = Decimal::ZERO;
        Ok(())
    }
    fn log_diagnostics(&self) -> Result<()> {
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        *self.state.borrow_mut() = AdjustmentState::default();
        Ok(())
    }
}

/// The caller supplies a prior-cohort target only after its absolute delay/jitter is due.
pub(super) struct NativeMark {
    pub instrument_id: InstrumentId,
    pub mark: f64,
    pub capacity: Decimal,
    pub target_quantity: Option<Decimal>,
}

pub(super) struct NativeRate {
    pub instrument_id: InstrumentId,
    pub rate: Decimal,
}

pub(super) struct NativeBook {
    pub snapshot: OrderBookDeltas,
    pub capacity: Decimal,
    pub target_quantity: Option<Decimal>,
}

pub(super) struct NativeLedger {
    engine: BacktestEngine,
    bus: Rc<RefCell<MessageBus>>,
    venue: Venue,
    instruments: BTreeMap<InstrumentId, InstrumentAny>,
    initial_balances: Vec<Money>,
    state: Rc<RefCell<State>>,
    adjustments: Adjustments,
    last_event: Option<u64>,
    purge_day: Option<u64>,
    synthetic_quote_ioc: bool,
    observed_l2: bool,
}
impl NativeLedger {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instruments: Vec<InstrumentAny>,
        initial_usdc: Decimal,
        margin: bool,
        taker_fee_bps: Decimal,
        extra_cost_bps: Decimal,
        initial_margin_fraction: Decimal,
        maintenance_margin_fraction: Decimal,
        synthetic_quote_ioc: bool,
    ) -> Result<Self> {
        Self::with_execution(
            instruments,
            initial_usdc,
            margin,
            taker_fee_bps,
            extra_cost_bps,
            initial_margin_fraction,
            maintenance_margin_fraction,
            synthetic_quote_ioc,
            false,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new_observed(
        instruments: Vec<InstrumentAny>,
        initial_usdc: Decimal,
        margin: bool,
        taker_fee_bps: Decimal,
        extra_cost_bps: Decimal,
        initial_margin_fraction: Decimal,
        maintenance_margin_fraction: Decimal,
    ) -> Result<Self> {
        Self::with_execution(
            instruments,
            initial_usdc,
            margin,
            taker_fee_bps,
            extra_cost_bps,
            initial_margin_fraction,
            maintenance_margin_fraction,
            false,
            true,
            None,
        )
    }
    pub fn new_conversion(
        instrument: InstrumentAny,
        initial_base: Money,
        taker_fee_bps: Decimal,
        extra_cost_bps: Decimal,
    ) -> Result<Self> {
        ensure!(
            instrument.base_currency() == Some(initial_base.currency)
                && initial_base.as_decimal() > Decimal::ZERO,
            "TICK_ECONOMIC_CONVERSION_BALANCE_INVALID"
        );
        let id = instrument.id();
        let ledger = Self::with_execution(
            vec![instrument],
            Decimal::ZERO,
            false,
            taker_fee_bps,
            extra_cost_bps,
            Decimal::ONE,
            Decimal::new(5, 1),
            false,
            true,
            Some(initial_base),
        )?;
        ledger.inventory_target_instruments(&[id])?;
        Ok(ledger)
    }
    #[allow(clippy::too_many_arguments)]
    fn with_execution(
        instruments: Vec<InstrumentAny>,
        initial_usdc: Decimal,
        margin: bool,
        taker_fee_bps: Decimal,
        extra_cost_bps: Decimal,
        initial_margin_fraction: Decimal,
        maintenance_margin_fraction: Decimal,
        synthetic_quote_ioc: bool,
        observed_l2: bool,
        initial_base: Option<Money>,
    ) -> Result<Self> {
        ensure!(
            (initial_usdc > Decimal::ZERO || initial_base.is_some()) && !instruments.is_empty(),
            "TICK_ECONOMIC_LEDGER_INPUT"
        );
        ensure!(
            taker_fee_bps >= Decimal::ZERO
                && extra_cost_bps >= Decimal::ZERO
                && arithmetic(taker_fee_bps.checked_add(extra_cost_bps))? < Decimal::from(10000)
                && maintenance_margin_fraction > Decimal::ZERO
                && initial_margin_fraction > maintenance_margin_fraction
                && initial_margin_fraction <= Decimal::ONE,
            "TICK_ECONOMIC_NATIVE_SCENARIO_INVALID"
        );
        let venue = instruments[0].venue();
        ensure!(
            instruments.iter().all(|i| i.venue() == venue
                && if margin {
                    matches!(i, InstrumentAny::CryptoPerpetual(_))
                        && !i.is_inverse()
                        && i.settlement_currency() == Currency::USDC()
                } else {
                    matches!(i, InstrumentAny::CurrencyPair(_))
                }),
            "TICK_ECONOMIC_LEDGER_INSTRUMENT"
        );
        let fee_fraction = arithmetic(
            arithmetic(taker_fee_bps.checked_add(extra_cost_bps))?
                .checked_div(Decimal::from(10000)),
        )?;
        let mut engine = BacktestEngine::new(BacktestEngineConfig {
            cache: Some(
                CacheConfig::builder()
                    .bar_capacity(1)
                    .tick_capacity(1)
                    .persist_account_events(false)
                    .build()?,
            ),
            logging: LoggerConfig::builder()
                .bypass_logging(true)
                .is_colored(false)
                .build()?,
            shutdown_on_error: true,
            bypass_logging: true,
            ..Default::default()
        })?;
        let bus = get_message_bus();
        let adjustments = Adjustments {
            nominal_settlement: margin,
            ..Default::default()
        };
        let mut balances = BTreeMap::from([("USDC".to_owned(), (Currency::USDC(), initial_usdc))]);
        for instrument in &instruments {
            for currency in [
                instrument.base_currency(),
                Some(instrument.quote_currency()),
                Some(instrument.settlement_currency()),
            ]
            .into_iter()
            .flatten()
            {
                balances
                    .entry(currency.code.to_string())
                    .or_insert((currency, Decimal::ZERO));
            }
        }
        if let Some(initial) = initial_base {
            balances.insert(
                initial.currency.code.to_string(),
                (initial.currency, initial.as_decimal()),
            );
        }
        let initial_balances = balances
            .into_iter()
            .map(|(_, (currency, value))| {
                Money::from_decimal(value, currency).map_err(anyhow::Error::from)
            })
            .collect::<Result<Vec<_>>>()?;
        engine.add_venue(
            SimulatedVenueConfig::builder()
                .venue(venue)
                .oms_type(OmsType::Netting)
                .account_type(if margin {
                    AccountType::Margin
                } else {
                    AccountType::Cash
                })
                .book_type(if observed_l2 {
                    BookType::L2_MBP
                } else {
                    BookType::L1_MBP
                })
                .allow_cash_borrowing(false)
                .starting_balances(initial_balances.clone())
                .default_leverage(Decimal::ONE)
                .margin_model(MarginModelHandle::new(ScenarioMargin {
                    initial: initial_margin_fraction,
                    maintenance: maintenance_margin_fraction,
                }))
                .modules(vec![SimulationModuleHandle::new(adjustments.clone())])
                .fee_model(FeeModelHandle::new(ScenarioFee(fee_fraction)))
                .fill_model(FillModelHandle::new(DefaultFillModel::new(
                    1.0,
                    0.0,
                    Some(1),
                )?))
                .bar_execution(!synthetic_quote_ioc && !observed_l2)
                .trade_execution(!observed_l2)
                .liquidity_consumption(observed_l2 || !synthetic_quote_ioc)
                .build()?,
        )?;
        let count = instruments.len();
        let instruments: BTreeMap<_, _> = instruments.into_iter().map(|i| (i.id(), i)).collect();
        ensure!(
            instruments.len() == count,
            "TICK_ECONOMIC_DUPLICATE_INSTRUMENT"
        );
        let bars = instruments.values().map(|i| bar_type(i.id())).collect();
        for instrument in instruments.values() {
            engine.add_instrument(instrument)?;
        }
        let state = Rc::new(RefCell::new(State::default()));
        engine.add_strategy(CohortStrategy {
            core: StrategyCore::new(StrategyConfig {
                strategy_id: Some(StrategyId::from(if margin {
                    "COHORT-MARGIN-001"
                } else {
                    "COHORT-CASH-001"
                })),
                ..Default::default()
            }),
            instruments: instruments.clone(),
            bars,
            venue,
            cash: !margin,
            synthetic_quote_ioc,
            observed_l2,
            cost_fraction: fee_fraction,
            state: state.clone(),
        })?;
        Ok(Self {
            engine,
            bus,
            venue,
            instruments,
            initial_balances,
            state,
            adjustments,
            last_event: None,
            purge_day: None,
            synthetic_quote_ioc,
            observed_l2,
        })
    }

    /// FX bridge targets reserve actual quote inventory; alpha targets remain per pair.
    pub fn inventory_target_instruments(&self, ids: &[InstrumentId]) -> Result<()> {
        ensure!(
            ids.iter().all(|id| self
                .instruments
                .get(id)
                .is_some_and(|instrument| matches!(instrument, InstrumentAny::CurrencyPair(_)))),
            "TICK_ECONOMIC_INVENTORY_TARGET_INVALID"
        );
        self.state
            .borrow_mut()
            .inventory_targets
            .extend(ids.iter().copied());
        Ok(())
    }

    pub fn current_quantity(&self, id: InstrumentId) -> Decimal {
        if self.state.borrow().inventory_targets.contains(&id) {
            self.instruments
                .get(&id)
                .and_then(|instrument| instrument.base_currency())
                .and_then(|currency| self.balance(currency))
                .map_or(Decimal::ZERO, |balance| balance.as_decimal())
        } else {
            self.engine
                .kernel()
                .cache
                .borrow()
                .positions_open_refs(Some(&self.venue), Some(&id), None, None, None)
                .into_iter()
                .map(|position| position.signed_decimal_qty())
                .sum()
        }
    }

    /// All marks at a shared event timestamp form one complete streaming batch.
    pub fn step_batch(&mut self, event: u64, rows: &[NativeMark]) -> Result<()> {
        self.step_batch_with_rates(event, rows, &[])
    }

    /// Actual rates are clock carriers with no native interval/next boundary; only the
    /// already-computed scenario adjustment is booked by the module, never native funding.
    pub fn funding_batch(&mut self, event: u64, rows: &[NativeRate]) -> Result<()> {
        self.step_batch_with_rates(event, &[], rows)
    }

    pub fn step_batch_with_rates(
        &mut self,
        event: u64,
        rows: &[NativeMark],
        rates: &[NativeRate],
    ) -> Result<()> {
        ensure!(
            !self.observed_l2 || rows.is_empty(),
            "TICK_ECONOMIC_OBSERVED_REQUIRES_BOOKS"
        );
        ensure!(
            self.last_event.is_none_or(|last| last < event)
                && (!rows.is_empty() || !rates.is_empty()),
            "TICK_ECONOMIC_NATIVE_EVENT_ORDER"
        );
        set_message_bus(self.bus.clone());
        ensure!(
            rows.iter()
                .map(|r| r.instrument_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == rows.len(),
            "TICK_ECONOMIC_DUPLICATE_MARK"
        );
        let mut events = Vec::with_capacity(rows.len() + rates.len());
        ensure!(
            rates
                .iter()
                .map(|r| r.instrument_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == rates.len(),
            "TICK_ECONOMIC_DUPLICATE_RATE"
        );
        for row in rates {
            ensure!(
                self.instruments.contains_key(&row.instrument_id),
                "TICK_ECONOMIC_FUNDING_INSTRUMENT_UNKNOWN"
            );
            events.push(Data::FundingRate(FundingRateUpdate::new(
                row.instrument_id,
                row.rate,
                None,
                None,
                event.into(),
                event.into(),
            )));
        }
        for row in rows {
            let instrument = self
                .instruments
                .get(&row.instrument_id)
                .context("TICK_ECONOMIC_INSTRUMENT_UNKNOWN")?;
            ensure!(
                row.mark.is_finite() && row.mark > 0.0 && row.capacity >= Decimal::ZERO,
                "TICK_ECONOMIC_MARK_INVALID"
            );
            let price = instrument.try_make_price(row.mark)?;
            let step = instrument.size_increment().as_decimal();
            let capacity = arithmetic(
                arithmetic(row.capacity.checked_div(step))?
                    .trunc()
                    .checked_mul(step),
            )?;
            let volume = instrument.try_make_qty_from_decimal(capacity, Some(true))?;
            if let Some(target) = row.target_quantity {
                ensure!(
                    self.state
                        .borrow_mut()
                        .targets
                        .insert(row.instrument_id, target)
                        .is_none(),
                    "TICK_ECONOMIC_DUPLICATE_TARGET"
                );
            }
            self.state
                .borrow_mut()
                .capacities
                .insert(row.instrument_id, row.capacity);
            if self.synthetic_quote_ioc {
                events.push(Data::Quote(QuoteTick::new(
                    row.instrument_id,
                    price,
                    price,
                    volume,
                    volume,
                    event.into(),
                    event.into(),
                )));
            }
            events.push(Data::Bar(Bar::new_checked(
                bar_type(row.instrument_id),
                price,
                price,
                price,
                price,
                volume,
                event.into(),
                event.into(),
            )?));
        }
        self.run_events(event, events)
    }

    pub fn step_books(
        &mut self,
        event: u64,
        rows: Vec<NativeBook>,
        rates: &[NativeRate],
    ) -> Result<()> {
        ensure!(
            self.observed_l2
                && self.last_event.is_none_or(|last| last < event)
                && (!rows.is_empty() || !rates.is_empty()),
            "TICK_ECONOMIC_NATIVE_EVENT_ORDER"
        );
        set_message_bus(self.bus.clone());
        let mut seen = BTreeSet::new();
        let mut events = Vec::new();
        for rate in rates {
            ensure!(
                self.instruments.contains_key(&rate.instrument_id),
                "TICK_ECONOMIC_FUNDING_INSTRUMENT_UNKNOWN"
            );
            events.push(Data::FundingRate(FundingRateUpdate::new(
                rate.instrument_id,
                rate.rate,
                None,
                None,
                event.into(),
                event.into(),
            )));
        }
        for mut row in rows {
            let id = row.snapshot.instrument_id;
            ensure!(
                self.instruments.contains_key(&id)
                    && seen.insert(id)
                    && row.snapshot.ts_init.as_u64() == event
                    && row.capacity >= Decimal::ZERO,
                "TICK_ECONOMIC_BOOK_BATCH_INVALID"
            );
            // Relabel exact raw values, including Clear zeros, without rounding archive levels.
            encode_book(&mut row.snapshot, &self.instruments[&id])?;
            if let Some(target) = row.target_quantity {
                ensure!(
                    self.state.borrow_mut().targets.insert(id, target).is_none(),
                    "TICK_ECONOMIC_DUPLICATE_TARGET"
                );
            }
            let first_book = self
                .state
                .borrow_mut()
                .capacities
                .insert(id, row.capacity)
                .is_none();
            let bid = row
                .snapshot
                .deltas
                .iter()
                .find(|delta| delta.order.side == Some(OrderSide::Buy));
            let ask = row
                .snapshot
                .deltas
                .iter()
                .find(|delta| delta.order.side == Some(OrderSide::Sell));
            if let (Some(bid), Some(ask)) = (bid, ask) {
                // Official market-order risk checks read cached quotes, not the L2 book.
                events.push(Data::Quote(QuoteTick::new(
                    id,
                    bid.order.price,
                    ask.order.price,
                    bid.order.size,
                    ask.order.size,
                    row.snapshot.ts_event,
                    row.snapshot.ts_init,
                )));
            }
            // IOC orders finish within a run; refresh depth before every target and preserve resets.
            if first_book || row.target_quantity.is_some() || bid.is_none() || ask.is_none() {
                events.push(Data::BookDeltas(Box::new(row.snapshot)));
            }
        }
        self.run_events(event, events)?;
        ensure!(
            self.state.borrow().targets.is_empty(),
            "TICK_ECONOMIC_BOOK_TARGET_NOT_CONSUMED"
        );
        Ok(())
    }

    fn run_events(&mut self, event: u64, events: Vec<Data>) -> Result<()> {
        self.engine.add_data(events, None, true, true)?;
        self.engine.run(None, None, None, true)?;
        self.engine.clear_data();
        self.last_event = Some(event);
        self.check()?;
        let day = event / 86_400_000_000_000;
        if self.purge_day != Some(day) {
            let mut cache = self.engine.kernel().cache.borrow_mut();
            cache.purge_closed_orders(event.into(), 0);
            cache.purge_closed_positions(event.into(), 0);
            cache.purge_account_events(event.into(), 0);
            self.purge_day = Some(day);
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn step(
        &mut self,
        event: u64,
        asset: &InstrumentAny,
        mark: f64,
        quantity: f64,
        target_quantity: Option<Decimal>,
    ) -> Result<()> {
        let capacity = Decimal::from_str_exact(&quantity.to_string())
            .context("TICK_ECONOMIC_CAPACITY_RANGE")?;
        self.step_batch(
            event,
            &[NativeMark {
                instrument_id: asset.id(),
                mark,
                capacity,
                target_quantity,
            }],
        )
    }
    pub fn queue_adjustment(&mut self, event: u64, amount: Money) -> Result<()> {
        let mut state = self.adjustments.state.borrow_mut();
        ensure!(
            state.queue.back().is_none_or(|(last, _)| *last <= event),
            "TICK_ECONOMIC_ADJUSTMENT_ORDER"
        );
        state.queue.push_back((event, amount));
        Ok(())
    }
    pub fn balance(&self, currency: Currency) -> Option<Money> {
        let cache = self.engine.kernel().cache.borrow();
        let result = match cache.account_for_venue(&self.venue) {
            Some(account) => account.balance_total(Some(currency)),
            // Configured collateral remains allocated before the first real native event.
            None if self.last_event.is_none() => self
                .initial_balances
                .iter()
                .find(|money| money.currency == currency)
                .copied(),
            None => None,
        };
        result
    }
    pub fn balances(&self) -> Vec<Money> {
        let cache = self.engine.kernel().cache.borrow();
        let result = match cache.account_for_venue(&self.venue) {
            Some(account) => account.balances_total().into_values().collect(),
            None if self.last_event.is_none() => self.initial_balances.clone(),
            None => Vec::new(),
        };
        result
    }
    pub fn open_instrument_ids(&self) -> Vec<InstrumentId> {
        self.engine
            .kernel()
            .cache
            .borrow()
            .positions_open_refs(Some(&self.venue), None, None, None, None)
            .into_iter()
            .map(|position| position.instrument_id)
            .collect()
    }
    pub fn positions(&self) -> Vec<Position> {
        self.engine
            .kernel()
            .cache
            .borrow()
            .positions_open_refs(Some(&self.venue), None, None, None, None)
            .into_iter()
            .map(|position| position.clone_without_events())
            .collect()
    }
    #[cfg(test)]
    pub fn pending_order_count(&self) -> usize {
        let cache = self.engine.kernel().cache.borrow();
        let count = cache.orders_open(None, None, None, None, None).len()
            + cache.orders_inflight(None, None, None, None, None).len();
        count
    }
    pub fn summary(&self) -> NativeSummary {
        let mut summary = self.state.borrow().summary.clone();
        summary.adjustments_applied = self.adjustments.state.borrow().applied;
        summary.adjustment_totals = self.adjustments.state.borrow().applied_totals.clone();
        summary.nominal_settlement_usdc = self.adjustments.state.borrow().nominal_settlement_usdc;
        summary
    }
    #[cfg(test)]
    pub fn orders(&self) -> Vec<OrderAny> {
        self.engine
            .kernel()
            .cache
            .borrow()
            .orders_refs(None, None, None, None, None)
            .into_iter()
            .map(|order| order.cloned())
            .collect()
    }
    fn check(&self) -> Result<()> {
        if let Some(error) = self.state.borrow().failure.as_deref() {
            anyhow::bail!("{error}");
        }
        if let Some(error) = self.adjustments.state.borrow().failure.as_deref() {
            anyhow::bail!("{error}");
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<()> {
        set_message_bus(self.bus.clone());
        if self.last_event.is_some() {
            self.engine.end()?;
        }
        self.check()?;
        ensure!(
            self.adjustments.state.borrow().queue.is_empty()
                && self.adjustments.state.borrow().pending.is_empty(),
            "TICK_ECONOMIC_UNAPPLIED_ADJUSTMENTS"
        );
        Ok(())
    }
}
impl Drop for NativeLedger {
    fn drop(&mut self) {
        set_message_bus(self.bus.clone());
        self.engine.dispose();
    }
}
fn bar_type(id: InstrumentId) -> BarType {
    BarType::new(
        id,
        BarSpecification::new(1, BarAggregation::Millisecond, PriceType::Last),
        AggregationSource::External,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        data::{BookOrder, OrderBookDelta},
        enums::{BookAction, OrderStatus, RecordFlag},
        events::OrderEventAny,
        identifiers::Symbol,
        instruments::{CryptoPerpetual, CurrencyPair},
        orders::Order,
    };

    fn spot() -> InstrumentAny {
        InstrumentAny::CurrencyPair(
            CurrencyPair::builder()
                .instrument_id(InstrumentId::from("BTC-USDC-SPOT.HYPERLIQUID"))
                .raw_symbol(Symbol::new("@test"))
                .base_currency(Currency::BTC())
                .quote_currency(Currency::USDC())
                .price_precision(2)
                .size_precision(4)
                .price_increment(Price::from("0.01"))
                .size_increment(Quantity::from("0.0001"))
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        )
    }
    fn perp() -> InstrumentAny {
        InstrumentAny::CryptoPerpetual(
            CryptoPerpetual::builder()
                .instrument_id(InstrumentId::from("BTC-USD-PERP.HYPERLIQUID"))
                .raw_symbol(Symbol::new("BTC"))
                .base_currency(Currency::BTC())
                .quote_currency(Currency::USD())
                .settlement_currency(Currency::USDC())
                .is_inverse(false)
                .price_precision(2)
                .size_precision(4)
                .price_increment(Price::from("0.01"))
                .size_increment(Quantity::from("0.0001"))
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        )
    }
    fn ledger(instrument: &InstrumentAny, margin: bool, fee: Decimal) -> NativeLedger {
        NativeLedger::new(
            vec![instrument.clone()],
            Decimal::from(1000),
            margin,
            fee,
            Decimal::ZERO,
            Decimal::new(5, 1),
            Decimal::new(25, 2),
            false,
        )
        .unwrap()
    }
    #[test]
    fn original_perp_quote_and_settlement_remain_distinct() {
        let instrument = perp();
        assert_eq!(instrument.quote_currency(), Currency::USD());
        assert_eq!(instrument.settlement_currency(), Currency::USDC());
        // Official 0.63 treats dollar stablecoin quotes as equivalent for quanto detection.
        assert_eq!(instrument.cost_currency(), Currency::USD());
        let margin = ScenarioMargin {
            initial: Decimal::new(5, 1),
            maintenance: Decimal::new(25, 2),
        };
        let value = margin
            .calculate_initial_margin(
                &instrument,
                Quantity::from("1.0000"),
                Price::from("100.00"),
                Decimal::ONE,
                None,
            )
            .unwrap();
        assert_eq!(value.currency, Currency::USDC());
        assert_eq!(value.as_decimal(), Decimal::from(50));
    }
    #[test]
    fn native_cash_buy_moves_inventory_caps_participation_and_deducts_fee_once() {
        let instrument = spot();
        let mut native = ledger(&instrument, false, Decimal::from(100));
        native
            .step(1_000_000_000, &instrument, 100.0, 10.0, None)
            .unwrap();
        native
            .step(2_000_000_000, &instrument, 100.0, 0.5, Some(Decimal::ONE))
            .unwrap();
        native
            .step(3_000_000_000, &instrument, 100.0, 10.0, None)
            .unwrap();
        // 0.67 refreshes same-price BAR liquidity. Its four equal OHLC legs
        // leave 0.5 / 4 BTC at the close when on_bar submits the IOC. The
        // preceding 10-BTC BAR must not provide stale execution liquidity.
        let submitted = Decimal::new(5, 1);
        let filled = submitted / Decimal::from(4);
        let price = Decimal::from(100);
        let commission = filled * price * Decimal::from(100) / Decimal::from(10_000);
        let orders = native.orders();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].quantity().as_decimal(), submitted);
        assert_eq!(orders[0].filled_qty().as_decimal(), filled);
        assert_eq!(orders[0].status(), OrderStatus::Canceled);
        let fills = orders[0]
            .events()
            .into_iter()
            .filter_map(|event| match event {
                OrderEventAny::Filled(fill) => Some(fill),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].last_qty.as_decimal(), filled);
        assert_eq!(fills[0].last_px.as_decimal(), price);
        assert_eq!(fills[0].commission.unwrap().currency, Currency::USDC());
        assert_eq!(fills[0].commission.unwrap().as_decimal(), commission);
        assert_eq!(
            native.balance(Currency::BTC()).unwrap().as_decimal(),
            filled
        );
        assert_eq!(
            native.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(1000) - filled * price - commission
        );
        assert_eq!(native.summary().commissions["USDC"], commission);
        native
            .step(
                4_000_000_000,
                &instrument,
                100.0,
                1000.0,
                Some(Decimal::from(1000)),
            )
            .unwrap();
        native
            .step(5_000_000_000, &instrument, 100.0, 10.0, None)
            .unwrap();
        assert!(native.balance(Currency::USDC()).unwrap().as_decimal() >= Decimal::ZERO);
        assert!(native.balance(Currency::BTC()).unwrap().as_decimal() < Decimal::from(10));
        native.finish().unwrap();
    }
    #[test]
    fn synthetic_quote_ioc_applies_fee_and_funding_once() {
        let instrument = perp();
        let mut native = NativeLedger::new(
            vec![instrument.clone()],
            Decimal::from(1000),
            true,
            Decimal::from(100),
            Decimal::ZERO,
            Decimal::new(5, 1),
            Decimal::new(25, 2),
            true,
        )
        .unwrap();
        native
            .step(1_000_000_000, &instrument, 100.0, 10.0, None)
            .unwrap();
        native
            .step(2_000_000_000, &instrument, 100.0, 0.5, Some(Decimal::ONE))
            .unwrap();
        assert_eq!(
            native.positions()[0].quantity.as_decimal(),
            Decimal::new(5, 1)
        );
        assert_eq!(native.summary().commissions["USDC"], Decimal::new(5, 1));
        native
            .queue_adjustment(3_000_000_000, Money::from("-5 USDC"))
            .unwrap();
        native
            .step_batch_with_rates(
                3_000_000_000,
                &[NativeMark {
                    instrument_id: instrument.id(),
                    mark: 100.0,
                    capacity: Decimal::ONE,
                    target_quantity: Some(Decimal::ZERO),
                }],
                &[NativeRate {
                    instrument_id: instrument.id(),
                    rate: Decimal::new(1, 2),
                }],
            )
            .unwrap();
        assert!(native.positions().is_empty());
        assert_eq!(native.summary().adjustments_applied, 1);
        assert_eq!(
            native.summary().adjustment_totals["USDC"],
            Decimal::from(-5)
        );
        native.finish().unwrap();
    }
    #[test]
    fn open_instrument_ids_match_positions_after_open_and_close() {
        let instrument = perp();
        let mut native = ledger(&instrument, true, Decimal::ZERO);
        for (event, target) in [(1, None), (2, Some(Decimal::ONE)), (3, Some(Decimal::ZERO))] {
            native
                .step(event * 1_000_000_000, &instrument, 100.0, 10.0, target)
                .unwrap();
            let ids = native.open_instrument_ids();
            assert_eq!(
                ids,
                native
                    .positions()
                    .iter()
                    .map(|p| p.instrument_id)
                    .collect::<Vec<_>>()
            );
            assert_eq!(ids.len(), usize::from(event == 2));
        }
        native.finish().unwrap();
    }
    #[test]
    fn native_cash_and_perp_message_buses_stay_isolated_and_adjustments_apply_once() {
        let spot = spot();
        let perp = perp();
        let mut cash = ledger(&spot, false, Decimal::ZERO);
        let mut margin = ledger(&perp, true, Decimal::ZERO);
        cash.step(1_000_000_000, &spot, 100.0, 10.0, None).unwrap();
        margin
            .step(1_000_000_000, &perp, 100.0, 10.0, None)
            .unwrap();
        cash.step(2_000_000_000, &spot, 100.0, 10.0, Some(Decimal::ONE))
            .unwrap();
        margin
            .step(2_000_000_000, &perp, 100.0, 10.0, Some(-Decimal::ONE))
            .unwrap();
        margin
            .queue_adjustment(3_000_000_000, Money::from("-5 USDC"))
            .unwrap();
        cash.step(3_000_000_000, &spot, 100.0, 10.0, None).unwrap();
        margin
            .funding_batch(
                3_000_000_000,
                &[NativeRate {
                    instrument_id: perp.id(),
                    rate: Decimal::new(1, 2),
                }],
            )
            .unwrap();
        margin
            .step(4_000_000_000, &perp, 100.0, 10.0, None)
            .unwrap();
        assert_eq!(
            cash.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(900)
        );
        assert_eq!(
            cash.balance(Currency::BTC()).unwrap().as_decimal(),
            Decimal::ONE
        );
        assert_eq!(
            margin.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(995)
        );
        assert_eq!(margin.positions().len(), 1);
        assert_eq!(margin.summary().adjustments_applied, 1);
        assert_eq!(
            margin.summary().adjustment_totals["USDC"],
            Decimal::from(-5)
        );
        margin
            .step(5_000_000_000, &perp, 101.0, 10.0, Some(Decimal::ZERO))
            .unwrap();
        margin
            .step(6_000_000_000, &perp, 101.0, 10.0, None)
            .unwrap();
        assert!(margin.positions().is_empty());
        assert_eq!(
            margin.balance(Currency::USD()).unwrap().as_decimal(),
            Decimal::ZERO
        );
        assert_eq!(
            margin.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(994)
        );
        assert_eq!(margin.summary().nominal_settlement_usdc, Decimal::from(-1));
        // Nominal settlement moves native PnL, never another funding payment.
        assert_eq!(margin.summary().adjustments_applied, 1);
        assert_eq!(
            margin.summary().adjustment_totals["USDC"],
            Decimal::from(-5)
        );
        cash.finish().unwrap();
        margin.finish().unwrap();
    }
    #[test]
    fn utc_purge_preserves_interleaved_cash_margin_wealth_open_state_and_summaries() {
        const DAY: u64 = 86_400_000_000_000;
        let spot = spot();
        let perp = perp();
        let mut cash = ledger(&spot, false, Decimal::from(100));
        let mut margin = ledger(&perp, true, Decimal::ZERO);
        cash.step(DAY - 3_000_000_000, &spot, 100.0, 10.0, None)
            .unwrap();
        margin
            .step(DAY - 3_000_000_000, &perp, 100.0, 10.0, None)
            .unwrap();
        cash.step(
            DAY - 2_000_000_000,
            &spot,
            100.0,
            10.0,
            Some(Decimal::from(2)),
        )
        .unwrap();
        margin
            .step(
                DAY - 2_000_000_000,
                &perp,
                100.0,
                10.0,
                Some(Decimal::from(-2)),
            )
            .unwrap();
        margin
            .step(DAY - 1_000_000_000, &perp, 110.0, 10.0, Some(-Decimal::ONE))
            .unwrap();
        margin
            .queue_adjustment(DAY + 1_000_000_000, Money::from("-3 USDC"))
            .unwrap();
        cash.step(DAY + 1_000_000_000, &spot, 100.0, 10.0, None)
            .unwrap();
        margin
            .funding_batch(
                DAY + 1_000_000_000,
                &[NativeRate {
                    instrument_id: perp.id(),
                    rate: Decimal::new(1, 2),
                }],
            )
            .unwrap();
        assert_eq!(cash.purge_day, Some(1));
        assert_eq!(margin.purge_day, Some(1));
        assert_eq!(
            cash.engine
                .kernel()
                .cache
                .borrow()
                .orders_closed_count(None, None, None, None, None),
            0
        );
        assert_eq!(
            margin
                .engine
                .kernel()
                .cache
                .borrow()
                .orders_closed_count(None, None, None, None, None),
            0
        );
        assert_eq!(
            cash.balance(Currency::BTC()).unwrap().as_decimal(),
            Decimal::from(2)
        );
        assert_eq!(
            cash.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(798)
        );
        assert_eq!(
            margin.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(987)
        );
        assert_eq!(margin.positions()[0].signed_decimal_qty(), -Decimal::ONE);
        assert_eq!(cash.summary().fills, 1);
        assert_eq!(cash.summary().commissions["USDC"], Decimal::from(2));
        assert_eq!(margin.summary().fills, 2);
        assert_eq!(
            margin.summary().adjustment_totals["USDC"],
            Decimal::from(-3)
        );
        // Closing the survivor proves the purge retained the original short's basis.
        margin
            .step(DAY + 2_000_000_000, &perp, 90.0, 10.0, Some(Decimal::ZERO))
            .unwrap();
        cash.step(DAY + 2_000_000_000, &spot, 100.0, 10.0, None)
            .unwrap();
        assert!(margin.positions().is_empty());
        assert_eq!(
            margin.balance(Currency::USD()).unwrap().as_decimal(),
            Decimal::ZERO
        );
        assert_eq!(
            margin.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(997)
        );
        assert_eq!(margin.summary().fills, 3);
        assert_eq!(margin.summary().adjustments_applied, 1);
        assert_eq!(margin.summary().nominal_settlement_usdc, Decimal::ZERO);
        let wealth = cash.balance(Currency::USDC()).unwrap().as_decimal()
            + cash.balance(Currency::BTC()).unwrap().as_decimal() * Decimal::from(100)
            + margin.balance(Currency::USDC()).unwrap().as_decimal();
        assert_eq!(wealth, Decimal::from(1995));
        cash.finish().unwrap();
        margin.finish().unwrap();
    }

    #[test]
    fn same_base_quote_pairs_keep_positions_separate_while_fx_reserves_follow_wallet() {
        let pair = |id: &str, base: Currency, quote: Currency| {
            InstrumentAny::CurrencyPair(
                CurrencyPair::builder()
                    .instrument_id(InstrumentId::from(id))
                    .raw_symbol(Symbol::new("TEST"))
                    .base_currency(base)
                    .quote_currency(quote)
                    .price_precision(2)
                    .size_precision(4)
                    .price_increment(Price::from("0.01"))
                    .size_increment(Quantity::from("0.0001"))
                    .ts_event(0_u64.into())
                    .ts_init(0_u64.into())
                    .build()
                    .unwrap(),
            )
        };
        let first = pair(
            "BTC-USDC-SPOT.HYPERLIQUID",
            Currency::BTC(),
            Currency::USDC(),
        );
        let second = pair(
            "BTC-USDT-SPOT.HYPERLIQUID",
            Currency::BTC(),
            Currency::USDT(),
        );
        let bridge = pair(
            "USDT-USDC-SPOT.HYPERLIQUID",
            Currency::USDT(),
            Currency::USDC(),
        );
        let mut native = NativeLedger::new(
            vec![first.clone(), second.clone(), bridge.clone()],
            Decimal::from(1000),
            false,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::new(5, 1),
            Decimal::new(25, 2),
            false,
        )
        .unwrap();
        native.inventory_target_instruments(&[bridge.id()]).unwrap();
        let mark = |asset: &InstrumentAny, price: f64, target| NativeMark {
            instrument_id: asset.id(),
            mark: price,
            capacity: Decimal::from(10000),
            target_quantity: target,
        };
        native
            .step_batch(
                1_000_000_000,
                &[
                    mark(&first, 10.0, None),
                    mark(&second, 10.0, None),
                    mark(&bridge, 1.0, None),
                ],
            )
            .unwrap();
        // Quote inventory comes from an actual native FX fill, never an injected balance.
        native
            .step_batch(
                2_000_000_000,
                &[mark(&bridge, 1.0, Some(Decimal::from(100)))],
            )
            .unwrap();
        native
            .step_batch(
                3_000_000_000,
                &[mark(&first, 10.0, Some(Decimal::from(10)))],
            )
            .unwrap();
        native
            .step_batch(
                4_000_000_000,
                &[mark(&second, 10.0, Some(Decimal::from(5)))],
            )
            .unwrap();
        assert_eq!(
            native.balance(Currency::BTC()).unwrap().as_decimal(),
            Decimal::from(15)
        );
        assert_eq!(
            native.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(800)
        );
        assert_eq!(
            native.balance(Currency::USDT()).unwrap().as_decimal(),
            Decimal::from(50)
        );
        let positions = native.positions();
        assert_eq!(
            positions
                .iter()
                .find(|p| p.instrument_id == first.id())
                .unwrap()
                .signed_decimal_qty(),
            Decimal::from(10)
        );
        assert_eq!(
            positions
                .iter()
                .find(|p| p.instrument_id == second.id())
                .unwrap()
                .signed_decimal_qty(),
            Decimal::from(5)
        );
        // Spending USDT changes the wallet, not the original FX pair position.
        native
            .step_batch(
                5_000_000_000,
                &[mark(&bridge, 1.0, Some(Decimal::from(50)))],
            )
            .unwrap();
        assert_eq!(
            native.balance(Currency::USDT()).unwrap().as_decimal(),
            Decimal::from(50)
        );
        assert_eq!(native.summary().fills, 3);
        // Closing one route must not sell the other route's five units.
        native
            .step_batch(6_000_000_000, &[mark(&first, 10.0, Some(Decimal::ZERO))])
            .unwrap();
        assert_eq!(
            native.balance(Currency::BTC()).unwrap().as_decimal(),
            Decimal::from(5)
        );
        assert_eq!(
            native.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(900)
        );
        assert_eq!(
            native
                .positions()
                .iter()
                .find(|p| p.instrument_id == second.id())
                .unwrap()
                .signed_decimal_qty(),
            Decimal::from(5)
        );
        assert_eq!(native.summary().fills, 4);
        native.finish().unwrap();
    }

    #[test]
    fn same_timestamp_must_arrive_in_one_complete_native_batch() {
        let instrument = spot();
        let mut native = ledger(&instrument, false, Decimal::ZERO);
        native
            .step(1_000_000_000, &instrument, 100.0, 10.0, None)
            .unwrap();
        assert!(native
            .step(1_000_000_000, &instrument, 100.0, 10.0, None)
            .is_err());
        native.finish().unwrap();
    }

    fn observed_snapshot(
        instrument: &InstrumentAny,
        at: u64,
        bid: &str,
        bid_size: &str,
        asks: &[(&str, &str)],
    ) -> OrderBookDeltas {
        let id = instrument.id();
        let mut clear = OrderBookDelta::clear(id, 0, (at - 100).into(), at.into());
        clear.order.price = Price::zero(instrument.price_precision());
        clear.order.size = Quantity::zero(instrument.size_precision());
        let mut rows = vec![
            clear,
            OrderBookDelta::new(
                id,
                BookAction::Add,
                BookOrder::new(
                    OrderSide::Buy,
                    Price::from(bid),
                    Quantity::from(bid_size),
                    0,
                ),
                RecordFlag::F_SNAPSHOT as u8,
                0,
                (at - 100).into(),
                at.into(),
            ),
        ];
        for (price, quantity) in asks {
            rows.push(OrderBookDelta::new(
                id,
                BookAction::Add,
                BookOrder::new(
                    OrderSide::Sell,
                    Price::from(*price),
                    Quantity::from(*quantity),
                    0,
                ),
                RecordFlag::F_SNAPSHOT as u8,
                0,
                (at - 100).into(),
                at.into(),
            ));
        }
        rows.last_mut().unwrap().flags |= RecordFlag::F_LAST as u8;
        OrderBookDeltas::new(id, rows)
    }

    #[test]
    fn observed_raw_precision_preserves_btc_and_ubtc_values_and_rejects_loss() {
        let InstrumentAny::CryptoPerpetual(mut btc) = perp() else {
            unreachable!()
        };
        // Original published BTC perp and UBTC spot precision and increments.
        btc.price_precision = 1;
        btc.size_precision = 5;
        btc.price_increment = Price::from("0.1");
        btc.size_increment = Quantity::from("0.00001");
        btc.min_notional = Some(Money::from("10 USD"));
        let InstrumentAny::CurrencyPair(mut ubtc) = spot() else {
            unreachable!()
        };
        ubtc.id = InstrumentId::from("UBTC-USDC-SPOT.HYPERLIQUID");
        ubtc.raw_symbol = Symbol::new("@142");
        ubtc.base_currency = Currency::new(
            "UBTC",
            8,
            0,
            "UBTC",
            nautilus_model::enums::CurrencyType::Crypto,
        );
        ubtc.price_precision = 3;
        ubtc.size_precision = 5;
        ubtc.price_increment = Price::from("0.001");
        ubtc.size_increment = Quantity::from("0.00001");
        ubtc.min_notional = Some(Money::from("10 USDC"));
        for (instrument, bid, ask, invalid_price) in [
            (
                InstrumentAny::CryptoPerpetual(btc),
                "100000.1",
                "100000.2",
                "100000.11",
            ),
            (
                InstrumentAny::CurrencyPair(ubtc),
                "100000.123",
                "100000.124",
                "100000.1234",
            ),
        ] {
            let id = instrument.id();
            let definition = serde_json::to_value(&instrument).unwrap();
            let raw_snapshot = |at| {
                let mut snapshot =
                    observed_snapshot(&instrument, at, bid, "0.00123", &[(ask, "0.00123")]);
                for delta in &mut snapshot.deltas {
                    let precision = nautilus_model::types::fixed::FIXED_PRECISION;
                    delta.order.price =
                        Price::from_raw_checked(delta.order.price.raw(), precision).unwrap();
                    delta.order.size =
                        Quantity::from_raw_checked(delta.order.size.raw(), precision).unwrap();
                }
                snapshot
            };
            let raw = raw_snapshot(1_000_000_000);
            let mut encoded = raw.clone();
            encode_book(&mut encoded, &instrument).unwrap();
            for (before, after) in raw.deltas.iter().zip(&encoded.deltas) {
                assert_eq!(after.order.price.raw(), before.order.price.raw());
                assert_eq!(after.order.size.raw(), before.order.size.raw());
                assert_eq!(after.order.price.precision, instrument.price_precision());
                assert_eq!(after.order.size.precision, instrument.size_precision());
                assert_eq!(after.action, before.action);
                assert_eq!(after.flags, before.flags);
                assert_eq!(after.ts_event, before.ts_event);
                assert_eq!(after.ts_init, before.ts_init);
            }
            let mut ledger = NativeLedger::new_observed(
                vec![instrument.clone()],
                Decimal::from(3000),
                matches!(instrument, InstrumentAny::CryptoPerpetual(_)),
                Decimal::ZERO,
                Decimal::ZERO,
                Decimal::new(5, 1),
                Decimal::new(25, 2),
            )
            .unwrap();
            for (at, target) in [
                (1_000_000_000, None),
                (2_000_000_000, Some(Decimal::new(1, 3))),
            ] {
                ledger
                    .step_books(
                        at,
                        vec![NativeBook {
                            snapshot: raw_snapshot(at),
                            capacity: Decimal::ONE,
                            target_quantity: target,
                        }],
                        &[],
                    )
                    .unwrap();
            }
            assert_eq!(ledger.current_quantity(id), Decimal::new(1, 3));
            assert_eq!(ledger.summary().fills, 1);
            assert_eq!(ledger.summary().rejected, 0);
            assert_eq!(ledger.pending_order_count(), 0);
            assert_eq!(
                serde_json::to_value(&ledger.instruments[&id]).unwrap(),
                definition
            );
            {
                let cache = ledger.engine.kernel().cache.borrow();
                let quote = cache.quote(&id).unwrap();
                assert_eq!(quote.bid_price.precision, instrument.price_precision());
                assert_eq!(quote.ask_size.precision, 5);
                assert_eq!(quote.bid_price.raw(), raw.deltas[1].order.price.raw());
                assert_eq!(quote.ts_event.as_u64(), 2_000_000_000 - 100);
                assert_eq!(quote.ts_init.as_u64(), 2_000_000_000);
            }
            for field in ["PRICE", "SIZE"] {
                let mut invalid = raw_snapshot(3_000_000_000);
                if field == "PRICE" {
                    invalid.deltas[1].order.price = Price::from(invalid_price);
                } else {
                    invalid.deltas[1].order.size = Quantity::from("0.001231");
                }
                let error = ledger
                    .step_books(
                        3_000_000_000,
                        vec![NativeBook {
                            snapshot: invalid,
                            capacity: Decimal::ZERO,
                            target_quantity: None,
                        }],
                        &[],
                    )
                    .unwrap_err();
                assert!(error.to_string().contains(&format!(
                    "TICK_ECONOMIC_BOOK_{field}_PRECISION_UNSUPPORTED: instrument_id={id}"
                )));
                assert_eq!(ledger.last_event, Some(2_000_000_000));
                assert_eq!(ledger.summary().submitted, 1);
            }
            ledger.finish().unwrap();
        }
    }

    #[test]
    fn observed_l2_ioc_uses_ask_depth_then_retains_partial_exit_inventory() {
        let instrument = spot();
        let mut ledger = NativeLedger::new_observed(
            vec![instrument.clone()],
            Decimal::from(1000),
            false,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::new(5, 1),
            Decimal::new(25, 2),
        )
        .unwrap();
        ledger
            .step_books(
                1_000_000_000,
                vec![NativeBook {
                    snapshot: observed_snapshot(
                        &instrument,
                        1_000_000_000,
                        "99.00",
                        "10.0000",
                        &[("101.00", "1.0000"), ("102.00", "1.0000")],
                    ),
                    capacity: Decimal::from(10),
                    target_quantity: Some(Decimal::from(3)),
                }],
                &[],
            )
            .unwrap();
        assert_eq!(
            ledger.balance(Currency::BTC()).unwrap().as_decimal(),
            Decimal::from(2)
        );
        assert_eq!(
            ledger.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(797)
        );
        ledger
            .step_books(
                2_000_000_000,
                vec![NativeBook {
                    snapshot: observed_snapshot(
                        &instrument,
                        2_000_000_000,
                        "98.00",
                        "0.5000",
                        &[("103.00", "1.0000")],
                    ),
                    capacity: Decimal::from(10),
                    target_quantity: Some(Decimal::ZERO),
                }],
                &[],
            )
            .unwrap();
        assert_eq!(
            ledger.balance(Currency::BTC()).unwrap().as_decimal(),
            Decimal::new(15, 1)
        );
        assert_eq!(
            ledger.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(846)
        );
        ledger.finish().unwrap();
    }

    #[test]
    fn observed_idle_quotes_advance_clock_and_next_ioc_uses_fresh_depth() {
        let instrument = spot();
        let id = instrument.id();
        let mut ledger = NativeLedger::new_observed(
            vec![instrument.clone()],
            Decimal::from(1000),
            false,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::new(5, 1),
            Decimal::new(25, 2),
        )
        .unwrap();
        let next_day = 86_401_000_000_000;
        ledger
            .queue_adjustment(next_day, Money::from("1 USDC"))
            .unwrap();
        for (at, bid, asks, target) in [
            (1_000_000_000, "99.00", &[("101.00", "10.0000")][..], None),
            (2_000_000_000, "199.00", &[("201.00", "0.5000")][..], None),
            (
                3_000_000_000,
                "299.00",
                &[("301.00", "0.5000"), ("302.00", "0.2500")][..],
                Some(Decimal::from(3)),
            ),
            (next_day, "399.00", &[("401.00", "10.0000")][..], None),
        ] {
            ledger
                .step_books(
                    at,
                    vec![NativeBook {
                        snapshot: observed_snapshot(&instrument, at, bid, "10.0000", asks),
                        capacity: Decimal::from(10),
                        target_quantity: target,
                    }],
                    &[],
                )
                .unwrap();
            assert_eq!(ledger.pending_order_count(), 0);
            let cache = ledger.engine.kernel().cache.borrow();
            let quote = cache.quote(&id).unwrap();
            assert_eq!(quote.bid_price, Price::from(bid));
            assert_eq!(quote.ask_price, Price::from(asks[0].0));
            assert_eq!(quote.ask_size, Quantity::from(asks[0].1));
            assert_eq!(quote.ts_event.as_u64(), at - 100);
            assert_eq!(quote.ts_init.as_u64(), at);
            if at <= 2_000_000_000 {
                assert_eq!(
                    cache.order_book(&id).unwrap().best_ask_price(),
                    Some(Price::from("101.00"))
                );
                assert_eq!(ledger.summary().submitted, 0);
            }
        }
        assert_eq!(ledger.current_quantity(id), Decimal::new(75, 2));
        assert_eq!(
            ledger.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::from(775)
        );
        assert_eq!(ledger.summary().submitted, 1);
        assert_eq!(ledger.summary().fills, 2);
        assert_eq!(ledger.summary().rejected, 0);
        assert_eq!(ledger.summary().adjustments_applied, 1);
        assert_eq!(ledger.last_event, Some(next_day));
        assert_eq!(ledger.purge_day, Some(1));
        ledger.finish().unwrap();
    }

    #[test]
    fn observed_cash_buy_sizes_crossed_levels_and_fees_within_wallet() {
        for (fee, filled, cash) in [("0", "1.9901", "0.0098"), ("100", "1.9705", "0.009090")] {
            let instrument = spot();
            let mut ledger = NativeLedger::new_observed(
                vec![instrument.clone()],
                Decimal::from(202),
                false,
                fee.parse().unwrap(),
                Decimal::ZERO,
                Decimal::new(5, 1),
                Decimal::new(25, 2),
            )
            .unwrap();
            ledger
                .step_books(
                    1_000_000_000,
                    vec![NativeBook {
                        snapshot: observed_snapshot(
                            &instrument,
                            1_000_000_000,
                            "99.00",
                            "10.0000",
                            &[
                                ("101.00", "1.0000"),
                                ("102.00", "1.0000"),
                                ("100000.00", "10.0000"),
                            ],
                        ),
                        capacity: Decimal::from(10),
                        target_quantity: Some(Decimal::from(2)),
                    }],
                    &[],
                )
                .unwrap();
            assert_eq!(
                ledger.balance(Currency::BTC()).unwrap().as_decimal(),
                filled.parse::<Decimal>().unwrap()
            );
            assert_eq!(
                ledger.balance(Currency::USDC()).unwrap().as_decimal(),
                cash.parse::<Decimal>().unwrap()
            );
            assert_eq!(ledger.summary().rejected, 0);
            ledger.finish().unwrap();
        }
    }

    fn only_side(snapshot: OrderBookDeltas, side: OrderSide) -> OrderBookDeltas {
        let mut rows = snapshot
            .deltas
            .into_iter()
            .filter(|row| row.action == BookAction::Clear || row.order.side == Some(side))
            .collect::<Vec<_>>();
        rows.last_mut().unwrap().flags |= RecordFlag::F_LAST as u8;
        OrderBookDeltas::new(snapshot.instrument_id, rows)
    }

    #[test]
    fn observed_idle_one_sided_and_empty_books_clear_previous_depth() {
        let instrument = spot();
        let id = instrument.id();
        let mut ledger = NativeLedger::new_observed(
            vec![instrument.clone()],
            Decimal::from(1000),
            false,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::new(5, 1),
            Decimal::new(25, 2),
        )
        .unwrap();
        for at in [1_000_000_000, 2_000_000_000] {
            ledger
                .step_books(
                    at,
                    vec![NativeBook {
                        snapshot: observed_snapshot(
                            &instrument,
                            at,
                            "99.00",
                            "10.0000",
                            &[("101.00", "10.0000")],
                        ),
                        capacity: Decimal::ZERO,
                        target_quantity: None,
                    }],
                    &[],
                )
                .unwrap();
        }
        for (index, side) in [Some(OrderSide::Buy), Some(OrderSide::Sell), None]
            .into_iter()
            .enumerate()
        {
            let at = (index as u64 + 3) * 1_000_000_000;
            let mut snapshot = observed_snapshot(
                &instrument,
                at,
                "199.00",
                "10.0000",
                &[("201.00", "10.0000")],
            );
            snapshot
                .deltas
                .retain(|row| row.action == BookAction::Clear || row.order.side == side);
            snapshot.deltas.last_mut().unwrap().flags |= RecordFlag::F_LAST as u8;
            let snapshot = OrderBookDeltas::new(id, snapshot.deltas);
            ledger
                .step_books(
                    at,
                    vec![NativeBook {
                        snapshot,
                        capacity: Decimal::ZERO,
                        target_quantity: None,
                    }],
                    &[],
                )
                .unwrap();
            let cache = ledger.engine.kernel().cache.borrow();
            let book = cache.order_book(&id).unwrap();
            assert_eq!(
                book.best_bid_price(),
                (side == Some(OrderSide::Buy)).then(|| Price::from("199.00"))
            );
            assert_eq!(
                book.best_ask_price(),
                (side == Some(OrderSide::Sell)).then(|| Price::from("201.00"))
            );
            assert_eq!(book.ts_last.as_u64(), at - 100);
            assert_eq!(cache.quote(&id).unwrap().ts_init.as_u64(), 2_000_000_000);
        }
        assert_eq!(ledger.summary().submitted, 0);
        assert_eq!(ledger.summary().fills, 0);
        ledger.finish().unwrap();
    }

    #[test]
    fn observed_one_sided_book_closes_cash_long_and_covers_perp_short() {
        for (instrument, margin) in [(spot(), false), (perp(), true)] {
            let mut ledger = NativeLedger::new_observed(
                vec![instrument.clone()],
                Decimal::from(1000),
                margin,
                Decimal::ZERO,
                Decimal::ZERO,
                Decimal::new(5, 1),
                Decimal::new(25, 2),
            )
            .unwrap();
            ledger
                .step_books(
                    1_000_000_000,
                    vec![NativeBook {
                        snapshot: observed_snapshot(
                            &instrument,
                            1_000_000_000,
                            "99.00",
                            "10.0000",
                            &[("101.00", "10.0000")],
                        ),
                        capacity: Decimal::from(10),
                        target_quantity: Some(if margin { -Decimal::ONE } else { Decimal::ONE }),
                    }],
                    &[],
                )
                .unwrap();
            let side = if margin {
                OrderSide::Sell
            } else {
                OrderSide::Buy
            };
            ledger
                .step_books(
                    2_000_000_000,
                    vec![NativeBook {
                        snapshot: only_side(
                            observed_snapshot(
                                &instrument,
                                2_000_000_000,
                                "98.00",
                                "10.0000",
                                &[("100.00", "0.5000"), ("101.00", "0.5000")],
                            ),
                            side,
                        ),
                        capacity: Decimal::from(10),
                        target_quantity: Some(Decimal::ZERO),
                    }],
                    &[],
                )
                .unwrap();
            assert!(ledger.positions().is_empty());
            assert_eq!(ledger.current_quantity(instrument.id()), Decimal::ZERO);
            assert_eq!(ledger.summary().rejected, 0);
            assert!(ledger
                .orders()
                .iter()
                .any(|order| order.order_type() == nautilus_model::enums::OrderType::Limit));
            ledger.finish().unwrap();
        }
    }

    #[test]
    fn observed_conversion_spends_original_base_and_books_native_fee() {
        let instrument = spot();
        let mut ledger = NativeLedger::new_conversion(
            instrument.clone(),
            Money::from("2 BTC"),
            Decimal::from(100),
            Decimal::ZERO,
        )
        .unwrap();
        ledger
            .step_books(
                1_000_000_000,
                vec![NativeBook {
                    snapshot: observed_snapshot(
                        &instrument,
                        1_000_000_000,
                        "99.00",
                        "1.0000",
                        &[("101.00", "1.0000")],
                    ),
                    capacity: Decimal::new(5, 1),
                    target_quantity: Some(Decimal::ZERO),
                }],
                &[],
            )
            .unwrap();
        assert_eq!(
            ledger.balance(Currency::BTC()).unwrap().as_decimal(),
            Decimal::new(15, 1)
        );
        assert_eq!(
            ledger.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::new(49005, 3)
        );
        assert_eq!(ledger.summary().commissions["USDC"], Decimal::new(495, 3));
        ledger.finish().unwrap();
    }
}
