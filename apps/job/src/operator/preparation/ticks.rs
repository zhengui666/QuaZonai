//! Physically separate native tick research/independent catalogs; metadata last.
use anyhow::{ensure, Context, Result};
use clap::Parser;
use contracts::ticks::NativeTradeTickFactorEvaluationRequestV1;
use nautilus_model::{
    data::TradeTick,
    instruments::Instrument,
    types::{Price, Quantity},
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(about = "Prepare isolated native TradeTick research inputs; no PIT qualification")]
pub(super) struct Arguments {
    #[arg(long)]
    catalog: PathBuf,
    #[arg(long)]
    request: PathBuf,
    #[arg(long)]
    output: PathBuf,
    /// Aggregate logical bytes across both isolated catalogs and final metadata.
    #[arg(long, default_value_t = 10 * 1024 * 1024 * 1024_u64)]
    maximum_output_bytes: u64,
}

struct OutputBudget<'a> {
    root: &'a Path,
    maximum: u64,
    files: BTreeMap<PathBuf, u64>,
    written: u64,
}

impl OutputBudget<'_> {
    fn record(&mut self, path: &Path) -> Result<()> {
        ensure!(
            path.starts_with(self.root),
            "TICK_PREPARATION_OUTPUT_INVALID"
        );
        let file = fs::symlink_metadata(path)?;
        ensure!(
            file.is_file() && !file.file_type().is_symlink(),
            "TICK_PREPARATION_OUTPUT_INVALID"
        );
        let previous = self.files.insert(path.to_owned(), file.len()).unwrap_or(0);
        self.written = self
            .written
            .checked_sub(previous)
            .and_then(|bytes| bytes.checked_add(file.len()))
            .context("TICK_PREPARATION_OUTPUT_LIMIT")?;
        ensure!(
            self.written <= self.maximum,
            "TICK_PREPARATION_OUTPUT_LIMIT"
        );
        Ok(())
    }

    fn measured(&self) -> Result<u64> {
        fn bytes(directory: &Path) -> Result<u64> {
            let mut total = 0_u64;
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                ensure!(!kind.is_symlink(), "TICK_PREPARATION_OUTPUT_INVALID");
                let value = if kind.is_dir() {
                    bytes(&entry.path())?
                } else {
                    ensure!(kind.is_file(), "TICK_PREPARATION_OUTPUT_INVALID");
                    entry.metadata()?.len()
                };
                total = total
                    .checked_add(value)
                    .context("TICK_PREPARATION_OUTPUT_LIMIT")?;
            }
            Ok(total)
        }
        bytes(self.root)
    }

    fn check(&self) -> Result<u64> {
        let measured = self.measured()?;
        ensure!(measured <= self.maximum, "TICK_PREPARATION_OUTPUT_LIMIT");
        Ok(measured)
    }
}

fn publish(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    writeln!(file)?;
    file.sync_all()?;
    Ok(())
}

fn write_series(
    catalog: &nautilus_persistence::backend::catalog::ParquetDataCatalog,
    root: &Path,
    original: &[TradeTick],
    budget: &mut OutputBudget<'_>,
) -> Result<()> {
    let mut encoded = original.to_vec();
    // Equal-availability native rows share a physical filename and metadata.
    // Preserve scaled raw values and normalize only their encoding precision.
    let mut start = 0;
    while start < encoded.len() {
        let end = encoded[start..]
            .iter()
            .position(|trade| trade.ts_init != encoded[start].ts_init)
            .map_or(encoded.len(), |offset| start + offset);
        let prices = encoded[start..end]
            .iter()
            .map(|t| t.price.precision)
            .max()
            .unwrap();
        let sizes = encoded[start..end]
            .iter()
            .map(|t| t.size.precision)
            .max()
            .unwrap();
        for trade in &mut encoded[start..end] {
            trade.price = Price::from_raw(trade.price.raw, prices);
            trade.size = Quantity::from_raw(trade.size.raw, sizes);
        }
        start = end;
    }
    let mut start = 0;
    while start < encoded.len() {
        let metadata = (
            encoded[start].price.precision,
            encoded[start].size.precision,
        );
        let mut end = start + 1;
        while end < encoded.len()
            && (encoded[end].price.precision, encoded[end].size.precision) == metadata
            && (end - start < 100_000 || encoded[end].ts_init == encoded[end - 1].ts_init)
        {
            end += 1;
        }
        let path = catalog.write_to_parquet(&encoded[start..end], None, None, None)?;
        budget.record(&if path.is_absolute() {
            path
        } else {
            root.join(path)
        })?;
        start = end;
    }
    Ok(())
}

fn same_rows(
    original: &BTreeMap<String, Vec<TradeTick>>,
    readback: &BTreeMap<String, Vec<TradeTick>>,
) -> bool {
    original.len() == readback.len()
        && original.iter().all(|(id, rows)| {
            readback.get(id).is_some_and(|actual| {
                rows.len() == actual.len()
                    && rows.iter().zip(actual).all(|(left, right)| {
                        left.instrument_id == right.instrument_id
                            && left.trade_id == right.trade_id
                            && left.price.raw == right.price.raw
                            && left.size.raw == right.size.raw
                            && left.aggressor_side == right.aggressor_side
                            && left.ts_event == right.ts_event
                            && left.ts_init == right.ts_init
                    })
            })
        })
}

fn copy_selected(
    source: &Path,
    root: &Path,
    request: &NativeTradeTickFactorEvaluationRequestV1,
    rows: &BTreeMap<String, Vec<TradeTick>>,
    budget: &mut OutputBudget<'_>,
) -> Result<()> {
    fs::create_dir_all(root)?;
    let original = crate::catalog::open_catalog(source)?;
    let mut definitions = original.instruments(Some(&request.input.instrument_ids), None, None)?;
    definitions.retain(|instrument| {
        request
            .input
            .instrument_ids
            .contains(&instrument.id().to_string())
    });
    definitions.sort_by_key(|instrument| (instrument.id(), instrument.ts_init()));
    let native = super::native(root)?;
    crate::catalog::currencies::write(root, &definitions)?;
    budget.record(&root.join("currencies.json"))?;
    for path in native.write_instruments(definitions.clone())? {
        budget.record(&if path.is_absolute() {
            path
        } else {
            root.join(path)
        })?;
    }
    for selected in rows.values() {
        write_series(&native, root, selected, budget)?;
    }
    let mut provenance = crate::ticks::catalog::source_provenance(source)?;
    if provenance.get("source_native_files").is_none() {
        provenance["source_native_files"] = provenance["native_files"].clone();
    }
    provenance["native_files"] =
        serde_json::to_value(crate::ticks::catalog::native_inventory(root)?)?;
    let trade_counts: BTreeMap<_, _> = rows.iter().map(|(id, rows)| (id, rows.len())).collect();
    provenance["native_prepared_selection"] = json!({
        "instrument_ids": request.input.instrument_ids,
        "event_start_ns": request.input.event_start_ns.get(),
        "event_end_ns": request.input.event_end_ns.get(),
        "trade_counts": trade_counts,
    });
    publish(&root.join("import-provenance.json"), &provenance)?;
    budget.record(&root.join("import-provenance.json"))?;
    ensure!(
        same_rows(rows, &crate::ticks::catalog::load(root, request)?),
        "TICK_PREPARATION_READBACK_MISMATCH"
    );
    let mut actual = native.instruments(Some(&request.input.instrument_ids), None, None)?;
    actual.sort_by_key(|instrument| (instrument.id(), instrument.ts_init()));
    ensure!(
        serde_json::to_value(actual)? == serde_json::to_value(definitions)?,
        "TICK_PREPARATION_DEFINITION_READBACK_MISMATCH"
    );
    Ok(())
}

pub(super) fn run(args: &Arguments) -> Result<Value> {
    ensure!(
        args.maximum_output_bytes > 0,
        "TICK_PREPARATION_OUTPUT_LIMIT"
    );
    let request: NativeTradeTickFactorEvaluationRequestV1 = super::read_json(&args.request)?;
    domain::ticks::evaluation_request(&request)?;
    if request.input.is_cohort_cache() {
        return crate::ticks::cohort_cache::prepare_views(
            &args.catalog,
            &request,
            &args.output,
            args.maximum_output_bytes,
        );
    }
    let source = crate::catalog::catalog_root(&args.catalog)?;
    let parent = args
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let output = parent.join(
        args.output
            .file_name()
            .context("TICK_OUTPUT_DIRECTORY_REQUIRED")?,
    );
    ensure!(
        !output.starts_with(&source),
        "OUTPUT_MUST_NOT_OVERLAP_SOURCE"
    );
    let series = crate::ticks::catalog::load(&source, &request)?;
    for rows in series.values() {
        ensure!(
            rows.chunk_by(|left, right| left.ts_event == right.ts_event)
                .all(|cohort| cohort.len() <= request.sampling.maximum_cohort_rows as usize),
            "TICK_COHORT_ROW_LIMIT"
        );
    }
    let mut coverage = crate::ticks::coverage(&request, &series)?;
    coverage.source_evidence = crate::ticks::catalog::source_evidence(&source, &request)?;
    let mut research_request = request.clone();
    research_request.input.event_end_ns = request.validation_end_ns;
    let research: BTreeMap<_, _> = series
        .iter()
        .map(|(id, rows)| {
            (
                id.clone(),
                rows.iter()
                    .copied()
                    .filter(|t| t.ts_event.as_u64() < request.validation_end_ns.get())
                    .collect(),
            )
        })
        .collect();
    ensure!(
        research
            .values()
            .any(|rows: &Vec<TradeTick>| !rows.is_empty()),
        "TICK_RESEARCH_SELECTION_EMPTY"
    );
    fs::create_dir(&output).context("OUTPUT_MUST_BE_NEW")?;
    let mut budget = OutputBudget {
        root: &output,
        maximum: args.maximum_output_bytes,
        files: BTreeMap::new(),
        written: 0,
    };
    copy_selected(
        &source,
        &output.join("research/catalog"),
        &research_request,
        &research,
        &mut budget,
    )?;
    copy_selected(
        &source,
        &output.join("independent/catalog"),
        &request,
        &series,
        &mut budget,
    )?;
    let mut metadata = json!({
        "schema_version":1,"modality":"TRADE_TICK",
        "request":request,"research_request":research_request,"coverage":coverage,
        "catalog_format":if cfg!(feature="high-precision") { "NAUTILUS_HIGH_PRECISION_16" } else { "NAUTILUS_STANDARD_9" },
        "operational_state":"NATIVE_PREPARED","scientific_qualification":"UNQUALIFIED_EXPLORATORY",
        "research_catalog":"research/catalog","independent_catalog":"independent/catalog",
        "maximum_output_bytes":args.maximum_output_bytes.to_string(),"output_bytes":"0",
    });
    let catalog_bytes = budget.check()?;
    // Include the metadata's own bytes, resolving the decimal-length fixed point
    // before publishing anything that can be mistaken for a completed input.
    let measured = loop {
        let metadata_bytes = u64::try_from(serde_json::to_vec_pretty(&metadata)?.len())?
            .checked_add(1)
            .context("TICK_PREPARATION_OUTPUT_LIMIT")?;
        let measured = catalog_bytes
            .checked_add(metadata_bytes)
            .context("TICK_PREPARATION_OUTPUT_LIMIT")?;
        ensure!(
            measured <= args.maximum_output_bytes,
            "TICK_PREPARATION_OUTPUT_LIMIT"
        );
        if metadata["output_bytes"].as_str() == Some(measured.to_string().as_str()) {
            break measured;
        }
        metadata["output_bytes"] = json!(measured.to_string());
    };
    publish(&output.join("catalog-metadata.json.partial"), &metadata)?;
    fs::hard_link(
        output.join("catalog-metadata.json.partial"),
        output.join("catalog-metadata.json"),
    )?;
    let _ = fs::remove_file(output.join("catalog-metadata.json.partial"));
    match budget.check() {
        Ok(actual) if actual == measured => {}
        other => {
            let _ = fs::remove_file(output.join("catalog-metadata.json"));
            other?;
            anyhow::bail!("TICK_PREPARATION_OUTPUT_CHANGED");
        }
    }
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        enums::AggressorSide,
        identifiers::{InstrumentId, TradeId},
        instruments::{stubs::audusd_sim, InstrumentAny},
    };

    fn request() -> NativeTradeTickFactorEvaluationRequestV1 {
        serde_json::from_value(json!({
            "schema_version":1,
            "input":{"modality":"TRADE_TICK","instrument_ids":["AUD/USD.SIM"],
                "event_start_ns":"0","event_end_ns":"80","decision_cutoff_ns":"80",
                "maximum_rows":100,"source_revision":"native-test-source",
                "availability":"EVENT_TIME_PROXY_UNVERIFIED"},
            "sampling":{"kind":"EVENT_TIME_COHORT_V1","maximum_cohort_rows":100},
            "target":{"kind":"FIXED_TRADE_COHORTS","horizon_cohorts":1},
            "parameters":{"fast_period_cohorts":1,"slow_period_cohorts":2},
            "train_end_ns":"30","validation_end_ns":"50"
        }))
        .unwrap()
    }

    #[test]
    fn native_preparation_physically_excludes_heldout_and_preserves_original_trades() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("original");
        fs::create_dir(&source).unwrap();
        let native = super::super::native(&source).unwrap();
        let instrument = InstrumentAny::CurrencyPair(audusd_sim());
        crate::catalog::currencies::write(&source, std::slice::from_ref(&instrument)).unwrap();
        native.write_instruments(vec![instrument]).unwrap();
        #[cfg(feature = "high-precision")]
        let (price, size) = (Price::from("1.2345678901"), Quantity::from("2.0000000001"));
        #[cfg(not(feature = "high-precision"))]
        let (price, size) = (Price::from("1.23456789"), Quantity::from("2.00000001"));
        let mut trades: Vec<_> = (1..=7_u64)
            .map(|i| {
                TradeTick::new(
                    InstrumentId::from("AUD/USD.SIM"),
                    price,
                    size,
                    AggressorSide::Buy,
                    TradeId::from(format!("original-{i}").as_str()),
                    (i * 10).into(),
                    (i * 10).into(),
                )
            })
            .collect();
        let mut tied = trades[0];
        tied.trade_id = TradeId::from("original-extra");
        trades.insert(1, tied);
        native.write_to_parquet(&trades, None, None, None).unwrap();
        let native_files = crate::ticks::catalog::native_inventory(&source).unwrap();
        publish(&source.join("import-provenance.json"), &json!({
            "schema_version":1,"status":"IMPORTED","source_revision":"native-test-source",
            "missing_symbols":{},"missing_definitions":{},"rejected_rows":{},
            "missing_partitions":[],"duplicate_trades":0,"readback_records":8,
            "event_start_ns":0,"event_end_ns":80,"historical_availability":"UNVERIFIED; event-time proxy",
            "native_files":native_files,
        })).unwrap();
        let request_file = temp.path().join("request.json");
        publish(&request_file, &request()).unwrap();
        let output = temp.path().join("prepared");
        let args = Arguments {
            catalog: source.clone(),
            request: request_file,
            output: output.clone(),
            maximum_output_bytes: 10 * 1024 * 1024 * 1024,
        };
        let metadata = run(&args).unwrap();
        assert_eq!(metadata["operational_state"], "NATIVE_PREPARED");
        assert_eq!(
            metadata["scientific_qualification"],
            "UNQUALIFIED_EXPLORATORY"
        );
        let measured = OutputBudget {
            root: &output,
            maximum: u64::MAX,
            files: BTreeMap::new(),
            written: 0,
        }
        .measured()
        .unwrap();
        assert_eq!(metadata["output_bytes"], measured.to_string());
        assert_eq!(metadata["maximum_output_bytes"], "10737418240");
        let research_request =
            serde_json::from_value(metadata["research_request"].clone()).unwrap();
        let research =
            crate::ticks::catalog::load(&output.join("research/catalog"), &research_request)
                .unwrap();
        assert_eq!(research["AUD/USD.SIM"].len(), 5);
        assert!(research["AUD/USD.SIM"]
            .iter()
            .all(|t| t.ts_event.as_u64() < 50));
        let mut public = super::super::native(&output.join("research/catalog")).unwrap();
        let physical = public
            .query::<TradeTick>(None, None, None, None, None, false)
            .unwrap();
        let physical = physical.map(|row| row.unwrap()).collect::<Vec<_>>();
        assert_eq!(physical.len(), 5);
        assert!(physical.iter().all(
            |row| matches!(row, nautilus_model::data::Data::Trade(t) if t.ts_event.as_u64() < 50)
        ));
        assert!(crate::ticks::catalog::load(&output.join("research/catalog"), &request()).is_err());
        let independent =
            crate::ticks::catalog::load(&output.join("independent/catalog"), &request()).unwrap();
        assert_eq!(independent["AUD/USD.SIM"], trades);
        assert!(output.join("catalog-metadata.json").is_file());
        assert!(!output.join("research/catalog/data/bars").exists());
        assert!(run(&args)
            .unwrap_err()
            .to_string()
            .contains("OUTPUT_MUST_BE_NEW"));
        assert_eq!(
            crate::ticks::catalog::load(&source, &request()).unwrap()["AUD/USD.SIM"],
            trades
        );
        let research_bytes = OutputBudget {
            root: &output.join("research"),
            maximum: u64::MAX,
            files: BTreeMap::new(),
            written: 0,
        }
        .measured()
        .unwrap();
        let independent_bytes = OutputBudget {
            root: &output.join("independent"),
            maximum: u64::MAX,
            files: BTreeMap::new(),
            written: 0,
        }
        .measured()
        .unwrap();
        let aggregate_limit = research_bytes.max(independent_bytes) + 1;
        assert!(aggregate_limit < research_bytes + independent_bytes);
        let aggregate = Arguments {
            catalog: source.clone(),
            request: args.request.clone(),
            output: temp.path().join("aggregate-limited"),
            maximum_output_bytes: aggregate_limit,
        };
        assert!(run(&aggregate)
            .unwrap_err()
            .to_string()
            .contains("TICK_PREPARATION_OUTPUT_LIMIT"));
        assert!(!aggregate.output.join("catalog-metadata.json").exists());
        let mut oversized_request = request();
        oversized_request.sampling.maximum_cohort_rows = 1;
        let oversized_file = temp.path().join("oversized-request.json");
        publish(&oversized_file, &oversized_request).unwrap();
        let oversized = Arguments {
            catalog: source.clone(),
            request: oversized_file,
            output: temp.path().join("oversized"),
            maximum_output_bytes: 10 * 1024 * 1024 * 1024,
        };
        assert!(run(&oversized)
            .unwrap_err()
            .to_string()
            .contains("TICK_COHORT_ROW_LIMIT"));
        assert!(!oversized.output.exists());
        let limited = Arguments {
            catalog: source.clone(),
            request: args.request.clone(),
            output: temp.path().join("limited"),
            maximum_output_bytes: 1,
        };
        assert!(run(&limited)
            .unwrap_err()
            .to_string()
            .contains("TICK_PREPARATION_OUTPUT_LIMIT"));
        assert!(!limited.output.join("catalog-metadata.json").exists());
        assert_eq!(
            crate::ticks::catalog::load(&source, &request()).unwrap()["AUD/USD.SIM"],
            trades
        );
        let mut provenance = crate::ticks::catalog::source_provenance(&source).unwrap();
        provenance["status"] = json!("INCOMPLETE");
        fs::write(
            source.join("import-provenance.json"),
            serde_json::to_vec(&provenance).unwrap(),
        )
        .unwrap();
        let failed = Arguments {
            catalog: source,
            request: args.request,
            output: temp.path().join("failed"),
            maximum_output_bytes: 10 * 1024 * 1024 * 1024,
        };
        assert!(run(&failed).is_err());
        assert!(!failed.output.join("catalog-metadata.json").exists());
        if let Some(destination) = std::env::var_os("QZ_TICK_NATIVE_FIXTURE_DIR") {
            export_connected_fixture(Path::new(&destination));
        }
    }

    /// Explicit test-only export for the connected real HTTP/Durable/native check.
    /// The destination must be new; this never overwrites an existing archive.
    fn export_connected_fixture(destination: &Path) {
        fs::create_dir(destination).unwrap();
        let source = destination.join("source");
        fs::create_dir(&source).unwrap();
        let native = super::super::native(&source).unwrap();
        let instrument = InstrumentAny::CurrencyPair(audusd_sim());
        publish(
            &destination.join("instrument-definitions.json"),
            &vec![instrument.clone()],
        )
        .unwrap();
        crate::catalog::currencies::write(&source, std::slice::from_ref(&instrument)).unwrap();
        native.write_instruments(vec![instrument]).unwrap();
        let mut selected = request();
        selected.input.event_end_ns = contracts::DbCounter::new(33_000_000_000).unwrap();
        selected.input.decision_cutoff_ns = selected.input.event_end_ns;
        selected.train_end_ns = contracts::DbCounter::new(11_000_000_000).unwrap();
        selected.validation_end_ns = contracts::DbCounter::new(22_000_000_000).unwrap();
        let trades = (1..=32_u64)
            .map(|i| {
                #[cfg(feature = "high-precision")]
                let (price, size) = (
                    format!("1.{:010}", 2_345_678_900_u64 + i * 1_000_000),
                    Quantity::from("2.0000000001"),
                );
                #[cfg(not(feature = "high-precision"))]
                let (price, size) = (
                    format!("1.{:09}", 234_567_800_u64 + i * 100_000),
                    Quantity::from("2.00000001"),
                );
                TradeTick::new(
                    InstrumentId::from("AUD/USD.SIM"),
                    Price::from(price.as_str()),
                    size,
                    AggressorSide::Buy,
                    TradeId::from(format!("controlled-{i}").as_str()),
                    (i * 1_000_000_000).into(),
                    (i * 1_000_000_000).into(),
                )
            })
            .collect::<Vec<_>>();
        native.write_to_parquet(&trades, None, None, None).unwrap();
        let inventory = crate::ticks::catalog::native_inventory(&source).unwrap();
        publish(&source.join("import-provenance.json"),&json!({
            "schema_version":1,"status":"IMPORTED","source_revision":"native-test-source",
            "source_reference":"CONTROLLED_NATIVE_SOFTWARE_FIXTURE_NOT_MARKET_DATA",
            "missing_symbols":{},"missing_definitions":{},"rejected_rows":{},"missing_partitions":[],"duplicate_trades":0,"readback_records":32,
            "event_start_ns":0,"event_end_ns":33_000_000_000_u64,"historical_availability":"UNVERIFIED; event-time proxy","native_files":inventory,
        })).unwrap();
        let request_file = destination.join("request.json");
        publish(&request_file, &selected).unwrap();
        let metadata = run(&Arguments {
            catalog: source,
            request: request_file,
            output: destination.join("prepared"),
            maximum_output_bytes: 10 * 1024 * 1024 * 1024,
        })
        .unwrap();
        assert_eq!(metadata["coverage"]["trade_count"], "32");
        let mut cohort_request = selected.clone();
        cohort_request.input.representation =
            Some(contracts::ticks::NativeTickRepresentationV1::EventTimeCohortCacheV1);
        cohort_request.input.maximum_rows = None;
        cohort_request.input.maximum_source_rows = Some(contracts::DbCounter::new(100).unwrap());
        let mut builder = crate::ticks::cohort_cache::Builder::new(
            &destination.join("cohort-source"),
            100,
            100,
            10_000_000,
        )
        .unwrap();
        for trade in &trades {
            builder.push(*trade).unwrap();
        }
        let cached = builder
            .seal("native-test-source", 0, 33_000_000_000)
            .unwrap();
        builder.publish(cached, json!({
            "status":"IMPORTED","source_revision":"native-test-source","availability":"EVENT_TIME_PROXY_UNVERIFIED",
            "source_reference":"CONTROLLED_NATIVE_SOFTWARE_FIXTURE_NOT_MARKET_DATA",
            "missing_symbols":{},"missing_definitions":{},"rejected_rows":{},"missing_partitions":[],"duplicate_trades":0,
            "event_start_ns":"0","event_end_ns":"33000000000"
        })).unwrap();
        let cohort_request_file = destination.join("cohort-request.json");
        publish(&cohort_request_file, &cohort_request).unwrap();
        let cohort_metadata = run(&Arguments {
            catalog: destination.join("cohort-source/catalog"),
            request: cohort_request_file,
            output: destination.join("cohort-prepared"),
            maximum_output_bytes: 10_000_000,
        })
        .unwrap();
        assert_eq!(cohort_metadata["coverage"]["trade_count"], "32");
        let model = r#"(module
            (global $value (mut f64) (f64.const 0))
            (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)
                local.get 0 i32.eqz if local.get 1 global.set $value end)
            (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64) global.get $value))"#;
        fs::write(destination.join("model.wat"), model).unwrap();
        fs::write(
            destination.join("model.wasm"),
            wat::parse_str(model).unwrap(),
        )
        .unwrap();
        fs::write(destination.join("SOFTWARE_FIXTURE.txt"),"Controlled native software fixture. Not real Hyperliquid data, market completeness, PIT evidence or profitability.\n").unwrap();
    }

    #[test]
    fn encoding_precision_normalization_preserves_native_numbers_ids_and_clocks() {
        let original = TradeTick::new(
            InstrumentId::from("AUD/USD.SIM"),
            Price::from("1.20"),
            Quantity::from("1.000"),
            AggressorSide::Buy,
            TradeId::from("original"),
            10_u64.into(),
            10_u64.into(),
        );
        let mut encoded = original;
        encoded.price = Price::from_raw(original.price.raw, 4);
        encoded.size = Quantity::from_raw(original.size.raw, 5);
        let one = BTreeMap::from([("AUD/USD.SIM".into(), vec![original])]);
        let two = BTreeMap::from([("AUD/USD.SIM".into(), vec![encoded])]);
        assert!(same_rows(&one, &two));
        let mut changed = two;
        changed.get_mut("AUD/USD.SIM").unwrap()[0].ts_init = 11_u64.into();
        assert!(!same_rows(&one, &changed));
    }
}
