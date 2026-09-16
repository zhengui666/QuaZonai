//! Shared native account/positions/fees, not an average of independent strategy NAVs.
#[path = "support/market.rs"]
mod market;
use contracts::{
    evidence::MetricStatus,
    science::{NativeReturnsKind, NativeSimulationResultV1, NativeStatisticGroup},
};
#[path = "support/command.rs"]
mod native;
use market::{count, instant, market};
use nautilus_backtest::result::CanonicalBacktestResult;
use nautilus_model::types::Money;
use std::{path::Path, str::FromStr};

fn simulate(
    root: &Path,
    request: &contracts::science::NativeSimulationRequestV1,
) -> Result<NativeSimulationResultV1, String> {
    let output = native::command(
        &["simulate".as_ref(), "--catalog".as_ref(), root.as_os_str()],
        request,
    );
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

#[test]
fn portfolio_metrics_preserve_native_values_missingness_and_frozen_gate() {
    use contracts::evidence::{Comparator, Decision, MetricRequirementV1};
    use domain::execution::portfolio_simulation_metrics;
    let evaluation = contracts::Id::new();
    let artifact = contracts::Id::new();
    for (minutes, cash) in [(20, false), (2 * 1440 + 20, false), (2 * 1440 + 20, true)] {
        let (directory, mut request) = market("0", minutes);
        if cash {
            request.target_points.truncate(1);
            request.target_points[0].cash_weight = "1".parse().unwrap();
            for target in &mut request.target_points[0].targets {
                target.weight = "0".parse().unwrap();
            }
        }
        let result = simulate(directory.path(), &request).unwrap();
        let (metrics, capabilities) =
            portfolio_simulation_metrics(evaluation, artifact, &request, &result).unwrap();
        assert_eq!(metrics.len(), 3);
        for (metric, key) in metrics.iter().zip([
            "Average (Return)",
            "Returns Volatility (252 days)",
            "Sharpe Ratio (252 days)",
        ]) {
            let original = result
                .statistics
                .iter()
                .find(|stat| stat.group == NativeStatisticGroup::Returns && stat.native_key == key)
                .unwrap();
            assert_eq!(metric.value, original.value);
            assert_eq!(metric.evaluation_id, evaluation);
            assert_eq!(metric.source_artifact_id, artifact);
            assert_eq!(metric.observation_count.get(), result.returns.len() as u64);
            assert_eq!(metric.frequency, "UTC_DAY");
            assert_eq!(metric.method_version, "0.63.0");
            assert_eq!(metric.scope, "portfolio");
            assert!(metric.period_start < metric.period_end);
        }
        assert_eq!(metrics[0].annualization_factor, None);
        assert_eq!(metrics[1].annualization_factor, Some(252.0));
        assert_eq!(metrics[2].annualization_factor, Some(252.0));
        if minutes == 20 {
            assert!(metrics
                .iter()
                .all(|metric| metric.status == MetricStatus::InsufficientData
                    && metric.reason_code.as_deref()
                        == Some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE")));
        } else if cash {
            assert_eq!(metrics[0].value, Some(0.0));
            assert_eq!(metrics[0].status, MetricStatus::Ok);
            assert_eq!(metrics[1].value, Some(0.0));
            assert_eq!(metrics[2].value, None);
            assert_eq!(metrics[2].status, MetricStatus::Failed);
            assert_eq!(
                metrics[2].reason_code.as_deref(),
                Some("NATIVE_STATISTIC_UNAVAILABLE")
            );
        }
        let mut requirements = vec![MetricRequirementV1 {
            schema_version: contracts::SchemaV1,
            metric_code: metrics[0].metric_code.clone(),
            scope: "portfolio".into(),
            comparator: Comparator::Ge,
            threshold_low: Some("-1000".parse().unwrap()),
            threshold_high: None,
            required: true,
            minimum_observations: count(2),
            method_allowlist: vec![metrics[0].method_id.clone()],
        }];
        let gate =
            domain::evidence::evaluate_metrics(evaluation, &requirements, &metrics, &capabilities)
                .unwrap();
        assert_eq!(gate.decision == Decision::Pass, minutes > 20);
        requirements[0].minimum_observations = count(1000);
        assert_ne!(
            domain::evidence::evaluate_metrics(evaluation, &requirements, &metrics, &capabilities)
                .unwrap()
                .decision,
            Decision::Pass
        );
        let mut missing = result.clone();
        missing.statistics.retain(|stat| {
            stat.group != NativeStatisticGroup::Returns || stat.native_key != "Average (Return)"
        });
        assert!(portfolio_simulation_metrics(evaluation, artifact, &request, &missing).is_err());
        let mut wrong = request.clone();
        wrong.settings.starting_capital = "1".parse().unwrap();
        assert!(portfolio_simulation_metrics(evaluation, artifact, &wrong, &result).is_err());
    }
}

#[test]
fn assumptions_read_original_native_instrument_fees_and_shared_settings_bounds() {
    let (directory, request) = market("0.001", 20);
    let data = job::catalog::load_catalog(directory.path(), &request.selection).unwrap();
    for (series, fee) in data.series.iter().zip(&request.settings.fee_rates) {
        let original = serde_json::to_value(&series.instrument).unwrap();
        let (class, definition) = domain::catalogs::instrument_definition(&original).unwrap();
        assert_eq!(class, "CurrencyPair");
        assert!(domain::catalogs::execution_account(
            class,
            contracts::science::NativeAccountKind::Cash
        )
        .is_err());
        assert!(domain::catalogs::execution_account(
            class,
            contracts::science::NativeAccountKind::Margin
        )
        .is_ok());
        assert_eq!(definition["id"], fee.instrument_id);
        assert_eq!(definition["quote_currency"], request.settings.base_currency);
        for (field, expected) in [("maker_fee", &fee.maker), ("taker_fee", &fee.taker)] {
            let actual: contracts::DecimalValue =
                serde_json::from_value(definition[field].clone()).unwrap();
            assert_eq!(&actual, expected);
        }
    }
    assert!(domain::portfolio::simulation_settings(&request.settings).is_ok());
    assert!(domain::catalogs::execution_account(
        "Equity",
        contracts::science::NativeAccountKind::Cash
    )
    .is_ok());
    assert!(domain::catalogs::execution_account(
        "Equity",
        contracts::science::NativeAccountKind::Margin
    )
    .is_ok());
    let mut invalid = request.settings.clone();
    invalid.fee_rates.push(invalid.fee_rates[0].clone());
    assert!(domain::portfolio::simulation_settings(&invalid).is_err());
    let mut invalid = request.settings.clone();
    invalid.snapshot_interval_ms = 0;
    assert!(domain::portfolio::simulation_settings(&invalid).is_err());
    let mut invalid = request.settings;
    invalid.account_kind = contracts::science::NativeAccountKind::Cash;
    invalid.leverage = "2".parse().unwrap();
    assert!(domain::portfolio::simulation_settings(&invalid).is_err());
}

#[test]
fn two_assets_rebalance_inside_one_native_account_with_real_positions() {
    // Daily account returns require snapshots across UTC dates, not 20 minutes.
    let (directory, request) = market("0", 2 * 1440 + 20);
    let result = simulate(directory.path(), &request).unwrap();
    assert_eq!(result.native_version, "0.63.0");
    assert_eq!(result.consumed_target_points.get(), 2);
    assert_eq!(
        result.summary.get("venues.total").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        result.summary.get("account.SIM.type").map(String::as_str),
        Some("MARGIN")
    );
    assert_eq!(
        result.summary.get("orders.open").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        result.summary.get("orders.inflight").map(String::as_str),
        Some("0")
    );
    assert!(result.orders.get() >= 4);
    assert!(result.positions.get() >= 2);
    assert!(result.events.get() > result.orders.get());
    assert_eq!(result.returns_kind, NativeReturnsKind::PortfolioDaily);
    assert_eq!(result.returns_status, MetricStatus::Ok);
    assert!(result.returns_reason.is_none());
    assert!(result.returns.len() >= 2);
    assert!(result
        .returns
        .windows(2)
        .all(|pair| pair[0].timestamp_ns < pair[1].timestamp_ns));
    assert!(result
        .returns
        .iter()
        .all(|r| r.value.is_none_or(f64::is_finite)));
    assert!(result
        .statistics
        .iter()
        .all(|r| r.value.is_none_or(f64::is_finite)));
    CanonicalBacktestResult::from_slice(&serde_json::to_vec(&result.canonical_result).unwrap())
        .unwrap();
}

fn assert_daily_returns_unavailable(result: &NativeSimulationResultV1) {
    assert_eq!(result.returns_kind, NativeReturnsKind::PortfolioDaily);
    assert_eq!(result.returns_status, MetricStatus::InsufficientData);
    assert_eq!(
        result.returns_reason.as_deref(),
        Some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE")
    );
    assert!(result.returns.is_empty());
    assert!(result
        .statistics
        .iter()
        .filter(|stat| stat.group == NativeStatisticGroup::Returns)
        .all(|stat| stat.value.is_none()));
}

#[test]
fn intraday_equity_does_not_invent_daily_returns() {
    let (directory, request) = market("0", 20);
    let result = simulate(directory.path(), &request).unwrap();
    assert!(result.orders.get() >= 4);
    assert_daily_returns_unavailable(&result);
}

#[test]
fn closed_position_returns_cannot_substitute_for_daily_portfolio_evidence() {
    let (directory, mut request) = market("0", 20);
    let mut exit = request.target_points[1].clone();
    exit.asof_ns = instant(12);
    exit.cash_weight = "1".parse().unwrap();
    for target in &mut exit.targets {
        target.weight = "0".parse().unwrap();
    }
    request.target_points.push(exit);
    let result = simulate(directory.path(), &request).unwrap();
    assert_eq!(result.consumed_target_points.get(), 3);
    // Native canonical output preserves its own position-return fallback.
    assert!(!result
        .canonical_result
        .pointer("/statistics/returns_series")
        .and_then(serde_json::Value::as_array)
        .unwrap()
        .is_empty());
    assert_daily_returns_unavailable(&result);
}

#[test]
fn real_flat_account_daily_zero_returns_are_not_missing_observations() {
    let (directory, mut request) = market("0", 2 * 1440 + 20);
    request.target_points.truncate(1);
    request.target_points[0].cash_weight = "1".parse().unwrap();
    for target in &mut request.target_points[0].targets {
        target.weight = "0".parse().unwrap();
    }
    let result = simulate(directory.path(), &request).unwrap();
    assert_eq!(result.orders.get(), 0);
    assert_eq!(
        result.returns_status,
        MetricStatus::Ok,
        "native flat-account snapshots: {}",
        result.canonical_result["portfolio_snapshots"]
    );
    assert!(result.returns_reason.is_none());
    assert!(result.returns.len() >= 2);
    assert!(result.returns.iter().all(|point| point.value == Some(0.0)));
}

#[test]
fn concurrent_native_processes_keep_independent_accounts_and_equity_curves() {
    let runs = (0..4)
        .map(|index| {
            std::thread::spawn(move || {
                let (directory, mut request) = market("0", 2 * 1440 + 20);
                if index % 2 == 0 {
                    request.target_points.truncate(1);
                    request.target_points[0].cash_weight = "1".parse().unwrap();
                    for target in &mut request.target_points[0].targets {
                        target.weight = "0".parse().unwrap();
                    }
                }
                let result = simulate(directory.path(), &request).unwrap();
                assert_eq!(result.returns_status, MetricStatus::Ok);
                assert!(result.returns.len() >= 2);
                if index % 2 == 0 {
                    assert_eq!(result.orders.get(), 0);
                    assert!(result.returns.iter().all(|point| point.value == Some(0.0)));
                } else {
                    assert!(result.orders.get() >= 4);
                    assert!(result
                        .returns
                        .iter()
                        .any(|point| point.value.is_some_and(|v| v != 0.0)));
                }
            })
        })
        .collect::<Vec<_>>();
    for run in runs {
        run.join().unwrap();
    }
}

#[test]
fn frozen_native_fees_change_the_same_shared_capital_simulation() {
    let (free_directory, free_request) = market("0", 20);
    let free = simulate(free_directory.path(), &free_request).unwrap();
    let (cost_directory, cost_request) = market("0.01", 20);
    let cost = simulate(cost_directory.path(), &cost_request).unwrap();
    let balance = |result: &contracts::science::NativeSimulationResultV1| {
        Money::from_str(result.summary.get("account.SIM.balance.USD.total").unwrap())
            .unwrap()
            .as_decimal()
    };
    assert!(balance(&cost) < balance(&free));
    assert_ne!(free.canonical_result, cost.canonical_result);
    let mut mismatched = cost_request;
    mismatched.settings.fee_rates[0].taker = "0".parse().unwrap();
    assert!(simulate(cost_directory.path(), &mismatched).is_err());
}

#[test]
fn frozen_fill_and_latency_models_reach_the_actual_native_venue() {
    use contracts::portfolio::NativeModelRefV1;
    let (directory, request) = market("0", 20);
    let balance = |result: NativeSimulationResultV1| {
        Money::from_str(result.summary.get("account.SIM.balance.USD.total").unwrap())
            .unwrap()
            .as_decimal()
    };
    let baseline = balance(simulate(directory.path(), &request).unwrap());
    let mut changed = request.clone();
    if let NativeModelRefV1::NautilusDefaultFill { parameters, .. } =
        &mut changed.settings.fill_model
    {
        parameters.prob_slippage = "1".parse().unwrap();
    } else {
        panic!("original fill model");
    }
    assert!(balance(simulate(directory.path(), &changed).unwrap()) < baseline);
    if let NativeModelRefV1::NautilusDefaultFill { parameters, .. } =
        &mut changed.settings.fill_model
    {
        parameters.prob_slippage = "0.5".parse().unwrap();
    }
    assert_eq!(
        balance(simulate(directory.path(), &changed).unwrap()),
        balance(simulate(directory.path(), &changed).unwrap())
    );
    let mut base_delay = request.clone();
    if let NativeModelRefV1::NautilusStaticLatency { parameters, .. } =
        &mut base_delay.settings.latency_model
    {
        parameters.base_latency_ns = parameters.insert_latency_ns;
        parameters.insert_latency_ns = count(0);
    }
    assert_eq!(
        balance(simulate(directory.path(), &base_delay).unwrap()),
        baseline
    );
    for mutation in 0..5 {
        let mut invalid = request.clone();
        match mutation {
            0 => invalid.settings.fee_model = invalid.settings.fill_model.clone(),
            1 => invalid.settings.fill_model = invalid.settings.latency_model.clone(),
            2 => {
                if let NativeModelRefV1::NautilusMakerTaker {
                    upstream_version, ..
                } = &mut invalid.settings.fee_model
                {
                    *upstream_version = "0".into();
                }
            }
            3 => {
                if let NativeModelRefV1::NautilusDefaultFill { parameters, .. } =
                    &mut invalid.settings.fill_model
                {
                    parameters.prob_slippage = "1.01".parse().unwrap();
                }
            }
            _ => {
                if let NativeModelRefV1::NautilusStaticLatency { upstream_class, .. } =
                    &mut invalid.settings.latency_model
                {
                    *upstream_class = "unknown".into();
                }
            }
        }
        assert!(
            simulate(directory.path(), &invalid).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn invalid_expired_future_or_coalesced_targets_do_not_become_success() {
    let (directory, request) = market("0", 20);
    let mut invalid = request.clone();
    invalid.target_points[0].targets[1].instrument_id =
        invalid.target_points[0].targets[0].instrument_id.clone();
    assert!(simulate(directory.path(), &invalid).is_err());
    invalid = request.clone();
    invalid.target_points[0].cash_weight = "1".parse().unwrap();
    assert!(simulate(directory.path(), &invalid).is_err());
    invalid = request.clone();
    invalid.target_points[0].valid_until_ns = invalid.target_points[0].asof_ns;
    assert!(simulate(directory.path(), &invalid).is_err());
    invalid = request.clone();
    invalid.target_points[1].asof_ns = instant(100);
    assert!(simulate(directory.path(), &invalid).is_err());
    invalid = request.clone();
    invalid.target_points[0].asof_ns = count(1);
    invalid.target_points[1].asof_ns = count(2);
    assert!(simulate(directory.path(), &invalid).is_err());
    invalid = request;
    invalid.settings.latency_model = market::execution_models::latency(0);
    assert!(simulate(directory.path(), &invalid).is_err());
}

#[test]
fn all_cash_is_a_real_native_result_not_fabricated_asset_exposure() {
    let (directory, mut request) = market("0", 20);
    request.target_points.truncate(1);
    request.target_points[0].cash_weight = "1".parse().unwrap();
    for target in &mut request.target_points[0].targets {
        target.weight = "0".parse().unwrap();
    }
    let result = simulate(directory.path(), &request).unwrap();
    assert_eq!(result.orders.get(), 0);
    assert_eq!(result.positions.get(), 0);
    assert_eq!(result.consumed_target_points.get(), 1);
    let balance =
        Money::from_str(result.summary.get("account.SIM.balance.USD.total").unwrap()).unwrap();
    assert_eq!(balance.as_decimal(), rust_decimal::Decimal::from(1_000_000));
}

#[test]
fn native_equities_rebalance_in_cash_and_margin_accounts_with_original_fees() {
    use contracts::science::NativeAccountKind;
    for (kind, label) in [
        (NativeAccountKind::Cash, "CASH"),
        (NativeAccountKind::Margin, "MARGIN"),
    ] {
        let mut balances = Vec::new();
        for fee in ["0", "0.01"] {
            let (directory, mut request) = market::equity_market(fee, 20);
            request.settings.account_kind = kind;
            let result = simulate(directory.path(), &request).unwrap();
            assert_eq!(result.native_version, "0.63.0");
            assert_eq!(result.consumed_target_points.get(), 2);
            assert_eq!(result.summary["venues.total"], "1");
            assert_eq!(result.summary["account.SIM.type"], label);
            assert_eq!(result.summary["orders.open"], "0");
            assert_eq!(result.summary["orders.inflight"], "0");
            assert!(result.orders.get() >= 4);
            assert!(result.positions.get() >= 2);
            balances.push(
                Money::from_str(&result.summary["account.SIM.balance.USD.total"])
                    .unwrap()
                    .as_decimal(),
            );
            request.settings.base_currency = "EUR".into();
            assert_eq!(
                simulate(directory.path(), &request).unwrap_err().trim(),
                "QZ_NATIVE_JOB_FAILED"
            );
        }
        assert!(
            balances[1] < balances[0],
            "native Equity fees must reduce the shared balance"
        );
    }
}

#[test]
fn native_binary_options_do_not_produce_portfolio_results_before_or_after_expiry() {
    use nautilus_model::{
        enums::AssetClass,
        instruments::{BinaryOption, Instrument, InstrumentAny},
        types::{Currency, Price, Quantity},
    };
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;
    let (source, request) = market::equity_market("0", 20);
    let observed = job::catalog::load_catalog(source.path(), &request.selection).unwrap();
    for expiration in [instant(5), instant(50)] {
        let directory = tempfile::tempdir().unwrap();
        let catalog = ParquetDataCatalog::from_uri(
            directory.path().to_str().unwrap(),
            None,
            Some(16),
            None,
            None,
        )
        .unwrap();
        for series in &observed.series {
            let instrument = BinaryOption::builder()
                .instrument_id(series.instrument.id())
                .raw_symbol(series.instrument.raw_symbol())
                .asset_class(AssetClass::Alternative)
                .currency(Currency::USD())
                .activation_ns(0_u64.into())
                .expiration_ns(expiration.get().into())
                .price_precision(5)
                .size_precision(0)
                .price_increment(Price::from("0.00001"))
                .size_increment(Quantity::from("1"))
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap();
            catalog
                .write_instruments(vec![InstrumentAny::BinaryOption(instrument)])
                .unwrap();
            let mut bars = series.bars.clone();
            for bar in &mut bars {
                bar.open = Price::new(bar.open.as_f64() / 10.0, 5);
                bar.high = Price::new(bar.high.as_f64() / 10.0, 5);
                bar.low = Price::new(bar.low.as_f64() / 10.0, 5);
                bar.close = Price::new(bar.close.as_f64() / 10.0, 5);
            }
            catalog.write_to_parquet(&bars, None, None, None).unwrap();
        }
        let data = job::catalog::load_catalog(directory.path(), &request.selection).unwrap();
        assert_eq!(data.rows, observed.rows);
        assert!(data
            .series
            .iter()
            .all(|s| matches!(s.instrument, InstrumentAny::BinaryOption(_))
                && s.instrument.expiration_ns().map(|value| value.as_u64())
                    == Some(expiration.get())));
        assert_eq!(
            job::simulation::simulate(directory.path(), &request)
                .unwrap_err()
                .to_string(),
            "SIMULATION_MARKET_UNSUPPORTED"
        );
        let output = native::command(
            &[
                "simulate".as_ref(),
                "--catalog".as_ref(),
                directory.path().as_os_str(),
            ],
            &request,
        );
        assert!(!output.status.success());
        assert!(
            output.stdout.is_empty(),
            "unsupported instruments must not publish a simulation result"
        );
        assert_eq!(
            String::from_utf8(output.stderr).unwrap().trim(),
            "QZ_NATIVE_JOB_FAILED"
        );
    }
}
