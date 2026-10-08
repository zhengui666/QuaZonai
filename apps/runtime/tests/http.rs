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
        max_cpu: Some(1),
        max_memory_mib: Some(512),
        max_wall_seconds: Some(60),
        max_output_bytes: None,
        max_parallel_jobs: Some(1),
        max_pending_jobs: Some(4),
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
            cpu: Some(1),
            cpu_seconds: Some(DbCounter::new(10).unwrap()),
            memory_mib: Some(64),
            wall_seconds: Some(30),
            output_bytes: Some(DbCounter::new(4096).unwrap()),
        },
        deadline_at: Some(runtime::now() + chrono::Duration::seconds(60)),
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
async fn native_object_upload_accepts_binary_above_former_cap_and_checks_exact_versioned_replays() {
    let f = fixture().await;
    let id = Id::new();
    let path = format!("/runtime/v1/objects/{id}");
    let bearer = format!("Bearer {CREDENTIAL}");
    let headers = [
        ("authorization", bearer.as_str()),
        ("content-type", "application/octet-stream"),
        ("x-qz-storage-version", "native-7"),
    ];
    let bytes = vec![0x5a; 64 * 1024 * 1024 + 1];
    let (status, first) = request(&f, Method::PUT, &path, &headers, bytes.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(first["artifact_id"], id.to_string());
    assert_eq!(first["byte_count"], bytes.len().to_string());
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
    assert_eq!(original.len(), 64 * 1024 * 1024 + 1);
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

#[tokio::test]
async fn every_generated_runtime_operation_requires_the_actual_bearer_and_declares_its_401() {
    let f = fixture().await;
    let document = serde_json::to_value(runtime::http::RuntimeApi::openapi()).unwrap();
    let scheme = &document["components"]["securitySchemes"]["RuntimeBearer"];
    assert_eq!(scheme["type"], "http");
    assert_eq!(scheme["scheme"], "bearer");
    let requirement = json!([{"RuntimeBearer":[]}]);
    assert_eq!(document["security"], requirement);
    let id = Id::new().to_string();
    let mut operations = 0;
    for (template, item) in document["paths"].as_object().unwrap() {
        for (method, operation) in item.as_object().unwrap() {
            if ![
                "get", "post", "put", "patch", "delete", "head", "options", "trace",
            ]
            .contains(&method.as_str())
            {
                continue;
            }
            operations += 1;
            assert_eq!(operation["security"], requirement);
            assert_eq!(
                operation["responses"]["401"]["content"]["application/json"]["schema"]["$ref"],
                "#/components/schemas/RuntimeProblem"
            );
            assert_eq!(
                operation["responses"]["401"]["headers"]["WWW-Authenticate"]["schema"]["enum"],
                json!(["Bearer"])
            );
            let path = template
                .replace("{external_job_id}", &format!("{id}%2F1"))
                .replace("{artifact_id}", &id)
                .replace("{storage_ref}", &id)
                .replace("{registered_ref}", "not-registered");
            let (status, body) = request(
                &f,
                method.to_ascii_uppercase().parse().unwrap(),
                &path,
                &[],
                b"{}".to_vec(),
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
            assert_eq!(body["status"], 401);
            assert_eq!(body["code"], "RUNTIME_AUTHENTICATION_REQUIRED");
        }
    }
    assert_eq!(operations, 8);
}

#[test]
fn native_runtime_openapi_keeps_binary_upload_and_stable_job_contracts() {
    let document = serde_json::to_value(runtime::http::RuntimeApi::openapi()).unwrap();
    let body = &document["paths"]["/runtime/v1/objects/{artifact_id}"]["put"]["requestBody"]
        ["content"]["application/octet-stream"]["schema"];
    assert_eq!(body["type"], "string");
    assert_eq!(body["format"], "binary");
    for (path, method) in [
        ("/runtime/v1/jobs/{external_job_id}", "get"),
        ("/runtime/v1/jobs/{external_job_id}/cancel", "post"),
        ("/runtime/v1/jobs/{external_job_id}/result", "get"),
        (
            "/runtime/v1/jobs/{external_job_id}/artifacts/{storage_ref}",
            "get",
        ),
    ] {
        assert_eq!(
            document["paths"][path][method]["responses"]["503"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/RuntimeProblem"
        );
    }
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

#[cfg(target_os = "linux")]
#[tokio::test]
async fn native_full_filesystem_upload_reports_capacity_failure_and_retries_original_object() {
    use std::{fs, io::Write, process::Command};
    const CHILD: &str = "QZ_RUNTIME_HTTP_FULL_FILESYSTEM_TEST";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = Command::new(std::env::var_os("QZ_TEST_UNSHARE").unwrap_or_else(|| "unshare".into()))
            .args(["--user", "--map-root-user", "--mount", "--", "sh", "-eu", "-c",
                "mount -t tmpfs -o size=16m,mode=0700 tmpfs \"$1\"; export TMPDIR=\"$1\"; exec \"$2\" --exact native_full_filesystem_upload_reports_capacity_failure_and_retries_original_object --nocapture",
                "runtime-http-full-filesystem"])
            .arg(root.path()).arg(std::env::current_exe().unwrap()).env(CHILD, root.path())
            .output().expect("native user and mount namespaces must be available");
        assert!(
            output.status.success(),
            "isolated HTTP ENOSPC test failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("RUNTIME_STORAGE_FULL"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(CREDENTIAL));
        assert!(!String::from_utf8_lossy(&output.stderr).contains(CREDENTIAL));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        return;
    };
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .without_time()
        .with_ansi(false)
        .init();
    let f = fixture().await;
    let id = Id::new();
    let path = format!("/runtime/v1/objects/{id}");
    let bearer = format!("Bearer {CREDENTIAL}");
    let headers = [
        ("authorization", bearer.as_str()),
        ("content-type", "application/octet-stream"),
        ("x-qz-storage-version", "original-1"),
    ];
    let filler_path = std::path::PathBuf::from(root).join("filler");
    let mut filler = fs::File::create(&filler_path).unwrap();
    let mut full = false;
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
    let bytes = vec![7; 128 * 1024];
    let (status, rejected) = request(&f, Method::PUT, &path, &headers, bytes.clone()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(rejected["code"], "RUNTIME_STORAGE_FULL");
    assert_eq!(rejected["retryable"], true);
    assert_eq!(rejected["field"], Value::Null);
    drop(filler);
    fs::remove_file(filler_path).unwrap();
    assert!(matches!(
        f.service.journal().input_object(id).await,
        Err(runtime::Failure::Missing)
    ));
    let (status, created) = request(&f, Method::PUT, &path, &headers, bytes.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, replayed) = request(&f, Method::PUT, &path, &headers, bytes.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed, created);
    assert_eq!(
        f.service.journal().input_object(id).await.unwrap(),
        ("original-1".to_owned(), bytes)
    );
    f.service.journal().close().await;
}

#[tokio::test]
async fn native_json_intake_has_no_implicit_body_cap_and_keeps_auth_and_complete_validation() {
    let f = fixture().await;
    let run = Id::new();
    let path = format!("/runtime/v1/jobs/{run}%2F1/cancel");
    let cancellation = json!({"schema_version":1,"run_id":run,"attempt_no":1,"owner_epoch":"1"});
    let mut bytes = vec![b' '; 2 * 1024 * 1024 + 1];
    bytes.extend_from_slice(&serde_json::to_vec(&cancellation).unwrap());
    assert_eq!(
        request(&f, Method::POST, &path, &[("content-type", "application/json")], bytes.clone()).await.0,
        StatusCode::UNAUTHORIZED
    );
    let bearer = format!("Bearer {CREDENTIAL}");
    let headers = [("authorization", bearer.as_str()), ("content-type", "application/json")];
    let (status, receipt) = request(&f, Method::POST, &path, &headers, bytes.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(receipt["state"], "CANCELLED");
    assert_eq!(receipt["run_id"], run.to_string());
    assert_eq!(
        authenticated(&f, Method::POST, &path, cancellation).await.1,
        receipt,
        "whitespace length cannot change the immutable cancellation receipt"
    );
    bytes.push(b'x');
    assert_eq!(
        request(&f, Method::POST, &path, &headers, bytes).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        authenticated(&f, Method::GET, &format!("/runtime/v1/jobs/{run}%2F1"), Value::Null).await.1,
        receipt,
        "invalid trailing data cannot alter a durable native identity"
    );
    assert!(f.service.journal().scheduling().await.unwrap().is_empty());
}

#[tokio::test]
async fn fifth_authenticated_runtime_request_is_not_rejected_by_four_waiting_bodies() {
    let f = fixture().await;
    let mut tasks = Vec::new();
    let mut releases = Vec::new();
    for _ in 0..4 {
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let body = Body::from_stream(futures_util::stream::once(async move {
            entered.send(()).unwrap();
            released.await.unwrap();
            Ok::<_, std::convert::Infallible>(axum::body::Bytes::from_static(b"original"))
        }));
        let req = Request::builder().method(Method::PUT).uri(format!("/runtime/v1/objects/{}", Id::new()))
            .header(header::AUTHORIZATION, format!("Bearer {CREDENTIAL}"))
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header("x-qz-storage-version", "1").body(body).unwrap();
        let app = f.app.clone();
        tasks.push(tokio::spawn(async move { app.oneshot(req).await.unwrap() }));
        tokio::time::timeout(std::time::Duration::from_secs(3), waiting).await.unwrap().unwrap();
        releases.push(release);
    }
    let (status, _) = request(&f, Method::GET, "/unknown", &[("authorization", &format!("Bearer {CREDENTIAL}"))], vec![]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for release in releases { release.send(()).unwrap(); }
    for task in tasks { assert_eq!(task.await.unwrap().status(), StatusCode::CREATED); }
    f.service.journal().close().await;
}

#[tokio::test]
async fn legitimate_runtime_upload_can_cross_fifteen_seconds_and_replay_its_original_receipt() {
    let f = fixture().await;
    let id = Id::new();
    let path = format!("/runtime/v1/objects/{id}");
    let body = Body::from_stream(futures_util::stream::once(async {
        tokio::time::sleep(std::time::Duration::from_secs(16)).await;
        Ok::<_, std::convert::Infallible>(axum::body::Bytes::from_static(b"slow original"))
    }));
    let req = Request::builder().method(Method::PUT).uri(&path)
        .header(header::AUTHORIZATION, format!("Bearer {CREDENTIAL}"))
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header("x-qz-storage-version", "original").body(body).unwrap();
    let response = tokio::time::timeout(std::time::Duration::from_secs(20), f.app.clone().oneshot(req)).await.unwrap().unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let receipt: Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(f.service.journal().input_object(id).await.unwrap(), ("original".to_owned(), b"slow original".to_vec()));
    // Explicit client replay uses the same native identity; no server-side
    // timeout retry or extra object publication is introduced.
    let (status, replay) = request(&f, Method::PUT, &path, &[("authorization", &format!("Bearer {CREDENTIAL}")), ("content-type", "application/octet-stream"), ("x-qz-storage-version", "original")], b"slow original".to_vec()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay, receipt);
    f.service.journal().close().await;
}

#[tokio::test]
async fn cancelled_runtime_request_body_does_not_publish_or_retry_an_object() {
    let f = fixture().await;
    let id = Id::new();
    let (entered, observed) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let body = Body::from_stream(futures_util::stream::once(async move {
        entered.send(()).unwrap();
        released.await.unwrap();
        Ok::<_, std::convert::Infallible>(axum::body::Bytes::from_static(b"unpublished"))
    }));
    let req = Request::builder().method(Method::PUT).uri(format!("/runtime/v1/objects/{id}"))
        .header(header::AUTHORIZATION, format!("Bearer {CREDENTIAL}"))
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header("x-qz-storage-version", "original").body(body).unwrap();
    let app = f.app.clone();
    let waiter = tokio::spawn(async move { app.oneshot(req).await.unwrap() });
    tokio::time::timeout(std::time::Duration::from_secs(3), observed).await.unwrap().unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert!(release.send(()).is_err(), "request cancellation must drop the body future");
    assert!(matches!(f.service.journal().input_object(id).await, Err(runtime::Failure::Missing)));
    f.service.journal().close().await;
}


struct TcpRequestFinished(Arc<tokio::sync::Notify>);
impl Drop for TcpRequestFinished {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

async fn serve_tcp_fixture(
    f: &Fixture,
) -> (
    std::net::SocketAddr,
    Arc<tokio::sync::Notify>,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let finished = Arc::new(tokio::sync::Notify::new());
    let observed = finished.clone();
    let app = f.app.clone().layer(axum::middleware::from_fn(
        move |request: Request<Body>, next: axum::middleware::Next| {
            let guard = TcpRequestFinished(observed.clone());
            async move {
                let response = next.run(request).await;
                drop(guard);
                response
            }
        },
    ));
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async { let _ = stopped.await; })
            .await
            .unwrap();
    });
    (address, finished, stop, task)
}

async fn open_tcp_upload(
    address: std::net::SocketAddr,
    id: Id,
    length: usize,
    first: &[u8],
) -> tokio::net::TcpStream {
    use tokio::io::AsyncWriteExt;
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let header = format!(
        "PUT /runtime/v1/objects/{id} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {CREDENTIAL}\r\nContent-Type: application/octet-stream\r\nx-qz-storage-version: original\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(header.as_bytes()).await.unwrap();
    stream.write_all(first).await.unwrap();
    stream.flush().await.unwrap();
    stream
}

async fn tcp_upload_receipt(mut stream: tokio::net::TcpStream) -> (u16, Value) {
    use tokio::io::AsyncReadExt;
    let mut response = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), stream.read_to_end(&mut response))
        .await.unwrap().unwrap();
    let separator = response.windows(4).position(|window| window == b"\r\n\r\n").unwrap();
    let header = std::str::from_utf8(&response[..separator]).unwrap();
    let status = header.lines().next().unwrap().split_whitespace().nth(1).unwrap().parse().unwrap();
    let length: usize = header.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse().unwrap())
    }).unwrap();
    let body = &response[separator + 4..];
    assert_eq!(body.len(), length, "the native TCP response must be read completely");
    assert!(!body.windows(CREDENTIAL.len()).any(|bytes| bytes == CREDENTIAL.as_bytes()));
    (status, serde_json::from_slice(body).unwrap())
}

#[tokio::test]
async fn real_tcp_upload_crosses_fifteen_seconds_and_replays_exact_original_bytes() {
    use tokio::io::AsyncWriteExt;
    let f = fixture().await;
    let (address, finished, stop, server) = serve_tcp_fixture(&f).await;
    let id = Id::new();
    let bytes = b"slow original TCP bytes";
    let mut stream = open_tcp_upload(address, id, bytes.len(), &bytes[..1]).await;
    tokio::time::sleep(std::time::Duration::from_secs(16)).await;
    stream.write_all(&bytes[1..]).await.unwrap();
    let (status, receipt) = tcp_upload_receipt(stream).await;
    assert_eq!(status, 201);
    tokio::time::timeout(std::time::Duration::from_secs(3), finished.notified()).await.unwrap();
    assert_eq!(f.service.journal().input_object(id).await.unwrap(), ("original".into(), bytes.to_vec()));
    let stream = open_tcp_upload(address, id, bytes.len(), bytes).await;
    let (status, replay) = tcp_upload_receipt(stream).await;
    assert_eq!(status, 200);
    assert_eq!(replay, receipt);
    assert_eq!(f.service.journal().input_object(id).await.unwrap(), ("original".into(), bytes.to_vec()));
    stop.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), server).await.unwrap().unwrap();
    f.service.journal().close().await;
}

#[tokio::test]
async fn real_tcp_disconnect_cannot_publish_partial_bytes_or_create_a_retry() {
    let f = fixture().await;
    let (address, finished, stop, server) = serve_tcp_fixture(&f).await;
    let id = Id::new();
    let bytes = b"complete original TCP object";
    let stream = open_tcp_upload(address, id, bytes.len(), &bytes[..1]).await;
    drop(stream);
    // Observe termination of the real server request future before checking the
    // durable journal. Elapsed time alone is not evidence of cancellation.
    tokio::time::timeout(std::time::Duration::from_secs(5), finished.notified()).await.unwrap();
    assert!(matches!(f.service.journal().input_object(id).await, Err(runtime::Failure::Missing)));
    let stream = open_tcp_upload(address, id, bytes.len(), bytes).await;
    let (status, receipt) = tcp_upload_receipt(stream).await;
    assert_eq!(status, 201, "the partial request did not publish this identity");
    assert_eq!(f.service.journal().input_object(id).await.unwrap(), ("original".into(), bytes.to_vec()));
    let stream = open_tcp_upload(address, id, bytes.len(), bytes).await;
    let (status, replay) = tcp_upload_receipt(stream).await;
    assert_eq!(status, 200);
    assert_eq!(replay, receipt);
    stop.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), server).await.unwrap().unwrap();
    f.service.journal().close().await;
}
