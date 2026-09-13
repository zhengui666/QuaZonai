//! Native CLI/TCP/PG authentication and failed admission; not positive qualification.
#[path = "support/client.rs"]
mod client;
#[path = "../../../tests/support/mandate.rs"]
mod mandate_support;
mod support;
use axum::http::StatusCode;
use client::{browser, invoke, listen};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt};

#[sqlx::test(migrations = "../../migrations")]
async fn portfolio_cli_requires_exact_human_intent_and_never_admits_a_missing_cycle(pool: PgPool) {
    let f = support::fixture(pool.clone()).await;
    let (enrollment, initial, totp) = support::start(&f).await;
    let (login, _) = support::confirm(&f, &enrollment, &initial, &totp, false).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap_or(initial);
    let login_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.browser_logins ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let actor = store::authority::Actor::Browser {
        login_id: login_id.to_string().try_into().unwrap(),
    };
    let source = mandate_support::request(&pool, &f.store, &actor).await;
    let mandate = f
        .store
        .create_mandate(&actor, "mandate", &source)
        .await
        .unwrap()
        .resource;
    let principal = browser(&f, &cookie, "build-cli", "/api/v2/machine-principals", json!({"schema_version":1,"name":"Portfolio CLI","kind":"CLI","project_id":source.project_id,"downstream_id":null,"enabled":true})).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = browser(&f, &cookie, "build-token", &format!("/api/v2/machine-principals/{}/credentials", principal.body["resource"]["id"].as_str().unwrap()), json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let file = f._state.path().join("portfolio-cli-token");
    fs::write(&file, token).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let body = json!({"schema_version":1,"cycle_id":contracts::Id::new(),"mandate_id":mandate.id,
        "input_set_id":contracts::Id::new(),"runtime_id":source.runtime_id,"expected_runtime_revision":source.expected_runtime_revision,
        "current_weights_source":{"kind":"FORWARD_SNAPSHOT","snapshot_id":contracts::Id::new()},"environment":"PAPER",
        "members":[{"qualification_id":contracts::Id::new(),"ensemble_weight":"0.5"},{"qualification_id":contracts::Id::new(),"ensemble_weight":"0.5"}],
        "limits":{"schema_version":1,"experiments":0,"cpu_seconds":"10","wall_seconds":10,"memory_mib":64,"output_bytes":"1024"}});
    let (origin, _listener) = listen(&f).await;
    let denied = invoke(
        &origin,
        &file,
        &["--idempotency-key", "build", "portfolio", "build"],
        body.clone(),
    )
    .await;
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let human = invoke(&origin, &file, &["--idempotency-key","build-human","operator-grant"], json!({"schema_version":1,"command":{"operation":"PORTFOLIO_BUILD","request":body},"target_id":mandate.id,"code":totp.generate((now/30+1)*30)})).await;
    assert!(human.status.success(), "portfolio human intent failed");
    let grant: Value = serde_json::from_slice(&human.stdout).unwrap();
    let arguments = [
        "--idempotency-key",
        "build",
        "--operator-grant",
        grant["resource"]["id"].as_str().unwrap(),
        "portfolio",
        "build",
    ];
    for _ in 0..2 {
        let rejected = invoke(&origin, &file, &arguments, body.clone()).await;
        assert!(!rejected.status.success());
        assert!(rejected.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&rejected.stderr).contains(token));
        assert_eq!(
            serde_json::from_slice::<Value>(&rejected.stderr).unwrap()["status"],
            422
        );
    }
    let mut changed = body;
    changed["cycle_id"] = json!(contracts::Id::new());
    let rejected = invoke(&origin, &file, &arguments, changed).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&rejected.stderr).unwrap()["status"],
        403
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.runs WHERE kind='PORTFOLIO_BUILD'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}
