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
