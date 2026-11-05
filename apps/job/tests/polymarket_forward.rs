//! Local CLI parsing only; no public source, account, credential or order is used.
#![cfg(all(feature = "catalog-prepare", feature = "polymarket-history"))]
use std::process::Command;

#[test]
fn retained_forward_operator_requires_original_plan_without_execution_budget_flag() {
    let help = Command::new(env!("CARGO_BIN_EXE_source-tools"))
        .args(["catalog-prepare", "prepare-forward", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("--plan") && help.contains("--recording") && help.contains("--output"));
    assert!(!help.contains("--max-seconds"));
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("not-created");
    let inputs = [
        "catalog-prepare",
        "prepare-forward",
        "--recording",
        "not-read.ndjson",
        "--warmup-catalog",
        "not-read-catalog",
        "--warmup-metadata",
        "not-read-metadata.json",
        "--warmup-selection",
        "not-read-selection.json",
        "--original-features",
        "not-read-features.json",
    ];
    let missing_plan = Command::new(env!("CARGO_BIN_EXE_source-tools"))
        .args(inputs)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(missing_plan.status.code(), Some(2));
    let diagnostic = String::from_utf8_lossy(&missing_plan.stderr);
    assert!(diagnostic.contains("required arguments") && diagnostic.contains("--plan"));
    let denied = Command::new(env!("CARGO_BIN_EXE_source-tools"))
        .args(inputs)
        .args(["--plan", "not-read-plan.json", "--max-seconds", "30"])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(denied.status.code(), Some(2));
    let diagnostic = String::from_utf8_lossy(&denied.stderr);
    assert!(diagnostic.contains("unexpected argument") && diagnostic.contains("--max-seconds"));
    assert!(!output.exists());
}

#[test]
fn retired_job_forward_recorder_is_not_a_live_control_plane_entrypoint() {
    let retired = Command::new(env!("CARGO_BIN_EXE_job"))
        .args(["polymarket-paper", "record-forward", "--help"])
        .output()
        .unwrap();
    assert_eq!(retired.status.code(), Some(2));
    assert!(retired.stdout.is_empty());
    assert!(String::from_utf8_lossy(&retired.stderr).contains("unrecognized subcommand"));
}

#[test]
fn forward_preparation_is_an_explicit_offline_operator_mode() {
    let help = Command::new(env!("CARGO_BIN_EXE_catalog-prepare"))
        .args(["prepare-forward", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    for flag in [
        "--recording",
        "--plan",
        "--warmup-catalog",
        "--warmup-metadata",
        "--warmup-selection",
        "--original-features",
        "--output",
    ] {
        assert!(help.contains(flag), "{flag}");
    }
}
