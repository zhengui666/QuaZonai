//! Exact native TradeTick selection with separately verified source provenance.
use anyhow::{ensure, Context, Result};
use contracts::ticks::NativeTradeTickFactorEvaluationRequestV1;
use nautilus_model::{
    data::{Data, TradeTick},
    identifiers::InstrumentId,
    instruments::{Instrument, InstrumentAny},
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    path::Path,
    str::FromStr,
};

fn unsigned(report: &Value, field: &str) -> Result<u64> {
    report
        .get(field)
        .and_then(Value::as_u64)
        .with_context(|| format!("TICK_PROVENANCE_FIELD_INVALID: {field}"))
}

fn empty_object(report: &Value, field: &str) -> Result<()> {
    let values = report
        .get(field)
        .and_then(Value::as_object)
        .with_context(|| format!("TICK_PROVENANCE_FIELD_INVALID: {field}"))?;
    ensure!(values.is_empty(), "TICK_SOURCE_INCOMPLETE: {field}");
    Ok(())
}

/// A successful native import does not establish PIT or exchange-wide completeness.
/// The embedded copy survives ordinary catalog registration/copying. A sibling
/// import report is also accepted when it carries the required native inventory;
/// older inventory-free reports cannot establish prepared tick input.
pub(crate) fn source_provenance(root: &Path) -> Result<Value> {
    let embedded = root.join("import-provenance.json");
    let sibling = root
        .parent()
        .context("TICK_PROVENANCE_MISSING")?
        .join("import-report.json");
    let path = if embedded.is_file() {
        embedded
    } else {
        sibling
    };
    let file = File::open(&path).context("TICK_PROVENANCE_MISSING")?;
    ensure!(
        file.metadata()?.len() <= 16 * 1024 * 1024,
        "TICK_PROVENANCE_LIMIT"
    );
    serde_json::from_reader(file).context("TICK_PROVENANCE_INVALID")
}

/// Source/import conventions use original sizes rather than introducing hashes.
/// Exact file-set agreement detects missing/extra partitions and incomplete copies.
pub(crate) fn native_inventory(root: &Path) -> Result<Vec<Value>> {
    fn visit(root: &Path, directory: &Path, files: &mut Vec<Value>) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            ensure!(!kind.is_symlink(), "TICK_NATIVE_INVENTORY_SYMLINK");
            if kind.is_dir() {
                visit(root, &entry.path(), files)?;
            } else {
                ensure!(kind.is_file(), "TICK_NATIVE_INVENTORY_NONREGULAR");
                let path = entry.path();
                let relative = path
                    .strip_prefix(root)?
                    .to_str()
                    .context("TICK_NATIVE_INVENTORY_PATH")?;
                if relative != "import-provenance.json" {
                    files.push(serde_json::json!({"path":relative,"size":entry.metadata()?.len()}));
                }
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    Ok(files)
}

fn validate_inventory(root: &Path, provenance: &Value) -> Result<()> {
    let expected = provenance
        .get("native_files")
        .and_then(Value::as_array)
        .context("TICK_NATIVE_INVENTORY_REQUIRED")?;
    ensure!(!expected.is_empty(), "TICK_NATIVE_INVENTORY_EMPTY");
    ensure!(
        *expected == native_inventory(root)?,
        "TICK_NATIVE_INVENTORY_MISMATCH"
    );
    Ok(())
}

fn validate_provenance(
    report: &Value,
    request: &NativeTradeTickFactorEvaluationRequestV1,
) -> Result<()> {
    ensure!(
        report.get("schema_version").and_then(Value::as_u64) == Some(1)
            && report.get("status").and_then(Value::as_str) == Some("IMPORTED"),
        "TICK_SOURCE_INCOMPLETE"
    );
    let revision = report
        .get("source_revision")
        .or_else(|| report.get("revision"))
        .and_then(Value::as_str)
        .or_else(|| {
            // Compatibility with the existing official market-history report,
            // whose source_reference ends in the original pinned revision.
            report
                .get("source_reference")
                .and_then(Value::as_str)
                .and_then(|reference| reference.rsplit_once("/tree/"))
                .map(|(_, revision)| revision)
        })
        .context("TICK_SOURCE_REVISION_MISSING")?;
    ensure!(
        revision == request.input.source_revision,
        "TICK_SOURCE_REVISION_MISMATCH"
    );
    for key in ["source_revision", "revision"] {
        if let Some(value) = report.get(key) {
            ensure!(
                value.as_str() == Some(revision),
                "TICK_SOURCE_REVISION_CONFLICT"
            );
        }
    }
    if let Some((_, referenced)) = report
        .get("source_reference")
        .and_then(Value::as_str)
        .and_then(|reference| reference.rsplit_once("/tree/"))
    {
        ensure!(referenced == revision, "TICK_SOURCE_REVISION_CONFLICT");
    }
    for field in ["missing_symbols", "rejected_rows"] {
        empty_object(report, field)?;
    }
    // Historical contract definitions can be absent for unrelated source
    // symbols. Selected instruments still require native representation below.
    ensure!(
        report
            .get("missing_definitions")
            .and_then(Value::as_object)
            .is_some(),
        "TICK_PROVENANCE_FIELD_INVALID: missing_definitions"
    );
    let missing = report
        .get("missing_partitions")
        .and_then(Value::as_array)
        .context("TICK_PROVENANCE_FIELD_INVALID: missing_partitions")?;
    ensure!(
        missing.is_empty(),
        "TICK_SOURCE_INCOMPLETE: missing_partitions"
    );
    ensure!(
        unsigned(report, "duplicate_trades")? == 0,
        "TICK_SOURCE_DUPLICATES_REPORTED"
    );
    ensure!(
        unsigned(report, "readback_records")? > 0,
        "TICK_SOURCE_READBACK_MISSING"
    );
    ensure!(
        request.input.event_start_ns.get() >= unsigned(report, "event_start_ns")?
            && request.input.event_end_ns.get() <= unsigned(report, "event_end_ns")?,
        "TICK_SOURCE_COVERAGE_INCOMPLETE"
    );
    ensure!(
        report
            .get("historical_availability")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("UNVERIFIED")),
        "TICK_AVAILABILITY_POLICY_UNSUPPORTED"
    );
    if let Some(availability) = report.get("availability") {
        ensure!(
            availability.as_str() == Some("EVENT_TIME_PROXY_UNVERIFIED"),
            "TICK_AVAILABILITY_POLICY_UNSUPPORTED"
        );
    }
    if let Some(precision) = report.get("record_precision").and_then(Value::as_str) {
        if precision.starts_with("NATIVE_ENCODING_") {
            ensure!(
                precision.starts_with(&format!(
                    "NATIVE_ENCODING_{};",
                    nautilus_model::types::fixed::FIXED_PRECISION
                )),
                "TICK_NATIVE_PRECISION_INCOMPATIBLE"
            );
        }
    }
    if let Some(prepared) = report.get("native_prepared_selection") {
        let ids = prepared
            .get("instrument_ids")
            .and_then(Value::as_array)
            .context("TICK_PREPARED_SELECTION_INVALID")?;
        ensure!(
            request.input.event_start_ns.get() >= unsigned(prepared, "event_start_ns")?
                && request.input.event_end_ns.get() <= unsigned(prepared, "event_end_ns")?
                && request
                    .input
                    .instrument_ids
                    .iter()
                    .all(|name| ids.iter().any(|id| id.as_str() == Some(name))),
            "TICK_PREPARED_SELECTION_OUTSIDE_SCOPE"
        );
    }
    Ok(())
}

/// Source evidence contains declarations, coverage and limitations, never raw ticks.
pub(crate) fn source_evidence(
    root: &Path,
    request: &NativeTradeTickFactorEvaluationRequestV1,
) -> Result<BTreeMap<String, Value>> {
    let report = source_provenance(root)?;
    validate_provenance(&report, request)?;
    let mut evidence = BTreeMap::new();
    for key in [
        "source_reference",
        "revision",
        "source_revision",
        "selected_streams",
        "selected_path_prefixes",
        "source_coverage",
        "missing_partitions",
        "missing_definitions",
        "definitions_observed_after_event_rows",
        "record_precision",
        "historical_availability",
        "availability",
        "source_observed_at",
        "license",
        "attribution",
        "duplicate_trades",
        "source_rows",
        "selected_rows",
        "readback_records",
        "native_prepared_selection",
        "source_native_files",
    ] {
        if let Some(value) = report.get(key) {
            evidence.insert(key.to_owned(), evidence_value(Some(key), value));
        }
    }
    if let Some(files) = report
        .get("source_manifest")
        .and_then(|m| m.get("files"))
        .and_then(Value::as_array)
    {
        let prefixes = report
            .get("selected_path_prefixes")
            .and_then(Value::as_array);
        let streams = report.get("selected_streams").and_then(Value::as_array);
        let mut selected = files
            .iter()
            .filter_map(|file| {
                let path = file.get("path")?.as_str()?;
                let stream = file.get("stream")?.as_str()?;
                if prefixes.is_some_and(|ps| {
                    !ps.is_empty()
                        && !ps
                            .iter()
                            .any(|p| p.as_str().is_some_and(|p| path.starts_with(p)))
                }) || streams.is_some_and(|ss| !ss.iter().any(|s| s.as_str() == Some(stream)))
                {
                    return None;
                }
                Some(serde_json::json!({"path":path,"stream":stream,"size":file.get("size")}))
            })
            .collect::<Vec<_>>();
        selected.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
        evidence.insert("selected_source_files".into(), Value::Array(selected));
    } else {
        evidence.insert(
            "selection_limitations".into(),
            serde_json::json!(["LEGACY_IMPORT_REPORT_DOES_NOT_RECORD_EXACT_SOURCE_PATHS"]),
        );
    }
    Ok(evidence)
}

fn evidence_value(key: Option<&str>, value: &Value) -> Value {
    if key.is_some_and(|key| key.ends_with("_ns")) {
        if let Some(clock) = value.as_u64() {
            return Value::String(clock.to_string());
        }
    }
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), evidence_value(Some(key), value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| evidence_value(None, value))
                .collect(),
        ),
        _ => value.clone(),
    }
}

/// Uses the official streaming catalog query and retains its exact native values.
/// The caller bounds the selected data set; no record is rounded or deduplicated.
pub(crate) fn load(
    root: &Path,
    request: &NativeTradeTickFactorEvaluationRequestV1,
) -> Result<BTreeMap<String, Vec<TradeTick>>> {
    ensure!(
        !request.input.is_cohort_cache(),
        "TICK_NATIVE_REPRESENTATION_REQUIRED"
    );
    let provenance = source_provenance(root)?;
    validate_provenance(&provenance, request)?;
    validate_inventory(root, &provenance)?;
    let mut selected = BTreeMap::new();
    for name in &request.input.instrument_ids {
        let id = InstrumentId::from_str(name).context("TICK_INSTRUMENT_ID_INVALID")?;
        ensure!(id.to_string() == *name, "TICK_INSTRUMENT_ID_NONCANONICAL");
        ensure!(
            selected.insert(name.clone(), Vec::new()).is_none(),
            "TICK_INSTRUMENT_DUPLICATE"
        );
    }
    ensure!(!selected.is_empty(), "TICK_INSTRUMENTS_EMPTY");
    let mut catalog =
        crate::catalog::open_catalog(root).context("TICK_NATIVE_FORMAT_OR_CATALOG_INCOMPATIBLE")?;
    // These definitions describe the native representation only. Filtering by
    // historical cutoff would incorrectly reject visibly late current metadata;
    // none of its economic parameters is consumed by factor-only evaluation.
    let definitions = catalog
        .instruments(Some(&request.input.instrument_ids), None, None)
        .context("TICK_NATIVE_DEFINITIONS_INVALID")?;
    for definition in &definitions {
        if request
            .input
            .instrument_ids
            .contains(&definition.id().to_string())
        {
            ensure!(
                matches!(
                    definition,
                    InstrumentAny::CurrencyPair(_) | InstrumentAny::CryptoPerpetual(_)
                ),
                "TICK_INSTRUMENT_TYPE_UNSUPPORTED"
            );
        }
    }
    let represented: BTreeSet<_> = definitions
        .iter()
        .map(|instrument| instrument.id().to_string())
        .collect();
    for name in selected.keys() {
        ensure!(
            represented.contains(name),
            "TICK_INSTRUMENT_DEFINITION_MISSING: {name}"
        );
    }
    let event_interval = format!(
        "ts_event >= {} AND ts_event < {}",
        request.input.event_start_ns.get(),
        request.input.event_end_ns.get()
    );
    // This initial policy requires exact event-time proxy clocks. Availability
    // bounds would hide contradictory in-window rows before validation, so use
    // the explicit event filter and validate every selected row's clocks below.
    let query = catalog
        .query::<TradeTick>(
            Some(request.input.instrument_ids.clone()),
            None,
            None,
            Some(&event_interval),
            None,
            false,
        )
        .context("TICK_NATIVE_FORMAT_OR_QUERY_INCOMPATIBLE")?;
    let mut identities = BTreeMap::new();
    let mut rows = 0_usize;
    for item in query {
        let Data::Trade(trade) = item.context("TICK_NATIVE_FORMAT_OR_DECODE_INCOMPATIBLE")? else {
            anyhow::bail!("TICK_NATIVE_TYPE_MISMATCH");
        };
        let event = trade.ts_event.as_u64();
        let available = trade.ts_init.as_u64();
        ensure!(event == available, "TICK_AVAILABILITY_POLICY_UNSUPPORTED");
        ensure!(
            event >= request.input.event_start_ns.get()
                && event < request.input.event_end_ns.get()
                && available <= request.input.decision_cutoff_ns.get(),
            "TICK_QUERY_OUTSIDE_SELECTION"
        );
        ensure!(
            rows < request.input.maximum_rows.context("TICK_RESOURCE_LIMIT")? as usize,
            "TICK_ROW_LIMIT"
        );
        let name = trade.instrument_id.to_string();
        let trades = selected
            .get_mut(&name)
            .context("TICK_UNREQUESTED_INSTRUMENT")?;
        ensure!(
            !trade.price.is_undefined()
                && trade.price.as_f64().is_finite()
                && trade.price.as_f64() > 0.0,
            "TICK_PRICE_INVALID"
        );
        ensure!(
            !trade.size.is_undefined()
                && trade.size.as_f64().is_finite()
                && trade.size.as_f64() > 0.0,
            "TICK_QUANTITY_INVALID"
        );
        let id = trade.trade_id.as_str();
        ensure!(
            !id.trim().is_empty() && !id.chars().any(char::is_control),
            "TICK_ID_MISSING"
        );
        let key = (trade.instrument_id, trade.ts_event, trade.trade_id);
        if let Some(previous) = identities.insert(key, trade) {
            ensure!(previous == trade, "TICK_ID_CONFLICT");
            anyhow::bail!("TICK_ID_DUPLICATE");
        }
        trades.push(trade);
        rows += 1;
    }
    ensure!(rows > 0, "TICK_EMPTY_SELECTION");
    if let Some(prepared) = provenance.get("native_prepared_selection") {
        if request.input.event_start_ns.get() == unsigned(prepared, "event_start_ns")?
            && request.input.event_end_ns.get() == unsigned(prepared, "event_end_ns")?
        {
            let counts = prepared
                .get("trade_counts")
                .and_then(Value::as_object)
                .context("TICK_PREPARED_COUNTS_INVALID")?;
            for (name, trades) in &selected {
                ensure!(
                    counts.get(name).and_then(Value::as_u64) == Some(trades.len() as u64),
                    "TICK_PREPARED_COUNTS_MISMATCH"
                );
            }
        }
    }
    for trades in selected.values_mut() {
        // A stable tie order is reproducibility only, never availability proof.
        trades.sort_by(|left, right| {
            left.ts_event
                .cmp(&right.ts_event)
                .then_with(|| left.trade_id.as_str().cmp(right.trade_id.as_str()))
        });
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        enums::AggressorSide,
        identifiers::{InstrumentId, TradeId},
        instruments::stubs::{audusd_sim, binary_option},
        types::{Price, Quantity},
    };
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;
    use serde_json::json;

    fn request() -> NativeTradeTickFactorEvaluationRequestV1 {
        serde_json::from_value(json!({
            "schema_version": 1,
            "input": {"modality":"TRADE_TICK","instrument_ids":["AUD/USD.SIM"],
                "event_start_ns":"10","event_end_ns":"40","decision_cutoff_ns":"50",
                "maximum_rows":100,"source_revision":"0123456789012345678901234567890123456789",
                "availability":"EVENT_TIME_PROXY_UNVERIFIED"},
            "sampling":{"kind":"EVENT_TIME_COHORT_V1","maximum_cohort_rows":100},
            "target":{"kind":"FIXED_TRADE_COHORTS","horizon_cohorts":1},
            "parameters":{"fast_period_cohorts":1,"slow_period_cohorts":2},
            "train_end_ns":"20","validation_end_ns":"30"
        }))
        .unwrap()
    }

    fn report() -> Value {
        json!({"schema_version":1,"status":"IMPORTED",
            "source_reference":"https://huggingface.co/datasets/test/archive/tree/0123456789012345678901234567890123456789",
            "missing_symbols":{},"rejected_rows":{},"missing_definitions":{"unselected-old-symbol":2},
            "missing_partitions":[],"duplicate_trades":0,"readback_records":10,
            "event_start_ns":0,"event_end_ns":100,
            "historical_availability":"UNVERIFIED; event-time proxy"})
    }

    #[test]
    fn provenance_binds_actual_revision_and_selected_coverage() {
        let request = request();
        assert!(validate_provenance(&report(), &request).is_ok());
        let mut changed = report();
        changed["source_revision"] = json!("different");
        assert!(validate_provenance(&changed, &request)
            .unwrap_err()
            .to_string()
            .contains("REVISION_MISMATCH"));
        let mut uncovered = report();
        uncovered["event_end_ns"] = json!(39);
        assert!(validate_provenance(&uncovered, &request)
            .unwrap_err()
            .to_string()
            .contains("COVERAGE_INCOMPLETE"));
    }

    #[test]
    fn reported_incompleteness_and_source_duplicates_are_not_prepared() {
        for (field, value) in [
            ("status", json!("INCOMPLETE")),
            ("missing_symbols", json!({"missing":1})),
            ("rejected_rows", json!({"precision":1})),
            ("missing_partitions", json!(["missing.parquet"])),
            ("duplicate_trades", json!(1)),
        ] {
            let mut invalid = report();
            invalid[field] = value;
            assert!(
                validate_provenance(&invalid, &request()).is_err(),
                "{field}"
            );
        }
    }

    fn native_fixture(rows: Vec<TradeTick>) -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let catalog = ParquetDataCatalog::from_uri(
            temp.path().to_str().unwrap(),
            None,
            Some(4096),
            None,
            None,
        )
        .unwrap();
        let instrument = InstrumentAny::CurrencyPair(audusd_sim());
        crate::catalog::currencies::write(temp.path(), std::slice::from_ref(&instrument)).unwrap();
        catalog.write_instruments(vec![instrument]).unwrap();
        catalog.write_to_parquet(&rows, None, None, None).unwrap();
        let mut source = report();
        source["native_files"] =
            serde_json::to_value(native_inventory(temp.path()).unwrap()).unwrap();
        fs::write(
            temp.path().join("import-provenance.json"),
            serde_json::to_vec(&source).unwrap(),
        )
        .unwrap();
        temp
    }

    fn tick(event: u64, available: u64, id: &str) -> TradeTick {
        TradeTick::new(
            InstrumentId::from("AUD/USD.SIM"),
            Price::from("1.20000"),
            Quantity::from("1.00000"),
            AggressorSide::Buy,
            TradeId::from(id),
            event.into(),
            available.into(),
        )
    }

    #[test]
    fn native_query_preserves_trade_id_reuse_at_distinct_event_times() {
        let original = vec![tick(10, 10, "reused"), tick(20, 20, "reused")];
        let temp = native_fixture(original.clone());
        assert_eq!(
            load(temp.path(), &request()).unwrap()["AUD/USD.SIM"],
            original
        );
    }

    #[test]
    fn native_query_rejects_duplicate_or_conflicting_event_scoped_trade_ids() {
        for (conflicting, expected) in [(false, "TICK_ID_DUPLICATE"), (true, "TICK_ID_CONFLICT")] {
            let first = tick(10, 10, "same");
            let mut second = first;
            if conflicting {
                second.price = Price::from("1.30000");
            }
            let temp = native_fixture(vec![first, second]);
            assert_eq!(
                load(temp.path(), &request()).unwrap_err().to_string(),
                expected
            );
        }
    }

    #[test]
    fn native_query_does_not_hide_in_window_rows_with_contradictory_proxy_clocks() {
        let temp = native_fixture(vec![tick(10, 10, "valid"), tick(20, 100, "late")]);
        assert_eq!(
            load(temp.path(), &request()).unwrap_err().to_string(),
            "TICK_AVAILABILITY_POLICY_UNSUPPORTED"
        );
    }

    #[test]
    fn original_native_inventory_rejects_removed_files_before_query() {
        let temp = native_fixture(vec![tick(10, 10, "valid")]);
        assert_eq!(
            load(temp.path(), &request()).unwrap()["AUD/USD.SIM"].len(),
            1
        );
        let files = native_inventory(temp.path()).unwrap();
        let parquet = files
            .iter()
            .find(|file| file["path"].as_str().unwrap().ends_with(".parquet"))
            .unwrap();
        fs::remove_file(temp.path().join(parquet["path"].as_str().unwrap())).unwrap();
        assert_eq!(
            load(temp.path(), &request()).unwrap_err().to_string(),
            "TICK_NATIVE_INVENTORY_MISMATCH"
        );
    }

    #[test]
    fn native_tick_research_rejects_outcome_contracts_without_changing_bar_support() {
        let directory = tempfile::tempdir().unwrap();
        let instrument = InstrumentAny::BinaryOption(binary_option());
        let id = instrument.id();
        let native = ParquetDataCatalog::from_uri(
            directory.path().to_str().unwrap(),
            None,
            Some(16),
            None,
            None,
        )
        .unwrap();
        crate::catalog::currencies::write(directory.path(), std::slice::from_ref(&instrument))
            .unwrap();
        native.write_instruments(vec![instrument]).unwrap();
        native
            .write_to_parquet(
                &[TradeTick::new(
                    id,
                    Price::from("0.5"),
                    Quantity::from("1"),
                    AggressorSide::Buy,
                    TradeId::from("outcome"),
                    20_u64.into(),
                    20_u64.into(),
                )],
                None,
                None,
                None,
            )
            .unwrap();
        let mut provenance = report();
        provenance["native_files"] =
            serde_json::to_value(native_inventory(directory.path()).unwrap()).unwrap();
        fs::write(
            directory.path().join("import-provenance.json"),
            serde_json::to_vec(&provenance).unwrap(),
        )
        .unwrap();
        let mut selected = request();
        selected.input.instrument_ids = vec![id.to_string()];
        assert!(load(directory.path(), &selected)
            .unwrap_err()
            .to_string()
            .contains("TICK_INSTRUMENT_TYPE_UNSUPPORTED"));
    }

    #[test]
    fn source_evidence_ns_clocks_never_pass_through_json_floating_point() {
        let value = serde_json::json!({"source_coverage":{"trades":{"first_event_ns":9_007_199_254_740_993_u64}}});
        let value = evidence_value(None, &value);
        assert_eq!(
            value["source_coverage"]["trades"]["first_event_ns"],
            "9007199254740993"
        );
    }
}
