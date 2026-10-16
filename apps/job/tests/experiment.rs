//! Real native subprocess acceptance over synthetic second-frequency Parquet.
#[path = "../../../tests/support/execution_models.rs"]
mod execution_models;
#[path = "support/experiment.rs"]
mod fixture;
#[path = "support/command.rs"]
mod native;
#[path = "support/portfolio_returns.rs"]
mod portfolio_returns;
use contracts::{science::*, Id};
use std::{ffi::OsString, fs, path::Path};

fn evaluate(
    root: &Path,
    request: &NativeExperimentEvaluationRequestV1,
    parts: &[FeatureObservationsV1],
    wasm: &[u8],
) -> Result<NativeExperimentEvaluationResultV1, String> {
    let input = tempfile::tempdir().unwrap();
    let model = input.path().join("model.wasm");
    fs::write(&model, wasm).unwrap();
    let dataset_revision_id = Id::new();
    let model_artifact_id = Id::new();
    let mut args: Vec<OsString> = vec![
        "evaluate-experiment".into(),
        "--catalog".into(),
        root.into(),
        "--model".into(),
        model.into(),
        "--dataset-revision-id".into(),
        dataset_revision_id.to_string().into(),
        "--model-artifact-id".into(),
        model_artifact_id.to_string().into(),
    ];
    let mut ids = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let path = input.path().join(format!("features-{index}.json"));
        fs::write(&path, serde_json::to_vec(part).unwrap()).unwrap();
        let id = Id::new();
        ids.push(id);
        args.extend([
            "--features".into(),
            path.into(),
            "--feature-artifact-id".into(),
            id.to_string().into(),
        ]);
    }
    let result = native::command(
        &args.iter().map(OsString::as_os_str).collect::<Vec<_>>(),
        request,
    );
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).into_owned());
    }
    let result = serde_json::from_slice(&result.stdout).map_err(|e| e.to_string())?;
    domain::execution::check_experiment_evaluation(
        request,
        dataset_revision_id,
        model_artifact_id,
        &ids,
        &result,
    )
    .map_err(|e| e.to_string())?;
    Ok(result)
}

#[test]
fn custom_data_asof_preserves_ties_zero_expiry_lag_and_observed_bar_labels() {
    let (root, request, parts) = fixture::fixture(false);
    let result = evaluate(root.path(), &request, &parts, &fixture::feature_policy()).unwrap();
    let first = &result.folds[0].decisions;
    assert_eq!(
        first.iter().map(|d| d.ordinal).collect::<Vec<_>>(),
        [11, 12, 13]
    );
    assert_eq!(first[0].features[0].value, Some(0.25));
    assert_eq!(first[0].features[0].sequence, Some(fixture::count(2)));
    assert_eq!(first[1].features[0].value, Some(0.0));
    assert_eq!(first[1].features[0].missing_reason, None);
    assert_eq!(first[2].features[0].value, None);
    assert_eq!(
        first[2].features[0].missing_reason,
        Some(FeatureMissingReasonV1::Expired)
    );
    assert_eq!(first[0].features[1].observed_available_ns, None);
    assert_eq!(
        first[0].features[1].effective_available_ns,
        Some(fixture::count(12 * fixture::SECOND))
    );
    assert_eq!(
        first[0].features[2].missing_reason,
        Some(FeatureMissingReasonV1::NotYetAvailable)
    );
    assert_eq!(first[0].features[2].event_ns, None);
    assert!(result
        .folds
        .iter()
        .flat_map(|f| &f.decisions)
        .any(|d| d.features[0].missing_reason == Some(FeatureMissingReasonV1::SourceMissing)));
    for fold in &result.folds {
        let start = fold.decisions[0].event_ns;
        assert_eq!(fold.simulation_request.selection.event_start_ns, start);
        assert!(fold.training_end_available_ns < fold.decisions[0].decision_ns);
        portfolio_returns::assert_daily_returns_unavailable(&fold.simulation);
        for decision in &fold.decisions {
            assert_eq!(
                decision.label_end_ns,
                Some(fixture::count(fixture::event(
                    decision.ordinal as usize + 2
                )))
            );
            assert_eq!(
                decision.label_available_ns,
                Some(fixture::count(
                    fixture::event(decision.ordinal as usize + 2) + 1
                ))
            );
        }
    }
    assert!(result
        .folds
        .iter()
        .flat_map(|f| &f.decisions)
        .any(|d| d.label_end_ns.unwrap().get() - d.event_ns.get() > 2 * fixture::SECOND));
}

#[test]
fn future_source_and_future_label_changes_cannot_rewrite_earlier_decisions() {
    let (root, request, parts) = fixture::fixture(false);
    let original = evaluate(root.path(), &request, &parts, &fixture::feature_policy()).unwrap();
    let (changed_root, changed_request, mut changed_parts) = fixture::fixture(true);
    changed_parts[0]
        .observations
        .iter_mut()
        .filter(|row| row.event_ns.get() >= 30 * fixture::SECOND)
        .for_each(|row| row.value = Some(0.8));
    let changed = evaluate(
        changed_root.path(),
        &changed_request,
        &changed_parts,
        &fixture::feature_policy(),
    )
    .unwrap();
    for (before, after) in original
        .folds
        .iter()
        .flat_map(|f| &f.decisions)
        .zip(changed.folds.iter().flat_map(|f| &f.decisions))
        .take_while(|(before, _)| before.ordinal < 20)
    {
        assert_eq!(before.target_weight, after.target_weight);
        assert_eq!(before.features, after.features);
    }
    assert_ne!(
        original
            .folds
            .iter()
            .flat_map(|f| &f.decisions)
            .map(|d| d.label_return)
            .collect::<Vec<_>>(),
        changed
            .folds
            .iter()
            .flat_map(|f| &f.decisions)
            .map(|d| d.label_return)
            .collect::<Vec<_>>()
    );
}

#[test]
fn independent_models_and_native_accounts_reset_and_targets_match_original_simulator() {
    let (root, request, parts) = fixture::fixture(false);
    let model = fixture::module(
        "(global $n (mut f64) (f64.const 0))",
        "",
        "global.get $n f64.const 0.125 f64.add global.set $n global.get $n",
    );
    let result = evaluate(root.path(), &request, &parts, &model).unwrap();
    assert!(result.folds.len() > 1);
    for fold in &result.folds {
        assert_eq!(
            fold.decisions
                .iter()
                .map(|d| d.target_weight.clone())
                .collect::<Vec<_>>(),
            ["0.125", "0.25", "0.375"].map(|v| v.parse().unwrap())
        );
        assert_eq!(
            fold.simulation_request.settings.starting_capital,
            request.settings.starting_capital
        );
        let direct = native::command(
            &[
                "simulate".as_ref(),
                "--catalog".as_ref(),
                root.path().as_os_str(),
            ],
            &fold.simulation_request,
        );
        assert!(direct.status.success());
        let direct: NativeSimulationResultV1 = serde_json::from_slice(&direct.stdout).unwrap();
        assert_eq!(direct.orders, fold.simulation.orders);
        assert!(direct.orders.get() > 0);
        assert_eq!(direct.positions, fold.simulation.positions);
        for field in [
            "accounts",
            "orders",
            "fills",
            "positions",
            "portfolio_snapshots",
        ] {
            assert!(
                direct.canonical_result.get(field).is_some(),
                "native field {field} missing"
            );
            assert_eq!(
                direct.canonical_result[field], fold.simulation.canonical_result[field],
                "native {field} differs"
            );
        }
    }
}

#[test]
fn feature_shards_order_cannot_change_equal_time_decisions() {
    let (root, request, parts) = fixture::fixture(false);
    let baseline = evaluate(root.path(), &request, &parts, &fixture::feature_policy()).unwrap();
    let mut first = parts[0].clone();
    let mut second = first.clone();
    second.observations = first.observations.split_off(3);
    first.observations.reverse();
    second.observations.reverse();
    let split = evaluate(
        root.path(),
        &request,
        &[second, first],
        &fixture::feature_policy(),
    )
    .unwrap();
    for (a, b) in baseline
        .folds
        .iter()
        .flat_map(|f| &f.decisions)
        .zip(split.folds.iter().flat_map(|f| &f.decisions))
    {
        assert_eq!(a.features, b.features);
        assert_eq!(a.target_weight, b.target_weight);
    }
}

#[test]
fn callback_traps_invalid_weights_and_wrong_abis_fail_the_native_job() {
    let (root, request, parts) = fixture::fixture(false);
    for wasm in [
        fixture::module("", "unreachable", "f64.const 0.25"),
        fixture::module("", "", "unreachable f64.const 0.25"),
        fixture::module("", "", "f64.const 1.1"),
        fixture::module("", "", "f64.const nan"),
        wat::parse_str("(module (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 0.25))").unwrap(),
    ] {
        assert!(evaluate(root.path(), &request, &parts, &wasm).is_err());
    }
}

fn native_variant(value: &serde_json::Value) -> &serde_json::Value {
    let object = value.as_object().expect("native enum object");
    assert_eq!(object.len(), 1, "one native enum variant");
    object.values().next().unwrap()
}

fn native_clock(value: &serde_json::Value) -> u64 {
    value
        .as_str()
        .expect("canonical native timestamp")
        .parse()
        .unwrap()
}

fn usdt_money(value: &serde_json::Value) -> nautilus_model::types::Money {
    let money: nautilus_model::types::Money = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(money.currency, nautilus_model::types::Currency::USDT());
    assert_ne!(money.currency, nautilus_model::types::Currency::USD());
    assert_eq!(money.currency.precision, 8);
    money
}

fn assert_native_usdt_account_fees_and_equity(canonical: &serde_json::Value) {
    let accounts = canonical["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 1);
    let base = &accounts[0]["Margin"]["base"];
    assert_eq!(base["base_currency"], "USDT");
    let starting = base["balances_starting"].as_object().unwrap();
    assert_eq!(starting.len(), 1);
    assert_eq!(
        usdt_money(&starting["USDT"]).as_decimal(),
        rust_decimal::Decimal::from(1_000_000)
    );
    let balances = base["balances"].as_object().unwrap();
    assert_eq!(balances.len(), 1);
    assert!(usdt_money(&balances["USDT"]["total"]).as_decimal() > rust_decimal::Decimal::ZERO);
    let fills = canonical["fills"].as_array().unwrap();
    assert!(!fills.is_empty(), "actual BTC/USDT native fills required");
    let mut total_commission = rust_decimal::Decimal::ZERO;
    for fill in fills {
        let event = native_variant(&fill["event"]);
        assert_eq!(event["instrument_id"], "BTC/USDT.SIM");
        assert_eq!(event["currency"], "USDT");
        let commission = usdt_money(&event["commission"]).as_decimal();
        assert!(commission > rust_decimal::Decimal::ZERO);
        total_commission += commission;
    }
    let commissions = base["commissions"].as_object().unwrap();
    assert_eq!(commissions.len(), 1);
    assert_eq!(
        usdt_money(&commissions["USDT"]).as_decimal(),
        total_commission
    );
    let snapshots = canonical["portfolio_snapshots"].as_array().unwrap();
    assert!(
        !snapshots.is_empty(),
        "actual native equity snapshots required"
    );
    for snapshot in snapshots {
        assert_eq!(snapshot["base_currency"], "USDT");
        assert_eq!(snapshot["account_type"], "MARGIN");
        let equity = snapshot["total_equity"].as_array().unwrap();
        assert_eq!(equity.len(), 1);
        assert!(usdt_money(&equity[0]).as_decimal() > rust_decimal::Decimal::ZERO);
        if !snapshot["base_currency_equity"].is_null() {
            usdt_money(&snapshot["base_currency_equity"]);
        }
    }
}

#[test]
fn delayed_target_receipts_keep_event_rows_and_native_fill_clocks_separate() {
    // Receipt lag exceeds the two-observed-bar label horizon. All three first
    // fold decisions occur after that fold's exclusive event-time end.
    let lag = 10 * fixture::SECOND;
    let model = fixture::module(
        "(global $n (mut f64) (f64.const 0))",
        "",
        "global.get $n f64.const 0.125 f64.add global.set $n global.get $n",
    );
    for market in [
        fixture::FixtureMarket::Equity,
        fixture::FixtureMarket::CryptoCurrencyPair,
        fixture::FixtureMarket::CryptoUsdtCurrencyPair,
    ] {
        let (root, request, parts) = fixture::market_fixture(false, market, lag);
        let result = evaluate(root.path(), &request, &parts, &model).unwrap();
        assert_eq!(result.source_row_count.get(), 40);
        assert!(result.folds[0]
            .decisions
            .iter()
            .all(|decision| decision.decision_ns
                > result.folds[0].simulation_request.selection.event_end_ns));
        let (_, latency) = domain::portfolio::simulation_models(&request.settings).unwrap();
        let latency_ns = latency.base_latency_ns.get() + latency.insert_latency_ns.get();
        for fold in &result.folds {
            let selected = &fold.simulation_request.selection;
            let last = fold.decisions.last().unwrap();
            assert_eq!(
                selected.event_end_ns.get(),
                last.label_end_ns.unwrap().get() + 1
            );
            assert_eq!(
                selected.decision_cutoff_ns,
                last.label_available_ns.unwrap()
            );
            let selected_market = job::catalog::load_catalog(root.path(), selected).unwrap();
            assert_eq!(selected_market.rows, fold.decisions.len() + 2);
            assert!(selected_market.series[0]
                .bars
                .iter()
                .all(|bar| bar.ts_init.as_u64() - bar.ts_event.as_u64() == lag));
            match market {
                fixture::FixtureMarket::Equity => {
                    assert_eq!(request.settings.account_kind, NativeAccountKind::Cash)
                }
                fixture::FixtureMarket::CryptoCurrencyPair
                | fixture::FixtureMarket::CryptoUsdtCurrencyPair => {
                    assert_eq!(request.settings.account_kind, NativeAccountKind::Margin);
                    let nautilus_model::instruments::InstrumentAny::CurrencyPair(pair) =
                        &selected_market.series[0].instrument
                    else {
                        panic!("native CurrencyPair required");
                    };
                    let quote = match market {
                        fixture::FixtureMarket::CryptoUsdtCurrencyPair => "USDT",
                        _ => "USD",
                    };
                    assert_eq!(request.instrument_id, format!("BTC/{quote}.SIM"));
                    assert_eq!(request.settings.base_currency, quote);
                    assert_eq!(pair.quote_currency.to_string(), quote);
                }
            }
            let canonical = &fold.simulation.canonical_result;
            if matches!(market, fixture::FixtureMarket::CryptoUsdtCurrencyPair) {
                assert_native_usdt_account_fees_and_equity(canonical);
            }
            let end = native_clock(&canonical["run"]["backtest_end_ns"]);
            assert!(end <= selected.decision_cutoff_ns.get());
            assert!(end <= request.selection.decision_cutoff_ns.get());
            let initialized = canonical["orders"]
                .as_array()
                .unwrap()
                .iter()
                .map(|order| {
                    let core = &native_variant(order)["core"];
                    let event = core["events"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find_map(|event| event.get("Initialized"))
                        .expect("native order initialization");
                    let submitted = native_clock(&event["ts_init"]);
                    assert!(fold
                        .decisions
                        .iter()
                        .any(|decision| decision.decision_ns.get() == submitted));
                    (event["client_order_id"].as_str().unwrap(), submitted)
                })
                .collect::<std::collections::BTreeMap<_, _>>();
            let fills = canonical["fills"].as_array().unwrap();
            assert!(!fills.is_empty(), "actual native fills required");
            for fill in fills {
                let submitted = initialized[fill["client_order_id"].as_str().unwrap()];
                let filled = native_variant(&fill["event"]);
                for clock in ["ts_event", "ts_init"] {
                    let actual = native_clock(&filled[clock]);
                    assert!(
                        actual >= submitted + latency_ns,
                        "native {clock} preceded decision plus latency"
                    );
                    assert!(
                        actual <= selected.decision_cutoff_ns.get(),
                        "native fill exceeded frozen availability"
                    );
                }
            }
            // A later declared decision cannot escape the same frozen input
            // availability cutoff, even though event time is already earlier.
            let mut invalid = fold.simulation_request.clone();
            let target = invalid.target_points.last_mut().unwrap();
            target.asof_ns = fixture::count(selected.decision_cutoff_ns.get() + 1);
            target.valid_until_ns =
                fixture::count(target.asof_ns.get() + request.target_ttl_ns.get());
            let rejected = native::command(
                &[
                    "simulate".as_ref(),
                    "--catalog".as_ref(),
                    root.path().as_os_str(),
                ],
                &invalid,
            );
            assert!(!rejected.status.success());
        }
    }
}

#[test]
fn btc_usdt_margin_replay_rejects_usd_settings() {
    let (root, mut request, parts) = fixture::market_fixture(
        false,
        fixture::FixtureMarket::CryptoUsdtCurrencyPair,
        10 * fixture::SECOND,
    );
    assert_eq!(request.instrument_id, "BTC/USDT.SIM");
    assert_eq!(request.settings.base_currency, "USDT");
    request.settings.base_currency = "USD".into();
    let error = evaluate(root.path(), &request, &parts, &fixture::feature_policy()).unwrap_err();
    assert!(error.contains("SIMULATION_MARKET_UNSUPPORTED"), "{error}");
}
