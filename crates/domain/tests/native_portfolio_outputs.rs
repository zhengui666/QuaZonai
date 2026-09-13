//! Synthetic structural counterexamples, not native numerical or market evidence.
//! Genuine solver/account output is independently exercised by job/tests/managed.
#[path = "../../../tests/support/execution_models.rs"]
mod execution_models;
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio_config;
use chrono::{DateTime, Utc};
use contracts::{
    evidence::MetricStatus,
    execution::NativeTaskParametersV1,
    portfolio::*,
    runtime_jobs::{native_output_contract, RuntimeOutputV1},
    science::*,
    DbCounter, Id, Revision, SchemaV1,
};
use domain::execution::output_bindings;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn native_portfolio_refuses_unbound_liquidity_numbers_or_references() {
    let input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let original = portfolio_config::request(&input);
    domain::execution::portfolio_build_request(&original).unwrap();
    for case in 0..3 {
        let mut request = original.clone();
        match case {
            0 => request.assets[0].available_notional = Some("100".parse().unwrap()),
            1 => request.mandate.constraints.liquidity_ref = Some(Id::new()),
            _ => request.mandate.constraints.max_participation = Some("0.1".parse().unwrap()),
        }
        assert!(domain::execution::portfolio_build_request(&request).is_err());
    }
}

#[test]
fn mandate_constraints_use_the_same_checks_as_native_allocation() {
    let mut request: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    domain::portfolio::portfolio_constraints(&request.constraints).unwrap();
    request.constraints.asset_overrides = vec![
        AssetBoundV1 {
            instrument_id: request.assets[0].instrument_id.clone(),
            min: "0".parse().unwrap(),
            max: "1".parse().unwrap(),
        };
        2
    ];
    assert!(domain::portfolio::portfolio_constraints(&request.constraints).is_err());
    assert!(domain::portfolio::allocation_input(&request).is_err());
    request.constraints.asset_overrides.clear();
    request.constraints.min_cash_weight = "2".parse().unwrap();
    request.constraints.max_cash_weight = "1".parse().unwrap();
    assert!(domain::portfolio::portfolio_constraints(&request.constraints).is_err());
    assert!(domain::portfolio::allocation_input(&request).is_err());
}

#[test]
fn rebalance_intent_requires_exact_kind_fields_and_native_iana_timezone() {
    let mut schedule = RebalanceScheduleV1 {
        schema_version: SchemaV1,
        kind: RebalanceKind::Manual,
        interval_seconds: None,
        calendar_ref: None,
        timezone: "America/New_York".into(),
        session_offset_seconds: None,
        max_input_age_seconds: 60,
        target_ttl_seconds: 60,
    };
    domain::portfolio::rebalance_schedule(&schedule).unwrap();
    schedule.interval_seconds = Some(60);
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    schedule.kind = RebalanceKind::FixedInterval;
    domain::portfolio::rebalance_schedule(&schedule).unwrap();
    schedule.interval_seconds = Some(0);
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    schedule.kind = RebalanceKind::CalendarSession;
    schedule.interval_seconds = None;
    schedule.calendar_ref = Some("XNYS-2026".into());
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    schedule.session_offset_seconds = Some(-300);
    domain::portfolio::rebalance_schedule(&schedule).unwrap();
    schedule.timezone = "Not/A_Zone".into();
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    schedule.timezone = "Asia/Shanghai".into();
    schedule.target_ttl_seconds = 0;
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    schedule.target_ttl_seconds = 60;
    schedule.max_input_age_seconds = 0;
    assert!(domain::portfolio::rebalance_schedule(&schedule).is_err());
    let mut wire = serde_json::to_value(&schedule).unwrap();
    wire["fallback_timezone"] = json!("UTC");
    assert!(serde_json::from_value::<RebalanceScheduleV1>(wire).is_err());
}

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn accepts<T: serde::Serialize>(
    parameters: &NativeTaskParametersV1,
    name: &str,
    value: &T,
) -> bool {
    let contract = native_output_contract(name, "1").unwrap();
    let bytes = serde_json::to_vec(value).unwrap();
    let descriptor = RuntimeOutputV1 {
        kind: contract.kind,
        schema: contracts::runtime::RuntimeArtifactSchemaV1 {
            name: name.into(),
            version: "1".into(),
        },
        storage_ref: Id::new(),
        storage_version: Revision::INITIAL,
        byte_count: count(bytes.len() as u64),
        media_type: contract.media_type.into(),
    };
    let now = DateTime::<Utc>::from_timestamp(100, 0).unwrap();
    output_bindings(parameters, None, now, now, &[(descriptor, bytes)]).is_ok()
}

#[test]
fn allocation_success_requires_exact_instruments_currency_weights_and_solver_contract() {
    let request: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let parameters = NativeTaskParametersV1::BuildPortfolio {
        schema_version: SchemaV1,
        dataset_revision_id: Id::new(),
        request: Box::new(portfolio_config::request(&request)),
    };
    let accepts_allocation = |parameters: &NativeTaskParametersV1,
                              allocation: &AllocationResultV1| {
        accepts(
            parameters,
            "qz.native_portfolio",
            &NativePortfolioBuildResultV1 {
                schema_version: SchemaV1,
                input: request.clone(),
                allocation: allocation.clone(),
                consumed_fuel: DbCounter::new(1).unwrap(),
            },
        )
    };
    let result = AllocationResultV1 {
        schema_version: SchemaV1,
        solver_status: SolverStatus::Optimal,
        reason_code: None,
        targets: Some(vec![
            AllocationTargetV1 {
                instrument_id: request.assets[0].instrument_id.clone(),
                currency: "USD".into(),
                weight: "0.8".parse().unwrap(),
            },
            AllocationTargetV1 {
                instrument_id: request.assets[1].instrument_id.clone(),
                currency: "USD".into(),
                weight: "0.2".parse().unwrap(),
            },
        ]),
        cash_weight: Some("0".parse().unwrap()),
        iterations: 10,
        cvar_risk_budget_witness: None,
        objective_value: Some(0.8),
        primal_residual: Some(0.0),
        dual_residual: Some(0.0),
    };
    assert!(accepts_allocation(&parameters, &result));
    let mut wrong_forecasts = request.clone();
    wrong_forecasts.assets.swap(0, 1);
    assert!(!accepts_allocation(
        &NativeTaskParametersV1::BuildPortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: Id::new(),
            request: Box::new(portfolio_config::request(&wrong_forecasts)),
        },
        &result
    ));
    for dimension in 0..12 {
        let mut invalid = result.clone();
        match dimension {
            0 => invalid.targets = Some(Vec::new()),
            1 => invalid.targets.as_mut().unwrap()[0].instrument_id = "OTHER.EXAMPLE".into(),
            2 => invalid.targets.as_mut().unwrap()[0].currency = "EUR".into(),
            3 => invalid.targets.as_mut().unwrap().swap(0, 1),
            4 => invalid.targets.as_mut().unwrap()[0].weight = "1.1".parse().unwrap(),
            5 => invalid.cash_weight = Some("0.2".parse().unwrap()),
            6 => invalid.solver_status = SolverStatus::AcceptableInaccurate,
            7 => invalid.reason_code = Some("NO_SOLUTION".into()),
            8 => invalid.objective_value = None,
            9 => invalid.primal_residual = Some(-0.1),
            10 => {
                invalid.iterations = domain::portfolio::optimizer_settings(&request.optimizer)
                    .unwrap()
                    .max_iterations
                    + 1
            }
            _ => invalid.cash_weight = None,
        }
        assert!(
            !accepts_allocation(&parameters, &invalid),
            "allocation dimension {dimension}"
        );
    }
    let unavailable = AllocationResultV1 {
        solver_status: SolverStatus::Infeasible,
        reason_code: Some("SOLVER_INFEASIBLE".into()),
        targets: None,
        cash_weight: None,
        objective_value: None,
        primal_residual: None,
        dual_residual: None,
        ..result
    };
    assert!(accepts_allocation(&parameters, &unavailable));
    let mut fabricated_fallback = unavailable;
    fabricated_fallback.cash_weight = Some("1".parse().unwrap());
    assert!(!accepts_allocation(&parameters, &fabricated_fallback));
}

fn simulation() -> (NativeTaskParametersV1, NativeSimulationResultV1) {
    let day = 86_400_000_000_000;
    let start = day + 1;
    let end = 2 * day;
    let request = NativeSimulationRequestV1 {
        schema_version: SchemaV1,
        selection: NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: vec!["A.SIM-1-SECOND-LAST-EXTERNAL".into()],
            event_start_ns: count(start),
            event_end_ns: count(end + 1),
            decision_cutoff_ns: count(end + 2),
            maximum_rows: 100,
        },
        settings: NativeSimulationSettingsV1 {
            schema_version: SchemaV1,
            base_currency: "USD".into(),
            starting_capital: "1000".parse().unwrap(),
            account_kind: NativeAccountKind::Cash,
            leverage: "1".parse().unwrap(),
            fill_model: execution_models::fill(),
            fee_model: execution_models::fee(),
            latency_model: execution_models::latency(1),
            snapshot_interval_ms: 1000,
            exposure_tolerance: "0.000001".parse().unwrap(),
            fee_rates: vec![NativeFeeRateV1 {
                instrument_id: "A.SIM".into(),
                maker: "0".parse().unwrap(),
                taker: "0".parse().unwrap(),
            }],
        },
        target_points: vec![NativeTargetPointV1 {
            schema_version: SchemaV1,
            asof_ns: count(start),
            valid_until_ns: count(end),
            targets: vec![AllocationTargetV1 {
                instrument_id: "A.SIM".into(),
                currency: "USD".into(),
                weight: "0".parse().unwrap(),
            }],
            cash_weight: "1".parse().unwrap(),
        }],
    };
    let summary = BTreeMap::from([
        ("venues.total".into(), "1".into()),
        ("orders.open".into(), "0".into()),
        ("orders.inflight".into(), "0".into()),
    ]);
    let result = NativeSimulationResultV1 {
        schema_version: SchemaV1,
        native_version: "0.63.0".into(),
        iterations: count(100),
        events: count(0),
        orders: count(0),
        positions: count(0),
        consumed_target_points: count(1),
        summary: summary.clone(),
        statistics: Vec::new(),
        returns_kind: NativeReturnsKind::PortfolioDaily,
        returns_status: MetricStatus::Ok,
        returns_reason: None,
        returns: vec![NativeReturnV1 {
            timestamp_ns: count(end),
            value: Some(0.0),
            reason_code: None,
        }],
        canonical_result: json!({
            "schema":"nautilus-backtest-result/v1",
            "run":{"outcome":"completed","iterations":"100","total_events":"0",
                "total_orders":"0","total_positions":"0",
                "backtest_start_ns":start.to_string(),"backtest_end_ns":end.to_string()},
            "summary":summary,
            "accounts":[{"Cash":{"base":{"id":"SIM-001","account_type":"CASH",
                "base_currency":"USD","balances_starting":{"USD":"1000.00 USD"}}}}],
            "portfolio_snapshots":[{"account_id":"SIM-001","account_type":"CASH",
                "base_currency":"USD","total_equity":["1000.00 USD"],"ts_event":end.to_string()}]
        }),
    };
    (
        NativeTaskParametersV1::SimulatePortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: Id::new(),
            request: Box::new(request),
        },
        result,
    )
}

#[test]
fn simulation_cannot_borrow_another_account_capital_currency_or_return_window() {
    let (parameters, result) = simulation();
    assert!(accepts(&parameters, "qz.native_simulation", &result));
    for dimension in 0..14 {
        let mut invalid = result.clone();
        match dimension {
            0 => {
                invalid.canonical_result["accounts"][0]["Cash"]["base"]["balances_starting"]
                    ["USD"] = json!("1001.00 USD")
            }
            1 => {
                invalid.canonical_result["accounts"][0]["Cash"]["base"]["base_currency"] =
                    json!("EUR")
            }
            2 => {
                invalid.canonical_result["accounts"][0]["Cash"]["base"]["account_type"] =
                    json!("MARGIN")
            }
            3 => {
                invalid.canonical_result["portfolio_snapshots"][0]["account_id"] =
                    json!("OTHER-001")
            }
            4 => {
                invalid.canonical_result["portfolio_snapshots"][0]["total_equity"] =
                    json!(["1000.00 EUR"])
            }
            5 => invalid.canonical_result["run"]["total_orders"] = json!("1"),
            6 => invalid.canonical_result["run"]["backtest_start_ns"] = json!("0"),
            7 => invalid.canonical_result["portfolio_snapshots"][0]["ts_event"] = json!("0"),
            8 => invalid.consumed_target_points = count(2),
            9 => invalid.returns[0].timestamp_ns = count(1),
            10 => invalid.returns_reason = Some("MISSING".into()),
            11 => invalid.returns[0].value = None,
            12 => invalid
                .summary
                .insert("orders.open".into(), "1".into())
                .map(|_| ())
                .unwrap(),
            _ => invalid.canonical_result["accounts"] = json!([]),
        }
        assert!(
            !accepts(&parameters, "qz.native_simulation", &invalid),
            "simulation dimension {dimension}"
        );
    }
    let mut changed = parameters;
    if let NativeTaskParametersV1::SimulatePortfolio { request, .. } = &mut changed {
        request.settings.starting_capital = "999".parse().unwrap();
    }
    assert!(!accepts(&changed, "qz.native_simulation", &result));
}
