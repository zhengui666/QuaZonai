//! Execute compiled official strategies against original native catalog events.
use anyhow::{ensure, Result};
use contracts::{native_strategy::*, DbCounter, SchemaV1};
use nautilus_backtest::{
    config::{BacktestDataConfig, BacktestEngineConfig, BacktestRunConfig, BacktestVenueConfig},
    node::BacktestNode,
};
use nautilus_common::{
    logging::logger::LoggerConfig,
    messages::system::ShutdownSystem,
    msgbus::{self, MessagingSwitchboard, ShareableMessageHandler},
};
use nautilus_core::UnixNanos;
use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
use nautilus_model::{
    data::{BarType, DataBatch, NautilusDataType, OrderBookDelta, OrderBookDeltas},
    enums::{BookType, RecordFlag},
    identifiers::{InstrumentId, TraderId},
    instruments::Instrument,
    types::{Currency, Money},
};
use nautilus_persistence::{
    catalog::types::CatalogQuery,
    config::{CatalogBackendType, DataCatalogConfig},
};
mod node_catalog;
pub(crate) mod performance;
#[cfg(test)]
mod preloaded_reference;
pub mod registry;
use registry::StrategyRegistration;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::Path,
    rc::Rc,
    str::FromStr,
};

// Observe the official shutdown notification without consuming its trigger or
// changing engine lifecycle. Keep only the first matching event, bounded for JSON output.
struct ShutdownDiagnostic {
    first: Rc<RefCell<Option<String>>>,
    handler: ShareableMessageHandler,
}
impl ShutdownDiagnostic {
    fn subscribe(trader_id: TraderId) -> Self {
        let first = Rc::new(RefCell::new(None));
        let captured = first.clone();
        let handler = ShareableMessageHandler::from_typed(move |event: &ShutdownSystem| {
            if event.trader_id == trader_id && captured.borrow().is_none() {
                *captured.borrow_mut() = Some(format!(
                    "OFFICIAL_SHUTDOWN_SYSTEM: {}",
                    serde_json::json!({"trader_id": event.trader_id.to_string(),
                        "component_id": event.component_id.to_string(),
                        "ts_init": event.ts_init.to_string(),
                        "reason": event.reason.as_ref().map(|reason| reason.chars().take(4096).collect::<String>()),
                        "reason_truncated": event.reason.as_ref().is_some_and(|reason| reason.chars().count() > 4096)})
                ));
            }
        });
        msgbus::subscribe_any(
            MessagingSwitchboard::shutdown_system_topic().into(),
            handler.clone(),
            None,
        );
        Self { first, handler }
    }
}
impl Drop for ShutdownDiagnostic {
    fn drop(&mut self) {
        msgbus::unsubscribe_any(
            MessagingSwitchboard::shutdown_system_topic().into(),
            &self.handler,
        );
    }
}

/// Configure only the official book-to-quote pipeline for the pure L2 OBI input.
/// Existing source quotes/trades and quote-driven strategies keep their own semantics.
fn configure_book_quotes(
    config: &mut BacktestEngineConfig,
    request: &NativeStrategyBacktestRequestV1,
) -> bool {
    let enabled = matches!(
        request.strategy,
        NativeCompiledStrategyV1::OrderBookImbalance { .. }
    ) && !request.selectors.is_empty()
        && request.selectors.iter().all(|selector| {
            selector.data_type.parse::<NautilusDataType>().ok()
                == Some(NautilusDataType::OrderBookDelta)
        });
    if enabled {
        config
            .data_engine
            .get_or_insert_with(Default::default)
            .emit_quotes_from_book = true;
    }
    enabled
}

pub fn run(
    root: &Path,
    request: NativeStrategyBacktestRequestV1,
) -> Result<NativeStrategyBacktestResultV1> {
    run_with_registration(root, request, None)
}

/// Run a statically linked user strategy on the same official node and data path.
pub fn run_with_registration(
    root: &Path,
    request: NativeStrategyBacktestRequestV1,
    registration: Option<StrategyRegistration>,
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
        NativeCompiledStrategyV1::HurstVpinDirectional { .. }
        | NativeCompiledStrategyV1::Registered { .. } => {}
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
    let mut engine_config = BacktestEngineConfig {
        logging: LoggerConfig::builder()
            .bypass_logging(true)
            .is_colored(false)
            .build()?,
        bypass_logging: true,
        shutdown_on_error: true,
        ..Default::default()
    };
    let book_quotes = configure_book_quotes(&mut engine_config, &request);
    let config = BacktestRunConfig::builder()
        .id("native-strategy".into())
        .venues(vec![venue_config])
        .data(data_configs)
        .engine(engine_config)
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
        let shutdown = ShutdownDiagnostic::subscribe(engine.trader_id());
        registry::add_strategy(engine, id, &request.strategy, registration)?;
        node.run()?;
        let engine = node
            .get_engine("native-strategy")
            .ok_or_else(|| anyhow::anyhow!("NATIVE_NODE_ENGINE_MISSING"))?;
        let mut notes = vec![if scenario.is_some() { "Hypothetical static instrument rules: the selected original definition and timestamps are unchanged, but applied as a fixed simulation assumption, not verified historical metadata.".into() } else { "Instrument definitions are selected by original catalog availability, not an independent source-history audit.".into() }, "Statically compiled Nautilus Strategy; not Python import or live execution.".into(), "Fees are explicit simulation assumptions; historical computation is not out-of-sample validation.".into()];
        if book_quotes {
            notes.push("OFFICIAL_BOOK_DERIVED_QUOTES: Nautilus DataEngine emit_quotes_from_book uses original L2 best bid/ask and size for quote cache/publication; these are not additional exchange observations or catalog QuoteTick rows. Original book events still drive matching. Native risk checks and stop-time market exits can change now that quotes exist; old unpriced results and new returns are not directly comparable.".into());
        }
        if let Some(diagnostic) = shutdown.first.borrow().clone() {
            notes.push(diagnostic);
        }
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

#[cfg(test)]
mod book_quote_tests {
    use super::*;
    use nautilus_model::{
        data::BookOrder,
        enums::{BookAction, OrderSide},
        instruments::{stubs::crypto_perpetual_ethusdt, InstrumentAny},
        types::{Price, Quantity},
    };
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
    use serde_json::json;
    const DAY: u64 = 86_400_000_000_000;
    fn request(id: InstrumentId) -> NativeStrategyBacktestRequestV1 {
        serde_json::from_value(json!({"schema_version":1,"instrument_id":id.to_string(),
          "selectors":[{"data_type":"OrderBookDelta","identifier":id.to_string()}],
          "event_start_ns":"1000000000","event_end_ns":(4*DAY).to_string(),"available_end_ns":(4*DAY).to_string(),
          "venue":{"account_type":"MARGIN","oms_type":"NETTING","book_type":"L2_MBP","starting_balances":["100000 USDT"],"base_currency":null,"leverage":"1","maker_fee":"0","taker_fee":"0"},
          "strategy":{"kind":"ORDER_BOOK_IMBALANCE","max_trade_size":"1.000","trigger_min_size":"1","trigger_imbalance_ratio":"0.2","min_seconds_between_triggers":0.0,"time_in_force":"IOC","dry_run":false}
        })).unwrap()
    }
    #[test]
    fn shutdown_diagnostic_observes_first_matching_event_and_unsubscribes() {
        let trader: TraderId = "TESTER-001".parse().unwrap();
        let observer = ShutdownDiagnostic::subscribe(trader);
        let event = |trader_id, reason| {
            ShutdownSystem::new(
                trader_id,
                "Strategy".into(),
                reason,
                nautilus_core::UUID4::new(),
                42.into(),
                None,
            )
        };
        msgbus::publish_any(
            MessagingSwitchboard::shutdown_system_topic(),
            &event("OTHER-001".parse().unwrap(), Some("other".into())),
        );
        assert!(observer.first.borrow().is_none());
        msgbus::publish_any(
            MessagingSwitchboard::shutdown_system_topic(),
            &event(trader, Some("é".repeat(5000))),
        );
        let first = observer.first.borrow().clone().unwrap();
        let diagnostic: serde_json::Value =
            serde_json::from_str(first.strip_prefix("OFFICIAL_SHUTDOWN_SYSTEM: ").unwrap())
                .unwrap();
        assert_eq!(diagnostic["reason"].as_str().unwrap().chars().count(), 4096);
        assert_eq!(diagnostic["reason_truncated"], true);
        msgbus::publish_any(
            MessagingSwitchboard::shutdown_system_topic(),
            &event(trader, Some("second".into())),
        );
        assert_eq!(observer.first.borrow().as_ref(), Some(&first));
        let retained = observer.first.clone();
        drop(observer);
        *retained.borrow_mut() = None;
        msgbus::publish_any(
            MessagingSwitchboard::shutdown_system_topic(),
            &event(trader, None),
        );
        assert!(retained.borrow().is_none());
        let next = ShutdownDiagnostic::subscribe(trader);
        msgbus::publish_any(
            MessagingSwitchboard::shutdown_system_topic(),
            &event(trader, None),
        );
        assert!(next
            .first
            .borrow()
            .as_ref()
            .unwrap()
            .contains("\"reason\":null"));
    }
    #[test]
    fn official_book_quote_configuration_only_changes_pure_l2_obi() {
        let mut request = request("ETHUSDT.BINANCE".parse().unwrap());
        let mut config = BacktestEngineConfig::default();
        assert!(configure_book_quotes(&mut config, &request));
        assert!(config.data_engine.unwrap().emit_quotes_from_book);
        for kind in ["QuoteTick", "TradeTick", "MarkPriceUpdate"] {
            let mut mixed = request.clone();
            mixed.selectors.push(
                serde_json::from_value(
                    json!({"data_type":kind,"identifier":request.instrument_id}),
                )
                .unwrap(),
            );
            let mut config = BacktestEngineConfig::default();
            assert!(!configure_book_quotes(&mut config, &mixed));
            assert!(config.data_engine.is_none());
        }
        request.strategy = serde_json::from_value(
            json!({"kind":"EMA_CROSS","trade_size":"1.000","fast_period":2,"slow_period":3}),
        )
        .unwrap();
        let mut config = BacktestEngineConfig::default();
        assert!(!configure_book_quotes(&mut config, &request));
        assert!(config.data_engine.is_none());
    }
    #[test]
    fn official_book_quotes_price_l2_positions_and_restore_native_stop_exit() {
        let dir = tempfile::tempdir().unwrap();
        let instrument = InstrumentAny::CryptoPerpetual(crypto_perpetual_ethusdt());
        let id = instrument.id();
        crate::catalog::currencies::write(dir.path(), std::slice::from_ref(&instrument)).unwrap();
        let catalog = ParquetDataCatalog::from_uri(
            dir.path().to_str().unwrap(),
            None,
            Some(4096),
            None,
            None,
        )
        .unwrap();
        catalog.write_instruments(vec![instrument]).unwrap();
        let mut books = Vec::new();
        for (index, time) in [1_000_000_000, DAY + 1_000_000_000, 2 * DAY + 1_000_000_000]
            .into_iter()
            .enumerate()
        {
            let init = time + 1;
            let mut clear = OrderBookDelta::clear(id, index as u64, time.into(), init.into());
            clear.order.price = Price::from("0.00");
            clear.order.size = Quantity::from("0.000");
            books.push(clear);
            for (side, price, size, last) in [
                (
                    OrderSide::Buy,
                    if index == 2 { "102.00" } else { "100.00" },
                    if index == 0 { "10.000" } else { "1.000" },
                    false,
                ),
                (
                    OrderSide::Sell,
                    if index == 2 { "103.00" } else { "101.00" },
                    "1.000",
                    true,
                ),
            ] {
                books.push(OrderBookDelta::new(
                    id,
                    BookAction::Add,
                    BookOrder::new(side, Price::from(price), Quantity::from(size), 0),
                    RecordFlag::F_SNAPSHOT as u8 | if last { RecordFlag::F_LAST as u8 } else { 0 },
                    index as u64,
                    time.into(),
                    init.into(),
                ));
            }
        }
        catalog.write_to_parquet(&books, None, None, None).unwrap();
        let root = dir.path().to_path_buf();
        let config = request(id);
        let old = std::thread::spawn(move || {
            super::preloaded_reference::run_with_quotes(&root, config, false).unwrap()
        })
        .join()
        .unwrap();
        let root = dir.path().to_path_buf();
        let config = request(id);
        let current = std::thread::spawn(move || super::run(&root, config).unwrap())
            .join()
            .unwrap();
        assert_eq!(
            old.equity_performance.reason.as_deref(),
            Some("UNPRICED_PORTFOLIO_SNAPSHOTS")
        );
        assert_eq!(
            current.equity_performance.unpriced_snapshot_count, "0",
            "{:?}",
            current.equity_performance
        );
        assert_eq!(
            current.equity_performance.status, "AVAILABLE",
            "{:?}",
            current.equity_performance
        );
        assert_eq!(current.data_rows_by_type.len(), 1);
        assert_eq!(current.data_rows_by_type["OrderBookDelta"].get(), 9);
        assert!(current
            .notes
            .iter()
            .any(|note| note.starts_with("OFFICIAL_BOOK_DERIVED_QUOTES:")));
        let old_fills = old.canonical_result["fills"].as_array().unwrap();
        let new_fills = current.canonical_result["fills"].as_array().unwrap();
        assert_eq!(
            old_fills.len(),
            1,
            "old no-quote path leaves only entry filled"
        );
        assert_eq!(
            new_fills.len(),
            2,
            "official OBI stop now closes via native market order"
        );
        // The same book signal fills at the same economic terms; canonical event
        // ordinals can differ because native valuation/account events now exist.
        for field in [
            "last_px",
            "last_qty",
            "commission",
            "order_side",
            "order_type",
            "ts_event",
            "ts_init",
        ] {
            assert_eq!(
                new_fills[0]["event"]["Filled"][field], old_fills[0]["event"]["Filled"][field],
                "{field}"
            );
        }
        assert_eq!(new_fills[1]["event"]["Filled"]["order_side"], "SELL");
        assert_eq!(new_fills[1]["event"]["Filled"]["order_type"], "MARKET");
        assert_eq!(new_fills[1]["event"]["Filled"]["last_px"], "102.00");
        assert_eq!(
            new_fills[1]["event"]["Filled"]["ts_event"],
            (4 * DAY).to_string()
        );
        assert!(
            old.canonical_result["orders"]
                .to_string()
                .contains("MARKET_PRICE_UNAVAILABLE"),
            "{}",
            old.canonical_result["orders"]
        );
        let root = dir.path().to_path_buf();
        let config = request(id);
        let enabled_preloaded = std::thread::spawn(move || {
            super::preloaded_reference::run_with_quotes(&root, config, true).unwrap()
        })
        .join()
        .unwrap();
        for field in ["fills", "orders", "positions"] {
            assert_eq!(
                current.canonical_result[field], enabled_preloaded.canonical_result[field],
                "streaming and preloaded official quotes: {field}"
            );
        }
    }
}
