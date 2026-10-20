//! Full registered BAR catalog -> native pooled factor statistics, one series at a time.
use super::{count, MetricPairs};
use crate::{
    catalog::validate_price_bar,
    forecast::{bar_features, counter},
    signals::SignalModule,
};
use anyhow::{ensure, Result};
use contracts::{factors::*, science::*, SchemaV1};
use domain::factors::{
    evaluation_request, exclusion_reason, partition_bounds, partition_for_event,
};
use nautilus_model::data::{Bar, BarType};
use nautilus_serialization::arrow::DecodeFromRecordBatch;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    path::{Path, PathBuf},
};

fn evaluation(
    request: &NativeCatalogFactorScanRequestV1,
    kind: BarType,
) -> NativeFactorEvaluationRequestV1 {
    NativeFactorEvaluationRequestV1 {
        schema_version: SchemaV1,
        train_end_ns: request.train_end_ns,
        validation_end_ns: request.validation_end_ns,
        forecast: NativeForecastRequestV1 {
            schema_version: SchemaV1,
            parameters: request.parameters.clone(),
            selection: NativeBarSelectionV1 {
                schema_version: SchemaV1,
                bar_types: vec![kind.to_string()],
                event_start_ns: request.event_start_ns,
                event_end_ns: request.event_end_ns,
                decision_cutoff_ns: request.decision_cutoff_ns,
                maximum_rows: request.maximum_rows,
            },
        },
    }
}

fn series_paths(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "parquet")
        {
            paths.push(path);
        }
    }
    paths.sort();
    ensure!(!paths.is_empty(), "CATALOG_EMPTY_SERIES");
    Ok(paths)
}

fn series_directories(root: &Path) -> Result<Vec<PathBuf>> {
    let root = crate::catalog::catalog_root(root)?;
    let mut paths = Vec::new();
    for entry in fs::read_dir(root.join("data/bars"))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    ensure!(!paths.is_empty(), "CATALOG_EMPTY_SELECTION");
    Ok(paths)
}

fn metadata_kind(metadata: &std::collections::HashMap<String, String>) -> Result<BarType> {
    let name = metadata
        .get("bar_type")
        .ok_or_else(|| anyhow::anyhow!("CATALOG_BAR_TYPE_MISSING"))?;
    Ok(crate::catalog::bar_types(std::slice::from_ref(name))?[0])
}

/// Machine-private identities for scientific exposure; no BAR values or labels.
pub fn catalog_factor_scope(root: &Path) -> Result<Vec<String>> {
    let mut identities = HashSet::new();
    let mut types = Vec::new();
    for directory in series_directories(root)? {
        let mut identity = None;
        for path in series_paths(&directory)? {
            let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?;
            let kind = metadata_kind(builder.schema().metadata())?;
            ensure!(
                identity.is_none_or(|original| original == kind),
                "CATALOG_SERIES_IDENTITY_MISMATCH"
            );
            identity = Some(kind);
        }
        let kind = identity.expect("nonempty native file list");
        ensure!(identities.insert(kind), "DUPLICATE_CATALOG_BAR_TYPE");
        types.push(kind.to_string());
    }
    Ok(types)
}

fn read_series(
    directory: &Path,
    request: &NativeCatalogFactorScanRequestV1,
) -> Result<(BarType, Vec<Bar>)> {
    let mut identity = None;
    let mut bars = Vec::new();
    for path in series_paths(directory)? {
        let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?;
        let metadata = builder.schema().metadata().clone();
        let kind = metadata_kind(&metadata)?;
        ensure!(
            identity.is_none_or(|original| original == kind),
            "CATALOG_SERIES_IDENTITY_MISMATCH"
        );
        if identity.is_none() {
            evaluation_request(&evaluation(request, kind))?;
        }
        identity = Some(kind);
        for batch in builder.with_batch_size(4096).build()? {
            for bar in Bar::decode_batch(&metadata, batch?)? {
                let event = bar.ts_event.as_u64();
                let available = bar.ts_init.as_u64();
                // Identical event/availability selection to the explicit native query.
                if available < request.event_start_ns.get()
                    || available > request.decision_cutoff_ns.get()
                    || event < request.event_start_ns.get()
                    || event >= request.event_end_ns.get()
                {
                    continue;
                }
                ensure!(event <= available, "CATALOG_TIME_INVALID");
                ensure!(bar.bar_type == kind, "CATALOG_UNREQUESTED_BAR_TYPE");
                ensure!(
                    bars.len() < request.maximum_rows as usize,
                    "CATALOG_ROW_LIMIT"
                );
                bars.push(bar);
            }
        }
    }
    bars.sort_by_key(|bar| (bar.ts_init, bar.ts_event));
    for (index, bar) in bars.iter().enumerate() {
        validate_price_bar(bar, index.checked_sub(1).map(|previous| &bars[previous]))?;
    }
    Ok((identity.expect("nonempty native file list"), bars))
}

/// Catalog scope is explicit in this distinct command, not an empty-selection fallback.
/// This reports component IC only; it creates no targets, accounts or qualification.
pub fn scan_factor_catalog(
    root: &Path,
    request: &NativeCatalogFactorScanRequestV1,
    wasm: &[u8],
) -> Result<NativeCatalogFactorScanSummaryV1> {
    scan_factor_catalog_with_scope(root, request, wasm, None)
}

/// An optional exact machine scope binds computation to the recorded identities.
pub fn scan_factor_catalog_with_scope(
    root: &Path,
    request: &NativeCatalogFactorScanRequestV1,
    wasm: &[u8],
    scope: Option<&[String]>,
) -> Result<NativeCatalogFactorScanSummaryV1> {
    let names = series_directories(root)?;
    let mut expected = scope
        .map(|types| {
            types
                .iter()
                .map(|kind| Ok(crate::catalog::bar_types(std::slice::from_ref(kind))?[0]))
                .collect::<Result<HashSet<_>>>()
        })
        .transpose()?;
    ensure!(
        expected.as_ref().is_none_or(|kinds| {
            kinds.len() == names.len() && scope.is_some_and(|types| types.len() == kinds.len())
        }),
        "CATALOG_SCOPE_CHANGED"
    );
    eprintln!("catalog_factor_scan: catalog_series={}", names.len());
    let module = SignalModule::new(wasm, request.parameters.total_fuel.is_some())?;
    let mut remaining = request.parameters.total_fuel.map(|fuel| fuel.get());
    let mut pairs: [MetricPairs; 3] = Default::default();
    let mut identities = HashSet::new();
    let (mut observed, mut warmup_only, mut observations) = (0, 0, 0);
    let mut template = None;
    for (index, name) in names.iter().enumerate() {
        if index > 0 && index % 10_000 == 0 {
            eprintln!(
                "catalog_factor_scan: series={index}/{} observations={observations}",
                names.len()
            );
        }
        let (kind, bars) = read_series(name, request)?;
        ensure!(identities.insert(kind), "DUPLICATE_CATALOG_BAR_TYPE");
        ensure!(
            expected.as_mut().is_none_or(|kinds| kinds.remove(&kind)),
            "CATALOG_SCOPE_CHANGED"
        );
        let original = evaluation(request, kind);
        template.get_or_insert_with(|| original.clone());
        observations += bars.len();
        ensure!(
            observations <= request.maximum_rows as usize,
            "CATALOG_ROW_LIMIT"
        );
        if bars.is_empty() {
            continue;
        }
        observed += 1;
        let mut model = module.instantiate(u64::try_from(bars.len())?, remaining)?;
        let mut predictions = 0;
        let horizon = request.parameters.label_horizon_observations as usize;
        for (index, (bar, features)) in bars
            .iter()
            .zip(bar_features(&bars, &request.parameters))
            .enumerate()
        {
            let prediction = features
                .map(|features| model.predict(features))
                .transpose()?;
            predictions += usize::from(prediction.is_some());
            let future = prediction.and_then(|_| bars.get(index + horizon));
            let label = future.map(|future| future.close.as_f64() / bar.close.as_f64() - 1.0);
            ensure!(label.is_none_or(f64::is_finite), "FORECAST_LABEL_NONFINITE");
            let event_ns = counter(bar.ts_event.as_u64())?;
            let observation = NativeForecastPointV1 {
                instrument_id: kind.instrument_id().to_string(),
                ordinal: u32::try_from(index)?,
                event_ns,
                available_ns: counter(bar.ts_init.as_u64())?,
                forecast: prediction,
                forecast_reason: prediction
                    .is_none()
                    .then_some(ForecastMissingReason::IndicatorWarmup),
                label_return: label,
                label_available_ns: future
                    .map(|bar| counter(bar.ts_init.as_u64()))
                    .transpose()?,
                label_reason: if prediction.is_none() {
                    Some(ForecastMissingReason::IndicatorWarmup)
                } else {
                    future
                        .is_none()
                        .then_some(ForecastMissingReason::LabelNotComplete)
                },
            };
            let partition = partition_for_event(&original, event_ns)
                .ok_or_else(|| anyhow::anyhow!("FACTOR_EVENT_OUTSIDE_SELECTION"))?;
            let label_event_ns = future
                .map(|bar| counter(bar.ts_event.as_u64()))
                .transpose()?;
            let exclusion_reason =
                exclusion_reason(&original, partition, &observation, label_event_ns);
            let slot = match partition {
                NativeFactorPartitionV1::Train => 0,
                NativeFactorPartitionV1::Validation => 1,
                NativeFactorPartitionV1::Heldout => 2,
            };
            pairs[slot].push(&NativeFactorPointV1 {
                observation,
                partition,
                label_event_ns,
                exclusion_reason,
            });
        }
        warmup_only += usize::from(predictions == 0);
        remaining = model.remaining_fuel();
    }
    ensure!(
        expected.as_ref().is_none_or(HashSet::is_empty),
        "CATALOG_SCOPE_CHANGED"
    );
    let template = template.expect("nonempty catalog");
    let partitions = [
        NativeFactorPartitionV1::Train,
        NativeFactorPartitionV1::Validation,
        NativeFactorPartitionV1::Heldout,
    ]
    .into_iter()
    .zip(pairs)
    .map(|(partition, pairs)| {
        let (start_ns, end_ns) = partition_bounds(&template, partition);
        Ok(NativeFactorPartitionReportV1 {
            partition,
            start_ns,
            end_ns,
            metric: pairs.finish()?,
            assets: Vec::new(),
        })
    })
    .collect::<Result<Vec<_>>>()?;
    Ok(NativeCatalogFactorScanSummaryV1 {
        schema_version: SchemaV1,
        request: request.clone(),
        native_versions: BTreeMap::from([
            ("nautilus-model".into(), "0.63.0".into()),
            ("nautilus-serialization".into(), "0.63.0".into()),
            ("nautilus-indicators".into(), "0.63.0".into()),
            ("ndarray".into(), "0.17.1".into()),
            ("ndarray-stats".into(), "0.7.0".into()),
            ("wasmi".into(), "2.0.0".into()),
        ]),
        methods: BTreeMap::from([
            ("universe".into(), "Every registered catalog BAR series, lexical directory order; no market shortlist".into()),
            ("decoding".into(), "Parquet Arrow reader; official nautilus_serialization::arrow::DecodeFromRecordBatch for Bar".into()),
            ("features".into(), "Original causal native EMA features; fresh Wasmi instance per series".into()),
            ("ic".into(), "ndarray-stats.pearson_correlation on all original included score/return pairs, not mean asset IC".into()),
            ("partitioning".into(), "Original shared half-open event/availability/label boundaries".into()),
            ("assets".into(), "Per-asset metrics omitted; coverage records all catalog/observed/empty/warmup-only series".into()),
        ]),
        consumed_fuel: request.parameters.total_fuel.zip(remaining).map(|(total, left)| contracts::DbCounter::new(total.get() - left).map_err(anyhow::Error::msg)).transpose()?,
        partitions,
        coverage: NativeCatalogFactorCoverageV1 {
            catalog_series: count(names.len())?, observed_series: count(observed)?,
            empty_series: count(names.len() - observed)?, warmup_only_series: count(warmup_only)?,
            observations: count(observations)?,
        },
    })
}
