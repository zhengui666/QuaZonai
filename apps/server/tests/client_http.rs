//! Actual CLI -> TCP -> native Axum/Bearer/TOTP/PostgreSQL data administration.
//! Every credential and service here is disposable.
#[path = "../../../crates/store/tests/support/mod.rs"]
mod candidate_fixture;
#[path = "support/client.rs"]
mod client;
#[path = "../../../tests/support/mandate.rs"]
mod mandate_support;
mod support;
use axum::http::StatusCode;
use client::{browser, invoke, listen};
use contracts::Id;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt};

#[sqlx::test(migrations = "../../migrations")]
async fn candidate_cli_reads_original_snapshots_with_project_scope(pool: PgPool) {
    let f = support::fixture(pool.clone()).await;
    let (enrollment, initial, totp) = support::start(&f).await;
    let (confirmed, _) = support::confirm(&f, &enrollment, &initial, &totp, false).await;
    assert_eq!(confirmed.status, StatusCode::OK);
    let cookie = confirmed.cookie.unwrap_or(initial);
    let data = candidate_fixture::fixture(&pool, candidate_fixture::budget()).await;
    let (_, candidate, _) = candidate_fixture::portfolio(&pool, &data).await;
    let principal = browser(&f,&cookie,"candidate-reader","/api/v2/machine-principals",json!({"schema_version":1,"name":"Candidate reader","kind":"CLI","project_id":data.project,"downstream_id":null,"enabled":true})).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = browser(&f,&cookie,"candidate-token",&format!("/api/v2/machine-principals/{}/credentials",principal.body["resource"]["id"].as_str().unwrap()),json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let file = f._state.path().join("candidate-reader-token");
    fs::write(&file, credential.body["token"].as_str().unwrap()).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let (origin, _listener) = listen(&f).await;
    let result = invoke(
        &origin,
        &file,
        &["portfolio", "candidate", "show", &candidate.to_string()],
        Value::Null,
    )
    .await;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let detail: contracts::portfolio::CandidateDetailV1 =
        serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(detail.header.id, candidate);
    assert!(detail.members.is_empty());
    assert!(detail.targets.is_empty());
    let result = invoke(
        &origin,
        &file,
        &[
            "portfolio",
            "candidate",
            "list",
            &data.project.to_string(),
            "--limit",
            "1",
        ],
        Value::Null,
    )
    .await;
    assert!(result.status.success());
    let page: contracts::control::Page<contracts::portfolio::CandidateViewV1> =
        serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(page.items[0].id, candidate);
    let result = invoke(
        &origin,
        &file,
        &[
            "portfolio",
            "candidate",
            "evaluations",
            &candidate.to_string(),
            "--limit",
            "1",
        ],
        Value::Null,
    )
    .await;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let page: contracts::control::Page<contracts::evidence::EvaluationView> =
        serde_json::from_slice(&result.stdout).unwrap();
    assert!(page.items.is_empty());
    assert!(page.next_cursor.is_none());
    let foreign = candidate_fixture::fixture(&pool, candidate_fixture::budget()).await;
    let (_, foreign_candidate, _) = candidate_fixture::portfolio(&pool, &foreign).await;
    let foreign_candidate = foreign_candidate.to_string();
    let foreign_project = foreign.project.to_string();
    for command in ["observations", "wakes"] {
        let result = invoke(
            &origin,
            &file,
            &[
                "forward",
                command,
                &data.project.to_string(),
                "--limit",
                "1",
            ],
            Value::Null,
        )
        .await;
        assert!(result.status.success());
        let page: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(page["items"], json!([]));
        assert!(page["next_cursor"].is_null());
        let denied = invoke(
            &origin,
            &file,
            &["forward", command, &foreign_project],
            Value::Null,
        )
        .await;
        assert!(!denied.status.success());
        assert!(denied.stdout.is_empty());
    }
    for args in [
        ["portfolio", "candidate", "show", &foreign_candidate],
        ["portfolio", "candidate", "list", &foreign_project],
        ["portfolio", "candidate", "evaluations", &foreign_candidate],
    ] {
        let result = invoke(&origin, &file, &args, Value::Null).await;
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_cli_human_grant_source_creation_replay_and_intent_binding_are_real_transactions(
    pool: PgPool,
) {
    let f = support::fixture(pool.clone()).await;
    let (enrollment, initial, totp) = support::start(&f).await;
    let (confirmed, _) = support::confirm(&f, &enrollment, &initial, &totp, false).await;
    assert_eq!(confirmed.status, StatusCode::OK);
    let cookie = confirmed.cookie.unwrap_or(initial);
    let principal = browser(&f, &cookie, "cli-principal", "/api/v2/machine-principals", json!({
        "schema_version":1,"name":"Native CLI acceptance","kind":"CLI","project_id":null,"downstream_id":null,"enabled":true
    })).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = browser(&f, &cookie, "cli-credential", &format!("/api/v2/machine-principals/{}/credentials",principal.body["resource"]["id"].as_str().unwrap()), json!({
        "schema_version":1,"scope_codes":["DOCTOR_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)
    })).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let credential_file = f._state.path().join("native-cli-token");
    fs::write(&credential_file, token).unwrap();
    fs::set_permissions(&credential_file, fs::Permissions::from_mode(0o600)).unwrap();
    let runtime_secret = browser(
        &f,
        &cookie,
        "runtime-credential",
        "/api/v2/settings/credentials",
        json!({
            "intent":{"schema_version":1,"purpose":"RUNTIME","label":"Disposable native Runtime"},
            "value":integrations::authentication::random_capability()
        }),
    )
    .await;
    assert_eq!(runtime_secret.status, StatusCode::CREATED);
    let runtime = browser(&f, &cookie, "runtime", "/api/v2/integrations/runtimes", json!({
        "schema_version":1,"configuration":{"name":"Unprobed native registry","endpoint":"https://runtime.invalid","tls_policy":"SYSTEM_CA","allowed_capabilities":["DATA_VALIDATE"],"enabled":true,"development_http":false},
        "credential_ref":runtime_secret.body["resource"]["id"],"ca_certificate_ref":null
    })).await;
    assert_eq!(runtime.status, StatusCode::CREATED);

    // The same native Store and vault, with an explicitly allowed test-loopback
    // ingress. Browser enrollment above is real; this second listener uses only Bearer.
    let (origin, _listener) = listen(&f).await;
    let body = json!({"schema_version":1,"name":"Native CLI source","runtime_id":runtime.body["resource"]["id"],"native_catalog_ref":"registered/cli-fixture","provider_kind":"NAUTILUS_CATALOG","enabled":true});
    let denied = invoke(
        &origin,
        &credential_file,
        &[
            "--idempotency-key",
            "source-create",
            "data",
            "source",
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
    let human = invoke(&origin, &credential_file, &["--idempotency-key","source-human-grant","operator-grant"], json!({
        "schema_version":1,"command":{"operation":"DATA_SOURCE_CREATE","request":body},"target_id":null,"code":totp.generate((now/30+1)*30)
    })).await;
    assert!(human.status.success(), "native human grant failed");
    assert!(human.stderr.is_empty());
    let grant: Value = serde_json::from_slice(&human.stdout).unwrap();
    let grant_id = grant["resource"]["id"].as_str().unwrap();
    let expected_id = grant["resource"]["target_id"].as_str().unwrap();
    let command = &[
        "--idempotency-key",
        "source-create",
        "--operator-grant",
        grant_id,
        "data",
        "source",
        "create",
    ];
    let mut receipt = Value::Null;
    for replay in [false, true] {
        let output = invoke(&origin, &credential_file, command, body.clone()).await;
        assert!(output.status.success(), "native data source command failed");
        assert!(output.stderr.is_empty());
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains(token),
            "native CLI credential leaked"
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["replayed"], replay);
        assert_eq!(value["resource"]["id"], expected_id);
        if replay {
            assert_eq!(value["resource"], receipt);
        } else {
            receipt = value["resource"].clone();
        }
    }
    let listed = invoke(
        &origin,
        &credential_file,
        &["data", "source", "list", "--limit", "1"],
        Value::Null,
    )
    .await;
    assert!(listed.status.success());
    let listed: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(listed["items"][0], receipt);
    let mut conflict = body.clone();
    conflict["name"] = json!("Different immutable intent");
    let conflict = invoke(&origin, &credential_file, command, conflict).await;
    assert!(!conflict.status.success());
    assert!(conflict.stdout.is_empty());
    let error: Value = serde_json::from_slice(&conflict.stderr).unwrap();
    // The original one-time grant authorizes only the exact original intent.
    // The native authority gate rejects this different body before receipt lookup.
    assert_eq!(error["status"], 403);
    assert_eq!(error["code"], "FORBIDDEN");
    let runtime_id: Id = runtime.body["resource"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.data_sources WHERE runtime_id=$1 AND native_catalog_ref='registered/cli-fixture'")
        .bind(runtime_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    // Read-only Doctor identity did not acquire arbitrary project or research access.
    let private = invoke(&origin, &credential_file, &["project", "list"], Value::Null).await;
    assert!(!private.status.success());
    assert!(private.stdout.is_empty());
    let error: Value = serde_json::from_slice(&private.stderr).unwrap();
    assert_eq!(error["status"], 403);

    let project = browser(&f, &cookie, "alpha-project", "/api/v2/projects", json!({
        "schema_version":1,"name":"Native CLI Alpha reads","description":"Relational view, not scientific acceptance","fork_from_project_id":null
    })).await;
    assert_eq!(project.status, StatusCode::CREATED);
    let project = project.body["resource"]["id"].as_str().unwrap();
    let alpha = Id::new();
    sqlx::query("INSERT INTO app.alphas(id,project_id,name,lifecycle) VALUES($1,$2,'CLI read fixture','RESEARCH')")
        .bind(alpha.as_uuid()).bind(project.parse::<uuid::Uuid>().unwrap()).execute(&pool).await.unwrap();
    let arguments = ["alpha", "list", "--project-id", project, "--limit", "1"];
    let denied = invoke(&origin, &credential_file, &arguments, Value::Null).await;
    assert!(!denied.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&denied.stderr).unwrap()["status"],
        403
    );
    let principal = browser(&f, &cookie, "alpha-reader", "/api/v2/machine-principals", json!({
        "schema_version":1,"name":"Scoped Alpha reader","kind":"CLI","project_id":project,"downstream_id":null,"enabled":true
    })).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = browser(&f, &cookie, "alpha-read-token", &format!("/api/v2/machine-principals/{}/credentials", principal.body["resource"]["id"].as_str().unwrap()), json!({
        "schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)
    })).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let reader_file = f._state.path().join("alpha-read-token");
    fs::write(&reader_file, credential.body["token"].as_str().unwrap()).unwrap();
    fs::set_permissions(&reader_file, fs::Permissions::from_mode(0o600)).unwrap();
    let result = invoke(&origin, &reader_file, &arguments, Value::Null).await;
    assert!(result.status.success(), "native Alpha list failed");
    let list: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(list["items"][0]["id"], alpha.to_string());
    assert!(list["items"][0]["active_version"].is_null());
    let id = alpha.to_string();
    let versions = invoke(
        &origin,
        &reader_file,
        &["alpha", "versions", &id],
        Value::Null,
    )
    .await;
    assert!(versions.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&versions.stdout).unwrap()["items"],
        json!([])
    );
    for arguments in [
        vec!["alpha", "show", &id, "1"],
        vec!["alpha", "evaluations", &id],
        vec!["alpha", "calibration", &id],
        vec!["alpha", "qualifications", &id, "--limit", "1"],
        vec!["evidence", "show", &id],
        vec!["evidence", "metrics", &id, "--limit", "1"],
        vec!["cycle", "selection", &id],
        vec!["cycle", "trials", &id, "--limit", "1"],
    ] {
        let missing = invoke(&origin, &reader_file, &arguments, Value::Null).await;
        assert!(!missing.status.success());
        assert!(missing.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&missing.stderr).unwrap()["status"],
            404
        );
    }
    let invalid = invoke(
        &origin,
        &reader_file,
        &["alpha", "show", &id, "0"],
        Value::Null,
    )
    .await;
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_mandate_cli_uses_original_human_grant_and_scoped_immutable_reads(pool: PgPool) {
    let f = support::fixture(pool.clone()).await;
    let (enrollment, initial, totp) = support::start(&f).await;
    let (confirmed, _) = support::confirm(&f, &enrollment, &initial, &totp, false).await;
    assert_eq!(confirmed.status, StatusCode::OK);
    let cookie = confirmed.cookie.unwrap_or(initial);
    let login: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.browser_logins ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let actor = store::authority::Actor::Browser {
        login_id: login.to_string().try_into().unwrap(),
    };
    let request = mandate_support::request(&pool, &f.store, &actor).await;
    let body = serde_json::to_value(&request).unwrap();
    let principal = browser(&f, &cookie, "mandate-cli", "/api/v2/machine-principals", json!({
        "schema_version":1,"name":"Scoped Mandate CLI","kind":"CLI","project_id":request.project_id,"downstream_id":null,"enabled":true
    })).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = browser(&f, &cookie, "mandate-token", &format!("/api/v2/machine-principals/{}/credentials", principal.body["resource"]["id"].as_str().unwrap()), json!({
        "schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)
    })).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let file = f._state.path().join("mandate-cli-token");
    fs::write(&file, token).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let (origin, _listener) = listen(&f).await;
    let denied = invoke(
        &origin,
        &file,
        &[
            "--idempotency-key",
            "mandate-create",
            "portfolio",
            "mandate",
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
    let human = invoke(&origin, &file, &["--idempotency-key","mandate-human","operator-grant"], json!({
        "schema_version":1,"command":{"operation":"MANDATE_CREATE","request":body},"target_id":null,"code":totp.generate((now/30+1)*30)
    })).await;
    assert!(human.status.success(), "native Mandate grant failed");
    let grant: Value = serde_json::from_slice(&human.stdout).unwrap();
    let grant_id = grant["resource"]["id"].as_str().unwrap();
    let target = grant["resource"]["target_id"].as_str().unwrap();
    let arguments = [
        "--idempotency-key",
        "mandate-create",
        "--operator-grant",
        grant_id,
        "portfolio",
        "mandate",
        "create",
    ];
    let mut original = Value::Null;
    for replay in [false, true] {
        let output = invoke(&origin, &file, &arguments, body.clone()).await;
        assert!(output.status.success(), "native Mandate command failed");
        assert!(output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
        let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(receipt["replayed"], replay);
        assert_eq!(receipt["resource"]["id"], target);
        assert_eq!(receipt["resource"]["content"], body["content"]);
        if replay {
            assert_eq!(receipt["resource"], original);
        } else {
            original = receipt["resource"].clone();
        }
    }
    let mut changed = body;
    changed["content"]["exposure_tolerance"] = json!("0.00001");
    let denied = invoke(&origin, &file, &arguments, changed).await;
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&denied.stderr).unwrap()["status"],
        403
    );
    let project = request.project_id.to_string();
    let listed = invoke(
        &origin,
        &file,
        &["portfolio", "mandate", "list", &project, "--limit", "1"],
        Value::Null,
    )
    .await;
    assert!(!String::from_utf8_lossy(&listed.stderr).contains(token));
    assert!(
        listed.status.success(),
        "Mandate list: {}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&listed.stdout).unwrap()["items"],
        json!([original])
    );
    let read = invoke(
        &origin,
        &file,
        &["portfolio", "mandate", "show", target],
        Value::Null,
    )
    .await;
    assert!(read.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&read.stdout).unwrap(),
        original
    );
    let other = mandate_support::request(&pool, &f.store, &actor).await;
    let other = f
        .store
        .create_mandate(&actor, "other-project", &other)
        .await
        .unwrap()
        .resource;
    let other_project = other.project_id.to_string();
    let other_id = other.id.to_string();
    for arguments in [
        vec!["portfolio", "mandate", "list", &other_project],
        vec!["portfolio", "mandate", "show", &other_id],
    ] {
        let denied = invoke(&origin, &file, &arguments, Value::Null).await;
        assert!(!denied.status.success());
        assert!(denied.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&denied.stderr).unwrap()["status"],
            404
        );
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.portfolio_mandates WHERE project_id=$1")
            .bind(request.project_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}
