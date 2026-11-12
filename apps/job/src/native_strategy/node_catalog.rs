//! Thin catalog adapter for official BacktestNode, retaining selected definitions and time windows.
//! No scheduling, market simulation, matching, or book assembly is implemented here.
use anyhow::{bail, ensure, Result};
use nautilus_core::{Params, UnixNanos};
use nautilus_model::{
    data::{Data, DataBatch, NautilusDataType, NautilusRecordType},
    instruments::{Instrument, InstrumentAny},
};
use nautilus_persistence::{
    backend::parquet::catalog::ParquetDataCatalog,
    catalog::{
        factory::{CatalogFactory, CatalogFactoryRegistry},
        session::{DataBatchQuery, DataBatchQueryResult, TypedDataBatchSession},
        traits::{CatalogReader, CatalogWriter, RecordBatch},
        types::{CatalogInstrumentQuery, CatalogQuery},
    },
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub(super) const BACKEND: &str = "quazonai-native-window";
pub(super) type RowCounts = Arc<Mutex<BTreeMap<String, u64>>>;

/// Existing notes carry count provenance through the existing task/report projections.
/// Neither read-only access nor Parquet Latest supplies isolation from other importers.
pub(super) fn row_count_notes(
    pre_scan: &BTreeMap<String, contracts::DbCounter>,
    reads: &RowCounts,
) -> Result<Vec<String>> {
    let mut observed = pre_scan
        .keys()
        .map(|kind| {
            Ok((
                kind.clone(),
                contracts::DbCounter::new(0).map_err(anyhow::Error::msg)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    for (kind, rows) in reads
        .lock()
        .map_err(|_| anyhow::anyhow!("NATIVE_NODE_COUNTER_POISONED"))?
        .iter()
    {
        observed.insert(
            kind.clone(),
            contracts::DbCounter::new(*rows).map_err(anyhow::Error::msg)?,
        );
    }
    let different = pre_scan.len() != observed.len()
        || pre_scan
            .iter()
            .any(|(kind, rows)| observed.get(kind).map(|value| value.get()) != Some(rows.get()));
    let mut notes = vec![
        "CATALOG_ROW_COUNTS: data_rows_by_type are rows observed by the pre-run validation scan, not a catalog snapshot or replay-consumed rows.".into(),
        format!("REPLAY_READER_ROWS_INCLUDING_PREFETCH: {}. These are rows read by replay sessions, including look-ahead; they are not replay-consumed rows.", serde_json::to_string(&observed)?),
        "CATALOG_READ_CONSISTENCY_UNVERIFIED: Parquet Latest reads and read-only access do not isolate concurrent imports or replacements. File-range groups are discovered per selector and opened later; new files may or may not be included. Reads, folds and candidates may see different contents; equal counts do not prove identical input. Official canonical run bounds and iterations describe execution, not a stable input snapshot.".into(),
    ];
    if different {
        notes.push("PRE_SCAN_REPLAY_READER_COUNTS_DIFFER: the pre-run scan and replay reader observed different row counts. Early stopping, look-ahead or catalog changes may contribute; no single cause or input consistency is inferred.".into());
    }
    Ok(notes)
}

/// Captures the exact original initial definition and only the authorized history updates.
/// The caller must pass no updates for a hypothetical STATIC_SCENARIO.
#[derive(Debug, Clone)]
pub(super) struct Window {
    pub root: PathBuf,
    pub initial: InstrumentAny,
    pub updates: Vec<InstrumentAny>,
    pub event_start: u64,
    pub event_end: u64,
    pub counts: RowCounts,
}

pub(super) fn factories(window: Window) -> CatalogFactoryRegistry {
    let factory: CatalogFactory = Arc::new(move |_| {
        let mut catalog = crate::catalog::open_catalog(&window.root)?;
        // Bounds the decoded page target, not DataFusion sort memory or total file buffers.
        catalog.batch_size = 4096;
        Ok(Box::new(WindowCatalog {
            catalog,
            window: window.clone(),
        }))
    });
    [(BACKEND.to_owned(), factory)].into_iter().collect()
}

#[derive(Debug)]
struct WindowCatalog {
    catalog: ParquetDataCatalog,
    window: Window,
}

impl CatalogReader for WindowCatalog {
    fn reset_session(&mut self) {
        self.catalog.reset_session();
    }

    fn instruments(&mut self, query: &CatalogInstrumentQuery) -> Result<Vec<InstrumentAny>> {
        if let Some(ids) = &query.instrument_ids {
            ensure!(
                ids.iter()
                    .all(|id| id == &self.window.initial.id().to_string()),
                "NATIVE_NODE_UNEXPECTED_INSTRUMENT"
            );
        }
        // No mutation of definition timestamps and no exposure of a future definition.
        Ok(vec![self.window.initial.clone()])
    }

    fn query_batch(&mut self, _: &CatalogQuery) -> Result<DataBatch> {
        bail!("NATIVE_NODE_COLLECTED_QUERY_DISABLED")
    }

    fn query_batch_session(
        &mut self,
        query: &CatalogQuery,
        _: Option<usize>,
    ) -> Result<DataBatchQueryResult> {
        if query.data_type == NautilusDataType::Instrument {
            let mut updates = self
                .window
                .updates
                .iter()
                .filter(|row| {
                    query.start.is_none_or(|start| row.ts_init() >= start)
                        && query.end.is_none_or(|end| row.ts_init() <= end)
                })
                .cloned()
                .collect::<Vec<_>>();
            updates.sort_by_key(Instrument::ts_init);
            return Ok(Box::new(TypedDataBatchSession::from_vec(
                updates,
                Some(4096),
            )));
        }
        let mut query = query.clone();
        ensure!(
            query.where_clause.is_none(),
            "NATIVE_NODE_UNEXPECTED_FILTER"
        );
        // Preserve the existing half-open event window separately from ts_init availability.
        query.where_clause = Some(format!(
            "CAST(ts_event AS BIGINT) >= {} AND CAST(ts_event AS BIGINT) < {}",
            self.window.event_start, self.window.event_end
        ));
        // Directory grouping may interleave equal-ts rows across overlapping files.
        // Use the official per-file source merge to preserve file-priority ties.
        query.params = Some(Params::new(
            [("optimize_file_loading".to_string(), false.into())].into(),
        ));
        let ids = query
            .identifiers
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("NATIVE_NODE_IDENTIFIER_REQUIRED"))?;
        ensure!(ids.len() == 1, "NATIVE_NODE_SINGLE_SELECTOR_SESSION");
        let intervals = self
            .catalog
            .get_intervals(&query.data_type.clone().into(), Some(&ids[0]))?;
        let groups = overlap_groups(intervals)
            .into_iter()
            .filter_map(|(start, end)| {
                let start = start.max(query.start.map_or(0, |value| value.as_u64()));
                let end = end.min(query.end.map_or(u64::MAX, |value| value.as_u64()));
                (start <= end).then_some((start, end))
            })
            .collect::<Vec<_>>()
            .into_iter();
        let inner: DataBatchQueryResult = Box::new(LazyGroups {
            root: self.window.root.clone(),
            query: query.clone(),
            groups,
            current: None,
        });
        Ok(Box::new(CountedSession {
            inner,
            kind: query.data_type.to_string(),
            counts: self.window.counts.clone(),
        }))
    }
}

// This groups file I/O ranges only. All row merging, frame assembly and replay remain upstream.
fn overlap_groups(mut ranges: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    ranges.sort_unstable();
    let mut groups: Vec<(u64, u64)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = groups.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        groups.push((start, end));
    }
    groups
}

struct LazyGroups {
    root: PathBuf,
    query: CatalogQuery,
    groups: std::vec::IntoIter<(u64, u64)>,
    current: Option<DataBatchQueryResult>,
}
impl DataBatchQuery for LazyGroups {
    fn next_batch(&mut self) -> Result<Option<DataBatch>> {
        loop {
            if let Some(current) = &mut self.current {
                if let Some(batch) = current.next_batch()? {
                    return Ok(Some(batch));
                }
            }
            // Drop exhausted sources and their prefetch tasks before opening the next group.
            self.current = None;
            let Some((start, end)) = self.groups.next() else {
                return Ok(None);
            };
            let mut catalog = crate::catalog::open_catalog(&self.root)?;
            catalog.batch_size = 4096;
            let mut query = self.query.clone();
            query.start = Some(start.into());
            query.end = Some(end.into());
            self.current = Some(catalog.query_batch_session(&query, Some(4096))?);
        }
    }
}

struct CountedSession {
    inner: DataBatchQueryResult,
    kind: String,
    counts: RowCounts,
}
impl DataBatchQuery for CountedSession {
    fn next_batch(&mut self) -> Result<Option<DataBatch>> {
        let batch = self.inner.next_batch()?;
        if let Some(batch) = &batch {
            let mut counts = self
                .counts
                .lock()
                .map_err(|_| anyhow::anyhow!("NATIVE_NODE_COUNTER_POISONED"))?;
            let count = counts.entry(self.kind.clone()).or_default();
            *count = count
                .checked_add(batch.len() as u64)
                .ok_or_else(|| anyhow::anyhow!("NATIVE_STRATEGY_ROW_COUNT_OVERFLOW"))?;
        }
        Ok(batch)
    }
}

// The factory API asks for a full catalog; this adapter is deliberately read-only.
impl CatalogWriter for WindowCatalog {
    fn write_instruments(&mut self, _: &[InstrumentAny]) -> Result<()> {
        bail!("NATIVE_NODE_READ_ONLY")
    }
    fn write_data(
        &mut self,
        _: &[Data],
        _: Option<UnixNanos>,
        _: Option<UnixNanos>,
        _: Option<Params>,
    ) -> Result<()> {
        bail!("NATIVE_NODE_READ_ONLY")
    }
    fn write_records(
        &mut self,
        _: NautilusRecordType,
        _: &[RecordBatch],
        _: Option<Params>,
    ) -> Result<()> {
        bail!("NATIVE_NODE_READ_ONLY")
    }
    fn record_empty_coverage(
        &mut self,
        _: NautilusDataType,
        _: Option<&str>,
        _: UnixNanos,
        _: UnixNanos,
    ) -> Result<()> {
        bail!("NATIVE_NODE_READ_ONLY")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        data::OrderBookDelta, enums::RecordFlag, instruments::stubs::currency_pair_btcusdt,
    };

    #[test]
    fn official_session_preserves_equal_ns_book_rows_and_event_filter() {
        let dir = tempfile::tempdir().unwrap();
        let initial = InstrumentAny::CurrencyPair(currency_pair_btcusdt());
        let id = initial.id();
        let catalog = crate::catalog::open_catalog(dir.path()).unwrap();
        let mut expected = Vec::new();
        // Rows share ts_init within each file and cross the 4096-row batch target.
        // Distinct sequence numbers detect reordering even though all rows are CLEARs.
        for file in 0..2u64 {
            let rows = (0..4101u64)
                .map(|index| {
                    let mut row =
                        OrderBookDelta::clear(id, file * 4101 + index, 10.into(), 20.into());
                    row.flags |= RecordFlag::F_LAST as u8;
                    row
                })
                .collect::<Vec<_>>();
            // Separate explicit filenames are required for identical timestamp ranges.
            // Use different original availability times per file for this initial fixture.
            let rows = rows
                .into_iter()
                .map(|mut row| {
                    row.ts_init = (20 + file).into();
                    row
                })
                .collect::<Vec<_>>();
            catalog.write_to_parquet(&rows, None, None, None).unwrap();
            expected.extend(rows);
        }
        let counts = RowCounts::default();
        let window = Window {
            root: dir.path().into(),
            initial,
            updates: Vec::new(),
            event_start: 10,
            event_end: 11,
            counts: counts.clone(),
        };
        let registry = factories(window);
        let mut adapted = registry[BACKEND](
            &nautilus_persistence::catalog::factory::CatalogConnectConfig::new("unused", None),
        )
        .unwrap();
        let query = CatalogQuery::new(NautilusDataType::OrderBookDelta)
            .with_identifiers(Some(vec![id.to_string()]))
            .with_range(Some(20.into()), Some(21.into()));
        let mut session = adapted.query_batch_session(&query, Some(4096)).unwrap();
        let mut actual = Vec::new();
        while let Some(batch) = session.next_batch().unwrap() {
            match batch {
                DataBatch::BookDelta(rows) => actual.extend_from_slice(rows.as_ref()),
                _ => panic!("unexpected batch"),
            }
        }
        assert_eq!(actual, expected);
        assert_eq!(counts.lock().unwrap()["OrderBookDelta"], 8202);
        // Repeated metadata lookup never replaces the explicitly selected initial definition.
        assert_eq!(
            adapted
                .instruments(&CatalogInstrumentQuery::new())
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    #[ignore = "manual isolated reader RSS measurement; synthetic data, not strategy evidence"]
    fn reader_rss_probe() {
        use nautilus_model::{
            data::BookOrder,
            enums::{BookAction, OrderSide},
            types::{Price, Quantity},
        };
        let mode = std::env::var("QZ_PROBE_MODE").unwrap_or_else(|_| "stream".into());
        let files: u64 = std::env::var("QZ_PROBE_FILES")
            .unwrap_or_else(|_| "648".into())
            .parse()
            .unwrap();
        let frames: u64 = std::env::var("QZ_PROBE_FRAMES")
            .unwrap_or_else(|_| "1365".into())
            .parse()
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let initial = InstrumentAny::CurrencyPair(currency_pair_btcusdt());
        let id = initial.id();
        let catalog = crate::catalog::open_catalog(dir.path()).unwrap();
        for file in 0..files {
            let mut rows = Vec::new();
            for frame in 0..frames {
                let ts = 1_000_000_000 + file * 1_000_000 + frame;
                let seq = file * frames + frame;
                let mut clear = OrderBookDelta::clear(id, seq, ts.into(), ts.into());
                clear.order.price = Price::from("0.00");
                clear.order.size = Quantity::from("0.000000");
                rows.push(clear);
                rows.push(OrderBookDelta::new(
                    id,
                    BookAction::Add,
                    BookOrder::new(
                        OrderSide::Buy,
                        Price::from("100.00"),
                        Quantity::from("1.000000"),
                        0,
                    ),
                    RecordFlag::F_SNAPSHOT as u8,
                    seq,
                    ts.into(),
                    ts.into(),
                ));
                rows.push(OrderBookDelta::new(
                    id,
                    BookAction::Add,
                    BookOrder::new(
                        OrderSide::Sell,
                        Price::from("101.00"),
                        Quantity::from("1.000000"),
                        0,
                    ),
                    RecordFlag::F_SNAPSHOT as u8 | RecordFlag::F_LAST as u8,
                    seq,
                    ts.into(),
                    ts.into(),
                ));
            }
            catalog.write_to_parquet(&rows, None, None, None).unwrap();
        }
        let query = CatalogQuery::new(NautilusDataType::OrderBookDelta)
            .with_identifiers(Some(vec![id.to_string()]));
        let start = std::time::Instant::now();
        if mode == "engine_preload" || mode == "node_lazy" || mode == "production_node" {
            crate::catalog::currencies::write(dir.path(), std::slice::from_ref(&initial)).unwrap();
            catalog.write_instruments(vec![initial.clone()]).unwrap();
            let result = probe_full_runner(dir.path(), initial, &mode);
            assert_eq!(result["run"]["iterations"], (files * frames).to_string());
            assert!(result["fills"].as_array().unwrap().is_empty());
            std::fs::write(
                format!("/tmp/qz-rss-{mode}-canonical.json"),
                serde_json::to_vec_pretty(&result).unwrap(),
            )
            .unwrap();
            let status = std::fs::read_to_string("/proc/self/status").unwrap();
            eprintln!("PROBE mode={mode} files={files} frames_per_file={frames} frames={} replay_elapsed={:?} {}", files * frames, start.elapsed(), status.lines().find(|line| line.starts_with("VmHWM:")).unwrap());
            return;
        }
        let mut count = 0u64;
        let mut checksum = 0u64;
        let mut retained = Vec::new();
        if mode == "preload" {
            let mut session =
                crate::catalog::file_batch_session(&catalog, query, None, 4096).unwrap();
            while let Some(batch) = session.next_batch().unwrap() {
                if let DataBatch::BookDelta(rows) = batch {
                    retained.extend_from_slice(rows.as_ref());
                }
            }
            for row in &retained {
                count += 1;
                checksum += row.sequence;
            }
        } else {
            let registry = factories(Window {
                root: dir.path().into(),
                initial,
                updates: Vec::new(),
                event_start: 0,
                event_end: u64::MAX,
                counts: RowCounts::default(),
            });
            let mut adapted = registry[BACKEND](
                &nautilus_persistence::catalog::factory::CatalogConnectConfig::new("unused", None),
            )
            .unwrap();
            let mut session = adapted.query_batch_session(&query, None).unwrap();
            while let Some(batch) = session.next_batch().unwrap() {
                if let DataBatch::BookDelta(rows) = batch {
                    for row in rows.as_ref() {
                        count += 1;
                        checksum += row.sequence;
                    }
                }
            }
        }
        assert_eq!(count, files * frames * 3);
        assert_eq!(checksum, (files * frames) * (files * frames - 1) / 2 * 3);
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        eprintln!("PROBE mode={mode} files={files} frames_per_file={frames} rows={count} checksum={checksum} read_elapsed={:?} {}", start.elapsed(), status.lines().find(|line| line.starts_with("VmHWM:")).unwrap());
        std::hint::black_box(retained);
    }

    fn probe_full_runner(
        root: &std::path::Path,
        initial: InstrumentAny,
        mode: &str,
    ) -> serde_json::Value {
        use nautilus_backtest::{
            config::{
                BacktestDataConfig, BacktestEngineConfig, BacktestRunConfig, BacktestVenueConfig,
            },
            node::BacktestNode,
        };
        use nautilus_common::logging::logger::LoggerConfig;
        use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
        use nautilus_model::enums::{AccountType, BookType, OmsType};
        use nautilus_persistence::config::{CatalogBackendType, DataCatalogConfig};
        let id = initial.id();
        let request: contracts::native_strategy::NativeStrategyBacktestRequestV1 = serde_json::from_value(serde_json::json!({"schema_version":1,"instrument_id":id.to_string(),
            "selectors":[{"data_type":"OrderBookDelta","identifier":id.to_string()}],
            "event_start_ns":"1000000000","event_end_ns":"10000000000","available_end_ns":"5000000000",
            "venue":{"account_type":"CASH","oms_type":"NETTING","book_type":"L2_MBP","starting_balances":["100000 USDT","1 BTC"],"base_currency":null,"leverage":"1","maker_fee":"0","taker_fee":"0"},
            "strategy":{"kind":"ORDER_BOOK_IMBALANCE","max_trade_size":"0.010000","trigger_min_size":"1","trigger_imbalance_ratio":"0.2","min_seconds_between_triggers":0.0,"time_in_force":"IOC","dry_run":true}})).unwrap();
        if mode == "production_node" {
            return crate::native_strategy::run(root, request)
                .unwrap()
                .canonical_result;
        }
        if mode == "engine_preload" {
            return crate::native_strategy::preloaded_reference::run(root, request)
                .unwrap()
                .canonical_result;
        }
        // Include the compatible selected-total counting pass in end-to-end time/RSS.
        let catalog = crate::catalog::open_catalog(root).unwrap();
        let mut preflight = crate::catalog::file_batch_session(
            &catalog,
            CatalogQuery::new(NautilusDataType::OrderBookDelta)
                .with_identifiers(Some(vec![id.to_string()])),
            Some(1_000_000_000..10_000_000_000),
            4096,
        )
        .unwrap();
        let mut total = 0u64;
        while let Some(batch) = preflight.next_batch().unwrap() {
            total += batch.len() as u64;
        }
        let counts = RowCounts::default();
        let registry = factories(Window {
            root: root.into(),
            initial,
            updates: Vec::new(),
            event_start: 1_000_000_000,
            event_end: 10_000_000_000,
            counts: counts.clone(),
        });
        let venue = BacktestVenueConfig::builder()
            .name("BINANCE")
            .oms_type(OmsType::Netting)
            .account_type(AccountType::Cash)
            .book_type(BookType::L2_MBP)
            .starting_balances(request.venue.starting_balances.clone())
            .default_leverage("1".parse().unwrap())
            .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()))
            .build()
            .unwrap();
        let data = BacktestDataConfig::builder()
            .data_type(NautilusDataType::OrderBookDelta)
            .instrument_id(id)
            .start_time(1_000_000_000.into())
            .catalog(DataCatalogConfig::new(
                root.to_str().unwrap().into(),
                None,
                Some(CatalogBackendType::External(BACKEND.into())),
            ))
            .batch_deltas(true)
            .optimize_file_loading(false)
            .build()
            .unwrap();
        let config = BacktestRunConfig::builder()
            .id("rss".into())
            .venues(vec![venue])
            .data(vec![data])
            .chunk_size(4096)
            .end(5_000_000_000.into())
            .raise_exception(true)
            .dispose_on_completion(false)
            .engine(BacktestEngineConfig {
                logging: LoggerConfig::builder()
                    .bypass_logging(true)
                    .is_colored(false)
                    .build()
                    .unwrap(),
                bypass_logging: true,
                shutdown_on_error: true,
                ..Default::default()
            })
            .build()
            .unwrap();
        let mut node = BacktestNode::builder(vec![config])
            .with_catalog_factories(registry)
            .build()
            .unwrap();
        node.build().unwrap();
        let crate::native_strategy::registry::CompiledNativeStrategy::OrderBookImbalance(strategy) =
            crate::native_strategy::registry::build_strategy(id, &request.strategy).unwrap()
        else {
            unreachable!()
        };
        node.get_engine_mut("rss")
            .unwrap()
            .add_strategy(strategy)
            .unwrap();
        node.run().unwrap();
        let mut result = node
            .get_engine("rss")
            .unwrap()
            .get_canonical_result()
            .unwrap()
            .as_value()
            .clone();
        node.dispose();
        assert_eq!(counts.lock().unwrap()["OrderBookDelta"], total);
        result["run"]["run_config_id"] = serde_json::Value::Null;
        result
    }

    #[test]
    fn same_ns_l2_frame_spans_files_without_losing_clear_or_last() {
        use nautilus_model::{
            data::BookOrder,
            enums::{BookAction, OrderSide},
            types::{Price, Quantity},
        };
        let dir = tempfile::tempdir().unwrap();
        let initial = InstrumentAny::CurrencyPair(currency_pair_btcusdt());
        let id = initial.id();
        let catalog = crate::catalog::open_catalog(dir.path()).unwrap();
        let mut clear = OrderBookDelta::clear(id, 0, 10.into(), 20.into());
        clear.order.price = Price::from("0.00");
        clear.order.size = Quantity::from("0.000000");
        let buy = OrderBookDelta::new(
            id,
            BookAction::Add,
            BookOrder::new(
                OrderSide::Buy,
                Price::from("100.00"),
                Quantity::from("1.000000"),
                0,
            ),
            RecordFlag::F_SNAPSHOT as u8,
            1,
            10.into(),
            20.into(),
        );
        let sell = OrderBookDelta::new(
            id,
            BookAction::Add,
            BookOrder::new(
                OrderSide::Sell,
                Price::from("101.00"),
                Quantity::from("1.000000"),
                0,
            ),
            RecordFlag::F_SNAPSHOT as u8 | RecordFlag::F_LAST as u8,
            2,
            10.into(),
            20.into(),
        );
        catalog
            .write_to_parquet(&[clear, buy], Some(19.into()), Some(20.into()), Some(true))
            .unwrap();
        catalog
            .write_to_parquet(&[sell], Some(20.into()), Some(21.into()), Some(true))
            .unwrap();
        let registry = factories(Window {
            root: dir.path().into(),
            initial,
            updates: Vec::new(),
            event_start: 10,
            event_end: 11,
            counts: RowCounts::default(),
        });
        let mut adapted = registry[BACKEND](
            &nautilus_persistence::catalog::factory::CatalogConnectConfig::new("unused", None),
        )
        .unwrap();
        let query = CatalogQuery::new(NautilusDataType::OrderBookDelta)
            .with_identifiers(Some(vec![id.to_string()]));
        let mut session = adapted.query_batch_session(&query, None).unwrap();
        let mut actual = Vec::new();
        while let Some(batch) = session.next_batch().unwrap() {
            if let DataBatch::BookDelta(rows) = batch {
                actual.extend_from_slice(rows.as_ref());
            }
        }
        assert_eq!(actual, vec![clear, buy, sell]);
        assert!(nautilus_model::data::OrderBookDeltas::new_checked(id, actual).is_ok());
    }

    #[test]
    fn definition_queries_preserve_original_baseline_and_cutoff() {
        let dir = tempfile::tempdir().unwrap();
        let mut baseline = currency_pair_btcusdt();
        baseline.ts_init = 10.into();
        baseline.ts_event = 9.into();
        let mut later = baseline.clone();
        later.ts_init = 30.into();
        later.ts_event = 29.into();
        let initial = InstrumentAny::CurrencyPair(baseline);
        let window = Window {
            root: dir.path().into(),
            initial: initial.clone(),
            updates: vec![InstrumentAny::CurrencyPair(later)],
            event_start: 0,
            event_end: 100,
            counts: RowCounts::default(),
        };
        let registry = factories(window.clone());
        let mut adapted = registry[BACKEND](
            &nautilus_persistence::catalog::factory::CatalogConnectConfig::new("unused", None),
        )
        .unwrap();
        let instruments = adapted.instruments(&CatalogInstrumentQuery::new()).unwrap();
        assert_eq!(instruments[0].ts_init(), initial.ts_init());
        assert_eq!(instruments[0].ts_event(), initial.ts_event());
        let query = CatalogQuery::new(NautilusDataType::Instrument)
            .with_range(Some(11.into()), Some(29.into()));
        assert!(adapted
            .query_batch_session(&query, None)
            .unwrap()
            .next_batch()
            .unwrap()
            .is_none());
        let query = CatalogQuery::new(NautilusDataType::Instrument)
            .with_range(Some(11.into()), Some(30.into()));
        let batch = adapted
            .query_batch_session(&query, None)
            .unwrap()
            .next_batch()
            .unwrap()
            .unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(
            nautilus_model::data::HasTsInit::ts_init(&batch.get(0).unwrap()).as_u64(),
            30
        );
        let registry = factories(Window {
            updates: Vec::new(),
            ..window
        });
        let mut static_catalog = registry[BACKEND](
            &nautilus_persistence::catalog::factory::CatalogConnectConfig::new("unused", None),
        )
        .unwrap();
        assert!(static_catalog
            .query_batch_session(&query, None)
            .unwrap()
            .next_batch()
            .unwrap()
            .is_none());
    }
    #[test]
    fn overlapping_file_ranges_include_equal_endpoints_and_chains() {
        assert_eq!(
            overlap_groups(vec![(30, 40), (2, 5), (5, 8), (7, 10), (11, 20), (40, 41)]),
            vec![(2, 10), (11, 20), (30, 41)]
        );
    }

    #[test]
    fn official_node_preserves_cross_group_frame_and_canonical_fills() {
        for explicit_start in [false, true] {
            verify_official_node(explicit_start, false, "baseline", false);
        }
    }

    #[test]
    fn official_node_preserves_tied_funding_and_explicit_or_inferred_start() {
        for explicit_start in [false, true] {
            verify_official_node(explicit_start, true, "baseline", false);
        }
    }

    #[test]
    fn official_node_preserves_static_and_historical_definitions() {
        for mode in ["historical", "static"] {
            for explicit_start in [false, true] {
                verify_official_node(explicit_start, true, mode, false);
            }
        }
    }

    #[test]
    fn selected_row_total_is_not_early_stop_prefetch_count() {
        verify_official_node(false, false, "baseline", true);
    }

    struct StopAtStart {
        core: nautilus_trading::strategy::StrategyCore,
    }
    impl StopAtStart {
        fn new() -> Self {
            Self {
                core: nautilus_trading::strategy::StrategyCore::new(
                    nautilus_trading::strategy::StrategyConfig {
                        strategy_id: Some("TEST-STOP-001".into()),
                        order_id_tag: Some("stop".into()),
                        ..Default::default()
                    },
                ),
            }
        }
    }
    nautilus_trading::nautilus_strategy!(StopAtStart);
    impl std::fmt::Debug for StopAtStart {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("StopAtStart")
        }
    }
    impl nautilus_common::actor::DataActor for StopAtStart {
        fn on_start(&mut self) -> anyhow::Result<()> {
            self.shutdown_system(Some("test-only early shutdown".into()));
            Ok(())
        }
    }

    fn verify_official_node(
        explicit_start: bool,
        funded: bool,
        definition_mode: &str,
        early_stop: bool,
    ) {
        use crate::native_strategy::registry::{build_strategy, CompiledNativeStrategy};
        use nautilus_backtest::{
            config::{
                BacktestDataConfig, BacktestEngineConfig, BacktestRunConfig, BacktestVenueConfig,
            },
            node::BacktestNode,
        };
        use nautilus_common::logging::logger::LoggerConfig;
        use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
        use nautilus_model::{
            data::BookOrder,
            enums::{AccountType, BookAction, BookType, OmsType, OrderSide},
            types::{Price, Quantity},
        };
        use nautilus_persistence::config::{CatalogBackendType, DataCatalogConfig};
        let dir = tempfile::tempdir().unwrap();
        let mut initial = if funded {
            InstrumentAny::CryptoPerpetual(
                nautilus_model::instruments::stubs::crypto_perpetual_ethusdt(),
            )
        } else {
            InstrumentAny::CurrencyPair(currency_pair_btcusdt())
        };
        let id = initial.id();
        let catalog = crate::catalog::open_catalog(dir.path()).unwrap();
        crate::catalog::currencies::write(dir.path(), std::slice::from_ref(&initial)).unwrap();
        let mut definitions = vec![initial.clone()];
        let mut updates = Vec::new();
        if definition_mode != "baseline" {
            let mut future = initial.clone();
            match &mut future {
                InstrumentAny::CurrencyPair(row) => {
                    row.ts_init = 2_000_000_000.into();
                    row.ts_event = 1_999_999_999.into();
                }
                InstrumentAny::CryptoPerpetual(row) => {
                    row.ts_init = 2_000_000_000.into();
                    row.ts_event = 1_999_999_999.into();
                }
                _ => unreachable!(),
            }
            definitions.push(future.clone());
            if definition_mode == "static" {
                initial = future;
            } else {
                updates.push(future);
            }
        }
        catalog.write_instruments(definitions).unwrap();
        for index in 0..3u64 {
            let ts = 1_000_000_000 + index * 1_000_000_000;
            let mut clear = OrderBookDelta::clear(id, index, ts.into(), ts.into());
            clear.order.price = Price::from("0.00");
            clear.order.size = Quantity::new(0.0, initial.size_precision());
            let buy = OrderBookDelta::new(
                id,
                BookAction::Add,
                BookOrder::new(
                    OrderSide::Buy,
                    Price::from("100.00"),
                    Quantity::new(10.0, initial.size_precision()),
                    0,
                ),
                RecordFlag::F_SNAPSHOT as u8,
                index,
                ts.into(),
                ts.into(),
            );
            let sell = OrderBookDelta::new(
                id,
                BookAction::Add,
                BookOrder::new(
                    OrderSide::Sell,
                    Price::from("101.00"),
                    Quantity::new(if funded { 1.0 } else { 0.1 }, initial.size_precision()),
                    0,
                ),
                RecordFlag::F_SNAPSHOT as u8 | RecordFlag::F_LAST as u8,
                index,
                ts.into(),
                (ts + 1).into(),
            );
            catalog
                .write_to_parquet(&[clear, buy], None, None, None)
                .unwrap();
            catalog.write_to_parquet(&[sell], None, None, None).unwrap();
        }
        let mut request: contracts::native_strategy::NativeStrategyBacktestRequestV1 = serde_json::from_value(serde_json::json!({"schema_version":1,"instrument_id":id.to_string(),
            "selectors":[{"data_type":"OrderBookDelta","identifier":id.to_string()}],
            "event_start_ns":"1000000000","event_end_ns":"10000000000","available_end_ns":"5000000000",
            "venue":{"account_type":"CASH","oms_type":"NETTING","book_type":"L2_MBP","starting_balances":["100000 USDT","1 BTC"],"base_currency":null,"leverage":"1","maker_fee":"0.001","taker_fee":"0.001"},
            "strategy":{"kind":"ORDER_BOOK_IMBALANCE","max_trade_size":"0.010000","trigger_min_size":"1","trigger_imbalance_ratio":"0.2","min_seconds_between_triggers":0.0,"time_in_force":"IOC","dry_run":false}})).unwrap();
        if explicit_start {
            request.available_start_ns = Some(contracts::DbCounter::new(1_000_000_000).unwrap());
        }
        if funded {
            use nautilus_model::data::{FundingRateUpdate, MarkPriceUpdate};
            let ts = 2_000_000_001u64;
            catalog
                .write_to_parquet(
                    &[MarkPriceUpdate::new(
                        id,
                        Price::from("100.00"),
                        ts.into(),
                        ts.into(),
                    )],
                    None,
                    None,
                    None,
                )
                .unwrap();
            catalog
                .write_to_parquet(
                    &[FundingRateUpdate::new(
                        id,
                        "0.01".parse().unwrap(),
                        None,
                        Some(ts.into()),
                        ts.into(),
                        ts.into(),
                    )],
                    None,
                    None,
                    None,
                )
                .unwrap();
            request.venue.account_type = "MARGIN".into();
            request.venue.starting_balances = vec!["100000 USDT".into()];
            request.strategy = serde_json::from_value(serde_json::json!({"kind":"ORDER_BOOK_IMBALANCE","max_trade_size":"1.000","trigger_min_size":"1","trigger_imbalance_ratio":"0.2","min_seconds_between_triggers":0.0,"time_in_force":"IOC","dry_run":false})).unwrap();
            for kind in ["MarkPriceUpdate", "FundingRateUpdate"] {
                request.selectors.push(
                    serde_json::from_value(
                        serde_json::json!({"data_type":kind,"identifier":id.to_string()}),
                    )
                    .unwrap(),
                );
            }
        }
        if definition_mode == "static" {
            request.instrument_definition = Some(
                contracts::native_strategy::NativeInstrumentDefinitionV1::StaticScenario {
                    ts_init_ns: contracts::DbCounter::new(initial.ts_init().as_u64()).unwrap(),
                    ts_event_ns: contracts::DbCounter::new(initial.ts_event().as_u64()).unwrap(),
                },
            );
        }
        // Compatibility preflight: count the entire selected window with the existing bounded
        // file reader. This is deliberately independent of node prefetch and early shutdown.
        let mut selected_total = 0u64;
        let query = CatalogQuery::new(NautilusDataType::OrderBookDelta)
            .with_identifiers(Some(vec![id.to_string()]))
            .with_range(Some(1_000_000_000.into()), Some(5_000_000_000.into()));
        let mut preflight = crate::catalog::file_batch_session(
            &catalog,
            query,
            Some(1_000_000_000..10_000_000_000),
            4096,
        )
        .unwrap();
        while let Some(batch) = preflight.next_batch().unwrap() {
            selected_total += batch.len() as u64;
        }
        assert_eq!(selected_total, 9);
        let root = dir.path().to_path_buf();
        let baseline_request = request.clone();
        let baseline = std::thread::spawn(move || {
            crate::native_strategy::preloaded_reference::run(&root, baseline_request).unwrap()
        })
        .join()
        .unwrap();
        let production_root = dir.path().to_path_buf();
        let production_request = request.clone();
        let production = std::thread::spawn(move || {
            crate::native_strategy::run(&production_root, production_request).unwrap()
        })
        .join()
        .unwrap();
        assert_eq!(
            serde_json::to_value(&production.instrument_definition).unwrap(),
            serde_json::to_value(&baseline.instrument_definition).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&production.data_rows_by_type).unwrap(),
            serde_json::to_value(&baseline.data_rows_by_type).unwrap()
        );
        assert!(production
            .notes
            .iter()
            .any(|note| note.starts_with("CATALOG_READ_CONSISTENCY_UNVERIFIED:")));
        assert!(production
            .notes
            .iter()
            .any(|note| note.starts_with("REPLAY_READER_ROWS_INCLUDING_PREFETCH:")));
        let mut production_report = production.canonical_result;
        assert_eq!(production_report["run"]["run_config_id"], "native-strategy");
        production_report["run"]["run_config_id"] = serde_json::Value::Null;
        let counts = RowCounts::default();
        let registry = factories(Window {
            root: dir.path().into(),
            initial,
            updates,
            event_start: 1_000_000_000,
            event_end: 10_000_000_000,
            counts: counts.clone(),
        });
        let venue = BacktestVenueConfig::builder()
            .name("BINANCE")
            .oms_type(OmsType::Netting)
            .account_type(if funded {
                AccountType::Margin
            } else {
                AccountType::Cash
            })
            .book_type(BookType::L2_MBP)
            .starting_balances(request.venue.starting_balances.clone())
            .default_leverage("1".parse().unwrap())
            .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::new(
                "0.001".parse().unwrap(),
                "0.001".parse().unwrap(),
            )))
            .build()
            .unwrap();
        let mut data = request
            .selectors
            .iter()
            .map(|selector| {
                BacktestDataConfig::builder()
                    .data_type(selector.data_type.parse().unwrap())
                    .catalog(DataCatalogConfig::new(
                        dir.path().to_str().unwrap().into(),
                        None,
                        Some(CatalogBackendType::External(BACKEND.into())),
                    ))
                    .instrument_id(id)
                    .start_time(1_000_000_000.into())
                    .batch_deltas(true)
                    .optimize_file_loading(false)
                    .build()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        if definition_mode == "historical" {
            data.insert(
                0,
                BacktestDataConfig::builder()
                    .data_type(NautilusDataType::Instrument)
                    .catalog(DataCatalogConfig::new(
                        dir.path().to_str().unwrap().into(),
                        None,
                        Some(CatalogBackendType::External(BACKEND.into())),
                    ))
                    .instrument_id(id)
                    .start_time(1_000_000_001.into())
                    .build()
                    .unwrap(),
            );
        }
        let config = BacktestRunConfig::builder()
            .id("prototype".into())
            .venues(vec![venue])
            .data(data)
            .engine(BacktestEngineConfig {
                logging: LoggerConfig::builder()
                    .bypass_logging(true)
                    .is_colored(false)
                    .build()
                    .unwrap(),
                bypass_logging: true,
                shutdown_on_error: true,
                ..Default::default()
            })
            .chunk_size(1)
            .maybe_start(request.available_start_ns.map(|v| v.get().into()))
            .end(5_000_000_000.into())
            .raise_exception(true)
            .dispose_on_completion(false)
            .build()
            .unwrap();
        let mut node = BacktestNode::builder(vec![config])
            .with_catalog_factories(registry)
            .build()
            .unwrap();
        node.build().unwrap();
        match build_strategy(id, &request.strategy).unwrap() {
            CompiledNativeStrategy::OrderBookImbalance(strategy) => node
                .get_engine_mut("prototype")
                .unwrap()
                .add_strategy(strategy)
                .unwrap(),
            _ => unreachable!(),
        }
        if early_stop {
            // Test-only lifecycle fault injection through the public upstream strategy API.
            // This is not registered in QuaZonai's production compiled strategy registry.
            node.get_engine_mut("prototype")
                .unwrap()
                .add_strategy(StopAtStart::new())
                .unwrap();
        }
        node.run().unwrap();
        let value = node
            .get_engine("prototype")
            .unwrap()
            .get_canonical_result()
            .unwrap()
            .as_value()
            .clone();
        if definition_mode != "baseline" {
            let engine = node.get_engine("prototype").unwrap();
            assert_eq!(
                engine
                    .kernel()
                    .cache
                    .borrow()
                    .instrument(&id)
                    .unwrap()
                    .ts_init()
                    .as_u64(),
                2_000_000_000
            );
        }
        node.dispose();
        if early_stop {
            assert_eq!(value["run"]["outcome"], "stopped");
            assert_eq!(value["run"]["iterations"], "0");
            let read = counts.lock().unwrap()["OrderBookDelta"];
            eprintln!(
                "EARLY_STOP selected={selected_total} read_prefetch={read} replay_iterations=0"
            );
            assert!(read < selected_total, "Prefetch must not be confused with selected-total rows; read={read} total={selected_total}");
            assert_eq!(
                baseline.data_rows_by_type["OrderBookDelta"].get(),
                selected_total
            );
            return;
        }
        assert_eq!(counts.lock().unwrap()["OrderBookDelta"], 9);
        assert_eq!(
            value["fills"].as_array().unwrap().len(),
            baseline.canonical_result["fills"].as_array().unwrap().len()
        );
        assert!(!value["fills"].as_array().unwrap().is_empty());
        fn economic_fills(value: &serde_json::Value) -> Vec<serde_json::Value> {
            value["fills"].as_array().unwrap().iter().map(|row| {
                let fill = &row["event"]["Filled"];
                assert!(fill["last_px"].is_string() && fill["last_qty"].is_string() && fill["commission"].is_string());
                serde_json::json!({"side":fill["order_side"],"price":fill["last_px"],"qty":fill["last_qty"],"commission":fill["commission"],"ts_event":fill["ts_event"],"ts_init":fill["ts_init"]})
            }).collect()
        }
        assert_eq!(
            economic_fills(&value),
            economic_fills(&baseline.canonical_result)
        );
        let mut canonical = value;
        assert_eq!(canonical["run"]["run_config_id"], "prototype");
        canonical["run"]["run_config_id"] = serde_json::Value::Null;
        let mut expected = baseline.canonical_result;
        if funded {
            fn identities(
                value: &mut serde_json::Value,
                ids: &mut Vec<String>,
                occurrences: &mut usize,
            ) {
                match value {
                    serde_json::Value::Object(fields) => {
                        if let Some(serde_json::Value::String(reason)) = fields.get_mut("reason") {
                            if let Some(id) = reason.strip_prefix("funding_settlement:") {
                                let _: nautilus_core::UUID4 = id
                                    .parse()
                                    .expect("Only upstream UUID4 identity is normalized");
                                let index =
                                    ids.iter().position(|known| known == id).unwrap_or_else(|| {
                                        ids.push(id.to_owned());
                                        ids.len() - 1
                                    });
                                *reason = format!("funding_settlement:identity-{index}");
                                *occurrences += 1;
                            }
                        }
                        for child in fields.values_mut() {
                            identities(child, ids, occurrences);
                        }
                    }
                    serde_json::Value::Array(items) => {
                        for child in items {
                            identities(child, ids, occurrences);
                        }
                    }
                    _ => {}
                }
            }
            let mut occurrences = Vec::new();
            for report in [&mut canonical, &mut expected, &mut production_report] {
                let mut ids = Vec::new();
                let mut count = 0;
                identities(report, &mut ids, &mut count);
                assert_eq!(ids.len(), 1);
                assert!(count > 0);
                occurrences.push(count);
            }
            assert_eq!(occurrences[0], occurrences[1]);
            assert_eq!(occurrences[0], occurrences[2]);
            assert_eq!(counts.lock().unwrap()["FundingRateUpdate"], 1);
        }
        let suffix = format!("{explicit_start}-{funded}-{definition_mode}");
        std::fs::write(
            format!("/tmp/qz-node-canonical-stream-{suffix}.json"),
            serde_json::to_vec_pretty(&canonical).unwrap(),
        )
        .unwrap();
        std::fs::write(
            format!("/tmp/qz-node-canonical-preload-{suffix}.json"),
            serde_json::to_vec_pretty(&expected).unwrap(),
        )
        .unwrap();
        assert!(
            canonical == expected,
            "Full canonical mismatch ({suffix}): inspect recorded JSON pair"
        );
        assert_eq!(
            production_report, expected,
            "Production wrapper must preserve the full canonical result ({suffix})"
        );
    }
    #[test]
    fn concurrent_import_is_reported_without_snapshot_claim() {
        let dir = tempfile::tempdir().unwrap();
        let initial = InstrumentAny::CurrencyPair(currency_pair_btcusdt());
        let id = initial.id();
        let catalog = crate::catalog::open_catalog(dir.path()).unwrap();
        let row = |ts: u64, seq| {
            let mut value = OrderBookDelta::clear(id, seq, ts.into(), ts.into());
            value.flags |= RecordFlag::F_LAST as u8;
            value
        };
        catalog
            .write_to_parquet(&[row(10, 0)], None, None, None)
            .unwrap();
        catalog
            .write_to_parquet(&[row(30, 1)], None, None, None)
            .unwrap();
        let query = CatalogQuery::new(NautilusDataType::OrderBookDelta)
            .with_identifiers(Some(vec![id.to_string()]));
        let mut scan =
            crate::catalog::file_batch_session(&catalog, query.clone(), Some(0..100), 4096)
                .unwrap();
        let mut scanned = 0;
        while let Some(batch) = scan.next_batch().unwrap() {
            scanned += batch.len() as u64;
        }
        let counts = RowCounts::default();
        let registry = factories(Window {
            root: dir.path().into(),
            initial,
            updates: Vec::new(),
            event_start: 0,
            event_end: 100,
            counts: counts.clone(),
        });
        let mut adapted = registry[BACKEND](
            &nautilus_persistence::catalog::factory::CatalogConnectConfig::new("unused", None),
        )
        .unwrap();
        let mut session = adapted.query_batch_session(&query, None).unwrap();
        assert_eq!(session.next_batch().unwrap().unwrap().len(), 1);
        // Imports after range discovery: a new gap is not discovered, while a new file
        // overlapping a not-yet-opened group is included by the official Latest query.
        catalog
            .write_to_parquet(&[row(20, 99)], None, None, None)
            .unwrap();
        catalog
            .write_to_parquet(&[row(30, 2)], Some(29.into()), Some(30.into()), Some(true))
            .unwrap();
        let mut later = Vec::new();
        while let Some(batch) = session.next_batch().unwrap() {
            if let DataBatch::BookDelta(rows) = batch {
                later.extend(rows.as_ref().iter().map(|value| value.sequence));
            }
        }
        assert_eq!(later, vec![2, 1]);
        let pre_scan = [(
            "OrderBookDelta".to_owned(),
            contracts::DbCounter::new(scanned).unwrap(),
        )]
        .into_iter()
        .collect();
        let notes = row_count_notes(&pre_scan, &counts).unwrap();
        assert_eq!(scanned, 2);
        assert_eq!(counts.lock().unwrap()["OrderBookDelta"], 3);
        assert!(notes
            .iter()
            .any(|note| note.contains("\"OrderBookDelta\":\"3\"")));
        assert!(notes
            .iter()
            .any(|note| note.starts_with("PRE_SCAN_REPLAY_READER_COUNTS_DIFFER:")));
        assert!(notes
            .iter()
            .any(|note| note.contains("new files may or may not be included")));
    }

    #[test]
    fn equal_read_counts_do_not_claim_identical_catalog_contents() {
        let dir = tempfile::tempdir().unwrap();
        let initial = InstrumentAny::CurrencyPair(currency_pair_btcusdt());
        let id = initial.id();
        let catalog = crate::catalog::open_catalog(dir.path()).unwrap();
        let row = |ts: u64, seq| {
            let mut value = OrderBookDelta::clear(id, seq, ts.into(), ts.into());
            value.flags |= RecordFlag::F_LAST as u8;
            value
        };
        catalog
            .write_to_parquet(&[row(10, 0)], None, None, None)
            .unwrap();
        let later_path = catalog
            .write_to_parquet(&[row(30, 1)], None, None, None)
            .unwrap();
        let query = CatalogQuery::new(NautilusDataType::OrderBookDelta)
            .with_identifiers(Some(vec![id.to_string()]));
        let mut scan =
            crate::catalog::file_batch_session(&catalog, query.clone(), Some(0..100), 4096)
                .unwrap();
        let mut scanned = 0;
        while let Some(batch) = scan.next_batch().unwrap() {
            scanned += batch.len() as u64;
        }
        let counts = RowCounts::default();
        let registry = factories(Window {
            root: dir.path().into(),
            initial,
            updates: Vec::new(),
            event_start: 0,
            event_end: 100,
            counts: counts.clone(),
        });
        let mut adapted = registry[BACKEND](
            &nautilus_persistence::catalog::factory::CatalogConnectConfig::new("unused", None),
        )
        .unwrap();
        let mut session = adapted.query_batch_session(&query, None).unwrap();
        session.next_batch().unwrap().unwrap();
        // Replace only this test's temporary native file, retaining timestamp and row count.
        std::fs::remove_file(catalog.reconstruct_full_uri(later_path.to_str().unwrap())).unwrap();
        catalog
            .write_to_parquet(&[row(30, 999)], None, None, None)
            .unwrap();
        let batch = session.next_batch().unwrap().unwrap();
        if let DataBatch::BookDelta(rows) = batch {
            assert_eq!(rows.as_ref()[0].sequence, 999);
        } else {
            panic!("wrong type");
        }
        assert!(session.next_batch().unwrap().is_none());
        let pre_scan = [(
            "OrderBookDelta".to_owned(),
            contracts::DbCounter::new(scanned).unwrap(),
        )]
        .into_iter()
        .collect();
        let notes = row_count_notes(&pre_scan, &counts).unwrap();
        assert_eq!(scanned, 2);
        assert_eq!(counts.lock().unwrap()["OrderBookDelta"], 2);
        assert!(!notes
            .iter()
            .any(|note| note.starts_with("PRE_SCAN_REPLAY_READER_COUNTS_DIFFER:")));
        assert!(notes
            .iter()
            .any(|note| note.contains("equal counts do not prove identical input")));
        assert!(notes
            .iter()
            .any(|note| note.contains("folds and candidates may see different contents")));
    }
}
