//! Native-only forward estimates; controlled source never qualifies live delivery.
use super::*;
use bigdecimal::{BigDecimal, ToPrimitive};

fn forward_fixture() -> (
    tempfile::TempDir,
    NativeObservedSpotBuildRequestV1,
    BTreeMap<Id, Vec<u8>>,
) {
    forward_fixture_with_weight("0.5", "0.5")
}

fn forward_fixture_with_weight(
    weight: &str,
    cash: &str,
) -> (
    tempfile::TempDir,
    NativeObservedSpotBuildRequestV1,
    BTreeMap<Id, Vec<u8>>,
) {
    // A real source price change after the source account snapshot. The latest
    // known balances must be revalued, not relabeled with the old NAV.
    let (root, mut original) = spot::source_fixture_with_prices(30, |index| {
        if index < 5 { "100.00" } else { "200.00" }.into()
    });
    let dataset = Id::new();
    original.selection.event_end_ns = count(START + 5 * MINUTE);
    original.selection.decision_cutoff_ns = count(START + 5 * MINUTE + 500_000_000);
    original.target_points.truncate(1);
    original.target_points[0].targets[0].weight = money(weight);
    original.target_points[0].cash_weight = money(cash);
    let produced =
        job::simulation::simulate_explicit(root.path(), &original, Some(dataset)).unwrap();
    let initial_state = projected_initial_state(&produced);
    let comparison = NativeObservedSpotComparisonRequestV1 {
        schema_version: SchemaV1,
        dataset_revision_id: dataset,
        initial_state: initial_state.clone(),
        simulation: original,
    };
    let (study, objects) = study_request(&comparison);
    let cutoff = count(START + 7 * MINUTE + 500_000_000);
    let mut selection = study.study.source_selection;
    selection.event_end_ns = cutoff;
    selection.decision_cutoff_ns = cutoff;
    (
        root,
        NativeObservedSpotBuildRequestV1 {
            schema_version: SchemaV1,
            dataset_revision_id: dataset,
            initial_state,
            selection,
            mandate: study.study.mandate,
            execution_settings: study.study.execution_settings,
            rolling_liquidity: None,
            assets: study.study.assets,
            members: study.study.members,
        },
        objects,
    )
}

#[test]
fn forward_cash_funding_uses_same_nav_and_real_rounded_buy_and_sell_costs() {
    use contracts::portfolio::{NativeCashFundingPolicyV1, NativeModelRefV1};
    for (weight, cash, floor, buying) in [("0", "1", "0", true), ("0.5", "0.5", "0.6", false)] {
        let (root, mut request, objects) = forward_fixture_with_weight(weight, cash);
        request.mandate.constraints.cash_funding =
            Some(NativeCashFundingPolicyV1::PreserveMinCashAfterLinearTurnoverV1);
        request.mandate.constraints.min_net_exposure = money("0");
        request.mandate.constraints.max_net_exposure = money("1");
        request.mandate.constraints.min_cash_weight = money(floor);
        request.mandate.constraints.max_cash_weight = money("1");
        // Registered analytical fixture method: the original 100 -> 200 price
        // shock has large variance. Fix lambda before the solve, so the positive
        // forecast makes the funding boundary active in both buy/sell cases.
        let NativeModelRefV1::ClarabelQp { parameters, .. } = &mut request.mandate.optimizer else {
            unreachable!()
        };
        parameters.risk_aversion = money("0.000001");
        let result =
            job::observed_spot::build(root.path(), &request, |id| Ok(objects[&id].clone()))
                .unwrap();
        assert!(result.target.is_some(), "{:?}", result.target_block_reason);
        assert_eq!(
            result.build.input.capital_assumption,
            result.valuation.total
        );
        assert_eq!(
            result.build.input.constraints.cash_funding,
            request.mandate.constraints.cash_funding
        );
        let trade = &result.trade_estimates[0];
        assert_eq!(trade.rounded_trade_quantity.is_positive(), buying);
        assert!(trade.modeled_linear_cost.is_positive());
        let quote = result
            .valuation
            .legs
            .iter()
            .find(|leg| leg.currency == "USDC")
            .unwrap()
            .report_value
            .as_decimal();
        let after_fees = quote
            - trade.rounded_trade_notional.as_decimal()
            - trade.modeled_linear_cost.as_decimal();
        assert!(after_fees >= BigDecimal::from(0));
        let allowance =
            request.mandate.exposure_tolerance.as_decimal() * result.valuation.total.as_decimal();
        let floor_notional = money(floor).as_decimal() * result.valuation.total.as_decimal();
        assert!(after_fees >= floor_notional - allowance);
        let input = &result.build.input;
        let current = input.assets[0]
            .current_weight
            .as_decimal()
            .to_f64()
            .unwrap();
        let rate = input.assets[0]
            .transaction_cost_rate
            .as_decimal()
            .to_f64()
            .unwrap();
        let floor: f64 = floor.parse().unwrap();
        let analytical = if buying {
            (1.0 - floor + rate * current) / (1.0 + rate)
        } else {
            (1.0 - floor - rate * current) / (1.0 - rate)
        };
        let target = result.target.as_ref().unwrap().targets[0]
            .weight
            .as_decimal()
            .to_f64()
            .unwrap();
        assert!(
            (target - analytical).abs() < 1e-7,
            "{target} != {analytical}"
        );
    }
}

#[test]
fn forward_zero_inventory_and_target_keep_complete_zero_trade_evidence() {
    let (root, mut request, objects) = forward_fixture_with_weight("0", "1");
    assert!(request
        .initial_state
        .snapshot
        .balances
        .iter()
        .all(|row| row.total.currency == "USDC" || !row.total.amount.is_positive()));
    request.mandate.constraints.min_net_exposure = money("0");
    request.mandate.constraints.max_net_exposure = money("0");
    request.mandate.constraints.min_cash_weight = money("1");
    request.mandate.constraints.max_cash_weight = money("1");
    let result =
        job::observed_spot::build(root.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    assert!(result.target.is_some(), "{:?}", result.target_block_reason);
    assert_eq!(result.trade_estimates.len(), request.assets.len());
    let trade = &result.trade_estimates[0];
    assert_eq!(trade.current_quantity, money("0"));
    assert_eq!(trade.rounded_trade_quantity, money("0"));
    assert_eq!(trade.post_rounding_notional, money("0"));
    assert_eq!(trade.modeled_linear_cost, money("0"));
    let rounded = result.rounded_modeled_comparison.as_ref().unwrap();
    assert_eq!(
        serde_json::to_value(rounded.proposed.as_ref().unwrap()).unwrap(),
        serde_json::to_value(&rounded.hold).unwrap()
    );
}

#[test]
fn forward_revalues_original_inventory_at_cutoff_and_keeps_same_horizon_oracle() {
    let (root, request, objects) = forward_fixture();
    let result =
        job::observed_spot::build(root.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    let target = result
        .target
        .as_ref()
        .unwrap_or_else(|| panic!("{:?}", result.target_block_reason));
    assert_eq!(
        serde_json::to_value(&result.request).unwrap(),
        serde_json::to_value(&request).unwrap()
    );
    assert_eq!(
        result.valuation.asof_ns,
        request.selection.decision_cutoff_ns
    );
    assert!(result.valuation.asof_ns > request.initial_state.snapshot.ts_event);
    assert!(result.valuation.available_ns <= request.selection.decision_cutoff_ns);
    let original = &request.initial_state.snapshot.balances;
    let btc = original
        .iter()
        .find(|b| b.total.currency == "BTC")
        .unwrap()
        .total
        .amount
        .as_decimal();
    let cash = original
        .iter()
        .find(|b| b.total.currency == "USDC")
        .unwrap()
        .total
        .amount
        .as_decimal();
    let expected = btc * BigDecimal::from(200) + cash;
    assert_eq!(result.valuation.total.as_decimal(), &expected);
    assert_ne!(
        result.valuation.total.as_decimal(),
        &(btc * BigDecimal::from(100) + cash)
    );
    assert_eq!(
        result.build.input.capital_assumption,
        result.valuation.total
    );
    let input = &result.build.input;
    let current = input.assets[0]
        .current_weight
        .as_decimal()
        .to_f64()
        .unwrap();
    assert!((current - (btc * BigDecimal::from(200) / expected).to_f64().unwrap()).abs() < 1e-15);
    assert!(current > 0.66);
    let trade = &result.trade_estimates[0];
    assert_eq!(trade.current_quantity.as_decimal(), btc);
    assert_eq!(
        trade.current_notional.as_decimal(),
        &(btc * BigDecimal::from(200))
    );
    assert_eq!(trade.price.as_decimal(), &BigDecimal::from(200));
    assert!(trade.rounded_trade_quantity.as_decimal() < &BigDecimal::from(0));
    assert_eq!(
        trade.post_rounding_quantity.as_decimal(),
        &(btc + trade.rounded_trade_quantity.as_decimal())
    );
    assert_eq!(
        trade.post_rounding_notional.as_decimal(),
        &(trade.post_rounding_quantity.as_decimal() * trade.price.as_decimal())
    );
    assert_eq!(
        trade.rounded_trade_notional.as_decimal(),
        &(trade.rounded_trade_quantity.as_decimal() * trade.price.as_decimal())
    );
    assert!(trade.price_event_ns <= target.asof_ns);
    assert!(trade.price_available_ns <= target.asof_ns);
    let proposed = target.targets[0].weight.as_decimal().to_f64().unwrap();
    assert_eq!(
        trade
            .target_notional
            .as_str()
            .parse::<BigDecimal>()
            .unwrap(),
        target.targets[0].weight.as_decimal() * result.valuation.total.as_decimal()
    );
    assert!((proposed - 0.6).abs() < 1e-8);
    assert_eq!(target.asof_ns, request.selection.decision_cutoff_ns);
    assert_eq!(
        target.valid_until_ns.get(),
        target.asof_ns.get()
            + 1_000_000_000 * u64::from(request.mandate.rebalance_schedule.target_ttl_seconds)
    );
    // Independent scalar oracle for the original one-asset sample variance.
    let returns = &input.return_history.asset_returns[0];
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let covariance =
        returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (returns.len() - 1) as f64;
    let mu: f64 = input
        .forecasts
        .members
        .iter()
        .map(|m| m.forecasts[0] * m.ensemble_weight.as_decimal().to_f64().unwrap())
        .sum();
    let cost = input.assets[0]
        .transaction_cost_rate
        .as_decimal()
        .to_f64()
        .unwrap();
    let lambda = domain::portfolio::optimizer_settings(&input.optimizer)
        .unwrap()
        .risk_aversion
        .as_decimal()
        .to_f64()
        .unwrap();
    for (weight, observed) in [
        (current, &result.modeled_comparison.hold),
        (
            proposed,
            result.modeled_comparison.proposed.as_ref().unwrap(),
        ),
    ] {
        let net = weight * mu - (weight - current).abs() * cost;
        assert!((observed.net_expected_return - net).abs() < 1e-14);
        assert!((observed.variance - weight * weight * covariance).abs() < 1e-14);
        assert!(
            (observed.objective_value - (lambda * weight * weight * covariance - net)).abs()
                < 1e-14
        );
    }
    assert_eq!(result.modeled_comparison.hold.linear_turnover_cost, 0.0);
    let rounded = result.rounded_modeled_comparison.as_ref().unwrap();
    let rounded_weight = (trade.post_rounding_notional.as_decimal()
        / result.valuation.total.as_decimal())
    .to_f64()
    .unwrap();
    let rounded_cost = (trade.modeled_linear_cost.as_decimal()
        / result.valuation.total.as_decimal())
    .to_f64()
    .unwrap();
    let rounded_proposed = rounded.proposed.as_ref().unwrap();
    assert!(
        (rounded_proposed.net_expected_return - (rounded_weight * mu - rounded_cost)).abs() < 1e-14
    );
    assert!(
        (rounded_proposed.variance - rounded_weight * rounded_weight * covariance).abs() < 1e-14
    );
    assert!(
        (rounded_proposed.objective_value
            - (lambda * rounded_weight * rounded_weight * covariance - rounded_weight * mu
                + rounded_cost))
            .abs()
            < 1e-14
    );
    assert!(
        result
            .modeled_comparison
            .proposed
            .as_ref()
            .unwrap()
            .linear_turnover_cost
            > 0.0
    );
    assert!(input
        .return_history
        .available_ns
        .iter()
        .all(|time| *time <= target.asof_ns));
    assert!(input
        .forecasts
        .members
        .iter()
        .all(|m| m.available_ns <= target.asof_ns));
    // Change actual future source prices, not result JSON, and prove the frozen
    // as-of solve cannot consume them.
    let (changed, _) = spot::source_fixture_with_prices(30, |index| {
        if index < 5 {
            "100.00"
        } else if index < 8 {
            "200.00"
        } else {
            "500.00"
        }
        .into()
    });
    let future_changed =
        job::observed_spot::build(changed.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    assert_eq!(
        serde_json::to_value(&future_changed.build.input).unwrap(),
        serde_json::to_value(input).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&future_changed.modeled_comparison).unwrap(),
        serde_json::to_value(&result.modeled_comparison).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&future_changed.target).unwrap(),
        serde_json::to_value(&result.target).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&future_changed.rounded_modeled_comparison).unwrap(),
        serde_json::to_value(&result.rounded_modeled_comparison).unwrap()
    );
    assert!(
        result.build.input.return_history.end_ns.last().unwrap() < &request.selection.event_end_ns
    );
}

#[test]
fn exact_continuous_notional_preserves_a_real_source_product_without_money_rounding() {
    let product = money("1017.2599047").as_decimal() * money("0.6000000000738411").as_decimal();
    assert!(product
        .to_plain_string()
        .parse::<contracts::DecimalValue>()
        .is_err());
    let exact: NativeForwardExactNotionalV1 = product.to_plain_string().try_into().unwrap();
    assert_eq!(exact.as_str(), "610.35594289511559034894317");
    let encoded = serde_json::to_value(&exact).unwrap();
    assert_eq!(
        serde_json::from_value::<NativeForwardExactNotionalV1>(encoded).unwrap(),
        exact
    );
    for unsupported in [
        "1e3".to_owned(),
        "NaN".to_owned(),
        format!("0.{}1", "0".repeat(36)),
        "1".repeat(41),
    ] {
        assert!(
            serde_json::from_value::<NativeForwardExactNotionalV1>(serde_json::json!(unsupported))
                .is_err()
        );
    }
}

#[test]
fn forward_sub_lot_improvement_is_exactly_hold_after_rounding() {
    let (root, mut request, objects) = forward_fixture();
    let initial =
        job::observed_spot::build(root.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    let weight =
        initial.build.input.assets[0].current_weight.as_decimal() - money("0.000001").as_decimal();
    let cash = BigDecimal::from(1) - &weight;
    request.mandate.constraints.min_net_exposure = weight.to_plain_string().parse().unwrap();
    request.mandate.constraints.max_net_exposure =
        request.mandate.constraints.min_net_exposure.clone();
    request.mandate.constraints.min_cash_weight = cash.to_plain_string().parse().unwrap();
    request.mandate.constraints.max_cash_weight =
        request.mandate.constraints.min_cash_weight.clone();
    let result =
        job::observed_spot::build(root.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    assert!(result.target.is_some(), "{:?}", result.target_block_reason);
    let trade = &result.trade_estimates[0];
    assert_eq!(trade.rounded_trade_quantity, money("0"));
    assert_eq!(trade.post_rounding_quantity, trade.current_quantity);
    assert_eq!(trade.post_rounding_notional, trade.current_notional);
    assert!(
        trade.post_rounding_notional.as_decimal()
            > &trade
                .target_notional
                .as_str()
                .parse::<BigDecimal>()
                .unwrap()
    );
    assert_eq!(trade.modeled_linear_cost, money("0"));
    assert!(
        result
            .modeled_comparison
            .proposed
            .as_ref()
            .unwrap()
            .objective_value
            < result.modeled_comparison.hold.objective_value
    );
    let rounded = result.rounded_modeled_comparison.as_ref().unwrap();
    assert_eq!(
        serde_json::to_value(rounded.proposed.as_ref().unwrap()).unwrap(),
        serde_json::to_value(&rounded.hold).unwrap()
    );
}

#[test]
fn forward_refuses_future_stale_unknown_and_foreign_source_without_replay() {
    let (root, original, original_objects) = forward_fixture();
    for mutation in 0..7 {
        let mut request = original.clone();
        let mut objects = original_objects.clone();
        match mutation {
            0 => {
                request.initial_state.available_ns =
                    count(request.selection.decision_cutoff_ns.get() + 1)
            }
            1 => request.mandate.rebalance_schedule.max_input_age_seconds = 1,
            2 => request.initial_state.open_orders_count = 1,
            3 => {
                request.initial_state.snapshot.account_id = "BINANCE-001".into();
                request.initial_state.open_orders_account_id = "BINANCE-001".into();
            }
            4 => request.assets[0].current_weight = money("0.5"),
            5 => {
                request.selection.event_end_ns =
                    count(request.selection.decision_cutoff_ns.get() + 1)
            }
            _ => {
                request
                    .execution_settings
                    .multi_currency_spot_cash
                    .as_mut()
                    .unwrap()
                    .maximum_price_age_ns = count(1);
                objects.insert(
                    request.mandate.constraints.transaction_costs_ref,
                    serde_json::to_vec(&request.execution_settings).unwrap(),
                );
            }
        }
        assert!(
            job::observed_spot::build(root.path(), &request, |id| Ok(objects[&id].clone()))
                .is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn forward_preserves_infeasible_or_untradeable_solver_evidence_without_targets() {
    let (root, mut request, mut objects) = forward_fixture();
    request.mandate.constraints.max_turnover_per_rebalance = money("0");
    let result =
        job::observed_spot::build(root.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    assert!(result.target.is_none());
    assert!(result.build.allocation.targets.is_none());
    assert!(result.target_block_reason.is_some());
    request.mandate.constraints.max_turnover_per_rebalance = money("2");
    // 60% is exactly lot-aligned; 61% has a genuine original-lot residue.
    request.mandate.constraints.min_net_exposure = money("0.61");
    request.mandate.constraints.max_net_exposure = money("0.61");
    request.mandate.constraints.min_cash_weight = money("0.39");
    request.mandate.constraints.max_cash_weight = money("0.39");
    request.mandate.exposure_tolerance = money("0.000000000001");
    request.execution_settings.exposure_tolerance = request.mandate.exposure_tolerance.clone();
    objects.insert(
        request.mandate.constraints.transaction_costs_ref,
        serde_json::to_vec(&request.execution_settings).unwrap(),
    );
    let result =
        job::observed_spot::build(root.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    assert!(result.build.allocation.targets.is_some());
    assert!(result.target.is_none());
    assert_eq!(
        result.target_block_reason.as_deref(),
        Some("SIMULATION_LOT_ROUNDING_EXCEEDS_TOLERANCE")
    );
    // A continuous 100% risky target leaves no cash for a positive frozen
    // transaction cost. Preserve the solve, but never publish an unfunded target.
    request.mandate.constraints.min_net_exposure = money("1");
    request.mandate.constraints.max_net_exposure = money("1");
    request.mandate.constraints.min_cash_weight = money("0");
    request.mandate.constraints.max_cash_weight = money("0");
    let result =
        job::observed_spot::build(root.path(), &request, |id| Ok(objects[&id].clone())).unwrap();
    assert!(result.build.allocation.targets.is_some());
    assert!(result.target.is_none());
    assert_eq!(
        result.target_block_reason.as_deref(),
        Some("FORWARD_LINEAR_COST_FUNDING_SHORTFALL")
    );
}

#[test]
fn forward_actual_cli_preserves_request_and_optional_fixture() {
    let (root, request, objects) = forward_fixture();
    let object_directory = tempfile::tempdir().unwrap();
    for (id, bytes) in &objects {
        std::fs::write(object_directory.path().join(id.to_string()), bytes).unwrap();
    }
    let output = native::command(
        &[
            "build-observed-spot".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
            "--objects".as_ref(),
            object_directory.path().as_os_str(),
        ],
        &request,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: NativeObservedSpotBuildResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result.target.is_some(), "{:?}", result.target_block_reason);
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        serde_json::to_value(&result.request).unwrap()
    );
    if let Some(destination) = std::env::var_os("QZ_PM_FORWARD_FIXTURE_DIR") {
        fn copy(source: &std::path::Path, to: &std::path::Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let target = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &target);
                } else {
                    std::fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        let destination = std::path::PathBuf::from(destination);
        std::fs::create_dir_all(&destination).unwrap();
        let destination = std::fs::canonicalize(destination).unwrap();
        let catalog = destination.join(format!("catalog-{}", request.dataset_revision_id));
        let objects = destination.join(format!("objects-{}", request.dataset_revision_id));
        copy(root.path(), &catalog);
        copy(object_directory.path(), &objects);
        let fixture = serde_json::json!({"evidenceKind":"CONTROLLED_NATIVE_FORWARD_MODEL_TEST_NOT_LIVE_QUALIFICATION","catalog":catalog,"objectsDirectory":objects,"request":request,"result":result});
        std::fs::write(
            destination.join("source.json"),
            serde_json::to_vec(&fixture).unwrap(),
        )
        .unwrap();
    }
}
