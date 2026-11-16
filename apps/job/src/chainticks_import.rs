//! Single-file, local source preparation using the official custom-data catalog.
use crate::chainticks_fill::{
    parse_observation, register_observation_type, ChainticksFillObservation, NaiveTimePolicy,
    SourceLocation,
};
use crate::source_market::{field, scalar};
use anyhow::{ensure, Context, Result};
use clap::Args;
use nautilus_model::data::{CustomData, Data};
use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
use parquet::file::reader::{FileReader, SerializedFileReader};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::PathBuf,
    sync::Arc,
};

#[derive(Args)]
pub struct ImportArgs {
    /// One explicitly selected local source Parquet. No discovery or network requests.
    #[arg(long)]
    pub source: PathBuf,
    /// Original source revision, retained as supplied rather than inferred from content.
    #[arg(long)]
    pub revision: String,
    /// Explicit interpretation of timezone-free source node local_time as UTC.
    #[arg(long)]
    pub assume_source_local_time_utc: bool,
    /// A new output directory under an existing local parent.
    #[arg(long)]
    pub output: PathBuf,
}

fn import_report(args: ImportArgs) -> Result<serde_json::Value> {
    ensure!(
        args.source.is_absolute() && args.source.is_file(),
        "EXISTING_ABSOLUTE_SOURCE_REQUIRED"
    );
    ensure!(!args.revision.trim().is_empty(), "SOURCE_REVISION_REQUIRED");
    ensure!(args.output.is_absolute(), "ABSOLUTE_OUTPUT_REQUIRED");
    ensure!(
        args.output.parent().is_some_and(|p| p.is_dir()),
        "OUTPUT_PARENT_MISSING"
    );
    let reader = SerializedFileReader::new(File::open(&args.source)?)?;
    // Atomic creation preserves previous success and partial failures.
    fs::create_dir(&args.output).context("NEW_OUTPUT_DIRECTORY_REQUIRED")?;
    let mut rejects = File::create(args.output.join("rejected-source-rows.jsonl"))?;
    let mut observations = Vec::new();
    let mut classifications = BTreeMap::<String, u64>::new();
    let mut rejected = 0_u64;
    let mut source_rows = 0_u64;
    let source_file = args.source.to_string_lossy().into_owned();
    let policy = if args.assume_source_local_time_utc {
        NaiveTimePolicy::AssumeUtc
    } else {
        NaiveTimePolicy::Reject
    };
    for (row_index, row) in reader.get_row_iter(None)?.enumerate() {
        let row = row.with_context(|| {
            format!("SOURCE_PARQUET_ROW file={source_file} row_zero_based={row_index}")
        })?;
        source_rows += 1;
        let location = SourceLocation {
            source: "Chainticks/perp-data",
            revision: &args.revision,
            file: &source_file,
            row_zero_based: row_index as u64,
        };
        let parsed = (|| {
            let raw = scalar(field(&row, "raw_json")?)?;
            let recorded = match row
                .get_column_iter()
                .find(|(name, _)| name.as_str() == "recorded_at")
                .map(|(_, value)| value)
            {
                None | Some(parquet::record::Field::Null) => None,
                Some(parquet::record::Field::Str(value)) => Some(value.as_str()),
                _ => anyhow::bail!("ARCHIVE_RECORDED_AT_STRING_OR_NULL_REQUIRED"),
            };
            parse_observation(&raw, &location, recorded, policy)
        })();
        match parsed {
            Ok(observation) => {
                *classifications
                    .entry(observation.classification.clone())
                    .or_default() += 1;
                observations.push(observation);
            }
            Err(error) => {
                rejected += 1;
                writeln!(
                    rejects,
                    "{}",
                    json!({"source":"Chainticks/perp-data","revision":args.revision,"file":source_file,"row_zero_based":row_index,"error":format!("{error:#}")})
                )?;
            }
        }
    }
    rejects.sync_all()?;
    ensure!(
        !observations.is_empty(),
        "NO_OBSERVATIONS_WITH_REAL_CLOCKS; original source and reject locations retained"
    );
    // Catalog availability order only. Original row identity and source envelope remain intact.
    observations.sort_by_key(|row| row.ts_init);
    register_observation_type();
    let originals = observations
        .into_iter()
        .map(|row| CustomData::from_arc(Arc::new(row)))
        .collect::<Vec<_>>();
    let mut catalog = create_catalog(&args.output)?;
    let file = catalog.write_custom_data_batch(&originals, None, None, Some(false))?;
    let loaded = catalog.query_custom_data_dynamic(
        "ChainticksFillObservation",
        None,
        None,
        None,
        None,
        None,
        true,
    )?;
    ensure!(
        loaded.len() == originals.len(),
        "CUSTOM_DATA_READBACK_COUNT_MISMATCH"
    );
    let expected = originals
        .iter()
        .map(|value| {
            let row = value
                .data
                .as_any()
                .downcast_ref::<ChainticksFillObservation>()
                .expect("constructed observation type");
            (row.source_row_zero_based, value)
        })
        .collect::<BTreeMap<_, _>>();
    let mut read_rows = BTreeSet::new();
    for read in &loaded {
        let Data::Custom(value) = read else {
            anyhow::bail!("CUSTOM_DATA_READBACK_TYPE_MISMATCH")
        };
        let row = value
            .data
            .as_any()
            .downcast_ref::<ChainticksFillObservation>()
            .context("CUSTOM_DATA_READBACK_OBSERVATION_TYPE_MISMATCH")?;
        ensure!(
            read_rows.insert(row.source_row_zero_based),
            "CUSTOM_DATA_READBACK_DUPLICATE_SOURCE_ROW"
        );
        ensure!(
            expected
                .get(&row.source_row_zero_based)
                .is_some_and(|original| *original == value),
            "CUSTOM_DATA_READBACK_MISMATCH row_zero_based={}",
            row.source_row_zero_based
        );
    }
    let report = json!({"schema_version":1,"status":if rejected==0 {"IMPORTED"} else {"INCOMPLETE"},"source":"Chainticks/perp-data","source_file":source_file,"source_revision":args.revision,"source_rows":source_rows,"written_observations":originals.len(),"readback_records":loaded.len(),"rejected_rows":rejected,"classifications":classifications,"native_type":"ChainticksFillObservation","native_file":file,"research_qualified":false,"deduplication":"NONE; all observations retain file and row identity; account-side fills are not market trade volume","clock_semantics":"ts_event=event.time ms converted to ns; ts_init=source node block.local_time, not local download or archive ingestion","naive_local_time_assumption":if args.assume_source_local_time_utc {"UTC_EXPLICIT"} else {"REJECT"},"classification_scope":"dir and crossed only; liquidation, builderFee and twapId remain in original envelope, not distinct direction classes","clock_validation":"No synchrony or source-clock causality guarantee; negative source-node minus event delays are retained, not repaired","ordering":"Stable ts_init ordering for native catalog only; not exchange execution order","registered_in_quazonai":false,"coverage":"UNVERIFIED; one explicitly selected source file, no listing/delisting or market completeness claim"});
    fs::write(
        args.output.join("import-report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

fn require_complete(report: &serde_json::Value) -> Result<()> {
    ensure!(
        report["status"] == "IMPORTED",
        "INCOMPLETE_SOURCE_IMPORT; inspect original report and rejected-source-rows.jsonl"
    );
    Ok(())
}

pub fn import(args: ImportArgs) -> Result<()> {
    let report = import_report(args)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    require_complete(&report)
}

pub fn native_fill_observation_import(
    request: contracts::market_import::NativeFillObservationImportRequestV1,
) -> Result<contracts::market_import::NativeMarketImportResultV1> {
    let output = PathBuf::from(request.output_path);
    let report = import_report(ImportArgs {
        source: PathBuf::from(request.source_path),
        revision: request.source_revision,
        assume_source_local_time_utc: request.assume_source_local_time_utc,
        output: output.clone(),
    })?;
    require_complete(&report)?;
    Ok(contracts::market_import::NativeMarketImportResultV1 {
        schema_version: request.schema_version,
        catalog_path: output.join("catalog").to_string_lossy().into_owned(),
        report_path: output
            .join("import-report.json")
            .to_string_lossy()
            .into_owned(),
        research_qualified: false,
        report: crate::market_history::import_report_wire(report),
    })
}

fn create_catalog(output: &std::path::Path) -> Result<ParquetDataCatalog> {
    let root = output.join("catalog");
    fs::create_dir(&root).context("NEW_NATIVE_CATALOG_DIRECTORY_REQUIRED")?;
    Ok(ParquetDataCatalog::new(&root, None, None, None, None))
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_custom_catalog_root_exists_before_official_constructor_and_is_not_replaced() {
        let output = tempfile::tempdir().unwrap();
        let catalog = super::create_catalog(output.path()).unwrap();
        assert!(output.path().join("catalog").is_dir());
        drop(catalog);
        assert!(super::create_catalog(output.path()).is_err());
    }
}
