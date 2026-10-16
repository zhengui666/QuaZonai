//! Local CLI parsing only; no public source, account, credential or order is used.
#![cfg(all(
    feature = "native-paper",
    feature = "catalog-prepare",
    feature = "polymarket-history"
))]
use std::process::Command;

#[test]
fn forward_recorder_has_business_plan_and_no_execution_budget_flag() {
    let help = Command::new(env!("CARGO_BIN_EXE_job"))
        .args(["polymarket-paper", "record-forward", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("--plan") && help.contains("--output"));
    assert!(!help.contains("--max-seconds"));
    let denied = Command::new(env!("CARGO_BIN_EXE_job"))
        .args([
            "polymarket-paper",
            "record-forward",
            "--plan",
            "not-read.json",
            "--output",
            "not-created.ndjson",
            "--max-seconds",
            "30",
        ])
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("unexpected argument"));
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
