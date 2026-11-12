//! Actual native account publications and fees; controlled source, not real trading evidence.
#![cfg(feature = "hyperliquid-offline")]
#[path = "support/command.rs"]
mod native;
#[path = "support/spot_cash.rs"]
mod spot;
use contracts::{
    observed_spot::*, science::*, spot_cash_report::NativeSpotCashObservationKindV1, Id, SchemaV1,
};
use nautilus_model::events::PortfolioSnapshot;
use spot::{count, money, MINUTE, START};

fn fixture(
    rows: u32,
    trend: bool,
) -> (
    tempfile::TempDir,
    NativeObservedSpotComparisonRequestV1,
    NativeSimulationResultV1,
) {
    let (root, mut simulation) = spot::source_fixture_with_rows(rows, trend);
    let dataset = Id::new();
    // Create the source account through a real original buy. Its fee and exact
    // remaining inventory are copied from the official subsequent publication.
    let mut origin = simulation.clone();
    origin.selection.event_end_ns = count(START + 5 * MINUTE);
    origin.selection.decision_cutoff_ns = count(START + 5 * MINUTE + 500_000_000);
    origin.target_points.truncate(1);
    let produced = job::simulation::simulate_explicit(root.path(), &origin, Some(dataset)).unwrap();
    let initial_state = original_initial_state(&produced);
    simulation.selection.event_start_ns = count(START + 6 * MINUTE);
    let point = &mut simulation.target_points[0];
    point.asof_ns = count(START + 7 * MINUTE + 500_000_000);
    point.valid_until_ns = count(START + 9 * MINUTE);
    point.targets[0].weight = money("0");
    point.cash_weight = money("1");
    simulation.target_points.truncate(1);
    (
        root,
        NativeObservedSpotComparisonRequestV1 {
            schema_version: SchemaV1,
            dataset_revision_id: dataset,
            initial_state,
            simulation,
        },
        produced,
    )
}

fn original_initial_state(produced: &NativeSimulationResultV1) -> NativeObservedSpotStartV1 {
    let initial_state = projected_initial_state(produced);
    assert!(initial_state
        .snapshot
        .balances
        .iter()
        .any(|balance| balance.total.currency == "BTC" && balance.total.amount.is_positive()));
    initial_state
}

fn projected_initial_state(produced: &NativeSimulationResultV1) -> NativeObservedSpotStartV1 {
    let report = produced.spot_cash_report.as_ref().unwrap();
    let raw = report
        .observations
        .iter()
        .rev()
        .find_map(|observation| match &observation.record {
            NativeSpotCashObservationKindV1::Snapshot { native, .. } => Some(native.clone()),
            _ => None,
        })
        .unwrap();
    let raw: PortfolioSnapshot = serde_json::from_value(raw).unwrap();
    let snapshot = job::account_observer::project_snapshot(&raw).unwrap();
    NativeObservedSpotStartV1 {
        schema_version: SchemaV1,
        snapshot_evidence_ref: "CONTROLLED_ORIGINAL_NATIVE_PUBLICATION".into(),
        source_native_version: "0.69.0".into(),
        available_ns: count(snapshot.ts_init.get() + 1),
        open_orders_evidence_ref: "CONTROLLED_SAME_RUN_CANONICAL_OPEN_ORDER_COUNT".into(),
        open_orders_account_id: snapshot.account_id.clone(),
        open_orders_asof_ns: snapshot.ts_event,
        open_orders_available_ns: count(snapshot.ts_init.get() + 2),
        open_orders_count: produced.summary["orders.open"].parse().unwrap(),
        snapshot,
    }
}

fn completed(leg: &NativeObservedSpotSimulationLegV1) -> &NativeSimulationResultV1 {
    assert!(
        matches!(leg.status, NativeObservedSpotLegStatusV1::Completed),
        "{:?}",
        leg.reason
    );
    leg.result.as_ref().unwrap()
}

#[test]
fn cli_preserves_sub_currency_capital_through_original_snapshot_and_wire_projection() {
    let (root, mut request) = spot::source_fixture();
    request.settings.starting_capital = money("1000.00000008");
    for point in &mut request.target_points {
        point.cash_weight = money("1");
        for target in &mut point.targets {
            target.weight = money("0");
        }
    }
    let revision = Id::new().to_string();
    let output = native::command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
            "--dataset-revision-id".as_ref(),
            revision.as_ref(),
        ],
        &request,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: NativeSimulationResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result.orders.get(), 0);
    let original = result
        .spot_cash_report
        .as_ref()
        .unwrap()
        .observations
        .iter()
        .find_map(|observation| match &observation.record {
            NativeSpotCashObservationKindV1::Snapshot { native, .. } => Some(native),
            _ => None,
        })
        .unwrap();
    // This is the actual original engine event retained by spot capture, with
    // its real UUID. The normalized canonical event is deliberately separate.
    let native: PortfolioSnapshot = serde_json::from_value(original.clone()).unwrap();
    let projected = job::account_observer::project_snapshot(&native).unwrap();
    let balance = projected
        .balances
        .iter()
        .find(|balance| balance.total.currency == "USDC")
        .unwrap();
    assert_eq!(balance.total.amount, request.settings.starting_capital);
    assert_eq!(balance.free.amount, request.settings.starting_capital);
    let equity = projected
        .total_equity
        .iter()
        .find(|money| money.currency == "USDC")
        .unwrap();
    assert_eq!(equity.amount, request.settings.starting_capital);
    assert_eq!(projected.event_id, native.event_id.to_string());
    assert_eq!(
        serde_json::to_value(&balance.total).unwrap()["amount"],
        "1000.00000008"
    );
    assert_eq!(
        serde_json::to_value(equity).unwrap()["amount"],
        "1000.00000008"
    );
}

#[test]
fn matched_hold_keeps_original_inventory_and_rebalance_pays_real_exit_fee() {
    let (root, request, _) = fixture(20, false);
    let result = job::observed_spot::compare(root.path(), &request).unwrap();
    let hold = completed(&result.hold);
    let rebalance = completed(&result.rebalance);
    assert_eq!(hold.orders.get(), 0);
    assert_eq!(hold.positions.get(), 0);
    assert_eq!(hold.consumed_target_points.get(), 0);
    assert!(rebalance.orders.get() > 0);
    let hold_account = &hold.canonical_result["accounts"][0]["Cash"]["base"];
    let rebalance_account = &rebalance.canonical_result["accounts"][0]["Cash"]["base"];
    assert_eq!(
        hold_account["balances_starting"],
        rebalance_account["balances_starting"]
    );
    for balance in &request.initial_state.snapshot.balances {
        let actual = hold_account["balances_starting"][&balance.total.currency]
            .as_str()
            .unwrap()
            .parse::<nautilus_model::types::Money>()
            .unwrap();
        assert_eq!(
            actual
                .as_decimal()
                .to_string()
                .parse::<contracts::DecimalValue>()
                .unwrap(),
            balance.total.amount
        );
    }
    let hold_report = hold.spot_cash_report.as_ref().unwrap();
    let rebalance_report = rebalance.spot_cash_report.as_ref().unwrap();
    assert_eq!(
        hold_report.flow_evidence.period_start_ns,
        rebalance_report.flow_evidence.period_start_ns
    );
    assert_eq!(
        hold_report.flow_evidence.period_end_ns,
        rebalance_report.flow_evidence.period_end_ns
    );
    assert_eq!(
        hold_report.flow_evidence.external_flow_count,
        Some(count(0))
    );
    assert_eq!(
        rebalance_report.flow_evidence.external_flow_count,
        Some(count(0))
    );
    let final_total = |simulation: &NativeSimulationResultV1| {
        simulation
            .spot_cash_report
            .as_ref()
            .unwrap()
            .observations
            .iter()
            .rev()
            .find_map(|observation| match &observation.record {
                NativeSpotCashObservationKindV1::Snapshot { valuation, .. } => {
                    match &valuation.outcome {
                        contracts::spot_cash::ReportCurrencyValuationOutcomeV1::Complete {
                            total,
                            ..
                        } => Some(total.clone()),
                        _ => None,
                    }
                }
                _ => None,
            })
            .unwrap()
    };
    assert!(
        final_total(rebalance).as_decimal() < final_total(hold).as_decimal(),
        "A real liquidation fee must reduce the matched account at constant price."
    );
    assert!(rebalance.canonical_result["orders"]
        .as_array()
        .unwrap()
        .iter()
        .any(|order| order.to_string().contains("SELL")));
    assert!(rebalance_account["commissions"]
        .to_string()
        .contains("USDC"));
    let mut unsupported = rebalance.clone();
    unsupported.native_version = "0.68.0".into();
    assert!(domain::execution::check_observed_spot_simulation(
        &request.simulation,
        &unsupported,
        &request.initial_state
    )
    .is_err());
    let mut altered = hold.clone();
    altered.orders = count(1);
    let mut hold_request = request.simulation.clone();
    hold_request.target_points.clear();
    assert!(domain::execution::check_observed_spot_simulation(
        &hold_request,
        &altered,
        &request.initial_state
    )
    .is_err());
}

#[test]
fn value_uses_original_balances_and_only_marks_available_at_snapshot_event() {
    let (root, comparison, _) = fixture(20, true);
    let mut selection = comparison.simulation.selection.clone();
    selection.event_start_ns = count(START);
    selection.event_end_ns = comparison.initial_state.snapshot.ts_event;
    selection.decision_cutoff_ns = comparison.initial_state.snapshot.ts_event;
    let request = NativeObservedSpotValuationRequestV1 {
        schema_version: SchemaV1,
        dataset_revision_id: comparison.dataset_revision_id,
        initial_state: comparison.initial_state.clone(),
        selection,
        settings: comparison.simulation.settings.clone(),
    };
    let value = job::observed_spot::value(root.path(), &request).unwrap();
    assert!(value.available_ns > value.asof_ns);
    assert!(value
        .legs
        .iter()
        .filter_map(|leg| leg.price_available_ns)
        .all(|clock| clock <= value.asof_ns));
    let expected = request
        .initial_state
        .snapshot
        .balances
        .iter()
        .map(|balance| {
            let multiplier = if balance.total.currency == "BTC" {
                money("100.04")
            } else {
                money("1")
            };
            balance.total.amount.as_decimal() * multiplier.as_decimal()
        })
        .sum::<bigdecimal::BigDecimal>();
    assert_eq!(value.total.as_decimal(), &expected);
    assert!(value.source_evidence.is_some());
    let mut future = request.clone();
    future.selection.decision_cutoff_ns = count(value.asof_ns.get() + MINUTE);
    assert!(job::observed_spot::value(root.path(), &future).is_err());
    let mut missing_orders = serde_json::to_value(&request).unwrap();
    missing_orders["initial_state"]
        .as_object_mut()
        .unwrap()
        .remove("open_orders_count");
    assert!(
        serde_json::from_value::<NativeObservedSpotValuationRequestV1>(missing_orders).is_err()
    );
}

#[test]
fn unknown_locked_wrong_account_and_future_state_cannot_be_replayed() {
    let (root, original, _) = fixture(20, false);
    for mutation in 0..6 {
        let mut request = original.clone();
        match mutation {
            0 => request.initial_state.open_orders_count = 1,
            1 => request
                .initial_state
                .open_orders_account_id
                .push_str("OTHER"),
            2 => request.initial_state.snapshot.balances[0].locked.amount = money("1"),
            3 => {
                request.initial_state.available_ns =
                    count(request.simulation.selection.event_start_ns.get() + 1)
            }
            4 => request.initial_state.snapshot.is_stale = true,
            _ => {
                request.initial_state.snapshot.account_id = "OTHER-001".into();
                request.initial_state.open_orders_account_id = "OTHER-001".into();
            }
        }
        assert!(
            job::observed_spot::compare(root.path(), &request).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn actual_cli_uses_the_explicit_observed_contract() {
    let (root, request, _) = fixture(20, false);
    let output = native::command(
        &[
            "compare-observed-spot".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
        ],
        &request,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: NativeObservedSpotComparisonResultV1 =
        serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(completed(&result.hold).orders.get(), 0);
    completed(&result.rebalance);
}

#[test]
fn observed_cash_fixed_buy_preserves_starting_balances_and_pays_native_base_fee() {
    let (root, mut source) = spot::source_fixture_with_rows(30, true);
    let dataset = Id::new();
    let mut origin = source.clone();
    origin.selection.event_end_ns = count(START + 5 * MINUTE);
    origin.selection.decision_cutoff_ns = count(START + 5 * MINUTE + 500_000_000);
    origin.target_points.truncate(1);
    origin.target_points[0].targets[0].weight = money("0");
    origin.target_points[0].cash_weight = money("1");
    let cash = job::simulation::simulate_explicit(root.path(), &origin, Some(dataset)).unwrap();
    assert_eq!(cash.orders.get(), 0);
    source.selection.event_start_ns = count(START + 6 * MINUTE);
    source.target_points.truncate(1);
    source.target_points[0].asof_ns = count(START + 7 * MINUTE + 500_000_000);
    source.target_points[0].valid_until_ns = count(START + 9 * MINUTE);
    source.target_points[0].targets[0].weight = money("0.5");
    source.target_points[0].cash_weight = money("0.5");
    let request = NativeObservedSpotComparisonRequestV1 {
        schema_version: SchemaV1, dataset_revision_id: dataset,
        initial_state: projected_initial_state(&cash), simulation: source,
    };
    let comparison = job::observed_spot::compare(root.path(), &request).unwrap();
    let hold = completed(&comparison.hold);
    let bought = completed(&comparison.rebalance);
    assert_eq!(hold.orders.get(), 0);
    assert!(bought.orders.get() > 0);
    let account = &bought.canonical_result["accounts"][0]["Cash"]["base"];
    assert_eq!(account["balances_starting"], hold.canonical_result["accounts"][0]["Cash"]["base"]["balances_starting"]);
    assert!(!account["commissions"].as_object().unwrap().is_empty());
    assert!(account["commissions"].to_string().contains("BTC"));
    assert!(bought.canonical_result["orders"].as_array().unwrap().iter().any(|order| order.to_string().contains("BUY")));
}
