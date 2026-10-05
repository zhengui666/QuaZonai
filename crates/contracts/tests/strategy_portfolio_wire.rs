use contracts::{
    delivery::{ReleaseCreateV1, TargetPackageV1},
    settings::{DownstreamConfigurationV1, PackageSchemaVersion},
    strategy_portfolio::*,
    DbCounter, Id,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    mem::{align_of, size_of},
};
use utoipa::PartialSchema;

#[path = "../../../tests/support/execution_models.rs"]
mod execution_models;

fn cash() -> Value {
    json!({"downstream_id":Id::new(),"trader_id":"TRADER-001","account_id":"BINANCE-001",
        "base_currency":"USDT","starting_capital":"1000","execution_assumptions_id":Id::new()})
}

fn v1_package() -> Value {
    let input: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    json!({
        "release_id":Id::new(),"package_schema_version":"1","environment_origin":"DEMO",
        "project_id":Id::new(),"candidate_id":Id::new(),"mandate_id":Id::new(),
        "qualification_refs":[Id::new(),Id::new()],"evaluation_refs":[Id::new()],"input_revision_refs":[Id::new()],
        "engine_versions":{"nautilus":"0.63.0"},"asof":"2026-10-03T00:00:00Z",
        "valid_from":"2026-10-03T00:00:00Z","valid_until":"2026-10-03T00:01:00Z",
        "base_currency":"USD","capital_assumption":"1000","current_weights_source":"NONE",
        "targets":[{"instrument_id":"EXAMPLE.SIM","target_weight":"0.25","currency":"USD"}],
        "cash_weight":"0.75","constraints_summary":input["constraints"],"exposure_tolerance":"0.01",
        "cost_assumption_ref":Id::new(),"compatible_market_capabilities":["fixture/1"],
        "limitations":["synthetic wire fixture"],"provenance_artifact_refs":[Id::new()]
    })
}

fn v2_package() -> Value {
    let mut package = v1_package();
    let object = package.as_object_mut().unwrap();
    for name in [
        "environment_origin",
        "qualification_refs",
        "evaluation_refs",
        "current_weights_source",
    ] {
        object.remove(name);
    }
    object.insert("package_schema_version".into(), json!("2"));
    object.insert("source_kind".into(), json!("NATIVE_TARGET_DECISION"));
    object.insert("execution_environment".into(), json!("PAPER"));
    object.insert("account_start".into(), cash());
    object.insert("execution_settings".into(), json!({
        "schema_version":1,"base_currency":"USDT","starting_capital":"1000",
        "account_kind":"MARGIN","leverage":"1","fee_model":execution_models::fee(),
        "fill_model":execution_models::fill(),"latency_model":execution_models::latency(1000000),
        "snapshot_interval_ms":1000,"exposure_tolerance":"0.01",
        "fee_rates":[{"instrument_id":"BTCUSDT.BINANCE","maker":"0","taker":"0.001"}]
    }));
    object.insert(
        "source".into(),
        json!({"run_id":Id::new(),"accepted_attempt_id":Id::new(),
        "report_artifact_id":Id::new(),"alpha_version_ids":[Id::new()],
        "input_provenance":{"dataset_revision_id":Id::new(),"market_data_origin":"REAL",
            "pit_status":"UNVERIFIED","revision_policy":"UNKNOWN","feature_artifact_origins":{}}}),
    );
    package
}

#[test]
fn legacy_package_wire_remains_unchanged_in_the_typed_envelope() {
    let value = v1_package();
    let parsed: TargetPackageEnvelopeV2 = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(parsed, TargetPackageEnvelopeV2::Forecast(_)));
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    let mut package: TargetPackageV1 = serde_json::from_value(value).unwrap();
    package.package_schema_version = PackageSchemaVersion::V2;
    assert!(serde_json::to_value(package).is_err());
}

#[test]
fn paper_execution_does_not_relabel_market_provenance_or_manufacture_qualification() {
    let value = v2_package();
    let parsed: TargetPackageEnvelopeV2 = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(parsed, TargetPackageEnvelopeV2::TargetDecision(_)));
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    assert!(serde_json::from_value::<TargetPackageV1>(value.clone()).is_err());
    assert_eq!(
        value["source"]["input_provenance"]["pit_status"],
        "UNVERIFIED"
    );
    assert_eq!(value["execution_environment"], "PAPER");
    assert!(value.get("qualification_refs").is_none());
    assert!(value.get("evaluation_refs").is_none());
}

#[test]
fn package_versions_select_their_own_body_and_reject_shape_downgrades() {
    for (mut body, wrong) in [
        (v1_package(), json!("2")),
        (v2_package(), json!("1")),
        (v2_package(), json!(2)),
        (v2_package(), json!("3")),
    ] {
        body["package_schema_version"] = wrong;
        assert!(serde_json::from_value::<TargetPackageEnvelopeV2>(body).is_err());
    }
}

#[test]
fn historical_and_current_purposes_cannot_hide_initial_account_assumptions() {
    let historical = json!({"purpose":"HISTORICAL_REPLAY"});
    assert!(serde_json::from_value::<StrategyPortfolioPurposeV1>(historical.clone()).is_ok());
    let current = json!({"purpose":"CURRENT_DECISION","account_start":cash(),
        "member_inputs":[{"alpha_version_id":Id::new(),"feature_artifact_ids":[Id::new()]}]});
    assert!(serde_json::from_value::<StrategyPortfolioPurposeV1>(current.clone()).is_ok());
    let mut missing = current;
    missing.as_object_mut().unwrap().remove("account_start");
    assert!(serde_json::from_value::<StrategyPortfolioPurposeV1>(missing).is_err());
    let mut hidden = historical;
    hidden["account_start"] = cash();
    assert!(serde_json::from_value::<StrategyPortfolioPurposeV1>(hidden).is_err());
}

#[test]
fn current_release_reference_is_distinct_from_a_forecast_evaluation() {
    let old = json!({"schema_version":1,"candidate_id":Id::new(),"evaluation_id":Id::new()});
    let parsed: ReleaseCreateEnvelopeV2 = serde_json::from_value(old.clone()).unwrap();
    assert!(matches!(parsed, ReleaseCreateEnvelopeV2::Forecast(_)));
    assert_eq!(serde_json::to_value(parsed).unwrap(), old);
    let new =
        json!({"schema_version":1,"candidate_id":Id::new(),"source_kind":"NATIVE_TARGET_DECISION"});
    assert!(matches!(
        serde_json::from_value::<ReleaseCreateEnvelopeV2>(new.clone()).unwrap(),
        ReleaseCreateEnvelopeV2::TargetDecision(_)
    ));
    assert!(serde_json::from_value::<ReleaseCreateV1>(new.clone()).is_err());
    let mut mixed = new;
    mixed["evaluation_id"] = json!(Id::new());
    assert!(serde_json::from_value::<ReleaseCreateEnvelopeV2>(mixed).is_err());
}

#[test]
fn version_two_is_discoverable_without_removing_version_one() {
    let value = json!({"name":"Paper","endpoint":"http://127.0.0.1:8099",
        "accepted_package_versions":["1","2"],"environments":"PAPER","enabled":true,"development_http":true});
    let parsed: DownstreamConfigurationV1 = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        parsed.accepted_package_versions,
        vec![PackageSchemaVersion::V1, PackageSchemaVersion::V2]
    );
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    assert!(serde_json::from_value::<PackageSchemaVersion>(json!("3")).is_err());
}

#[test]
fn strategy_alpha_keeps_replay_state_identity_without_forecast_fields() {
    let experiment = Id::new();
    let value = json!({"schema_version":1,"output_kind":"TARGET_WEIGHT","id":Id::new(),
        "project_id":Id::new(),"alpha_id":Id::new(),"version":"1","experiment_id":experiment,
        "root_lineage_id":Id::new(),"created_at":"2026-10-03T00:00:00Z",
        "policy":{"schema_version":1,"output_kind":"TARGET_WEIGHT",
            "source":{"experiment_id":experiment,"evaluation_run_id":Id::new(),
                "accepted_attempt_id":Id::new(),"report_artifact_id":Id::new()},
            "code_artifact_id":Id::new(),"model_artifact_id":Id::new(),
            "parameter_artifact_id":Id::new(),"dataset_revision_id":Id::new(),
            "feature_artifact_ids":[Id::new()],"feature_schema":[{
                "feature_key":"input-price","source_ref":"frozen:source","source_key":"input-price",
                "availability":{"basis":"OBSERVED"},"max_age_ns":"1000000000"}],
            "instrument_id":"EXAMPLE.SIM","base_currency":"USD",
            "model_abi":contracts::science::FEATURE_MODEL_ABI_V2,
            "initialization":{"source_fold_index":0,"first_ordinal":37,
                "first_event_ns":"9007199254740993","first_decision_ns":"9007199254740995"},
            "target_ttl_ns":"5000000000","runtime_image_ref":"fixture/native:1"}});
    let parsed: AlphaVersionEnvelopeV2 = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(parsed, AlphaVersionEnvelopeV2::TargetWeight(_)));
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    assert!(value.get("forecast_unit").is_none());
    let mut mixed = value;
    mixed["forecast_unit"] = json!("RETURN_PER_HORIZON");
    assert!(serde_json::from_value::<AlphaVersionEnvelopeV2>(mixed).is_err());
}

#[test]
fn legacy_alpha_retains_its_forecast_shape() {
    let value = json!({"id":Id::new(),"project_id":Id::new(),"alpha_id":Id::new(),
        "version":"1","experiment_id":Id::new(),"root_lineage_id":Id::new(),
        "code_artifact_id":Id::new(),"model_artifact_id":Id::new(),"signal_contract_version":"1",
        "signal_kind":"EXPECTED_RETURN","horizon_kind":"FIXED_BARS","horizon_value":"4",
        "forecast_unit":"RETURN_PER_HORIZON","calibration_id":null,"runtime_image_ref":"fixture/native:1",
        "origin":"SYNTHETIC","created_at":"2026-10-03T00:00:00Z"});
    let parsed: AlphaVersionEnvelopeV2 = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(parsed, AlphaVersionEnvelopeV2::Forecast(_)));
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}

#[test]
fn current_package_keeps_actual_execution_costs_not_only_an_assumption_id() {
    let value = v2_package();
    let parsed: TargetPackageEnvelopeV2 = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap()["execution_settings"],
        value["execution_settings"]
    );
    let mut absent = value;
    absent.as_object_mut().unwrap().remove("execution_settings");
    assert!(serde_json::from_value::<TargetPackageEnvelopeV2>(absent).is_err());
}

#[test]
fn legacy_candidate_list_stays_flat_while_detail_keeps_its_existing_shape() {
    let value = json!({
        "id":Id::new(),"project_id":Id::new(),"mandate_id":Id::new(),"input_set_id":Id::new(),"run_id":Id::new(),
        "decision_asof":"2026-10-03T00:00:00Z","created_at":"2026-10-03T00:00:00Z",
        "execution_status":"SUCCEEDED","solver_status":"OPTIMAL","evidence_status":"VALID","origin":"SYNTHETIC",
        "reason_code":null,"forecast_artifact_id":null,"covariance_artifact_id":null,
        "diagnostics_artifact_id":Id::new(),"target_artifact_id":null,"allocation_evaluation_id":null,
        "cash_weight":null,"current_weights_source":"NONE","current_weights_artifact_id":null
    });
    let listed: PortfolioCandidateListEnvelopeV2 = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(
        listed,
        PortfolioCandidateListEnvelopeV2::Forecast(_)
    ));
    assert_eq!(serde_json::to_value(listed).unwrap(), value);
    assert!(serde_json::from_value::<PortfolioCandidateEnvelopeV2>(value.clone()).is_err());
    let detail = json!({"header":value,"members":[],"targets":[]});
    let parsed: PortfolioCandidateEnvelopeV2 = serde_json::from_value(detail.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), detail);
    assert!(serde_json::from_value::<PortfolioCandidateListEnvelopeV2>(detail).is_err());
}

#[test]
fn strategy_report_uses_the_existing_native_output_contract_registry() {
    let output =
        contracts::runtime_jobs::native_output_contract("qz.strategy_portfolio", "1").unwrap();
    assert_eq!(
        output.kind,
        contracts::runtime_jobs::RuntimeOutputKind::Report
    );
    assert_eq!(output.media_type, "application/json");
    assert!(
        contracts::runtime_jobs::native_output_contract("qz.strategy_portfolio", "2").is_none()
    );
}

#[test]
fn mandate_envelopes_keep_both_existing_flat_wire_shapes() {
    let mut allocation: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    allocation["optimizer"]["parameters"]["cvar_confidence"] = Value::Null;
    allocation["optimizer"]["parameters"]["risk_budgeting"] = Value::Null;
    let forecast = json!({
        "objective":allocation["objective"],"risk_measure":allocation["risk"],
        "base_currency":"USD","capital_assumption":"1000000","universe_version_id":Id::new(),
        "covariance_estimator":allocation["covariance_estimator"],
        "alpha_ensemble":allocation["alpha_ensemble"],"optimizer":allocation["optimizer"],
        "constraints":allocation["constraints"],"exposure_tolerance":"0.000001",
        "rebalance_schedule":{"schema_version":1,"kind":"MANUAL","interval_seconds":null,
            "calendar_ref":null,"timezone":"UTC","session_offset_seconds":null,
            "max_input_age_seconds":60,"target_ttl_seconds":300},
        "required_evaluation_policy_id":Id::new(),"execution_assumptions_id":Id::new()
    });
    let strategy = json!({
        "schema_version":1,"allocation_method":"FIXED_TARGET_WEIGHTS",
        "base_currency":"USD","capital_assumption":"1000000","universe_version_id":Id::new(),
        "execution_assumptions_id":Id::new(),"constraints":allocation["constraints"],
        "exposure_tolerance":"0.000001","max_input_age_seconds":60,"target_ttl_seconds":300
    });
    for (content, is_strategy) in [(forecast, false), (strategy, true)] {
        let create = json!({"schema_version":1,"project_id":Id::new(),"runtime_id":Id::new(),
            "expected_runtime_revision":"1","content":content});
        let parsed: MandateCreateEnvelopeV2 = serde_json::from_value(create.clone()).unwrap();
        assert_eq!(
            matches!(parsed, MandateCreateEnvelopeV2::Strategy(_)),
            is_strategy
        );
        assert_eq!(serde_json::to_value(parsed).unwrap(), create);
        let mut view = json!({"id":Id::new(),"project_id":create["project_id"],"version":1,
            "content":content,"created_at":"2026-10-03T00:00:00Z"});
        if is_strategy {
            view["schema_version"] = json!(1);
        }
        let parsed: MandateViewEnvelopeV2 = serde_json::from_value(view.clone()).unwrap();
        assert_eq!(
            matches!(parsed, MandateViewEnvelopeV2::Strategy(_)),
            is_strategy
        );
        assert_eq!(serde_json::to_value(parsed).unwrap(), view);
    }
}

#[test]
fn composition_outcomes_keep_existing_fields_and_exact_numeric_values() {
    let target = json!({"schema_version":1,"asof_ns":"9007199254740993",
        "valid_until_ns":"9007199254740995",
        "targets":[{"instrument_id":"EXAMPLE.SIM","currency":"USD","weight":"0.25"}],
        "cash_weight":"0.75"});
    let current = json!({"purpose":"CURRENT_DECISION","account_start":cash(),"target":target,
        "predictions_per_member":{(Id::new().to_string()):"9007199254740993"}});
    let historical = json!({"purpose":"HISTORICAL_REPLAY",
        "simulation_request":{"schema_version":1,
            "selection":{"schema_version":1,"bar_types":["EXAMPLE.SIM-1-MINUTE-LAST-EXTERNAL"],
                "event_start_ns":"1","event_end_ns":"9007199254740995",
                "decision_cutoff_ns":"9007199254740993","maximum_rows":10},
            "settings":v2_package()["execution_settings"],"target_points":[target]},
        "simulation":{"schema_version":1,"native_version":"0.63.0","iterations":"1",
            "events":"1","orders":"1","positions":"1","consumed_target_points":"1",
            "summary":{},"statistics":[],"returns_kind":"PORTFOLIO_DAILY","returns_status":"OK",
            "returns_reason":null,"returns":[{"timestamp_ns":"9007199254740993",
                "value":9.999450030218071e-6_f64,"reason_code":null}],
            "canonical_result":{"preserved":true}}
    });
    for (wire, is_current) in [(current, true), (historical, false)] {
        let parsed: StrategyCompositionOutcomeV1 =
            serde_json::from_slice(&serde_json::to_vec(&wire).unwrap()).unwrap();
        assert_eq!(
            matches!(parsed, StrategyCompositionOutcomeV1::CurrentDecision { .. }),
            is_current
        );
        let restored = serde_json::to_value(parsed).unwrap();
        assert_eq!(restored, wire);
        if !is_current {
            assert_eq!(
                restored["simulation"]["returns"][0]["value"]
                    .as_f64()
                    .unwrap()
                    .to_bits(),
                9.999450030218071e-6_f64.to_bits()
            );
        }
    }
}

#[test]
fn indirection_keeps_envelopes_small_and_preserves_existing_schemas() {
    // Each envelope stores a discriminant and one pointer, regardless of payload size.
    for size in [
        size_of::<MandateCreateEnvelopeV2>(),
        size_of::<MandateViewEnvelopeV2>(),
        size_of::<TargetPackageEnvelopeV2>(),
    ] {
        assert!(size <= 2 * size_of::<usize>());
    }
    // Current decisions keep targets and counters inline, with only account metadata indirect.
    assert!(
        size_of::<StrategyCompositionOutcomeV1>()
            <= size_of::<contracts::science::NativeTargetPointV1>()
                + size_of::<BTreeMap<Id, DbCounter>>()
                + size_of::<usize>()
                + 2 * align_of::<StrategyCompositionOutcomeV1>()
    );
    let committed: Value = serde_json::from_str(include_str!(
        "../../../contracts/generated/domain-v1.openapi.json"
    ))
    .unwrap();
    for (name, schema) in [
        ("MandateCreateEnvelopeV2", MandateCreateEnvelopeV2::schema()),
        ("MandateViewEnvelopeV2", MandateViewEnvelopeV2::schema()),
        (
            "StrategyCompositionOutcomeV1",
            StrategyCompositionOutcomeV1::schema(),
        ),
        ("TargetPackageEnvelopeV2", TargetPackageEnvelopeV2::schema()),
    ] {
        assert_eq!(
            serde_json::to_value(schema).unwrap(),
            committed["components"]["schemas"][name],
            "{name}"
        );
    }
}
