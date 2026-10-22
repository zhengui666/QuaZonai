//! Synthetic structural counterexamples, not native numerical or market evidence.
//! Genuine solver/account output is independently exercised by job/tests/managed.
use portfolio_config::execution_models;
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

fn cash_funding_contract_fixture() -> (AllocationInputV1, AllocationResultV1) {
    let mut input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    input.objective = AllocationObjective::MaxUtility;
    input.exposure_tolerance = "0.001".parse().unwrap();
    input.current_cash_weight = "1".parse().unwrap();
    input.constraints.min_net_exposure = "0".parse().unwrap();
    input.constraints.max_cash_weight = "1".parse().unwrap();
    input.constraints.cash_funding =
        Some(NativeCashFundingPolicyV1::PreserveMinCashAfterLinearTurnoverV1);
    for asset in &mut input.assets {
        asset.current_weight = "0".parse().unwrap();
        asset.transaction_cost_rate = "0.0005".parse().unwrap();
    }
    let targets = input
        .assets
        .iter()
        .zip(["1", "0"])
        .map(|(asset, weight)| AllocationTargetV1 {
            instrument_id: asset.instrument_id.clone(),
            currency: asset.currency.clone(),
            weight: weight.parse().unwrap(),
        })
        .collect();
    let result = AllocationResultV1 {
        schema_version: SchemaV1,
        solver_status: SolverStatus::Optimal,
        reason_code: None,
        targets: Some(targets),
        cash_weight: Some("0".parse().unwrap()),
        cvar_risk_budget_witness: None,
        iterations: 1,
        objective_value: Some(0.0),
        primal_residual: Some(0.0),
        dual_residual: Some(0.0),
    };
    (input, result)
}

#[test]
fn cash_funding_cannot_use_weight_tolerance_to_authorize_negative_post_fee_cash() {
    let (input, result) = cash_funding_contract_fixture();
    domain::portfolio::allocation_input(&input).unwrap();
    // A structural malformed-success counterexample, not a fabricated solver
    // run: cash=0, actual linear cost=.0005, min cash=0, tolerance=.001.
    // The original relaxed floor admits -.0005; CASH no-borrow must not.
    assert_eq!(
        domain::portfolio::allocation_result(&input, &result),
        Err(domain::DomainError::Invalid("cash_funding_borrowing"))
    );
}

#[test]
fn cash_funding_cannot_inflate_reported_cash_with_capital_tolerance() {
    for (weight, fee, reported_cash) in [
        ("1", "0.0005", "0.0005"),
        ("1", "0.000000000000000001", "0.000000000000000001"),
        ("1.000000000000000001", "0", "0"),
    ] {
        let (mut input, mut result) = cash_funding_contract_fixture();
        input.assets[0].transaction_cost_rate = fee.parse().unwrap();
        result.targets.as_mut().unwrap()[0].weight = weight.parse().unwrap();
        result.cash_weight = Some(reported_cash.parse().unwrap());
        domain::portfolio::allocation_input(&input).unwrap();
        // The reported cash pays all declared fees, and capital equality is
        // within the registered .001 tolerance. But original NAV minus actual
        // target weights and fees is strictly negative: no borrowed cash exists.
        assert_eq!(
            domain::portfolio::allocation_result(&input, &result),
            Err(domain::DomainError::Invalid("cash_funding_nav_shortfall")),
            "weight={weight}, fee={fee}, reported_cash={reported_cash}",
        );
    }
}

#[test]
fn cash_funding_cannot_spend_capital_tolerance_a_second_time_on_cash_floor() {
    let (mut input, mut result) = cash_funding_contract_fixture();
    input.constraints.min_cash_weight = "0.2".parse().unwrap();
    input.assets[0].transaction_cost_rate = "0.002".parse().unwrap();
    input.assets[1].transaction_cost_rate = "0".parse().unwrap();
    let targets = result.targets.as_mut().unwrap();
    targets[0].weight = "0.5".parse().unwrap();
    targets[1].weight = "0.301".parse().unwrap();
    result.cash_weight = Some("0.2".parse().unwrap());
    // net=.801, cash=.2 is within capital tolerance; cost=.001.
    // Reported post-fee cash .199 meets minimum(.2)-tolerance(.001),
    // but the original NAV funds only .198. The same allowance is not spent twice.
    assert_eq!(
        domain::portfolio::allocation_result(&input, &result),
        Err(domain::DomainError::Invalid("cash_funding_bound")),
    );
}

#[test]
fn cash_funding_cannot_use_long_only_tolerance_to_short_unowned_assets() {
    let (mut input, mut result) = cash_funding_contract_fixture();
    result.targets.as_mut().unwrap()[0].weight = "-0.0005".parse().unwrap();
    result.cash_weight = Some("1.0005".parse().unwrap());
    // The old general allocation protocol permits bounded numerical exposure
    // residuals. The opt-in CASH method cannot authorize borrowing the asset.
    input.constraints.cash_funding = None;
    domain::portfolio::allocation_result(&input, &result).unwrap();
    input.constraints.cash_funding =
        Some(NativeCashFundingPolicyV1::PreserveMinCashAfterLinearTurnoverV1);
    assert_eq!(
        domain::portfolio::allocation_result(&input, &result),
        Err(domain::DomainError::Invalid("cash_funding_shorting")),
    );
}

#[test]
fn cash_funding_zero_post_fee_cash_is_exact_and_positive_floor_retains_frozen_tolerance() {
    let (mut input, mut result) = cash_funding_contract_fixture();
    input.assets[0].transaction_cost_rate = "0.001".parse().unwrap();
    input.assets[1].transaction_cost_rate = "0".parse().unwrap();
    let targets = result.targets.as_mut().unwrap();
    targets[0].weight = "0.5".parse().unwrap();
    targets[1].weight = "0.4995".parse().unwrap();
    result.cash_weight = Some("0.0005".parse().unwrap());
    // Exact zero after fees is fundable. A positive minimum may use the
    // registered weight tolerance, but that allowance cannot create borrowing.
    for minimum in ["0", "0.001"] {
        input.constraints.min_cash_weight = minimum.parse().unwrap();
        domain::portfolio::allocation_result(&input, &result).unwrap();
    }
    input.constraints.min_cash_weight = "0".parse().unwrap();
    input.assets[0].transaction_cost_rate = "0.001000000000000001".parse().unwrap();
    assert_eq!(
        domain::portfolio::allocation_result(&input, &result),
        Err(domain::DomainError::Invalid("cash_funding_borrowing"))
    );
}

#[test]
fn native_portfolio_refuses_unbound_execution_costs() {
    let input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let original = portfolio_config::request(&input);
    domain::execution::portfolio_build_request(&original).unwrap();
    for case in 0..4 {
        let mut request = original.clone();
        match case {
            0 => request.execution_settings.base_currency = "EUR".into(),
            1 => request.execution_settings.starting_capital = "1".parse().unwrap(),
            2 => request.execution_settings.fee_rates[0].taker = "0.25".parse().unwrap(),
            _ => request.execution_settings.fee_rates.clear(),
        }
        assert!(
            domain::execution::portfolio_build_request(&request).is_err(),
            "cost case {case}"
        );
    }
}

#[test]
fn native_one_tick_cost_planning_is_explicit_and_bound() {
    let input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let mut request = portfolio_config::request(&input);
    for asset in &mut request.assets {
        asset.transaction_cost_rate = "0.01".parse().unwrap();
    }
    for fee in &mut request.execution_settings.fee_rates {
        fee.taker = "0.01".parse().unwrap();
    }
    let NativeModelRefV1::NautilusDefaultFill { parameters, .. } =
        &mut request.execution_settings.fill_model
    else {
        unreachable!()
    };
    parameters.prob_slippage = "0.5".parse().unwrap();
    let references = request
        .assets
        .iter()
        .map(|asset| NativePortfolioSlippageReferenceV1 {
            instrument_id: asset.instrument_id.clone(),
            currency: asset.currency.clone(),
            event_ns: input.forecasts.forecast_asof_ns,
            available_ns: input.forecasts.forecast_asof_ns,
            close_price: "10".parse().unwrap(),
            price_increment: "0.01".parse().unwrap(),
        })
        .collect::<Vec<_>>();
    let costs = domain::execution::portfolio_execution_costs(&request, &references).unwrap();
    assert_eq!(costs[0].transaction_cost_rate, "0.010505".parse().unwrap());
    let mut thirds = references.clone();
    thirds[0].close_price = "3".parse().unwrap();
    assert_eq!(
        domain::execution::portfolio_execution_costs(&request, &thirds).unwrap()[0]
            .transaction_cost_rate,
        "0.011683333333333334".parse().unwrap()
    );
    for case in 0..6 {
        let mut invalid = references.clone();
        match case {
            0 => invalid.clear(),
            1 => invalid[0].price_increment = "0".parse().unwrap(),
            2 => invalid[0].price_increment = "10".parse().unwrap(),
            3 => invalid[0].currency = "EUR".into(),
            4 => invalid[0].instrument_id = "foreign".into(),
            _ => {
                invalid[0].available_ns =
                    DbCounter::new(request.selection.decision_cutoff_ns.get() + 1).unwrap()
            }
        }
        assert!(
            domain::execution::portfolio_execution_costs(&request, &invalid).is_err(),
            "reference case {case}"
        );
    }
}

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
                bar_notionals: Vec::new(),
                slippage_references: Vec::new(),
                input: request.clone(),
                allocation: allocation.clone(),
                consumed_fuel: Some(DbCounter::new(1).unwrap()),
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
        settlements: Vec::new(),
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
            multi_currency_spot_cash: None,
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
            rebalance_instrument_ids: None,
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
        settled_replay_ns: None,
        spot_cash_report: None,
        recorded_fee_replay: None,
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

fn recorded_liquidity_settings() -> NativeSimulationSettingsV1 {
    let (NativeTaskParametersV1::SimulatePortfolio { request, .. }, _) = simulation() else {
        unreachable!()
    };
    let mut settings = request.settings;
    let parameters = domain::portfolio::simulation_models(&settings)
        .unwrap()
        .0
        .clone();
    settings.fill_model = NativeModelRefV1::PolymarketV2RecordedLiquidity {
        schema_version: SchemaV1,
        native_version: NAUTILUS_EXECUTION_VERSION.into(),
        parameters,
    };
    settings.fee_model = NativeModelRefV1::PolymarketV2RecordedFees {
        schema_version: SchemaV1,
        native_version: NAUTILUS_EXECUTION_VERSION.into(),
        parameters: contracts::recorded_fees::RecordedTradeFeeParametersV1 {},
    };
    settings.base_currency = "pUSD".into();
    settings.account_kind = NativeAccountKind::Margin;
    settings
}

fn fill_parameters_mut(model: &mut NativeModelRefV1) -> &mut NautilusFillParametersV1 {
    match model {
        NativeModelRefV1::NautilusDefaultFill { parameters, .. }
        | NativeModelRefV1::PolymarketV2RecordedLiquidity { parameters, .. } => parameters,
        _ => unreachable!(),
    }
}

#[test]
fn recorded_liquidity_requires_recorded_fees_and_exact_native_versions() {
    use domain::portfolio::{simulation_models, simulation_settings};
    let original = recorded_liquidity_settings();
    simulation_settings(&original).unwrap();
    let unavailable = domain::DomainError::CapabilityUnavailable("native_simulation_models");
    for version in ["", "0.62.0", "0.63.1"] {
        for fill_version in [true, false] {
            let mut settings = original.clone();
            let model = if fill_version {
                &mut settings.fill_model
            } else {
                &mut settings.fee_model
            };
            match model {
                NativeModelRefV1::PolymarketV2RecordedLiquidity { native_version, .. }
                | NativeModelRefV1::PolymarketV2RecordedFees { native_version, .. } => {
                    *native_version = version.into();
                }
                _ => unreachable!(),
            }
            assert_eq!(simulation_models(&settings), Err(unavailable.clone()));
        }
    }
    for fee_model in [
        execution_models::fee(),
        NativeModelRefV1::NautilusPolymarket {
            schema_version: SchemaV1,
            upstream_class: NAUTILUS_POLYMARKET_FEE_CLASS.into(),
            upstream_version: NAUTILUS_EXECUTION_VERSION.into(),
            parameters: NautilusFeeParametersV1 {},
        },
        original.fill_model.clone(),
    ] {
        let mut settings = original.clone();
        settings.fee_model = fee_model;
        assert_eq!(simulation_models(&settings), Err(unavailable.clone()));
    }
}

#[test]
fn recorded_liquidity_retains_recorded_fee_account_and_authentic_binary_option_rules() {
    use domain::portfolio::simulation_models;
    let original = recorded_liquidity_settings();
    let mut legacy = original.clone();
    legacy.fill_model = execution_models::fill();
    for original in [original, legacy] {
        simulation_models(&original).unwrap();
        assert!(!domain::prediction::uses_native_fee(&original.fee_model));
        for case in 0..4 {
            let mut settings = original.clone();
            match case {
                0 => settings.base_currency = "USDC".into(),
                1 => settings.account_kind = NativeAccountKind::Cash,
                2 => settings.leverage = "2".parse().unwrap(),
                _ => settings.multi_currency_spot_cash = Some(serde_json::from_value(json!({
                    "schema_version": 1, "mode": "MULTI_CURRENCY_CASH", "report_currency": "pUSD",
                    "price_method": "CLOSED_BAR_CLOSE", "allowed_instrument_ids": ["A.SIM"],
                    "returns_policy": "FRESH_SIMULATION_NO_EXTERNAL_FLOWS",
                    "daily_sampling": "NATIVE_UTC_MIDNIGHT_BOUNDARIES", "maximum_price_age_ns": "1"
                })).unwrap()),
            }
            assert_eq!(
                simulation_models(&settings),
                Err(domain::DomainError::CapabilityUnavailable(
                    "native_simulation_models"
                )),
                "account case {case}"
            );
        }
    }
    let mut authentic = recorded_liquidity_settings();
    authentic.fill_model = execution_models::fill();
    authentic.fee_model = NativeModelRefV1::NautilusPolymarket {
        schema_version: SchemaV1,
        upstream_class: NAUTILUS_POLYMARKET_FEE_CLASS.into(),
        upstream_version: NAUTILUS_EXECUTION_VERSION.into(),
        parameters: NautilusFeeParametersV1 {},
    };
    authentic.account_kind = NativeAccountKind::Cash;
    domain::portfolio::simulation_settings(&authentic).unwrap();
    assert!(domain::prediction::uses_native_fee(&authentic.fee_model));
    domain::catalogs::execution_account("BinaryOption", NativeAccountKind::Cash).unwrap();
    assert!(
        domain::catalogs::execution_account("BinaryOption", NativeAccountKind::Margin).is_err()
    );
}

#[test]
fn recorded_liquidity_validates_probabilities_seed_and_latency_exactly_as_default_fill() {
    use domain::portfolio::simulation_models;
    let recorded = recorded_liquidity_settings();
    let mut legacy = recorded.clone();
    legacy.fill_model = execution_models::fill();
    for mut settings in [recorded, legacy] {
        for seed in [0, 7, i64::MAX as u64] {
            for (limit, slippage) in [("0", "1"), ("1", "0"), ("0.75", "0.25")] {
                let parameters = NautilusFillParametersV1 {
                    prob_fill_on_limit: limit.parse().unwrap(),
                    prob_slippage: slippage.parse().unwrap(),
                    random_seed: count(seed),
                };
                *fill_parameters_mut(&mut settings.fill_model) = parameters.clone();
                assert_eq!(simulation_models(&settings).unwrap().0, &parameters);
            }
        }
        for (limit, slippage) in [
            ("-0.000000000000000001", "0"),
            ("1.000000000000000001", "0"),
            ("1", "-0.000000000000000001"),
            ("1", "1.000000000000000001"),
        ] {
            let parameters = fill_parameters_mut(&mut settings.fill_model);
            parameters.prob_fill_on_limit = limit.parse().unwrap();
            parameters.prob_slippage = slippage.parse().unwrap();
            assert_eq!(
                simulation_models(&settings),
                Err(domain::DomainError::Invalid("native_simulation_parameters"))
            );
        }
        let parameters = fill_parameters_mut(&mut settings.fill_model);
        parameters.prob_fill_on_limit = "1".parse().unwrap();
        parameters.prob_slippage = "1".parse().unwrap();
        settings.latency_model = execution_models::latency(0);
        assert_eq!(
            simulation_models(&settings),
            Err(domain::DomainError::Invalid("native_simulation_parameters"))
        );
    }
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

#[test]
fn native_simulation_binding_validates_scoped_full_account_targets() {
    let (mut parameters, result) = simulation();
    let NativeTaskParametersV1::SimulatePortfolio { request, .. } = &mut parameters else {
        unreachable!()
    };
    request.target_points[0].rebalance_instrument_ids = Some(vec!["A.SIM".into()]);
    assert!(accepts(&parameters, "qz.native_simulation", &result));
    for dimension in 0..6 {
        let mut invalid = parameters.clone();
        let NativeTaskParametersV1::SimulatePortfolio { request, .. } = &mut invalid else {
            unreachable!()
        };
        let point = &mut request.target_points[0];
        match dimension {
            0 => point.rebalance_instrument_ids = Some(vec![]),
            1 => point.rebalance_instrument_ids = Some(vec!["A.SIM".into(), "A.SIM".into()]),
            2 => point.rebalance_instrument_ids = Some(vec!["OTHER.SIM".into()]),
            3 => point.cash_weight = "0.9".parse().unwrap(),
            4 => point.targets[0].weight = "-0.1".parse().unwrap(),
            _ => point.cash_weight = "1.1".parse().unwrap(),
        }
        assert!(
            !accepts(&invalid, "qz.native_simulation", &result),
            "scope binding dimension {dimension}"
        );
    }
}

#[test]
fn independent_target_policy_report_binds_refs_folds_clocks_weights_and_native_accounts() {
    use contracts::research::{SplitKind, SplitPolicyV1};
    let (template_task, template_result) = simulation();
    let NativeTaskParametersV1::SimulatePortfolio {
        request: template, ..
    } = template_task
    else {
        unreachable!()
    };
    let request = NativeExperimentEvaluationRequestV1 {
        binary_option: None,
        schema_version: SchemaV1,
        selection: NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: template.selection.bar_types.clone(),
            event_start_ns: count(1),
            event_end_ns: count(101),
            decision_cutoff_ns: count(102),
            maximum_rows: 10,
        },
        instrument_id: "A.SIM".into(),
        feature_schema: vec![FeatureDefinitionV1 {
            feature_key: "external-signal".into(),
            source_ref: "snapshot:warehouse".into(),
            source_key: "event-3".into(),
            availability: FeatureAvailabilityV1::Observed,
            max_age_ns: None,
        }],
        split_policy: SplitPolicyV1 {
            schema_version: SchemaV1,
            kind: SplitKind::WalkForward,
            train_size: count(4),
            test_size: count(2),
            step_size: Some(count(2)),
            group_count: None,
            test_group_count: None,
            purge_observations: count(1),
            embargo_observations: count(0),
            label_horizon_observations: Some(count(1)),
            interval_validation_required: true,
            sealed_revision_id: Id::new(),
        },
        label_horizon_observations: 1,
        total_fuel: Some(count(100_000)),
        target_ttl_ns: count(200),
        decision_output: ExperimentDecisionOutputV1::TargetWeight,
        settings: template.settings.clone(),
    };
    domain::execution::experiment_request(&request).unwrap();
    // Irregular event spacing proves label end is an observed-row clock, not a duration.
    let event = |index: usize| ((index + 1) * (index + 1)) as u64;
    let folds = domain::execution::validation::validation_folds(&request.split_policy, 9)
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(index, indices)| {
            let decisions: Vec<_> = indices
                .test
                .iter()
                .map(|&ordinal| NativeExperimentDecisionV1 {
                    ordinal: ordinal as u32,
                    event_ns: count(event(ordinal)),
                    decision_ns: count(event(ordinal) + 1),
                    label_end_ns: Some(count(event(ordinal + 1))),
                    label_available_ns: Some(count(event(ordinal + 1) + 1)),
                    label_return: Some(0.0),
                    features: vec![FeatureValueV1 {
                        value: None,
                        missing_reason: Some(FeatureMissingReasonV1::NotYetAvailable),
                        event_ns: None,
                        observed_available_ns: None,
                        effective_available_ns: None,
                        sequence: None,
                    }],
                    target_weight: "0".parse().unwrap(),
                })
                .collect();
            let mut replay = (*template).clone();
            replay.selection.event_start_ns = decisions[0].event_ns;
            replay.selection.event_end_ns =
                count(decisions.last().unwrap().label_end_ns.unwrap().get() + 1);
            replay.selection.decision_cutoff_ns =
                decisions.last().unwrap().label_available_ns.unwrap();
            replay.selection.maximum_rows = 10;
            replay.target_points = decisions
                .iter()
                .map(|point| NativeTargetPointV1 {
                    schema_version: SchemaV1,
                    rebalance_instrument_ids: None,
                    asof_ns: point.decision_ns,
                    valid_until_ns: count(point.decision_ns.get() + request.target_ttl_ns.get()),
                    targets: vec![AllocationTargetV1 {
                        instrument_id: request.instrument_id.clone(),
                        currency: "USD".into(),
                        weight: point.target_weight.clone(),
                    }],
                    cash_weight: "1".parse().unwrap(),
                })
                .collect();
            let mut native = template_result.clone();
            native.consumed_target_points = count(decisions.len() as u64);
            native.returns.clear();
            native.returns_status = MetricStatus::InsufficientData;
            native.returns_reason = Some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE".into());
            native.canonical_result["run"]["backtest_start_ns"] =
                json!(replay.selection.event_start_ns.get().to_string());
            native.canonical_result["run"]["backtest_end_ns"] =
                json!(replay.selection.decision_cutoff_ns.get().to_string());
            native.canonical_result["portfolio_snapshots"][0]["ts_event"] =
                json!(replay.selection.decision_cutoff_ns.get().to_string());
            NativeExperimentFoldV1 {
                fold_index: index as u16,
                training_end_available_ns: count(event(*indices.train.last().unwrap() + 1) + 1),
                training_ordinals: indices.train.into_iter().map(|n| n as u32).collect(),
                decisions,
                simulation_request: replay,
                simulation: native,
            }
        })
        .collect();
    let feature_artifact_ids = vec![Id::new(), Id::new()];
    let dataset_revision_id = Id::new();
    let model_artifact_id = Id::new();
    let result = NativeExperimentEvaluationResultV1 {
        schema_version: SchemaV1,
        native_versions: BTreeMap::from([
            ("nautilus-backtest".into(), "0.63.0".into()),
            ("solow-cv".into(), "0.7.3".into()),
            ("wasmi".into(), "2.0.0".into()),
        ]),
        dataset_revision_id,
        model_artifact_id,
        request: request.clone(),
        feature_artifact_ids: feature_artifact_ids.clone(),
        instrument_id: request.instrument_id.clone(),
        consumed_fuel: Some(count(1)),
        source_row_count: count(10),
        feature_count: 1,
        folds,
    };
    let task = NativeTaskParametersV1::EvaluateExperiment {
        schema_version: SchemaV1,
        dataset_revision_id,
        model_artifact_id,
        feature_artifact_ids,
        request: Box::new(request),
    };
    let downloaded: contracts::execution::NativeJsonOutputV1 =
        serde_json::from_slice(&serde_json::to_vec(&result).unwrap()).unwrap();
    assert!(matches!(
        downloaded,
        contracts::execution::NativeJsonOutputV1::ExperimentEvaluation(_)
    ));
    assert!(accepts(&task, "qz.experiment_evaluation", &result));
    for dimension in 0..18 {
        let mut invalid = result.clone();
        match dimension {
            0 => invalid.feature_artifact_ids.reverse(),
            1 => invalid.folds[0].training_ordinals[0] = 1,
            2 => invalid.folds[0].decisions[0].ordinal += 1,
            3 => {
                invalid.folds[0].training_end_available_ns =
                    invalid.folds[0].decisions[0].decision_ns
            }
            4 => invalid.folds[0].simulation_request.target_points[0].valid_until_ns = count(50),
            5 => invalid.folds[0].decisions[0].target_weight = "0.5".parse().unwrap(),
            6 => {
                invalid.folds[0]
                    .simulation_request
                    .settings
                    .starting_capital = "999".parse().unwrap()
            }
            7 => invalid.folds[0].decisions[0].features[0].event_ns = Some(count(1)),
            8 => invalid.folds[0].simulation_request.selection.event_start_ns = count(1),
            9 => invalid.folds[0].decisions[0].label_available_ns = None,
            10 => {
                invalid.folds[0].decisions[0].label_end_ns = Some(count(10_000));
                invalid.folds[0].decisions[0].label_available_ns = Some(count(10_001));
            }
            11 => {
                invalid.folds[0].decisions[0].label_end_ns = Some(count(37));
                invalid.folds[0].decisions[0].label_available_ns = Some(count(38));
            }
            12 => invalid.folds[0].decisions[0].label_available_ns = Some(count(51)),
            13 => invalid.folds[0].training_end_available_ns = count(0),
            14 => invalid.folds[1].training_end_available_ns = count(49),
            15 => invalid.dataset_revision_id = Id::new(),
            16 => invalid.model_artifact_id = Id::new(),
            _ => {
                invalid.folds[0].simulation_request.target_points[0].rebalance_instrument_ids =
                    Some(vec!["A.SIM".into()]);
            }
        }
        assert!(
            !accepts(&task, "qz.experiment_evaluation", &invalid),
            "experiment dimension {dimension}"
        );
    }
    for dimension in 0..5 {
        let mut other = task.clone();
        let NativeTaskParametersV1::EvaluateExperiment {
            dataset_revision_id,
            model_artifact_id,
            request,
            ..
        } = &mut other
        else {
            unreachable!()
        };
        match dimension {
            0 => *dataset_revision_id = Id::new(),
            1 => *model_artifact_id = Id::new(),
            2 => request.feature_schema[0].feature_key = "different-meaning".into(),
            3 => request.feature_schema[0].source_ref = "different-source".into(),
            _ => request.feature_schema[0].source_key = "different-event".into(),
        }
        assert!(
            !accepts(&other, "qz.experiment_evaluation", &result),
            "reassociated request dimension {dimension}"
        );
    }
}

#[test]
fn complete_native_snapshot_collection_crosses_the_old_million_point_gate() {
    let (parameters, mut result) = simulation();
    let first = result.canonical_result["portfolio_snapshots"][0].clone();
    let start = result.canonical_result["run"]["backtest_start_ns"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let snapshots: Vec<_> = (0..1_000_003)
        .map(|index| {
            let mut snapshot = first.clone();
            snapshot["ts_event"] = json!((start + index).to_string());
            snapshot
        })
        .collect();
    result.canonical_result["portfolio_snapshots"] = snapshots.into();
    assert!(accepts(&parameters, "qz.native_simulation", &result));
    result.canonical_result["portfolio_snapshots"][1_000_002]["account_id"] = json!("FOREIGN");
    assert!(!accepts(&parameters, "qz.native_simulation", &result));
}

#[test]
fn full_native_statistic_and_summary_collections_keep_binding_and_duplicate_checks() {
    let (parameters, mut result) = simulation();
    result.statistics = (0..4097)
        .map(|index| NativeStatisticV1 {
            group: NativeStatisticGroup::General,
            native_key: format!("native-statistic-{index}"),
            currency: None,
            value: Some(1.0),
            reason_code: None,
        })
        .collect();
    for index in 0..8190 {
        result
            .summary
            .insert(format!("native-summary-{index}"), "recorded".into());
    }
    result.canonical_result["summary"] = serde_json::to_value(&result.summary).unwrap();
    assert_eq!(result.summary.len(), 8193);
    assert!(accepts(&parameters, "qz.native_simulation", &result));
    result.statistics.push(result.statistics[0].clone());
    assert!(!accepts(&parameters, "qz.native_simulation", &result));
    result.statistics.pop();
    result.canonical_result["summary"]["native-summary-0"] = json!("foreign");
    assert!(!accepts(&parameters, "qz.native_simulation", &result));
}
