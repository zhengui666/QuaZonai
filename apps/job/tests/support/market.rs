//! Native synthetic catalog fixture. Never REAL data or a production initialization path.
#![allow(dead_code)]
pub use portfolio_config::execution_models;
#[path = "../../../../tests/support/portfolio.rs"]
mod portfolio_config;
use contracts::{portfolio::AllocationTargetV1, science::*, DbCounter, SchemaV1};
use nautilus_model::{
    data::{Bar, BarType},
    identifiers::{InstrumentId, Symbol},
    instruments::{CurrencyPair, InstrumentAny},
    types::{Currency, Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use rust_decimal::Decimal;
use std::str::FromStr;

pub const INTERVAL_NS: u64 = 60_000_000_000;
pub fn count(n: u64) -> DbCounter {
    DbCounter::new(n).unwrap()
}
pub fn instant(n: u64) -> DbCounter {
    count(n * INTERVAL_NS + 1)
}

pub fn market(fee: &str, rows_per_asset: u32) -> (tempfile::TempDir, NativeSimulationRequestV1) {
    market_direction(fee, rows_per_asset, 1.0, "10000000")
}

fn market_direction(
    fee: &str,
    rows_per_asset: u32,
    direction: f64,
    volume: &str,
) -> (tempfile::TempDir, NativeSimulationRequestV1) {
    let directory = tempfile::tempdir().unwrap();
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    let usd = Currency::from_str("USD").unwrap();
    let mut types = Vec::new();
    let mut rates = Vec::new();
    let mut targets = Vec::new();
    for (name, base, multiplier) in [("EUR/USD.SIM", "EUR", 1.0), ("GBP/USD.SIM", "GBP", 2.0)] {
        let id = InstrumentId::from_str(name).unwrap();
        let instrument = CurrencyPair::builder()
            .instrument_id(id)
            .raw_symbol(Symbol::new_checked(name.split('.').next().unwrap()).unwrap())
            .base_currency(Currency::from_str(base).unwrap())
            .quote_currency(usd)
            .price_precision(5)
            .size_precision(0)
            .price_increment(Price::from("0.00001"))
            .size_increment(Quantity::from("1"))
            .maker_fee(Decimal::ZERO)
            .taker_fee(Decimal::from_str(fee).unwrap())
            .margin_init(Decimal::ONE)
            .margin_maint(Decimal::ONE)
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap();
        catalog
            .write_instruments(vec![InstrumentAny::CurrencyPair(instrument)])
            .unwrap();
        let kind = BarType::from_str(&format!("{name}-1-MINUTE-LAST-EXTERNAL")).unwrap();
        types.push(kind.to_string());
        let bars = (1..=rows_per_asset)
            .map(|i| {
                let price = multiplier + direction * f64::from(i) * 0.001;
                Bar::new_checked(
                    kind,
                    Price::from(format!("{price:.5}").as_str()),
                    Price::from(format!("{:.5}", price + 0.0002).as_str()),
                    Price::from(format!("{:.5}", price - 0.0002).as_str()),
                    Price::from(format!("{price:.5}").as_str()),
                    Quantity::from(volume),
                    (u64::from(i) * INTERVAL_NS).into(),
                    (u64::from(i) * INTERVAL_NS + 1).into(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        catalog.write_to_parquet(&bars, None, None, None).unwrap();
        rates.push(NativeFeeRateV1 {
            instrument_id: name.into(),
            maker: "0".parse().unwrap(),
            taker: fee.parse().unwrap(),
        });
        targets.push(AllocationTargetV1 {
            instrument_id: name.into(),
            currency: "USD".into(),
            weight: "0.4".parse().unwrap(),
        });
    }
    let end = count((u64::from(rows_per_asset) + 1) * INTERVAL_NS);
    let selection = NativeBarSelectionV1 {
        schema_version: SchemaV1,
        bar_types: types,
        event_start_ns: DbCounter::ZERO,
        event_end_ns: end,
        decision_cutoff_ns: end,
        maximum_rows: rows_per_asset * 2,
    };
    let mut later = targets.clone();
    later[0].weight = "0.2".parse().unwrap();
    later[1].weight = "0.6".parse().unwrap();
    let request = NativeSimulationRequestV1 {
        schema_version: SchemaV1,
        selection,
        settings: NativeSimulationSettingsV1 {
            schema_version: SchemaV1,
            base_currency: "USD".into(),
            starting_capital: "1000000".parse().unwrap(),
            account_kind: NativeAccountKind::Margin,
            leverage: "1".parse().unwrap(),
            fill_model: execution_models::fill(),
            fee_model: execution_models::fee(),
            latency_model: execution_models::latency(1_000_000),
            snapshot_interval_ms: 60_000,
            exposure_tolerance: "0.00001".parse().unwrap(),
            fee_rates: rates,
        },
        target_points: vec![
            NativeTargetPointV1 {
                schema_version: SchemaV1,
                asof_ns: instant(2),
                valid_until_ns: end,
                targets,
                cash_weight: "0.2".parse().unwrap(),
            },
            NativeTargetPointV1 {
                schema_version: SchemaV1,
                asof_ns: instant(7),
                valid_until_ns: end,
                targets: later,
                cash_weight: "0.2".parse().unwrap(),
            },
        ],
    };
    (directory, request)
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
    portfolio_from_market(market_direction("0", 20, -1.0, "10000000"))
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
        portfolio_from_market(market_direction("0", 2900, 1.0, volume));
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
            coverage_start_ns: count(request.evaluation_start_ns.get() + INTERVAL_NS),
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
