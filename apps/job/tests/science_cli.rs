//! Exercise the actual native binary, protocol input and safe error channel.
#[path = "support/market.rs"]
mod market;
use contracts::science::{NativeForecastResultV1, NativeSimulationResultV1};
use market::{forecast_request, market, module};
#[path = "support/command.rs"]
mod native;
use native::command;

#[test]
fn real_native_cli_forecasts_and_replays_a_single_shared_account() {
    let (directory, request) = market("0", 20);
    let model = directory.path().join("model.wasm");
    std::fs::write(&model, module("local.get 2 local.get 3 f64.sub")).unwrap();
    let result = command(
        &[
            "forecast".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
            "--model".as_ref(),
            model.as_os_str(),
        ],
        &forecast_request(&request),
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let forecast: NativeForecastResultV1 = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(forecast.points.len(), 40);
    let result = command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
        ],
        &request,
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let simulation: NativeSimulationResultV1 = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(simulation.consumed_target_points.get(), 2);
    assert!(simulation.orders.get() >= 4);
}

#[test]
fn price_bar_forecast_cli_matches_the_original_features_without_trading_definitions() {
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;

    let (original, request) = market("0", 20);
    let forecast_request = forecast_request(&request);
    let wasm = module("local.get 2 local.get 3 f64.sub");
    let expected = job::forecast::forecast(original.path(), &forecast_request, &wasm).unwrap();
    let data = job::catalog::load_catalog(original.path(), &request.selection).unwrap();
    let prices = tempfile::tempdir().unwrap();
    let catalog =
        ParquetDataCatalog::from_uri(prices.path().to_str().unwrap(), None, Some(16), None, None)
            .unwrap();
    for series in &data.series {
        catalog
            .write_to_parquet(&series.bars, None, None, None)
            .unwrap();
    }
    let model = prices.path().join("model.wasm");
    std::fs::write(&model, &wasm).unwrap();
    let result = command(
        &[
            "forecast-factor".as_ref(),
            "--catalog".as_ref(),
            prices.path().as_os_str(),
            "--model".as_ref(),
            model.as_os_str(),
        ],
        &forecast_request,
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let observed: NativeForecastResultV1 = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        serde_json::to_value(observed).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert!(job::forecast::forecast(prices.path(), &forecast_request, &wasm).is_err());
}

#[test]
fn native_cli_reports_invalid_model_cause_without_echoing_model_contents() {
    let (directory, request) = market("0", 20);
    let model = directory.path().join("private-sentinel.wasm");
    std::fs::write(&model, b"SENTINEL_NOT_FOR_LOGS").unwrap();
    let result = command(
        &[
            "forecast".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
            "--model".as_ref(),
            model.as_os_str(),
        ],
        &forecast_request(&request),
    );
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert_eq!(
        stderr.trim(),
        "QZ_NATIVE_JOB_FAILED\nSIGNAL_REQUIRES_WASM_BINARY"
    );
    assert!(!stderr.contains("SENTINEL_NOT_FOR_LOGS"));
}

#[cfg(unix)]
#[test]
fn native_cli_rejects_model_symlinks_and_fifos_without_opening_them() {
    let (directory, request) = market("0", 20);
    let model = directory.path().join("original.wasm");
    std::fs::write(&model, module("f64.const 1")).unwrap();
    let link = directory.path().join("link.wasm");
    std::os::unix::fs::symlink(&model, &link).unwrap();
    let result = command(
        &[
            "forecast".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
            "--model".as_ref(),
            link.as_os_str(),
        ],
        &forecast_request(&request),
    );
    assert!(!result.status.success());
    assert_eq!(
        String::from_utf8_lossy(&result.stderr).trim(),
        "QZ_NATIVE_JOB_FAILED\nnative model file limit"
    );
    let fifo = directory.path().join("fifo.wasm");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    let result = command(
        &[
            "forecast".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
            "--model".as_ref(),
            fifo.as_os_str(),
        ],
        &forecast_request(&request),
    );
    assert!(!result.status.success());
    assert_eq!(
        String::from_utf8_lossy(&result.stderr).trim(),
        "QZ_NATIVE_JOB_FAILED\nnative model file limit"
    );
}

#[test]
fn malformed_native_catalog_exits_through_the_safe_cli_error_channel() {
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
    let (directory, request) = market("0", 20);
    let model = directory.path().join("model.wasm");
    std::fs::write(&model, module("f64.const 1")).unwrap();
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    let files = catalog.list_parquet_files("data").unwrap();
    assert!(!files.is_empty());
    let path = directory.path().join(&files[0]);
    let original = std::fs::read(&path).unwrap();
    let mut corrupted = original.clone();
    let footer = corrupted.len() - 8;
    corrupted[footer..footer + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(&path, &corrupted).unwrap();
    for result in [
        command(
            &[
                "forecast".as_ref(),
                "--catalog".as_ref(),
                directory.path().as_os_str(),
                "--model".as_ref(),
                model.as_os_str(),
            ],
            &forecast_request(&request),
        ),
        command(
            &[
                "simulate".as_ref(),
                "--catalog".as_ref(),
                directory.path().as_os_str(),
            ],
            &request,
        ),
    ] {
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&result.stderr).lines().next(),
            Some("QZ_NATIVE_JOB_FAILED")
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), corrupted);
    std::fs::write(path, original).unwrap();
    let recovered = command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
        ],
        &request,
    );
    assert!(recovered.status.success());
    let result: NativeSimulationResultV1 = serde_json::from_slice(&recovered.stdout).unwrap();
    assert_eq!(result.consumed_target_points.get(), 2);
}

#[test]
fn native_capacity_uses_original_catalog_before_decision_and_reports_conditional_bounds() {
    use contracts::capacity::{NativeCapacityReportV1, NativeCapacityRequestV1};
    use contracts::science::NativeRollingBarLiquidityPolicyV1;
    let (directory, simulation) = market("0", 20);
    let mut request = NativeCapacityRequestV1 {
        schema_version: contracts::SchemaV1,
        simulation,
        policy: NativeRollingBarLiquidityPolicyV1 {
            schema_version: contracts::SchemaV1,
            maximum_age_seconds: 120,
            participation_limit: "0.01".parse().unwrap(),
        },
        capitals: vec![
            "1000.000000000000000001".parse().unwrap(),
            "10000000".parse().unwrap(),
        ],
    };
    let run = |request: &NativeCapacityRequestV1| {
        let output = command(
            &[
                "assess-capacity".as_ref(),
                "--catalog".as_ref(),
                directory.path().as_os_str(),
            ],
            request,
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<NativeCapacityReportV1>(&output.stdout).unwrap()
    };
    let report = run(&request);
    assert_eq!(report.status, "ESTIMATED");
    assert_eq!(
        report.maximum_capital.unwrap().as_decimal(),
        &"250500".parse::<bigdecimal::BigDecimal>().unwrap()
    );
    assert_eq!(report.binding_instrument.as_deref(), Some("EUR/USD.SIM"));
    let constraints = report.constraints.as_ref().unwrap();
    assert_eq!(constraints.len(), 2);
    for constraint in constraints {
        assert_eq!(constraint.instrument_id, "EUR/USD.SIM");
        assert_eq!(constraint.decision_ns, market::instant(2));
        assert_eq!(
            constraint.bar_event_ns,
            market::count(2 * market::INTERVAL_NS)
        );
        assert_eq!(constraint.bar_available_ns, market::instant(2));
        assert_eq!(
            constraint.observed_notional.as_decimal(),
            &"10020000".parse::<bigdecimal::BigDecimal>().unwrap()
        );
        assert_eq!(
            constraint.required_weight.as_decimal(),
            &"0.4".parse::<bigdecimal::BigDecimal>().unwrap()
        );
    }
    let coverage = report.coverage.as_ref().unwrap();
    assert_eq!(
        (coverage.decision_count, coverage.assessed_decision_count),
        (2, 2)
    );
    assert_eq!(
        (
            coverage.required_observations,
            coverage.covered_observations,
            coverage.unique_bars,
            coverage.reused_bar_observations
        ),
        (4, 4, 4, 0)
    );
    let stress = report.volume_stress.as_ref().unwrap();
    assert_eq!(
        stress[0].volume_multiplier.as_decimal(),
        &bigdecimal::BigDecimal::from(1)
    );
    assert_eq!(
        serde_json::to_value(&stress[0].scenarios).unwrap(),
        serde_json::to_value(&report.scenarios).unwrap()
    );
    assert_eq!(
        stress[1].maximum_capital.as_ref().unwrap().as_decimal(),
        &"125250".parse::<bigdecimal::BigDecimal>().unwrap()
    );
    assert_eq!(
        stress[1].scenarios[0]
            .maximum_participation
            .as_ref()
            .unwrap()
            .as_decimal(),
        &(report.scenarios[0]
            .maximum_participation
            .as_ref()
            .unwrap()
            .as_decimal()
            * 2)
    );
    let economic = report.economic_capacity.as_ref().unwrap();
    assert_eq!(economic.status, "UNAVAILABLE");
    assert!(economic.maximum_capital.is_none());
    assert_eq!(economic.missing_inputs.len(), 4);
    assert_eq!(report.scenarios[0].capital, request.capitals[0]);
    assert_eq!(report.scenarios[0].status, "FEASIBLE");
    assert_eq!(report.scenarios[1].status, "CONSTRAINED");
    assert!(report.minimum_capital.unwrap().is_positive());
    // The second BAR has occurred, but is not yet available: it cannot fund this decision.
    request.simulation.target_points[0].asof_ns = market::count(2 * market::INTERVAL_NS);
    assert_eq!(
        run(&request).maximum_capital.unwrap().as_decimal(),
        &"250250".parse::<bigdecimal::BigDecimal>().unwrap()
    );
    request.simulation.target_points[0].asof_ns = market::count(market::INTERVAL_NS);
    assert_eq!(
        run(&request).reason.as_deref(),
        Some("available_at_liquidity_missing")
    );
    request.simulation.target_points.remove(0);
    request.simulation.target_points[0].asof_ns = market::count(100 * market::INTERVAL_NS);
    request.simulation.target_points[0].valid_until_ns = market::count(101 * market::INTERVAL_NS);
    let stale = run(&request);
    assert_eq!(stale.reason.as_deref(), Some("liquidity_stale"));
    assert_eq!(stale.coverage.as_ref().unwrap().required_observations, 2);
    assert_eq!(stale.coverage.as_ref().unwrap().covered_observations, 0);
    assert!(stale
        .volume_stress
        .as_ref()
        .unwrap()
        .iter()
        .all(|row| row.status == "UNAVAILABLE" && row.maximum_capital.is_none()));
    assert_eq!(stale.scenarios.len(), request.capitals.len());
    for (row, capital) in stale.scenarios.iter().zip(&request.capitals) {
        assert_eq!(&row.capital, capital);
        assert_eq!(row.status, "UNAVAILABLE");
        assert_eq!(row.reason.as_deref(), Some("liquidity_stale"));
        assert!(row.required_turnover.is_none() && row.maximum_participation.is_none());
        assert!(row.usable_capital.is_none() && row.unallocated_cash.is_none());
    }
    request.simulation.target_points[0].asof_ns = market::instant(7);
    request.simulation.settings.base_currency = "EUR".into();
    assert_eq!(run(&request).reason.as_deref(), Some("currency_mismatch"));
    request.capitals[0] = "-1".parse().unwrap();
    let invalid = command(
        &[
            "assess-capacity".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
        ],
        &request,
    );
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("capacity_capital_invalid"));
}

#[test]
fn native_capacity_reused_bar_does_not_replenish_the_turnover_budget() {
    use contracts::capacity::NativeCapacityRequestV1;
    let (directory, mut simulation) = market("0", 20);
    let mut exit = simulation.target_points[0].clone();
    exit.asof_ns = market::count(exit.asof_ns.get() + 1);
    for target in &mut exit.targets {
        target.weight = "0".parse().unwrap();
    }
    exit.cash_weight = "1".parse().unwrap();
    simulation.target_points = vec![simulation.target_points[0].clone(), exit];
    let request = NativeCapacityRequestV1 {
        schema_version: contracts::SchemaV1,
        simulation,
        policy: serde_json::from_value(serde_json::json!({"schema_version":1,"maximum_age_seconds":120,"participation_limit":"0.01"})).unwrap(),
        capitals: vec!["200000".parse().unwrap()],
    };
    let report = job::capacity::assess(directory.path(), &request).unwrap();
    // Establishment 0.4 plus liquidation 0.4 share the original 10,020,000 BAR.
    assert_eq!(
        report.maximum_capital.unwrap().as_decimal(),
        &"125250".parse::<bigdecimal::BigDecimal>().unwrap()
    );
    assert_eq!(report.scenarios[0].status, "CONSTRAINED");
    let coverage = report.coverage.as_ref().unwrap();
    assert_eq!(
        (
            coverage.required_observations,
            coverage.covered_observations,
            coverage.unique_bars,
            coverage.reused_bar_observations
        ),
        (4, 4, 2, 2)
    );
    let entry = &report.constraints.as_ref().unwrap()[0];
    assert_eq!(
        entry.required_weight.as_decimal(),
        &"0.8".parse::<bigdecimal::BigDecimal>().unwrap()
    );
    assert_eq!(
        entry.decision_ns,
        request.simulation.target_points[1].asof_ns
    );
    assert_eq!(
        report.scenarios[0]
            .required_turnover
            .as_ref()
            .unwrap()
            .as_decimal(),
        &"160000".parse::<bigdecimal::BigDecimal>().unwrap()
    );
}

#[test]
fn native_capacity_scoped_zero_does_not_sell_a_retained_out_of_scope_holding() {
    use contracts::capacity::NativeCapacityRequestV1;
    let (directory, mut simulation) = market("0", 20);
    simulation.target_points[0].rebalance_instrument_ids = Some(vec!["EUR/USD.SIM".into()]);
    simulation.target_points[1].rebalance_instrument_ids = Some(vec!["GBP/USD.SIM".into()]);
    simulation.target_points[1].targets[0].weight = "0".parse().unwrap();
    simulation.target_points[1].targets[1].weight = "0.5".parse().unwrap();
    simulation.target_points[1].cash_weight = "0.5".parse().unwrap();
    let request = NativeCapacityRequestV1 {
        schema_version: contracts::SchemaV1,
        simulation,
        policy: serde_json::from_value(serde_json::json!({"schema_version":1,"maximum_age_seconds":120,"participation_limit":"0.01"})).unwrap(),
        capitals: vec!["1000".parse().unwrap()],
    };
    let report = job::capacity::assess(directory.path(), &request).unwrap();
    assert_eq!(report.status, "ESTIMATED");
    // First only EUR is established; second establishes GBP and still assesses EUR's exit.
    let coverage = report.coverage.as_ref().unwrap();
    assert_eq!(
        (
            coverage.required_observations,
            coverage.covered_observations,
            coverage.unique_bars
        ),
        (3, 3, 3)
    );
    assert_eq!(
        report.scenarios[0]
            .entry_rebalance_turnover
            .as_ref()
            .unwrap()
            .as_decimal(),
        &bigdecimal::BigDecimal::from(500)
    );
    assert_eq!(
        report.scenarios[0]
            .exit_turnover
            .as_ref()
            .unwrap()
            .as_decimal(),
        &bigdecimal::BigDecimal::from(500)
    );
}

#[test]
fn native_capacity_missing_targets_are_not_silent_liquidation_or_optimistic_capacity() {
    use contracts::capacity::NativeCapacityRequestV1;
    for scoped in [false, true] {
        let (directory, mut simulation) = market("0", 20);
        simulation.target_points[1].targets.remove(0);
        simulation.target_points[1].cash_weight = "0.4".parse().unwrap();
        if scoped {
            simulation.target_points[1].rebalance_instrument_ids = Some(vec!["GBP/USD.SIM".into()]);
        }
        // Inspect the actual native replay contract, not assumed sparse-vector semantics.
        let output = command(
            &[
                "simulate".as_ref(),
                "--catalog".as_ref(),
                directory.path().as_os_str(),
            ],
            &simulation,
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("SIMULATION_TARGET_INVALID"));
        let request = NativeCapacityRequestV1 {
            schema_version: contracts::SchemaV1,
            simulation,
            policy: serde_json::from_value(serde_json::json!({"schema_version":1,"maximum_age_seconds":120,"participation_limit":"0.01"})).unwrap(),
            capitals: vec!["1000".parse().unwrap()],
        };
        let report = job::capacity::assess(directory.path(), &request).unwrap();
        assert_eq!(report.status, "UNAVAILABLE");
        assert_eq!(
            report.reason.as_deref(),
            Some("capacity_target_identity_mismatch")
        );
        assert!(report.maximum_capital.is_none());
    }
}

fn target_blend_request() -> serde_json::Value {
    use serde_json::json;
    let row = |asof: u64| {
        let point = |a: &str, b: &str, cash: &str, ttl: u64| {
            json!({
                "schema_version": 1, "asof_ns": asof.to_string(),
                "valid_until_ns": (asof + ttl).to_string(),
                "targets": [
                    {"instrument_id": "A.SIM", "currency": "USD", "weight": a},
                    {"instrument_id": "B.SIM", "currency": "USD", "weight": b}
                ], "cash_weight": cash
            })
        };
        vec![
            point(
                "0.10000000000000001",
                "0.2",
                "0.69999999999999999",
                60_000_000_000,
            ),
            point("0.7", "0.1", "0.2", 30_000_000_000),
        ]
    };
    json!({"schema_version": 1, "weights": ["0.3", "0.7"],
        "rows": [row(9_007_199_254_740_993), row(9_007_289_254_740_993)]})
}

#[test]
fn native_cli_blend_targets_preserves_exact_decimals_clocks_and_minimum_expiry() {
    let request = target_blend_request();
    let output = command(&["blend-targets".as_ref()], &request);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let result: contracts::science::NativeTargetBlendResultV1 =
        serde_json::from_slice(&output.stdout).unwrap();
    let points: Vec<_> = request["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mut expected = row[1].clone();
            expected["targets"][0]["weight"] = "0.520000000000000003".into();
            expected["targets"][1]["weight"] = "0.13".into();
            expected["cash_weight"] = "0.349999999999999997".into();
            expected
        })
        .collect();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::json!({"schema_version": 1, "target_points": points})
    );
}

#[test]
fn native_cli_blend_targets_rejects_bad_weights_and_original_point_misalignment() {
    use serde_json::json;
    for case in 0..13 {
        let mut request = target_blend_request();
        match case {
            0 => request["rows"] = json!([]),
            1 => request["rows"][0] = json!([]),
            2 => {
                request["rows"][0].as_array_mut().unwrap().pop();
            }
            3 => request["weights"] = json!(["0.3", "0.6"]),
            4 => request["weights"] = json!(["-0.1", "1.1"]),
            5 => request["weights"] = json!([0.3, 0.7]),
            6 => request["rows"][0][1]["asof_ns"] = "9007199254740994".into(),
            7 => request["rows"][0][1]["valid_until_ns"] = request["rows"][0][1]["asof_ns"].clone(),
            8 => request["rows"][0][1]["targets"][0]["instrument_id"] = "OTHER.SIM".into(),
            9 => request["rows"][0][1]["targets"][0]["currency"] = "EUR".into(),
            10 => request["rows"][0][1]["cash_weight"] = "0".into(),
            11 => request["rows"][0][1]["targets"][0]["weight"] = "-0.1".into(),
            _ => request["rows"][0][0]["targets"] = json!([]),
        }
        let output = command(&["blend-targets".as_ref()], &request);
        assert_eq!(output.status.code(), Some(1), "case {case}");
        assert!(output.stdout.is_empty(), "case {case}");
    }
}
