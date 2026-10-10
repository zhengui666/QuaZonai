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
    // Target trading authority ends at expiry; existing positions may wait
    // longer for source-observed resolution without authorizing another trade.
    target.valid_until_ns = market::count(expiration);
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
        request.settlements =
            prediction::settlement_groups(expiration, (19 + delay_minutes) * STEP, payouts);
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
            .map(|error| {
                format!(
                    "{error:?}; domain={:?}",
                    error.downcast_ref::<domain::DomainError>()
                )
            })
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

fn early_close(receipt: u64) -> (tempfile::TempDir, NativeSimulationRequestV1) {
    let (_unused, mut request) = simulation("0", None, 0);
    let root = tempfile::tempdir().unwrap();
    prediction::write_catalog(root.path(), 17, "0", 18 * STEP, false);
    request.target_points[0].asof_ns = market::count(5 * STEP + 1);
    request.settlements = prediction::settlement_groups(8 * STEP, receipt, ["1.0000", "0.0000"]);
    prediction::settle(root.path(), 8 * STEP, receipt, ["1.0000", "0.0000"]);
    (root, request)
}

#[test]
fn early_original_close_redeems_both_positions_at_receipt_without_changing_expiry() {
    let (root, request) = early_close(9 * STEP);
    let result = job::simulation::simulate(root.path(), &request).unwrap();
    for key in ["positions.open", "orders.open", "orders.inflight"] {
        assert_eq!(result.summary[key], "0");
    }
    let fills = result.canonical_result["fills"].as_array().unwrap();
    let orders = result.canonical_result["orders"].as_array().unwrap();
    let settlement_orders = orders
        .iter()
        .map(|order| {
            order
                .as_object()
                .unwrap()
                .values()
                .find_map(|payload| payload.get("core"))
                .unwrap()
        })
        .filter(|core| {
            core["is_reduce_only"].as_bool().unwrap()
                && core["tags"].as_array().is_some_and(|tags| {
                    tags.iter().any(|tag| tag == "EXPIRATION_POLYMARKET_CLOSE")
                })
        })
        .collect::<Vec<_>>();
    let redemptions = fills
        .iter()
        .filter_map(|row| {
            settlement_orders
                .iter()
                .find(|core| core["client_order_id"] == row["client_order_id"])
                .map(|core| (row, *core))
        })
        .collect::<Vec<_>>();
    assert_eq!(redemptions.len(), 2, "orders={orders:?}; fills={fills:?}");
    for (id, payout) in IDS.iter().zip(["1.0000", "0.0000"]) {
        let (row, core) = redemptions
            .iter()
            .find(|(row, _)| row["event"]["Filled"]["instrument_id"] == *id)
            .copied()
            .unwrap_or_else(|| panic!("instrument={id}; redemptions={redemptions:?}"));
        let fill = &row["event"]["Filled"];
        let context = format!(
            "order_id={} ordinal={} instrument={} qty={} px={} commission={} ts_event={} tags={} reduce_only={}",
            core["client_order_id"],
            row["order_event_ordinal"],
            fill["instrument_id"],
            fill["last_qty"],
            fill["last_px"],
            fill["commission"],
            fill["ts_event"],
            core["tags"],
            core["is_reduce_only"]
        );
        eprintln!("canonical early redemption: {context}");
        assert_eq!(fill["client_order_id"], core["client_order_id"], "{context}");
        assert_eq!(fill["instrument_id"], core["instrument_id"], "{context}");
        assert_eq!(fill["last_px"], payout, "{context}");
        assert_eq!(fill["ts_event"], (9 * STEP).to_string(), "{context}");
        let fee: nautilus_model::types::Money =
            fill["commission"].as_str().unwrap().parse().unwrap();
        assert_eq!(fee.as_decimal(), rust_decimal::Decimal::ZERO, "{context}");
    }
    let data = job::catalog::load_catalog(root.path(), &request.selection).unwrap();
    for series in data.series {
        let nautilus_model::instruments::InstrumentAny::BinaryOption(binary) = series.instrument
        else {
            panic!("binary required");
        };
        assert_eq!(binary.expiration_ns.as_u64(), 18 * STEP);
    }
}

#[test]
fn market_order_after_original_close_receipt_before_expiry_has_native_closed_reason() {
    let (root, mut request) = early_close(9 * STEP);
    let mut after_close = request.target_points[0].clone();
    after_close.asof_ns = market::count(10 * STEP + 1);
    request.target_points.push(after_close);
    let error = job::simulation::simulate(root.path(), &request).unwrap_err();
    let chain = format!("{error:#}");
    assert_eq!(error.to_string(), "NATIVE_TARGET_REPLAY_FAILED");
    assert!(
        chain.contains("NATIVE_ORDER_REJECTED") && chain.contains("CLOSED"),
        "{chain}"
    );
}

#[test]
fn own_and_cross_member_close_bar_receipt_ties_fail_both_scientific_entrypoints() {
    use nautilus_model::{
        data::{Bar, BarType, InstrumentClose},
        enums::InstrumentCloseType,
        types::{Price, Quantity},
    };
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;
    for cross_member in [false, true] {
        let (_unused, mut request) = simulation("0", None, 0);
        let root = tempfile::tempdir().unwrap();
        let catalog =
            ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, None, None, None)
                .unwrap();
        catalog
            .write_instruments(prediction::instruments("0", 18 * STEP))
            .unwrap();
        for (index, id) in IDS.iter().enumerate() {
            let kind: BarType = format!("{id}-1-MINUTE-LAST-EXTERNAL").parse().unwrap();
            let offset = if index == 0 { 1 } else { 3 };
            let bars = (1..=17)
                .map(|minute| {
                    Bar::new_checked(
                        kind,
                        Price::from("0.4000"),
                        Price::from("0.4000"),
                        Price::from("0.4000"),
                        Price::from("0.4000"),
                        Quantity::from("10000000.000000"),
                        (minute * STEP).into(),
                        (minute * STEP + offset).into(),
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            catalog.write_to_parquet(&bars, None, None, None).unwrap();
        }
        request.target_points[0].asof_ns = market::count(5 * STEP + 3);
        // Close for member zero ties only its own BAR, or only member one's BAR.
        let receipt = 9 * STEP + if cross_member { 3 } else { 1 };
        request.settlements =
            prediction::settlement_groups(8 * STEP, receipt, ["1.0000", "0.0000"]);
        request.settlements[0].outcomes[1].ts_init = market::count(9 * STEP + 2);
        for (id, px, at) in [
            (IDS[0], "1.0000", receipt),
            (IDS[1], "0.0000", 9 * STEP + 2),
        ] {
            let close = InstrumentClose::new(
                id.parse().unwrap(),
                Price::from(px),
                InstrumentCloseType::ContractExpired,
                (8 * STEP).into(),
                at.into(),
            );
            catalog
                .write_to_parquet(&[close], None, None, None)
                .unwrap();
        }
        let selected = contracts::execution::NativeDatasetSelectionV1 {
            dataset_revision_id: contracts::Id::new(),
            selection: request.selection.clone(),
            settlements: request.settlements.clone(),
        };
        let error = job::simulation::simulate(root.path(), &request).unwrap_err();
        assert!(
            format!("{error:#}").contains("POLYMARKET_CLOSE_BAR_RECEIPT_AMBIGUOUS"),
            "{error:#}"
        );
        let error = job::catalog::measure_catalog(root.path(), &selected, false)
            .err()
            .expect("receipt ambiguity must fail measurement");
        assert!(
            format!("{error:#}").contains("POLYMARKET_CLOSE_BAR_RECEIPT_AMBIGUOUS"),
            "{error:#}"
        );
    }
}

#[test]
fn unselected_sibling_close_receipt_tie_rejects_selected_bar_at_both_entrypoints() {
    use nautilus_model::{data::InstrumentClose, enums::InstrumentCloseType, types::Price};
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;

    let (root, mut request) = simulation("0", None, 0);
    request.selection.bar_types.truncate(1);
    request.settings.fee_rates.truncate(1);
    request.target_points[0].targets.truncate(1);
    request.target_points[0].cash_weight = "0.8".parse().unwrap();
    let selected_receipt = 9 * STEP + 2;
    let sibling_receipt = 9 * STEP + 1;
    request.settlements =
        prediction::settlement_groups(8 * STEP, selected_receipt, ["1.0000", "0.0000"]);
    request.settlements[0].outcomes[1].ts_init = market::count(sibling_receipt);
    let catalog =
        ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, None, None, None)
            .unwrap();
    // Only A is selected; B's verified original close ties A's BAR receipt.
    for (id, price, receipt) in [
        (IDS[0], "1.0000", selected_receipt),
        (IDS[1], "0.0000", sibling_receipt),
    ] {
        let close = InstrumentClose::new(
            id.parse().unwrap(),
            Price::from(price),
            InstrumentCloseType::ContractExpired,
            (8 * STEP).into(),
            receipt.into(),
        );
        catalog
            .write_to_parquet(&[close], None, None, None)
            .unwrap();
    }
    let selected = contracts::execution::NativeDatasetSelectionV1 {
        dataset_revision_id: contracts::Id::new(),
        selection: request.selection.clone(),
        settlements: request.settlements.clone(),
    };
    let replay_error = job::simulation::simulate(root.path(), &request)
        .err()
        .map(|error| format!("{error:#}"));
    let measurement_error = job::catalog::measure_catalog(root.path(), &selected, false)
        .err()
        .map(|error| format!("{error:#}"));
    eprintln!("single A sibling receipt: simulate={replay_error:?}; measure_catalog={measurement_error:?}");
    let expected = Some("POLYMARKET_CLOSE_BAR_RECEIPT_AMBIGUOUS");
    assert_eq!(
        (replay_error.as_deref(), measurement_error.as_deref()),
        (expected, expected)
    );
}

#[test]
fn equal_close_event_with_distinct_bar_receipt_is_not_receipt_ambiguity() {
    let (root, request) = early_close(8 * STEP + 2);
    let selected = contracts::execution::NativeDatasetSelectionV1 {
        dataset_revision_id: contracts::Id::new(),
        selection: request.selection.clone(),
        settlements: request.settlements.clone(),
    };
    assert!(job::catalog::measure_catalog(root.path(), &selected, false).is_ok());
    let result = job::simulation::simulate(root.path(), &request).unwrap();
    assert_eq!(result.summary["positions.open"], "0");
}

#[test]
fn native_bar_remainder_cannot_trade_above_original_bounds_but_redemption_can() {
    use nautilus_model::{
        data::{Bar, BarType},
        instruments::InstrumentAny,
        types::{Price, Quantity},
    };
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;

    for (volume, reject_remainder) in [("1000000.000000", false), ("8.000000", true)] {
        let (_unused, request) = simulation("0", Some(["1.0000", "0.0000"]), 0);
        let root = tempfile::tempdir().unwrap();
        let catalog =
            ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, None, None, None)
                .unwrap();
        for (index, instrument) in prediction::instruments("0", 18 * STEP)
            .into_iter()
            .enumerate()
        {
            let mut value = serde_json::to_value(instrument).unwrap();
            value["BinaryOption"]["price_increment"] = "0.0100".into();
            value["BinaryOption"]["min_price"] = "0.0100".into();
            value["BinaryOption"]["max_price"] = "0.9900".into();
            let instrument: InstrumentAny = serde_json::from_value(value).unwrap();
            catalog.write_instruments(vec![instrument]).unwrap();
            let kind: BarType = request.selection.bar_types[index].parse().unwrap();
            let price = Price::from(if index == 0 { "0.9900" } else { "0.5000" });
            let volume = Quantity::from(if index == 0 { volume } else { "1000000.000000" });
            let bars = (1..=17_u64)
                .map(|minute| {
                    Bar::new_checked(
                        kind,
                        price,
                        price,
                        price,
                        price,
                        volume,
                        (minute * STEP).into(),
                        (minute * STEP + 1).into(),
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            catalog.write_to_parquet(&bars, None, None, None).unwrap();
        }
        prediction::settle(root.path(), 18 * STEP, 19 * STEP, ["1.0000", "0.0000"]);
        let result = job::simulation::simulate(root.path(), &request);
        if reject_remainder {
            // Eight shares become four two-share native BAR ticks. The first
            // partial fill costs 1.98 pUSD; the larger remaining buy is natively
            // extrapolated from .9900 to 1.0000, exceeding the original maximum.
            match result {
                Ok(result) => panic!(
                    "native out-of-bounds ordinary fill was accepted: {}",
                    result.canonical_result["fills"]
                ),
                Err(error) => assert!(
                    error.to_string().contains("NATIVE_TARGET_REPLAY_FAILED"),
                    "{error:#}"
                ),
            }
        } else {
            let result = result.unwrap();
            assert_eq!(result.summary["positions.open"], "0");
            let closes = result.canonical_result["fills"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| &value["event"]["Filled"])
                .filter(|fill| fill["order_side"] == "SELL")
                .collect::<Vec<_>>();
            assert_eq!(closes.len(), 2);
            for (id, payout) in IDS.iter().zip(["1.0000", "0.0000"]) {
                let close = closes
                    .iter()
                    .find(|fill| fill["instrument_id"] == *id)
                    .unwrap();
                assert_eq!(close["last_px"], payout);
                assert_eq!(close["ts_event"], (19 * STEP).to_string());
            }
        }
    }
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
    domain::execution::portfolio_study_cutoffs(&request).expect("synthetic study input contract");
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
        .map(|error| {
            format!(
                "{error:?}; domain={:?}",
                error.downcast_ref::<domain::DomainError>()
            )
        });
        panic!(
            "{}; fixture diagnostic: {detail:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let result: NativePortfolioStudyResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    domain::execution::check_portfolio_study(&request, &result).unwrap();
    assert_eq!(result.frames.len(), 3);
    let simulation_request = result.simulation_request.as_ref().unwrap();
    let simulation = result.simulation.as_ref().unwrap();
    let (metrics, _) = domain::execution::portfolio_simulation_metrics(
        contracts::Id::new(),
        contracts::Id::new(),
        simulation_request,
        simulation,
    )
    .unwrap();
    assert_eq!(metrics[0].annualization_factor, None);
    assert_eq!(metrics[1].annualization_factor, Some(365.0));
    assert_eq!(metrics[2].annualization_factor, Some(365.0));
    let original = simulation
        .statistics
        .iter()
        .find(|s| {
            s.group == NativeStatisticGroup::Returns
                && s.native_key == "Returns Volatility (365 days)"
        })
        .unwrap();
    assert_eq!(metrics[1].value, original.value);
    assert!(!simulation
        .statistics
        .iter()
        .any(|s| s.group == NativeStatisticGroup::Returns && s.native_key.ends_with("(252 days)")));
    assert!(result.consumed_fuel.unwrap().get() > 0);
}

#[test]
fn unregistered_changed_and_missing_settlements_cannot_change_a_frozen_replay() {
    let (catalog, request) = simulation("0", Some(["1.0000", "0.0000"]), 0);
    for mutation in 0..4 {
        let mut changed = request.clone();
        match mutation {
            0 => changed.settlements.clear(),
            1 => {
                for outcome in &mut changed.settlements[0].outcomes {
                    outcome.close_price = "0.5".parse().unwrap();
                }
            }
            2 => {
                for outcome in &mut changed.settlements[0].outcomes {
                    outcome.ts_init = market::count(outcome.ts_init.get() - 1);
                }
            }
            _ => {
                for outcome in &mut changed.settlements[0].outcomes {
                    outcome.ts_event = market::count(outcome.ts_event.get() - 1);
                }
            }
        }
        let error = simulate(catalog.path(), &changed).unwrap_err();
        assert!(
            error.contains("POLYMARKET_SETTLEMENT_SOURCE_MISMATCH"),
            "{mutation}: {error}"
        );
    }
    let (missing, mut frozen) = simulation("0", None, 0);
    frozen.settlements = request.settlements;
    assert!(simulate(missing.path(), &frozen)
        .unwrap_err()
        .contains("POLYMARKET_SETTLEMENT_SOURCE_MISMATCH"));
}

#[test]
fn native_settlement_rejects_incoherent_complete_condition_payouts() {
    for payouts in [
        ["1.0000", "1.0000"],
        ["0.8000", "0.8000"],
        ["0.0000", "0.0000"],
    ] {
        let (catalog, request) = simulation("0", Some(payouts), 0);
        let error = simulate(catalog.path(), &request).unwrap_err();
        assert!(
            error.contains("polymarket_incoherent_payout_vector"),
            "{error}"
        );
    }
}

#[test]
fn one_traded_outcome_still_requires_the_complete_source_payout_vector() {
    let (catalog, mut request) = simulation("0", Some(["1.0000", "0.0000"]), 0);
    request.selection.bar_types.truncate(1);
    request.settings.fee_rates.truncate(1);
    request.target_points[0].targets.truncate(1);
    request.target_points[0].cash_weight = "0.8".parse().unwrap();
    let result = simulate(catalog.path(), &request).unwrap();
    assert!((pnl(&result) - 300.0).abs() < 0.0001);
    request.settlements[0].outcomes[1].close_price = "0.5".parse().unwrap();
    assert!(simulate(catalog.path(), &request).is_err());
    request.settlements[0].outcomes.truncate(1);
    assert!(simulate(catalog.path(), &request).is_err());
}

#[test]
fn close_on_the_exclusive_holding_boundary_is_not_early_cash() {
    let (catalog, mut request) = simulation("0", Some(["1.0000", "0.0000"]), 0);
    request.selection.event_end_ns = market::count(19 * STEP);
    request.selection.decision_cutoff_ns = request.selection.event_end_ns;
    assert!(simulate(catalog.path(), &request)
        .unwrap_err()
        .contains("POLYMARKET_PENDING_RESOLUTION"));
}

#[test]
fn adding_a_close_to_an_existing_bar_catalog_does_not_authorize_it() {
    use nautilus_model::{data::InstrumentClose, enums::InstrumentCloseType, types::Price};
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;
    let (root, request) = simulation("0", Some(["1.0000", "0.0000"]), 0);
    let catalog =
        ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, None, None, None)
            .unwrap();
    let extra = InstrumentClose::new(
        IDS[0].parse().unwrap(),
        Price::from("0.5000"),
        InstrumentCloseType::ContractExpired,
        (18 * STEP).into(),
        (20 * STEP).into(),
    );
    catalog
        .write_to_parquet(&[extra], None, None, None)
        .unwrap();
    assert!(simulate(root.path(), &request).is_err());
}

#[test]
fn portfolio_build_rejects_expired_and_cross_expiry_targets_before_publishing_weights() {
    let (_unused, original, model) = market::portfolio();
    let cutoff = original.selection.decision_cutoff_ns.get();
    let until =
        cutoff + u64::from(original.mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000;
    for (expiration, accepted) in [
        (cutoff - 1, false),
        (cutoff, false),
        (until - 1, false),
        (until, true),
        (until + 1, true),
    ] {
        let mut request = original.clone();
        let catalog = tempfile::tempdir().unwrap();
        prediction::write_catalog(catalog.path(), 20, "0", expiration, true);
        prediction::settings(&mut request.execution_settings, "0", expiration);
        request.mandate.base_currency = "pUSD".into();
        request.current_weights.base_currency = "pUSD".into();
        request.selection.bar_types = IDS
            .iter()
            .map(|id| format!("{id}-1-MINUTE-LAST-EXTERNAL"))
            .collect();
        for ((asset, weight), id) in request
            .assets
            .iter_mut()
            .zip(&mut request.current_weights.weights)
            .zip(IDS)
        {
            asset.instrument_id = id.into();
            asset.currency = "pUSD".into();
            weight.instrument_id = id.into();
            weight.currency = "pUSD".into();
        }
        domain::execution::portfolio_build_request(&request).unwrap_or_else(|error| {
            panic!("expiry={expiration}; fixture contract: {error:?}; request={request:?}")
        });
        let result = job::portfolio::build(catalog.path(), &request, |id| {
            if id == request.mandate.constraints.transaction_costs_ref {
                Ok(serde_json::to_vec(&request.execution_settings)?)
            } else if id == request.current_weights_artifact_id {
                Ok(serde_json::to_vec(&request.current_weights)?)
            } else {
                Ok(model.clone())
            }
        });
        if accepted {
            assert!(result.is_ok(), "expiry={expiration}: {result:?}");
        } else {
            let failure = result.unwrap_err();
            let error = format!(
                "{failure:?}; domain={:?}",
                failure.downcast_ref::<domain::DomainError>()
            );
            assert!(
                error.contains("polymarket_target_lifetime"),
                "expiry={expiration}: {error}"
            );
        }
    }
}
