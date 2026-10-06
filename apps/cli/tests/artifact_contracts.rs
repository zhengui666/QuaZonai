//! Offline installed-binary discovery and native wire parsing, not scientific evaluation.
use contracts::science::{ExperimentEvaluationParametersV1, FeatureObservationsV1};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::process::{Command, Output};

fn invoke(args: &[&str]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_quazonai"))
        .env_clear()
        .current_dir(directory.path())
        .args(args)
        .output()
        .unwrap()
}

fn successful(args: &[&str]) -> Value {
    let output = invoke(args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn check_refs(value: &Value, root: &Value) {
    match value {
        Value::Object(fields) => {
            if let Some(reference) = fields.get("$ref") {
                let pointer = reference.as_str().unwrap().strip_prefix('#').unwrap();
                assert!(root.pointer(pointer).is_some(), "unresolved {pointer}");
            }
            for value in fields.values() {
                check_refs(value, root);
            }
        }
        Value::Array(values) => {
            for value in values {
                check_refs(value, root);
            }
        }
        _ => {}
    }
}

#[test]
fn domain_discovery_reuses_native_export_without_changing_default_http_contracts() {
    let native: Value = serde_json::from_str(&contracts::openapi_json().unwrap()).unwrap();
    assert_eq!(successful(&["openapi", "--domain"]), native);
    let api: Value = serde_json::from_str(include_str!(
        "../../../contracts/generated/api-v2.openapi.json"
    ))
    .unwrap();
    assert_eq!(successful(&["openapi"]), api);
    let names = successful(&["openapi", "--domain", "--list-schemas"]);
    assert_eq!(names["schema_version"], 1);
    assert_eq!(
        names["schemas"],
        json!(native["components"]["schemas"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>())
    );
    for name in ["ExperimentEvaluationParametersV1", "FeatureObservationsV1"] {
        let selected = successful(&["openapi", "--domain", "--schema", name]);
        assert_eq!(selected["name"], name);
        assert_eq!(selected["schema_version"], 1);
        assert_eq!(
            selected["schema"]["$ref"],
            format!("#/components/schemas/{name}")
        );
        check_refs(&selected, &selected);
        let schemas = selected["components"]["schemas"].as_object().unwrap();
        assert!(schemas.contains_key(name));
        assert!(schemas.len() < native["components"]["schemas"].as_object().unwrap().len());
        for (name, schema) in schemas {
            assert_eq!(schema, &native["components"]["schemas"][name]);
        }
    }
}

fn assert_wire<T: DeserializeOwned + Serialize>(name: &str, input: Value) {
    let selected = successful(&["openapi", "--domain", "--schema", name]);
    let schema = &selected["components"]["schemas"][name];
    let parsed: T = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), input);
    assert_eq!(schema["additionalProperties"], false);
    let mut extra = input.clone();
    extra["unknown_field"] = json!(true);
    assert!(serde_json::from_value::<T>(extra).is_err());
    for field in schema["required"].as_array().unwrap() {
        let mut missing = input.clone();
        missing
            .as_object_mut()
            .unwrap()
            .remove(field.as_str().unwrap());
        assert!(
            serde_json::from_value::<T>(missing).is_err(),
            "{name}: missing {field}"
        );
    }
    assert_eq!(
        schema["properties"]["schema_version"]["$ref"],
        "#/components/schemas/SchemaV1"
    );
    assert_eq!(
        selected["components"]["schemas"]["SchemaV1"]["enum"],
        json!([1])
    );
    assert_eq!(
        selected["components"]["schemas"]["SchemaV1"]["type"],
        "integer"
    );
    for version in [json!("1"), json!(2), Value::Null] {
        let mut invalid = input.clone();
        invalid["schema_version"] = version;
        assert!(serde_json::from_value::<T>(invalid).is_err());
    }
    assert_eq!(
        selected["components"]["schemas"]["DbCounter"]["type"],
        "string"
    );
}

#[test]
fn discovered_scientific_contracts_match_native_version_strictness_and_counter_types() {
    let features = json!([{"feature_key":"fixture","source_ref":"fixture:wire-test",
        "source_key":"fixture","availability":{"basis":"OBSERVED"},"max_age_ns":null}]);
    let observations = json!({"schema_version":1,"partition":"VALIDATION",
        "feature_schema":features,"observations":[{"feature_index":0,
        "event_ns":"9007199254740993","observed_available_ns":null,"sequence":"0",
        "value":0.0,"missing_reason":null}]});
    assert_wire::<FeatureObservationsV1>("FeatureObservationsV1", observations.clone());
    let mut invalid = observations.clone();
    invalid["observations"][0]["event_ns"] = json!(9007199254740993_u64);
    assert!(serde_json::from_value::<FeatureObservationsV1>(invalid).is_err());
    let mut missing_value = observations;
    missing_value["observations"][0]["value"] = Value::Null;
    missing_value["observations"][0]["missing_reason"] = json!("fixture missing");
    assert_wire::<FeatureObservationsV1>("FeatureObservationsV1", missing_value);

    let parameters = json!({"schema_version":1,
        "dataset_revision_id":"018fc823-8e40-7000-8000-000000000001",
        "feature_artifact_ids":["018fc823-8e40-7000-8000-000000000002"],
        "instrument_id":"FIXTURE.SIM","feature_schema":features,
        "label_horizon_observations":1,"total_fuel":"9007199254740993",
        "target_ttl_ns":"1","decision_output":"TARGET_WEIGHT",
        "settings":{"schema_version":1,"base_currency":"USD","starting_capital":"1000",
        "account_kind":"CASH","leverage":"1","snapshot_interval_ms":1,"exposure_tolerance":"0.01",
        "fee_rates":[{"instrument_id":"FIXTURE.SIM","maker":"0","taker":"0"}],
        "fee_model":{"schema_version":1,"adapter_kind":"NAUTILUS_MAKER_TAKER",
            "upstream_class":"fixture","upstream_version":"fixture","parameters":{}},
        "fill_model":{"schema_version":1,"adapter_kind":"NAUTILUS_DEFAULT_FILL",
            "upstream_class":"fixture","upstream_version":"fixture",
            "parameters":{"prob_fill_on_limit":"1","prob_slippage":"0","random_seed":"1"}},
        "latency_model":{"schema_version":1,"adapter_kind":"NAUTILUS_STATIC_LATENCY",
            "upstream_class":"fixture","upstream_version":"fixture",
            "parameters":{"base_latency_ns":"0","insert_latency_ns":"0","update_latency_ns":"0","cancel_latency_ns":"0"}}}});
    assert_wire::<ExperimentEvaluationParametersV1>(
        "ExperimentEvaluationParametersV1",
        parameters.clone(),
    );
    let mut invalid = parameters;
    invalid["total_fuel"] = json!(9007199254740993_u64);
    assert!(serde_json::from_value::<ExperimentEvaluationParametersV1>(invalid).is_err());
}

#[test]
fn unknown_or_unavailable_version_names_and_conflicting_selectors_fail_without_output() {
    for name in [
        "DoesNotExist",
        "FeatureObservationsV2",
        "ExperimentEvaluationParametersV2",
    ] {
        let output = invoke(&["openapi", "--domain", "--schema", name]);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("CLI_INPUT_INVALID"));
    }
    let output = invoke(&[
        "openapi",
        "--domain",
        "--schema",
        "FeatureObservationsV1",
        "--list-schemas",
    ]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn portable_skill_scientific_discovery_examples_execute_without_repository_or_login() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/quazonai");
    for file in ["references/connection.md", "references/research.md"] {
        let instructions = std::fs::read_to_string(source.join(file)).unwrap();
        let mut count = 0;
        for line in instructions
            .lines()
            .filter(|line| line.starts_with("quazonai openapi --domain"))
        {
            let args: Vec<_> = line.split_whitespace().skip(1).collect();
            successful(&args);
            count += 1;
        }
        assert_eq!(count, 2, "{file}");
    }
    assert!(std::fs::read_to_string(source.join("SKILL.md"))
        .unwrap()
        .contains("quazonai openapi --domain --schema NAME"));
}
