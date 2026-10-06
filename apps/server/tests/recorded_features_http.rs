//! Real Axum/session/PostgreSQL/original object storage, all FIXTURE data.
#[path = "../../../crates/store/tests/support/data.rs"]
mod data_fixture;
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use contracts::{
    catalogs::{RecordedFeatureFragmentV1, RecordedFeatureInputsV1},
    research::DataPartition,
    DbCounter, Id, SchemaV1,
};
use integrations::artifacts::ArtifactStore;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;

fn n(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn content() -> String {
    concat!(" \n{\"schema_version\":1,\"partition\":\"DISCOVERY\",",
        "\"feature_schema\":[{\"feature_key\":\"price\",\"source_ref\":\"fixture:original\",\"source_key\":\"paired\",",
        "\"availability\":{\"basis\":\"OBSERVED\"},\"max_age_ns\":null}],",
        "\"observations\":[{\"feature_index\":0,\"event_ns\":\"50000000000\",\"observed_available_ns\":\"55000000000\",",
        "\"sequence\":\"0\",\"value\":0.10000000000000002,\"missing_reason\":null},",
        "{\"feature_index\":0,\"event_ns\":\"100000000000\",\"observed_available_ns\":\"110000000000\",",
        "\"sequence\":\"1\",\"value\":null,\"missing_reason\":\"recorded_missing\"}]}\n").into()
}
async fn command(
    f: &support::Fixture,
    cookie: &str,
    key: &str,
    method: &str,
    path: &str,
    body: Value,
) -> support::Reply {
    support::exchange(
        &f.app,
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost")
            .header(header::ORIGIN, "https://localhost")
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", key)
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
}

async fn scoped(
    f: &support::Fixture,
    token: &str,
    method: &str,
    path: &str,
    body: Value,
) -> support::Reply {
    support::exchange(
        &f.app,
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", "scoped-feature-read")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn registered_features_keep_original_bytes_and_source_through_http(pool: PgPool) {
    let f = support::fixture_with_runtime_targets(
        pool.clone(),
        Some(server::runtime_transport::RuntimeTargets::default()),
    )
    .await;
    let cookie = support::local_session(&f).await.cookie.unwrap();
    let actor = store::authority::Actor::Browser {
        login_id: f.store.local_browser().await.unwrap().id,
    };
    let objects = Arc::new(ArtifactStore::open(&f._state.path().join("artifacts")).unwrap());
    let d = data_fixture::prepare(
        &pool,
        f.store.clone(),
        actor,
        None,
        tempfile::tempdir().unwrap(),
        objects.clone(),
    )
    .await;
    let raw = content();
    let mut metadata = data_fixture::catalog_fixture::metadata();
    metadata.recorded_feature_inputs = Some(RecordedFeatureInputsV1 {
        schema_version: SchemaV1,
        source_selection_start_ns: n(40_000_000_000),
        source_selection_end_ns: n(101_000_000_000),
        partition: DataPartition::Discovery,
        fragments: vec![RecordedFeatureFragmentV1 {
            part_key: "features-0001".into(),
            byte_count: n(raw.len() as u64),
            observations: n(2),
            min_event_ns: n(50_000_000_000),
            max_event_ns: n(100_000_000_000),
            min_observed_available_ns: n(55_000_000_000),
            max_observed_available_ns: n(110_000_000_000),
        }],
    });
    let ticket = data_fixture::ticket(&d, "recorded-dataset", &data_fixture::request(&d)).await;
    let dataset = data_fixture::complete(&d, ticket, serde_json::to_vec(&metadata).unwrap())
        .await
        .unwrap()
        .resource;
    let route = format!("/api/v2/data/revisions/{}/features", dataset.id);
    let list = format!("{route}?project_id={}", d.project);
    let request = json!({"schema_version":1,"project_id":d.project,"dataset_revision_id":dataset.id,"feature_part_key":"features-0001","content":raw});
    let anonymous = support::call(&f, "GET", &list, Value::Null, None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    let empty = command(&f, &cookie, "read-empty", "GET", &list, Value::Null).await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.body);
    assert_eq!(empty.body["items"], json!([]));
    let mut mismatch = request.clone();
    mismatch["dataset_revision_id"] = json!(Id::new());
    assert_eq!(
        command(&f, &cookie, "route-mismatch", "POST", &route, mismatch)
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let first = command(
        &f,
        &cookie,
        "original-feature",
        "POST",
        &route,
        request.clone(),
    )
    .await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    assert_eq!(
        first.body["resource"]["source_binding"]["origin"],
        "FIXTURE"
    );
    assert_eq!(
        first.body["resource"]["source_binding"]["pit_status"],
        "UNVERIFIED"
    );
    assert_eq!(
        first.body["resource"]["source_binding"]["dataset_revision_id"],
        json!(dataset.id)
    );
    let artifact: Id = first.body["resource"]["artifact_id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    assert_eq!(
        objects.read(artifact, n(raw.len() as u64)).unwrap(),
        raw.as_bytes()
    );
    let replay = command(
        &f,
        &cookie,
        "original-feature",
        "POST",
        &route,
        request.clone(),
    )
    .await;
    assert_eq!(replay.status, StatusCode::CREATED, "{}", replay.body);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], first.body["resource"]);
    let principal = command(
        &f,
        &cookie,
        "feature-reader",
        "POST",
        "/api/v2/machine-principals",
        json!({
            "schema_version":1,"name":"Recorded feature reader","kind":"CLI",
            "project_id":d.project,"downstream_id":null,"enabled":true
        }),
    )
    .await;
    assert_eq!(principal.status, StatusCode::CREATED, "{}", principal.body);
    let credential = command(&f, &cookie, "feature-reader-token", "POST",
        &format!("/api/v2/machine-principals/{}/credentials", principal.body["resource"]["id"].as_str().unwrap()),
        json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})
    ).await;
    assert_eq!(
        credential.status,
        StatusCode::CREATED,
        "{}",
        credential.body
    );
    let token = credential.body["token"].as_str().unwrap();
    let allowed = scoped(&f, token, "GET", &list, Value::Null).await;
    assert_eq!(allowed.status, StatusCode::OK, "{}", allowed.body);
    assert_eq!(
        allowed.body["items"],
        json!([first.body["resource"].clone()])
    );
    let foreign = format!("{route}?project_id={}", Id::new());
    assert_eq!(
        scoped(&f, token, "GET", &foreign, Value::Null).await.status,
        StatusCode::NOT_FOUND
    );
    let no_grant = scoped(&f, token, "POST", &route, request.clone()).await;
    assert_eq!(no_grant.status, StatusCode::FORBIDDEN, "{}", no_grant.body);
    let mut changed = request;
    changed["content"] = json!(raw.replace("0.10000000000000002", "0.20000000000000002"));
    assert_eq!(
        command(&f, &cookie, "original-feature", "POST", &route, changed)
            .await
            .status,
        StatusCode::CONFLICT
    );
    let listed = command(&f, &cookie, "read-features", "GET", &list, Value::Null).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.body);
    assert_eq!(
        listed.body["items"],
        json!([first.body["resource"].clone()])
    );
    assert_eq!(
        command(&f, &cookie, "missing-project", "GET", &route, Value::Null)
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[test]
fn feature_routes_publish_native_contracts_without_origin_override() {
    let api: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    let route = &api["paths"]["/api/v2/data/revisions/{id}/features"];
    assert!(route["get"].is_object());
    assert!(route["post"].is_object());
    assert_eq!(route["get"]["operationId"], "list_recorded_features");
    assert_eq!(route["post"]["operationId"], "register_recorded_feature");
    assert!(route["post"]["responses"].get("413").is_none());
    assert!(route["post"]["responses"].get("422").is_some());
    let schema = &api["components"]["schemas"]["RecordedFeatureRegisterV1"];
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"].get("origin").is_none());
    assert!(schema["properties"].get("pit_status").is_none());
    assert_eq!(schema["properties"]["content"]["type"], "string");
}

#[test]
fn operation_ids_are_unique_across_all_native_http_contracts() {
    let api: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    let mut operations = std::collections::BTreeMap::new();
    for (path, item) in api["paths"].as_object().unwrap() {
        for method in [
            "get", "post", "put", "patch", "delete", "options", "head", "trace",
        ] {
            let Some(operation) = item.get(method) else {
                continue;
            };
            let id = operation["operationId"].as_str().expect("operation ID");
            assert!(
                operations.insert(id, (path, method)).is_none(),
                "duplicate operationId {id} at {method} {path}"
            );
        }
    }
}
