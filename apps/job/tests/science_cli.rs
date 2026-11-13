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

#[test]
fn retired_ensemble_and_review_allocation_commands_are_not_executable() {
    for operation in [
        "blend-targets",
        "capital-curve",
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
