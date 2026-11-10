//! Offline operator preparation; original provenance is declared, native facts are measured.
use crate::catalog::{
    measure_catalog, quality_report, select_instrument_versions, NativeMarketData,
};
use anyhow::{ensure, Context, Result};
use clap::Parser;
use contracts::{
    catalogs::RuntimeCatalogMetadataV1, execution::NativeDatasetSelectionV1,
    research::DataPartition,
};
use nautilus_core::UnixNanos;
use nautilus_model::instruments::{Instrument, InstrumentAny};
use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[path = "preparation/ticks.rs"]
mod ticks;

#[derive(Parser)]
#[command(
    version,
    about = "Prepare an isolated native BAR catalog without certifying its source",
    after_help = "Native source imports: catalog-prepare prepare-ticks --help"
)]
struct Arguments {
    /// Original local native catalog; never modified.
    #[arg(long)]
    catalog: PathBuf,
    /// RuntimeCatalogMetadataV1 without derived quality/row_count. Native definitions may be omitted.
    #[arg(long)]
    declaration: PathBuf,
    /// Explicit NativeDatasetSelectionV1.
    #[arg(long)]
    selection: PathBuf,
    /// Independent frozen maker/taker venue policy; never inferred from request fee hints.
    #[arg(long)]
    fee_source: Option<PathBuf>,
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
    let mut input = options.open(path)?;
    let size = input.metadata()?;
    ensure!(size.is_file(), "PREPARATION_INPUT_LIMIT");
    let mut bytes = Vec::new();
    input.read_to_end(&mut bytes)?;
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

/// Preserve original definitions and updates for the selected native instruments.
fn originals(
    root: &Path,
    data: &NativeMarketData,
    selected: &NativeDatasetSelectionV1,
) -> Result<Vec<InstrumentAny>> {
    let ids = data
        .series
        .iter()
        .map(|s| s.instrument.id().to_string())
        .collect::<Vec<_>>();
    let catalog = native(root)?;
    let instruments = catalog.query_instruments_filtered(
        Some(&ids),
        None,
        Some(UnixNanos::from(selected.selection.decision_cutoff_ns.get())),
    )?;
    let definitions = select_instrument_versions(instruments, &selected.selection)?;
    ensure!(definitions.len() == ids.len(), "CATALOG_INSTRUMENT_MISSING");
    for series in &data.series {
        let observed = std::iter::once(&series.instrument)
            .chain(&series.instrument_updates)
            .collect::<Vec<_>>();
        ensure!(
            serde_json::to_value(&definitions[&series.instrument.id().to_string()])?
                == serde_json::to_value(observed)?,
            "CATALOG_INSTRUMENT_CHANGED"
        );
    }
    Ok(definitions.into_values().flatten().collect())
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
    let (mut observed, measured) = measure_catalog(&args.catalog, &selected, !sealed)?;
    if let Some(path) = &args.fee_source {
        let source = crate::fee_models::catalog_fees::load_file(path, &selected.selection)?;
        ensure!(
            observed
                .execution_fees
                .as_ref()
                .is_none_or(|original| *original == source),
            "CONFLICTING_EXECUTION_FEE_SOURCES"
        );
        observed.execution_fees = Some(source);
    }
    if let Some(source) = &observed.execution_fees {
        crate::fee_models::catalog_fees::validate_source_market(source, &observed)?;
    }
    let instruments = originals(&args.catalog, &observed, &selected)?;
    let definitions = serde_json::to_value(&instruments)?;
    let universe = object
        .get_mut("universe")
        .and_then(Value::as_object_mut)
        .context("UNIVERSE_REQUIRED")?;
    if let Some(declared) = universe.get("instrument_definitions") {
        let declared = serde_json::from_value::<Vec<Value>>(declared.clone())?;
        let declared = domain::catalogs::instrument_versions(&declared)?
            .into_values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        ensure!(
            Value::Array(declared) == definitions,
            "DECLARED_INSTRUMENTS_DIFFER_FROM_SOURCE"
        );
    }
    universe.insert("instrument_definitions".into(), definitions);
    object.insert(
        "row_count".into(),
        serde_json::to_value(measured.row_count)?,
    );
    object.insert(
        "quality".into(),
        serde_json::to_value(quality_report(vec![measured.clone()])?)?,
    );
    if let Some(declared) = object.get("execution_fees") {
        let declared: Option<contracts::catalogs::FrozenMakerTakerFeesV1> =
            serde_json::from_value(declared.clone())?;
        ensure!(
            declared == observed.execution_fees,
            "DECLARED_EXECUTION_FEES_DIFFER_FROM_SOURCE"
        );
    }
    object.insert(
        "execution_fees".into(),
        serde_json::to_value(&observed.execution_fees)?,
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
                .is_some_and(|n| n <= selected.selection.decision_cutoff_ns.get()),
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
    let spot_source = match fs::symlink_metadata(
        args.catalog
            .join(contracts::spot_cash_source::SPOT_CANDLE_SOURCE_FILE),
    ) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if spot_source {
        ensure!(
            metadata.pit_status == contracts::research::PitStatus::Unverified,
            "SPOT_REST_HISTORICAL_PIT_MUST_REMAIN_UNVERIFIED"
        );
        #[cfg(feature = "hyperliquid-offline")]
        crate::spot_cash_source::load_closed_rows(&args.catalog, &selected.selection)?;
        #[cfg(not(feature = "hyperliquid-offline"))]
        anyhow::bail!("SPOT_SOURCE_CAPABILITY_NOT_BUILT: hyperliquid-offline");
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
    crate::catalog::currencies::write(&root, &instruments)?;
    if let Some(source) = &observed.execution_fees {
        crate::fee_models::catalog_fees::write(&root, source)?;
    }
    native.write_instruments(instruments.clone())?;
    for series in &observed.series {
        native.write_to_parquet(&series.bars, None, None, None)?;
    }

    let (readback, quality) = measure_catalog(&root, &selected, !sealed)?;
    ensure!(
        serde_json::to_value(&quality)? == serde_json::to_value(&measured)?
            && serde_json::to_value(&readback.execution_fees)?
                == serde_json::to_value(&observed.execution_fees)?
            && readback
                .series
                .iter()
                .zip(&observed.series)
                .all(|(a, b)| a.bars == b.bars),
        "CATALOG_READBACK_MISMATCH"
    );
    #[cfg(feature = "hyperliquid-offline")]
    if spot_source {
        crate::spot_cash_source::preserve_for_selection(&source, &root, &selected.selection)?;
    }
    let readback_instruments = originals(&root, &readback, &selected)?;
    ensure!(
        serde_json::to_value(readback_instruments)? == serde_json::to_value(instruments)?,
        "CATALOG_ORIGINALS_READBACK_MISMATCH"
    );
    if let Some(path) = &args.fee_source {
        let original = crate::fee_models::catalog_fees::load_file(path, &selected.selection)?;
        ensure!(
            Some(&original) == observed.execution_fees.as_ref(),
            "PREPARATION_FEE_SOURCE_CHANGED"
        );
    }
    let bytes = serde_json::to_vec_pretty(&metadata)?;
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

pub fn run(argv: Vec<std::ffi::OsString>) {
    let is_tick =
        argv.get(1).map(|arg| arg.as_os_str()) == Some(std::ffi::OsStr::new("prepare-ticks"));
    // Preserve the original root invocation; the explicit source-import mode has
    // its own argument contract and never manufactures a registration declaration.
    let result =
        if argv.get(1).map(|arg| arg.as_os_str()) == Some(std::ffi::OsStr::new("prepare-ticks")) {
            let argv = std::iter::once(std::ffi::OsString::from("catalog-prepare prepare-ticks"))
                .chain(argv.into_iter().skip(2));
            ticks::run(&ticks::Arguments::parse_from(argv))
        } else {
            prepare(&Arguments::parse_from(argv))
                .and_then(|metadata| Ok(serde_json::to_value(metadata)?))
        };
    match result {
        Ok(metadata) => println!(
            "{}",
            serde_json::to_string(&metadata).expect("typed metadata")
        ),
        Err(error) => {
            if is_tick {
                let reason = error
                    .chain()
                    .map(ToString::to_string)
                    .find(|message| {
                        message.starts_with("TICK_")
                            || message == "OUTPUT_MUST_BE_NEW"
                            || message == "OUTPUT_MUST_NOT_OVERLAP_SOURCE"
                    })
                    .unwrap_or_else(|| "TICK_CATALOG_PREPARATION_FAILED".into());
                eprintln!("{reason}");
            } else {
                eprintln!("QZ_CATALOG_PREPARATION_FAILED");
            }
            std::process::exit(1);
        }
    }
}
