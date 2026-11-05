//! Legacy row allowances are optional input evidence, never read quotas.
use contracts::{factors::NativeCatalogFactorScanRequestV1, science::NativeBarSelectionV1};
use serde_json::json;

#[test]
fn bar_selection_allows_omitted_or_zero_legacy_row_allowance() {
    let mut wire = json!({"schema_version":1,"bar_types":["AUD/USD.SIM-1-MINUTE-LAST-EXTERNAL"],"event_start_ns":"1","event_end_ns":"2","decision_cutoff_ns":"2"});
    let original: NativeBarSelectionV1 = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(original.maximum_rows, 0);
    wire["maximum_rows"] = json!(0);
    assert!(serde_json::from_value::<NativeBarSelectionV1>(wire).is_ok());
}

#[test]
fn catalog_scan_allows_omitted_legacy_row_allowance() {
    let wire = json!({"schema_version":1,"event_start_ns":"1","train_end_ns":"2","validation_end_ns":"3","event_end_ns":"4","decision_cutoff_ns":"4","parameters":{"schema_version":1,"fast_period":1,"slow_period":2,"label_horizon_observations":1}});
    let request: NativeCatalogFactorScanRequestV1 = serde_json::from_value(wire).unwrap();
    assert_eq!(request.maximum_rows, 0);
}
