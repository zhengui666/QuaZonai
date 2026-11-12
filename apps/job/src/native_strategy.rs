//! Execute compiled official strategies against original native catalog events.
use anyhow::{ensure, Result};
use contracts::{native_strategy::*, DbCounter, SchemaV1};
use nautilus_backtest::{
    config::{BacktestDataConfig, BacktestEngineConfig, BacktestRunConfig, BacktestVenueConfig},
    node::BacktestNode,
};
use nautilus_common::logging::logger::LoggerConfig;
use nautilus_core::UnixNanos;
use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
use nautilus_model::{
    data::{BarType, DataBatch, NautilusDataType, OrderBookDelta, OrderBookDeltas},
    enums::{BookType, RecordFlag},
    identifiers::InstrumentId,
    instruments::Instrument,
    types::{Currency, Money},
};
use nautilus_persistence::{
    catalog::types::CatalogQuery,
    config::{CatalogBackendType, DataCatalogConfig},
};
mod node_catalog;
mod performance;
#[cfg(test)]
mod preloaded_reference;
pub mod registry;
use registry::{build_strategy, CompiledNativeStrategy};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    str::FromStr,
};

pub fn run(
    root: &Path,
    request: NativeStrategyBacktestRequestV1,
) -> Result<NativeStrategyBacktestResultV1> {
    ensure!(
        request.event_start_ns < request.event_end_ns,
        "NATIVE_STRATEGY_EVENT_RANGE"
    );
    ensure!(
        request.available_end_ns >= request.event_start_ns,
        "NATIVE_STRATEGY_AVAILABILITY_RANGE"
    );
    ensure!(
        !request.selectors.is_empty(),
        "NATIVE_STRATEGY_DATA_REQUIRED"
    );
    let available_start = request
        .available_start_ns
        .unwrap_or(request.event_start_ns)
        .get();
    ensure!(
        available_start >= request.event_start_ns.get()
            && available_start <= request.available_end_ns.get(),
        "NATIVE_STRATEGY_AVAILABILITY_START"
    );
    let id: InstrumentId = request.instrument_id.parse()?;
    let catalog = crate::catalog::open_catalog(root)?;
    let scenario = match &request.instrument_definition {
        Some(NativeInstrumentDefinitionV1::StaticScenario {
            ts_init_ns,
            ts_event_ns,
        }) => Some((*ts_init_ns, *ts_event_ns)),
        _ => None,
    };
    let definitions = catalog.query_instruments_filtered(
        Some(&[id.to_string()]),
        None,
        Some(
            scenario
                .map(|(init, _)| init.get())
                .unwrap_or(request.available_end_ns.get())
                .into(),
        ),
    )?;
    let instrument = if let Some((init, event)) = scenario {
        let matches = definitions
            .iter()
            .filter(|x| {
                x.id() == id
                    && x.ts_init().as_u64() == init.get()
                    && x.ts_event().as_u64() == event.get()
            })
            .collect::<Vec<_>>();
        ensure!(
            matches.len() == 1,
            "NATIVE_STRATEGY_SCENARIO_DEFINITION_NOT_UNIQUE_OR_MISSING"
        );
        matches[0].clone()
    } else {
        definitions
            .iter()
            .filter(|x| x.id() == id && x.ts_init().as_u64() <= available_start)
            .max_by_key(|x| x.ts_init())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("NATIVE_STRATEGY_INSTRUMENT_UNAVAILABLE_AT_START"))?
    };
    let definition_basis = NativeInstrumentDefinitionBasisV1 {
        mode: if scenario.is_some() {
            "STATIC_SCENARIO"
        } else {
            "HISTORICAL_AS_OF_START"
        }
        .into(),
        ts_init_ns: DbCounter::new(instrument.ts_init().as_u64()).map_err(anyhow::Error::msg)?,
        ts_event_ns: DbCounter::new(instrument.ts_event().as_u64()).map_err(anyhow::Error::msg)?,
        scope: if scenario.is_some() {
            "HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED"
        } else {
            "CATALOG_DEFINITION_AVAILABLE_AT_WINDOW_START"
        }
        .into(),
    };
    let venue = &request.venue;
    let book: BookType = venue.book_type.parse()?;
    match &request.strategy {
        NativeCompiledStrategyV1::EmaCross { .. } => ensure!(
            request
                .selectors
                .iter()
                .any(|x| x.data_type.parse::<NautilusDataType>().ok()
                    == Some(NautilusDataType::QuoteTick)),
            "NATIVE_EMA_QUOTE_DATA_REQUIRED"
        ),
        NativeCompiledStrategyV1::GridMarketMaker { .. } => ensure!(
            request
                .selectors
                .iter()
                .any(|x| x.data_type.parse::<NautilusDataType>().ok()
                    == Some(NautilusDataType::QuoteTick)),
            "NATIVE_GRID_QUOTE_DATA_REQUIRED"
        ),
        // Subscriptions and internal BAR aggregation belong to the official strategy/data engine.
        NativeCompiledStrategyV1::HurstVpinDirectional { .. } => {}
        NativeCompiledStrategyV1::OrderBookImbalance { .. } => {
            ensure!(book == BookType::L2_MBP, "NATIVE_IMBALANCE_L2_REQUIRED");
            ensure!(
                request
                    .selectors
                    .iter()
                    .any(|x| x.data_type.parse::<NautilusDataType>().ok()
                        == Some(NautilusDataType::OrderBookDelta)),
                "NATIVE_IMBALANCE_BOOK_DATA_REQUIRED"
            );
        }
    }

    let updates = definitions
        .into_iter()
        .filter(|x| scenario.is_none() && x.id() == id && x.ts_init().as_u64() > available_start)
        .collect::<Vec<_>>();
    let catalog_config = DataCatalogConfig::new(
        root.to_string_lossy().into_owned(),
        None,
        Some(CatalogBackendType::External(node_catalog::BACKEND.into())),
    );
    let mut data_configs = Vec::new();
    if !updates.is_empty() {
        data_configs.push(
            BacktestDataConfig::builder()
                .data_type(NautilusDataType::Instrument)
                .catalog(catalog_config.clone())
                .instrument_id(id)
                .start_time(available_start.into())
                .build()?,
        );
    }
    // Count and validate rows observed before strategy execution. Parquet has no snapshot
    // isolation: later reads may change. Report this scan separately from replay reads.
    let mut counts: BTreeMap<String, DbCounter> = BTreeMap::new();
    let mut selected = BTreeSet::new();
    for selector in &request.selectors {
        let kind: NautilusDataType = selector.data_type.parse()?;
        let selector_instrument = if kind == NautilusDataType::Bar {
            selector.identifier.parse::<BarType>()?.instrument_id()
        } else {
            selector.identifier.parse::<InstrumentId>()?
        };
        ensure!(
            selector_instrument == id,
            "NATIVE_STRATEGY_SINGLE_INSTRUMENT_SELECTOR"
        );
        ensure!(
            selected.insert((kind.to_string(), selector.identifier.clone())),
            "NATIVE_STRATEGY_DUPLICATE_SELECTOR"
        );
        let query = CatalogQuery::new(kind.clone())
            .with_identifiers(Some(vec![selector.identifier.clone()]))
            .with_range(
                Some(UnixNanos::from(available_start)),
                Some(UnixNanos::from(request.available_end_ns.get())),
            );
        let mut session = crate::catalog::file_batch_session(
            &catalog,
            query,
            Some(request.event_start_ns.get()..request.event_end_ns.get()),
            4096,
        )?;
        let mut rows = 0u64;
        let mut pending = Vec::<OrderBookDelta>::new();
        while let Some(batch) = session.next_batch()? {
            rows = rows
                .checked_add(batch.len() as u64)
                .ok_or_else(|| anyhow::anyhow!("NATIVE_STRATEGY_ROW_COUNT_OVERFLOW"))?;
            if let DataBatch::BookDelta(values) = batch {
                for delta in values.as_ref() {
                    pending.push(*delta);
                    if delta.flags & RecordFlag::F_LAST as u8 != 0 {
                        // Validation only; the official node performs replay grouping later.
                        OrderBookDeltas::new_checked(id, std::mem::take(&mut pending))?;
                    }
                }
            }
        }
        ensure!(pending.is_empty(), "NATIVE_BOOK_INCOMPLETE_EVENT_BATCH");
        counts.insert(
            kind.to_string(),
            DbCounter::new(
                counts
                    .get(&kind.to_string())
                    .map_or(0, |count| count.get())
                    .checked_add(rows)
                    .ok_or_else(|| anyhow::anyhow!("NATIVE_STRATEGY_ROW_COUNT_OVERFLOW"))?,
            )
            .map_err(anyhow::Error::msg)?,
        );
        data_configs.push(
            BacktestDataConfig::builder()
                .data_type(kind.clone())
                .catalog(catalog_config.clone())
                .instrument_id(id)
                .maybe_bar_types(
                    (kind == NautilusDataType::Bar).then(|| vec![selector.identifier.clone()]),
                )
                .start_time(available_start.into())
                .end_time(request.available_end_ns.get().into())
                .batch_deltas(kind == NautilusDataType::OrderBookDelta)
                .optimize_file_loading(false)
                .build()?,
        );
    }
    ensure!(
        counts.values().any(|rows| rows.get() > 0),
        "NATIVE_STRATEGY_EMPTY_DATA"
    );
    // Keep the previous exact-money validation, rather than accepting rounded starting balances.
    for balance in &venue.starting_balances {
        Money::from_str_exact(balance).map_err(anyhow::Error::msg)?;
    }
    let venue_config = BacktestVenueConfig::builder()
        .name(id.venue.as_str())
        .account_type(venue.account_type.parse()?)
        .oms_type(venue.oms_type.parse()?)
        .book_type(book)
        .bar_execution(venue.bar_execution.unwrap_or(true))
        .starting_balances(venue.starting_balances.clone())
        .maybe_base_currency(
            venue
                .base_currency
                .as_deref()
                .map(Currency::from_str)
                .transpose()?,
        )
        .default_leverage(venue.leverage.parse()?)
        .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::new(
            venue.maker_fee.parse()?,
            venue.taker_fee.parse()?,
        )))
        .build()?;
    let config = BacktestRunConfig::builder()
        .id("native-strategy".into())
        .venues(vec![venue_config])
        .data(data_configs)
        .engine(BacktestEngineConfig {
            logging: LoggerConfig::builder()
                .bypass_logging(true)
                .is_colored(false)
                .build()?,
            bypass_logging: true,
            shutdown_on_error: true,
            ..Default::default()
        })
        .chunk_size(4096)
        // None must remain None: data availability filtering is not an explicit engine start.
        .maybe_start(request.available_start_ns.map(|v| v.get().into()))
        .end(request.available_end_ns.get().into())
        .raise_exception(true)
        .dispose_on_completion(false)
        .build()?;
    let replay_reads = node_catalog::RowCounts::default();
    let factories = node_catalog::factories(node_catalog::Window {
        root: root.into(),
        initial: instrument,
        updates,
        event_start: request.event_start_ns.get(),
        event_end: request.event_end_ns.get(),
        counts: replay_reads.clone(),
    });
    let mut node = BacktestNode::builder(vec![config])
        .with_catalog_factories(factories)
        .build()?;
    let result = (|| -> Result<_> {
        node.build()?;
        let engine = node
            .get_engine_mut("native-strategy")
            .ok_or_else(|| anyhow::anyhow!("NATIVE_NODE_ENGINE_MISSING"))?;
        match build_strategy(id, &request.strategy)? {
            CompiledNativeStrategy::EmaCross(strategy) => engine.add_strategy(strategy)?,
            CompiledNativeStrategy::HurstVpinDirectional(strategy) => {
                engine.add_strategy(*strategy)?
            }
            CompiledNativeStrategy::GridMarketMaker(strategy) => engine.add_strategy(*strategy)?,
            CompiledNativeStrategy::OrderBookImbalance(strategy) => {
                engine.add_strategy(strategy)?
            }
        }
        node.run()?;
        let engine = node
            .get_engine("native-strategy")
            .ok_or_else(|| anyhow::anyhow!("NATIVE_NODE_ENGINE_MISSING"))?;
        let mut notes = vec![if scenario.is_some() { "Hypothetical static instrument rules: the selected original definition and timestamps are unchanged, but applied as a fixed simulation assumption, not verified historical metadata.".into() } else { "Instrument definitions are selected by original catalog availability, not an independent source-history audit.".into() }, "Compiled upstream strategy; not arbitrary Python import or live execution.".into(), "Fees are explicit simulation assumptions; historical computation is not out-of-sample validation.".into()];
        notes.extend(node_catalog::row_count_notes(&counts, &replay_reads)?);
        Ok(NativeStrategyBacktestResultV1 {
            schema_version: SchemaV1,
            request: request.clone(),
            native_version: crate::NAUTILUS_VERSION.into(),
            instrument_definition: definition_basis,
            canonical_result: engine.get_canonical_result()?.as_value().clone(),
            equity_performance: performance::from_engine(engine),
            data_rows_by_type: counts,
            notes,
        })
    })();
    node.dispose();
    result
}
