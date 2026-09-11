//! Real TOTP/Axum/PostgreSQL/ArtifactStore + actual TLS metadata transport.
//! The returned source document is explicitly FIXTURE and cannot create qualification.
#[path = "../../../tests/support/catalog_metadata.rs"]
mod metadata_fixture;
#[path = "support/runtime_native.rs"]
mod native;
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use serde_json::{json, Value};
use server::runtime_transport::{RuntimeTarget, RuntimeTargets};
use sqlx::PgPool;
use std::sync::{atomic::Ordering, Arc};
use support::{Fixture, Reply};

async fn command(
    f: &Fixture,
    cookie: &str,
    key: &str,
    method: &str,
    path: &str,
    body: Value,
) -> Reply {
    support::exchange(
        &f.app,
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "research.example")
            .header(header::ORIGIN, "https://research.example")
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", key)
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_fresh_authenticated_operator_can_distinguish_empty_management_from_unavailable(
    pool: PgPool,
) {
    let f = support::fixture(pool).await;
    let (enrollment, initial_cookie, totp) = support::start(&f).await;
    let (confirmation, _) = support::confirm(&f, &enrollment, &initial_cookie, &totp, false).await;
    assert_eq!(confirmation.status, StatusCode::OK);
    let cookie = confirmation.cookie.unwrap_or(initial_cookie);
    for path in [
        "/api/v2/data/sources",
        "/api/v2/data/revisions",
        "/api/v2/data/universes",
    ] {
        let reply = command(&f, &cookie, "read-empty", "GET", path, Value::Null).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.body["items"], json!([]));
        assert!(reply.body["next_cursor"].is_null());
        let anonymous = support::call(&f, "GET", path, Value::Null, None).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    }
}

async fn setup(
    pool: PgPool,
    pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
) -> (Fixture, String, native::NativeTls, Value) {
    let document = metadata_fixture::metadata();
    let endpoint = format!(
        "/runtime/v1/catalogs/{}/metadata?storage_version={}",
        document.registered_ref, document.storage_version
    );
    let tls = native::native_tls_catalog(
        endpoint,
        serde_json::to_vec_pretty(&document).unwrap(),
        pause,
    )
    .await;
    let targets = RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: tls.endpoint(),
            addresses: vec![tls.server.address],
        }],
        false,
    )
    .unwrap();
    let f = support::fixture_with_runtime_targets(pool, Some(targets)).await;
    let (enrollment, initial_cookie, totp) = support::start(&f).await;
    let (confirmation, _) = support::confirm(&f, &enrollment, &initial_cookie, &totp, false).await;
    assert_eq!(confirmation.status, StatusCode::OK);
    let cookie = confirmation.cookie.unwrap_or(initial_cookie);
    let credential = command(&f, &cookie, "runtime-secret", "POST", "/api/v2/settings/credentials", json!({
        "intent":{"schema_version":1,"purpose":"RUNTIME","label":"Controlled Runtime credential"},"value":native::SECRET
    })).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let ca = command(&f, &cookie, "runtime-ca", "POST", "/api/v2/settings/credentials", json!({
        "intent":{"schema_version":1,"purpose":"TLS_CA","label":"Controlled native CA"},"value":String::from_utf8(tls.ca.clone()).unwrap()
    })).await;
    assert_eq!(ca.status, StatusCode::CREATED);
    let runtime = command(&f, &cookie, "runtime", "POST", "/api/v2/integrations/runtimes", json!({
        "schema_version":1,"configuration":{"name":"TLS catalog Runtime","endpoint":tls.endpoint(),"tls_policy":"PINNED_CA","allowed_capabilities":["DATA_VALIDATE"],"enabled":true,"development_http":false},
        "credential_ref":credential.body["resource"]["id"],"ca_certificate_ref":ca.body["resource"]["id"]
    })).await;
    assert_eq!(runtime.status, StatusCode::CREATED);
    let project = command(&f, &cookie, "project", "POST", "/api/v2/projects", json!({"schema_version":1,"name":"License evidence","description":"Controlled fixture only","fork_from_project_id":null})).await;
    assert_eq!(project.status, StatusCode::CREATED);
    let proof = command(
        &f,
        &cookie,
        "proof",
        "POST",
        "/api/v2/artifacts",
        json!({
            "schema_version":1,"project_id":project.body["resource"]["id"],"kind":"REPORT",
            "content":"{\"schema_version\":1,\"license\":\"controlled source test only\"}"
        }),
    )
    .await;
    assert_eq!(proof.status, StatusCode::CREATED);
    let source = command(&f, &cookie, "source", "POST", "/api/v2/data/sources", json!({
        "schema_version":1,"name":"Controlled native catalog","runtime_id":runtime.body["resource"]["id"],"native_catalog_ref":document.registered_ref,"provider_kind":"NAUTILUS_CATALOG","enabled":true
    })).await;
    assert_eq!(source.status, StatusCode::CREATED);
    let source_id = source.body["resource"]["id"].as_str().unwrap();
    let grant = command(&f, &cookie, "license", "POST", &format!("/api/v2/data/sources/{source_id}/grants"), json!({
        "schema_version":1,"source_id":source_id,"license_reference":"Controlled synthetic metadata license",
        "evidence_artifact_id":proof.body["resource"]["id"],"allowed_uses":"RESEARCH","valid_from":"2000-01-01T00:00:00Z","valid_until":null
    })).await;
    assert_eq!(grant.status, StatusCode::CREATED);
    let intent = json!({
        "schema_version":1,"source_id":source_id,"grant_id":grant.body["resource"]["id"],
        "expected_source_revision":source.body["resource"]["revision"],"expected_runtime_revision":runtime.body["resource"]["revision"],
        "native_storage_version":document.storage_version,"existing_universe_version_id":null
    });
    assert_eq!(
        tls.server.requests.load(Ordering::SeqCst),
        0,
        "configuration cannot perform speculative native reads"
    );
    (f, cookie, tls, intent)
}

#[sqlx::test(migrations = "../../migrations")]
async fn authentic_http_registration_publishes_native_metadata_and_replay_does_not_refetch(
    pool: PgPool,
) {
    let (f, cookie, tls, intent) = setup(pool.clone(), None).await;
    let response = command(
        &f,
        &cookie,
        "register",
        "POST",
        "/api/v2/data/revisions",
        intent.clone(),
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["resource"]["origin"], "FIXTURE");
    assert_eq!(response.body["resource"]["pit_status"], "UNVERIFIED");
    assert_eq!(response.body["resource"]["license_state"], "ACTIVE");
    assert_eq!(response.body["resource"]["row_count"], "3");
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 1);
    let again = command(
        &f,
        &cookie,
        "register",
        "POST",
        "/api/v2/data/revisions",
        intent.clone(),
    )
    .await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(again.body["resource"], response.body["resource"]);
    assert_eq!(again.body["replayed"], true);
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 1);
    let id = response.body["resource"]["id"].as_str().unwrap();
    let read = support::call(
        &f,
        "GET",
        &format!("/api/v2/data/revisions/{id}"),
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.body["id"], id);
    let view = read.body.to_string();
    assert!(!view.contains(native::SECRET));
    assert!(!view.contains("credential_ref"));
    assert!(!view.contains("/home/"));
    let universe = response.body["resource"]["universe_version_id"]
        .as_str()
        .unwrap();
    let read = support::call(
        &f,
        "GET",
        &format!("/api/v2/data/universes/{universe}"),
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(read.status, StatusCode::OK);
    let evidence: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.dataset_registration_evidence")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(evidence, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn authenticated_http_native_validation_creates_one_real_queued_run_without_sql_preparation(
    pool: PgPool,
) {
    let (f, cookie, tls, registration) = setup(pool.clone(), None).await;
    let registered = command(
        &f,
        &cookie,
        "validation-dataset",
        "POST",
        "/api/v2/data/revisions",
        registration.clone(),
    )
    .await;
    assert_eq!(registered.status, StatusCode::OK);
    let source_id = registration["source_id"].as_str().unwrap();
    let source = command(
        &f,
        &cookie,
        "read-source",
        "GET",
        &format!("/api/v2/data/sources/{source_id}"),
        Value::Null,
    )
    .await;
    assert_eq!(source.status, StatusCode::OK);
    let runtime_id = source.body["runtime_id"].as_str().unwrap();
    let probe = command(
        &f,
        &cookie,
        "validation-probe",
        "POST",
        &format!("/api/v2/integrations/runtimes/{runtime_id}/probe"),
        json!({"schema_version":1,"expected_revision":registration["expected_runtime_revision"]}),
    )
    .await;
    assert_eq!(probe.status, StatusCode::OK);
    assert_eq!(probe.body["resource"]["outcome"]["status"], "AVAILABLE");
    let projects = command(
        &f,
        &cookie,
        "read-project",
        "GET",
        "/api/v2/projects",
        Value::Null,
    )
    .await;
    assert_eq!(projects.status, StatusCode::OK);
    let project = projects.body["items"][0]["id"].clone();
    let input = command(&f, &cookie, "validation-input", "POST", "/api/v2/input-sets", json!({
        "schema_version":1,"project_id":project,"purpose":"DISCOVERY",
        "decision_cutoff":metadata_fixture::metadata().available_through,
        "items":[{"kind":"DATASET","dataset_revision_id":registered.body["resource"]["id"],"role":"DISCOVERY"}]
    })).await;
    assert_eq!(input.status, StatusCode::CREATED);
    let body = json!({"schema_version":1,"project_id":project,"input_set_id":input.body["resource"]["header"]["id"],
        "runtime_id":runtime_id,"expected_runtime_revision":registration["expected_runtime_revision"],
        "limits":{"schema_version":1,"experiments":0,"cpu_seconds":"10","wall_seconds":60,"memory_mib":512,"output_bytes":"65536"}});
    let admitted = command(
        &f,
        &cookie,
        "native-validation",
        "POST",
        "/api/v2/data/validate",
        body.clone(),
    )
    .await;
    assert_eq!(admitted.status, StatusCode::ACCEPTED);
    assert_eq!(admitted.body["resource"]["state"], "QUEUED");
    assert_eq!(admitted.body["resource"]["kind"], "DATA_VALIDATE");
    assert!(admitted.body["resource"]["cycle_id"].is_null());
    let replay = command(
        &f,
        &cookie,
        "native-validation",
        "POST",
        "/api/v2/data/validate",
        body.clone(),
    )
    .await;
    assert_eq!(replay.status, StatusCode::ACCEPTED);
    assert_eq!(replay.body["resource"], admitted.body["resource"]);
    assert_eq!(replay.body["replayed"], true);
    let anonymous = support::call(&f, "POST", "/api/v2/data/validate", body.clone(), None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    let mut changed = body;
    changed["limits"]["wall_seconds"] = json!(61);
    let conflict = command(
        &f,
        &cookie,
        "native-validation",
        "POST",
        "/api/v2/data/validate",
        changed,
    )
    .await;
    assert_eq!(conflict.status, StatusCode::CONFLICT);
    assert_eq!(conflict.body["code"], "IDEMPOTENCY_CONFLICT");
    // SQL below only inspects real effects; every fixture resource was created
    // through the same public authenticated HTTP commands available to an operator.
    let facts: (i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.run_native_tasks),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.qualifications)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 1, 1, 0));
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_metadata_network_wait_does_not_hold_source_runtime_or_grant_locks(pool: PgPool) {
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (f, cookie, _tls, intent) =
        setup(pool.clone(), Some((entered.clone(), release.clone()))).await;
    let registration = command(
        &f,
        &cookie,
        "network-boundary",
        "POST",
        "/api/v2/data/revisions",
        intent.clone(),
    );
    let inspect = async {
        tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let source: uuid::Uuid = intent["source_id"].as_str().unwrap().parse().unwrap();
        let grant: uuid::Uuid = intent["grant_id"].as_str().unwrap().parse().unwrap();
        let runtime: uuid::Uuid = sqlx::query_scalar(
            "SELECT runtime_id FROM app.data_sources WHERE id=$1 FOR UPDATE NOWAIT",
        )
        .bind(source)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query("SELECT id FROM app.runtime_integrations WHERE id=$1 FOR UPDATE NOWAIT")
            .bind(runtime)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SELECT id FROM app.data_use_grants WHERE id=$1 FOR UPDATE NOWAIT")
            .bind(grant)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        tx.rollback().await.unwrap();
        release.notify_one();
    };
    let (response, ()) = tokio::join!(registration, inspect);
    assert_eq!(response.status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn data_http_rejects_client_authority_missing_keys_unauthenticated_and_bad_query_fields(
    pool: PgPool,
) {
    let (f, cookie, tls, mut intent) = setup(pool, None).await;
    intent["origin"] = json!("REAL");
    let authority = command(
        &f,
        &cookie,
        "forged-origin",
        "POST",
        "/api/v2/data/revisions",
        intent.clone(),
    )
    .await;
    assert_eq!(authority.status, StatusCode::UNPROCESSABLE_ENTITY);
    intent.as_object_mut().unwrap().remove("origin");
    let missing_key =
        support::call(&f, "POST", "/api/v2/data/revisions", intent, Some(&cookie)).await;
    assert_eq!(missing_key.status, StatusCode::UNPROCESSABLE_ENTITY);
    for path in [
        "/api/v2/data/sources",
        "/api/v2/data/revisions",
        "/api/v2/data/universes",
    ] {
        let anonymous = support::call(&f, "GET", path, Value::Null, None).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
        let invalid = support::call(
            &f,
            "GET",
            &format!("{path}?limit=0"),
            Value::Null,
            Some(&cookie),
        )
        .await;
        assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    }
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 0);
}
