//! Offline operator preparation; original provenance is declared, native facts are measured.
use anyhow::{ensure, Context, Result};
use clap::Parser;
use contracts::{
    catalogs::RuntimeCatalogMetadataV1, execution::NativeDatasetSelectionV1,
    research::DataPartition,
};
use job::catalog::{measure_catalog, quality_report, NativeMarketData};
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Data, InstrumentClose},
    instruments::{Instrument, InstrumentAny},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_JSON_BYTES: u64 = 1024 * 1024;

#[derive(Parser)]
#[command(
    version,
    about = "Prepare an isolated native BAR catalog without certifying its source"
)]
struct Arguments {
    /// Original local native catalog; never modified.
    #[arg(long)]
    catalog: PathBuf,
    /// RuntimeCatalogMetadataV1 without derived quality/row_count. Native definitions may be omitted.
    #[arg(long)]
    declaration: PathBuf,
    /// Explicit NativeDatasetSelectionV1, including original complete settlement groups if present.
    #[arg(long)]
    selection: PathBuf,
    /// New directory, containing catalog/ and a final catalog-metadata.json.
    #[arg(long)]
    output: PathBuf,
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let input = options.open(path)?;
    let size = input.metadata()?;
    ensure!(
        size.is_file() && size.len() <= MAX_JSON_BYTES,
        "PREPARATION_INPUT_LIMIT"
    );
    let mut bytes = Vec::new();
    input.take(MAX_JSON_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == size.len(),
        "PREPARATION_INPUT_CHANGED"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

fn native(root: &Path) -> Result<ParquetDataCatalog> {
    ParquetDataCatalog::from_uri(
        root.to_str().context("CATALOG_PATH_ENCODING")?,
        None,
        Some(4096),
        None,
        None,
    )
}

/// Keep untraded siblings needed by a selected condition, without copying other instruments.
fn originals(
    root: &Path,
    data: &NativeMarketData,
    selected: &NativeDatasetSelectionV1,
) -> Result<(Vec<InstrumentAny>, Vec<InstrumentClose>)> {
    let mut ids = data
        .series
        .iter()
        .map(|s| s.instrument.id().to_string())
        .collect::<BTreeSet<_>>();
    let mut payouts = BTreeMap::new();
    for outcome in selected.settlements.iter().flat_map(|g| &g.outcomes) {
        ids.insert(outcome.instrument_id.clone());
        ensure!(
            payouts
                .insert(outcome.instrument_id.clone(), outcome)
                .is_none(),
            "DUPLICATE_SETTLEMENT"
        );
    }
    ensure!(ids.len() <= 256, "CATALOG_INSTRUMENT_LIMIT");
    let mut catalog = native(root)?;
    let instruments = catalog.instruments(
        Some(&ids.iter().cloned().collect::<Vec<_>>()),
        None,
        Some(UnixNanos::from(selected.selection.decision_cutoff_ns.get())),
    )?;
    let mut definitions = BTreeMap::new();
    for instrument in instruments {
        let id = instrument.id().to_string();
        ensure!(
            ids.contains(&id) && !definitions.contains_key(&id),
            "CATALOG_INSTRUMENT_VERSION_MISMATCH"
        );
        ensure!(
            instrument.ts_event() <= instrument.ts_init()
                && instrument.ts_init().as_u64() <= selected.selection.decision_cutoff_ns.get(),
            "INSTRUMENT_DEFINITION_FROM_FUTURE"
        );
        if let Some(outcome) = payouts.get(&id) {
            ensure!(
                instrument.ts_init().as_u64() <= outcome.ts_init.get(),
                "INSTRUMENT_DEFINITION_FROM_FUTURE"
            );
        }
        if matches!(instrument, InstrumentAny::BinaryOption(_)) {
            let value = serde_json::to_value(&instrument)?;
            let (activation, expiration) = domain::prediction::instrument(&value["BinaryOption"])?;
            if let Some(outcome) = payouts.get(&id) {
                ensure!(
                    outcome.ts_event.get() >= activation,
                    "SETTLEMENT_BEFORE_ACTIVATION"
                );
            }
            if let Some(series) = data
                .series
                .iter()
                .find(|s| s.instrument.id() == instrument.id())
            {
                // Missing traded-instrument fees remain missing; an original zero rate is supported.
                domain::prediction::planning_fee(&value["BinaryOption"])?;
                ensure!(
                    series.bars.iter().all(|b| b.ts_event.as_u64() >= activation
                        && b.ts_event.as_u64() <= expiration
                        && [b.open, b.high, b.low, b.close]
                            .iter()
                            .all(|p| p.as_decimal() < rust_decimal::Decimal::ONE)),
                    "BINARY_BAR_OUTSIDE_NATIVE_CONTRACT"
                );
            }
        }
        definitions.insert(id, instrument);
    }
    ensure!(definitions.len() == ids.len(), "CATALOG_INSTRUMENT_MISSING");
    for series in &data.series {
        ensure!(
            serde_json::to_value(&definitions[&series.instrument.id().to_string()])?
                == serde_json::to_value(&series.instrument)?,
            "CATALOG_INSTRUMENT_CHANGED"
        );
    }
    let mut closes = Vec::new();
    if !payouts.is_empty() {
        let query = catalog.query::<InstrumentClose>(
            Some(payouts.keys().cloned().collect()),
            None,
            Some(UnixNanos::from(selected.selection.decision_cutoff_ns.get())),
            None,
            None,
            true,
        )?;
        let mut seen = BTreeSet::new();
        for record in query.take(payouts.len() + 1) {
            let Data::InstrumentClose(close) = record? else {
                anyhow::bail!("CATALOG_NATIVE_TYPE_MISMATCH")
            };
            let id = close.instrument_id.to_string();
            let outcome = payouts.get(&id).context("UNDECLARED_SETTLEMENT")?;
            ensure!(
                seen.insert(id.clone())
                    && close.ts_event.as_u64() == outcome.ts_event.get()
                    && close.ts_init.as_u64() == outcome.ts_init.get()
                    && close
                        .close_price
                        .as_decimal()
                        .to_string()
                        .parse::<contracts::DecimalValue>()
                        .map_err(anyhow::Error::msg)?
                        == outcome.close_price
                    && close.close_type
                        == nautilus_model::enums::InstrumentCloseType::ContractExpired
                    && close.close_price.precision == definitions[&id].price_precision(),
                "SETTLEMENT_SOURCE_MISMATCH"
            );
            closes.push(close);
        }
        ensure!(seen.len() == payouts.len(), "SETTLEMENT_SOURCE_MISMATCH");
    }
    closes.sort_by_key(|close| close.instrument_id);
    Ok((definitions.into_values().collect(), closes))
}

fn prepare(args: &Arguments) -> Result<RuntimeCatalogMetadataV1> {
    let selected: NativeDatasetSelectionV1 = read_json(&args.selection)?;
    let mut declaration: Value = read_json(&args.declaration)?;
    let object = declaration
        .as_object_mut()
        .context("METADATA_OBJECT_REQUIRED")?;
    ensure!(
        !object.contains_key("row_count") && !object.contains_key("quality"),
        "MEASURED_FIELDS_MUST_BE_OMITTED"
    );
    let partition: DataPartition = serde_json::from_value(
        object
            .get("partition")
            .context("PARTITION_REQUIRED")?
            .clone(),
    )?;
    let sealed = partition == DataPartition::Sealed;
    let (observed, mut measured) = measure_catalog(&args.catalog, &selected, !sealed)?;
    let (instruments, closes) = originals(&args.catalog, &observed, &selected)?;
    let definitions = serde_json::to_value(&instruments)?;
    let universe = object
        .get_mut("universe")
        .and_then(Value::as_object_mut)
        .context("UNIVERSE_REQUIRED")?;
    if let Some(declared) = universe.get("instrument_definitions") {
        let mut declared = serde_json::from_value::<Vec<Value>>(declared.clone())?;
        declared.sort_by_key(|v| {
            domain::catalogs::instrument_definition(v)
                .ok()
                .and_then(|(_, v)| v["id"].as_str())
                .unwrap_or("")
                .to_owned()
        });
        ensure!(
            Value::Array(declared) == definitions,
            "DECLARED_INSTRUMENTS_DIFFER_FROM_SOURCE"
        );
    }
    universe.insert("instrument_definitions".into(), definitions);
    if sealed {
        measured.settlements.clear();
    }
    object.insert(
        "row_count".into(),
        serde_json::to_value(measured.row_count)?,
    );
    object.insert(
        "quality".into(),
        serde_json::to_value(quality_report(vec![measured.clone()])?)?,
    );
    let metadata: RuntimeCatalogMetadataV1 = serde_json::from_value(declaration)?;
    ensure!(
        metadata.provider_kind == "NAUTILUS_CATALOG"
            && metadata
                .event_start
                .timestamp_nanos_opt()
                .and_then(|n| u64::try_from(n).ok())
                == Some(selected.selection.event_start_ns.get())
            && metadata
                .event_end
                .timestamp_nanos_opt()
                .and_then(|n| u64::try_from(n).ok())
                == Some(selected.selection.event_end_ns.get())
            && metadata
                .available_through
                .timestamp_nanos_opt()
                .and_then(|n| u64::try_from(n).ok())
                .is_some_and(|n| n <= selected.selection.decision_cutoff_ns.get()
                    && selected
                        .settlements
                        .iter()
                        .flat_map(|g| &g.outcomes)
                        .all(|o| o.ts_init.get() <= n)),
        "DECLARED_SELECTION_MISMATCH"
    );
    for timestamp in [
        metadata.event_start,
        metadata.event_end,
        metadata.available_through,
        metadata.universe.selection_asof,
        metadata.universe.coverage_start,
        metadata.universe.coverage_end,
    ] {
        ensure!(
            timestamp.timestamp_subsec_nanos().is_multiple_of(1000),
            "REGISTRATION_MICROSECOND_PRECISION_REQUIRED"
        );
    }
    domain::catalogs::metadata(&metadata, chrono::Utc::now())?;
    domain::data::registry_key(&metadata.registered_ref)?;
    let source = args.catalog.canonicalize()?;
    let parent = args
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let output = parent.join(
        args.output
            .file_name()
            .context("OUTPUT_DIRECTORY_REQUIRED")?,
    );
    ensure!(
        !output.starts_with(&source),
        "OUTPUT_MUST_NOT_OVERLAP_SOURCE"
    );
    fs::create_dir(&output).context("OUTPUT_MUST_BE_NEW")?;
    let root = output.join("catalog");
    fs::create_dir(&root)?;
    let native = native(&root)?;
    native.write_instruments(instruments.clone())?;
    for series in &observed.series {
        native.write_to_parquet(&series.bars, None, None, None)?;
    }
    for close in &closes {
        native.write_to_parquet(&[*close], None, None, None)?;
    }
    let (readback, mut quality) = measure_catalog(&root, &selected, !sealed)?;
    if sealed {
        quality.settlements.clear();
    }
    ensure!(
        serde_json::to_value(&quality)? == serde_json::to_value(&measured)?
            && readback
                .series
                .iter()
                .zip(&observed.series)
                .all(|(a, b)| a.bars == b.bars),
        "CATALOG_READBACK_MISMATCH"
    );
    let (readback_instruments, readback_closes) = originals(&root, &readback, &selected)?;
    ensure!(
        serde_json::to_value(readback_instruments)? == serde_json::to_value(instruments)?
            && readback_closes == closes,
        "CATALOG_ORIGINALS_READBACK_MISMATCH"
    );
    let bytes = serde_json::to_vec_pretty(&metadata)?;
    ensure!(
        bytes.len() as u64 <= MAX_JSON_BYTES,
        "PREPARATION_METADATA_LIMIT"
    );
    let partial = output.join("catalog-metadata.json.partial");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::hard_link(&partial, output.join("catalog-metadata.json"))?;
    // Publication is complete; failure to remove the staging link cannot undo it.
    let _ = fs::remove_file(partial);
    Ok(metadata)
}

fn main() {
    match prepare(&Arguments::parse()) {
        Ok(metadata) => println!(
            "{}",
            serde_json::to_string(&metadata).expect("typed metadata")
        ),
        Err(_) => {
            eprintln!("QZ_CATALOG_PREPARATION_FAILED");
            std::process::exit(1);
        }
    }
}
