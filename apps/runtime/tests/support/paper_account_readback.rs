//! Continue the original SQLx producer before its database, vault or listener drops.
//! The prebuilt Paper test owns native execution and snapshot assertions; this
//! continuation transports its exact retained bytes through the shipping CLI.
use super::{client, direct_strategy::ClaimedStrategy, support};
#[path = "../../../server/tests/support/native_account_pipeline.rs"]
#[allow(dead_code)]
mod native_account_pipeline;
#[path = "../../../server/tests/support/portable_client.rs"]
mod portable_client;

use axum::http::StatusCode;
use contracts::{
    account_observation::AccountObservationSubmitV1, strategy_portfolio::TargetPackageEnvelopeV2,
};
use native_account_pipeline::{assert_clean_exit, records};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Stdio, time::Duration};

async fn ledger(pool: &PgPool) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
            'sources', (SELECT coalesce(jsonb_agg(to_jsonb(s) ORDER BY id), '[]'::jsonb) FROM app.native_account_sources s),
            'observations', (SELECT coalesce(jsonb_agg(to_jsonb(o) ORDER BY id), '[]'::jsonb) FROM app.native_account_observations o),
            'cursors', (SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY source_id), '[]'::jsonb) FROM app.native_account_cursors c))",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn invoke(
    directory: &Path,
    origin: &str,
    credential: &Path,
    token: &str,
    arguments: &[&str],
) -> std::process::Output {
    let mut command = vec![
        "--origin",
        origin,
        "--credential-file",
        credential.to_str().unwrap(),
        "--development-http",
    ];
    command.extend_from_slice(arguments);
    let output = portable_client::saved(directory, &command, Value::Null).await;
    assert_clean_exit(&output, "joined Paper account CLI", Some(token));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
    output
}

pub(super) async fn run(
    pool: &PgPool,
    auth: &support::Fixture,
    cookie: &str,
    origin: &str,
    produced: &ClaimedStrategy,
    harness: &Path,
    reader_credential: &Path,
) {
    let TargetPackageEnvelopeV2::TargetDecision(package) = &produced.claim.package else {
        panic!("same producer must return its SQL V2 target decision claim")
    };
    let directory = tempfile::tempdir().unwrap();
    let claim_path = directory.path().join("original-claim.json");
    let claim_bytes = serde_json::to_vec(&json!({
        "claim": produced.claim,
        "execution_assumptions": produced.execution_assumptions,
    }))
    .unwrap();
    fs::write(&claim_path, &claim_bytes).unwrap();
    let segment_path = directory.path().join("original-paper.ndjson");
    let child = tokio::process::Command::new(harness)
        .args([
            "--exact",
            "claimed_target_reaches_one_native_paper_session_and_retained_shutdown",
            "--test-threads=1",
            "--nocapture",
        ])
        .current_dir(directory.path())
        .env_clear()
        .env("QZ_NATIVE_STRATEGY_CLAIM_FILE", &claim_path)
        .env("QZ_NATIVE_PAPER_OBSERVATIONS_FILE", &segment_path)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run the same-candidate native Paper test executable");
    let paper = tokio::time::timeout(Duration::from_secs(60), child.wait_with_output())
        .await
        .expect("joined native Paper subprocess deadline")
        .unwrap();
    if !paper.status.success() {
        // Rust's test harness puts assertion failures on stdout. This child
        // receives only the fixture claim/output paths, never CLI credentials.
        let stdout = String::from_utf8_lossy(&paper.stdout);
        let mut start = stdout.len().saturating_sub(2048);
        while !stdout.is_char_boundary(start) {
            start += 1;
        }
        eprintln!(
            "native Paper test stdout tail (at most 2048 bytes):\n{}",
            &stdout[start..]
        );
        assert_clean_exit(&paper, "joined native Paper test", None);
    }
    // The file is created only after the original test's fill, fee, native
    // snapshot, retry and disconnected-shutdown assertions all succeed.
    let segment = fs::read(&segment_path).unwrap();
    let envelopes = records(&segment);
    assert!(envelopes.len() > 1);
    let binding = &envelopes[0]["binding"];
    assert_eq!(binding["project_id"], json!(package.project_id));
    assert_eq!(binding["environment"], "PAPER");
    assert_eq!(binding["native_trader_id"], package.account_start.trader_id);
    assert_eq!(
        binding["native_account_id"],
        package.account_start.account_id
    );
    assert!(!binding["native_session_id"].as_str().unwrap().is_empty());
    for envelope in &envelopes {
        assert_eq!(envelope["binding"], *binding);
    }
    let terminal = envelopes.last().unwrap();
    assert_eq!(terminal["connection"], "DISCONNECTED");
    assert!(terminal["snapshot"].is_null());
    assert_eq!(fs::read(&claim_path).unwrap(), claim_bytes);

    // Issue a genuine verifier in the producer's original shared vault for its
    // existing downstream principal. No recreated project/downstream/profile.
    let credential = client::browser(
        auth,
        cookie,
        "native-paper-account-relay-credential",
        &format!("/api/v2/machine-principals/{}/credentials", produced.principal_id),
        json!({"schema_version":1,"scope_codes":["FORWARD_SUBMIT"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)}),
    )
    .await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let credential_path = directory.path().join("downstream-token");
    fs::write(&credential_path, token).unwrap();
    fs::set_permissions(&credential_path, fs::Permissions::from_mode(0o600)).unwrap();
    let arguments = [
        "forward",
        "accounts",
        "relay",
        "--input",
        segment_path.to_str().unwrap(),
        "--max-attempts",
        "2",
    ];
    let before = ledger(pool).await;
    assert_eq!(before["sources"], json!([]));
    assert_eq!(before["observations"], json!([]));
    assert_eq!(before["cursors"], json!([]));
    let sent = invoke(
        directory.path(),
        origin,
        &credential_path,
        token,
        &arguments,
    )
    .await;
    let receipts = records(&sent.stdout);
    assert_eq!(receipts.len(), envelopes.len());
    let first = &receipts[0]["resource"];
    for (receipt, envelope) in receipts.iter().zip(&envelopes) {
        assert_eq!(receipt["replayed"], false);
        assert_eq!(receipt["resource"]["observation"], *envelope);
        assert_eq!(
            receipt["resource"]["downstream_id"],
            json!(package.account_start.downstream_id)
        );
        assert_eq!(receipt["resource"]["source_id"], first["source_id"]);
        assert_eq!(receipt["resource"]["gap_before"], false);
    }
    let last = &receipts.last().unwrap()["resource"];
    let latest = receipts
        .iter()
        .map(|receipt| &receipt["resource"])
        .filter(|resource| !resource["observation"]["snapshot"].is_null())
        .max_by_key(|resource| {
            resource["observation"]["snapshot"]["ts_init"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
        })
        .expect("Paper test retained original native snapshots");
    let latest_envelope: AccountObservationSubmitV1 =
        serde_json::from_value(latest["observation"].clone()).unwrap();

    // Reuse the producer's original project-scoped CLI/RESEARCH_READ token.
    // Explicit credential-file is the shipping machine-token entrypoint.
    let reader = fs::read_to_string(reader_credential).unwrap();
    let project = package.project_id.to_string();
    let source = first["source_id"].as_str().unwrap();
    let current_args = ["forward", "accounts", "current", &project, source];
    let current: Value = serde_json::from_slice(
        &invoke(
            directory.path(),
            origin,
            reader_credential,
            &reader,
            &current_args,
        )
        .await
        .stdout,
    )
    .unwrap();
    assert_eq!(current["latest_snapshot"], *latest);
    assert_eq!(
        current["valuation"],
        json!(domain::account_observation::valuation(
            latest_envelope.snapshot.as_ref()
        ))
    );
    assert_eq!(current["source"]["binding"], *binding);
    assert_eq!(
        current["source"]["downstream_id"],
        json!(package.account_start.downstream_id)
    );
    assert_eq!(current["source"]["connection"], "DISCONNECTED");
    assert_eq!(current["source"]["last_sequence"], terminal["sequence"]);
    assert_eq!(current["source"]["dropped_events"], "0");
    assert_eq!(current["source"]["has_gap"], false);
    assert_eq!(current["source"]["last_observation_id"], last["id"]);
    assert_eq!(current["source"]["latest_snapshot_id"], latest["id"]);
    assert_eq!(
        current["source"]["last_observed_at_ns"],
        terminal["observed_at_ns"]
    );
    assert_eq!(current["source"]["last_received_at"], last["received_at"]);
    let history = history(
        directory.path(),
        origin,
        reader_credential,
        &reader,
        &project,
        source,
    )
    .await;
    let mut expected: Vec<_> = receipts
        .iter()
        .map(|receipt| receipt["resource"].clone())
        .collect();
    expected.sort_by(|left, right| right["id"].as_str().cmp(&left["id"].as_str()));
    assert_eq!(
        history, expected,
        "every original event, session, clock and currency survives PostgreSQL history"
    );
    let persisted = ledger(pool).await;
    assert_eq!(persisted["sources"].as_array().unwrap().len(), 1);
    assert_eq!(
        persisted["observations"].as_array().unwrap().len(),
        envelopes.len()
    );
    assert_eq!(persisted["cursors"].as_array().unwrap().len(), 1);

    // A fresh shipping CLI process resends the same retained bytes. Compare all
    // persisted columns as well as receipts, so replay cannot refresh clocks.
    let replay = invoke(
        directory.path(),
        origin,
        &credential_path,
        token,
        &arguments,
    )
    .await;
    let replayed = records(&replay.stdout);
    assert_eq!(replayed.len(), receipts.len());
    for (replay, receipt) in replayed.iter().zip(&receipts) {
        assert_eq!(replay["replayed"], true);
        assert_eq!(replay["resource"], receipt["resource"]);
    }
    assert_eq!(ledger(pool).await, persisted);
    let mut after: Value = serde_json::from_slice(
        &invoke(
            directory.path(),
            origin,
            reader_credential,
            &reader,
            &current_args,
        )
        .await
        .stdout,
    )
    .unwrap();
    let mut current = current;
    // This is only the read clock, not evidence freshness or native event time.
    after["source"]
        .as_object_mut()
        .unwrap()
        .remove("checked_at");
    current["source"]
        .as_object_mut()
        .unwrap()
        .remove("checked_at");
    assert_eq!(after, current);
    assert_eq!(
        self::history(
            directory.path(),
            origin,
            reader_credential,
            &reader,
            &project,
            source
        )
        .await,
        history
    );
    assert_eq!(fs::read(&segment_path).unwrap(), segment);
    assert_eq!(fs::read(&claim_path).unwrap(), claim_bytes);
    println!(
        "NATIVE_STRATEGY_PAPER_ACCOUNT_READBACK_OK original_sql_claim=1 native_paper_session=1 sources=1 observations={} replayed={} disconnected=true",
        envelopes.len(),
        replayed.len()
    );
}

async fn history(
    directory: &Path,
    origin: &str,
    credential: &Path,
    token: &str,
    project: &str,
    source: &str,
) -> Vec<Value> {
    let mut items = Vec::new();
    let mut cursor = None::<String>;
    loop {
        let mut arguments = vec![
            "forward", "accounts", "history", project, source, "--limit", "100",
        ];
        if let Some(cursor) = &cursor {
            arguments.extend(["--cursor", cursor]);
        }
        let page: Value = serde_json::from_slice(
            &invoke(directory, origin, credential, token, &arguments)
                .await
                .stdout,
        )
        .unwrap();
        let next = page["items"].as_array().unwrap();
        items.extend(next.iter().cloned());
        if page["next_cursor"].is_null() {
            return items;
        }
        assert!(!next.is_empty());
        let next_cursor = page["next_cursor"].as_str().unwrap().to_owned();
        assert_ne!(cursor.as_ref(), Some(&next_cursor));
        cursor = Some(next_cursor);
    }
}
