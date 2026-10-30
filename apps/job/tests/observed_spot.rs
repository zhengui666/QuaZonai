//! Actual native account publications and fees; controlled source, not real trading evidence.
#![cfg(feature = "hyperliquid-offline")]
#[path = "support/command.rs"]
mod native;
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio_config;
#[path = "support/spot_cash.rs"]
mod spot;
use contracts::{
    observed_spot::*, science::*, spot_cash_report::NativeSpotCashObservationKindV1, Id, SchemaV1,
};
use nautilus_model::events::PortfolioSnapshot;
use spot::{count, money, MINUTE, START};
use std::collections::BTreeMap;
#[path = "support/observed_spot_forward.rs"]
mod forward_tests;
#[cfg(feature = "catalog-prepare")]
#[path = "support/observed_spot_pm.rs"]
mod multi_day;

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
    let initial_state = NativeObservedSpotStartV1 {
        schema_version: SchemaV1,
        snapshot_evidence_ref: "CONTROLLED_ORIGINAL_NATIVE_PUBLICATION".into(),
        source_native_version: "0.67.0".into(),
        available_ns: count(snapshot.ts_init.get() + 1),
        open_orders_evidence_ref: "CONTROLLED_SAME_RUN_CANONICAL_OPEN_ORDER_COUNT".into(),
        open_orders_account_id: snapshot.account_id.clone(),
        open_orders_asof_ns: snapshot.ts_event,
        open_orders_available_ns: count(snapshot.ts_init.get() + 2),
        open_orders_count: produced.summary["orders.open"].parse().unwrap(),
        snapshot,
    };
    initial_state
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
                request.initial_state.snapshot.account_id = "BINANCE-001".into();
                request.initial_state.open_orders_account_id = "BINANCE-001".into();
            }
        }
        assert!(
            job::observed_spot::compare(root.path(), &request).is_err(),
            "mutation {mutation}"
        );
    }
}

fn study_request(
    comparison: &NativeObservedSpotComparisonRequestV1,
) -> (NativeObservedSpotStudyRequestV1, BTreeMap<Id, Vec<u8>>) {
    let input = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let mut template = portfolio_config::request(&input);
    template.assets.truncate(1);
    template.assets[0].instrument_id = "BTC-SPOT.HYPERLIQUID".into();
    template.assets[0].currency = "USDC".into();
    template.assets[0].groups.clear();
    template.assets[0].current_weight = money("0"); // Schedule placeholder; actual state must replace it.
    template.assets[0].transaction_cost_rate = money("0.0007");
    template.mandate.base_currency = "USDC".into();
    template.mandate.capital_assumption = money("1000");
    template.mandate.exposure_tolerance = comparison.simulation.settings.exposure_tolerance.clone();
    template.mandate.objective = contracts::portfolio::AllocationObjective::MaxUtility;
    template.mandate.constraints.min_cash_weight = money("0.4");
    template.mandate.constraints.max_cash_weight = money("0.4");
    template.mandate.constraints.min_net_exposure = money("0.6");
    template.mandate.constraints.max_net_exposure = money("0.6");
    // Source BAR event is its open; a fresh account receives its first fully
    // closed BAR after the replay start. The registered fixture permits this
    // observable execution delay while retaining original prediction clocks.
    template.mandate.rebalance_schedule.max_input_age_seconds = 300;
    template.mandate.rebalance_schedule.target_ttl_seconds = 600;
    let mut objects = BTreeMap::new();
    objects.insert(
        template.mandate.constraints.transaction_costs_ref,
        serde_json::to_vec(&comparison.simulation.settings).unwrap(),
    );
    for member in &template.members {
        objects.insert(member.model_artifact_id, wat::parse_str("(module (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 0.01))").unwrap());
    }
    let mut source_selection = comparison.simulation.selection.clone();
    source_selection.event_start_ns = count(START);
    source_selection.event_end_ns = count(START + 22 * MINUTE + 500_000_000);
    source_selection.decision_cutoff_ns = source_selection.event_end_ns;
    let study = NativePortfolioStudyRequestV1 {
        schema_version: SchemaV1,
        settlements: vec![],
        source_selection,
        evaluation_start_ns: count(START + 7 * MINUTE + 500_000_000),
        manual_cutoffs_ns: Some(vec![
            count(START + 7 * MINUTE + 500_000_000),
            count(START + 12 * MINUTE + 500_000_000),
        ]),
        calendar: None,
        research_available_through_ns: count(START),
        mandate: template.mandate,
        execution_settings: comparison.simulation.settings.clone(),
        rolling_liquidity: None,
        assets: template.assets,
        members: template.members,
    };
    (
        NativeObservedSpotStudyRequestV1 {
            schema_version: SchemaV1,
            dataset_revision_id: comparison.dataset_revision_id,
            initial_state: comparison.initial_state.clone(),
            study,
        },
        objects,
    )
}

#[test]
fn study_solves_from_actual_inventory_and_preserves_native_state_witness() {
    let (root, comparison, _) = fixture(30, true);
    let (request, objects) = study_request(&comparison);
    let result =
        job::observed_spot::study(root.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    completed(&result.hold);
    assert!(
        matches!(
            result.rebalance.status,
            NativeObservedSpotLegStatusV1::Completed
        ),
        "{:?}",
        result.rebalance.reason
    );
    let study = result.rebalance.result.as_ref().unwrap();
    assert_eq!(study.frames.len(), 2);
    assert!(study.frames[0].input.assets[0].current_weight.is_positive());
    assert_ne!(study.frames[0].input.current_cash_weight, money("1"));
    assert_ne!(study.frames[0].input.capital_assumption, money("1000"));
    assert!(study
        .frames
        .iter()
        .all(|frame| frame.input.assets[0].transaction_cost_rate == money("0.0007")));
    let mut forged = study.clone();
    forged.frames[0].input.assets[0].current_weight = money("0");
    assert!(domain::execution::check_observed_spot_study(
        &request.study,
        &forged,
        &request.initial_state
    )
    .is_err());
    // Legacy fresh-account study still refuses this opt-in mode.
    assert!(
        job::study::evaluate(root.path(), &request.study, |id| Ok(objects[&id].clone())).is_err()
    );
}

#[test]
fn observed_study_cash_funding_reoptimizes_real_cash_and_pays_native_fees() {
    use contracts::portfolio::NativeCashFundingPolicyV1;
    let (root, mut source) = spot::source_fixture_with_rows(30, true);
    let dataset = Id::new();
    let mut origin = source.clone();
    origin.selection.event_end_ns = count(START + 5 * MINUTE);
    origin.selection.decision_cutoff_ns = count(START + 5 * MINUTE + 500_000_000);
    origin.target_points.truncate(1);
    origin.target_points[0].targets[0].weight = money("0");
    origin.target_points[0].cash_weight = money("1");
    let actual_cash =
        job::simulation::simulate_explicit(root.path(), &origin, Some(dataset)).unwrap();
    assert_eq!(actual_cash.orders.get(), 0);
    source.selection.event_start_ns = count(START + 6 * MINUTE);
    let comparison = NativeObservedSpotComparisonRequestV1 {
        schema_version: SchemaV1,
        dataset_revision_id: dataset,
        initial_state: projected_initial_state(&actual_cash),
        simulation: source,
    };
    let (mut request, objects) = study_request(&comparison);
    let constraints = &mut request.study.mandate.constraints;
    constraints.cash_funding =
        Some(NativeCashFundingPolicyV1::PreserveMinCashAfterLinearTurnoverV1);
    constraints.min_cash_weight = money("0");
    constraints.max_cash_weight = money("1");
    constraints.min_net_exposure = money("0");
    constraints.max_net_exposure = money("1");
    let result =
        job::observed_spot::study(root.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    assert_eq!(completed(&result.hold).orders.get(), 0);
    assert_eq!(
        result.rebalance.status,
        NativeObservedSpotLegStatusV1::Completed,
        "{:?}",
        result.rebalance.reason
    );
    let study = result.rebalance.result.as_ref().unwrap();
    assert_eq!(study.frames[0].input.current_cash_weight, money("1"));
    assert_eq!(study.frames[0].input.assets[0].current_weight, money("0"));
    assert!(study.frames[1].input.assets[0].current_weight.is_positive());
    for frame in &study.frames {
        assert_eq!(
            frame.input.constraints.cash_funding,
            request.study.mandate.constraints.cash_funding
        );
        domain::portfolio::allocation_result(&frame.input, &frame.allocation).unwrap();
        let actual_fee: bigdecimal::BigDecimal = frame
            .allocation
            .targets
            .as_ref()
            .unwrap()
            .iter()
            .zip(&frame.input.assets)
            .map(|(target, asset)| {
                (target.weight.as_decimal() - asset.current_weight.as_decimal()).abs()
                    * asset.transaction_cost_rate.as_decimal()
            })
            .sum();
        assert!(
            frame.allocation.cash_weight.as_ref().unwrap().as_decimal()
                + frame.input.exposure_tolerance.as_decimal()
                >= actual_fee
        );
    }
    let simulation = study.simulation.as_ref().unwrap();
    assert!(simulation.orders.get() > 0);
    let account = &simulation.canonical_result["accounts"][0]["Cash"]["base"];
    assert_eq!(
        account["balances_starting"],
        completed(&result.hold).canonical_result["accounts"][0]["Cash"]["base"]
            ["balances_starting"]
    );
    assert!(!account["commissions"].as_object().unwrap().is_empty());
    assert!(account["commissions"].to_string().contains("BTC"));
    // The source type, not a leverage number, enables the new funding method.
    assert!(domain::execution::portfolio_study_cutoffs(&request.study).is_err());
}

#[test]
fn observed_study_cash_funding_sells_pay_native_quote_fees() {
    use contracts::portfolio::NativeCashFundingPolicyV1;
    let (root, comparison, _) = fixture(30, true);
    let (mut request, objects) = study_request(&comparison);
    let constraints = &mut request.study.mandate.constraints;
    constraints.cash_funding =
        Some(NativeCashFundingPolicyV1::PreserveMinCashAfterLinearTurnoverV1);
    constraints.min_cash_weight = money("0.8");
    constraints.max_cash_weight = money("1");
    constraints.min_net_exposure = money("0");
    constraints.max_net_exposure = money("1");
    let result =
        job::observed_spot::study(root.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    assert_eq!(completed(&result.hold).orders.get(), 0);
    assert_eq!(
        result.rebalance.status,
        NativeObservedSpotLegStatusV1::Completed,
        "{:?}",
        result.rebalance.reason
    );
    let study = result.rebalance.result.as_ref().unwrap();
    for frame in &study.frames {
        domain::portfolio::allocation_result(&frame.input, &frame.allocation).unwrap();
        let target = &frame.allocation.targets.as_ref().unwrap()[0];
        let asset = &frame.input.assets[0];
        let cost = (target.weight.as_decimal() - asset.current_weight.as_decimal()).abs()
            * asset.transaction_cost_rate.as_decimal();
        let cash_after = frame.allocation.cash_weight.as_ref().unwrap().as_decimal() - cost;
        assert!(
            cash_after + frame.input.exposure_tolerance.as_decimal() >= *money("0.8").as_decimal()
        );
    }
    let first = &study.frames[0];
    assert!(
        first.allocation.targets.as_ref().unwrap()[0]
            .weight
            .as_decimal()
            < first.input.assets[0].current_weight.as_decimal()
    );
    let simulation = study.simulation.as_ref().unwrap();
    assert!(simulation.canonical_result["orders"]
        .as_array()
        .unwrap()
        .iter()
        .any(|order| order.to_string().contains("SELL")));
    let commissions = &simulation.canonical_result["accounts"][0]["Cash"]["base"]["commissions"];
    assert!(!commissions.as_object().unwrap().is_empty());
    assert!(commissions.to_string().contains("USDC"));
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
fn infeasible_rebalance_retains_solver_frames_and_the_matching_hold() {
    let (root, comparison, _) = fixture(30, true);
    let (mut request, objects) = study_request(&comparison);
    request.study.mandate.constraints.max_turnover_per_rebalance = money("0");
    let result =
        job::observed_spot::study(root.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    completed(&result.hold);
    assert_eq!(
        result.rebalance.status,
        NativeObservedSpotLegStatusV1::Failed
    );
    assert!(result.rebalance.reason.is_some());
    let diagnostic = result
        .rebalance
        .result
        .as_ref()
        .expect("Keep actual infeasible solver frames");
    assert!(!diagnostic.frames.is_empty());
    assert!(diagnostic.simulation.is_none());
    assert!(diagnostic.frames[0].allocation.targets.is_none());
    assert!(diagnostic.frames[0].input.assets[0]
        .current_weight
        .is_positive());
}

/// Export only original native evidence for cross-language protocol regression.
/// These controlled historical rows do not certify exchange applicability or PIT.
#[test]
#[cfg(feature = "catalog-prepare")]
fn export_observed_native_fixture_when_requested() {
    use sha2::{Digest, Sha256};
    use std::path::Path;
    let Some(directory) = std::env::var_os("QZ_PM_OBSERVED_FIXTURE_DIR") else {
        return;
    };
    let destination = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&destination).unwrap();
    let destination = std::fs::canonicalize(destination).unwrap();
    let (source, comparison, origin) = fixture(30, true);
    fn copy_tree(source: &Path, destination: &Path) {
        std::fs::create_dir_all(destination).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let target = destination.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    let catalog = destination.join(format!(
        "controlled-catalog-{}",
        comparison.dataset_revision_id
    ));
    copy_tree(source.path(), &catalog);
    fn manifest(root: &Path, path: &Path, entries: &mut Vec<serde_json::Value>) {
        let mut files = std::fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        files.sort();
        for file in files {
            if file.is_dir() {
                manifest(root, &file, entries);
            } else {
                let bytes = std::fs::read(&file).unwrap();
                entries.push(serde_json::json!({"path":file.strip_prefix(root).unwrap(),"size":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}));
            }
        }
    }
    let mut entries = Vec::new();
    manifest(&catalog, &catalog, &mut entries);
    let manifest_bytes = serde_json::to_vec(&entries).unwrap();
    let content_ref = format!("sha256:{:x}", Sha256::digest(&manifest_bytes));
    std::fs::write(destination.join("catalog-manifest.json"), manifest_bytes).unwrap();
    let (study, objects) = study_request(&comparison);
    let objects_directory = destination.join(format!("objects-{}", comparison.dataset_revision_id));
    std::fs::create_dir_all(&objects_directory).unwrap();
    for (id, bytes) in &objects {
        std::fs::write(objects_directory.join(id.to_string()), bytes).unwrap();
    }
    let mut infeasible = study.clone();
    infeasible
        .study
        .mandate
        .constraints
        .max_turnover_per_rebalance = money("0");
    let mut selection = comparison.simulation.selection.clone();
    selection.event_start_ns = count(START);
    selection.event_end_ns = comparison.initial_state.snapshot.ts_event;
    selection.decision_cutoff_ns = comparison.initial_state.snapshot.ts_event;
    let valuation_request = NativeObservedSpotValuationRequestV1 {
        schema_version: SchemaV1,
        dataset_revision_id: comparison.dataset_revision_id,
        initial_state: comparison.initial_state.clone(),
        selection: selection.clone(),
        settings: comparison.simulation.settings.clone(),
    };
    let valuation = job::observed_spot::value(&catalog, &valuation_request).unwrap();
    let compared = job::observed_spot::compare(&catalog, &comparison).unwrap();
    completed(&compared.hold);
    completed(&compared.rebalance);
    let studied =
        job::observed_spot::study(&catalog, &study, |id| Ok(objects[&id].clone())).unwrap();
    completed(&studied.hold);
    assert_eq!(
        studied.rebalance.status,
        NativeObservedSpotLegStatusV1::Completed,
        "{:?}",
        studied.rebalance.reason
    );
    let failed =
        job::observed_spot::study(&catalog, &infeasible, |id| Ok(objects[&id].clone())).unwrap();
    completed(&failed.hold);
    assert_eq!(
        failed.rebalance.status,
        NativeObservedSpotLegStatusV1::Failed
    );
    assert!(!failed.rebalance.result.as_ref().unwrap().frames.is_empty());
    for (name, value) in [
        ("origin.json", serde_json::to_value(&origin).unwrap()),
        ("value.json", serde_json::to_value(&valuation).unwrap()),
        ("comparison.json", serde_json::to_value(&compared).unwrap()),
        ("study.json", serde_json::to_value(&studied).unwrap()),
        (
            "infeasible-study.json",
            serde_json::to_value(&failed).unwrap(),
        ),
    ] {
        std::fs::write(destination.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
    }
    let document = serde_json::json!({
        "evidenceKind":"CONTROLLED_NATIVE_TEST_NOT_MARKET_OR_ACCOUNT_QUALIFICATION",
        "catalog":catalog,"nativeDatasetRevisionId":comparison.dataset_revision_id,
        "pmDatasetRevisionId":format!("controlled-pm-dataset-{}",comparison.dataset_revision_id),
        "contentRef":content_ref,"initial_state":comparison.initial_state,
        "settings":comparison.simulation.settings,"simulation":comparison.simulation,
        "valuationSelection":selection,"study":study.study,"infeasibleStudy":infeasible.study,
        "objectsDirectory":objects_directory
    });
    std::fs::write(
        destination.join("source.pending.json"),
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();
    std::fs::rename(
        destination.join("source.pending.json"),
        destination.join("source.json"),
    )
    .unwrap();
}
