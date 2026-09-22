//! Native regression tests, not real-market performance claims.
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;
#[path = "support/polymarket.rs"]
mod prediction;
use contracts::{portfolio::*, science::*};
use prediction::{IDS, STEP};
use std::{fs, path::Path};

fn simulation(
    rate: &str,
    resolved: Option<[&str; 2]>,
    delay_minutes: u64,
) -> (tempfile::TempDir, NativeSimulationRequestV1) {
    let (_unused, mut request) = market::market("0", 20);
    let directory = tempfile::tempdir().unwrap();
    let expiration = 18 * STEP;
    prediction::write_catalog(directory.path(), 17, rate, expiration, false);
    prediction::settings(&mut request.settings, rate, expiration);
    request.settings.starting_capital = "1000".parse().unwrap();
    request.selection.bar_types = IDS
        .iter()
        .map(|id| format!("{id}-1-MINUTE-LAST-EXTERNAL"))
        .collect();
    request.selection.event_end_ns = market::count((21 + delay_minutes) * STEP);
    request.selection.decision_cutoff_ns = request.selection.event_end_ns;
    request.target_points.truncate(1);
    let target = &mut request.target_points[0];
    target.valid_until_ns = request.selection.event_end_ns;
    target.cash_weight = "0.6".parse().unwrap();
    target.targets = IDS
        .iter()
        .map(|id| AllocationTargetV1 {
            instrument_id: (*id).into(),
            currency: "pUSD".into(),
            weight: "0.2".parse().unwrap(),
        })
        .collect();
    if let Some(payouts) = resolved {
        prediction::settle(
            directory.path(),
            expiration,
            (19 + delay_minutes) * STEP,
            payouts,
        );
    }
    (directory, request)
}

fn simulate(
    root: &Path,
    request: &NativeSimulationRequestV1,
) -> Result<NativeSimulationResultV1, String> {
    let output = native::command(
        &["simulate".as_ref(), "--catalog".as_ref(), root.as_os_str()],
        request,
    );
    if !output.status.success() {
        // Keep the real CLI failure; inspect only this synthetic fixture in-process
        // for a useful test diagnostic. The second call cannot turn failure into success.
        let detail = job::simulation::simulate(root, request)
            .err()
            .map(|error| format!("{error:?}"))
            .unwrap_or_else(|| "CLI/library result mismatch".into());
        return Err(format!(
            "{}: {detail}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}

fn pnl(result: &NativeSimulationResultV1) -> f64 {
    result
        .statistics
        .iter()
        .find(|s| {
            s.group == NativeStatisticGroup::Pnl
                && s.native_key == "PnL (total)"
                && s.currency.as_deref() == Some("pUSD")
        })
        .unwrap_or_else(|| panic!("native statistics: {:?}", result.statistics))
        .value
        .unwrap()
}

#[test]
fn native_shared_cash_settles_zero_one_and_half_at_original_availability() {
    for payouts in [["1.0000", "0.0000"], ["0.5000", "0.5000"]] {
        for delay in [0, 2 * 1440] {
            let (catalog, request) = simulation("0", Some(payouts), delay);
            let result = simulate(catalog.path(), &request).unwrap();
            assert!(result
                .statistics
                .iter()
                .filter(|s| s.group == NativeStatisticGroup::Pnl)
                .all(|s| s.currency.as_deref() == Some("pUSD")));
            assert_eq!(result.consumed_target_points.get(), 1);
            assert_eq!(
                result
                    .summary
                    .get("account.POLYMARKET.type")
                    .map(String::as_str),
                Some("CASH")
            );
            assert_eq!(
                result.summary.get("orders.open").map(String::as_str),
                Some("0")
            );
            // Both 500-share positions cost 400 pUSD and return 500 pUSD, sharing one account.
            assert!(
                (pnl(&result) - 100.0).abs() < 0.0001,
                "{:?}",
                result.statistics
            );
            let canonical: nautilus_backtest::result::CanonicalBacktestResult =
                nautilus_backtest::result::CanonicalBacktestResult::from_slice(
                    &serde_json::to_vec(&result.canonical_result).unwrap(),
                )
                .unwrap();
            let text = serde_json::to_string(canonical.as_value()).unwrap();
            assert!(text.contains("POLYMARKET"));
            assert!(!text.contains("USDC.e"));
        }
    }
}

#[test]
fn native_price_dependent_commission_is_not_a_fixed_planning_coefficient() {
    let (catalog, request) = simulation("0.05", Some(["1.0000", "0.0000"]), 0);
    let result = simulate(catalog.path(), &request).unwrap();
    // Independent fixture calculation: each 500-share buy at .4 incurs 500*.05*.4*.6=6.
    // Redemptions are not exchange trades, including a half payout.
    assert!(
        (pnl(&result) - 88.0).abs() < 0.0001,
        "{:?}",
        result.statistics
    );
    let (half, half_request) = simulation("0.05", Some(["0.5000", "0.5000"]), 0);
    assert!((pnl(&simulate(half.path(), &half_request).unwrap()) - 88.0).abs() < 0.0001);
}

#[test]
fn no_settlement_is_not_assumed_from_expiry_or_the_last_price() {
    let (catalog, request) = simulation("0", None, 0);
    assert!(simulate(catalog.path(), &request)
        .unwrap_err()
        .contains("POLYMARKET_PENDING_RESOLUTION"));
    let mut before_expiry = request;
    before_expiry.selection.event_end_ns = market::count(17 * STEP + 2);
    before_expiry.selection.decision_cutoff_ns = before_expiry.selection.event_end_ns;
    before_expiry.target_points[0].valid_until_ns = before_expiry.selection.event_end_ns;
    assert!(simulate(catalog.path(), &before_expiry).is_ok());
}

#[test]
fn original_fee_model_and_collateral_are_required_without_usd_aliases() {
    let (catalog, request) = simulation("0.05", Some(["1.0000", "0.0000"]), 0);
    let mut wrong = request.clone();
    wrong.settings.fee_model = market::execution_models::fee();
    assert!(simulate(catalog.path(), &wrong)
        .unwrap_err()
        .contains("POLYMARKET_NATIVE_FEE_MODEL_REQUIRED"));
    let mut wrong = request;
    wrong.settings.base_currency = "USD".into();
    for point in &mut wrong.target_points {
        for target in &mut point.targets {
            target.currency = "USD".into();
        }
    }
    assert!(simulate(catalog.path(), &wrong).is_err());
}

#[test]
fn two_original_alpha_members_run_through_native_portfolio_study() {
    let (_unused, mut request, model) = market::study();
    let catalog = tempfile::tempdir().unwrap();
    let expiry = 4 * 1440 * STEP;
    prediction::write_catalog(catalog.path(), 2900, "0", expiry, true);
    prediction::settings(&mut request.execution_settings, "0", expiry);
    request.mandate.base_currency = "pUSD".into();
    request.source_selection.bar_types = IDS
        .iter()
        .map(|id| format!("{id}-1-MINUTE-LAST-EXTERNAL"))
        .collect();
    for (asset, id) in request.assets.iter_mut().zip(IDS) {
        asset.instrument_id = id.into();
        asset.currency = "pUSD".into();
    }
    assert!(request.members.len() >= 2);
    assert_ne!(
        request.members[0].alpha_version_id,
        request.members[1].alpha_version_id
    );
    let objects = tempfile::tempdir().unwrap();
    for member in &request.members {
        fs::write(
            objects.path().join(member.model_artifact_id.to_string()),
            &model,
        )
        .unwrap();
    }
    fs::write(
        objects.path().join(
            request
                .mandate
                .constraints
                .transaction_costs_ref
                .to_string(),
        ),
        serde_json::to_vec(&request.execution_settings).unwrap(),
    )
    .unwrap();
    let output = native::command(
        &[
            "study-portfolio".as_ref(),
            "--catalog".as_ref(),
            catalog.path().as_os_str(),
            "--objects".as_ref(),
            objects.path().as_os_str(),
        ],
        &request,
    );
    if !output.status.success() {
        let detail = job::study::evaluate(catalog.path(), &request, |id| {
            Ok(fs::read(objects.path().join(id.to_string()))?)
        })
        .err()
        .map(|error| format!("{error:?}"));
        panic!(
            "{}; fixture diagnostic: {detail:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let result: NativePortfolioStudyResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    domain::execution::check_portfolio_study(&request, &result).unwrap();
    assert_eq!(result.frames.len(), 3);
    assert!(result.consumed_fuel.get() > 0);
}
