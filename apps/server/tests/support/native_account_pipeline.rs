//! Test-only subprocess and exact native JSON comparisons; no accounting arithmetic.
use contracts::DecimalValue;
use serde_json::{json, Value};
use std::{
    fs::File,
    path::Path,
    process::{Output, Stdio},
    time::Duration,
};

pub fn assert_clean_exit(output: &Output, label: &str, token: Option<&str>) {
    if output.status.success() && output.stderr.is_empty() {
        return;
    }
    let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    // Redact the entire known credential before selecting the bounded tail, so
    // truncation cannot expose a partial token. Never include argv/env/input.
    if let Some(token) = token.filter(|token| !token.is_empty()) {
        stderr = stderr.replace(token, "[REDACTED]");
    }
    let mut start = stderr.len().saturating_sub(4096);
    while !stderr.is_char_boundary(start) {
        start += 1;
    }
    panic!(
        "{label} did not exit cleanly: {}; stderr tail (at most 4096 bytes):\n{}",
        output.status,
        &stderr[start..],
    );
}

pub async fn run(
    binary_variable: &str,
    directory: &Path,
    arguments: &[&str],
    input: Option<&Path>,
) -> std::process::Output {
    let binary = std::env::var_os(binary_variable)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("build this candidate and set {binary_variable} to its Cargo-reported executable path"));
    assert!(
        binary.is_absolute() && binary.is_file(),
        "invalid {binary_variable}"
    );
    let child = tokio::process::Command::new(binary)
        .args(arguments)
        .current_dir(directory)
        .env_clear()
        .kill_on_drop(true)
        .stdin(input.map_or_else(Stdio::null, |path| File::open(path).unwrap().into()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("execute the explicitly supplied native test binary");
    let output = tokio::time::timeout(Duration::from_secs(25), child.wait_with_output())
        .await
        .expect("native account subprocess deadline")
        .unwrap();
    assert_clean_exit(&output, binary_variable, None);
    output
}

pub fn records(bytes: &[u8]) -> Vec<Value> {
    assert!(
        bytes.ends_with(b"\n"),
        "must retain complete NDJSON records"
    );
    bytes
        .split_inclusive(|byte| *byte == b'\n')
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}

fn assert_money(actual: &Value, native: &Value) {
    // Official Money serde emits an exact decimal plus its currency. Parse only
    // with the existing wire decimal type: no f64, rounding or economic formula.
    let (amount, currency) = native.as_str().unwrap().rsplit_once(' ').unwrap();
    assert_eq!(
        actual,
        &json!({"amount": amount.parse::<DecimalValue>().unwrap(), "currency": currency})
    );
}

pub fn assert_snapshot(actual: &Value, native: &Value) {
    for field in [
        "account_id",
        "account_type",
        "base_currency",
        "is_stale",
        "event_id",
        "stale_instruments",
        "stale_currencies",
        "unpriced_instruments",
    ] {
        assert_eq!(actual[field], native[field], "native field {field}");
    }
    for field in ["ts_event", "ts_init"] {
        assert_eq!(
            actual[field],
            native[field].as_u64().unwrap().to_string(),
            "native clock {field}"
        );
    }
    for field in ["total_equity", "realized_pnls", "unrealized_pnls"] {
        let actual = actual[field].as_array().unwrap();
        let native = native[field].as_array().unwrap();
        assert_eq!(actual.len(), native.len(), "native money count {field}");
        for (actual, native) in actual.iter().zip(native) {
            assert_money(actual, native);
        }
    }
    let balances = actual["balances"].as_array().unwrap();
    let native_balances = native["balances"].as_array().unwrap();
    assert_eq!(balances.len(), native_balances.len());
    for (balance, native_balance) in balances.iter().zip(native_balances) {
        for field in ["total", "locked", "free"] {
            assert_money(&balance[field], &native_balance[field]);
        }
    }
    // This cash fixture has neither margins nor a common base currency. Empty
    // native PnL is unavailable session evidence; do not manufacture a zero.
    for field in ["margins", "realized_pnls", "unrealized_pnls"] {
        assert_eq!(native[field], json!([]));
        assert_eq!(actual[field], native[field]);
    }
    assert!(native["base_currency_equity"].is_null());
    assert_eq!(
        actual["base_currency_equity"],
        native["base_currency_equity"]
    );
}
