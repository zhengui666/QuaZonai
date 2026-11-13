//! Exercise the actual native binary, protocol input and safe error channel.
#[path = "support/market.rs"]
mod market;
use contracts::science::NativeSimulationResultV1;
use market::market;
#[path = "support/command.rs"]
mod native;
use native::command;

#[test]
fn real_native_cli_replays_a_single_shared_account() {
    let (directory, request) = market("0", 20);
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
fn malformed_native_catalog_exits_through_the_safe_cli_error_channel() {
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
    let (directory, request) = market("0", 20);
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
    let result = command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
        ],
        &request,
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&result.stderr).lines().next(),
        Some("QZ_NATIVE_JOB_FAILED")
    );
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

#[test]
fn retired_ensemble_and_review_allocation_commands_are_not_executable() {
    for operation in [
        "assess-capacity",
        "size-account",
        "replay-tick-economic",
        "forecast",
        "forecast-factor",
        "allocate",
        "study-portfolio",
        "build-observed-spot",
        "study-observed-spot",
        "fit-ensemble",
        "apply-ensemble",
        "freeze-ensemble-calibrations",
        "review-allocate",
        "review-science",
        "evaluate-factor",
        "evaluate-tick-factor",
        "scan-factor-catalog",
        "validate-alpha",
        "evaluate-sealed-alpha",
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_job"))
            .arg(operation)
            .output()
            .unwrap();
        assert!(!output.status.success(), "retired operation {operation}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
    }
}

#[test]
fn source_cli_rejects_missing_source_owner_metadata_without_success_output() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_job"))
        .arg("review-source")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"schema_version":1,"metadata":{},"observed_at":"2026-11-02T00:00:00Z"}"#)
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("QZ_NATIVE_JOB_FAILED"));
}
