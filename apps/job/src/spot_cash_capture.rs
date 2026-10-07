//! Read-only official-event capture for the opt-in spot CASH replay path.
//! No real orders, account edits, fees, synthesized snapshots or flow assertions.
use crate::catalog::NativeMarketData;
use anyhow::{Result, anyhow, ensure};
use contracts::{DbCounter, Id, science::NativeSimulationSettingsV1, spot_cash::*};
use domain::spot_cash::SpotCashContext;
use nautilus_backtest::engine::BacktestEngine;
use nautilus_common::{
    clock::Clock,
    msgbus::{self, MessageBus, TypedHandler},
};
use nautilus_model::{
    data::Bar,
    enums::AccountType,
    events::PortfolioSnapshot,
    identifiers::AccountId,
    instruments::{Instrument, InstrumentAny},
};
use serde::Serialize;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

const MAX_RECORDS: usize = 1_000_000;
fn count(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}
fn exact(value: impl ToString) -> Result<contracts::DecimalValue> {
    value.to_string().parse().map_err(anyhow::Error::msg)
}
fn key(bar: &Bar) -> (String, u64, u64) {
    (
        bar.bar_type.to_string(),
        bar.ts_event.as_u64(),
        bar.ts_init.as_u64(),
    )
}
// Native Price/Quantity equality compares numeric raw values, not precision.
// Provenance must preserve the complete native row representation as well.
fn same_bar(left: &Bar, right: &Bar) -> bool {
    left == right
        && left.open.precision == right.open.precision
        && left.high.precision == right.high.precision
        && left.low.precision == right.low.precision
        && left.close.precision == right.close.precision
        && left.volume.precision == right.volume.precision
}

/// Exact boundary facts from the frozen source adapter. Native Bar alone does
/// not declare whether ts_event is a bucket open or close. This type has no
/// deserializer: the trusted preparation integration owns source verification.
#[derive(Clone, Debug, Serialize)]
pub struct ClosedBarSourceRow {
    pub event_time: NativeSpotBarEventTimeV1,
    pub source_row_key: String,
    pub bar_open_ns: DbCounter,
    pub bar_close_ns: DbCounter,
    pub native: Bar,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObservedSpotCashRecord {
    Bar {
        sequence: DbCounter,
        native_clock_ns: DbCounter,
        source: ClosedBarSourceRow,
    },
    Snapshot {
        sequence: DbCounter,
        native_clock_ns: DbCounter,
        /// Exact public native document, including its original UUID4 and clocks.
        native: PortfolioSnapshot,
        frame: NativeSpotValuationFrameV1,
        valuation: ReportCurrencyValuationV1,
    },
}

/// Produced only after the observed native run returned successfully. A generic
/// observer cannot prove no external transfers: this tape is not that receipt.
#[derive(Debug, Serialize)]
pub struct SpotCashObservationTape {
    pub observer_session_id: Id,
    pub native_instance_id: NativeSpotRuntimeIdV1,
    pub native_run_id: NativeSpotRuntimeIdV1,
    pub dataset_revision_id: Id,
    pub session: NativeSpotCashSessionV1,
    pub instruments: Vec<NativeSpotInstrumentV1>,
    pub native_run_receipt: serde_json::Value,
    pub flow_evidence: Option<NativeSpotCashFlowEvidenceV1>,
    pub records: Vec<ObservedSpotCashRecord>,
}

struct State {
    settings: NativeSimulationSettingsV1,
    observer_session_id: Id,
    native_instance_id: NativeSpotRuntimeIdV1,
    dataset_revision_id: Id,
    period_start_ns: DbCounter,
    period_end_ns: DbCounter,
    instruments: Vec<NativeSpotInstrumentV1>,
    venue: String,
    originals: BTreeMap<(String, u64, u64), ClosedBarSourceRow>,
    seen_bars: BTreeSet<(String, u64, u64)>,
    seen_snapshots: BTreeSet<String>,
    latest: BTreeMap<String, NativeSpotPriceV1>,
    context: Option<SpotCashContext>,
    account_id: Option<String>,
    expected_initial_balance: Option<NativeSpotCashMoneyV1>,
    sequence: u64,
    last_clock: Option<u64>,
    capacity: usize,
    records: Vec<ObservedSpotCashRecord>,
    failure: Option<String>,
}

impl State {
    fn new(
        settings: &NativeSimulationSettingsV1,
        instance: NativeSpotRuntimeIdV1,
        dataset: Id,
        market: &NativeMarketData,
        rows: Vec<ClosedBarSourceRow>,
        capacity: usize,
    ) -> Result<Self> {
        domain::spot_cash::account_plan(settings)?;
        ensure!(
            (1..=MAX_RECORDS).contains(&capacity),
            "SPOT_CAPTURE_CAPACITY"
        );
        ensure!(
            !market.series.is_empty() && market.rows > 0 && market.rows <= MAX_RECORDS,
            "SPOT_CAPTURE_MARKET_EMPTY"
        );
        let venue = market.series[0].instrument.venue().to_string();
        let mut instruments = Vec::new();
        let mut expected = BTreeMap::new();
        let mut clocks = Vec::new();
        for series in &market.series {
            ensure!(
                series.instrument_updates.is_empty(),
                "SPOT_CAPTURE_INSTRUMENT_UPDATE_UNSUPPORTED"
            );
            let InstrumentAny::CurrencyPair(pair) = &series.instrument else {
                return Err(anyhow!("SPOT_CAPTURE_NOT_CURRENCY_PAIR"));
            };
            instruments.push(NativeSpotInstrumentV1 {
                instrument_id: series.instrument.id().to_string(),
                venue: series.instrument.venue().to_string(),
                native_kind: "CurrencyPair".into(),
                base_currency: pair.base_currency.to_string(),
                quote_currency: pair.quote_currency.to_string(),
                is_inverse: series.instrument.is_inverse(),
                has_expiration: series.instrument.has_expiration(),
                multiplier: exact(series.instrument.multiplier().as_decimal())?,
            });
            for bar in &series.bars {
                ensure!(
                    bar.bar_type == series.bar_type && expected.insert(key(bar), *bar).is_none(),
                    "SPOT_CAPTURE_DUPLICATE_SOURCE_BAR"
                );
                clocks.push(bar.ts_init.as_u64());
            }
        }
        ensure!(
            expected.len() == market.rows && rows.len() == market.rows,
            "SPOT_CAPTURE_SOURCE_COVERAGE"
        );
        let mut originals = BTreeMap::new();
        let mut source_keys = BTreeSet::new();
        for row in rows {
            ensure!(
                !row.source_row_key.is_empty()
                    && row.source_row_key.len() <= 200
                    && row.source_row_key.trim() == row.source_row_key,
                "SPOT_CAPTURE_SOURCE_ROW_ID"
            );
            ensure!(
                source_keys.insert(row.source_row_key.clone()),
                "SPOT_CAPTURE_SOURCE_ROW_REUSED"
            );
            ensure!(
                row.bar_open_ns < row.bar_close_ns
                    && row.bar_close_ns.get() <= row.native.ts_init.as_u64()
                    && row.native.ts_event.as_u64()
                        == match row.event_time {
                            NativeSpotBarEventTimeV1::Open => row.bar_open_ns.get(),
                            NativeSpotBarEventTimeV1::CloseExclusive => row.bar_close_ns.get(),
                        },
                "SPOT_CAPTURE_CLOSED_BOUNDARY_MISMATCH"
            );
            let row_key = key(&row.native);
            ensure!(
                expected
                    .remove(&row_key)
                    .as_ref()
                    .is_some_and(|original| same_bar(original, &row.native)),
                "SPOT_CAPTURE_SOURCE_BAR_CHANGED"
            );
            originals.insert(row_key, row);
        }
        ensure!(expected.is_empty(), "SPOT_CAPTURE_SOURCE_ROW_MISSING");
        let start = *clocks
            .iter()
            .min()
            .ok_or_else(|| anyhow!("SPOT_CAPTURE_NO_CLOCK"))?;
        let end = *clocks
            .iter()
            .max()
            .ok_or_else(|| anyhow!("SPOT_CAPTURE_NO_CLOCK"))?;
        ensure!(start < end, "SPOT_CAPTURE_ZERO_LENGTH_NATIVE_RUN");
        Ok(Self {
            settings: settings.clone(),
            observer_session_id: Id::new(),
            native_instance_id: instance,
            dataset_revision_id: dataset,
            period_start_ns: count(start)?,
            period_end_ns: count(end)?,
            instruments,
            venue,
            originals,
            seen_bars: BTreeSet::new(),
            seen_snapshots: BTreeSet::new(),
            latest: BTreeMap::new(),
            context: None,
            account_id: None,
            expected_initial_balance: None,
            sequence: 0,
            last_clock: None,
            capacity,
            records: Vec::new(),
            failure: None,
        })
    }

    fn next(&mut self, clock: u64) -> Result<DbCounter> {
        ensure!(
            self.records.len() < self.capacity,
            "SPOT_CAPTURE_RECORD_LIMIT"
        );
        ensure!(
            self.last_clock.is_none_or(|last| last <= clock),
            "SPOT_CAPTURE_CLOCK_REVERSED"
        );
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow!("SPOT_CAPTURE_SEQUENCE_EXHAUSTED"))?;
        self.last_clock = Some(clock);
        count(self.sequence)
    }
    fn record_error(&mut self, result: Result<()>) {
        if let Err(error) = result {
            self.failure.get_or_insert_with(|| format!("{error:#}"));
        }
    }
    fn observe_bar(&mut self, bar: &Bar, clock: u64) -> Result<()> {
        let source_key = key(bar);
        let source = self
            .originals
            .get(&source_key)
            .ok_or_else(|| anyhow!("SPOT_CAPTURE_UNEXPECTED_BAR"))?
            .clone();
        ensure!(
            same_bar(&source.native, bar) && clock == bar.ts_init.as_u64(),
            "SPOT_CAPTURE_BAR_OR_CLOCK_MISMATCH"
        );
        ensure!(
            self.seen_bars.insert(source_key),
            "SPOT_CAPTURE_REPLAYED_BAR"
        );
        let sequence = self.next(clock)?;
        let instrument_id = bar.bar_type.instrument_id().to_string();
        self.latest.insert(
            instrument_id.clone(),
            NativeSpotPriceV1 {
                session_id: self.observer_session_id,
                native_instance_id: self.native_instance_id.clone(),
                dataset_revision_id: self.dataset_revision_id,
                source_row_key: source.source_row_key.clone(),
                instrument_id,
                method: NativeSpotPriceMethodV1::ClosedBarClose,
                observed_sequence: sequence,
                bar_open_ns: source.bar_open_ns,
                event_ns: source.bar_close_ns,
                available_ns: count(bar.ts_init.as_u64())?,
                price: exact(bar.close.as_decimal())?,
            },
        );
        self.records.push(ObservedSpotCashRecord::Bar {
            sequence,
            native_clock_ns: count(clock)?,
            source,
        });
        Ok(())
    }
    fn observe_snapshot(&mut self, snapshot: &PortfolioSnapshot, clock: u64) -> Result<()> {
        self.observe_snapshot_with_origin(
            snapshot,
            clock,
            NativeSpotSnapshotOriginV1::NativePublication,
        )
    }

    fn observe_snapshot_with_origin(
        &mut self,
        snapshot: &PortfolioSnapshot,
        clock: u64,
        origin: NativeSpotSnapshotOriginV1,
    ) -> Result<()> {
        ensure!(
            snapshot.ts_event == snapshot.ts_init && snapshot.ts_init.as_u64() == clock,
            "SPOT_CAPTURE_LATE_OR_BACKDATED_SNAPSHOT"
        );
        ensure!(
            snapshot.account_type == AccountType::Cash && snapshot.base_currency.is_none(),
            "SPOT_CAPTURE_NOT_MULTI_CURRENCY_CASH"
        );
        let account_id = snapshot.account_id.to_string();
        ensure!(
            self.account_id
                .as_ref()
                .is_none_or(|known| known == &account_id),
            "SPOT_CAPTURE_MULTIPLE_ACCOUNTS"
        );
        ensure!(
            self.seen_snapshots.insert(snapshot.event_id.to_string()),
            "SPOT_CAPTURE_SNAPSHOT_REPLAYED"
        );
        let sequence = self.next(clock)?;
        if self.context.is_none() {
            if let Some(expected) = &self.expected_initial_balance {
                ensure!(
                    snapshot.balances.len() == 1,
                    "SPOT_CAPTURE_INITIAL_CURRENCIES_MISMATCH"
                );
                let initial = &snapshot.balances[0];
                ensure!(
                    initial.total.currency.to_string() == expected.currency
                        && exact(initial.total.as_decimal())? == expected.amount
                        && initial.free == initial.total
                        && initial.locked.as_decimal() == rust_decimal::Decimal::ZERO,
                    "SPOT_CAPTURE_INITIAL_CAPITAL_MISMATCH"
                );
            }
            self.context = Some(SpotCashContext::new(
                &self.settings,
                NativeSpotCashSessionV1 {
                    session_id: self.observer_session_id,
                    native_instance_id: self.native_instance_id.clone(),
                    dataset_revision_id: self.dataset_revision_id,
                    account_id: account_id.clone(),
                    venue: self.venue.clone(),
                    period_start_ns: self.period_start_ns,
                    period_end_ns: self.period_end_ns,
                },
                &self.instruments,
            )?);
            self.account_id = Some(account_id.clone());
        }
        let balances = snapshot
            .balances
            .iter()
            .map(|balance| {
                ensure!(
                    balance.total.currency == balance.free.currency
                        && balance.total.currency == balance.locked.currency,
                    "SPOT_CAPTURE_BALANCE_CURRENCY_MISMATCH"
                );
                Ok(NativeSpotCashBalanceV1 {
                    currency: balance.total.currency.to_string(),
                    total: exact(balance.total.as_decimal())?,
                    free: exact(balance.free.as_decimal())?,
                    locked: exact(balance.locked.as_decimal())?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let frame = NativeSpotValuationFrameV1 {
            snapshot: NativeSpotCashSnapshotV1 {
                binding: NativeSpotSnapshotBindingV1 {
                    origin,
                    session_id: self.observer_session_id,
                    native_instance_id: self.native_instance_id.clone(),
                    dataset_revision_id: self.dataset_revision_id,
                    event_id: snapshot
                        .event_id
                        .to_string()
                        .parse()
                        .map_err(anyhow::Error::msg)?,
                    snapshot_sequence: sequence,
                    account_id,
                    venue: self.venue.clone(),
                    asof_ns: count(clock)?,
                },
                account_kind: contracts::science::NativeAccountKind::Cash,
                native_base_currency: None,
                balances_complete: true,
                balances,
            },
            prices: self.latest.values().cloned().collect(),
        };
        let valuation = self
            .context
            .as_ref()
            .expect("created context")
            .value(&frame);
        self.records.push(ObservedSpotCashRecord::Snapshot {
            sequence,
            native_clock_ns: count(clock)?,
            native: snapshot.clone(),
            frame,
            valuation,
        });
        Ok(())
    }
}

/// Owns subscriptions only. The surrounding native runner retains the engine.
pub(crate) struct Capture {
    bus: Rc<RefCell<MessageBus>>,
    active: Rc<Cell<bool>>,
    state: Rc<RefCell<State>>,
    bar_handler: TypedHandler<Bar>,
    snapshot_handler: TypedHandler<PortfolioSnapshot>,
}
fn bound_bus(bus: &Rc<RefCell<MessageBus>>, instance: &NativeSpotRuntimeIdV1) -> Result<()> {
    ensure!(
        Rc::ptr_eq(&msgbus::get_message_bus(), bus),
        "SPOT_CAPTURE_MESSAGE_BUS_CHANGED"
    );
    let current = bus
        .try_borrow()
        .map_err(|_| anyhow!("SPOT_CAPTURE_MESSAGE_BUS_BUSY"))?;
    ensure!(
        current.instance_id.to_string() == instance.to_string(),
        "SPOT_CAPTURE_MESSAGE_BUS_INSTANCE_MISMATCH"
    );
    Ok(())
}

impl Capture {
    fn attach(state: State, clock: Rc<RefCell<dyn Clock>>) -> Result<Self> {
        let bus = msgbus::get_message_bus();
        bound_bus(&bus, &state.native_instance_id)?;
        let active = Rc::new(Cell::new(true));
        let state = Rc::new(RefCell::new(state));
        let bar_state = Rc::downgrade(&state);
        let bar_clock = clock.clone();
        let bar_bus = Rc::downgrade(&bus);
        let bar_active = active.clone();
        let bar_handler = TypedHandler::from(move |bar: &Bar| {
            if !bar_active.get() {
                return;
            }
            let (Some(state), Some(bus)) = (bar_state.upgrade(), bar_bus.upgrade()) else {
                return;
            };
            let mut state = state.borrow_mut();
            if state.failure.is_some() {
                return;
            }
            let result = bound_bus(&bus, &state.native_instance_id).and_then(|()| {
                bar_clock
                    .try_borrow()
                    .map_err(|_| anyhow!("SPOT_CAPTURE_CLOCK_BUSY"))
                    .and_then(|clock| state.observe_bar(bar, clock.timestamp_ns().as_u64()))
            });
            state.record_error(result);
        });
        let snapshot_state = Rc::downgrade(&state);
        let snapshot_bus = Rc::downgrade(&bus);
        let snapshot_active = active.clone();
        let snapshot_handler = TypedHandler::from(move |snapshot: &PortfolioSnapshot| {
            if !snapshot_active.get() {
                return;
            }
            let (Some(state), Some(bus)) = (snapshot_state.upgrade(), snapshot_bus.upgrade())
            else {
                return;
            };
            let mut state = state.borrow_mut();
            if state.failure.is_some() {
                return;
            }
            let result = bound_bus(&bus, &state.native_instance_id).and_then(|()| {
                clock
                    .try_borrow()
                    .map_err(|_| anyhow!("SPOT_CAPTURE_CLOCK_BUSY"))
                    .and_then(|clock| {
                        state.observe_snapshot(snapshot, clock.timestamp_ns().as_u64())
                    })
            });
            state.record_error(result);
        });
        // Official portfolio bars use priority 10. This records actual callback
        // order before portfolio work caused by the same bar publication.
        msgbus::subscribe_bars("data.bars.*EXTERNAL".into(), bar_handler.clone(), Some(20));
        msgbus::subscribe_portfolio_snapshot(
            "events.portfolio.*".into(),
            snapshot_handler.clone(),
            Some(20),
        );
        Ok(Self {
            bus,
            active,
            state,
            bar_handler,
            snapshot_handler,
        })
    }
    fn ensure_attached_bus(&self) -> Result<()> {
        bound_bus(&self.bus, &self.state.borrow().native_instance_id)
    }
}

/// A decision captures an actual official snapshot, never a cached prior total.
#[derive(Clone, Debug)]
pub(crate) struct SpotCashDecisionSnapshot {
    pub asof_ns: DbCounter,
    pub total: contracts::DecimalValue,
    pub balances: Vec<NativeSpotCashBalanceV1>,
    pub prices: Vec<NativeSpotPriceV1>,
}

#[derive(Clone)]
pub(crate) struct SpotCashCaptureHandle {
    state: Rc<RefCell<State>>,
    bus: Rc<RefCell<MessageBus>>,
    active: Rc<Cell<bool>>,
    portfolio: Rc<RefCell<nautilus_portfolio::portfolio::Portfolio>>,
    clock: Rc<RefCell<dyn Clock>>,
}
impl std::fmt::Debug for SpotCashCaptureHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SpotCashCaptureHandle")
    }
}
impl SpotCashCaptureHandle {
    pub(crate) fn decision_snapshot(&self) -> Result<SpotCashDecisionSnapshot> {
        ensure!(self.active.get(), "SPOT_CAPTURE_ALREADY_DETACHED");
        let account_id = {
            let state = self.state.borrow();
            bound_bus(&self.bus, &state.native_instance_id)?;
            ensure!(state.failure.is_none(), "SPOT_CAPTURE_PRIOR_FAILURE");
            state
                .account_id
                .clone()
                .ok_or_else(|| anyhow!("SPOT_CAPTURE_INITIAL_SNAPSHOT_MISSING"))?
        };
        let native = self
            .portfolio
            .try_borrow_mut()
            .map_err(|_| anyhow!("SPOT_CAPTURE_PORTFOLIO_BUSY"))?
            .build_snapshot(&AccountId::new_checked(&account_id)?)
            .ok_or_else(|| anyhow!("SPOT_CAPTURE_ACCOUNT_SNAPSHOT_UNAVAILABLE"))?;
        let now = self
            .clock
            .try_borrow()
            .map_err(|_| anyhow!("SPOT_CAPTURE_CLOCK_BUSY"))?
            .timestamp_ns()
            .as_u64();
        let mut state = self.state.borrow_mut();
        state.observe_snapshot_with_origin(
            &native,
            now,
            NativeSpotSnapshotOriginV1::DecisionBuild,
        )?;
        let Some(ObservedSpotCashRecord::Snapshot {
            frame, valuation, ..
        }) = state.records.last()
        else {
            return Err(anyhow!("SPOT_CAPTURE_DECISION_RECORD_MISSING"));
        };
        let ReportCurrencyValuationOutcomeV1::Complete { total, .. } = &valuation.outcome else {
            return Err(anyhow!(
                "SPOT_DECISION_VALUATION_UNAVAILABLE: {:?}",
                valuation.outcome
            ));
        };
        Ok(SpotCashDecisionSnapshot {
            asof_ns: frame.snapshot.binding.asof_ns,
            total: total.clone(),
            balances: frame.snapshot.balances.clone(),
            prices: frame.prices.clone(),
        })
    }
}

impl Capture {
    pub(crate) fn prepare(
        engine: &BacktestEngine,
        settings: &NativeSimulationSettingsV1,
        dataset_revision_id: Id,
        market: &NativeMarketData,
        closed_rows: Vec<ClosedBarSourceRow>,
        maximum_records: usize,
        horizon_end_ns: Option<DbCounter>,
    ) -> Result<Self> {
        ensure!(
            engine.iteration() == 0 && engine.run_id().is_none(),
            "SPOT_CAPTURE_REQUIRES_FRESH_NATIVE_RUN"
        );
        ensure!(
            engine
                .kernel()
                .cache
                .borrow()
                .accounts_all_owned()
                .is_empty(),
            "SPOT_CAPTURE_RESTORED_ACCOUNT_UNSUPPORTED"
        );
        ensure!(
            engine.list_venues().len() == 1
                && engine.list_venues()[0]
                    == market
                        .series
                        .first()
                        .ok_or_else(|| anyhow!("SPOT_CAPTURE_MARKET_EMPTY"))?
                        .instrument
                        .venue(),
            "SPOT_CAPTURE_VENUE_MISMATCH"
        );
        validate_engine_instruments(engine, market)?;
        let instance = engine
            .instance_id()
            .to_string()
            .parse()
            .map_err(anyhow::Error::msg)?;
        let mut state = State::new(
            settings,
            instance,
            dataset_revision_id,
            market,
            closed_rows,
            maximum_records,
        )?;
        if let Some(end) = horizon_end_ns {
            ensure!(
                end >= state.period_end_ns,
                "SPOT_CAPTURE_HORIZON_BEFORE_DATA"
            );
            state.period_end_ns = end;
        }
        state.expected_initial_balance = domain::spot_cash::account_plan(settings)?
            .starting_balances
            .into_iter()
            .next();
        Self::attach(state, engine.kernel().clock())
    }
    pub(crate) fn handle(&self, engine: &BacktestEngine) -> Result<SpotCashCaptureHandle> {
        ensure!(
            engine.instance_id().to_string() == self.state.borrow().native_instance_id.to_string(),
            "SPOT_CAPTURE_ENGINE_CHANGED"
        );
        self.ensure_attached_bus()?;
        Ok(SpotCashCaptureHandle {
            state: self.state.clone(),
            bus: self.bus.clone(),
            active: self.active.clone(),
            portfolio: engine.kernel().portfolio.clone(),
            clock: engine.kernel().clock(),
        })
    }
    pub(crate) fn finish(self, engine: &BacktestEngine) -> Result<SpotCashObservationTape> {
        let bus_result = self.ensure_attached_bus();
        let state = self.state.clone();
        drop(self);
        bus_result?;
        let native = engine.get_result();
        let mut state = state.borrow_mut();
        if let Some(error) = state.failure.take() {
            return Err(anyhow!(error).context("SPOT_NATIVE_OBSERVATION_FAILED"));
        }
        ensure!(
            state.seen_bars.len() == state.originals.len() && !state.seen_snapshots.is_empty(),
            "SPOT_CAPTURE_NATIVE_STREAM_INCOMPLETE"
        );
        ensure!(
            native.instance_id.to_string() == state.native_instance_id.to_string(),
            "SPOT_CAPTURE_ENGINE_CHANGED"
        );
        let run_id = native
            .run_id
            .ok_or_else(|| anyhow!("SPOT_CAPTURE_NATIVE_RUN_ID_MISSING"))?
            .to_string()
            .parse()
            .map_err(anyhow::Error::msg)?;
        let start = count(
            native
                .backtest_start
                .ok_or_else(|| anyhow!("SPOT_CAPTURE_NATIVE_START_MISSING"))?
                .as_u64(),
        )?;
        let end = count(
            native
                .backtest_end
                .ok_or_else(|| anyhow!("SPOT_CAPTURE_NATIVE_END_MISSING"))?
                .as_u64(),
        )?;
        ensure!(
            start == state.period_start_ns && end <= state.period_end_ns && start < end,
            "SPOT_CAPTURE_NATIVE_WINDOW_MISMATCH"
        );
        let session = NativeSpotCashSessionV1 {
            session_id: state.observer_session_id,
            native_instance_id: state.native_instance_id,
            dataset_revision_id: state.dataset_revision_id,
            account_id: state
                .account_id
                .clone()
                .ok_or_else(|| anyhow!("SPOT_CAPTURE_ACCOUNT_MISSING"))?,
            venue: state.venue.clone(),
            period_start_ns: start,
            period_end_ns: end,
        };
        let context = SpotCashContext::new(&state.settings, session.clone(), &state.instruments)?;
        for record in &mut state.records {
            if let ObservedSpotCashRecord::Snapshot {
                frame, valuation, ..
            } = record
            {
                *valuation = context.value(frame);
            }
        }
        Ok(SpotCashObservationTape {
            observer_session_id: state.observer_session_id,
            native_instance_id: state.native_instance_id,
            native_run_id: run_id,
            dataset_revision_id: state.dataset_revision_id,
            session,
            instruments: state.instruments.clone(),
            native_run_receipt: serde_json::to_value(native)?,
            flow_evidence: None,
            records: std::mem::take(&mut state.records),
        })
    }
}

// Official typed unsubscribe functions target the thread-local bus. Restore its
// current owner even when an error/panic unwinds cleanup of the attached bus.
struct RestoreMessageBus(Rc<RefCell<MessageBus>>);
impl Drop for RestoreMessageBus {
    fn drop(&mut self) {
        msgbus::set_message_bus(self.0.clone());
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.active.set(false);
        let current_bus = msgbus::get_message_bus();
        if self.bus.try_borrow_mut().is_err() || current_bus.try_borrow().is_err() {
            // Weak callback captures and the inactive token prevent recording or
            // retaining the session even if a caller holds the bus during Drop.
            if let Ok(mut state) = self.state.try_borrow_mut() {
                state
                    .failure
                    .get_or_insert_with(|| "SPOT_CAPTURE_CLEANUP_BUS_BUSY".into());
            }
            return;
        }
        let restore = RestoreMessageBus(current_bus);
        msgbus::set_message_bus(self.bus.clone());
        msgbus::unsubscribe_bars("data.bars.*EXTERNAL".into(), &self.bar_handler);
        msgbus::unsubscribe_portfolio_snapshot("events.portfolio.*".into(), &self.snapshot_handler);
        drop(restore);
    }
}

fn validate_engine_instruments(engine: &BacktestEngine, market: &NativeMarketData) -> Result<()> {
    let cache = engine.kernel().cache.borrow();
    ensure!(
        cache.instrument_ids(None).len() == market.series.len(),
        "SPOT_CAPTURE_ENGINE_INSTRUMENT_SCOPE_MISMATCH"
    );
    for series in &market.series {
        let actual = cache
            .instrument(&series.instrument.id())
            .ok_or_else(|| anyhow!("SPOT_CAPTURE_ENGINE_INSTRUMENT_MISSING"))?;
        // Complete original enum/definition, including currency, multipliers,
        // precision and original clocks. Numeric Instrument equality is not enough.
        ensure!(
            serde_json::to_value(&actual)? == serde_json::to_value(&series.instrument)?,
            "SPOT_CAPTURE_ENGINE_INSTRUMENT_CHANGED"
        );
    }
    Ok(())
}

/// Runs one already-configured, fresh native backtest with read-only observation.
/// This does not assert that an arbitrary configured engine has no cash-flow
/// modules/clients. Only a future owned engine constructor can establish that.
/// The caller must dispose the engine, just as for the official run API.
pub fn record_native_run(
    engine: &mut BacktestEngine,
    settings: &NativeSimulationSettingsV1,
    dataset_revision_id: Id,
    market: &NativeMarketData,
    closed_rows: Vec<ClosedBarSourceRow>,
    maximum_records: usize,
) -> Result<SpotCashObservationTape> {
    let capture = Capture::prepare(
        engine,
        settings,
        dataset_revision_id,
        market,
        closed_rows,
        maximum_records,
        None,
    )?;
    let result = engine.run(None, None, None, false);
    match result {
        Ok(()) => capture.finish(engine),
        Err(error) => {
            drop(capture);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests;
