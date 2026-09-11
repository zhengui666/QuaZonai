//! Actual native TCP/Axum/reqwest exchanges. Not a claim of OCI or scientific acceptance.
#[path = "support/runtime_native.rs"]
mod native;
use axum::{
    body::{to_bytes, Body, Bytes},
    extract::Path,
    http::{header, HeaderMap, Request, Response, StatusCode},
    routing::get,
    Router,
};
use chrono::{DateTime, Duration, Utc};
use contracts::{
    research::DataPartition,
    runs::RunKind,
    runtime::{RuntimeArtifactSchemaV1, RuntimeProbeFailure},
    runtime_jobs::*,
    DbCounter, Id, Revision, SchemaV1,
};
use domain::runtime_jobs::external_id;
use native::SECRET;
use serde_json::json;
use server::runtime_transport::{
    RuntimeRequestError, RuntimeTarget, RuntimeTargets, RuntimeTransport,
};
use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use store::lifecycle::RuntimeSnapshot;
use tokio::{net::TcpListener, task::JoinHandle};

fn instant() -> DateTime<Utc> {
    DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap()
}
fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn client(address: SocketAddr) -> RuntimeTransport {
    let origin = format!("http://{address}");
    let targets = RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: origin.clone(),
            addresses: vec![address],
        }],
        true,
    )
    .unwrap();
    RuntimeTransport::new(
        &targets,
        &RuntimeSnapshot {
            schema_version: SchemaV1,
            endpoint: origin,
            credential_ref: Id::new().to_string(),
            tls_policy: "SYSTEM_CA".into(),
            ca_certificate_ref: None,
            development_http: true,
            protocol_version: "1".into(),
            allowed_capabilities: vec!["DATA_VALIDATE".into()],
        },
        SECRET.as_bytes(),
        None,
    )
    .unwrap()
}
struct Seen {
    method: String,
    uri: String,
    body: Vec<u8>,
    version: Option<String>,
    authorized: bool,
}
struct NativeExchange {
    client: RuntimeTransport,
    address: SocketAddr,
    seen: Arc<Mutex<Vec<Seen>>>,
    task: JoinHandle<()>,
}
impl Drop for NativeExchange {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl NativeExchange {
    fn assert_count(&self, expected: usize) {
        let seen = self.seen.lock().unwrap();
        assert_eq!(seen.len(), expected);
        assert!(
            seen.iter().all(|item| item.authorized),
            "native authorization boundary mismatch"
        );
    }
}
async fn exchange(
    status: StatusCode,
    payload: Vec<u8>,
    media: &str,
    extra: HeaderMap,
    chunked: bool,
) -> NativeExchange {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let records = seen.clone();
    let media = media.to_owned();
    let app = Router::new().fallback(move |request: Request<Body>| {
        let records = records.clone();
        let payload = payload.clone();
        let media = media.clone();
        let extra = extra.clone();
        async move {
            let authorized = request
                .headers()
                .get(header::AUTHORIZATION)
                .is_some_and(|value| value == format!("Bearer {SECRET}").as_str())
                && !request.headers().contains_key(header::COOKIE);
            let method = request.method().to_string();
            let uri = request.uri().to_string();
            let version = request
                .headers()
                .get("x-qz-storage-version")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let body = to_bytes(request.into_body(), 1024 * 1024)
                .await
                .unwrap()
                .to_vec();
            records.lock().unwrap().push(Seen {
                method,
                uri,
                body,
                version,
                authorized,
            });
            let mut response = Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, media);
            for (key, value) in &extra {
                response = response.header(key, value);
            }
            let body = if chunked {
                let chunks: Vec<Result<Bytes, Infallible>> = payload
                    .chunks(4096)
                    .map(|part| Ok(Bytes::copy_from_slice(part)))
                    .collect();
                Body::from_stream(futures_util::stream::iter(chunks))
            } else {
                Body::from(payload)
            };
            response.body(body).unwrap()
        }
    });
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    NativeExchange {
        client: client(address),
        address,
        seen,
        task,
    }
}
fn spec() -> JobSpecV1 {
    let run_id = Id::new();
    JobSpecV1 {
        schema_version: SchemaV1,
        run_id,
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
        external_job_id: external_id(run_id, 1).unwrap(),
        job_kind: RunKind::DataValidate,
        image_ref: format!("localhost/science@sha256:{}", "a".repeat(64)),
        input_set_id: Id::new(),
        inputs: vec![RuntimeInputV1::Dataset {
            revision_id: Id::new(),
            registered_ref: "fixture-catalog".into(),
            storage_version: "1".into(),
            role: DataPartition::Discovery,
        }],
        parameters_artifact_id: Id::new(),
        limits: RuntimeJobLimitsV1 {
            cpu: 1,
            cpu_seconds: count(10),
            memory_mib: 64,
            wall_seconds: 30,
            output_bytes: count(4096),
        },
        deadline_at: instant() + Duration::seconds(30),
        requested_output_schemas: vec![RuntimeArtifactSchemaV1 {
            name: "qz.data_quality".into(),
            version: "1".into(),
        }],
    }
}
fn status(spec: &JobSpecV1) -> RuntimeJobStatusV1 {
    RuntimeJobStatusV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: spec.attempt_no,
        external_job_id: spec.external_job_id.clone(),
        state: RuntimeJobState::Accepted,
        has_result: false,
        submitted_at: instant(),
        started_at: None,
        finished_at: None,
    }
}
fn output(bytes: &[u8]) -> RuntimeOutputV1 {
    RuntimeOutputV1 {
        kind: RuntimeOutputKind::DataQuality,
        schema: RuntimeArtifactSchemaV1 {
            name: "qz.data_quality".into(),
            version: "1".into(),
        },
        storage_ref: Id::new(),
        storage_version: Revision::INITIAL,
        byte_count: count(bytes.len() as u64),
        media_type: "application/json".into(),
    }
}

#[tokio::test]
async fn exact_submit_and_status_use_one_encoded_native_identity_segment() {
    let spec = spec();
    let reply = status(&spec);
    let server = exchange(
        StatusCode::ACCEPTED,
        serde_json::to_vec(&reply).unwrap(),
        "application/json",
        HeaderMap::new(),
        false,
    )
    .await;
    let received = server.client.submit_job(&spec).await.unwrap();
    assert_eq!(received, reply);
    server.assert_count(1);
    {
        let seen = server.seen.lock().unwrap();
        assert_eq!(seen[0].method, "POST");
        assert_eq!(seen[0].uri, "/runtime/v1/jobs");
        assert!(
            serde_json::from_slice::<serde_json::Value>(&seen[0].body).unwrap()
                == serde_json::to_value(&spec).unwrap()
        );
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let expected_id = spec.external_job_id.clone();
    let app = Router::new().route(
        "/runtime/v1/jobs/{job}",
        get(move |Path(job): Path<String>| {
            let expected_id = expected_id.clone();
            let reply = reply.clone();
            async move {
                assert_eq!(job, expected_id);
                axum::Json(reply)
            }
        }),
    );
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let received = client(address)
        .job_status(&spec.external_job_id)
        .await
        .unwrap();
    assert_eq!(received.state, RuntimeJobState::Accepted);
    task.abort();
}

#[tokio::test]
async fn missing_is_not_confirmation_and_cancel_uses_the_exact_current_fence() {
    let spec = spec();
    let missing = exchange(
        StatusCode::NOT_FOUND,
        SECRET.as_bytes().to_vec(),
        "application/json",
        HeaderMap::new(),
        false,
    )
    .await;
    assert_eq!(
        missing.client.job_status(&spec.external_job_id).await.err(),
        Some(RuntimeRequestError::Missing)
    );
    missing.assert_count(1);
    let command = RuntimeCancelV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: spec.attempt_no,
        owner_epoch: Revision::INITIAL.next().unwrap(),
    };
    let mut reply = status(&spec);
    reply.state = RuntimeJobState::CancelRequested;
    let pending = exchange(
        StatusCode::ACCEPTED,
        serde_json::to_vec(&reply).unwrap(),
        "application/json",
        HeaderMap::new(),
        false,
    )
    .await;
    let received = pending
        .client
        .cancel_job(&spec.external_job_id, &command)
        .await
        .unwrap();
    assert!(!received.state.is_terminal());
    pending.assert_count(1);
    let seen = pending.seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(
        seen[0].uri,
        format!("/runtime/v1/jobs/{}%2F1/cancel", spec.run_id)
    );
    assert!(serde_json::from_slice::<RuntimeCancelV1>(&seen[0].body).unwrap() == command);
}

#[tokio::test]
async fn foreign_status_and_untrusted_local_identity_never_become_adoptable() {
    let spec = spec();
    let wrong = status(&self::spec());
    let server = exchange(
        StatusCode::OK,
        serde_json::to_vec(&wrong).unwrap(),
        "application/json",
        HeaderMap::new(),
        false,
    )
    .await;
    assert_eq!(
        server.client.job_status(&spec.external_job_id).await.err(),
        Some(RuntimeRequestError::Contract)
    );
    server.assert_count(1);
    assert!(server.client.job_status("../../private").await.is_err());
    let command = RuntimeCancelV1 {
        schema_version: SchemaV1,
        run_id: Id::new(),
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
    };
    assert!(server
        .client
        .cancel_job(&spec.external_job_id, &command)
        .await
        .is_err());
    assert!(server
        .client
        .upload_object(Id::new(), "version\r\ninjected", vec![1])
        .await
        .is_err());
    server.assert_count(1);
}

#[tokio::test]
async fn native_object_copy_checks_immutable_version_and_exact_receipt() {
    let id = Id::new();
    let bytes = b"immutable native parameters".to_vec();
    let receipt = RuntimeObjectReceiptV1 {
        schema_version: SchemaV1,
        artifact_id: id,
        storage_version: "native-version_7".into(),
        byte_count: count(bytes.len() as u64),
    };
    let server = exchange(
        StatusCode::CREATED,
        serde_json::to_vec(&receipt).unwrap(),
        "application/json",
        HeaderMap::new(),
        false,
    )
    .await;
    assert!(
        server
            .client
            .upload_object(id, &receipt.storage_version, bytes.clone())
            .await
            .unwrap()
            == receipt
    );
    server.assert_count(1);
    {
        let seen = server.seen.lock().unwrap();
        assert_eq!(seen[0].method, "PUT");
        assert_eq!(seen[0].uri, format!("/runtime/v1/objects/{id}"));
        assert_eq!(seen[0].version.as_deref(), Some("native-version_7"));
        assert!(seen[0].body == bytes);
    }
    assert!(server
        .client
        .upload_object(Id::new(), &receipt.storage_version, bytes)
        .await
        .is_err());
    server.assert_count(2);
}

#[tokio::test]
async fn validated_manifest_retains_the_exact_original_bytes_and_rejects_foreign_input_sets() {
    let spec = spec();
    let start = instant() - Duration::seconds(1);
    let mut manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: 1,
        external_job_id: spec.external_job_id.clone(),
        input_set_id: spec.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: std::collections::BTreeMap::from([("fixture-engine".into(), "1".into())]),
        started_at: Some(start),
        finished_at: instant(),
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: count(1000),
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: count(2),
        },
        artifacts: vec![output(b"{}")],
        error: None,
    };
    let raw = serde_json::to_vec_pretty(&manifest).unwrap();
    let server = exchange(
        StatusCode::OK,
        raw.clone(),
        "application/json;charset=utf-8",
        HeaderMap::new(),
        true,
    )
    .await;
    let received = server.client.job_result(&spec, start).await.unwrap();
    assert!(received.raw_document == raw);
    assert_eq!(received.manifest.input_set_id, spec.input_set_id);
    server.assert_count(1);
    manifest.input_set_id = Id::new();
    let wrong = exchange(
        StatusCode::OK,
        serde_json::to_vec(&manifest).unwrap(),
        "application/json",
        HeaderMap::new(),
        false,
    )
    .await;
    assert_eq!(
        wrong.client.job_result(&spec, start).await.err(),
        Some(RuntimeRequestError::Contract)
    );
    wrong.assert_count(1);
}

#[tokio::test]
async fn job_artifacts_are_scoped_bounded_and_never_publish_reflected_credentials() {
    let spec = spec();
    let bytes = br#"{"observed":true}"#.to_vec();
    let artifact = output(&bytes);
    let server = exchange(
        StatusCode::OK,
        bytes.clone(),
        "application/json",
        HeaderMap::new(),
        true,
    )
    .await;
    assert!(
        server
            .client
            .job_artifact(&spec.external_job_id, &artifact)
            .await
            .unwrap()
            == bytes
    );
    server.assert_count(1);
    assert_eq!(
        server.seen.lock().unwrap()[0].uri,
        format!(
            "/runtime/v1/jobs/{}%2F1/artifacts/{}",
            spec.run_id, artifact.storage_ref
        )
    );
    let mut too_small = artifact.clone();
    too_small.byte_count = count(2);
    assert_eq!(
        server
            .client
            .job_artifact(&spec.external_job_id, &too_small)
            .await
            .err(),
        Some(RuntimeRequestError::ResponseLimit)
    );
    let escaped = SECRET
        .chars()
        .map(|value| format!("\\u{:04x}", value as u32))
        .collect::<String>();
    let reflected = format!(r#"{{"report":"{escaped}","report":"safe"}}"#).into_bytes();
    let reflection = exchange(
        StatusCode::OK,
        reflected.clone(),
        "application/json",
        HeaderMap::new(),
        false,
    )
    .await;
    assert_eq!(
        reflection
            .client
            .job_artifact(&spec.external_job_id, &output(&reflected))
            .await
            .err(),
        Some(RuntimeRequestError::Contract)
    );
    reflection.assert_count(1);
}

#[tokio::test]
async fn duplicate_native_map_keys_cannot_hide_an_earlier_escaped_credential() {
    let mut payload = serde_json::to_string(&native::capabilities(instant())).unwrap();
    let escaped = SECRET
        .chars()
        .map(|value| format!("\\u{:04x}", value as u32))
        .collect::<String>();
    payload = payload.replace(
        "\"engine_versions\":{",
        &format!("\"engine_versions\":{{\"earlier\":\"{escaped}\",\"earlier\":\"1\","),
    );
    let server = exchange(
        StatusCode::OK,
        payload.into_bytes(),
        "application/json",
        HeaderMap::new(),
        false,
    )
    .await;
    assert_eq!(
        server.client.capabilities().await.err(),
        Some(RuntimeProbeFailure::ContractUnsupported)
    );
    server.assert_count(1);
}

#[tokio::test]
async fn redirect_compression_duplicate_headers_and_large_chunks_are_rejected_without_retry() {
    let spec = spec();
    let sink = exchange(
        StatusCode::OK,
        vec![],
        "application/json",
        HeaderMap::new(),
        false,
    )
    .await;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::LOCATION,
        format!("http://{}/runtime/v1/jobs", sink.address)
            .parse()
            .unwrap(),
    );
    let redirect = exchange(
        StatusCode::TEMPORARY_REDIRECT,
        vec![],
        "application/json",
        headers,
        false,
    )
    .await;
    assert!(redirect
        .client
        .job_status(&spec.external_job_id)
        .await
        .is_err());
    redirect.assert_count(1);
    sink.assert_count(0);
    for header_name in [header::CONTENT_ENCODING, header::CONTENT_TYPE] {
        let mut headers = HeaderMap::new();
        headers.insert(
            header_name.clone(),
            if header_name == header::CONTENT_TYPE {
                "application/json"
            } else {
                "gzip"
            }
            .parse()
            .unwrap(),
        );
        let server = exchange(
            StatusCode::OK,
            serde_json::to_vec(&status(&spec)).unwrap(),
            "application/json",
            headers,
            false,
        )
        .await;
        assert_eq!(
            server.client.job_status(&spec.external_job_id).await.err(),
            Some(RuntimeRequestError::Contract)
        );
        server.assert_count(1);
    }
    let large = exchange(
        StatusCode::OK,
        vec![b'x'; 1024 * 1024 + 1],
        "application/json",
        HeaderMap::new(),
        true,
    )
    .await;
    assert_eq!(
        large.client.job_status(&spec.external_job_id).await.err(),
        Some(RuntimeRequestError::ResponseLimit)
    );
    large.assert_count(1);
    let unavailable = exchange(
        StatusCode::SERVICE_UNAVAILABLE,
        serde_json::to_vec(&json!({"secret":SECRET})).unwrap(),
        "application/json",
        HeaderMap::new(),
        false,
    )
    .await;
    assert_eq!(
        unavailable
            .client
            .job_status(&spec.external_job_id)
            .await
            .err(),
        Some(RuntimeRequestError::Unavailable)
    );
    unavailable.assert_count(1);
}
