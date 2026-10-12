//! Actual local cookie sessions, Axum reads/freeze and retained historical Store codecs.
//! Explicit scientific startup and controlled original history never launch an Agent.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/historical_alpha.rs"]
mod historical_alpha;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
mod support;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::authority::Actor;
use support::{exchange, fixture, invalid_bearer, local_session, Fixture, Reply};

async fn browser(
    f: &Fixture,
    cookie: &str,
    method: &str,
    path: &str,
    key: &str,
    body: Value,
) -> Reply {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "localhost")
        .header(header::ORIGIN, "https://localhost")
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", key)
        .body(if body.is_null() {
            Body::empty()
        } else {
            Body::from(serde_json::to_vec(&body).unwrap())
        })
        .unwrap();
    exchange(&f.app, request).await
}

async fn setup(pool: &PgPool) -> (Fixture, String, cycle_support::Fixture) {
    let f = support::fixture_with_runtime_targets(
        pool.clone(),
        Some(server::runtime_transport::RuntimeTargets::default()),
    )
    .await;
    let login = local_session(&f).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap();
    let login_id: String = sqlx::query_scalar(
        "SELECT id::text FROM app.browser_logins ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let actor = Actor::Browser {
        login_id: login_id.try_into().unwrap(),
    };
    let objects = std::sync::Arc::new(
        integrations::artifacts::ArtifactStore::open(&f._state.path().join("artifacts")).unwrap(),
    );
    let data = cycle_support::setup_with_objects(pool, &f.store, &actor, objects).await;
    (f, cookie, data)
}

async fn activate(f: &Fixture, cookie: &str, project: contracts::Id) -> Value {
    let path = format!("/api/v2/projects/{project}");
    let current = browser(f, cookie, "GET", &path, "unused", Value::Null).await;
    assert_eq!(current.status, StatusCode::OK, "{}", current.body);
    let updated = browser(
        f,
        cookie,
        "PATCH",
        &path,
        "activate",
        json!({
            "schema_version": 1,
            "expected_revision": current.body["revision"],
            "name": current.body["name"],
            "description": current.body["description"],
            "state": "ACTIVE"
        }),
    )
    .await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.body);
    updated.body["resource"].clone()
}

// Explicit data preparation through the Store, not the retired public start route.
// Historical selection below uses marked test-only archived facts, not native inference.
async fn scientific_start(
    f: &Fixture,
    data: &cycle_support::Fixture,
    key: &str,
    body: Value,
) -> Reply {
    use axum::response::IntoResponse;
    let login: String = sqlx::query_scalar(
        "SELECT id::text FROM app.browser_logins ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&f.store.native_pool())
    .await
    .unwrap();
    let actor = Actor::Browser {
        login_id: login.try_into().unwrap(),
    };
    let request = contracts::cycles::ExternalCycleStartIntent {
        schema_version: contracts::SchemaV1,
        project_id: data.data.project,
        request: serde_json::from_value(body).unwrap(),
    };
    let reading = data.objects.clone();
    let publishing = data.objects.clone();
    let mut allocated = None;
    let result = f
        .store
        .start_scientific_cycle(
            &actor,
            key,
            &request,
            move |id, size| {
                let objects = reading.clone();
                async move {
                    tokio::task::spawn_blocking(move || objects.read(id, size))
                        .await
                        .map_err(|_| store::StoreError::Integrity)?
                        .map_err(|_| store::StoreError::Integrity)
                }
            },
            |object| {
                allocated = Some(object.id);
                let objects = publishing.clone();
                async move {
                    tokio::task::spawn_blocking(move || objects.put(object.id, &object.bytes))
                        .await
                        .map_err(|_| store::StoreError::Integrity)?
                        .map_err(|error| match error {
                            integrations::artifacts::ArtifactError::Io(error)
                                if error.kind() == std::io::ErrorKind::StorageFull =>
                            {
                                store::StoreError::StorageFull
                            }
                            _ => store::StoreError::Integrity,
                        })
                }
            },
        )
        .await;
    if let Some(id) = allocated.filter(|_| result.is_err()) {
        let objects = data.objects.clone();
        f.store
            .discard_unpublished_operator_artifact(id, move |id| async move {
                objects
                    .discard_unpublished(id)
                    .map_err(|_| store::StoreError::Integrity)
            })
            .await
            .unwrap();
    }
    let response = match result {
        Ok(result) => (StatusCode::ACCEPTED, axum::Json(result)).into_response(),
        Err(error) => server::error::ApiError::from(error).into_response(),
    };
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    Reply {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap(),
        cookie: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn authenticated_freeze_and_historical_cycle_keep_original_receipts_and_selection_reads(
    pool: PgPool,
) {
    let (f, cookie, data) = setup(&pool).await;
    let freeze_path = format!("/api/v2/briefs/{}/freeze", data.brief.id);
    let freeze_body = serde_json::to_value(&data.freeze).unwrap();
    let frozen = browser(
        &f,
        &cookie,
        "POST",
        &freeze_path,
        "freeze",
        freeze_body.clone(),
    )
    .await;
    assert_eq!(frozen.status, StatusCode::OK, "{}", frozen.body);
    assert_eq!(frozen.body["resource"]["brief"]["state"], "FROZEN");
    assert_eq!(frozen.headers[header::CACHE_CONTROL], "no-store");
    let context = browser(
        &f,
        &cookie,
        "GET",
        &format!("/api/v2/briefs/{}/execution-context", data.brief.id),
        "unused",
        Value::Null,
    )
    .await;
    assert_eq!(context.status, StatusCode::OK);
    assert_eq!(context.body, frozen.body["resource"]);
    let replay = browser(&f, &cookie, "POST", &freeze_path, "freeze", freeze_body).await;
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], frozen.body["resource"]);

    let project = activate(&f, &cookie, data.data.project).await;
    let path = format!("/api/v2/projects/{}/cycles", data.data.project);
    let body = json!({
        "schema_version": 1,
        "brief_id": data.brief.id,
        "expected_revision": project["revision"]
    });
    let started = scientific_start(&f, &data, "start", body.clone()).await;
    assert_eq!(started.status, StatusCode::ACCEPTED, "{}", started.body);
    assert_eq!(started.body["resource"]["run"]["kind"], "DATA_VALIDATE");
    assert_eq!(started.body["resource"]["run"]["state"], "QUEUED");
    assert!(started.body["resource"]["cycle"]["researcher_profile"].is_null());
    assert!(started.body["resource"]["cycle"]["reviewer_profile"].is_null());
    let run_id = started.body["resource"]["run"]["id"].as_str().unwrap();
    let cycle_id = started.body["resource"]["cycle"]["id"].as_str().unwrap();
    let run = browser(
        &f,
        &cookie,
        "GET",
        &format!("/api/v2/runs/{run_id}"),
        "unused",
        Value::Null,
    )
    .await;
    assert_eq!(run.status, StatusCode::OK, "{}", run.body);
    assert_eq!(run.body, started.body["resource"]["run"]);
    let cycle = browser(
        &f,
        &cookie,
        "GET",
        &format!("/api/v2/cycles/{cycle_id}"),
        "unused",
        Value::Null,
    )
    .await;
    assert_eq!(cycle.status, StatusCode::OK);
    assert_eq!(cycle.body["initial_run_id"], run_id);
    let replay = scientific_start(&f, &data, "start", body).await;
    assert_eq!(replay.status, StatusCode::ACCEPTED);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], started.body["resource"]);
    let page = browser(&f, &cookie, "GET", &path, "unused", Value::Null).await;
    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(page.body["items"].as_array().unwrap().len(), 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pgmq.q_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    for suffix in ["selection", "selection/trials"] {
        let absent = browser(
            &f,
            &cookie,
            "GET",
            &format!("/api/v2/cycles/{cycle_id}/{suffix}"),
            "unused",
            Value::Null,
        )
        .await;
        assert_eq!(absent.status, StatusCode::NOT_FOUND);
    }
    let preparation = contracts::Id::try_from(run_id.to_owned()).unwrap();
    mission_support::complete(&pool, &f.store, &data, preparation, false).await;
    let messages = f.store.read_native_run_messages(60, 100).await.unwrap();
    let preparation_message = messages
        .iter()
        .find(|message| message.run_id == preparation)
        .unwrap();
    f.store.acknowledge_run(preparation_message).await.unwrap();
    let retired: i64 = sqlx::query_scalar("SELECT count(*) FROM app.run_missions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        retired, 0,
        "data completion and ACK do not schedule an Agent"
    );
    let login: String = sqlx::query_scalar(
        "SELECT id::text FROM app.browser_logins ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let actor = Actor::Browser {
        login_id: login.try_into().unwrap(),
    };
    let lease = historical_alpha::parent(
        &pool,
        &f.store,
        &actor,
        &data,
        contracts::Id::try_from(cycle_id.to_owned()).unwrap(),
    )
    .await;
    let message_id: i64 =
        sqlx::query_scalar("SELECT msg_id FROM pgmq.q_runs WHERE message->>'run_id'=$1")
            .bind(lease.run.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    let message = store::lifecycle::RunMessage {
        message_id,
        run_id: lease.run.id,
        read_count: 1,
    };
    let path = format!("/api/v2/runs/{}", lease.run.id);
    let current = browser(&f, &cookie, "GET", &path, "unused", Value::Null).await;
    let cancelled = browser(
        &f,
        &cookie,
        "POST",
        &format!("{path}/cancel"),
        "cancel-empty",
        json!({"schema_version":1,"expected_revision":current.body["revision"]}),
    )
    .await;
    assert_eq!(
        cancelled.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        cancelled.body
    );
    assert_eq!(
        f.store.get_run(&actor, lease.run.id).await.unwrap(),
        lease.run
    );
    // Only this marked archival fixture supplies an original terminal for read/selection checks.
    historical_alpha::terminal(&pool, &lease).await;
    f.store
        .freeze_cycle_selection(&actor, lease.run.id)
        .await
        .unwrap();
    f.store.acknowledge_run(&message).await.unwrap();
    let path = format!("/api/v2/cycles/{cycle_id}/selection");
    let selection = browser(&f, &cookie, "GET", &path, "unused", Value::Null).await;
    assert_eq!(selection.status, StatusCode::OK, "{}", selection.body);
    assert_eq!(selection.body["research_run_id"], lease.run.id.to_string());
    assert_eq!(selection.body["status"], "INCONCLUSIVE");
    assert_eq!(selection.body["trial_count"], "0");
    let page = browser(
        &f,
        &cookie,
        "GET",
        &format!("{path}/trials?limit=1"),
        "unused",
        Value::Null,
    )
    .await;
    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(
        page.body,
        json!({"schema_version":1,"items":[],"next_cursor":null})
    );
    for path in [
        format!("{path}/trials?limit=0"),
        format!("{path}/trials?unexpected=true"),
        "/api/v2/cycles/bad/selection".into(),
    ] {
        assert_eq!(
            browser(&f, &cookie, "GET", &path, "unused", Value::Null)
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn startup_rejects_forged_success_stale_revision_and_invalid_machine_mutation(pool: PgPool) {
    let (f, cookie, data) = setup(&pool).await;
    let path = format!("/api/v2/briefs/{}/freeze", data.brief.id);
    let mut forged = serde_json::to_value(&data.freeze).unwrap();
    forged["capabilities"] = json!({"status": "AVAILABLE"});
    let rejected = browser(&f, &cookie, "POST", &path, "injected", forged).await;
    assert_eq!(rejected.status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut stale = serde_json::to_value(&data.freeze).unwrap();
    stale["expected_revision"] = json!("99");
    let rejected = browser(&f, &cookie, "POST", &path, "stale", stale).await;
    assert_eq!(rejected.status, StatusCode::CONFLICT);
    let rejected = invalid_bearer(
        &f,
        "POST",
        &path,
        serde_json::to_value(&data.freeze).unwrap(),
    )
    .await;
    assert_eq!(rejected.status, StatusCode::UNAUTHORIZED);
    let contexts: i64 = sqlx::query_scalar("SELECT count(*) FROM app.brief_execution_contexts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(contexts, 0);
    let cycles: i64 = sqlx::query_scalar("SELECT count(*) FROM app.research_cycles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cycles, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn retired_cycle_start_cannot_admit_work_even_without_an_artifact_deployment(pool: PgPool) {
    let f = fixture(pool.clone()).await;
    let login = local_session(&f).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap();
    let response = browser(
        &f,
        &cookie,
        "POST",
        &format!("/api/v2/projects/{}/cycles", contracts::Id::new()),
        "missing-artifact-store",
        json!({"schema_version":1,"brief_id":contracts::Id::new(),"expected_revision":"1",
            "researcher_profile":{"profile_id":contracts::Id::new(),"expected_revision":"1"},
            "reviewer_profile":{"profile_id":contracts::Id::new(),"expected_revision":"1"}}),
    )
    .await;
    assert_eq!(response.status, StatusCode::METHOD_NOT_ALLOWED);
    let counts: (i64, i64) =
        sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM pgmq.q_runs)")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(counts, (0, 0));
}

#[test]
fn startup_routes_are_in_the_actual_native_http_contract() {
    let document: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    for (path, method, status, identity) in [
        (
            "/api/v2/briefs/{id}/freeze",
            "post",
            "200",
            "freezeResearchBrief",
        ),
        (
            "/api/v2/briefs/{id}/execution-context",
            "get",
            "200",
            "getResearchBriefExecutionContext",
        ),
        (
            "/api/v2/projects/{id}/cycles",
            "get",
            "200",
            "listProjectResearchCycles",
        ),
        ("/api/v2/cycles/{id}", "get", "200", "getResearchCycle"),
    ] {
        let operation = &document["paths"][path][method];
        assert!(operation["responses"][status].is_object(), "{path}");
        assert!(operation["responses"]["503"].is_object(), "{path}");
        assert_eq!(operation["operationId"], identity, "{path}");
    }
    assert!(document["paths"]["/api/v2/projects/{id}/cycles"]["post"].is_null());
}

#[cfg(target_os = "linux")]
#[sqlx::test(migrations = "../../migrations")]
async fn native_full_filesystem_rolls_back_cycle_admission_and_allows_same_intent_retry(
    pool: PgPool,
) {
    use std::{fs, io::Write, process::Command};
    const CHILD: &str = "QZ_CYCLE_FULL_FILESYSTEM_TEST";
    let Some(root) = std::env::var_os(CHILD) else {
        // SQLx derives the child database from the same test name. Release this
        // unused parent connection before the isolated child recreates it.
        pool.close().await;
        let root = tempfile::tempdir().unwrap();
        let output = Command::new(std::env::var_os("QZ_TEST_UNSHARE").unwrap_or_else(|| "unshare".into()))
            .args(["--user", "--map-root-user", "--mount", "--", "sh", "-eu", "-c",
                "mount -t tmpfs -o size=16m,mode=0700 tmpfs \"$1\"; export TMPDIR=\"$1\"; exec \"$2\" --exact native_full_filesystem_rolls_back_cycle_admission_and_allows_same_intent_retry --nocapture",
                "cycle-full-filesystem"])
            .arg(root.path()).arg(std::env::current_exe().unwrap()).env(CHILD, root.path())
            .output().expect("native user and mount namespaces must be available");
        assert!(
            output.status.success(),
            "isolated Cycle ENOSPC test failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("STORAGE_FULL"),
            "native storage alert must be emitted"
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        return;
    };
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .without_time()
        .with_ansi(false)
        .init();
    let (f, cookie, data) = setup(&pool).await;
    let frozen = browser(
        &f,
        &cookie,
        "POST",
        &format!("/api/v2/briefs/{}/freeze", data.brief.id),
        "freeze-full-filesystem",
        serde_json::to_value(&data.freeze).unwrap(),
    )
    .await;
    assert_eq!(frozen.status, StatusCode::OK);
    let project = activate(&f, &cookie, data.data.project).await;
    let body = json!({"schema_version":1,"brief_id":data.brief.id,"expected_revision":project["revision"]});
    let objects = f._state.path().join("artifacts");
    let mut before = fs::read_dir(&objects)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    before.sort();
    let filler_path = std::path::PathBuf::from(root).join("filler");
    let mut filler = fs::File::create(&filler_path).unwrap();
    let mut full = false;
    // Bounded independently of the mount: never fill an arbitrary host filesystem.
    for _ in 0..512 {
        match filler.write_all(&[0; 64 * 1024]) {
            Ok(()) => (),
            Err(error) => {
                assert_eq!(error.kind(), std::io::ErrorKind::StorageFull);
                full = true;
                break;
            }
        }
    }
    assert!(full, "private 16 MiB tmpfs must exhaust before 32 MiB");
    let rejected = scientific_start(&f, &data, "full-filesystem-start", body.clone()).await;
    assert_eq!(
        rejected.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        rejected.body
    );
    assert_eq!(rejected.body["code"], "STORAGE_FULL");
    assert_eq!(rejected.body["retryable"], true);
    assert!(rejected.body["detail"]
        .as_str()
        .unwrap()
        .contains("存储空间已满"));
    // Independent database reads prove the failed request left no admitted work.
    for table in ["app.research_cycles", "app.runs", "pgmq.q_runs"] {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    let mut after = fs::read_dir(&objects)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    after.sort();
    assert_eq!(
        after, before,
        "old bytes retained and failed staging removed"
    );
    drop(filler);
    fs::remove_file(filler_path).unwrap();
    let retry = scientific_start(&f, &data, "full-filesystem-start", body.clone()).await;
    assert_eq!(retry.status, StatusCode::ACCEPTED, "{}", retry.body);
    assert_eq!(
        retry.body["replayed"], false,
        "failure did not persist a receipt"
    );
    let replay = scientific_start(&f, &data, "full-filesystem-start", body).await;
    assert_eq!(replay.status, StatusCode::ACCEPTED);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], retry.body["resource"]);
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM pgmq.q_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(queued, 1);
}
