//! Disposable real HTTP/PG launcher for the existing Job library Paper fixture.
//! This does not run a public market host, a real account or the full Serve flow.
use super::*;
use std::{io::Write, process::Stdio, time::Duration};

const CHILD: &str = "polymarket_streaming_paper::capital_exit_tests::paper_service_acceptance::production_poller_uses_registered_original_paper_and_real_receipts";

async fn execute(pool: PgPool, scenario: &str) {
    let mut a = fixture(&pool).await;
    let owner = server::paper_capital_exit::PaperCapitalExitOwnerConfiguration {
        schema_version: SchemaV1,
        project_id: a.project,
        downstream_id: a.downstream,
        native_trader_id: "QZ-CAPITAL-PAPER-001".into(),
        native_account_id: "POLYMARKET-001".into(),
        native_client_id: "POLYMARKET".into(),
        native_version: "0.63.0".into(),
        venue: "POLYMARKET".into(),
        collateral_currency: "pUSD".into(),
        instrument_id: "fixture-event-101.POLYMARKET".into(),
        controlled_strategy_ids: vec!["QZ-PAPER-001".into()],
    };
    let (origin, listener) = client::listen_with_paper_owners(
        &a.f,
        server::paper_capital_exit::PaperCapitalExitOwners::new(vec![owner]).unwrap(),
    )
    .await;
    a.origin = origin;
    a._listener = listener;
    // A listener has its own signing key. Obtain its genuine browser cookie via
    // the existing test login, never forge or copy another router's cookie.
    let login = a
        .http
        .post(format!("{}/api/v2/auth/login", a.origin))
        .header("origin", &a.origin)
        .json(
            &json!({"schema_version":1,"password":"native-test-password","remember_device":false}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    a.browser_cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let directory = tempfile::tempdir().unwrap();
    let result_path = directory.path().join("paper-poller-result.json");
    let mut input = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
    input.write_all(&serde_json::to_vec(&json!({"origin":a.origin,"token":a.token,"browser_cookie":a.browser_cookie,"project":a.project,"downstream":a.downstream,"scenario":scenario,"result":result_path})).unwrap()).unwrap();
    input.as_file().sync_all().unwrap();
    // The root coordinator supplies Cargo's exact composed-source lib executable.
    // Do not rediscover it by globbing a shared target or compile inside this test.
    let executable=std::env::var_os("QZ_PAPER_SERVICE_TEST_BINARY").map(std::path::PathBuf::from)
        .expect("provide this composed candidate's Cargo-reported job --lib native-paper-test executable");
    assert!(executable.is_absolute() && executable.is_file());
    let child = tokio::process::Command::new(executable)
        .args([
            CHILD,
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
    let output = tokio::time::timeout(Duration::from_secs(90), child.wait_with_output())
        .await
        .expect("Paper Poller/PG fixture deadline")
        .unwrap();
    if !output.status.success() {
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        )
        .replace(&a.token, "[REDACTED]")
        .replace(&a.browser_cookie, "[REDACTED]");
        let mut begin = diagnostic.len().saturating_sub(8000);
        while !diagnostic.is_char_boundary(begin) {
            begin += 1;
        }
        panic!(
            "Paper Poller {scenario} fixture failed {}: {}",
            output.status,
            &diagnostic[begin..]
        );
    }
    let result: Value = serde_json::from_slice(&std::fs::read(result_path).unwrap()).unwrap();
    assert_eq!(
        result["actual_execution"],
        "PRODUCTION_POLLER_ORIGINAL_PAPER_ENGINE_FIXTURE"
    );
    assert_eq!(result["scientific_qualification"], "NOT_ASSESSED");
    assert_eq!(result["full_serve_host_exercised"], false);
    assert_eq!(result["withdrawal_performed"], false);
    assert_eq!(result["intent"]["funds"]["withdrawability"], "SIMULATED");
    assert_eq!(
        result["intent"]["funds"]["released_cash_amount"]["amount"],
        "850"
    );
    assert_ne!(result["intent"]["state"], "COMPLETED");
    let intent: Id = serde_json::from_value(result["intent"]["id"].clone()).unwrap();
    let source: Id = serde_json::from_value(result["intent"]["account_source_id"].clone()).unwrap();
    let original:Vec<Value>=sqlx::query_scalar("SELECT content FROM app.capital_exit_evidence WHERE intent_id=$1 ORDER BY external_message_id")
        .bind(intent.as_uuid()).fetch_all(&pool).await.unwrap();
    assert_eq!(
        original,
        result["original_evidence"].as_array().unwrap().clone(),
        "PG changed, lost or duplicated original native evidence"
    );
    let source_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.native_account_sources WHERE project_id=$1")
            .bind(a.project.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        source_count, 1,
        "unknown registration response created another source"
    );
    let intent_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.capital_exit_intents WHERE project_id=$1")
            .bind(a.project.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        intent_count, 1,
        "start or claim retry created another intent"
    );
    let reserved:String=sqlx::query_scalar("SELECT reserved_amount::text FROM app.managed_capital_reservations WHERE paper_account_source_id=$1")
        .bind(source.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        reserved.parse::<contracts::DecimalValue>().unwrap(),
        "850".parse().unwrap()
    );
    let fence: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT fence_evidence_id FROM app.capital_exit_intents WHERE id=$1")
            .bind(intent.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(fence.is_some(), "missing actual persisted fence");
    let attempts = result["attempts"].as_array().unwrap();
    for lost in attempts.iter().filter(|a| !a["withheld"].is_null()) {
        let replay = attempts
            .iter()
            .find(|a| {
                a["path"] == lost["path"]
                    && a["body"] == lost["body"]
                    && a["key"] == lost["key"]
                    && a["withheld"].is_null()
            })
            .unwrap();
        assert_eq!(replay["response"]["replayed"], true);
        assert_eq!(replay["response"]["resource"], lost["response"]["resource"]);
    }
    if scenario == "release" {
        assert_eq!(
            result["final_native"]["issued"].as_array().unwrap().len(),
            1,
            "lost replies created another native reduction"
        );
    } else if matches!(scenario, "pause_resume" | "cancel_resume") {
        let silence = &result["silence"];
        for key in ["clock", "quote", "original_frames", "position", "free_cash"] {
            assert_eq!(
                silence["before"][key], silence["after"][key],
                "silent source changed {key}"
            );
        }
        assert_eq!(
            silence["native_pending"]["evidence"]["phase"],
            "CANCELLING_EXIT"
        );
        assert!(silence["native_pending"]["evidence"]["released_cash_amount"].is_null());
        assert_eq!(
            silence["pending"]["funds"]["released_cash_amount"],
            silence["action_receipt"]["funds"]["released_cash_amount"]
        );
        assert!(silence["pending"]["funds"]["verified_withdrawable_amount"].is_null());
        assert_eq!(
            result["intent"]["owner_command"]["instruction"]["action"],
            "RESUME"
        );
    }
    if matches!(scenario, "cancel_resume" | "waiting_resume") {
        let late = &result["late_resume"];
        let expected = if scenario == "cancel_resume" {
            "CANCELLED_RESERVED"
        } else {
            "WAITING_EVIDENCE"
        };
        assert_eq!(late["from_state"], expected);
        assert_eq!(late["terminal_receipt"]["state"], expected);
        assert_ne!(
            late["original_command_id"],
            late["resume_receipt"]["command_id"]
        );
        assert_eq!(
            result["intent"]["command_id"],
            late["resume_receipt"]["command_id"]
        );
        assert_eq!(
            result["intent"]["owner_command"]["instruction"]["action"],
            "RESUME"
        );
        assert_eq!(
            result["intent"]["account_source_id"],
            late["terminal_receipt"]["account_source_id"]
        );
        assert_eq!(
            result["intent"]["managed_account_key"],
            late["terminal_receipt"]["managed_account_key"]
        );
        if scenario == "cancel_resume" {
            assert_eq!(late["post_terminal_discovery_observed"], true);
        } else {
            assert_eq!(late["repeated_availability_observed"], true);
            for key in ["issued", "position", "free_cash"] {
                assert_eq!(
                    late["native_before_resume"][key],
                    result["final_native"][key]
                );
            }
        }
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires this exact composed candidate's QZ_PAPER_SERVICE_TEST_BINARY"]
async fn production_paper_poller_pg_releases_with_original_receipts_after_lost_replies(
    pool: PgPool,
) {
    execute(pool, "release").await;
}
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires this exact composed candidate's QZ_PAPER_SERVICE_TEST_BINARY"]
async fn production_paper_poller_pg_silent_pause_then_active_resume_keeps_original_clock(
    pool: PgPool,
) {
    execute(pool, "pause_resume").await;
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires this exact composed candidate's QZ_PAPER_SERVICE_TEST_BINARY"]
async fn production_paper_poller_pg_late_cancelled_reserved_resume_uses_original_intent(
    pool: PgPool,
) {
    execute(pool, "cancel_resume").await;
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires this exact composed candidate's QZ_PAPER_SERVICE_TEST_BINARY"]
async fn production_paper_poller_pg_late_waiting_evidence_resume_preserves_released_cash(
    pool: PgPool,
) {
    execute(pool, "waiting_resume").await;
}
