//! Native CLI assumptions through actual HTTP/PG; controlled catalog/probe sources.
#[path = "../../../tests/support/execution_assumptions.rs"]
mod assumptions_support;
#[path = "support/client.rs"]
mod client;
mod support;
use axum::http::StatusCode;
use client::{browser, invoke, listen};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt};

#[sqlx::test(migrations = "../../migrations")]
async fn native_assumptions_cli_preserves_operator_intent_sources_and_original_receipt(
    pool: PgPool,
) {
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
    let objects = std::sync::Arc::new(
        integrations::artifacts::ArtifactStore::open(&f._state.path().join("artifacts")).unwrap(),
    );
    let data = assumptions_support::data::prepare(
        &pool,
        f.store.clone(),
        actor,
        None,
        tempfile::tempdir().unwrap(),
        objects,
    )
    .await;
    let (_data, mut request) = assumptions_support::prepare(&pool, data).await;
    request.rolling_liquidity = Some(contracts::science::NativeRollingBarLiquidityPolicyV1 {
        schema_version: contracts::SchemaV1,
        maximum_age_seconds: 3600,
        participation_limit: "0.123456789012345678".parse().unwrap(),
    });
    let body = serde_json::to_value(&request).unwrap();
    let principal = browser(&f, &cookie, "assumptions-cli", "/api/v2/machine-principals", json!({"schema_version":1,"name":"Assumptions CLI","kind":"CLI","project_id":request.project_id,"downstream_id":null,"enabled":true})).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = browser(&f, &cookie, "assumptions-token", &format!("/api/v2/machine-principals/{}/credentials", principal.body["resource"]["id"].as_str().unwrap()), json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let file = f._state.path().join("assumptions-cli-token");
    fs::write(&file, token).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let (origin, _listener) = listen(&f).await;
    let denied = invoke(
        &origin,
        &file,
        &[
            "--idempotency-key",
            "assumptions-create",
            "portfolio",
            "assumptions",
            "create",
        ],
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
    let human = invoke(&origin, &file, &["--idempotency-key","assumptions-human","operator-grant"], json!({"schema_version":1,"command":{"operation":"EXECUTION_ASSUMPTIONS_CREATE","request":body},"target_id":null,"code":totp.generate((now/30+1)*30)})).await;
    assert!(human.status.success(), "native assumptions grant failed");
    let grant: Value = serde_json::from_slice(&human.stdout).unwrap();
    let arguments = [
        "--idempotency-key",
        "assumptions-create",
        "--operator-grant",
        grant["resource"]["id"].as_str().unwrap(),
        "portfolio",
        "assumptions",
        "create",
    ];
    let mut original = Value::Null;
    for replay in [false, true] {
        let output = invoke(&origin, &file, &arguments, body.clone()).await;
        assert!(output.status.success(), "native assumptions create failed");
        assert!(output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
        let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(receipt["replayed"], replay);
        assert_eq!(receipt["resource"]["id"], grant["resource"]["target_id"]);
        assert_eq!(receipt["resource"]["settings"], body["settings"]);
        if replay {
            assert_eq!(receipt["resource"], original);
        } else {
            original = receipt["resource"].clone();
        }
    }
    let project = request.project_id.to_string();
    let listed = invoke(
        &origin,
        &file,
        &["portfolio", "assumptions", "list", &project, "--limit", "1"],
        Value::Null,
    )
    .await;
    assert!(listed.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&listed.stdout).unwrap()["items"],
        json!([original])
    );
    let read = invoke(
        &origin,
        &file,
        &[
            "portfolio",
            "assumptions",
            "show",
            original["id"].as_str().unwrap(),
        ],
        Value::Null,
    )
    .await;
    assert!(read.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&read.stdout).unwrap(),
        original
    );
    let mut changed = body;
    changed["settlement_rule_ref"] = "different".into();
    let denied = invoke(&origin, &file, &arguments, changed).await;
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&denied.stderr).unwrap()["status"],
        403
    );
}
