//! Fuel absence is an execution mode, never a sentinel maximum or invented zero.
use contracts::{science::*, DbCounter};
use serde_json::{json, to_value};
use utoipa::PartialSchema;

#[test]
fn omitted_fuel_is_unmetered_and_explicit_large_budget_round_trips() {
    let mut wire =
        json!({"schema_version":1,"fast_period":2,"slow_period":5,"label_horizon_observations":1});
    let request: NativeForecastParametersV1 = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(request.total_fuel, None);
    assert!(to_value(request).unwrap().get("total_fuel").is_none());
    wire["total_fuel"] = json!("1000000001");
    let request: NativeForecastParametersV1 = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(
        request.total_fuel,
        Some(DbCounter::new(1_000_000_001).unwrap())
    );
    assert_eq!(to_value(request).unwrap(), wire);
    wire["total_fuel"] = json!(1_000_000_001_u64);
    assert!(serde_json::from_value::<NativeForecastParametersV1>(wire).is_err());
}

#[test]
fn unmetered_output_is_explicit_null_and_measured_zero_remains_distinct() {
    let wire = json!({"schema_version":1,"native_versions":{},"consumed_fuel":null,"points":[]});
    let mut result: NativeForecastResultV1 = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(result.consumed_fuel, None);
    assert_eq!(to_value(&result).unwrap(), wire);
    result.consumed_fuel = Some(DbCounter::ZERO);
    assert_eq!(to_value(&result).unwrap()["consumed_fuel"], "0");
}

#[test]
fn generated_schema_expresses_optional_budget_and_required_nullable_measurement() {
    let schema: serde_json::Value =
        serde_json::from_str(&contracts::openapi_json().unwrap()).unwrap();
    let schemas = &schema["components"]["schemas"];
    for name in [
        "NativeForecastParametersV1",
        "ExperimentEvaluationParametersV1",
        "NativeExperimentEvaluationRequestV1",
        "NativeStrategyCompositionRequestV1",
    ] {
        let request = &schemas[name];
        assert!(
            !request["required"]
                .as_array()
                .unwrap_or_else(|| panic!("missing {name}: {request}"))
                .iter()
                .any(|v| v == "total_fuel"),
            "{name}"
        );
    }
    for name in [
        "NativeForecastResultV1",
        "NativeAlphaValidationResultV1",
        "NativeExperimentEvaluationResultV1",
        "NativeStrategyCompositionResultV1",
    ] {
        let result = &schemas[name];
        assert!(
            result["required"]
                .as_array()
                .unwrap_or_else(|| panic!("missing {name}: {result}"))
                .iter()
                .any(|v| v == "consumed_fuel"),
            "{name}"
        );
        assert!(
            result["properties"]["consumed_fuel"]["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["type"] == "null"),
            "{name}"
        );
    }
}

#[test]
fn portfolio_schema_keeps_unmetered_aggregate_explicit() {
    for result in [
        to_value(NativePortfolioBuildResultV1::schema()).unwrap(),
        to_value(NativePortfolioStudyResultV1::schema()).unwrap(),
    ] {
        assert!(result["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "consumed_fuel"));
        assert!(result["properties"]["consumed_fuel"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["type"] == "null"));
    }
}
