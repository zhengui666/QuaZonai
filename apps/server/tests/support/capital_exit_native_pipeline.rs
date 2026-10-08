//! Disposable real Server/PG bridge to the same Job owner and official Sandbox.
//! Explicit native executable only; no production executable or credential source.
use super::*;
use std::{io::Write, process::Stdio, time::Duration};

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires this composed candidate's native capital-exit test executable"]
async fn capital_exit_http_releases_partial_cash_with_original_native_events(pool: PgPool) {
    let a = fixture_with_paper_exit(&pool, true).await;
    let directory = tempfile::tempdir().unwrap();
    let registered = directory.path().join("trusted-owner-registered.json");
    let result = directory.path().join("native-result.json");
    let mut input = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
    input.write_all(&serde_json::to_vec(&json!({"origin":a.origin,"token":a.token,"browser_cookie":a.browser_cookie,"project":a.project,"registered":registered,"result":result})).unwrap()).unwrap();
    input.as_file().sync_all().unwrap();
    let executable=std::env::var_os("QZ_NATIVE_CAPITAL_EXIT_TEST_BINARY").map(std::path::PathBuf::from).expect("build the composed Job native_capital_exit test with native-paper-test,native-sandbox-test and supply its Cargo-reported executable");
    assert!(executable.is_absolute() && executable.is_file());
    let child = tokio::process::Command::new(executable)
        .args([
            "capital_exit_http_pipeline::real_http_capital_exit_uses_original_sandbox_events",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .current_dir(directory.path())
        .env_clear()
        .stdin(std::fs::File::open(input.path()).unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let registration = async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            // Only the production authenticated intake hook may register this
            // actual source. The fixture observes it, never bypasses registration.
            let source=sqlx::query("SELECT s.id FROM app.native_account_sources s JOIN app.managed_capital_reservations r ON r.paper_account_source_id=s.id WHERE s.project_id=$1 AND s.downstream_id=$2 AND s.native_client_id='QZ-EXIT-SANDBOX' AND s.native_account_id='QZEXIT-001'")
                .bind(a.project.as_uuid()).bind(a.downstream.as_uuid()).fetch_optional(&pool).await.unwrap();
            if source.is_some() {
                std::fs::write(&registered, b"{\"registered\":true}\n").unwrap();
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "original client-bound native source never reached HTTP intake"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    let execution = async {
        tokio::time::timeout(Duration::from_secs(60), child.wait_with_output())
            .await
            .expect("native HTTP fixture deadline")
            .unwrap()
    };
    let ((), output) = tokio::join!(registration, execution);
    // Test harness writes its one selected test result to stdout. On failure,
    // redact the temporary API token before showing a bounded diagnostic tail.
    if !output.status.success() {
        let mut diagnostic = String::from_utf8_lossy(&output.stderr).into_owned();
        diagnostic.push_str(&String::from_utf8_lossy(&output.stdout));
        diagnostic = diagnostic
            .replace(&a.token, "[REDACTED]")
            .replace(&a.browser_cookie, "[REDACTED]");
        let mut begin = diagnostic.len().saturating_sub(8000);
        while !diagnostic.is_char_boundary(begin) {
            begin += 1;
        }
        panic!(
            "native partial-exit fixture failed {}: {}",
            output.status,
            &diagnostic[begin..]
        );
    }
    let result: Value = serde_json::from_slice(&std::fs::read(result).unwrap()).unwrap();
    assert_eq!(result["actual_execution"], "OFFICIAL_SANDBOX_ONLY");
    assert_eq!(result["withdrawal_performed"], false);
    assert_eq!(result["intent"]["funds"]["withdrawability"], "SIMULATED");
    assert_eq!(
        result["intent"]["funds"]["released_cash_amount"]["amount"],
        "70"
    );
    assert_ne!(result["intent"]["state"], "COMPLETED");
    assert_eq!(result["original_fills"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["original_cancellations"].as_array().unwrap().len(),
        1
    );
    let intent: Id = serde_json::from_value(result["intent"]["id"].clone()).unwrap();
    let original: Vec<Value> = sqlx::query_scalar(
        "SELECT content FROM app.capital_exit_evidence WHERE intent_id=$1 ORDER BY sequence",
    )
    .bind(intent.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        original,
        result["original_evidence"].as_array().unwrap().clone(),
        "HTTP intake changed or duplicated original native reports"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.capital_exit_intents WHERE project_id=$1")
            .bind(a.project.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "idempotent start created duplicate intents");
    let reserved:String=sqlx::query_scalar("SELECT r.reserved_amount::text FROM app.managed_capital_reservations r JOIN app.capital_exit_intents i ON i.managed_account_key=r.managed_account_key WHERE i.id=$1 AND r.paper_account_source_id=i.account_source_id").bind(intent.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        reserved.parse::<contracts::DecimalValue>().unwrap(),
        "70".parse().unwrap()
    );
    // The exact native terminal observer frame was also relayed after stop. A
    // read must lose readiness instead of displaying a stopped account as ready.
    let current = a
        .http
        .get(format!("{}/api/v2/capital-exits/{intent}", a.origin))
        .header("cookie", &a.browser_cookie)
        .send()
        .await
        .unwrap();
    assert!(current.status().is_success());
    let current: Value = current.json().await.unwrap();
    assert!(current["funds"]["verified_withdrawable_amount"].is_null());
    assert_ne!(current["funds"]["withdrawability"], "VERIFIED");
}
