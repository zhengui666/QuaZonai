//! Actual Axum routes and SQLite. A deliberately missing Docker socket never attests OCI readiness.
use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    Router,
};
use contracts::{
    execution::NativeTaskParametersV1, research::ArtifactInputRole, runtime_jobs::*, DbCounter, Id,
    Revision, SchemaV1,
};
use runtime::{
    config::{ImageRegistration, RuntimeConfig},
    supervisor::RuntimeService,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;
use utoipa::OpenApi;

const CREDENTIAL: &str = "controlled-runtime-http-regression-credential";
struct Fixture {
    _root: tempfile::TempDir,
    service: Arc<RuntimeService>,
    app: Router,
}
async fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let config = RuntimeConfig {
        schema_version: SchemaV1,
        state_dir: root.path().join("state"),
        credential_file: root.path().join("credential"),
        docker_socket: root.path().join("not-a-docker-socket"),
        bind: "127.0.0.1:18973".parse().unwrap(),
        images: vec![ImageRegistration {
            job_kind: contracts::runs::RunKind::DataValidate,
            image_ref: format!("sha256:{}", "a".repeat(64)),
        }],
        catalogs: vec![],
        max_cpu: 1,
        max_memory_mib: 512,
        max_wall_seconds: 60,
        max_output_bytes: 64 * 1024 * 1024,
        max_parallel_jobs: 1,
        max_pending_jobs: 4,
        storage_quota_bytes: 128 * 1024 * 1024,
    };
    let service = RuntimeService::open(config).await.unwrap();
    let app = runtime::http::router(service.clone(), CREDENTIAL.into()).unwrap();
    Fixture {
        _root: root,
        service,
        app,
    }
}
async fn request(
    f: &Fixture,
    method: Method,
    path: &str,
    headers: &[(&str, &str)],
    bytes: Vec<u8>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = f
        .app
        .clone()
        .oneshot(builder.body(Body::from(bytes)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    assert!(
        !body
            .windows(CREDENTIAL.len())
            .any(|bytes| bytes == CREDENTIAL.as_bytes()),
        "credential escaped into a native response"
    );
    let value = serde_json::from_slice(&body).unwrap();
    (status, value)
}
async fn authenticated(
    f: &Fixture,
    method: Method,
    path: &str,
    value: Value,
) -> (StatusCode, Value) {
    request(
        f,
        method,
        path,
        &[
            ("authorization", &format!("Bearer {CREDENTIAL}")),
            ("content-type", "application/json"),
        ],
        serde_json::to_vec(&value).unwrap(),
    )
    .await
}
async fn compile_spec(f: &Fixture) -> JobSpecV1 {
    let code = Id::new();
    let parameters = Id::new();
    let value = NativeTaskParametersV1::CompileModel {
        schema_version: SchemaV1,
        code_artifact_id: code,
    };
    let bytes = serde_json::to_vec(&value).unwrap();
    f.service
        .journal()
        .put_object(code, "1", b"native fixture code")
        .await
        .unwrap();
    f.service
        .journal()
        .put_object(parameters, "1", &bytes)
        .await
        .unwrap();
    let run_id = Id::new();
    JobSpecV1 {
        schema_version: SchemaV1,
        run_id,
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
        external_job_id: domain::runtime_jobs::external_id(run_id, 1).unwrap(),
        job_kind: value.job_kind(),
        image_ref: format!("sha256:{}", "a".repeat(64)),
        input_set_id: Id::new(),
        inputs: vec![RuntimeInputV1::Artifact {
            artifact_id: code,
            storage_version: "1".into(),
            byte_count: DbCounter::new(19).unwrap(),
            role: ArtifactInputRole::Code,
        }],
        parameters_artifact_id: parameters,
        limits: RuntimeJobLimitsV1 {
            cpu: 1,
            cpu_seconds: DbCounter::new(10).unwrap(),
            memory_mib: 64,
            wall_seconds: 30,
            output_bytes: DbCounter::new(4096).unwrap(),
        },
        deadline_at: runtime::now() + chrono::Duration::seconds(60),
        requested_output_schemas: value.output_schemas(),
    }
}

#[tokio::test]
async fn bearer_is_required_and_cookie_or_duplicate_credentials_cannot_override_it() {
    let f = fixture().await;
    let path = "/runtime/v1/capabilities";
    for headers in [
        vec![],
        vec![("authorization", "Bearer wrong")],
        vec![("cookie", "operator=not-a-runtime-credential")],
    ] {
        let (status, body) = request(&f, Method::GET, path, &headers, vec![]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["code"], "RUNTIME_AUTHENTICATION_REQUIRED");
    }
    let bearer = format!("Bearer {CREDENTIAL}");
    for headers in [
        vec![
            ("authorization", bearer.as_str()),
            ("authorization", bearer.as_str()),
        ],
        vec![("authorization", bearer.as_str()), ("cookie", "operator=1")],
    ] {
        assert_eq!(
            request(&f, Method::GET, path, &headers, vec![]).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (status, body) = authenticated(&f, Method::GET, path, Value::Null).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "RUNTIME_ENGINE_UNAVAILABLE");
    assert!(f.service.journal().scheduling().await.unwrap().is_empty());
}

#[tokio::test]
async fn native_object_upload_accepts_binary_above_json_limit_and_checks_exact_versioned_replays() {
    let f = fixture().await;
    let id = Id::new();
    let path = format!("/runtime/v1/objects/{id}");
    let bearer = format!("Bearer {CREDENTIAL}");
    let headers = [
        ("authorization", bearer.as_str()),
        ("content-type", "application/octet-stream"),
        ("x-qz-storage-version", "native-7"),
    ];
    let bytes = vec![0x5a; 2 * 1024 * 1024];
    let (status, first) = request(&f, Method::PUT, &path, &headers, bytes.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(first["artifact_id"], id.to_string());
    assert_eq!(first["byte_count"], "2097152");
    let (status, repeated) = request(&f, Method::PUT, &path, &headers, bytes.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first, repeated);
    let mut changed = bytes;
    changed[0] = 0;
    assert_eq!(
        request(&f, Method::PUT, &path, &headers, changed).await.0,
        StatusCode::CONFLICT
    );
    let mut bad = headers.to_vec();
    bad.push(("x-qz-storage-version", "native-7"));
    assert_eq!(
        request(&f, Method::PUT, &path, &bad, vec![1]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (version, original) = f.service.journal().input_object(id).await.unwrap();
    assert_eq!(version, "native-7");
    assert!(original.iter().all(|value| *value == 0x5a));
    // Global input objects have no public download capability.
    assert_eq!(
        authenticated(&f, Method::GET, &path, Value::Null).await.0,
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn cancellation_before_submission_is_durable_and_late_post_needs_no_live_engine() {
    let f = fixture().await;
    let spec = compile_spec(&f).await;
    let path = format!("/runtime/v1/jobs/{}%2F1", spec.run_id);
    let missing = authenticated(&f, Method::GET, &path, Value::Null).await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
    let cancel = json!({"schema_version":1,"run_id":spec.run_id,"attempt_no":1,"owner_epoch":"1"});
    let (status, tombstone) =
        authenticated(&f, Method::POST, &format!("{path}/cancel"), cancel.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tombstone["state"], "CANCELLED");
    assert_eq!(tombstone["has_result"], false);
    let late = authenticated(
        &f,
        Method::POST,
        "/runtime/v1/jobs",
        serde_json::to_value(&spec).unwrap(),
    )
    .await;
    assert_eq!(late.0, StatusCode::OK);
    assert_eq!(late.1, tombstone);
    assert_eq!(
        authenticated(&f, Method::GET, &path, Value::Null).await.1,
        tombstone
    );
    assert_eq!(
        authenticated(&f, Method::POST, &format!("{path}/cancel"), cancel)
            .await
            .1,
        tombstone
    );
    assert_eq!(
        authenticated(&f, Method::GET, &format!("{path}/result"), Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert!(f.service.journal().scheduling().await.unwrap().is_empty());
}

#[tokio::test]
async fn unavailable_native_engine_never_creates_a_successful_admission() {
    let f = fixture().await;
    let spec = compile_spec(&f).await;
    let (status, body) = authenticated(
        &f,
        Method::POST,
        "/runtime/v1/jobs",
        serde_json::to_value(&spec).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "RUNTIME_ENGINE_UNAVAILABLE");
    assert!(f.service.journal().scheduling().await.unwrap().is_empty());
    assert!(matches!(
        f.service.journal().get(&spec.external_job_id).await,
        Err(runtime::Failure::Missing)
    ));
}

#[tokio::test]
async fn unknown_fields_paths_and_catalog_versions_are_not_generic_execution_capabilities() {
    let f = fixture().await;
    let spec = compile_spec(&f).await;
    let mut body = serde_json::to_value(&spec).unwrap();
    body["command"] = json!(["read-host-secrets"]);
    assert_eq!(
        authenticated(&f, Method::POST, "/runtime/v1/jobs", body)
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    for path in [
        "/runtime/v1/jobs/not-an-identity",
        "/runtime/v1/catalogs/unknown/metadata?storage_version=1",
        "/runtime/v1/catalogs/unknown/metadata?storage_version=1&host_path=%2Fetc",
    ] {
        let (status, body) = authenticated(&f, Method::GET, path, Value::Null).await;
        assert!(matches!(
            status,
            StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY
        ));
        assert!(body.get("code").is_some());
    }
    assert!(f.service.journal().scheduling().await.unwrap().is_empty());
}

#[test]
fn native_runtime_openapi_keeps_binary_upload_and_stable_job_contracts() {
    let document = serde_json::to_value(runtime::http::RuntimeApi::openapi()).unwrap();
    let body = &document["paths"]["/runtime/v1/objects/{artifact_id}"]["put"]["requestBody"]
        ["content"]["application/octet-stream"]["schema"];
    assert_eq!(body["type"], "string");
    assert_eq!(body["format"], "binary");
    let schemas = &document["components"]["schemas"];
    for name in [
        "JobSpecV1",
        "ResultManifestV1",
        "RuntimeJobStatusV1",
        "RuntimeCancelV1",
        "RuntimeObjectReceiptV1",
        "RuntimeCatalogMetadataV1",
        "RuntimeCapabilitiesV1",
    ] {
        assert!(schemas.get(name).is_some(), "missing native wire schema");
    }
}
