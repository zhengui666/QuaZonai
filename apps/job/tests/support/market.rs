//! Native synthetic catalog fixture. Never REAL data or a production initialization path.
#![allow(dead_code)]
pub use portfolio_config::execution_models;
#[path = "market_catalog.rs"]
mod market_catalog;
#[path = "../../../../tests/support/portfolio.rs"]
mod portfolio_config;
use contracts::{science::*, SchemaV1};
pub use market_catalog::{count, instant, INTERVAL_NS};

pub fn market(fee: &str, rows_per_asset: u32) -> (tempfile::TempDir, NativeSimulationRequestV1) {
    market_direction(fee, rows_per_asset, 1.0, "10000000", false)
}

pub fn equity_market(
    fee: &str,
    rows_per_asset: u32,
) -> (tempfile::TempDir, NativeSimulationRequestV1) {
    market_direction(fee, rows_per_asset, 1.0, "10000000", true)
}

fn market_direction(
    fee: &str,
    rows_per_asset: u32,
    direction: f64,
    volume: &str,
    equities: bool,
) -> (tempfile::TempDir, NativeSimulationRequestV1) {
    market_catalog::market_direction(
        fee,
        rows_per_asset,
        direction,
        volume,
        equities,
        [
            execution_models::fill(),
            execution_models::fee(),
            execution_models::latency(1_000_000),
        ],
    )
}

pub fn forecast_request(simulation: &NativeSimulationRequestV1) -> NativeForecastRequestV1 {
    NativeForecastRequestV1 {
        schema_version: SchemaV1,
        selection: simulation.selection.clone(),
        parameters: NativeForecastParametersV1 {
            schema_version: SchemaV1,
            fast_period: 2,
            slow_period: 3,
            label_horizon_observations: 2,
            total_fuel: count(100_000_000),
        },
    }
}
pub fn alpha_validation_request(
    source: &NativeSimulationRequestV1,
) -> NativeAlphaValidationRequestV1 {
    NativeAlphaValidationRequestV1 {
        schema_version: SchemaV1,
        forecast: forecast_request(source),
        split_policy: contracts::research::SplitPolicyV1 {
            schema_version: SchemaV1,
            kind: contracts::research::SplitKind::WalkForward,
            train_size: count(8),
            test_size: count(3),
            step_size: Some(count(3)),
            group_count: None,
            test_group_count: None,
            purge_observations: count(2),
            embargo_observations: count(1),
            label_horizon_observations: Some(count(2)),
            interval_validation_required: true,
            sealed_revision_id: contracts::Id::new(),
        },
        target_kind: contracts::brief::TargetKind::Score,
    }
}

pub fn module(body: &str) -> Vec<u8> {
    wat::parse_str(format!("(module (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) {body}))")).unwrap()
}

pub fn portfolio() -> (tempfile::TempDir, NativePortfolioBuildRequestV1, Vec<u8>) {
    portfolio_from_market(market("0", 20))
}

pub fn portfolio_with_losses() -> (tempfile::TempDir, NativePortfolioBuildRequestV1, Vec<u8>) {
    portfolio_from_market(market_direction("0", 20, -1.0, "10000000", false))
}

pub fn study() -> (tempfile::TempDir, NativePortfolioStudyRequestV1, Vec<u8>) {
    study_with_volume("10000000")
}

pub fn study_liquidity(
    volume: &str,
    participation: &str,
) -> (tempfile::TempDir, NativePortfolioStudyRequestV1, Vec<u8>) {
    let (catalog, mut request, model) = study_with_volume(volume);
    let policy = NativeRollingBarLiquidityPolicyV1 {
        schema_version: SchemaV1,
        maximum_age_seconds: 120,
        participation_limit: participation.parse().unwrap(),
    };
    request.mandate.constraints.liquidity_ref = Some(contracts::Id::new());
    request.mandate.constraints.max_participation = Some(policy.participation_limit.clone());
    request.rolling_liquidity = Some(policy);
    (catalog, request, model)
}

fn study_with_volume(volume: &str) -> (tempfile::TempDir, NativePortfolioStudyRequestV1, Vec<u8>) {
    let (catalog, original, model) =
        portfolio_from_market(market_direction("0", 2900, 1.0, volume, false));
    let mut request = NativePortfolioStudyRequestV1 {
        schema_version: SchemaV1,
        source_selection: original.selection,
        evaluation_start_ns: instant(10),
        manual_cutoffs_ns: None,
        calendar: None,
        rolling_liquidity: None,
        research_available_through_ns: instant(9),
        mandate: original.mandate,
        execution_settings: original.execution_settings,
        assets: original.assets,
        members: original.members,
    };
    request.mandate.capital_assumption = "10000000".parse().unwrap();
    request.execution_settings.starting_capital = request.mandate.capital_assumption.clone();
    request.execution_settings.snapshot_interval_ms = 60_000;
    request.mandate.rebalance_schedule.kind = contracts::portfolio::RebalanceKind::FixedInterval;
    request.mandate.rebalance_schedule.interval_seconds = Some(86400);
    request.mandate.rebalance_schedule.target_ttl_seconds = 86400;
    request.mandate.rebalance_schedule.max_input_age_seconds = 120;
    request.mandate.constraints.min_cash_weight = "0.01".parse().unwrap();
    request.mandate.constraints.max_cash_weight = "0.01".parse().unwrap();
    request.mandate.constraints.min_net_exposure = "0.99".parse().unwrap();
    request.mandate.constraints.max_net_exposure = "0.99".parse().unwrap();
    for asset in &mut request.assets {
        asset.current_weight = "0".parse().unwrap();
    }
    for member in &mut request.members {
        member.parameters.total_fuel = count(100_000_000);
    }
    (catalog, request, model)
}

pub fn calendar_schedule(request: &mut NativePortfolioStudyRequestV1) {
    let mut cutoffs = domain::execution::portfolio_study_cutoffs(request).unwrap();
    cutoffs[1] = count(cutoffs[1].get() - INTERVAL_NS);
    let schedule = &mut request.mandate.rebalance_schedule;
    schedule.kind = contracts::portfolio::RebalanceKind::CalendarSession;
    schedule.interval_seconds = None;
    schedule.calendar_ref = Some("SIM-SESSIONS".into());
    schedule.session_offset_seconds = Some(-60);
    schedule.target_ttl_seconds = 86_460;
    request.calendar = Some(NativePortfolioCalendarV1 {
        artifact_id: contracts::Id::new(),
        calendar: NativeCalendarSessionsV1 {
            schema_version: SchemaV1,
            calendar_ref: "SIM-SESSIONS".into(),
            calendar_version: "synthetic-sessions-1".into(),
            timezone: schedule.timezone.clone(),
            source_reference: "explicit synthetic calendar fixture; not an exchange calendar"
                .into(),
            available_at_ns: request.research_available_through_ns,
            coverage_start_ns: request.source_selection.event_start_ns,
            coverage_end_ns: count(request.source_selection.event_end_ns.get() + INTERVAL_NS),
            sessions: cutoffs
                .into_iter()
                .map(|cutoff| NativeCalendarSessionV1 {
                    open_ns: count(cutoff.get() - 4 * INTERVAL_NS),
                    close_ns: count(cutoff.get() + INTERVAL_NS),
                })
                .collect(),
        },
    });
}

pub fn portfolio_from_market(
    (directory, simulation): (tempfile::TempDir, NativeSimulationRequestV1),
) -> (tempfile::TempDir, NativePortfolioBuildRequestV1, Vec<u8>) {
    let input = serde_json::from_str(include_str!(
        "../../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let mut request = portfolio_config::request(&input);
    request.execution_settings.account_kind = simulation.settings.account_kind;
    request.selection = simulation.selection;
    request.current_weights.asof_ns = request.selection.decision_cutoff_ns;
    request.current_weights.available_ns = request.selection.decision_cutoff_ns;
    request.current_weights.valid_until_ns = count(request.selection.decision_cutoff_ns.get() + 1);
    request.mandate.rebalance_schedule.max_input_age_seconds = 60;
    for (asset, fee) in request
        .assets
        .iter_mut()
        .zip(&simulation.settings.fee_rates)
    {
        asset.instrument_id = fee.instrument_id.clone();
        asset.transaction_cost_rate = fee.taker.clone();
    }
    for (weight, asset) in request
        .current_weights
        .weights
        .iter_mut()
        .zip(&request.assets)
    {
        weight.instrument_id = asset.instrument_id.clone();
    }
    request.execution_settings.fee_rates = simulation.settings.fee_rates;
    for member in &mut request.members {
        member.parameters.label_horizon_observations = 2;
    }
    (directory, request, module("f64.const 0.01"))
}

pub fn sealed() -> (
    tempfile::TempDir,
    NativeAlphaSealedRequestV1,
    NativeFrozenCalibrationV1,
    Vec<u8>,
) {
    let (directory, source) = market("0", 50);
    let wasm = module("local.get 0");
    let mut train = alpha_validation_request(&source);
    train.forecast.selection.event_end_ns = count(26 * INTERVAL_NS);
    train.forecast.selection.decision_cutoff_ns = train.forecast.selection.event_end_ns;
    let report = job::validation::validate_alpha(directory.path(), &train, &wasm).unwrap();
    let frozen = domain::execution::freeze_alpha_calibration(&train, &report, contracts::Id::new())
        .unwrap()
        .unwrap();
    let frozen = serde_json::from_slice(&serde_json::to_vec(&frozen).unwrap()).unwrap();
    let mut forecast = forecast_request(&source);
    forecast.selection.event_start_ns = count(26 * INTERVAL_NS);
    (
        directory,
        NativeAlphaSealedRequestV1 {
            schema_version: SchemaV1,
            forecast,
            target_kind: contracts::brief::TargetKind::Score,
            research_available_through_ns: instant(25),
        },
        frozen,
        wasm,
    )
}
