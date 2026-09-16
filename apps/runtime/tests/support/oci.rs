//! Actual private runtime process and native Docker resources used only by OCI acceptance.
#![allow(dead_code)]
use bollard::{
    models::ContainerInspectResponse, query_parameters::ListContainersOptionsBuilder, Docker,
};
use contracts::{
    execution::NativeTaskParametersV1,
    research::ArtifactInputRole,
    runtime::{RuntimeCapabilitiesV1, RuntimeImageV1},
    runtime_jobs::*,
    DbCounter, Id, Revision, SchemaV1,
};
use reqwest::{Client, StatusCode, Url};
use runtime::{
    config::{ImageRegistration, RuntimeConfig},
    now,
};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::{
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

pub const SECRET: &str = "native-oci-fixture-private-runtime-credential";
pub const SIGNAL: &str = r#"#![no_std]
#[panic_handler] fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
#[no_mangle] pub extern "C" fn predict(close:f64,_previous:f64,fast:f64,slow:f64,_volume:f64,_open:f64,_high:f64,_low:f64)->f64 {
    (fast-slow)/close
}
"#;
pub const SLOW_SIGNAL: &str = r#"#![no_std]
#![allow(long_running_const_eval)]
#[panic_handler] fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
const SLOW:u64 = { let mut i=0u64; let mut n=0u64; while i<10_000_000_000 { n=n.wrapping_add(i); i+=1; } n };
#[no_mangle] pub extern "C" fn predict(a:f64,b:f64,c:f64,d:f64,e:f64,f:f64,g:f64,h:f64)->f64 { (SLOW as f64)+a+b+c+d+e+f+g+h }
"#;

pub fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
pub fn image() -> String {
    let image = std::env::var("QUAZONAI_NATIVE_JOB_IMAGE").expect(
        "native-oci requires a genuinely built native image; absence is not a skipped success",
    );
    assert!(
        image.starts_with("sha256:") && image.len() == 71 && domain::runtime::pinned_image(&image)
    );
    image
}
pub fn docker_socket() -> PathBuf {
    PathBuf::from(
        std::env::var("QUAZONAI_DOCKER_SOCKET")
            .expect("native-oci requires an explicitly selected native Docker socket"),
    )
}
pub async fn docker() -> Docker {
    Docker::connect_with_socket(
        docker_socket().to_str().unwrap(),
        5,
        bollard::API_DEFAULT_VERSION,
    )
    .unwrap()
    .negotiate_version()
    .await
    .expect("native Docker must actually be available for OCI acceptance")
}

pub struct Fixture {
    pub client: Client,
    pub origin: Url,
    pub config_path: PathBuf,
    pub config: RuntimeConfig,
    pub child: Option<Child>,
    pub runs: Vec<Id>,
    pub directory: tempfile::TempDir,
}
impl Fixture {
    pub async fn open() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let credential = directory.path().join("credential");
        fs::write(&credential, SECRET).unwrap();
        fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
        let image = image();
        let config = RuntimeConfig {
            schema_version: SchemaV1,
            state_dir: directory.path().join("state"),
            credential_file: credential,
            docker_socket: docker_socket(),
            bind: address,
            images: [
                contracts::runs::RunKind::DataValidate,
                contracts::runs::RunKind::AlphaEvaluate,
                contracts::runs::RunKind::PortfolioBuild,
                contracts::runs::RunKind::PortfolioSimulate,
            ]
            .into_iter()
            .map(|job_kind| ImageRegistration {
                job_kind,
                image_ref: image.clone(),
            })
            .collect(),
            catalogs: Vec::new(),
            max_cpu: 1,
            max_memory_mib: 1024,
            max_wall_seconds: 120,
            max_output_bytes: 64 * 1024 * 1024,
            max_parallel_jobs: 2,
            max_pending_jobs: 8,
            storage_quota_bytes: 256 * 1024 * 1024,
        };
        let config_path = directory.path().join("runtime.json");
        // Exact known fields; the production reader, not a test-specific config parser, consumes this.
        let config_json = json!({
            "schema_version":1,"state_dir":config.state_dir,"credential_file":config.credential_file,
            "docker_socket":config.docker_socket,"bind":config.bind.to_string(),
            "images":config.images.iter().map(|registration| json!({"job_kind":registration.job_kind,"image_ref":registration.image_ref})).collect::<Vec<_>>(),
            "catalogs":[],"max_cpu":1,"max_memory_mib":1024,"max_wall_seconds":120,
            "max_output_bytes":67108864,"max_parallel_jobs":2,"max_pending_jobs":8,"storage_quota_bytes":268435456,
        });
        fs::write(&config_path, serde_json::to_vec(&config_json).unwrap()).unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        let mut authorization =
            reqwest::header::HeaderValue::from_str(&format!("Bearer {SECRET}")).unwrap();
        authorization.set_sensitive(true);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::AUTHORIZATION, authorization);
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        let origin = Url::parse(&format!("http://{address}")).unwrap();
        let mut fixture = Self {
            client,
            origin,
            config_path,
            config,
            child: None,
            runs: Vec::new(),
            directory,
        };
        fixture.restart().await;
        let capabilities: RuntimeCapabilitiesV1 = fixture
            .json(
                reqwest::Method::GET,
                &["capabilities"],
                None,
                &[StatusCode::OK],
            )
            .await;
        domain::runtime::capabilities(&capabilities, now()).unwrap();
        assert!(capabilities
            .image_refs
            .iter()
            .all(|RuntimeImageV1 { image_ref, .. }| *image_ref == image));
        fixture
    }
    pub fn url(&self, segments: &[&str]) -> Url {
        let mut url = self.origin.clone();
        url.path_segments_mut()
            .unwrap()
            .clear()
            .extend(["runtime", "v1"])
            .extend(segments.iter().copied());
        url
    }
    pub async fn restart(&mut self) {
        assert!(self.child.is_none());
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.directory.path().join("gateway.log"))
            .unwrap();
        self.child = Some(
            Command::new(env!("CARGO_BIN_EXE_runtime"))
                .args(["serve", "--config"])
                .arg(&self.config_path)
                .env_clear()
                .env(
                    "QUAZONAI_TEST_HOST_SECRET",
                    "untrusted-job-must-not-inherit-this-fixture-value",
                )
                .stdin(Stdio::null())
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap(),
        );
        let id = domain::runtime_jobs::external_id(Id::new(), 1).unwrap();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                assert!(
                    self.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                    "native runtime exited before listening; inspect private gateway log"
                );
                if let Ok(response) = self.client.get(self.url(&["jobs", &id])).send().await {
                    if response.status() == StatusCode::NOT_FOUND {
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("native gateway startup deadline");
    }
    pub fn crash(&mut self) {
        let mut child = self.child.take().expect("owned native gateway process");
        child.kill().unwrap();
        child.wait().unwrap();
    }
    pub async fn json<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        segments: &[&str],
        body: Option<serde_json::Value>,
        statuses: &[StatusCode],
    ) -> T {
        let mut request = self.client.request(method, self.url(segments));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.expect("native runtime request failed");
        assert!(
            statuses.contains(&response.status()),
            "native HTTP returned unexpected status {}",
            response.status()
        );
        assert_eq!(
            response.headers()[reqwest::header::CACHE_CONTROL],
            "no-store"
        );
        let bytes = response.bytes().await.unwrap();
        assert!(bytes.len() <= 1024 * 1024);
        assert!(
            !bytes
                .windows(SECRET.len())
                .any(|part| part == SECRET.as_bytes()),
            "native response reflected credential"
        );
        serde_json::from_slice(&bytes).expect("native response did not satisfy shared DTO")
    }
    pub async fn object(&self, id: Id, bytes: &[u8]) {
        let response = self
            .client
            .put(self.url(&["objects", &id.to_string()]))
            .header("content-type", "application/octet-stream")
            .header("x-qz-storage-version", "1")
            .body(bytes.to_vec())
            .send()
            .await
            .unwrap();
        assert!(matches!(
            response.status(),
            StatusCode::OK | StatusCode::CREATED
        ));
        let receipt: RuntimeObjectReceiptV1 = response.json().await.unwrap();
        assert_eq!(receipt.artifact_id, id);
        assert_eq!(receipt.byte_count.get(), bytes.len() as u64);
        assert_eq!(receipt.storage_version, "1");
    }
    pub async fn compile(&mut self, code: &str, wall_seconds: u32) -> JobSpecV1 {
        let source = Id::new();
        let parameters = Id::new();
        let operation = NativeTaskParametersV1::CompileModel {
            schema_version: SchemaV1,
            code_artifact_id: source,
        };
        self.object(source, code.as_bytes()).await;
        self.object(parameters, &serde_json::to_vec(&operation).unwrap())
            .await;
        let run = Id::new();
        self.runs.push(run);
        JobSpecV1 {
            schema_version: SchemaV1,
            run_id: run,
            attempt_no: 1,
            owner_epoch: Revision::INITIAL,
            external_job_id: domain::runtime_jobs::external_id(run, 1).unwrap(),
            job_kind: operation.job_kind(),
            image_ref: image(),
            input_set_id: Id::new(),
            inputs: vec![RuntimeInputV1::Artifact {
                artifact_id: source,
                storage_version: "1".into(),
                byte_count: count(code.len() as u64),
                role: ArtifactInputRole::Code,
            }],
            parameters_artifact_id: parameters,
            limits: RuntimeJobLimitsV1 {
                cpu: 1,
                cpu_seconds: count(u64::from(wall_seconds)),
                memory_mib: 512,
                wall_seconds,
                output_bytes: count(4 * 1024 * 1024),
            },
            deadline_at: now() + chrono::Duration::seconds(i64::from(wall_seconds) + 20),
            requested_output_schemas: operation.output_schemas(),
        }
    }
    pub async fn submit(&self, spec: &JobSpecV1) -> RuntimeJobStatusV1 {
        self.json(
            reqwest::Method::POST,
            &["jobs"],
            Some(serde_json::to_value(spec).unwrap()),
            &[StatusCode::OK, StatusCode::ACCEPTED],
        )
        .await
    }
    pub async fn status(&self, spec: &JobSpecV1) -> RuntimeJobStatusV1 {
        self.json(
            reqwest::Method::GET,
            &["jobs", &spec.external_job_id],
            None,
            &[StatusCode::OK],
        )
        .await
    }
    pub async fn terminal(&self, spec: &JobSpecV1) -> RuntimeJobStatusV1 {
        tokio::time::timeout(
            Duration::from_secs(u64::from(spec.limits.wall_seconds) + 40),
            async {
                loop {
                    let status = self.status(spec).await;
                    if status.state.is_terminal() {
                        return status;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
        )
        .await
        .expect("native runtime terminal reconciliation deadline")
    }
    pub async fn manifest(&self, spec: &JobSpecV1) -> ResultManifestV1 {
        self.json(
            reqwest::Method::GET,
            &["jobs", &spec.external_job_id, "result"],
            None,
            &[StatusCode::OK],
        )
        .await
    }
    pub async fn native_container(&self, spec: &JobSpecV1) -> ContainerInspectResponse {
        tokio::time::timeout(Duration::from_secs(20), async {
            let docker = docker().await;
            let mut filters = std::collections::HashMap::<String, Vec<String>>::new();
            filters.insert(
                "label".into(),
                vec![format!("io.quazonai.run={}", spec.run_id)],
            );
            loop {
                let options = ListContainersOptionsBuilder::default()
                    .all(true)
                    .filters(&filters)
                    .build();
                let containers = docker.list_containers(Some(options)).await.unwrap();
                assert!(
                    containers.len() <= 1,
                    "stable external identity created multiple native containers"
                );
                if let Some(container) = containers.first() {
                    return docker
                        .inspect_container(container.id.as_deref().unwrap(), None)
                        .await
                        .unwrap();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("native container creation deadline")
    }
    pub fn assert_private_logs(&self) {
        let log = fs::read(self.directory.path().join("gateway.log")).unwrap();
        assert!(
            !log.windows(SECRET.len())
                .any(|part| part == SECRET.as_bytes()),
            "native process log exposed credential"
        );
        assert!(
            !log.windows(SIGNAL.len())
                .any(|part| part == SIGNAL.as_bytes()),
            "native process log exposed source body"
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // Only explicit UUIDs generated for this fixture can be removed. This is native
        // test cleanup, not a product retention/identity deletion endpoint.
        for run in &self.runs {
            let result = Command::new("timeout")
                .args(["--kill-after=2s", "15s", "docker", "--host"])
                .arg(format!("unix://{}", docker_socket().display()))
                .args(["ps", "--all", "--quiet", "--no-trunc", "--filter"])
                .arg(format!("label=io.quazonai.run={run}"))
                .stdin(Stdio::null())
                .output();
            if let Ok(result) = result {
                if !result.status.success() {
                    continue;
                }
                for id in String::from_utf8_lossy(&result.stdout).lines() {
                    if id.len() == 64
                        && id
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    {
                        let _ = Command::new("timeout")
                            .args(["--kill-after=2s", "15s", "docker", "--host"])
                            .arg(format!("unix://{}", docker_socket().display()))
                            .args(["rm", "--force", "--volumes", id])
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status();
                    }
                }
            }
        }
    }
}
