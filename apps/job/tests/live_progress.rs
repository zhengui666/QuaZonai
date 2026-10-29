//! Exercise the actual native process and official snapshot topic, not a mock curve.
#[path = "support/market.rs"]
mod market;

use contracts::science::NativeSimulationResultV1;
use std::{
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn running_simulation_publishes_original_native_account_snapshots() {
    let (catalog, request) = market::market("0.001", 2_000);
    let preview = tempfile::tempdir().unwrap();
    let path = preview.path().join("progress.ndjson");
    let mut child = Command::new(env!("CARGO_BIN_EXE_job"))
        .arg("simulate")
        .arg("--catalog")
        .arg(catalog.path())
        .env_clear()
        .env("QZ_PROGRESS_PATH", &path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let start = Instant::now();
    let observed_while_running = loop {
        let running = child.try_wait().unwrap().is_none();
        if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() > 0) {
            break running;
        }
        if !running || start.elapsed() > Duration::from_secs(30) {
            break false;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        observed_while_running,
        "native snapshots must be readable before process completion"
    );
    let result: NativeSimulationResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    let final_snapshots: Vec<_> = result.canonical_result["portfolio_snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .map(without_canonical_identity)
        .collect();
    let encoded = std::fs::read_to_string(path).unwrap();
    let snapshots: Vec<serde_json::Value> = encoded
        .lines()
        .map(|line| {
            let record: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(record.as_object().unwrap().len(), 1);
            without_canonical_identity(&record["snapshot"])
        })
        .collect();
    assert!(snapshots.len() >= 2);
    for snapshot in &snapshots {
        assert!(
            final_snapshots.contains(snapshot),
            "preview must preserve an original native snapshot"
        );
    }
    let clock = |snapshot: &serde_json::Value| {
        snapshot["ts_event"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
    };
    assert_eq!(
        clock(snapshots.first().unwrap()),
        final_snapshots.iter().map(clock).min().unwrap()
    );
    assert_eq!(
        clock(snapshots.last().unwrap()),
        final_snapshots.iter().map(clock).max().unwrap()
    );
}

fn without_canonical_identity(snapshot: &serde_json::Value) -> serde_json::Value {
    let mut value = snapshot.clone();
    // The final upstream canonical result replaces original UUIDs with event-N.
    // Preview retains the original event UUID; compare its unmodified account facts.
    value.as_object_mut().unwrap().remove("event_id");
    value
}
