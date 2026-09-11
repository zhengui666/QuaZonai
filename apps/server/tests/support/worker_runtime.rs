//! Controlled TCP Runtime for Worker fault tests. Never an engine or production evidence.
use super::tasks;
use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use contracts::{
    execution::NativeTaskParametersV1, runtime_jobs::*, settings::RuntimeUpdate, DbCounter, Id,
    Revision, SchemaV1,
};
use integrations::{
    artifacts::ArtifactStore, authentication::random_capability, secrets::SecretVault,
};
use server::{
    runtime_transport::{RuntimeTarget, RuntimeTargets},
    worker::Worker,
};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::{net::TcpListener, task::JoinHandle};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Behavior {
    LostSubmitAck,
    MissingUntilCancelled,
    InvalidPayload,
}

#[derive(Default, Clone)]
pub struct Counts {
    pub uploads: usize,
    pub submits: usize,
    pub queries: usize,
    pub cancels: usize,
    pub output_reads: usize,
}

struct NativeJob {
    spec: JobSpecV1,
    status: RuntimeJobStatusV1,
    manifest: Vec<u8>,
    outputs: BTreeMap<Id, Vec<u8>>,
}
#[derive(Default)]
struct Journal {
    counts: Counts,
    inputs: BTreeMap<Id, Vec<u8>>,
    jobs: BTreeMap<String, NativeJob>,
}
struct Endpoint {
    secret: String,
    behavior: Behavior,
    journal: Mutex<Journal>,
}

pub struct Harness {
    pub fixture: tasks::Fixture,
    pub worker: Worker,
    pub targets: RuntimeTargets,
    endpoint: Arc<Endpoint>,
    listener: JoinHandle<()>,
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.listener.abort();
    }
}
impl Harness {
    pub fn counts(&self) -> Counts {
        self.endpoint.journal.lock().unwrap().counts.clone()
    }
    pub fn transport(
        &self,
        lease: &store::lifecycle::RunLease,
    ) -> server::runtime_transport::RuntimeTransport {
        server::runtime_transport::RuntimeTransport::new(
            &self.targets,
            &lease.runtime,
            self.endpoint.secret.as_bytes(),
            None,
        )
        .unwrap()
    }
    pub fn received_spec(&self, id: &str) -> serde_json::Value {
        serde_json::to_value(
            &self
                .endpoint
                .journal
                .lock()
                .unwrap()
                .jobs
                .get(id)
                .unwrap()
                .spec,
        )
        .unwrap()
    }
}

pub async fn setup(pool: &PgPool, behavior: Behavior) -> Harness {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    let mut data = tasks::data::setup(pool, None).await;
    let root = data.directory.path();
    // Native SecretVault opens an existing private directory; the fixture must
    // provision it just as the real init-state command does.
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join("worker-secrets"))
            .unwrap();
    }
    SecretVault::initialize_key(&root.join("worker-master.key")).unwrap();
    let vault = SecretVault::open(
        &root.join("worker-secrets"),
        &root.join("worker-master.key"),
    )
    .unwrap();
    let secret = random_capability();
    let reference = vault.put("RUNTIME", secret.as_bytes()).unwrap();
    let mut configuration = data.runtime.configuration.clone();
    configuration.endpoint = origin.clone();
    configuration.development_http = true;
    data.runtime = data
        .store
        .update_runtime(
            &data.actor,
            "worker-runtime-transport",
            data.runtime.id,
            &RuntimeUpdate {
                schema_version: SchemaV1,
                expected_revision: data.runtime.revision,
                configuration,
                credential_ref: Some(reference),
                ca_certificate_ref: None,
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    let fixture = tasks::prepare(pool, data).await;
    let targets = RuntimeTargets::new(
        vec![RuntimeTarget {
            origin,
            addresses: vec![address],
        }],
        true,
    )
    .unwrap();
    let worker = Worker::new(
        fixture.data.store.clone(),
        vault,
        ArtifactStore::open(&fixture.data.directory.path().join("objects")).unwrap(),
        targets.clone(),
        2,
    )
    .unwrap();
    let endpoint = Arc::new(Endpoint {
        secret,
        behavior,
        journal: Mutex::new(Journal::default()),
    });
    let router = Router::new()
        .route("/runtime/v1/objects/{id}", put(upload))
        .route("/runtime/v1/jobs", post(submit))
        .route("/runtime/v1/jobs/{id}", get(status))
        .route("/runtime/v1/jobs/{id}/cancel", post(cancel))
        .route("/runtime/v1/jobs/{id}/result", get(result))
        .route("/runtime/v1/jobs/{id}/artifacts/{object}", get(output))
        .with_state(endpoint.clone());
    let listener = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Harness {
        fixture,
        worker,
        targets,
        endpoint,
        listener,
    }
}

fn authorized(headers: &HeaderMap, endpoint: &Endpoint) -> bool {
    headers.get_all(header::AUTHORIZATION).iter().count() == 1
        && headers
            .get(header::AUTHORIZATION)
            .is_some_and(|value| value == format!("Bearer {}", endpoint.secret).as_str())
        && !headers.contains_key(header::COOKIE)
}
fn now() -> DateTime<Utc> {
    DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap()
}
fn json<T: serde::Serialize>(status: StatusCode, value: T) -> Response {
    (status, Json(value)).into_response()
}
fn empty(status: StatusCode) -> Response {
    status.into_response()
}

async fn upload(
    State(endpoint): State<Arc<Endpoint>>,
    Path(id): Path<Id>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    if !authorized(&headers, &endpoint) {
        return empty(StatusCode::UNAUTHORIZED);
    }
    if headers
        .get("x-qz-storage-version")
        .and_then(|v| v.to_str().ok())
        != Some("1")
    {
        return empty(StatusCode::CONFLICT);
    }
    let mut journal = endpoint.journal.lock().unwrap();
    journal.counts.uploads += 1;
    if let Some(existing) = journal.inputs.get(&id) {
        assert_eq!(
            existing.as_slice(),
            bytes.as_ref(),
            "native input cannot change under an object identity"
        );
    } else {
        journal.inputs.insert(id, bytes.to_vec());
    }
    json(
        StatusCode::CREATED,
        RuntimeObjectReceiptV1 {
            schema_version: SchemaV1,
            artifact_id: id,
            storage_version: "1".into(),
            byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
        },
    )
}

async fn submit(
    State(endpoint): State<Arc<Endpoint>>,
    headers: HeaderMap,
    Json(spec): Json<JobSpecV1>,
) -> Response {
    if !authorized(&headers, &endpoint) {
        return empty(StatusCode::UNAUTHORIZED);
    }
    domain::runtime_jobs::spec_shape(&spec).unwrap();
    let mut journal = endpoint.journal.lock().unwrap();
    journal.counts.submits += 1;
    if journal.jobs.contains_key(&spec.external_job_id) {
        return empty(StatusCode::CONFLICT);
    }
    let parameters: NativeTaskParametersV1 = serde_json::from_slice(
        journal
            .inputs
            .get(&spec.parameters_artifact_id)
            .expect("parameters were not uploaded before submit"),
    )
    .unwrap();
    let NativeTaskParametersV1::ValidateData { selections, .. } = parameters else {
        panic!("wrong operation in controlled native Worker scenario")
    };
    let at = now();
    let mut quality = tasks::data::catalog_fixture::metadata().quality;
    quality.checked_at = at;
    assert_eq!(selections.len(), 1);
    quality.datasets[0].dataset_revision_id = selections[0].dataset_revision_id;
    quality.datasets[0].selection = selections[0].selection.clone();
    let mut bytes = serde_json::to_vec(&quality).unwrap();
    let object = Id::new();
    let descriptor = RuntimeOutputV1 {
        kind: RuntimeOutputKind::DataQuality,
        schema: spec.requested_output_schemas[0].clone(),
        storage_ref: object,
        storage_version: Revision::INITIAL,
        byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
        media_type: "application/json".into(),
    };
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: spec.attempt_no,
        external_job_id: spec.external_job_id.clone(),
        input_set_id: spec.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: BTreeMap::from([("controlled-fixture".into(), "1".into())]),
        started_at: Some(at),
        finished_at: at,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: DbCounter::ZERO,
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: descriptor.byte_count,
        },
        artifacts: vec![descriptor],
        error: None,
    };
    if endpoint.behavior == Behavior::InvalidPayload {
        bytes[0] = b'!';
    }
    let status = RuntimeJobStatusV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: spec.attempt_no,
        external_job_id: spec.external_job_id.clone(),
        state: RuntimeJobState::Succeeded,
        has_result: true,
        submitted_at: at,
        started_at: Some(at),
        finished_at: Some(at),
    };
    journal.jobs.insert(
        spec.external_job_id.clone(),
        NativeJob {
            spec,
            status: status.clone(),
            manifest: serde_json::to_vec(&manifest).unwrap(),
            outputs: BTreeMap::from([(object, bytes)]),
        },
    );
    if endpoint.behavior == Behavior::InvalidPayload {
        json(StatusCode::ACCEPTED, status)
    } else {
        empty(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn status(
    State(endpoint): State<Arc<Endpoint>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&headers, &endpoint) {
        return empty(StatusCode::UNAUTHORIZED);
    }
    let mut journal = endpoint.journal.lock().unwrap();
    journal.counts.queries += 1;
    if endpoint.behavior == Behavior::MissingUntilCancelled {
        return empty(StatusCode::NOT_FOUND);
    }
    match journal.jobs.get(&id) {
        Some(job) => json(StatusCode::OK, job.status.clone()),
        None => empty(StatusCode::NOT_FOUND),
    }
}

async fn cancel(
    State(endpoint): State<Arc<Endpoint>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(command): Json<RuntimeCancelV1>,
) -> Response {
    if !authorized(&headers, &endpoint) {
        return empty(StatusCode::UNAUTHORIZED);
    }
    assert_eq!(
        domain::runtime_jobs::parse_external_id(&id).unwrap(),
        (command.run_id, command.attempt_no)
    );
    let mut journal = endpoint.journal.lock().unwrap();
    journal.counts.cancels += 1;
    let job = journal.jobs.get_mut(&id).unwrap();
    assert!(command.owner_epoch >= job.spec.owner_epoch);
    if endpoint.behavior == Behavior::MissingUntilCancelled {
        job.status.state = RuntimeJobState::Cancelled;
        job.status.has_result = false;
        job.status.started_at = None;
        job.status.finished_at = Some(now());
    }
    json(StatusCode::OK, job.status.clone())
}

async fn result(
    State(endpoint): State<Arc<Endpoint>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&headers, &endpoint) {
        return empty(StatusCode::UNAUTHORIZED);
    }
    let journal = endpoint.journal.lock().unwrap();
    let Some(job) = journal.jobs.get(&id) else {
        return empty(StatusCode::NOT_FOUND);
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(job.manifest.clone()))
        .unwrap()
}

async fn output(
    State(endpoint): State<Arc<Endpoint>>,
    Path((id, object)): Path<(String, Id)>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&headers, &endpoint) {
        return empty(StatusCode::UNAUTHORIZED);
    }
    let mut journal = endpoint.journal.lock().unwrap();
    journal.counts.output_reads += 1;
    let bytes = journal
        .jobs
        .get(&id)
        .and_then(|job| job.outputs.get(&object))
        .cloned();
    match bytes {
        Some(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(bytes))
            .unwrap(),
        None => empty(StatusCode::NOT_FOUND),
    }
}
