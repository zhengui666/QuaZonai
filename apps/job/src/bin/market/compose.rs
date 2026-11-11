//! Complete file-disjoint source units become one continuous per-instrument cache.
use super::*;
use job::ticks::cohort_cache;
use serde_json::Value;

fn add(target: &mut u64, value: u64) -> Result<()> {
    *target = target
        .checked_add(value)
        .context("COHORT_COMPOSE_COUNT_OVERFLOW")?;
    Ok(())
}

fn merge_counts(target: &mut BTreeMap<String, u64>, source: BTreeMap<String, u64>) -> Result<()> {
    for (key, count) in source {
        add(target.entry(key).or_default(), count)?;
    }
    Ok(())
}

pub(super) fn run(
    manifest_path: &Path,
    instruments_path: &Path,
    units: &[PathBuf],
    output: &Path,
    maximum_output_bytes: u64,
) -> Result<Report> {
    let destination = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?
        .join(
            output
                .file_name()
                .context("TICK_OUTPUT_DIRECTORY_REQUIRED")?,
        );
    for unit in units {
        ensure!(
            !destination.starts_with(unit.canonicalize()?),
            "OUTPUT_MUST_NOT_OVERLAP_SOURCE"
        );
    }
    let manifest: Manifest = json(manifest_path)?;
    let source: Value = json(manifest_path)?;
    let instruments: Value = json(instruments_path)?;
    let prefixes = selection_prefixes(&manifest, &[])?;
    ensure!(
        selected_missing(&manifest, &prefixes).is_empty(),
        "COHORT_COMPOSE_MISSING_SOURCE_PARTITIONS"
    );
    let expected = manifest
        .files
        .iter()
        .filter(|file| file.stream.is_some() && selected_path(&file.path, &prefixes))
        .map(|file| {
            ensure!(
                manifest.stream(file.stream.as_deref().unwrap())?.kind == "trades",
                "COHORT_COMPOSE_TRADE_SELECTION_REQUIRED"
            );
            Ok(file.path.clone())
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        !expected.is_empty() && !units.is_empty(),
        "COHORT_COMPOSE_EMPTY_SELECTION"
    );
    // Only small time/path descriptors survive each unit's provenance load.
    let mut ordered = Vec::new();
    for directory in units {
        let cache = cohort_cache::manifest(&directory.join("catalog"))?;
        ordered.push((cache.event_start_ns.get(), directory.clone()));
    }
    ordered.sort_by_key(|(start, _)| *start);
    let mut next = date_ns(manifest.start_date)?;
    let end = date_ns(manifest.end_date)?;
    let mut covered = BTreeSet::new();
    let mut representation = None;
    let mut precision = None;
    let mut streams = BTreeSet::new();
    let mut result = Report::default();
    let mut roots = Vec::new();
    for (_, directory) in &ordered {
        let cache = cohort_cache::manifest(&directory.join("catalog"))?;
        let mut report: Report = json(&directory.join("import-report.json"))?;
        let mut normalized = serde_json::to_value(&report)?;
        cohort_cache::exact_json(&mut normalized);
        ensure!(
            normalized == cache.source_report,
            "COHORT_COMPOSE_REPORT_MISMATCH"
        );
        ensure!(
            report.status == "IMPORTED"
                && report.modality == "TRADE_TICK"
                && report.availability == "EVENT_TIME_PROXY_UNVERIFIED"
                && report.source_manifest == source
                && report.instrument_source == instruments
                && report.source_revision == manifest.revision
                && report.revision == manifest.revision
                && cache.source_revision == manifest.revision
                && report.representation.as_deref() == Some(cache.representation.as_str()),
            "COHORT_COMPOSE_RECIPE_MISMATCH"
        );
        ensure!(
            report.event_start_ns == next
                && report.event_end_ns > next
                && report.event_end_ns <= end
                && report.event_start_ns.is_multiple_of(86_400_000_000_000)
                && report.event_end_ns.is_multiple_of(86_400_000_000_000)
                && cache.event_start_ns.get() == report.event_start_ns
                && cache.event_end_ns.get() == report.event_end_ns,
            "COHORT_COMPOSE_COMPLETE_CONTIGUOUS_DAYS_REQUIRED"
        );
        ensure!(
            report.outside_window_rows == 0
                && report.duplicate_trades == 0
                && report.missing_symbols.is_empty()
                && report.rejected_rows.is_empty()
                && report.missing_partitions.is_empty()
                && !report.registered_in_quazonai,
            "COHORT_COMPOSE_INCOMPLETE_UNIT"
        );
        ensure!(
            cache.trade_count.get() == report.selected_rows
                && cache.cohort_count.get() == report.readback_records,
            "COHORT_COMPOSE_COUNTS_MISMATCH"
        );
        let unit_prefixes = selection_prefixes(&manifest, &report.selected_path_prefixes)?;
        let mut selected = 0;
        for file in &manifest.files {
            if file.stream.is_some() && selected_path(&file.path, &unit_prefixes) {
                ensure!(
                    expected.contains(&file.path) && covered.insert(file.path.clone()),
                    "COHORT_COMPOSE_SOURCE_FILE_OVERLAP_OR_OUTSIDE_SELECTION"
                );
                selected += 1;
            }
        }
        ensure!(selected > 0, "COHORT_COMPOSE_EMPTY_UNIT");
        if let Some(previous) = &representation {
            ensure!(
                previous == &cache.representation
                    && precision.as_ref() == Some(&report.record_precision),
                "COHORT_COMPOSE_RECIPE_MISMATCH"
            );
        } else {
            representation = Some(cache.representation.clone());
            precision = Some(report.record_precision.clone());
            result.schema_version = report.schema_version;
            result.source_reference = report.source_reference.clone();
            result.revision = report.revision.clone();
            result.source_revision = report.source_revision.clone();
            result.modality = report.modality.clone();
            result.representation = report.representation.clone();
            result.availability = report.availability.clone();
            result.source_manifest = source.clone();
            result.instrument_source = instruments.clone();
            result.source_observed_at = report.source_observed_at;
            result.license = report.license.clone();
            result.attribution = report.attribution.clone();
            result.record_precision = report.record_precision.clone();
            result.historical_availability = report.historical_availability.clone();
        }
        ensure!(
            report.schema_version == result.schema_version
                && report.source_reference == result.source_reference
                && report.license == result.license
                && report.attribution == result.attribution
                && report.source_observed_at == result.source_observed_at
                && report.historical_availability == result.historical_availability,
            "COHORT_COMPOSE_RECIPE_MISMATCH"
        );
        streams.extend(report.selected_streams);
        for (name, coverage) in report.source_coverage {
            let total = result.source_coverage.entry(name).or_default();
            add(&mut total.source_rows, coverage.source_rows)?;
            add(&mut total.selected_rows, coverage.selected_rows)?;
            if let Some(first) = coverage.first_event_ns {
                total.first_event_ns =
                    Some(total.first_event_ns.map_or(first, |value| value.min(first)));
            }
            if let Some(last) = coverage.last_event_ns {
                total.last_event_ns =
                    Some(total.last_event_ns.map_or(last, |value| value.max(last)));
            }
        }
        add(&mut result.source_rows, report.source_rows)?;
        add(&mut result.filtered_rows, report.filtered_rows)?;
        add(&mut result.selected_rows, report.selected_rows)?;
        add(
            &mut result.definitions_observed_after_event_rows,
            report.definitions_observed_after_event_rows,
        )?;
        add(&mut result.empty_books, report.empty_books)?;
        add(&mut result.one_sided_books, report.one_sided_books)?;
        add(&mut result.readback_records, report.readback_records)?;
        merge_counts(
            &mut result.missing_definitions,
            std::mem::take(&mut report.missing_definitions),
        )?;
        next = report.event_end_ns;
        roots.push(directory.join("catalog"));
    }
    ensure!(
        next == end && covered == expected,
        "COHORT_COMPOSE_FULL_SOURCE_COVERAGE_REQUIRED"
    );
    result.status = "IMPORTED".into();
    result.event_start_ns = date_ns(manifest.start_date)?;
    result.event_end_ns = end;
    result.selected_path_prefixes = prefixes;
    result.selected_streams = streams.into_iter().collect();
    cohort_cache::concatenate(
        &roots,
        output,
        serde_json::to_value(result)?,
        maximum_output_bytes,
    )?;
    json(&output.join("import-report.json"))
}

fn report_count(report: &Value, field: &str) -> Result<u64> {
    report[field]
        .as_u64()
        .or_else(|| report[field].as_str().and_then(|value| value.parse().ok()))
        .with_context(|| format!("COHORT_COMPOSE_REPORT_FIELD_INVALID: {field}"))
}

/// Compose original Received units, retaining their separate source declarations.
pub(super) fn received(
    revision: &str,
    instruments_path: &Path,
    units: &[PathBuf],
    output: &Path,
    maximum_output_bytes: u64,
) -> Result<Value> {
    ensure!(
        !revision.trim().is_empty() && !units.is_empty(),
        "COHORT_COMPOSE_EMPTY_SELECTION"
    );
    let instruments: Value = json(instruments_path)?;
    let definitions = job::catalog::currencies::instruments_from_value(instruments.clone())?
        .into_iter()
        .map(|instrument| (instrument.id().to_string(), instrument))
        .collect::<BTreeMap<_, _>>();
    let mut universe = BTreeSet::new();
    if let Some(streams) = instruments["source_selection"]["native_ids_by_stream"].as_object() {
        for ids in streams.values() {
            for id in ids
                .as_array()
                .context("COHORT_COMPOSE_SOURCE_UNIVERSE_INVALID")?
            {
                universe.insert(
                    id.as_str()
                        .context("COHORT_COMPOSE_SOURCE_UNIVERSE_INVALID")?
                        .to_owned(),
                );
            }
        }
    }
    let mut ordered = Vec::new();
    for unit in units {
        let root = unit.join("catalog");
        let cache = cohort_cache::manifest(&root)?;
        ordered.push((cache.event_start_ns.get(), root));
    }
    ordered.sort_by_key(|(start, _)| *start);
    let mut result = serde_json::json!({
        "schema_version":1,"status":"IMPORTED","source_revision":revision,"revision":revision,
        "modality":"TRADE_TICK","availability":"RECEIVED_TIME_OBSERVED",
        "representation":cohort_cache::RECEIVED_FLOW_REPRESENTATION,
        "selected_streams":[],"selected_path_prefixes":[],"instrument_source":instruments,
        "missing_symbols":{},"rejected_rows":{},"missing_definitions":{},"missing_partitions":[],
        "source_coverage":{},"registered_in_quazonai":false,
        "definition_semantics":"Selected current official definitions provide one representation contract for the full historical window. Original unit definitions remain in source_manifest.components. For legacy USD-quoted, USDC-settled perpetuals, selecting USDC quotes changes native Money rounding from USD cents to USDC precision.",
        "source_manifest":{"revision":revision,"composition":"DERIVED_RECEIVED_COHORT_BUNDLE_V1","components":[]}
    });
    let mut streams = BTreeSet::new();
    let mut roots = Vec::new();
    let scalar_fields = [
        "source_reference",
        "license",
        "attribution",
        "record_precision",
        "historical_availability",
        "coverage",
        "deduplication_policy",
        "source_age_population",
    ];
    let counters = [
        "source_rows",
        "outside_window_rows",
        "filtered_rows",
        "selected_rows",
        "duplicate_trades",
        "deduplicated_trades",
        "definitions_observed_after_event_rows",
        "empty_books",
        "one_sided_books",
        "readback_records",
        "source_age_excluded_rows",
    ];
    for (_, root) in ordered {
        let cache = cohort_cache::manifest(&root)?;
        let report = cache.source_report;
        for file in cache.files {
            universe.insert(file.instrument_id);
        }
        // Cohort floats retain their source values; current encodings must retain the same tick and lot representation.
        for original in
            job::catalog::currencies::instruments_from_value(report["instrument_source"].clone())?
        {
            let current = definitions
                .get(&original.id().to_string())
                .context("COHORT_COMPOSE_CURRENT_DEFINITION_MISSING")?;
            ensure!(
                current.price_precision() == original.price_precision()
                    && current.size_precision() == original.size_precision()
                    && current.price_increment() == original.price_increment()
                    && current.size_increment() == original.size_increment(),
                "COHORT_COMPOSE_NATIVE_PRECISION_CHANGED"
            );
        }
        if roots.is_empty() {
            for key in scalar_fields {
                result[key] = report[key].clone();
            }
            result["source_observed_at"] = Value::Null;
            for key in ["repository", "coverage", "source_reference"] {
                result["source_manifest"][key] = report["source_manifest"][key].clone();
            }
        } else {
            for key in scalar_fields {
                ensure!(result[key] == report[key], "COHORT_COMPOSE_RECIPE_MISMATCH");
            }
            ensure!(
                result["source_manifest"]["repository"] == report["source_manifest"]["repository"],
                "COHORT_COMPOSE_RECIPE_MISMATCH"
            );
        }
        for field in counters {
            let mut total = result[field].as_u64().unwrap_or(0);
            if !report[field].is_null() {
                add(&mut total, report_count(&report, field)?)?;
            }
            result[field] = serde_json::json!(total);
        }
        for (name, count) in report["missing_definitions"]
            .as_object()
            .context("COHORT_COMPOSE_REPORT_INVALID")?
        {
            let mut total = result["missing_definitions"][name].as_u64().unwrap_or(0);
            add(
                &mut total,
                count.as_u64().context("COHORT_COMPOSE_REPORT_INVALID")?,
            )?;
            result["missing_definitions"][name] = serde_json::json!(total);
        }
        result["missing_partitions"].as_array_mut().unwrap().extend(
            report["missing_partitions"]
                .as_array()
                .context("COHORT_COMPOSE_REPORT_INVALID")?
                .iter()
                .cloned(),
        );
        for stream in report["selected_streams"]
            .as_array()
            .context("COHORT_COMPOSE_REPORT_INVALID")?
        {
            streams.insert(
                stream
                    .as_str()
                    .context("COHORT_COMPOSE_REPORT_INVALID")?
                    .to_owned(),
            );
        }
        for (stream, coverage) in report["source_coverage"]
            .as_object()
            .context("COHORT_COMPOSE_REPORT_INVALID")?
        {
            if result["source_coverage"][stream].is_null() {
                result["source_coverage"][stream] =
                    serde_json::json!({"source_rows":0,"selected_rows":0});
            }
            let target = &mut result["source_coverage"][stream];
            for key in ["source_rows", "selected_rows"] {
                let mut total = report_count(target, key)?;
                add(&mut total, report_count(coverage, key)?)?;
                target[key] = serde_json::json!(total);
            }
            for key in ["first_event_ns", "last_event_ns"] {
                if !coverage[key].is_null() {
                    let value = report_count(coverage, key)?;
                    let previous = if target[key].is_null() {
                        value
                    } else {
                        report_count(target, key)?
                    };
                    target[key] = serde_json::json!(if key == "first_event_ns" {
                        value.min(previous)
                    } else {
                        value.max(previous)
                    });
                }
            }
        }
        result["source_manifest"]["components"]
            .as_array_mut()
            .unwrap()
            .push(report);
        roots.push(root);
    }
    ensure!(
        universe.iter().all(|id| definitions.contains_key(id)),
        "COHORT_COMPOSE_CURRENT_DEFINITION_MISSING"
    );
    result["source_manifest"]["instrument_ids"] = serde_json::json!(universe);
    result["selected_streams"] = serde_json::json!(streams);
    cohort_cache::concatenate(&roots, output, result, maximum_output_bytes)?;
    json(&output.join("import-report.json"))
}
