//! Native HTTP authentication, immutable files and real PostgreSQL transactions.
//! Uploaded source/report text is deliberately NOT production evaluation evidence.
#[path = "support/artifact_cancellation.rs"]
mod artifact_cancellation;
#[path = "support/mission_attempt.rs"]
mod mission_attempt;
#[path = "../../../crates/store/tests/support/mod.rs"]
mod run_support;
mod support;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    response::Response,
};
use chrono::{Duration, Utc};
use contracts::{artifacts::MAX_UPLOAD_BYTES, Id};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use serde_json::{json, Value};
use server::{AppState, WebPolicy};
use sqlx::PgPool;
use std::sync::Arc;
use support::*;
use tokio::sync::Semaphore;
use tower::ServiceExt;
use tower_sessions::cookie::Key;

async fn setup(pool: &PgPool) -> (Fixture, String, Id, Arc<Semaphore>) {
    let mut f = fixture(pool.clone()).await;
    let root = f._state.path();
    let state = AppState::new(
        f.store.clone(),
        SecretVault::open(&root.join("secrets"), &root.join("master.key")).unwrap(),
        WebPolicy::new(
            "https://research.example",
            "127.0.0.1:8080".parse().unwrap(),
            false,
        )
        .unwrap(),
    )
    .with_artifact_store(ArtifactStore::open(&root.join("artifacts")).unwrap());
    let slots = state.artifact_slots.clone();
    f.app = server::router(state, Key::generate());
    let (e, cookie, native) = start(&f).await;
    let (login, _) = confirm(&f, &e, &cookie, &native, true).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap();
    let created = send(&f, "POST", "/api/v2/projects", "project", json!({"schema_version":1,"name":"artifacts","description":"native file checks","fork_from_project_id":null}), Some(&cookie), None).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let project = created.body["resource"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    (f, cookie, project, slots)
}
fn request(
    method: &str,
    path: &str,
    key: &str,
    value: Value,
    cookie: Option<&str>,
    bearer: Option<&str>,
) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "research.example")
        .header("idempotency-key", key);
    if let Some(cookie) = cookie {
        request = request
            .header(header::COOKIE, cookie)
            .header(header::ORIGIN, "https://research.example");
    }
    if let Some(bearer) = bearer {
        request = request.header(header::AUTHORIZATION, bearer);
    }
    let body = if value.is_null() {
        Body::empty()
    } else {
        request = request.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&value).unwrap())
    };
    request.body(body).unwrap()
}
async fn send(
    f: &Fixture,
    method: &str,
    path: &str,
    key: &str,
    value: Value,
    cookie: Option<&str>,
    bearer: Option<&str>,
) -> Reply {
    exchange(&f.app, request(method, path, key, value, cookie, bearer)).await
}
fn upload(project: Id, text: &str) -> Value {
    json!({"schema_version":1,"project_id":project,"kind":"CODE","content":text})
}
async fn bearer(
    f: &Fixture,
    cookie: &str,
    project: Id,
    scopes: &[&str],
    key: &str,
) -> (String, Id) {
    let p = send(f,"POST","/api/v2/machine-principals",key,json!({"schema_version":1,"name":"research CLI","kind":"CLI","project_id":project,"downstream_id":null,"enabled":true}),Some(cookie),None).await;
    assert_eq!(p.status, StatusCode::CREATED, "{}", p.body);
    let path = format!(
        "/api/v2/machine-principals/{}/credentials",
        p.body["resource"]["id"].as_str().unwrap()
    );
    let c = send(
        f,
        "POST",
        &path,
        &format!("{key}-credential"),
        json!({"schema_version":1,"scope_codes":scopes,"expires_at":Utc::now()+Duration::hours(1)}),
        Some(cookie),
        None,
    )
    .await;
    assert_eq!(c.status, StatusCode::CREATED, "{}", c.body);
    (
        format!("Bearer {}", c.body["token"].as_str().unwrap()),
        c.body["resource"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
            .try_into()
            .unwrap(),
    )
}
async fn raw(f: &Fixture, path: &str, cookie: &str) -> Response {
    f.app
        .clone()
        .oneshot(request(
            "GET",
            path,
            "unused",
            Value::Null,
            Some(cookie),
            None,
        ))
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn upload_download_and_same_length_replay_use_real_original_bytes(pool: PgPool) {
    let (f, cookie, project, _) = setup(&pool).await;
    let text = "// Original 中文\nfn signal() -> f64 { 0.0 }\n";
    let body = upload(project, text);
    let created = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "source",
        body.clone(),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let first = &created.body["resource"];
    assert_eq!(first["origin"], "SYNTHETIC");
    assert_eq!(first["access_class"], "RESEARCH");
    assert_eq!(first["kind"], "CODE");
    assert_eq!(first["byte_count"], text.len().to_string());
    assert!(first["producer_run_id"].is_null());
    for field in [
        "content",
        "storage_backend",
        "storage_object_ref",
        "storage_version",
        "credential_ref",
    ] {
        assert!(first.get(field).is_none());
    }
    let id = first["id"].as_str().unwrap();
    let response = raw(&f, &format!("/api/v2/artifacts/{id}/content"), &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/octet-stream"
    );
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(response.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("attachment;"));
    assert_eq!(
        &to_bytes(response.into_body(), MAX_UPLOAD_BYTES)
            .await
            .unwrap()[..],
        text.as_bytes()
    );
    let replay = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "source",
        body,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(replay.status, StatusCode::CREATED);
    assert_eq!(&replay.body["resource"], first);
    assert_eq!(replay.body["replayed"], true);
    let different = text.replace("0.0", "1.0");
    assert_eq!(different.len(), text.len());
    let rejected = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "source",
        upload(project, &different),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(rejected.status, StatusCode::CONFLICT);
    assert_eq!(rejected.body["code"], "IDEMPOTENCY_CONFLICT");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.artifacts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        std::fs::read_dir(f._state.path().join("artifacts"))
            .unwrap()
            .count(),
        1
    );
    let receipt:Value=sqlx::query_scalar("SELECT normalized_nonsecret_request FROM app.command_receipts WHERE operation='ARTIFACT_SUBMIT'").fetch_one(&pool).await.unwrap();
    assert!(receipt.get("content").is_none());
    assert!(!receipt.to_string().contains("signal"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn machine_scope_project_and_revocation_are_enforced_without_operator_grant(pool: PgPool) {
    let (f, cookie, project, _) = setup(&pool).await;
    let (read_only, _) = bearer(&f, &cookie, project, &["RESEARCH_READ"], "reader").await;
    assert_eq!(
        send(
            &f,
            "POST",
            "/api/v2/artifacts",
            "denied",
            upload(project, "source"),
            None,
            Some(&read_only)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let (writer, credential) = bearer(
        &f,
        &cookie,
        project,
        &["ARTIFACT_SUBMIT", "RESEARCH_READ"],
        "writer",
    )
    .await;
    let created = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "source",
        upload(project, "original"),
        None,
        Some(&writer),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let id = created.body["resource"]["id"].as_str().unwrap();
    assert_eq!(
        send(
            &f,
            "GET",
            &format!("/api/v2/artifacts/{id}"),
            "",
            Value::Null,
            None,
            Some(&read_only)
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &f,
            "POST",
            "/api/v2/artifacts",
            "other",
            upload(Id::new(), "original"),
            None,
            Some(&writer)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    let revoked = send(
        &f,
        "POST",
        &format!("/api/v2/machine-credentials/{credential}/revoke"),
        "revoke",
        json!({"schema_version":1,"reason":"remove upload access"}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(revoked.status, StatusCode::OK);
    assert_eq!(
        send(
            &f,
            "POST",
            "/api/v2/artifacts",
            "source",
            upload(project, "original"),
            None,
            Some(&writer)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &f,
            "GET",
            &format!("/api/v2/artifacts/{id}/content"),
            "",
            Value::Null,
            None,
            Some(&writer)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejects_forged_provenance_bad_documents_and_byte_overflow_before_publication(
    pool: PgPool,
) {
    let (f, cookie, project, _) = setup(&pool).await;
    for field in [
        "origin",
        "access_class",
        "storage_object_ref",
        "producer_run_id",
        "id",
    ] {
        let mut body = upload(project, "source");
        body[field] = json!("REAL");
        assert_eq!(
            send(
                &f,
                "POST",
                "/api/v2/artifacts",
                field,
                body,
                Some(&cookie),
                None
            )
            .await
            .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    for kind in [
        "METRICS",
        "PACKAGE",
        "TARGETS",
        "SIGNALS",
        "MODEL",
        "DATA_QUALITY",
    ] {
        let mut body = upload(project, "source");
        body["kind"] = json!(kind);
        assert_eq!(
            send(
                &f,
                "POST",
                "/api/v2/artifacts",
                kind,
                body,
                Some(&cookie),
                None
            )
            .await
            .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    for text in ["{}", "[]", "{\"schema_version\":2}"] {
        let body = json!({"schema_version":1,"project_id":project,"kind":"REPORT","content":text});
        assert_eq!(
            send(
                &f,
                "POST",
                "/api/v2/artifacts",
                "document",
                body,
                Some(&cookie),
                None
            )
            .await
            .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let large = "界".repeat(MAX_UPLOAD_BYTES / 3 + 1);
    assert_eq!(
        send(
            &f,
            "POST",
            "/api/v2/artifacts",
            "overflow",
            upload(project, &large),
            Some(&cookie),
            None
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.artifacts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        std::fs::read_dir(f._state.path().join("artifacts"))
            .unwrap()
            .count(),
        0
    );
    // This source is never executed or served with an active content type.
    let report =
        " {\"schema_version\":1,\"decision\":\"PASS\",\"explanation\":\"untrusted claim\"}\n";
    let accepted = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "report",
        json!({"schema_version":1,"project_id":project,"kind":"REPORT","content":report}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(accepted.status, StatusCode::CREATED);
    assert_eq!(accepted.body["resource"]["origin"], "SYNTHETIC");
    let qualifications: i64 = sqlx::query_scalar("SELECT count(*) FROM app.qualifications")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(qualifications, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn evaluator_only_objects_never_leak_through_general_artifact_routes(pool: PgPool) {
    let (f, cookie, project, _) = setup(&pool).await;
    let sealed = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json','fixture','1','LOCAL','sealed-private-location','1',10,'EVALUATOR_ONLY','FIXTURE','RUNTIME','AUDIT')")
        .bind(sealed.as_uuid()).bind(project.as_uuid()).execute(&pool).await.unwrap();
    let (reader, _) = bearer(&f, &cookie, project, &["RESEARCH_READ"], "reader").await;
    for path in [
        format!("/api/v2/artifacts/{sealed}"),
        format!("/api/v2/artifacts/{sealed}/content"),
    ] {
        assert_eq!(
            send(&f, "GET", &path, "", Value::Null, Some(&cookie), None)
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&f, "GET", &path, "", Value::Null, None, Some(&reader))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }
    let list = send(
        &f,
        "GET",
        &format!("/api/v2/artifacts?project_id={project}"),
        "",
        Value::Null,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(list.status, StatusCode::OK);
    assert!(list.body["items"].as_array().unwrap().is_empty());
    assert!(!list.body.to_string().contains("sealed-private-location"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn database_failure_preserves_orphan_without_publishing_a_false_receipt(pool: PgPool) {
    let (f, cookie, project, _) = setup(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION public.reject_artifact() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected metadata failure'; END $$; CREATE TRIGGER reject_upload BEFORE INSERT ON app.artifacts FOR EACH ROW EXECUTE FUNCTION public.reject_artifact();").execute(&pool).await.unwrap();
    let failed = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "retry",
        upload(project, "preserve uncertain object"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(!failed
        .body
        .to_string()
        .contains("injected metadata failure"));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE operation='ARTIFACT_SUBMIT'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        std::fs::read_dir(f._state.path().join("artifacts"))
            .unwrap()
            .count(),
        1
    );
    sqlx::query("DROP TRIGGER reject_upload ON app.artifacts")
        .execute(&pool)
        .await
        .unwrap();
    let retried = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "retry",
        upload(project, "preserve uncertain object"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(retried.status, StatusCode::CREATED);
    assert_eq!(retried.body["replayed"], false);
    assert_eq!(
        std::fs::read_dir(f._state.path().join("artifacts"))
            .unwrap()
            .count(),
        2
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_capacity_is_held_before_body_parse_and_until_download_disconnect(pool: PgPool) {
    let (f, cookie, project, slots) = setup(&pool).await;
    let reserved = slots.clone().acquire_many_owned(4).await.unwrap();
    let rejected = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "full",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(rejected.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(rejected.headers[header::RETRY_AFTER], "1");
    drop(reserved);
    let created = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "source",
        upload(project, "original"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let path = format!(
        "/api/v2/artifacts/{}/content",
        created.body["resource"]["id"].as_str().unwrap()
    );
    let mut responses = Vec::new();
    for _ in 0..4 {
        let response = raw(&f, &path, &cookie).await;
        assert_eq!(response.status(), StatusCode::OK);
        responses.push(response);
    }
    assert_eq!(slots.available_permits(), 0);
    let fifth = raw(&f, &path, &cookie).await;
    assert_eq!(fifth.status(), StatusCode::TOO_MANY_REQUESTS);
    responses.pop();
    assert_eq!(slots.available_permits(), 1);
    let next = raw(&f, &path, &cookie).await;
    assert_eq!(next.status(), StatusCode::OK);
    drop(next);
    drop(responses);
    assert_eq!(slots.available_permits(), 4);
}

async fn mission_token(
    f: &Fixture,
    pool: &PgPool,
    principal: Id,
    seconds: i32,
) -> (String, Id, Id) {
    mission_token_scoped(
        f,
        pool,
        principal,
        seconds,
        &["ARTIFACT_SUBMIT", "RESEARCH_READ"],
    )
    .await
}
async fn mission_token_scoped(
    f: &Fixture,
    pool: &PgPool,
    principal: Id,
    seconds: i32,
    scopes: &[&str],
) -> (String, Id, Id) {
    use integrations::authentication::{
        capability_verifier, format_machine_token, random_capability,
    };
    let secret = random_capability();
    let verifier = capability_verifier(&secret).unwrap();
    let root = f._state.path();
    let vault = SecretVault::open(&root.join("secrets"), &root.join("master.key")).unwrap();
    let reference = vault.put("MACHINE_VERIFIER", verifier.as_bytes()).unwrap();
    let public = Id::new();
    let credential = Id::new();
    sqlx::query("INSERT INTO app.machine_credentials(id,principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by) VALUES($1,$2,$3,$4,1,$6,clock_timestamp(),clock_timestamp()+$5*interval '1 second','MISSION_SERVICE')")
        .bind(credential.as_uuid()).bind(principal.as_uuid()).bind(public.to_string()).bind(reference.to_string()).bind(seconds).bind(scopes).execute(pool).await.unwrap();
    (
        format!("Bearer {}", format_machine_token(public, &secret).unwrap()),
        credential,
        reference,
    )
}
async fn mission(f: &Fixture, pool: &PgPool) -> (Id, Id, Id, Id) {
    use contracts::{lifecycle::JobLimitsV1, runs::RunKind, DbCounter, Revision, SchemaV1};
    use store::lifecycle::{ClaimResult, RunSubmission};
    let data = run_support::fixture(pool, run_support::budget()).await;
    let runtime = Id::new();
    sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled) VALUES($1,'artifact fixture','https://runtime.example','SYSTEM_CA','fixture',ARRAY['AGENT_RESEARCH'],'1',true)")
        .bind(runtime.as_uuid()).execute(pool).await.unwrap();
    let request = RunSubmission {
        cycle_id: data.cycle,
        input_set_id: data.input_set,
        runtime_id: runtime,
        runtime_revision: Revision::INITIAL,
        kind: RunKind::AgentResearch,
        limits: JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 1,
            cpu_seconds: DbCounter::new(100).unwrap(),
            wall_seconds: 3600,
            memory_mib: 1024,
            output_bytes: DbCounter::new(8).unwrap(),
        },
    };
    let run = f
        .store
        .enqueue_run("mission", &request)
        .await
        .unwrap()
        .resource;
    let message = f
        .store
        .read_run_messages(30, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == run.id)
        .unwrap();
    let ClaimResult::Leased(lease) = f
        .store
        .claim_run(&message, "artifact-test", 60)
        .await
        .unwrap()
    else {
        panic!("native lease expected")
    };
    let principal = Id::new();
    sqlx::query("INSERT INTO app.machine_principals(id,name,kind,project_id,run_id,enabled,credential_epoch) VALUES($1,'isolated researcher','MISSION',$2,$3,true,1)")
        .bind(principal.as_uuid()).bind(data.project.as_uuid()).bind(run.id.as_uuid()).execute(pool).await.unwrap();
    (data.project, run.id, lease.fence.attempt_id, principal)
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_mission_outputs_obey_one_budget_and_cannot_borrow_a_new_attempt(pool: PgPool) {
    let (f, _cookie, _, _) = setup(&pool).await;
    let (project, run, attempt, principal) = mission(&f, &pool).await;
    let (bearer, credential, _) = mission_token(&f, &pool, principal, 600).await;
    let stored: String = sqlx::query_scalar(
        "SELECT issuer_attempt_id::text FROM app.machine_credentials WHERE id=$1",
    )
    .bind(credential.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, attempt.to_string());
    let results = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(
            send(
                &f,
                "POST",
                "/api/v2/artifacts",
                "left",
                upload(project, "12345"),
                None,
                Some(&bearer)
            ),
            send(
                &f,
                "POST",
                "/api/v2/artifacts",
                "right",
                upload(project, "12345"),
                None,
                Some(&bearer)
            )
        )
    })
    .await
    .expect("two uploads must serialize, not deadlock upgrading shared locks");
    let (created, key, failed) = if results.0.status == StatusCode::CREATED {
        (results.0, "left", results.1)
    } else {
        (results.1, "right", results.0)
    };
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(
        failed.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        failed.body
    );
    assert_eq!(failed.body["code"], "BUDGET_EXHAUSTED");
    assert_eq!(failed.body["retryable"], false);
    assert_eq!(
        failed.body["field_errors"][0]["field"],
        "artifact_output_bytes"
    );
    assert_eq!(created.body["resource"]["producer_run_id"], run.to_string());
    assert_eq!(
        created.body["resource"]["producer_attempt_id"],
        attempt.to_string()
    );
    assert_eq!(created.body["resource"]["created_by"], "AGENT");
    let replay = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        key,
        upload(project, "12345"),
        None,
        Some(&bearer),
    )
    .await;
    assert_eq!(replay.status, StatusCode::CREATED);
    assert_eq!(replay.body["replayed"], true);
    let total: i64 = sqlx::query_scalar(
        "SELECT SUM(byte_count)::bigint FROM app.artifacts WHERE producer_run_id=$1",
    )
    .bind(run.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total, 5);

    // Relational fault injection only: represent a later Attempt. This does not
    // claim the unrelated runtime retry/orchestration service is implemented.
    let later = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,dispatch_state,runtime_state) VALUES($1,$2,2,'next-worker',1,clock_timestamp()+interval '1 hour','NOT_SENT','UNKNOWN')")
        .bind(later.as_uuid()).bind(run.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE app.runs SET active_attempt_id=$2,current_attempt_no=2 WHERE id=$1")
        .bind(run.as_uuid())
        .bind(later.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let denied = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "stale",
        upload(project, "x"),
        None,
        Some(&bearer),
    )
    .await;
    assert_eq!(denied.status, StatusCode::UNAUTHORIZED, "{}", denied.body);
    let (new_token, _, _) = mission_token(&f, &pool, principal, 600).await;
    let over = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "over",
        upload(project, "1234"),
        None,
        Some(&new_token),
    )
    .await;
    assert_eq!(over.status, StatusCode::TOO_MANY_REQUESTS, "{}", over.body);
    assert_eq!(over.body["code"], "BUDGET_EXHAUSTED");
    assert_eq!(over.body["retryable"], false);
    assert_eq!(
        over.body["field_errors"][0]["field"],
        "artifact_output_bytes"
    );
    let exact = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "exact",
        upload(project, "123"),
        None,
        Some(&new_token),
    )
    .await;
    assert_eq!(exact.status, StatusCode::CREATED, "{}", exact.body);
    assert_eq!(
        exact.body["resource"]["producer_attempt_id"],
        later.to_string()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn expiry_during_native_io_cannot_publish_after_authority_expires(pool: PgPool) {
    let (f, _, _, _) = setup(&pool).await;
    let (project, _run, _, principal) = mission(&f, &pool).await;
    let (_, credential, reference) = mission_token(&f, &pool, principal, 2).await;
    let actor = store::authority::Actor::Machine {
        credential_id: credential,
        verifier_ref: reference,
        operator_grant: None,
    };
    let request: contracts::artifacts::ArtifactCreate =
        serde_json::from_value(upload(project, "source")).unwrap();
    let prepared = f
        .store
        .prepare_artifact_upload(&actor, "expiry", &request)
        .await
        .unwrap();
    let id = prepared.id();
    let objects = ArtifactStore::open(&f._state.path().join("artifacts")).unwrap();
    objects.put(id, request.content.as_bytes()).unwrap();
    sqlx::query("SELECT pg_sleep(GREATEST(0,EXTRACT(EPOCH FROM expires_at-clock_timestamp()))+0.02) FROM app.machine_credentials WHERE id=$1")
        .bind(credential.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        prepared.publish().await,
        Err(store::StoreError::InvalidCredentials)
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.artifacts WHERE id=$1")
        .bind(id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(f
        ._state
        .path()
        .join("artifacts")
        .join(id.to_string())
        .is_file());
}
