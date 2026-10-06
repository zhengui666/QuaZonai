//! One bounded native Paper session driven by retained public Polymarket ticks.
//!
//! Host this on an independent native engine thread/process, never the public
//! LiveNode event thread. The caller retains original source frames and validates
//! the original Q claim; this module has no delivery or financial authority.
//! It preserves one official engine/account and delegates all matching and fees.
use anyhow::{anyhow, ensure, Result};
use bigdecimal::ToPrimitive;
use contracts::{
    portfolio::PortfolioConstraintsV1,
    science::{NativeAccountKind, NativeSimulationSettingsV1, NativeTargetPointV1},
    strategy_portfolio::FreshPaperCashV1,
};
use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
};
use nautilus_common::{
    component::component_state, enums::ComponentState, logging::logger::LoggerConfig,
};
use nautilus_execution::models::{
    fill::{DefaultFillModel, FillModelHandle},
    latency::{LatencyModelHandle, StaticLatencyModel},
};
use nautilus_model::{
    data::{BarType, Data},
    enums::{AccountType, BookType, OmsType},
    identifiers::{AccountId, ClientId, InstrumentId, StrategyId, TraderId, Venue},
    instruments::{Instrument, InstrumentAny},
    types::{Currency, Money},
};
use nautilus_portfolio::config::PortfolioConfig;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, str::FromStr};

use crate::{
    catalog::{NativeBarSeries, NativeMarketData},
    simulation::{streaming_paper_target_strategy, ReplayStatus},
};

const MAX_TIMESTAMP_EVENTS: usize = 65_536;

/// Retain the final timestamp until a later native timestamp or source EOF proves
/// that it is complete. Late/equal-after-flush input never becomes a new batch.
#[derive(Default)]
struct TimestampBatch {
    pending: Vec<Data>,
    pending_at: Option<u64>,
    flushed_through: Option<u64>,
}

fn tick_identity(data: &Data) -> Result<(InstrumentId, u64, u64)> {
    match data {
        Data::Quote(value) => Ok((
            value.instrument_id,
            value.ts_event.as_u64(),
            value.ts_init.as_u64(),
        )),
        Data::Trade(value) => Ok((
            value.instrument_id,
            value.ts_event.as_u64(),
            value.ts_init.as_u64(),
        )),
        _ => Err(anyhow!("PAPER_STREAM_REQUIRES_ORIGINAL_QUOTE_OR_TRADE")),
    }
}

impl TimestampBatch {
    fn push(&mut self, data: Data) -> Result<Option<Vec<Data>>> {
        let (_, _, at) = tick_identity(&data)?;
        ensure!(
            self.flushed_through.is_none_or(|previous| at > previous)
                && self.pending_at.is_none_or(|previous| at >= previous),
            "PAPER_STREAM_LATE_NATIVE_TIMESTAMP"
        );
        let complete = if self.pending_at.is_some_and(|previous| at > previous) {
            self.flushed_through = self.pending_at;
            Some(std::mem::take(&mut self.pending))
        } else {
            None
        };
        ensure!(
            self.pending.len() < MAX_TIMESTAMP_EVENTS,
            "PAPER_STREAM_TIMESTAMP_CAPACITY"
        );
        self.pending_at = Some(at);
        self.pending.push(data);
        Ok(complete)
    }

    fn source_ended(&mut self) -> Vec<Data> {
        self.flushed_through = self.pending_at.take();
        std::mem::take(&mut self.pending)
    }
}

/// Ignore only record clocks when comparing terms. Original refresh frames remain
/// in the caller's source log. All economic fields and the original fee schedule
/// must match; do not update an in-use matching engine behind an accepted target.
fn economic_definition(instrument: &InstrumentAny) -> Result<Value> {
    let mut payload = crate::prediction::native_payload(instrument)?;
    domain::prediction::planning_fee(&payload)?;
    let value = payload
        .as_object_mut()
        .ok_or_else(|| anyhow!("PAPER_STREAM_BINARY_DEFINITION"))?;
    value.remove("ts_event");
    value.remove("ts_init");
    Ok(payload)
}

pub struct PolymarketStreamingPaper {
    engine: BacktestEngine,
    replay: Rc<RefCell<ReplayStatus>>,
    definitions: BTreeMap<InstrumentId, (InstrumentAny, Value)>,
    batches: TimestampBatch,
    failure: Option<String>,
    original_frames: u64,
    completed_batches: u64,
    native_started: bool,
    quote_covered_through: Option<u64>,
}

impl PolymarketStreamingPaper {
    /// `account_start`, settings and target must come from the same original
    /// accepted Q package. The outer consumer owns that existing binding check.
    pub fn new(
        account_start: &FreshPaperCashV1,
        settings: &NativeSimulationSettingsV1,
        constraints: &PortfolioConstraintsV1,
        target: NativeTargetPointV1,
        instruments: Vec<InstrumentAny>,
        bar_types: Vec<BarType>,
    ) -> Result<Self> {
        ensure!(
            account_start.account_id == "POLYMARKET-001"
                && account_start.base_currency == settings.base_currency
                && account_start.starting_capital == settings.starting_capital
                && settings.account_kind == NativeAccountKind::Cash
                && settings.leverage.as_decimal() == &bigdecimal::BigDecimal::from(1)
                && domain::prediction::uses_native_fee(&settings.fee_model),
            "PAPER_STREAM_ORIGINAL_CASH_ACCOUNT_BINDING"
        );
        ensure!(
            !instruments.is_empty() && instruments.len() == bar_types.len(),
            "PAPER_STREAM_INSTRUMENT_SCOPE"
        );
        let metadata = NativeMarketData {
            series: instruments
                .iter()
                .zip(&bar_types)
                .map(|(instrument, bar_type)| NativeBarSeries {
                    instrument: instrument.clone(),
                    instrument_updates: Vec::new(),
                    bar_type: *bar_type,
                    bars: Vec::new(),
                })
                .collect(),
            rows: 0,
        };
        // Existing native/account/currency and original fee-schedule predicates.
        crate::simulation::execution_market(&metadata, settings)?;
        crate::prediction::validate_market(&metadata, settings)?;
        crate::prediction::target_window(
            &instruments,
            target.asof_ns.get(),
            target.valid_until_ns.get(),
        )?;
        let mut definitions = BTreeMap::new();
        for instrument in &instruments {
            ensure!(
                instrument.venue() == Venue::from("POLYMARKET"),
                "PAPER_STREAM_VENUE"
            );
            ensure!(
                definitions
                    .insert(
                        instrument.id(),
                        (instrument.clone(), economic_definition(instrument)?)
                    )
                    .is_none(),
                "PAPER_STREAM_DUPLICATE_INSTRUMENT"
            );
        }
        let (fill, latency) = domain::portfolio::simulation_models(settings)?;
        let mut config = BacktestEngineConfig {
            trader_id: TraderId::new_checked(&account_start.trader_id)?,
            load_state: false,
            save_state: false,
            portfolio: Some(
                PortfolioConfig::builder()
                    .snapshot_interval_ms(u64::from(settings.snapshot_interval_ms))
                    .build()?,
            ),
            logging: LoggerConfig::builder()
                .bypass_logging(true)
                .is_colored(false)
                .build()?,
            shutdown_on_error: true,
            bypass_logging: true,
            ..Default::default()
        };
        let data_config = config.data_engine.get_or_insert_default();
        data_config.time_bars_build_with_no_updates = false;
        data_config.time_bars_skip_first_non_full_bar = true;
        data_config.time_bars_timestamp_on_close = true;
        data_config.validate_data_sequence = true;
        let mut engine = BacktestEngine::new(config)?;
        let built = (|| -> Result<Rc<RefCell<ReplayStatus>>> {
            engine.add_venue(
                SimulatedVenueConfig::builder()
                    .venue(Venue::from("POLYMARKET"))
                    .oms_type(OmsType::Netting)
                    .account_type(AccountType::Cash)
                    .book_type(BookType::L1_MBP)
                    .base_currency(Currency::from_str(&settings.base_currency)?)
                    .starting_balances(vec![Money::from_str(&format!(
                        "{} {}",
                        settings.starting_capital.as_decimal().to_plain_string(),
                        settings.base_currency
                    ))
                    .map_err(anyhow::Error::msg)?])
                    .default_leverage(rust_decimal::Decimal::ONE)
                    .fee_model(crate::prediction::fee_model(settings))
                    .fill_model(FillModelHandle::new(DefaultFillModel::new(
                        fill.prob_fill_on_limit
                            .as_decimal()
                            .to_f64()
                            .ok_or_else(|| anyhow!("PAPER_STREAM_FILL_RANGE"))?,
                        fill.prob_slippage
                            .as_decimal()
                            .to_f64()
                            .ok_or_else(|| anyhow!("PAPER_STREAM_FILL_RANGE"))?,
                        Some(fill.random_seed.get()),
                    )?))
                    .latency_model(LatencyModelHandle::new(StaticLatencyModel::new(
                        latency.base_latency_ns.get().into(),
                        latency.insert_latency_ns.get().into(),
                        latency.update_latency_ns.get().into(),
                        latency.cancel_latency_ns.get().into(),
                    )))
                    // Match against actual quotes. Internal bars only schedule targets.
                    .bar_execution(false)
                    .trade_execution(false)
                    .liquidity_consumption(true)
                    .build()?,
            )?;
            for instrument in &instruments {
                engine.add_instrument(instrument)?;
            }
            let (mut strategy, replay) = streaming_paper_target_strategy(
                settings.clone(),
                instruments,
                bar_types,
                target,
                StrategyId::from("QZ-PAPER-001"),
                ClientId::from("POLYMARKET"),
            )?;
            strategy.require_strategy_constraints(
                constraints.clone(),
                settings.base_currency.clone(),
                settings.exposure_tolerance.clone(),
            );
            // The official engine creates a new Cash account and never restores
            // state; the existing LiveNode-only Margin assertion is not reused.
            engine.add_strategy(strategy)?;
            Ok(replay)
        })();
        let replay = match built {
            Ok(replay) => replay,
            Err(error) => {
                engine.dispose();
                return Err(error);
            }
        };
        Ok(Self {
            engine,
            replay,
            definitions,
            batches: TimestampBatch::default(),
            failure: None,
            original_frames: 0,
            completed_batches: 0,
            native_started: false,
            quote_covered_through: None,
        })
    }

    fn stop_accounting(&mut self, error: anyhow::Error) -> anyhow::Error {
        if self.failure.is_none() {
            self.failure = Some(error.to_string());
        }
        error
    }

    /// Call for every original source refresh before later ticks. A changed or
    /// missing fee definition makes subsequent profit unavailable, never zero.
    pub fn observe_definition(&mut self, instrument: &InstrumentAny) -> Result<()> {
        let result = (|| {
            ensure!(self.failure.is_none(), "PAPER_STREAM_ALREADY_INCOMPLETE");
            let (_, original) = self
                .definitions
                .get(&instrument.id())
                .ok_or_else(|| anyhow!("PAPER_STREAM_UNSELECTED_INSTRUMENT"))?;
            ensure!(
                *original == economic_definition(instrument)?,
                "PAPER_STREAM_DEFINITION_CHANGED_UNSUPPORTED"
            );
            Ok(())
        })();
        result.map_err(|error| self.stop_accounting(error))
    }

    /// The host reports a source loss/disconnection explicitly. Retained native
    /// state remains inspectable, but the run cannot later resume as complete.
    pub fn source_gap(&mut self) {
        self.stop_accounting(anyhow!("PAPER_STREAM_SOURCE_GAP"));
    }

    pub fn push(&mut self, data: Data) -> Result<()> {
        let result = (|| -> Result<()> {
            ensure!(self.failure.is_none(), "PAPER_STREAM_ALREADY_INCOMPLETE");
            let (id, event_at, initialized_at) = tick_identity(&data)?;
            let (instrument, _) = self
                .definitions
                .get(&id)
                .ok_or_else(|| anyhow!("PAPER_STREAM_UNSELECTED_INSTRUMENT"))?;
            ensure!(
                event_at <= initialized_at && instrument.ts_init().as_u64() <= initialized_at,
                "PAPER_STREAM_SOURCE_CLOCKS"
            );
            self.original_frames += 1;
            if let Some(batch) = self.batches.push(data)? {
                self.execute_batch(batch)?;
            }
            Ok(())
        })();
        result.map_err(|error| self.stop_accounting(error))
    }

    fn execute_batch(&mut self, batch: Vec<Data>) -> Result<()> {
        // In official L1_MBP, TradeTick updates the bid/ask even with
        // trade_execution=false. Preserve trades in source ordering/evidence,
        // but only original quotes may supply this quote-only execution book.
        let quotes = batch
            .into_iter()
            .filter(|event| matches!(event, Data::Quote(_)))
            .collect::<Vec<_>>();
        if quotes.is_empty() {
            return Ok(());
        }
        let covered_through = quotes
            .iter()
            .map(tick_identity)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|(_, _, at)| at)
            .max();
        self.engine.clear_data();
        // One homogeneous item per official add_data call preserves captured
        // tie order (official iterator appends streams FIFO on equal timestamps).
        for event in quotes {
            self.engine.add_data(vec![event], None, true, true)?;
        }
        self.native_started = true;
        self.engine.run(None, None, None, true)?;
        ensure!(
            self.replay.borrow().failure.is_none(),
            "PAPER_STREAM_NATIVE_TARGET_FAILED"
        );
        self.completed_batches += 1;
        self.quote_covered_through = covered_through;
        Ok(())
    }

    /// Call only after the producer confirms the original source segment ended.
    /// Incomplete source segments must use `source_gap` before finalization.
    pub fn finish(mut self) -> Result<Value> {
        if self.failure.is_none() {
            let final_batch = self.batches.source_ended();
            if let Err(error) = self.execute_batch(final_batch) {
                self.stop_accounting(error);
            }
        }
        if self.failure.is_none() && self.native_started {
            if let Err(error) = self.engine.end() {
                self.stop_accounting(error);
            }
        }
        let consumed = self.replay.borrow().consumed;
        if self.failure.is_none() && (consumed != 1 || self.replay.borrow().failure.is_some()) {
            self.stop_accounting(anyhow!("PAPER_STREAM_ORIGINAL_TARGET_NOT_COMPLETED"));
        }
        let strategy_stopped = component_state(&StrategyId::from("QZ-PAPER-001").inner())
            .is_ok_and(|state| state == ComponentState::Stopped);
        if self.failure.is_none() && !strategy_stopped {
            self.stop_accounting(anyhow!("PAPER_STREAM_NATIVE_STRATEGY_NOT_STOPPED"));
        }
        let result = self.engine.get_result();
        // Official end() can advance to a latency-deferred order arrival and
        // match against the last cached book. Preserve its raw record, but never
        // treat execution beyond the actual source segment as measured profit.
        let covered_through = self.quote_covered_through;
        if self.failure.is_none()
            && !result
                .backtest_end
                .is_some_and(|end| covered_through.is_some_and(|covered| end.as_u64() <= covered))
        {
            self.stop_accounting(anyhow!("PAPER_STREAM_NATIVE_EXECUTION_BEYOND_SOURCE"));
        }
        if self.failure.is_none()
            && (result.summary.get("orders.open").map(String::as_str) != Some("0")
                || result.summary.get("orders.inflight").map(String::as_str) != Some("0"))
        {
            self.stop_accounting(anyhow!("PAPER_STREAM_NATIVE_ORDERS_UNSETTLED"));
        }
        let canonical = if self.native_started {
            self.engine
                .get_canonical_result()
                .and_then(|value| value.to_bytes())
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).map_err(Into::into))
                .ok()
        } else {
            None
        };
        if self.failure.is_none()
            && !canonical.as_ref().is_some_and(|value| {
                value["run"]["outcome"] == "completed"
                    && value["components"]["trader_state"] == "STOPPED"
            })
        {
            self.stop_accounting(anyhow!("PAPER_STREAM_NATIVE_RUN_NOT_COMPLETED"));
        }
        // Canonical output normalizes generated identities for comparison. Keep
        // original PortfolioSnapshot values separately for the existing account
        // projection/relay; never reconstruct them from canonical records.
        let snapshots = self
            .engine
            .kernel()
            .portfolio
            .borrow()
            .snapshots(&AccountId::from("POLYMARKET-001"));
        let source_currency = self
            .definitions
            .values()
            .next()
            .map(|(instrument, _)| instrument.quote_currency());
        let terminal_valuation_complete = snapshots.last().is_some_and(|snapshot| {
            snapshot.account_id == AccountId::from("POLYMARKET-001")
                && snapshot.account_type == AccountType::Cash
                && snapshot.base_currency == source_currency
                && result.backtest_end == Some(snapshot.ts_init)
                && snapshot.ts_event <= snapshot.ts_init
                && !snapshot.is_stale
                && snapshot.stale_instruments.is_empty()
                && snapshot.stale_currencies.is_empty()
                && snapshot.unpriced_instruments.is_empty()
                && snapshot
                    .base_currency_equity
                    .as_ref()
                    .is_some_and(|equity| Some(equity.currency) == source_currency)
        });
        if self.failure.is_none() && !terminal_valuation_complete {
            self.stop_accounting(anyhow!("PAPER_STREAM_TERMINAL_VALUATION_UNAVAILABLE"));
        }
        let original_snapshots = serde_json::to_value(snapshots).ok();
        if self.failure.is_none()
            && !original_snapshots
                .as_ref()
                .and_then(Value::as_array)
                .is_some_and(|values| !values.is_empty())
        {
            self.stop_accounting(anyhow!("PAPER_STREAM_NATIVE_SNAPSHOTS_UNAVAILABLE"));
        }
        let complete = self.failure.is_none();
        let report = json!({
            "schema_version": 1,
            "mode": "PUBLIC_DATA_DRIVEN_NATIVE_PAPER_SIMULATION",
            "native_version": "0.63.0",
            "native_account_id": "POLYMARKET-001",
            "native_trader_id": result.trader_id,
            "native_session_id": result.instance_id,
            "native_run_id": result.run_id,
            "native_strategy_stopped": strategy_stopped,
            "native_terminal_valuation_complete": terminal_valuation_complete,
            "source_covered_through_ns": covered_through.map(|value| value.to_string()),
            "native_ended_at_ns": result.backtest_end.map(|value| value.as_u64().to_string()),
            "source_clock": "ORIGINAL_NATIVE_TS_EVENT_AND_TS_INIT",
            "execution_data": "ORIGINAL_QUOTES_ONLY_TRADES_RETAINED_AS_SOURCE_EVIDENCE",
            "latency_basis": "ORIGINAL_FROZEN_STATIC_LATENCY_IN_NATIVE_EVENT_TIME",
            "fee_basis": "UNCHANGED_OFFICIAL_POLYMARKET_FEE_MODEL_FROM_ORIGINAL_SCHEDULE",
            "original_tick_frames": self.original_frames,
            "completed_timestamp_batches": self.completed_batches,
            "consumed_original_target_points": consumed,
            "performance_status": if complete { "NATIVE_SIMULATION_AVAILABLE" } else { "UNAVAILABLE" },
            "reason_code": self.failure,
            "canonical_result": canonical,
            "canonical_identity_semantics": "OFFICIAL_NORMALIZED_COMPARISON_PROJECTION",
            "original_portfolio_snapshots": original_snapshots,
            "scientific_qualification": "NOT_ASSESSED",
            "live_account_or_orders": false,
        });
        Ok(report)
    }
}

impl Drop for PolymarketStreamingPaper {
    fn drop(&mut self) {
        // Dispose on both report completion and interrupted/failed host handling.
        // Disposal does not turn an incomplete source into a successful result.
        self.engine.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_core::UnixNanos;
    use nautilus_model::{
        data::QuoteTick,
        types::{Price, Quantity},
    };

    // Lexical/timestamp fixtures test batching only; they are not real-market data.
    fn quote(at: u64) -> Data {
        Data::Quote(QuoteTick::new(
            InstrumentId::from("condition-1.POLYMARKET"),
            Price::from("0.49"),
            Price::from("0.51"),
            Quantity::from("10"),
            Quantity::from("10"),
            UnixNanos::from(at),
            UnixNanos::from(at),
        ))
    }

    #[test]
    fn same_native_timestamp_is_never_split_across_runs() {
        let mut batches = TimestampBatch::default();
        assert!(batches.push(quote(10)).unwrap().is_none());
        assert!(batches.push(quote(10)).unwrap().is_none());
        assert_eq!(batches.push(quote(11)).unwrap().unwrap().len(), 2);
        assert_eq!(batches.source_ended().len(), 1);
    }

    #[test]
    fn late_native_timestamp_cannot_become_a_second_batch() {
        let mut batches = TimestampBatch::default();
        batches.push(quote(10)).unwrap();
        batches.push(quote(11)).unwrap();
        assert!(batches.push(quote(10)).is_err());
    }
}
