//! Execute compiled official strategies against original native catalog events.
use anyhow::{Result, ensure};
use contracts::{DbCounter, SchemaV1, native_strategy::*};
use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
};
use nautilus_common::logging::logger::LoggerConfig;
use nautilus_core::UnixNanos;
use nautilus_execution::models::fee::{FeeModelHandle, MakerTakerFeeModel};
use nautilus_model::{
    data::{DataBatch, NautilusDataType, OrderBookDelta, OrderBookDeltas},
    enums::{BookType, RecordFlag},
    identifiers::InstrumentId,
    instruments::Instrument,
    types::{Currency, Money},
};
use nautilus_persistence::catalog::types::CatalogQuery;
pub mod registry;
use registry::{CompiledNativeStrategy, build_strategy};
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
    let definitions = catalog.query_instruments_filtered(
        Some(&[id.to_string()]),
        None,
        Some(request.available_end_ns.get().into()),
    )?;
    let instrument = definitions
        .iter()
        .filter(|x| x.id() == id && x.ts_init().as_u64() <= available_start)
        .cloned()
        .max_by_key(|x| x.ts_init())
        .ok_or_else(|| anyhow::anyhow!("NATIVE_STRATEGY_INSTRUMENT_UNAVAILABLE_AT_START"))?;
    let config = BacktestEngineConfig {
        logging: LoggerConfig::builder()
            .bypass_logging(true)
            .is_colored(false)
            .build()?,
        bypass_logging: true,
        shutdown_on_error: true,
        ..Default::default()
    };
    let mut engine = BacktestEngine::new(config)?;
    let result = (|| -> Result<_> {
        let venue = &request.venue;
        let book: BookType = venue.book_type.parse()?;
        engine.add_venue(
            SimulatedVenueConfig::builder()
                .venue(id.venue)
                .account_type(venue.account_type.parse()?)
                .oms_type(venue.oms_type.parse()?)
                .book_type(book)
                .starting_balances(
                    venue
                        .starting_balances
                        .iter()
                        .map(|v| Money::from_str_exact(v).map_err(anyhow::Error::msg))
                        .collect::<Result<Vec<_>>>()?,
                )
                .maybe_base_currency(
                    venue
                        .base_currency
                        .as_deref()
                        .map(Currency::from_str)
                        .transpose()?,
                )
                .default_leverage(venue.leverage.parse()?)
                .fee_model(FeeModelHandle::new(MakerTakerFeeModel::new(
                    venue.maker_fee.parse()?,
                    venue.taker_fee.parse()?,
                )))
                .build()?,
        )?;
        engine.add_instrument(&instrument)?;
        let updates = definitions
            .into_iter()
            .filter(|x| x.id() == id && x.ts_init().as_u64() > available_start)
            .collect::<Vec<_>>();
        if !updates.is_empty() {
            engine.add_data_batch(DataBatch::Instrument(updates.into()), None, true, true)?;
        }
        match &request.strategy {
            NativeCompiledStrategyV1::EmaCross { .. } => ensure!(
                request
                    .selectors
                    .iter()
                    .any(|x| x.data_type.parse::<NautilusDataType>().ok()
                        == Some(NautilusDataType::QuoteTick)),
                "NATIVE_EMA_QUOTE_DATA_REQUIRED"
            ),
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
        match build_strategy(id, &request.strategy)? {
            CompiledNativeStrategy::EmaCross(strategy) => engine.add_strategy(strategy)?,
            CompiledNativeStrategy::OrderBookImbalance(strategy) => {
                engine.add_strategy(strategy)?
            }
        }
        let mut counts = BTreeMap::new();
        let mut selected = BTreeSet::new();
        for selector in &request.selectors {
            ensure!(
                selector.identifier == id.to_string(),
                "NATIVE_STRATEGY_SINGLE_INSTRUMENT_SELECTOR"
            );
            let kind: NautilusDataType = selector.data_type.parse()?;
            ensure!(
                selected.insert(kind.to_string()),
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
                rows += batch.len() as u64;
                if let DataBatch::BookDelta(values) = batch {
                    for delta in values.as_ref() {
                        pending.push(*delta);
                        if delta.flags & RecordFlag::F_LAST as u8 != 0 {
                            let grouped =
                                OrderBookDeltas::new_checked(id, std::mem::take(&mut pending))?;
                            engine.add_data_batch(
                                DataBatch::BookDeltas(vec![grouped].into()),
                                None,
                                true,
                                true,
                            )?;
                        }
                    }
                } else {
                    engine.add_data_batch(batch, None, true, true)?;
                }
            }
            ensure!(pending.is_empty(), "NATIVE_BOOK_INCOMPLETE_EVENT_BATCH");
            counts.insert(
                kind.to_string(),
                DbCounter::new(rows).map_err(anyhow::Error::msg)?,
            );
        }
        ensure!(
            counts.values().any(|rows| rows.get() > 0),
            "NATIVE_STRATEGY_EMPTY_DATA"
        );
        engine.run(
            request.available_start_ns.map(|v| v.get().into()),
            Some(request.available_end_ns.get().into()),
            None,
            false,
        )?;
        Ok(NativeStrategyBacktestResultV1 { schema_version: SchemaV1, request: request.clone(), native_version: crate::NAUTILUS_VERSION.into(), canonical_result: engine.get_canonical_result()?.as_value().clone(), data_rows_by_type: counts, notes: vec!["Compiled upstream strategy; not arbitrary Python import or live execution.".into(), "Fees are explicit simulation assumptions; historical computation is not out-of-sample validation.".into()] })
    })();
    engine.dispose();
    result
}
