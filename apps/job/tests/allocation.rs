//! Native Clarabel solves, checked against independent small analytical answers.
//! Example instruments and assumptions are SYNTHETIC, not qualification evidence.
use bigdecimal::ToPrimitive;
use contracts::{portfolio::*, DecimalValue, Id};

fn input() -> AllocationInputV1 {
    serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap()
}
fn decimal(value: &str) -> DecimalValue {
    value.parse().unwrap()
}

fn forecast_input() -> PortfolioForecastInputV1 {
    use contracts::{brief::HorizonKind, evidence::ForecastUnit, DbCounter, SchemaV1};
    let instruments: Vec<_> = input()
        .assets
        .into_iter()
        .map(|a| a.instrument_id)
        .collect();
    let asof = DbCounter::new(10_000_000_000).unwrap();
    let decision = DbCounter::new(20_000_000_000).unwrap();
    let horizon = DbCounter::new(4).unwrap();
    let bars: Vec<_> = instruments
        .iter()
        .map(|id| format!("{id}-1-MINUTE-LAST-EXTERNAL"))
        .collect();
    PortfolioForecastInputV1 {
        schema_version: SchemaV1,
        decision_asof_ns: decision,
        forecast_asof_ns: asof,
        horizon_kind: HorizonKind::FixedBars,
        horizon_value: horizon,
        base_currency: "USD".into(),
        max_input_age_seconds: 10,
        bar_types: bars.clone(),
        instrument_ids: instruments.clone(),
        members: [(vec![0.1, 0.2], "0.25"), (vec![0.3, 0.0], "0.75")]
            .into_iter()
            .map(|(forecasts, weight)| AlphaForecastV1 {
                alpha_id: Id::new(),
                alpha_version_id: Id::new(),
                forecast_unit: ForecastUnit::ReturnPerHorizon,
                horizon_kind: HorizonKind::FixedBars,
                horizon_value: horizon,
                base_currency: "USD".into(),
                asof_ns: asof,
                available_ns: decision,
                ensemble_weight: decimal(weight),
                bar_types: bars.clone(),
                instrument_ids: instruments.clone(),
                forecasts,
            })
            .collect(),
    }
}
fn weights(request: &AllocationInputV1) -> Vec<f64> {
    let result = job::allocate(request).unwrap();
    assert_eq!(result.solver_status, SolverStatus::Optimal, "{result:?}");
    domain::portfolio::allocation_result(request, &result).unwrap();
    result
        .targets
        .unwrap()
        .iter()
        .map(|v| v.weight.as_decimal().to_f64().unwrap())
        .collect()
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() <= 1e-5, "{actual} != {expected}");
}

#[test]
fn native_minimum_risk_uses_the_whole_covariance_not_equal_weights() {
    let request = input();
    let result = weights(&request);
    near(result[0], 0.8);
    near(result[1], 0.2);
    let mut changed = request;
    changed.covariance = vec![vec![4.0, 0.0], vec![0.0, 1.0]];
    let changed = weights(&changed);
    near(changed[0], 0.2);
    near(changed[1], 0.8);
}

#[test]
fn native_utility_and_transaction_costs_change_the_optimum() {
    let mut request = input();
    request.objective = AllocationObjective::MaxUtility;
    let result = weights(&request);
    // f(x)=x²+4(1-x)²-0.1x-0.2(1-x), derivative 10x-7.9.
    near(result[0], 0.79);
    request.objective = AllocationObjective::MinRisk;
    for asset in &mut request.assets {
        asset.transaction_cost_rate = decimal("0.5");
    }
    // Both legs cost 0.5*|x-0.5|: derivative 10x-8+1 for x>=0.5.
    near(weights(&request)[0], 0.7);
}

#[test]
fn actual_fixed_mixture_predictions_feed_one_native_utility_problem() {
    let forecasts = forecast_input();
    let forecast = job::validation::aligned_portfolio_forecast(&forecasts).unwrap();
    near(forecast[0], 0.25);
    near(forecast[1], 0.05);
    let mut request = input();
    request.objective = AllocationObjective::MaxUtility;
    request.forecasts = forecasts.clone();
    // x²+4(1-x)²-0.25x-0.05(1-x) has derivative 10x-8.2.
    let target = weights(&request);
    near(target[0], 0.82);
    near(target[1], 0.18);
    assert_eq!(
        forecasts
            .members
            .iter()
            .map(|m| m.ensemble_weight.clone())
            .collect::<Vec<_>>(),
        [decimal("0.25"), decimal("0.75")]
    );
}

#[test]
fn incompatible_original_forecasts_never_reach_native_aggregation() {
    use contracts::{brief::HorizonKind, evidence::ForecastUnit, DbCounter};
    let mutations: &[fn(&mut PortfolioForecastInputV1)] = &[
        |r| r.members[1].alpha_id = r.members[0].alpha_id,
        |r| r.members[1].alpha_version_id = r.members[0].alpha_version_id,
        |r| r.members[1].forecast_unit = ForecastUnit::UnitlessScore,
        |r| r.members[1].forecast_unit = ForecastUnit::ResidualReturnPerHorizon,
        |r| r.members[1].base_currency = "EUR".into(),
        |r| r.members[1].horizon_kind = HorizonKind::FixedDuration,
        |r| r.members[1].horizon_value = DbCounter::new(5).unwrap(),
        |r| r.members[1].asof_ns = DbCounter::new(9_000_000_000).unwrap(),
        |r| r.members[1].available_ns = DbCounter::new(20_000_000_001).unwrap(),
        |r| r.members[1].available_ns = DbCounter::new(9_999_999_999).unwrap(),
        |r| r.members[1].instrument_ids.swap(0, 1),
        |r| r.members[1].bar_types[0] = r.members[1].bar_types[0].replace("MINUTE", "HOUR"),
        |r| {
            r.members[1].forecasts.pop();
        },
        |r| r.members[1].forecasts[0] = f64::NAN,
        |r| r.instrument_ids[1] = r.instrument_ids[0].clone(),
        |r| r.max_input_age_seconds = 9,
        |r| r.max_input_age_seconds = 0,
        |r| r.decision_asof_ns = DbCounter::new(9_999_999_999).unwrap(),
        |r| r.horizon_kind = HorizonKind::VariableInterval,
    ];
    for (index, change) in mutations.iter().enumerate() {
        let mut request = forecast_input();
        change(&mut request);
        assert!(
            job::validation::aligned_portfolio_forecast(&request).is_err(),
            "case {index}"
        );
    }
    for malformed in [true, false] {
        let mut request = forecast_input();
        request.bar_types[0] = if malformed {
            "invalid-native-bar".into()
        } else {
            request.bar_types[0].replace("MINUTE", "HOUR")
        };
        for member in &mut request.members {
            member.bar_types = request.bar_types.clone();
        }
        assert!(job::validation::aligned_portfolio_forecast(&request).is_err());
    }
    let mut request = forecast_input();
    let mut extra = request.members[0].clone();
    extra.alpha_version_id = Id::new();
    extra.ensemble_weight = decimal("0.125");
    request.members[0].ensemble_weight = decimal("0.125");
    request.members.push(extra);
    job::validation::aligned_portfolio_forecast(&request).unwrap();
    request.members[1].ensemble_weight = decimal("0");
    request.members[0].ensemble_weight = decimal("0.5");
    request.members[2].ensemble_weight = decimal("0.5");
    assert!(
        job::validation::aligned_portfolio_forecast(&request).is_err(),
        "two positive versions of one Alpha cannot count as two distinct positive Alphas"
    );
    let mut request = forecast_input();
    request.members[0].forecasts[0] = f64::NAN;
    assert!(
        serde_json::to_value(&request).is_err(),
        "NaN cannot turn into a missing value"
    );
}

#[test]
fn native_mixture_rejects_missing_predictions_and_never_repairs_weights() {
    use job::validation::fixed_weighted_forecast as mix;
    let forecasts = [vec![0.1, 0.2], vec![0.3, 0.0]];
    for weights in [
        vec!["1"],
        vec!["1", "1"],
        vec!["1", "0"],
        vec!["-1", "2"],
        vec!["0.5", "0.500000000000000001"],
    ] {
        assert!(mix(
            &forecasts,
            &weights.into_iter().map(decimal).collect::<Vec<_>>()
        )
        .is_err());
    }
    let weights = [decimal("0.5"), decimal("0.5")];
    for forecasts in [
        vec![],
        vec![vec![1.0]],
        vec![vec![], vec![]],
        vec![vec![1.0], vec![1.0, 2.0]],
        vec![vec![f64::NAN], vec![0.0]],
        vec![vec![1.0], vec![f64::INFINITY]],
        vec![vec![1.0; MAX_ALLOCATION_ASSETS + 1]; 2],
    ] {
        assert!(mix(&forecasts, &weights).is_err());
    }
    let rows = vec![vec![0.1]; MAX_ALLOCATION_ASSETS + 1];
    assert!(mix(&rows, &vec![decimal("0.5"); rows.len()]).is_err());
}

#[test]
fn asset_overrides_and_group_bounds_enter_the_native_problem() {
    let mut request = input();
    request.constraints.asset_overrides.push(AssetBoundV1 {
        instrument_id: request.assets[0].instrument_id.clone(),
        min: decimal("0"),
        max: decimal("0.6"),
    });
    near(weights(&request)[0], 0.6);
    request.constraints.group_bounds.push(GroupBoundV1 {
        group_id: "sector-a".into(),
        min: decimal("0.3"),
        max: decimal("0.4"),
    });
    near(weights(&request)[0], 0.4);
}

#[test]
fn cash_and_net_exposure_are_one_shared_capital_constraint() {
    let mut request = input();
    request.constraints.min_cash_weight = decimal("0.2");
    request.constraints.max_cash_weight = decimal("0.2");
    request.constraints.min_net_exposure = decimal("0.8");
    request.constraints.max_net_exposure = decimal("0.8");
    let result = job::allocate(&request).unwrap();
    assert_eq!(result.solver_status, SolverStatus::Optimal);
    near(
        result.cash_weight.unwrap().as_decimal().to_f64().unwrap(),
        0.2,
    );
    let result = weights(&request);
    near(result[0], 0.64);
    near(result[1], 0.16);
}

#[test]
fn turnover_and_real_notional_participation_bound_both_legs() {
    let mut request = input();
    request.constraints.max_turnover_per_rebalance = decimal("0.1");
    near(weights(&request)[0], 0.55);
    request.constraints.max_turnover_per_rebalance = decimal("2");
    request.constraints.max_participation = Some(decimal("0.1"));
    request.constraints.liquidity_ref = Some(Id::new());
    for asset in &mut request.assets {
        asset.available_notional = Some(decimal("500000"));
    }
    near(weights(&request)[0], 0.55);
    for asset in &mut request.assets {
        asset.available_notional = Some(decimal("0"));
    }
    near(weights(&request)[0], 0.5);
}

#[test]
fn signed_exposures_obey_native_gross_and_net_limits() {
    let mut request = input();
    request.objective = AllocationObjective::MaxUtility;
    request.risk_aversion = decimal("0.1");
    request.constraints.long_only = false;
    request.constraints.min_asset_weight = decimal("-1");
    request.constraints.max_asset_weight = decimal("2");
    request.constraints.max_gross_exposure = decimal("1.4");
    for member in &mut request.forecasts.members {
        member.forecasts[0] = 5.0;
    }
    let result = weights(&request);
    near(result[0], 1.2);
    near(result[1], -0.2);
}

#[test]
fn native_infeasibility_never_publishes_a_certificate_as_targets() {
    let mut request = input();
    request.constraints.max_asset_weight = decimal("0.1");
    let result = job::allocate(&request).unwrap();
    assert_eq!(result.solver_status, SolverStatus::Infeasible);
    assert!(result.targets.is_none());
    assert!(result.cash_weight.is_none());
    assert!(result.reason_code.is_some());
    domain::portfolio::allocation_result(&request, &result).unwrap();
}

#[test]
fn no_iteration_or_accuracy_failure_becomes_a_fallback_allocation() {
    let mut request = input();
    let NativeModelRefV1::ClarabelQp { parameters, .. } = &mut request.optimizer else {
        panic!("native optimizer");
    };
    parameters.max_iterations = 1;
    let result = job::allocate(&request).unwrap();
    assert_eq!(result.solver_status, SolverStatus::Failed);
    assert!(result.targets.is_none() && result.cash_weight.is_none());
}

#[test]
fn native_model_references_bind_roles_versions_and_strict_parameters() {
    let original = input();
    let result = job::allocate(&original).unwrap();
    for role in ["optimizer", "alpha_ensemble"] {
        for (field, value) in [
            ("adapter_kind", serde_json::json!("DEFAULT")),
            ("upstream_class", serde_json::json!("unknown::Model")),
            ("upstream_version", serde_json::json!("0.0.0")),
            ("schema_version", serde_json::json!(2)),
            ("extra", serde_json::json!(true)),
        ] {
            let mut bad = serde_json::to_value(&original).unwrap();
            bad[role][field] = value;
            if let Ok(bad) = serde_json::from_value::<AllocationInputV1>(bad) {
                assert!(job::allocate(&bad).is_err(), "{role}.{field}");
                assert!(domain::portfolio::allocation_result(&bad, &result).is_err());
            }
        }
        let mut bad = serde_json::to_value(&original).unwrap();
        bad[role]["parameters"]["unknown"] = serde_json::json!(true);
        assert!(serde_json::from_value::<AllocationInputV1>(bad).is_err());
        let mut missing = serde_json::to_value(&original).unwrap();
        missing.as_object_mut().unwrap().remove(role);
        assert!(serde_json::from_value::<AllocationInputV1>(missing).is_err());
    }
    let mut reversed = original.clone();
    std::mem::swap(&mut reversed.optimizer, &mut reversed.alpha_ensemble);
    assert!(job::allocate(&reversed).is_err());
    assert!(domain::portfolio::allocation_result(&reversed, &result).is_err());
    let mut legacy = serde_json::to_value(&original).unwrap();
    legacy["settings"] = legacy["optimizer"]["parameters"].clone();
    assert!(serde_json::from_value::<AllocationInputV1>(legacy).is_err());
}

#[test]
fn invalid_or_unsupported_inputs_are_not_silently_repaired() {
    let mut legacy = serde_json::to_value(input()).unwrap();
    legacy["assets"][0]["expected_return"] = serde_json::json!(100);
    assert!(serde_json::from_value::<AllocationInputV1>(legacy).is_err());
    let mut missing = serde_json::to_value(input()).unwrap();
    missing.as_object_mut().unwrap().remove("forecasts");
    assert!(serde_json::from_value::<AllocationInputV1>(missing).is_err());
    let mut request = input();
    request.covariance[0][1] = 0.1;
    assert!(job::allocate(&request).is_err());
    request.covariance = vec![vec![1.0, 2.0], vec![2.0, 1.0]];
    assert!(job::allocate(&request).is_err());
    request = input();
    request.covariance[0][0] = f64::NAN;
    assert!(job::allocate(&request).is_err());
    assert!(serde_json::to_value(&request).is_err());
    request = input();
    request.assets[1].instrument_id = request.assets[0].instrument_id.clone();
    assert!(job::allocate(&request).is_err());
    request = input();
    request.assets[0].transaction_cost_rate = decimal("-0.1");
    assert!(job::allocate(&request).is_err());
    request = input();
    request.constraints.max_participation = Some(decimal("0.1"));
    assert!(job::allocate(&request).is_err());
    request = input();
    request.current_cash_weight = decimal("0.1");
    assert!(job::allocate(&request).is_err());
    request = input();
    request.risk = AllocationRisk::Cvar;
    assert!(job::allocate(&request).is_err());
    request = input();
    request.objective = AllocationObjective::RiskBudgeting;
    assert!(job::allocate(&request).is_err());
    request = input();
    request.constraints.max_ex_ante_risk = Some(decimal("0.1"));
    assert!(job::allocate(&request).is_err());
}

#[test]
fn publication_rechecks_exact_decimal_targets_and_original_identities() {
    let request = input();
    let original = job::allocate(&request).unwrap();
    let mut result = original.clone();
    result.targets.as_mut().unwrap()[0].weight = decimal("2");
    assert!(domain::portfolio::allocation_result(&request, &result).is_err());
    result = original.clone();
    result.targets.as_mut().unwrap().swap(0, 1);
    assert!(domain::portfolio::allocation_result(&request, &result).is_err());
    result = original.clone();
    result.targets.as_mut().unwrap()[0].currency = "EUR".into();
    assert!(domain::portfolio::allocation_result(&request, &result).is_err());
    result = original.clone();
    result.cash_weight = None;
    assert!(domain::portfolio::allocation_result(&request, &result).is_err());
    result = original;
    result.solver_status = SolverStatus::Infeasible;
    assert!(domain::portfolio::allocation_result(&request, &result).is_err());
}

#[test]
fn native_cli_consumes_exact_json_without_issuing_delivery_authority() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_job"))
        .arg("allocate")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(include_bytes!(
            "../../../tests/contracts/allocation-input.json"
        ))
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: AllocationResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result.solver_status, SolverStatus::Optimal);
    domain::portfolio::allocation_result(&input(), &result).unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("approval"));
}
