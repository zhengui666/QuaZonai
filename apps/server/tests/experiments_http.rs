//! Real authentication, ArtifactStore bytes and PostgreSQL proposal publication.
//! The parent Cycle is an explicit relational fixture, not a freeze/readiness bypass.
#[path = "../../../tests/support/brief.rs"]
mod brief_support;
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/experiments.rs"]
mod experiment_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/experiment_tasks.rs"]
mod native_experiment_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use chrono::{Duration, Utc};
use contracts::{experiments::ExperimentView, Id};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use serde_json::{json, Value};
use server::{AppState, WebPolicy};
use sqlx::{PgPool, Row};
use store::authority::Actor;
use support::{confirm, exchange, fixture, start, Fixture, Reply};
use tower_sessions::cookie::Key;

#[sqlx::test(migrations = "../../migrations")]
async fn operator_http_admits_original_calibrated_sealed_run_without_another_trial(pool: PgPool) {
    use contracts::{
        control::CommandResult, evidence::AlphaEvaluateRequestV1, lifecycle::JobLimitsV1,
        runs::RunSnapshotV1, DbCounter, SchemaV1,
    };
    use store::{
        lifecycle::{native::NativeObjectPublication, ClaimResult},
        StoreError,
    };
    let (f, cookie, _) = setup(&pool).await;
    let login: String = sqlx::query_scalar(
        "SELECT id::text FROM app.browser_logins ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let actor = Actor::Browser {
        login_id: login.try_into().unwrap(),
    };
    let objects =
        std::sync::Arc::new(ArtifactStore::open(&f._state.path().join("artifacts")).unwrap());
    let data = cycle_support::setup_with_objects(&pool, &f.store, &actor, objects).await;
    f.store
        .freeze_brief(
            &actor,
            "http-sealed-freeze",
            data.brief.id,
            &data.freeze,
            |id, size| data.read(id, size),
        )
        .await
        .unwrap();
    let start = cycle_support::start_request(&f.store, &actor, &data).await;
    let started = data
        .start(&f.store, &actor, "http-sealed-cycle", &start)
        .await
        .unwrap()
        .resource;
    mission_support::complete(&pool, &f.store, &data, started.run.id, false).await;
    assert!(f.store.advance_initial_cycle(started.run.id).await.unwrap());
    let message = f
        .store
        .read_mission_messages(60, 1)
        .await
        .unwrap()
        .remove(0);
    let Some(ClaimResult::Leased(parent)) = f
        .store
        .claim_mission(&message, "http-sealed-source", 120)
        .await
        .unwrap()
    else {
        panic!("research lease required")
    };
    let experiment =
        native_experiment_support::propose(&pool, &f.store, &actor, &data, started.cycle.id).await;
    let mut limits = JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 1,
        cpu_seconds: DbCounter::new(10).unwrap(),
        wall_seconds: 60,
        memory_mib: 1024,
        output_bytes: DbCounter::new(1024 * 1024).unwrap(),
    };
    let publish = |object: NativeObjectPublication| {
        let objects = data.objects.clone();
        async move {
            objects
                .put(object.id, &object.bytes)
                .map_err(|_| StoreError::Integrity)
        }
    };
    let compiler = f
        .store
        .start_experiment_compilation(parent.run.id, &parent.fence, experiment, &limits, publish)
        .await
        .unwrap()
        .resource
        .id;
    native_experiment_support::complete_compilation(&pool, &f.store, &data, compiler).await;
    limits.experiments = 0;
    let forecast = f
        .store
        .start_experiment_forecast(
            parent.run.id,
            &parent.fence,
            experiment,
            &limits,
            |id, size| data.read(id, size),
            publish,
        )
        .await
        .unwrap()
        .resource
        .id;
    native_experiment_support::complete_forecast(&pool, &f.store, &data, forecast).await;
    f.store
        .prepare_research_alpha(parent.run.id, &parent.fence, experiment)
        .await
        .unwrap();
    let validation = f
        .store
        .start_experiment_validation(
            parent.run.id,
            &parent.fence,
            experiment,
            &limits,
            |id, size| data.read(id, size),
            publish,
        )
        .await
        .unwrap()
        .resource
        .id;
    native_experiment_support::complete_validation(&pool, &f.store, &data, validation, 1000, 0.8)
        .await;
    let evaluation = f
        .store
        .publish_scientific_result(validation, |id, size| data.read(id, size), publish)
        .await
        .unwrap()
        .unwrap()
        .resource;
    let target = sqlx::query("SELECT v.id,e.subject_alpha_version_id FROM app.alpha_versions v JOIN app.calibrations c ON c.id=v.calibration_id JOIN app.evaluations e ON e.id=c.validation_evaluation_id WHERE e.id=$1")
        .bind(evaluation.as_uuid()).fetch_one(&pool).await.unwrap();
    let alpha = Id::try_from(target.get::<uuid::Uuid, _>("id").to_string()).unwrap();
    let original = Id::try_from(
        target
            .get::<uuid::Uuid, _>("subject_alpha_version_id")
            .to_string(),
    )
    .unwrap();
    let request = AlphaEvaluateRequestV1 {
        schema_version: SchemaV1,
        cycle_id: started.cycle.id,
        policy_id: data.brief.content.evaluation_policy_id,
        input_set_id: data.freeze.execution_context.sealed_input_set_id,
        runtime_id: data.freeze.execution_context.runtime_id,
        expected_runtime_revision: data.freeze.execution_context.runtime_revision,
        limits,
    };
    let body = serde_json::to_value(&request).unwrap();
    let unbound = browser(
        &f,
        &cookie,
        "POST",
        &format!("/api/v2/alpha-versions/{original}/evaluations"),
        "unbound-score",
        body.clone(),
    )
    .await;
    assert_eq!(unbound.status, StatusCode::UNPROCESSABLE_ENTITY);
    let path = format!("/api/v2/alpha-versions/{alpha}/evaluations");
    let response = browser(
        &f,
        &cookie,
        "POST",
        &path,
        "original-held-out",
        body.clone(),
    )
    .await;
    assert_eq!(response.status, StatusCode::ACCEPTED, "{:?}", response.body);
    let accepted: CommandResult<RunSnapshotV1> = serde_json::from_value(response.body).unwrap();
    assert_eq!(accepted.resource.cycle_id, Some(started.cycle.id));
    assert_eq!(accepted.resource.state, contracts::runs::RunState::Queued);
    let allocation: Value =
        sqlx::query_scalar("SELECT limits FROM app.run_admissions WHERE run_id=$1")
            .bind(accepted.resource.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(allocation["experiments"], 0);
    let opportunities: i64 = sqlx::query_scalar("SELECT count(*) FROM app.sealed_opportunities")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        opportunities, 0,
        "HTTP admission does not grant native data access"
    );
    let replay = browser(
        &f,
        &cookie,
        "POST",
        &path,
        "original-held-out",
        body.clone(),
    )
    .await;
    assert_eq!(replay.status, StatusCode::ACCEPTED);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(
        replay.body["resource"]["id"],
        accepted.resource.id.to_string()
    );
    let mut changed = body;
    changed["limits"]["wall_seconds"] = json!(59);
    assert_eq!(
        browser(&f, &cookie, "POST", &path, "original-held-out", changed)
            .await
            .status,
        StatusCode::CONFLICT
    );
    // These native producer bytes are controlled. Actual scientific execution has its own OCI tests.
    let message = f
        .store
        .read_native_run_messages(60, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == accepted.resource.id)
        .unwrap();
    let Some(ClaimResult::Leased(lease)) = f
        .store
        .claim_native_run(&message, "http-held-out-result", 60)
        .await
        .unwrap()
    else {
        panic!("sealed lease required")
    };
    native_experiment_support::complete_sealed(&pool, &f.store, &data, *lease).await;
    f.store
        .publish_scientific_result(
            accepted.resource.id,
            |id, size| data.read(id, size),
            publish,
        )
        .await
        .unwrap()
        .unwrap();
    f.store.acknowledge_run(&message).await.unwrap();
    let decision: String = sqlx::query_scalar(
        "SELECT decision FROM app.evaluations WHERE run_id=$1 AND evaluation_kind='SEALED'",
    )
    .bind(accepted.resource.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(decision, "REJECT");
}

async fn send(
    f: &Fixture,
    method: &str,
    path: &str,
    value: Value,
    headers: &[(&str, &str)],
) -> Reply {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "research.example");
    for (key, value) in headers {
        builder = builder.header(*key, *value);
    }
    let body = if value.is_null() {
        Body::empty()
    } else {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&value).unwrap())
    };
    exchange(&f.app, builder.body(body).unwrap()).await
}
async fn browser(
    f: &Fixture,
    cookie: &str,
    method: &str,
    path: &str,
    key: &str,
    value: Value,
) -> Reply {
    send(
        f,
        method,
        path,
        value,
        &[
            ("cookie", cookie),
            ("origin", "https://research.example"),
            ("idempotency-key", key),
        ],
    )
    .await
}

async fn setup(pool: &PgPool) -> (Fixture, String, experiment_support::Fixture) {
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
    f.app = server::router(state, Key::generate());
    let (enrollment, cookie, totp) = start(&f).await;
    let (login, _) = confirm(&f, &enrollment, &cookie, &totp, true).await;
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
    let mut e = experiment_support::setup(pool, &f.store, &actor, 3).await;
    // Replace all relational fixture references with objects uploaded through
    // the real native byte store and authenticated HTTP publication endpoint.
    for kind in ["CODE", "PARAMETERS", "REPORT"] {
        let content = if kind == "CODE" {
            "pub fn research_value() -> f64 { 1.0 }"
        } else {
            "{\"schema_version\":1,\"source\":\"test\"}"
        };
        let uploaded = browser(
            &f,
            &cookie,
            "POST",
            "/api/v2/artifacts",
            kind,
            json!({"schema_version":1,"project_id":e.data.project,"kind":kind,"content":content}),
        )
        .await;
        assert_eq!(uploaded.status, StatusCode::CREATED, "{}", uploaded.body);
        assert_eq!(uploaded.body["resource"]["origin"], "SYNTHETIC");
        let id: Id = uploaded.body["resource"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
            .try_into()
            .unwrap();
        match kind {
            "CODE" => e.request.code_artifact_id = Some(id),
            "PARAMETERS" => e.request.parameter_artifact_id = id,
            _ => e.request.proposal_artifact_id = id,
        }
        let bytes = browser(
            &f,
            &cookie,
            "GET",
            &format!("/api/v2/artifacts/{id}/content"),
            "unused",
            Value::Null,
        )
        .await;
        assert_eq!(bytes.status, StatusCode::OK);
        assert_eq!(
            bytes.headers[header::CONTENT_TYPE],
            "application/octet-stream"
        );
    }
    (f, cookie, e)
}

#[sqlx::test(migrations = "../../migrations")]
async fn alpha_routes_return_exact_versions_and_never_substitute_active_or_invent_origin(
    pool: PgPool,
) {
    let (f, cookie, e) = setup(&pool).await;
    let proposal = browser(
        &f,
        &cookie,
        "POST",
        "/api/v2/experiments",
        "alpha-proposal",
        serde_json::to_value(&e.request).unwrap(),
    )
    .await;
    assert_eq!(proposal.status, StatusCode::CREATED);
    let experiment: Id = proposal.body["resource"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    let alpha = Id::new();
    sqlx::query("INSERT INTO app.alphas(id,project_id,name,lifecycle) VALUES($1,$2,'Relational version routing fixture','RESEARCH')")
        .bind(alpha.as_uuid()).bind(e.data.project.as_uuid()).execute(&pool).await.unwrap();
    let mut versions = Vec::new();
    for number in [1, i32::MAX] {
        let id = Id::new();
        sqlx::query("INSERT INTO app.alpha_versions(id,project_id,alpha_id,version,experiment_id,root_lineage_id,code_artifact_id,signal_contract_version,signal_kind,horizon_kind,horizon_value,forecast_unit,runtime_image_ref) SELECT $1,p.id,$2,$3,$4,p.root_lineage_id,$5,'1','SCORE','FIXED_BARS',9007199254740993,'UNITLESS_SCORE','routing-fixture' FROM app.projects p WHERE p.id=$6")
            .bind(id.as_uuid()).bind(alpha.as_uuid()).bind(number).bind(experiment.as_uuid()).bind(e.request.code_artifact_id.unwrap().as_uuid())
            .bind(e.data.project.as_uuid()).execute(&pool).await.unwrap();
        versions.push(id);
    }
    sqlx::query("UPDATE app.alphas SET active_version_id=$2,revision=revision+1 WHERE id=$1")
        .bind(alpha.as_uuid())
        .bind(versions[1].as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let get = |path: String| {
        let f = &f;
        let cookie = &cookie;
        async move { browser(f, cookie, "GET", &path, "unused", Value::Null).await }
    };
    let alphas = get(format!("/api/v2/alphas?project_id={}", e.data.project)).await;
    assert_eq!(alphas.status, StatusCode::OK);
    assert_eq!(
        alphas.body["items"][0]["active_version"],
        i32::MAX.to_string()
    );
    assert_eq!(
        alphas.body["items"][0]["active_version_id"],
        versions[1].to_string()
    );
    let first = get(format!("/api/v2/alphas/{alpha}/versions?limit=1")).await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.body["items"][0]["id"], versions[1].to_string());
    let cursor = first.body["next_cursor"].as_str().unwrap();
    let second = get(format!(
        "/api/v2/alphas/{alpha}/versions?limit=1&cursor={cursor}"
    ))
    .await;
    assert_eq!(second.body["items"][0]["id"], versions[0].to_string());
    assert!(second.body["next_cursor"].is_null());
    let original = get(format!("/api/v2/alphas/{alpha}/versions/1")).await;
    assert_eq!(original.status, StatusCode::OK);
    assert_eq!(original.body["id"], versions[0].to_string());
    assert_eq!(original.body["version"], "1");
    assert_eq!(original.body["horizon_value"], "9007199254740993");
    assert!(
        original.body["origin"].is_null(),
        "CODE origin cannot stand in for absent native data provenance"
    );
    assert!(original.body["calibration_id"].is_null());
    let qualifications = get(format!(
        "/api/v2/alpha-versions/{}/qualifications?limit=1",
        versions[0]
    ))
    .await;
    assert_eq!(qualifications.status, StatusCode::OK);
    assert_eq!(qualifications.body["items"], json!([]));
    assert!(qualifications.body["next_cursor"].is_null());
    for (suffix, status) in [
        (
            format!("{}/qualifications?limit=0", versions[0]),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            format!("{}/qualifications", contracts::Id::new()),
            StatusCode::NOT_FOUND,
        ),
        (
            "not-an-id/qualifications".into(),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        assert_eq!(
            get(format!("/api/v2/alpha-versions/{suffix}")).await.status,
            status
        );
    }
    for (value, expected) in [
        (versions[0].to_string(), StatusCode::NOT_FOUND),
        ("not-an-id".into(), StatusCode::UNPROCESSABLE_ENTITY),
    ] {
        assert_eq!(
            get(format!("/api/v2/alpha-versions/{value}/calibration"))
                .await
                .status,
            expected
        );
    }
    assert_eq!(
        get(format!("/api/v2/alphas/{alpha}/versions/2"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(format!("/api/v2/alphas/{alpha}/versions/0"))
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        get(format!("/api/v2/alphas/{alpha}/versions/9007199254740993"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let absent = get(format!(
        "/api/v2/alpha-versions/{}/evaluations",
        versions[0]
    ))
    .await;
    assert_eq!(absent.status, StatusCode::OK);
    assert_eq!(absent.body["items"], json!([]));
    for suffix in ["", "/metrics"] {
        assert_eq!(
            get(format!("/api/v2/evaluations/{}{suffix}", Id::new()))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(format!("/api/v2/evaluations/not-an-id{suffix}"))
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn browser_proposes_lists_and_replays_real_uploaded_artifact_references(pool: PgPool) {
    let (f, cookie, fixture) = setup(&pool).await;
    let body = serde_json::to_value(&fixture.request).unwrap();
    let created = browser(
        &f,
        &cookie,
        "POST",
        "/api/v2/experiments",
        "proposal",
        body.clone(),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.headers[header::CACHE_CONTROL], "no-store");
    let resource: ExperimentView =
        serde_json::from_value(created.body["resource"].clone()).unwrap();
    assert_eq!(
        resource.proposal_artifact_id,
        fixture.request.proposal_artifact_id
    );
    assert!(resource.run_id.is_none());
    let get = browser(
        &f,
        &cookie,
        "GET",
        &format!("/api/v2/experiments/{}", resource.id),
        "unused",
        Value::Null,
    )
    .await;
    assert_eq!(get.status, StatusCode::OK);
    assert_eq!(get.body, created.body["resource"]);
    let listing = browser(
        &f,
        &cookie,
        "GET",
        &format!(
            "/api/v2/experiments?project_id={}&limit=1",
            fixture.data.project
        ),
        "unused",
        Value::Null,
    )
    .await;
    assert_eq!(listing.status, StatusCode::OK);
    assert_eq!(listing.body["items"][0], get.body);
    let replay = browser(
        &f,
        &cookie,
        "POST",
        "/api/v2/experiments",
        "proposal",
        body.clone(),
    )
    .await;
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], created.body["resource"]);
    let mut invalid = body.clone();
    invalid["outcome"] = json!("SUPPORTED");
    let denied = browser(
        &f,
        &cookie,
        "POST",
        "/api/v2/experiments",
        "fake-pass",
        invalid,
    )
    .await;
    assert_eq!(denied.status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut different = body.clone();
    different["hypothesis"] = json!("Changed premise");
    assert_eq!(
        browser(
            &f,
            &cookie,
            "POST",
            "/api/v2/experiments",
            "proposal",
            different
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    for key in ["second", "third"] {
        assert_eq!(
            browser(
                &f,
                &cookie,
                "POST",
                "/api/v2/experiments",
                key,
                body.clone()
            )
            .await
            .status,
            StatusCode::CREATED
        );
    }
    let exhausted = browser(
        &f,
        &cookie,
        "POST",
        "/api/v2/experiments",
        "exhausted",
        body,
    )
    .await;
    assert_eq!(exhausted.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(exhausted.body["code"], "BUDGET_EXHAUSTED");
    assert_eq!(exhausted.body["retryable"], false);
    assert_eq!(exhausted.body["field_errors"][0]["field"], "experiments");
    assert!(!exhausted.headers.contains_key(header::RETRY_AFTER));
}

#[sqlx::test(migrations = "../../migrations")]
async fn machine_requires_scoped_submission_without_acquiring_operator_grant(pool: PgPool) {
    let (f, cookie, fixture) = setup(&pool).await;
    let body = serde_json::to_value(&fixture.request).unwrap();
    assert_eq!(
        send(
            &f,
            "POST",
            "/api/v2/experiments",
            body.clone(),
            &[
                ("origin", "https://research.example"),
                ("idempotency-key", "anon"),
            ]
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &f,
            "POST",
            "/api/v2/experiments",
            body.clone(),
            &[("cookie", &cookie), ("idempotency-key", "origin-missing")]
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    for (key, scopes, expected) in [
        ("reader", json!(["RESEARCH_READ"]), StatusCode::FORBIDDEN),
        (
            "author",
            json!(["RESEARCH_READ", "EXPERIMENT_SUBMIT"]),
            StatusCode::CREATED,
        ),
    ] {
        let principal = browser(&f, &cookie, "POST", "/api/v2/machine-principals", key,
            json!({"schema_version":1,"name":key,"kind":"CLI","project_id":fixture.data.project,"downstream_id":null,"enabled":true})).await;
        assert_eq!(principal.status, StatusCode::CREATED, "{}", principal.body);
        let id = principal.body["resource"]["id"].as_str().unwrap();
        let credential = browser(&f, &cookie, "POST", &format!("/api/v2/machine-principals/{id}/credentials"), key,
            json!({"schema_version":1,"scope_codes":scopes,"expires_at":Utc::now()+Duration::hours(1)})).await;
        assert_eq!(
            credential.status,
            StatusCode::CREATED,
            "{}",
            credential.body
        );
        let bearer = format!("Bearer {}", credential.body["token"].as_str().unwrap());
        let reply = send(
            &f,
            "POST",
            "/api/v2/experiments",
            body.clone(),
            &[("authorization", &bearer), ("idempotency-key", key)],
        )
        .await;
        assert_eq!(reply.status, expected, "{}", reply.body);
    }
}

#[test]
fn the_actual_http_contract_declares_proposal_capability_without_a_human_grant() {
    let document: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    let path = &document["paths"]["/api/v2/experiments"]["post"];
    assert_eq!(path["operationId"], "propose_experiment");
    assert_eq!(
        path["security"],
        json!([{"BrowserSession":[]},{"MachineBearer":[]}])
    );
    assert!(path["responses"]["201"]["content"]["application/json"].is_object());
    assert!(path["responses"]["429"]["content"]["application/problem+json"].is_object());
    let schema = &document["components"]["schemas"]["ExperimentProposalV1"];
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"].get("outcome").is_none());
    assert!(schema["properties"].get("author_run_id").is_none());
}
