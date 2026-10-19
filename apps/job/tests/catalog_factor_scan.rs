//! Complete-catalog factor scans must retain the explicit evaluator's native science.
#[path = "support/factors_catalog.rs"]
mod factors_catalog;
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;

use contracts::factors::NativeCatalogFactorScanRequestV1;
use factors_catalog::mixed_catalog;
use job::factors::{evaluate_factor, scan_factor_catalog};
use market::{count, module, INTERVAL_NS};

fn scan_request(
    request: &contracts::factors::NativeFactorEvaluationRequestV1,
) -> NativeCatalogFactorScanRequestV1 {
    let selection = &request.forecast.selection;
    NativeCatalogFactorScanRequestV1 {
        schema_version: contracts::SchemaV1,
        event_start_ns: selection.event_start_ns,
        train_end_ns: request.train_end_ns,
        validation_end_ns: request.validation_end_ns,
        event_end_ns: selection.event_end_ns,
        decision_cutoff_ns: selection.decision_cutoff_ns,
        maximum_rows: selection.maximum_rows,
        parameters: request.forecast.parameters.clone(),
    }
}

#[test]
fn complete_catalog_scan_matches_original_pooled_metrics_and_shared_fuel() {
    let (root, mut request) = mixed_catalog(0.0, true);
    let wasm = module("local.get 4 f64.const 50 f64.sub");
    request.forecast.parameters.total_fuel = Some(count(1_000_000));
    let original = evaluate_factor(root.path(), &request, &wasm).unwrap();
    let mut input = scan_request(&request);
    let scanned = scan_factor_catalog(root.path(), &input, &wasm).unwrap();
    assert_eq!(
        serde_json::to_value(&scanned.request).unwrap(),
        serde_json::to_value(&input).unwrap()
    );
    assert_eq!(scanned.coverage.catalog_series.get(), 2);
    assert_eq!(scanned.coverage.observed_series.get(), 2);
    assert_eq!(scanned.coverage.observations.get(), 54);
    assert_eq!(scanned.consumed_fuel, original.consumed_fuel);
    let model = root.path().join("model.wasm");
    std::fs::write(&model, &wasm).unwrap();
    let result = native::command(
        &[
            "scan-factor-catalog".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
            "--model".as_ref(),
            model.as_os_str(),
        ],
        &input,
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap(),
        serde_json::to_value(&scanned).unwrap()
    );
    for (actual, expected) in scanned.partitions.iter().zip(&original.partitions) {
        assert_eq!(
            serde_json::to_value(&actual.metric.counts).unwrap(),
            serde_json::to_value(&expected.metric.counts).unwrap()
        );
        assert_eq!(
            actual.metric.pearson_ic.status,
            expected.metric.pearson_ic.status
        );
        match (
            actual.metric.pearson_ic.value,
            expected.metric.pearson_ic.value,
        ) {
            (Some(actual), Some(expected)) => assert!((actual - expected).abs() < 1e-12),
            (None, None) => {}
            values => panic!("inconsistent missing metric: {values:?}"),
        }
        assert!(
            actual.assets.is_empty(),
            "a pooled scan must not repeat millions of asset summaries"
        );
    }
    input.parameters.total_fuel = Some(count(original.consumed_fuel.unwrap().get() - 1));
    assert!(
        scan_factor_catalog(root.path(), &input, &wasm).is_err(),
        "the fuel budget must cover all series together"
    );
}

#[test]
fn complete_catalog_reports_short_and_unobserved_series_without_filling_metrics() {
    let (root, request) = mixed_catalog(0.0, false);
    let market = job::catalog::load_catalog(root.path(), &request.forecast.selection).unwrap();
    let mut bar = market.series[0].bars[0];
    let catalog = nautilus_persistence::backend::catalog::ParquetDataCatalog::from_uri(
        root.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    bar.bar_type = "SHORT.SIM-1-MINUTE-LAST-EXTERNAL".parse().unwrap();
    catalog.write_to_parquet(&[bar], None, None, None).unwrap();
    bar.bar_type = "LATE.SIM-1-MINUTE-LAST-EXTERNAL".parse().unwrap();
    bar.ts_event = (30 * INTERVAL_NS).into();
    bar.ts_init = (30 * INTERVAL_NS + 1).into();
    catalog.write_to_parquet(&[bar], None, None, None).unwrap();
    let mut input = scan_request(&request);
    input.maximum_rows = 56;
    input.event_end_ns = input.validation_end_ns;
    input.decision_cutoff_ns = input.validation_end_ns;
    input.parameters.total_fuel = None;
    let scanned = scan_factor_catalog(root.path(), &input, &module("local.get 4")).unwrap();
    assert_eq!(scanned.coverage.catalog_series.get(), 4);
    assert_eq!(scanned.coverage.observed_series.get(), 3);
    assert_eq!(scanned.coverage.empty_series.get(), 1);
    assert_eq!(scanned.coverage.warmup_only_series.get(), 1);
    assert_eq!(scanned.partitions[2].metric.pearson_ic.value, None);
    input.maximum_rows = 28;
    assert!(
        scan_factor_catalog(root.path(), &input, &module("local.get 4"))
            .unwrap_err()
            .to_string()
            .contains("CATALOG_ROW_LIMIT")
    );
}

#[test]
fn catalog_scan_accepts_multiple_timeframes_for_the_same_instrument() {
    let (root, request) = mixed_catalog(0.0, false);
    let market = job::catalog::load_catalog(root.path(), &request.forecast.selection).unwrap();
    let mut bars = market.series[0].bars.clone();
    let hour = 60 * INTERVAL_NS;
    let kind = format!("{}-1-HOUR-LAST-EXTERNAL", bars[0].bar_type.instrument_id())
        .parse()
        .unwrap();
    for (index, bar) in bars.iter_mut().enumerate() {
        bar.bar_type = kind;
        bar.ts_event = ((index as u64 + 1) * hour).into();
        bar.ts_init = (bar.ts_event.as_u64() + 1).into();
    }
    let catalog = nautilus_persistence::backend::catalog::ParquetDataCatalog::from_uri(
        root.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    catalog.write_to_parquet(&bars, None, None, None).unwrap();
    let mut input = scan_request(&request);
    input.train_end_ns = count(12 * hour);
    input.validation_end_ns = count(22 * hour);
    input.event_end_ns = count(28 * hour);
    input.decision_cutoff_ns = input.event_end_ns;
    input.maximum_rows = 81;
    input.parameters.total_fuel = None;
    let result = scan_factor_catalog(root.path(), &input, &module("local.get 4")).unwrap();
    assert_eq!(result.coverage.catalog_series.get(), 3);
    assert_eq!(result.coverage.observed_series.get(), 3);
    assert_eq!(result.coverage.observations.get(), 81);
    assert_eq!(result.partitions[0].metric.counts.observations.get(), 65);
    assert_eq!(result.partitions[1].metric.counts.observations.get(), 10);
    assert_eq!(result.partitions[2].metric.counts.observations.get(), 6);
}

#[test]
fn catalog_scan_retains_native_root_and_positive_price_validation() {
    let (root, request) = mixed_catalog(0.0, false);
    let input = scan_request(&request);
    let wasm = module("f64.const 1");
    #[cfg(unix)]
    {
        let links = tempfile::tempdir().unwrap();
        let link = links.path().join("catalog");
        std::os::unix::fs::symlink(root.path(), &link).unwrap();
        assert!(scan_factor_catalog(&link, &input, &wasm)
            .unwrap_err()
            .to_string()
            .contains("CATALOG_ROOT_INVALID"));
    }
    let market = job::catalog::load_catalog(root.path(), &request.forecast.selection).unwrap();
    let mut bar = market.series[0].bars[0];
    bar.open = "0.00000".parse().unwrap();
    bar.high = bar.open;
    bar.low = bar.open;
    bar.close = bar.open;
    let invalid = tempfile::tempdir().unwrap();
    let catalog = nautilus_persistence::backend::catalog::ParquetDataCatalog::from_uri(
        invalid.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    catalog.write_to_parquet(&[bar], None, None, None).unwrap();
    assert!(scan_factor_catalog(invalid.path(), &input, &wasm)
        .unwrap_err()
        .to_string()
        .contains("UNSUPPORTED_NONPOSITIVE_PRICE"));
}

#[test]
fn iterative_scan_clips_labels_and_preserves_unmetered_fuel() {
    let (root, request) = mixed_catalog(0.0, false);
    let (changed, _) = mixed_catalog(100.0, false);
    let mut input = scan_request(&request);
    input.parameters.total_fuel = None;
    input.event_end_ns = input.validation_end_ns;
    input.decision_cutoff_ns = input.validation_end_ns;
    let wasm = module("local.get 4 f64.const 50 f64.sub");
    let first = scan_factor_catalog(root.path(), &input, &wasm).unwrap();
    let second = scan_factor_catalog(changed.path(), &input, &wasm).unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&second).unwrap()
    );
    assert_eq!(first.consumed_fuel, None);
    assert_eq!(first.partitions[2].metric.counts.observations.get(), 0);
    assert_eq!(first.partitions[2].metric.pearson_ic.value, None);
    input.parameters.total_fuel = Some(count(1));
    assert!(scan_factor_catalog(root.path(), &input, &wasm).is_err());
    input.parameters.total_fuel = None;
    input.validation_end_ns = count(10 * INTERVAL_NS);
    assert!(scan_factor_catalog(root.path(), &input, &wasm).is_err());
}
