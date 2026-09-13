//! Structural/correlation fixtures only. No synthetic result confers data provenance
//! or scientific qualification; actual native outputs are tested in job/managed.
use chrono::{DateTime, Utc};
use contracts::{
    execution::*,
    runtime_jobs::{native_output_contract, RuntimeOutputV1},
    science::*,
    DbCounter, Id, Revision, SchemaV1,
};
use domain::execution::{output_bindings, output_shape};
use std::collections::BTreeMap;
#[path = "../../../tests/support/catalog_metadata.rs"]
mod catalog_fixture;

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn clock() -> DateTime<Utc> {
    DateTime::from_timestamp(100, 0).unwrap()
}
fn output<T: serde::Serialize>(name: &str, value: &T) -> (RuntimeOutputV1, Vec<u8>) {
    let bytes = serde_json::to_vec(value).unwrap();
    descriptor(name, bytes)
}
fn descriptor(name: &str, bytes: Vec<u8>) -> (RuntimeOutputV1, Vec<u8>) {
    let contract = native_output_contract(name, "1").unwrap();
    (
        RuntimeOutputV1 {
            kind: contract.kind,
            schema: contracts::runtime::RuntimeArtifactSchemaV1 {
                name: name.into(),
                version: "1".into(),
            },
            storage_ref: Id::new(),
            storage_version: Revision::INITIAL,
            byte_count: count(bytes.len() as u64),
            media_type: contract.media_type.into(),
        },
        bytes,
    )
}
fn accepts(parameters: &NativeTaskParametersV1, outputs: &[(RuntimeOutputV1, Vec<u8>)]) -> bool {
    output_bindings(
        parameters,
        None,
        clock() - chrono::Duration::seconds(1),
        clock(),
        outputs,
    )
    .is_ok()
}
fn quality() -> (NativeTaskParametersV1, NativeDataQualityReportV1) {
    let dataset = Id::new();
    let selection = NativeBarSelectionV1 {
        schema_version: SchemaV1,
        bar_types: vec!["TEST-ASSET.SIM-1-MINUTE-LAST-EXTERNAL".into()],
        event_start_ns: count(1),
        event_end_ns: count(10),
        decision_cutoff_ns: count(100),
        maximum_rows: 10,
    };
    (
        NativeTaskParametersV1::ValidateData {
            schema_version: SchemaV1,
            selections: vec![NativeDatasetSelectionV1 {
                dataset_revision_id: dataset,
                selection: selection.clone(),
            }],
        },
        NativeDataQualityReportV1 {
            schema_version: SchemaV1,
            native_version: "nautilus-persistence/0.63.0".into(),
            checked_at: clock(),
            datasets: vec![NativeDatasetQualityV1 {
                dataset_revision_id: dataset,
                selection,
                row_count: count(2),
                instrument_ids: vec!["TEST-ASSET.SIM".into()],
                first_event_ns: count(1),
                last_event_ns: count(2),
                available_through_ns: count(3),
                last_bar_notionals: None,
            }],
        },
    )
}

#[test]
fn structurally_deserializable_empty_quality_is_not_successful_validation() {
    let (_, mut report) = quality();
    report.datasets.clear();
    let (descriptor, bytes) = output("qz.data_quality", &report);
    assert!(serde_json::from_slice::<NativeDataQualityReportV1>(&bytes).is_ok());
    assert!(output_shape(&descriptor, &bytes).is_err());
}

#[test]
fn every_quality_dimension_and_exact_native_selection_are_checked() {
    let (parameters, report) = quality();
    assert!(accepts(&parameters, &[output("qz.data_quality", &report)]));
    for dimension in 0..15 {
        let mut invalid = report.clone();
        match dimension {
            0 => invalid.datasets.push(invalid.datasets[0].clone()),
            1 => invalid.datasets[0].instrument_ids.clear(),
            2 => invalid.datasets[0].instrument_ids[0] = "TEST".into(),
            3 => invalid.datasets[0].row_count = count(0),
            4 => invalid.datasets[0].row_count = count(11),
            5 => invalid.datasets[0].first_event_ns = count(0),
            6 => invalid.datasets[0].last_event_ns = count(10),
            7 => invalid.datasets[0].available_through_ns = count(1),
            8 => invalid.datasets[0].available_through_ns = count(101),
            9 => invalid.datasets[0].selection.bar_types.clear(),
            10 => invalid.datasets[0].selection.event_end_ns = count(1),
            11 => invalid.datasets[0].dataset_revision_id = Id::new(),
            12 => invalid.datasets[0].selection.maximum_rows = 11,
            13 => invalid.checked_at += chrono::Duration::seconds(1),
            _ => invalid.native_version = "native-shaped-string-is-not-a-version".into(),
        }
        assert!(
            !accepts(&parameters, &[output("qz.data_quality", &invalid)]),
            "quality dimension {dimension}"
        );
    }
    let foreign = quality().0;
    assert!(!accepts(&foreign, &[output("qz.data_quality", &report)]));
    assert!(!accepts(&parameters, &[]));
}

#[test]
fn measured_bar_notionals_preserve_identity_causality_and_zero_volume() {
    let (parameters, mut report) = quality();
    report.datasets[0].last_bar_notionals = Some(vec![NativeBarNotionalV1 {
        instrument_id: "TEST-ASSET.SIM".into(),
        currency: "USD".into(),
        event_ns: count(2),
        available_ns: count(3),
        close_price: "2".parse().unwrap(),
        traded_volume: "10".parse().unwrap(),
        notional_value: "20".parse().unwrap(),
    }]);
    assert!(accepts(&parameters, &[output("qz.data_quality", &report)]));
    for case in 0..12 {
        let mut changed = report.clone();
        let values = changed.datasets[0].last_bar_notionals.as_mut().unwrap();
        match case {
            0 => values.clear(),
            1 => values.push(values[0].clone()),
            2 => values[0].instrument_id = "FOREIGN.SIM".into(),
            3 => values[0].currency.clear(),
            4 => values[0].event_ns = count(1),
            5 => values[0].event_ns = count(4),
            6 => values[0].available_ns = count(2),
            7 => values[0].available_ns = count(4),
            8 => values[0].close_price = "0".parse().unwrap(),
            9 => values[0].traded_volume = "-1".parse().unwrap(),
            10 => values[0].notional_value = "-1".parse().unwrap(),
            _ => values[0].traded_volume = "0".parse().unwrap(),
        }
        assert!(
            !accepts(&parameters, &[output("qz.data_quality", &changed)]),
            "case {case}"
        );
    }
    let value = &mut report.datasets[0].last_bar_notionals.as_mut().unwrap()[0];
    value.traded_volume = "0".parse().unwrap();
    value.notional_value = "0".parse().unwrap();
    assert!(accepts(&parameters, &[output("qz.data_quality", &report)]));
}

#[test]
fn registered_sealed_metadata_rejects_bar_values_but_preserves_unknown_observation() {
    let mut metadata = catalog_fixture::metadata();
    let quality = &mut metadata.quality.datasets[0];
    quality.last_bar_notionals = Some(vec![NativeBarNotionalV1 {
        instrument_id: quality.instrument_ids[0].clone(),
        currency: "USD".into(),
        event_ns: quality.last_event_ns,
        available_ns: quality.available_through_ns,
        close_price: "1".parse().unwrap(),
        traded_volume: "1".parse().unwrap(),
        notional_value: "1".parse().unwrap(),
    }]);
    assert!(domain::catalogs::metadata(&metadata, catalog_fixture::instant(600)).is_ok());
    metadata.partition = contracts::research::DataPartition::Sealed;
    assert!(domain::catalogs::metadata(&metadata, catalog_fixture::instant(600)).is_err());
    metadata.quality.datasets[0].last_bar_notionals = None;
    assert!(domain::catalogs::metadata(&metadata, catalog_fixture::instant(600)).is_ok());
}

#[test]
fn original_calendar_metadata_requires_bound_coverage_order_and_observed_availability() {
    let original = catalog_fixture::calendar_metadata();
    let observed = catalog_fixture::instant(600);
    domain::catalogs::metadata(&original, observed).unwrap();
    for case in 0..9 {
        let mut changed = original.clone();
        let calendar = changed.universe.calendar_sessions.as_mut().unwrap();
        match case {
            0 => calendar.calendar_ref = "foreign".into(),
            1 => calendar.calendar_version = "foreign".into(),
            2 => calendar.timezone = "not-a-timezone".into(),
            3 => calendar.available_at_ns = count(600_000_000_001),
            4 => calendar.coverage_start_ns = count(1),
            5 => calendar.coverage_end_ns = count(599_999_999_999),
            6 => calendar.sessions.clear(),
            7 => calendar.sessions.push(calendar.sessions[0].clone()),
            _ => calendar.sessions[0].open_ns = calendar.sessions[0].close_ns,
        }
        assert!(
            domain::catalogs::metadata(&changed, observed).is_err(),
            "case {case}"
        );
    }
    let mut at_boundary = original;
    at_boundary
        .universe
        .calendar_sessions
        .as_mut()
        .unwrap()
        .available_at_ns = count(600_000_000_000);
    domain::catalogs::metadata(&at_boundary, observed).unwrap();
}

#[test]
fn historical_bar_liquidity_has_its_own_explicit_exclusive_age_bound() {
    use contracts::execution_assumptions::BarLiquidityAssumptionV1;
    let (_, mut report) = quality();
    let q = &mut report.datasets[0];
    q.last_bar_notionals = Some(vec![NativeBarNotionalV1 {
        instrument_id: q.instrument_ids[0].clone(),
        currency: "USD".into(),
        event_ns: count(2),
        available_ns: count(3),
        close_price: "1".parse().unwrap(),
        traded_volume: "0".parse().unwrap(),
        notional_value: "0".parse().unwrap(),
    }]);
    let mut assumption = BarLiquidityAssumptionV1 {
        schema_version: contracts::SchemaV1,
        report_artifact_id: Id::new(),
        maximum_age_seconds: 1,
        participation_limit: "0.1".parse().unwrap(),
    };
    assert!(
        domain::portfolio::bar_liquidity_values(&assumption, q, "USD", count(1_000_000_001))
            .is_ok()
    );
    assert!(
        domain::portfolio::bar_liquidity_values(&assumption, q, "USD", count(1_000_000_002))
            .is_err()
    );
    assert!(domain::portfolio::bar_liquidity_values(&assumption, q, "EUR", count(100)).is_err());
    assert!(domain::portfolio::bar_liquidity_values(&assumption, q, "USD", count(2)).is_err());
    for limit in ["0", "-0.1", "1.1"] {
        assumption.participation_limit = limit.parse().unwrap();
        assert!(domain::portfolio::bar_liquidity_assumption(&assumption).is_err());
    }
    assumption.participation_limit = "1".parse().unwrap();
    assumption.maximum_age_seconds = 0;
    assert!(domain::portfolio::bar_liquidity_assumption(&assumption).is_err());
}

#[test]
fn compiler_report_cannot_borrow_another_code_or_model_object() {
    let code = Id::new();
    let parameters = NativeTaskParametersV1::CompileModel {
        schema_version: SchemaV1,
        code_artifact_id: code,
    };
    let model = descriptor("qz.wasm_model", b"\0asm\x01\0\0\0".to_vec());
    let report = NativeModelCompilationV1 {
        schema_version: SchemaV1,
        code_artifact_id: code,
        model_storage_ref: model.0.storage_ref,
        rustc_version: "rustc 1.98.1 (native version-format fixture)".into(),
        target: "wasm32-unknown-unknown".into(),
        abi: "predict(f64,f64,f64,f64,f64,f64,f64,f64)->f64".into(),
        module_bytes: model.0.byte_count,
    };
    // The minimal Wasm header is a structural fixture, not a runnable exported model.
    assert!(accepts(
        &parameters,
        &[model.clone(), output("qz.model_compilation", &report)]
    ));
    for dimension in 0..5 {
        let mut invalid = report.clone();
        match dimension {
            0 => invalid.code_artifact_id = Id::new(),
            1 => invalid.model_storage_ref = Id::new(),
            2 => invalid.module_bytes = count(9),
            3 => invalid.rustc_version = "rustc 1.98.10 spoof".into(),
            _ => invalid.abi = "arbitrary-entrypoint".into(),
        }
        assert!(!accepts(
            &parameters,
            &[model.clone(), output("qz.model_compilation", &invalid)]
        ));
    }
}

fn forecast() -> (NativeTaskParametersV1, NativeForecastResultV1) {
    let selection = NativeBarSelectionV1 {
        schema_version: SchemaV1,
        bar_types: vec!["A.SIM-1-SECOND-LAST-EXTERNAL".into()],
        event_start_ns: count(0),
        event_end_ns: count(100),
        decision_cutoff_ns: count(100),
        maximum_rows: 10,
    };
    let request = NativeForecastRequestV1 {
        schema_version: SchemaV1,
        selection,
        parameters: NativeForecastParametersV1 {
            schema_version: SchemaV1,
            fast_period: 1,
            slow_period: 2,
            label_horizon_observations: 1,
            total_fuel: count(100),
        },
    };
    let points = (0..4)
        .map(|i| NativeForecastPointV1 {
            instrument_id: "A.SIM".into(),
            ordinal: i,
            event_ns: count(u64::from(i) * 10),
            available_ns: count(u64::from(i) * 10 + 1),
            forecast: (i > 0).then_some(0.01),
            forecast_reason: (i == 0).then_some(ForecastMissingReason::IndicatorWarmup),
            label_return: (i > 0 && i < 3).then_some(0.02),
            label_available_ns: (i > 0 && i < 3).then(|| count(u64::from(i + 1) * 10 + 1)),
            label_reason: if i == 0 {
                Some(ForecastMissingReason::IndicatorWarmup)
            } else if i == 3 {
                Some(ForecastMissingReason::LabelNotComplete)
            } else {
                None
            },
        })
        .collect();
    (
        NativeTaskParametersV1::EvaluateAlpha {
            schema_version: SchemaV1,
            dataset_revision_id: Id::new(),
            model_artifact_id: Id::new(),
            request,
        },
        NativeForecastResultV1 {
            schema_version: SchemaV1,
            native_versions: BTreeMap::from([
                ("nautilus-indicators".into(), "0.63.0".into()),
                ("nautilus-persistence".into(), "0.63.0".into()),
                ("wasmi".into(), "2.0.0".into()),
            ]),
            consumed_fuel: count(10),
            points,
        },
    )
}

#[test]
fn forecast_cannot_expand_visibility_change_horizon_or_hide_missing_values() {
    let (parameters, report) = forecast();
    assert!(accepts(
        &parameters,
        &[output("qz.native_forecast", &report)]
    ));
    for dimension in 0..10 {
        let mut invalid = report.clone();
        match dimension {
            0 => invalid.points.clear(),
            1 => invalid.points[0].instrument_id = "B.SIM".into(),
            2 => invalid.points[1].ordinal = 0,
            3 => invalid.points[1].event_ns = invalid.points[0].event_ns,
            4 => invalid.points[3].available_ns = count(101),
            5 => invalid.points[1].label_available_ns = Some(count(32)),
            6 => invalid.points[0].forecast_reason = None,
            7 => invalid.points[3].label_return = Some(0.01),
            8 => invalid.consumed_fuel = count(101),
            _ => invalid.native_versions.remove("wasmi").map(|_| ()).unwrap(),
        }
        assert!(
            !accepts(&parameters, &[output("qz.native_forecast", &invalid)]),
            "forecast dimension {dimension}"
        );
    }
    let mut other = parameters;
    if let NativeTaskParametersV1::EvaluateAlpha { request, .. } = &mut other {
        request.parameters.label_horizon_observations = 2;
    }
    assert!(!accepts(&other, &[output("qz.native_forecast", &report)]));
}
