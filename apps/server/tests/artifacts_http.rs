//! Native HTTP authentication, immutable files and real PostgreSQL transactions.
//! Uploaded source/report text is deliberately NOT production evaluation evidence.
#[path = "support/artifact_cancellation.rs"]
mod artifact_cancellation;
#[path = "support/mission_attempt.rs"]
mod mission_attempt;
#[path = "support/mission_owner_wait.rs"]
mod mission_owner_wait;
#[path = "../../../crates/store/tests/support/mod.rs"]
mod run_support;
mod support;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    response::Response,
};
use chrono::{Duration, Utc};
use contracts::Id;
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use serde_json::{json, Value};
use server::{AppState, WebPolicy};
use sqlx::PgPool;
use support::*;
use tower::ServiceExt;
use tower_sessions::cookie::Key;

async fn setup(pool: &PgPool) -> (Fixture, String, Id) {
    let mut f = fixture(pool.clone()).await;
    let root = f._state.path();
    let state = AppState::new(
        f.store.clone(),
        SecretVault::open(&root.join("secrets"), &root.join("master.key")).unwrap(),
        WebPolicy::new(
            "https://localhost",
            "127.0.0.1:8080".parse().unwrap(),
            false,
        )
        .unwrap(),
    )
    .with_artifact_store(ArtifactStore::open(&root.join("artifacts")).unwrap());
    f.app = server::router(state, Key::generate());
    let login = local_session(&f).await;
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
    (f, cookie, project)
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
        .header(header::HOST, "localhost")
        .header("idempotency-key", key);
    if let Some(cookie) = cookie {
        request = request
            .header(header::COOKIE, cookie)
            .header(header::ORIGIN, "https://localhost");
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
    bearer_until(
        f,
        cookie,
        project,
        scopes,
        key,
        Utc::now() + Duration::hours(1),
    )
    .await
}
async fn bearer_until(
    f: &Fixture,
    cookie: &str,
    project: Id,
    scopes: &[&str],
    key: &str,
    expires_at: chrono::DateTime<Utc>,
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
        json!({"schema_version":1,"scope_codes":scopes,"expires_at":expires_at}),
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
    let (f, cookie, project) = setup(&pool).await;
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
        &to_bytes(response.into_body(), usize::MAX).await.unwrap()[..],
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
    let (f, cookie, project) = setup(&pool).await;
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
    let (f, cookie, project) = setup(&pool).await;
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
    let large = "界".repeat(2 * 1024 * 1024 / 3 + 1);
    let created = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "large-original",
        upload(project, &large),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(
        created.body["resource"]["byte_count"],
        large.len().to_string()
    );
    let id = created.body["resource"]["id"].as_str().unwrap();
    let response = raw(&f, &format!("/api/v2/artifacts/{id}/content"), &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        large.as_bytes()
    );
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
    let (f, cookie, project) = setup(&pool).await;
    let sealed = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json','fixture','1','LOCAL','sealed-private-location','1',10,'EVALUATOR_ONLY','FIXTURE','RUNTIME','AUDIT')")
        .bind(sealed.as_uuid()).bind(project.as_uuid()).execute(&pool).await.unwrap();
    let (reader, _) = bearer(&f, &cookie, project, &["RESEARCH_READ"], "reader").await;
    for path in [
        format!("/api/v2/artifacts/{sealed}"),
        format!("/api/v2/artifacts/{sealed}/content"),
        format!("/api/v2/artifacts/{sealed}/agent-evaluation"),
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
    let (f, cookie, project) = setup(&pool).await;
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
async fn artifact_downloads_cross_four_concurrent_streams_without_rejecting_uploads(pool: PgPool) {
    let (f, cookie, project) = setup(&pool).await;
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
    for _ in 0..5 {
        let response = raw(&f, &path, &cookie).await;
        assert_eq!(response.status(), StatusCode::OK);
        responses.push(response);
    }
    let next = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "parallel-upload",
        upload(project, "next"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(next.status, StatusCode::CREATED);
    responses.pop();
    for response in responses {
        assert_eq!(
            to_bytes(response.into_body(), 64).await.unwrap().as_ref(),
            b"original"
        );
    }
    let replay = raw(&f, &path, &cookie).await;
    assert_eq!(
        to_bytes(replay.into_body(), 64).await.unwrap().as_ref(),
        b"original"
    );
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

async fn mission(_f: &Fixture, pool: &PgPool) -> (Id, Id, Id, Id) {
    // Original Run/Attempt/admission facts for historical credential and file
    // regressions. Never call today's retired Agent admission or claim methods.
    let data = run_support::fixture(pool, run_support::budget()).await;
    let (runtime, run, attempt, principal) = (Id::new(), Id::new(), Id::new(), Id::new());
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled) VALUES($1,'historical artifact fixture','https://runtime.example','SYSTEM_CA','fixture',ARRAY['AGENT_RESEARCH'],'1',true)")
        .bind(runtime.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.runs(id,project_id,cycle_id,kind,input_set_id,state,deadline_at,queued_at,started_at,current_attempt_no,active_attempt_id) VALUES($1,$2,$3,'AGENT_RESEARCH',$4,'RUNNING',clock_timestamp()+interval '1 hour',clock_timestamp(),clock_timestamp(),1,$5)")
        .bind(run.as_uuid()).bind(data.project.as_uuid()).bind(data.cycle.as_uuid()).bind(data.input_set.as_uuid()).bind(attempt.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,dispatch_state,runtime_state) VALUES($1,$2,1,'historical-artifact-owner',1,clock_timestamp()+interval '1 hour','ACKNOWLEDGED','RUNNING')")
        .bind(attempt.as_uuid()).bind(run.as_uuid()).execute(&mut *tx).await.unwrap();
    let message: i64 = sqlx::query_scalar("SELECT pgmq.send('runs',$1)")
        .bind(json!({"schema_version":1,"run_id":run}))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app.run_admissions(run_id,project_id,cycle_id,command_key,normalized_request,initial_snapshot,limits,runtime_id,runtime_revision,runtime_snapshot,initial_queue_message_id) VALUES($1,$2,$3,'historical-artifact-admission',$4,$4,$5,$6,1,$4,$7)")
        .bind(run.as_uuid()).bind(data.project.as_uuid()).bind(data.cycle.as_uuid())
        .bind(json!({"schema_version":1,"historical_fixture":true}))
        .bind(json!({"schema_version":1,"experiments":0,"cpu_seconds":"100","wall_seconds":3600,"memory_mib":1024,"output_bytes":"8"}))
        .bind(runtime.as_uuid()).bind(message).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.machine_principals(id,name,kind,project_id,run_id,enabled,credential_epoch) VALUES($1,'historical researcher fixture','MISSION',$2,$3,true,1)")
        .bind(principal.as_uuid()).bind(data.project.as_uuid()).bind(run.as_uuid()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    (data.project, run, attempt, principal)
}

async fn historical_artifact(
    f: &Fixture,
    pool: &PgPool,
    project: Id,
    run: Id,
    attempt: Id,
    content: &str,
) -> Id {
    // Original Agent provenance is a test-only historical fact, not a new upload.
    let id = Id::new();
    ArtifactStore::open(&f._state.path().join("artifacts"))
        .unwrap()
        .put(id, content.as_bytes())
        .unwrap();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,'CODE','text/x-rust','qz.rust_source','1','LOCAL',$5,'1',$6,'RESEARCH','SYNTHETIC','AGENT','REFERENCED')")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(run.as_uuid()).bind(attempt.as_uuid())
        .bind(id.to_string()).bind(content.len() as i64).execute(pool).await.unwrap();
    id
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_cli_uploads_publish_one_original_receipt_without_legacy_provenance(
    pool: PgPool,
) {
    let (f, cookie, project) = setup(&pool).await;
    let (token, _) = bearer(
        &f,
        &cookie,
        project,
        &["ARTIFACT_SUBMIT", "RESEARCH_READ"],
        "concurrent-cli",
    )
    .await;
    let (left, right) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(
            send(
                &f,
                "POST",
                "/api/v2/artifacts",
                "one-original",
                upload(project, "12345"),
                None,
                Some(&token)
            ),
            send(
                &f,
                "POST",
                "/api/v2/artifacts",
                "one-original",
                upload(project, "12345"),
                None,
                Some(&token)
            ),
        )
    })
    .await
    .expect("concurrent uploads must serialize without a lock upgrade deadlock");
    for reply in [&left, &right] {
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
        assert_eq!(reply.body["resource"]["created_by"], "OPERATOR");
        assert!(reply.body["resource"]["producer_run_id"].is_null());
        assert!(reply.body["resource"]["producer_attempt_id"].is_null());
    }
    assert_eq!(left.body["resource"], right.body["resource"]);
    assert_ne!(left.body["replayed"], right.body["replayed"]);
    let total: (i64, i64) = sqlx::query_as(
        "SELECT count(*),SUM(byte_count)::bigint FROM app.artifacts WHERE project_id=$1",
    )
    .bind(project.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total, (1, 5));
    let path = format!(
        "/api/v2/artifacts/{}/content",
        left.body["resource"]["id"].as_str().unwrap()
    );
    assert_eq!(
        to_bytes(raw(&f, &path, &cookie).await.into_body(), 64)
            .await
            .unwrap()
            .as_ref(),
        b"12345"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn retired_mission_uploads_cannot_change_history_or_borrow_a_new_attempt(pool: PgPool) {
    let (f, _cookie, _) = setup(&pool).await;
    let (project, run, attempt, principal) = mission(&f, &pool).await;
    let artifact = historical_artifact(&f, &pool, project, run, attempt, "12345").await;
    let (bearer, credential, _) = mission_token(&f, &pool, principal, 600).await;
    let stored: String = sqlx::query_scalar(
        "SELECT issuer_attempt_id::text FROM app.machine_credentials WHERE id=$1",
    )
    .bind(credential.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, attempt.to_string());
    let original = send(
        &f,
        "GET",
        &format!("/api/v2/artifacts/{artifact}"),
        "unused",
        Value::Null,
        None,
        Some(&bearer),
    )
    .await;
    assert_eq!(original.status, StatusCode::OK);
    assert_eq!(original.body["producer_run_id"], run.to_string());
    assert_eq!(original.body["producer_attempt_id"], attempt.to_string());
    assert_eq!(original.body["created_by"], "AGENT");
    let (left, right) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
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
            ),
        )
    })
    .await
    .expect("retired uploads must not deadlock");
    for reply in [&left, &right] {
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
        assert_eq!(reply.body["code"], "FORBIDDEN");
        assert_eq!(reply.body["retryable"], false);
    }
    // Original later-Attempt facts do not revive the retired writer.
    let later = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,dispatch_state,runtime_state) VALUES($1,$2,2,'historical-next-worker',1,clock_timestamp()+interval '1 hour','NOT_SENT','UNKNOWN')")
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
    assert_eq!(denied.status, StatusCode::FORBIDDEN, "{}", denied.body);
    let identity = send(
        &f,
        "GET",
        "/api/v2/auth/machine",
        "unused",
        Value::Null,
        None,
        Some(&bearer),
    )
    .await;
    assert_eq!(identity.status, StatusCode::UNAUTHORIZED);
    let (new_token, _, _) = mission_token(&f, &pool, principal, 600).await;
    for text in ["1234", "123"] {
        let denied = send(
            &f,
            "POST",
            "/api/v2/artifacts",
            text,
            upload(project, text),
            None,
            Some(&new_token),
        )
        .await;
        assert_eq!(denied.status, StatusCode::FORBIDDEN, "{}", denied.body);
    }
    let current = send(
        &f,
        "GET",
        &format!("/api/v2/artifacts/{artifact}"),
        "unused",
        Value::Null,
        None,
        Some(&new_token),
    )
    .await;
    assert_eq!(current.status, StatusCode::OK);
    assert_eq!(current.body, original.body);
    let total: (i64, i64) = sqlx::query_as(
        "SELECT count(*),SUM(byte_count)::bigint FROM app.artifacts WHERE producer_run_id=$1",
    )
    .bind(run.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total, (1, 5));
    let limit: Value = sqlx::query_scalar("SELECT limits FROM app.run_admissions WHERE run_id=$1")
        .bind(run.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(limit["output_bytes"], "8");
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM app.command_receipts WHERE operation='ARTIFACT_SUBMIT' AND normalized_nonsecret_request->>'project_id'=$1")
        .bind(project.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(receipts, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn expiry_during_native_io_cannot_publish_after_authority_expires(pool: PgPool) {
    let (f, cookie, project) = setup(&pool).await;
    let (_, credential) = bearer_until(
        &f,
        &cookie,
        project,
        &["ARTIFACT_SUBMIT"],
        "expiry-cli",
        Utc::now() + Duration::seconds(2),
    )
    .await;
    let reference: String =
        sqlx::query_scalar("SELECT verifier_ref FROM app.machine_credentials WHERE id=$1")
            .bind(credential.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let reference = Id::try_from(reference).unwrap();
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

fn agent_report() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/agent-evaluation/unrun-v1.json"
    ))
    .unwrap()
}
fn report_upload(project: Id, report: &Value) -> Value {
    json!({"schema_version":1,"project_id":project,"kind":"REPORT","content":report.to_string()})
}
#[sqlx::test(migrations = "../../migrations")]
async fn agent_report_retains_immutable_bytes_and_replay(pool: PgPool) {
    let (f, cookie, project) = setup(&pool).await;
    let mut report = agent_report();
    report["recorded_at"] = json!("2026-9-30T00:00:00Z");
    report["status"] = json!("BLOCKED");
    report["cases"][0]["status"] = json!("BLOCKED");
    report["cases"][0]["measurements"] = json!({"input_tokens":"9007199254740993","output_tokens":"0","elapsed_ms":null,"tool_calls":"9007199254740993","cost":{"amount":"0.000000000000000001","currency":"EUR"}});
    let body = report_upload(project, &report);
    let created = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "agent-report",
        body.clone(),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let replay = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "agent-report",
        body,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(replay.body["resource"], created.body["resource"]);
    assert_eq!(replay.body["replayed"], true);
    let id = created.body["resource"]["id"].as_str().unwrap();
    let response = raw(
        &f,
        &format!("/api/v2/artifacts/{id}/agent-evaluation"),
        &cookie,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let typed: Value = serde_json::from_slice(&bytes).unwrap();
    let mut expected = report.clone();
    expected["recorded_at"] = json!("2026-09-30T00:00:00Z");
    assert_eq!(typed, expected);
    let original = raw(&f, &format!("/api/v2/artifacts/{id}/content"), &cookie).await;
    assert_eq!(original.status(), StatusCode::OK);
    let original = to_bytes(original.into_body(), usize::MAX).await.unwrap();
    assert_eq!(original.as_ref(), report.to_string().as_bytes());
    let qualifications: i64 = sqlx::query_scalar("SELECT count(*) FROM app.qualifications")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(qualifications, 0);
}
#[sqlx::test(migrations = "../../migrations")]
async fn agent_report_rejects_forged_pass_and_unsupported_documents(pool: PgPool) {
    let (f, cookie, project) = setup(&pool).await;
    for (index, (pointer, replacement)) in [
        ("/status", json!("PASS")),
        ("/recorded_at", json!("+10000-01-01T00:00:00Z")),
        ("/recorded_at", json!("2026-09-30T12:00:60Z")),
        (
            "/cases/0/measurements/input_tokens",
            json!(9007199254740993_u64),
        ),
        (
            "/held_out/sha256",
            agent_report()["tuning"]["sha256"].clone(),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut report = agent_report();
        *report.pointer_mut(pointer).unwrap() = replacement;
        let rejected = send(
            &f,
            "POST",
            "/api/v2/artifacts",
            &format!("invalid-report-{index}"),
            report_upload(project, &report),
            Some(&cookie),
            None,
        )
        .await;
        assert_eq!(
            rejected.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            rejected.body
        );
    }
    let mut report = agent_report();
    report["forged_qualification"] = json!("PASS");
    let rejected = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "unknown-field",
        report_upload(project, &report),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(rejected.status, StatusCode::UNPROCESSABLE_ENTITY);
    let ordinary = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "ordinary",
        report_upload(
            project,
            &json!({"schema_version":1,"note":"generic report remains supported"}),
        ),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(ordinary.status, StatusCode::CREATED);
    let id = ordinary.body["resource"]["id"].as_str().unwrap();
    let response = raw(
        &f,
        &format!("/api/v2/artifacts/{id}/agent-evaluation"),
        &cookie,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
#[sqlx::test(migrations = "../../migrations")]
async fn agent_report_read_enforces_project_scope_and_revocation(pool: PgPool) {
    let (f, cookie, project) = setup(&pool).await;
    let created = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        "report",
        report_upload(project, &agent_report()),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let id = created.body["resource"]["id"].as_str().unwrap();
    let path = format!("/api/v2/artifacts/{id}/agent-evaluation");
    assert_eq!(
        send(&f, "GET", &path, "", Value::Null, None, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let (reader, credential) =
        bearer(&f, &cookie, project, &["RESEARCH_READ"], "report-reader").await;
    assert_eq!(
        send(&f, "GET", &path, "", Value::Null, None, Some(&reader))
            .await
            .status,
        StatusCode::OK
    );
    let other=send(&f,"POST","/api/v2/projects","other-project",json!({"schema_version":1,"name":"other","description":"scope fixture","fork_from_project_id":null}),Some(&cookie),None).await;
    let other_id = other.body["resource"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    let (other_reader, _) = bearer(&f, &cookie, other_id, &["RESEARCH_READ"], "other-reader").await;
    assert_eq!(
        send(&f, "GET", &path, "", Value::Null, None, Some(&other_reader))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let mut original_reads = Vec::new();
    for _ in 0..5 {
        let response = raw(&f, &path, &cookie).await;
        assert_eq!(response.status(), StatusCode::OK);
        original_reads.push(response);
    }
    drop(original_reads);
    let revoked = send(
        &f,
        "POST",
        &format!("/api/v2/machine-credentials/{credential}/revoke"),
        "revoke-report-reader",
        json!({"schema_version":1,"reason":"read test complete"}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(revoked.status, StatusCode::OK);
    assert_eq!(
        send(&f, "GET", &path, "", Value::Null, None, Some(&reader))
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn agent_report_and_binary_reads_share_backend_unavailable_mapping(pool: PgPool) {
    let (f, cookie, project) = setup(&pool).await;
    let id = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json','fixture','1','OBJECT_STORE','unsupported-test-object','1',10,'RESEARCH','SYNTHETIC','OPERATOR','AUDIT')")
        .bind(id.as_uuid()).bind(project.as_uuid()).execute(&pool).await.unwrap();
    for suffix in ["content", "agent-evaluation"] {
        let reply = send(
            &f,
            "GET",
            &format!("/api/v2/artifacts/{id}/{suffix}"),
            "",
            Value::Null,
            Some(&cookie),
            None,
        )
        .await;
        assert_eq!(reply.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(reply.body["code"], "ARTIFACT_BACKEND_UNAVAILABLE");
    }
}
