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
    data::{Bar, BarSpecification, BarType, Data, FundingRateUpdate},
    enums::{
        AccountType, AggregationSource, BarAggregation, BookType, OmsType, OrderSide, PriceType,
        TimeInForce,
    },
    events::{OrderDenied, OrderFilled, OrderRejected},
    identifiers::{InstrumentId, StrategyId, Venue},
    instruments::{Instrument, InstrumentAny},
    orders::OrderAny,
    position::Position,
    types::{Currency, Money, Price, Quantity},
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
        for bar in self.bars.clone() {
            self.subscribe_bars(bar, None, None);
        }
        Ok(())
    }
    fn on_bar(&mut self, bar: &Bar) -> Result<()> {
        let id = bar.instrument_id();
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
        let capacity = self
            .state
            .borrow()
            .capacities
            .get(&id)
            .copied()
            .context("TICK_ECONOMIC_CAPACITY_MISSING")?;
        let mut amount = difference.abs().min(capacity);
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
                        bar.close
                            .as_decimal()
                            .checked_mul(instrument.multiplier().as_decimal()),
                    )?
                    .checked_mul(arithmetic(Decimal::ONE.checked_add(self.cost_fraction))?),
                )?;
                arithmetic(free.max(Decimal::ZERO).checked_div(unit))?
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
            arithmetic(quantity.as_decimal().checked_mul(bar.close.as_decimal()))?
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
        let order = self.order().market(
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
        );
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
    ) -> Result<Self> {
        ensure!(
            initial_usdc > Decimal::ZERO && !instruments.is_empty(),
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
                .book_type(BookType::L1_MBP)
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
                .bar_execution(true)
                .liquidity_consumption(true)
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
    pub fn summary(&self) -> NativeSummary {
        let mut summary = self.state.borrow().summary.clone();
        summary.adjustments_applied = self.adjustments.state.borrow().applied;
        summary.adjustment_totals = self.adjustments.state.borrow().applied_totals.clone();
        summary.nominal_settlement_usdc = self.adjustments.state.borrow().nominal_settlement_usdc;
        summary
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
        BarSpecification::new(1, BarAggregation::Tick, PriceType::Last),
        AggregationSource::External,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        identifiers::Symbol,
        instruments::{CryptoPerpetual, CurrencyPair},
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
        assert_eq!(
            native.balance(Currency::BTC()).unwrap().as_decimal(),
            Decimal::new(5, 1)
        );
        assert_eq!(
            native.balance(Currency::USDC()).unwrap().as_decimal(),
            Decimal::new(9495, 1)
        );
        assert_eq!(native.summary().commissions["USDC"], Decimal::new(5, 1));
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
}
