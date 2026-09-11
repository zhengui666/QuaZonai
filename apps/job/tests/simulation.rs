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
    invalid.settings.insert_latency_ns = count(0);
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
