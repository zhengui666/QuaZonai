//! Thin native Docker API adapter. No Docker CLI, ambient socket discovery or retrying START.
use crate::{
    config::{RegisteredCatalog, RuntimeConfig},
    now, Failure, Result,
};
use bollard::{
    errors::Error as DockerError,
    models::*,
    query_parameters::{CreateContainerOptionsBuilder, StatsOptionsBuilder},
    Docker, API_DEFAULT_VERSION,
};
use chrono::{DateTime, Utc};
use contracts::{runtime::*, runtime_jobs::JobSpecV1, DbCounter, Id, SchemaV1};
use futures_util::StreamExt;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
    sync::Arc,
    time::Duration,
};

pub const NATIVE_STACK: &str = "rust/1.98.1;nautilus/0.63.0;clarabel/0.11.1;wasmi/2.0.0";
pub const JOB_ENTRYPOINT: &str = "/usr/local/bin/job";

#[derive(Clone)]
pub struct NativeEngine {
    docker: Arc<tokio::sync::OnceCell<Docker>>,
    config: Arc<RuntimeConfig>,
}

pub struct NativeImage {
    pub id: String,
    pub versions: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct ContainerObservation {
    pub id: String,
    pub role: String,
    pub running: bool,
    pub created_only: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i64>,
    pub oom_killed: bool,
}

#[derive(Clone, Copy, Default)]
pub struct NativeUsage {
    pub cpu_nanoseconds: Option<u64>,
    pub peak_memory_bytes: Option<u64>,
}

fn engine_error(_: DockerError) -> Failure {
    Failure::Engine
}
fn missing(error: &DockerError) -> bool {
    matches!(
        error,
        DockerError::DockerResponseServerError {
            status_code: 404,
            ..
        }
    )
}
fn collision(error: &DockerError) -> bool {
    matches!(
        error,
        DockerError::DockerResponseServerError {
            status_code: 409,
            ..
        }
    )
}
fn native_time(value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.starts_with("0001-01-01T00:00:00") {
        return Ok(None);
    }
    let parsed = DateTime::parse_from_rfc3339(value).map_err(|_| Failure::Integrity)?;
    DateTime::from_timestamp_micros(parsed.timestamp_micros())
        .map(Some)
        .ok_or(Failure::Integrity)
}
fn native_container_id(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Failure::Integrity);
    }
    Ok(())
}
fn labels(instance: Id, spec: &JobSpecV1, role: &str) -> HashMap<String, String> {
    HashMap::from([
        ("io.quazonai.runtime".into(), instance.to_string()),
        ("io.quazonai.run".into(), spec.run_id.to_string()),
        ("io.quazonai.attempt".into(), spec.attempt_no.to_string()),
        (
            "io.quazonai.external-id".into(),
            spec.external_job_id.clone(),
        ),
        ("io.quazonai.role".into(), role.into()),
    ])
}
pub fn container_name(instance: Id, spec: &JobSpecV1) -> String {
    format!("quazonai-{}-{}-{}", instance, spec.run_id, spec.attempt_no)
}
fn ulimit(name: &str, value: i64) -> ResourcesUlimits {
    ResourcesUlimits {
        name: Some(name.into()),
        soft: Some(value),
        hard: Some(value),
    }
}
pub fn bind(source: &Path, destination: &str, readonly: bool) -> Result<Mount> {
    Ok(Mount {
        typ: Some(MountType::BIND),
        source: Some(
            source
                .to_str()
                .ok_or(Failure::Invalid("native_mount_path"))?
                .to_owned(),
        ),
        target: Some(destination.to_owned()),
        read_only: Some(readonly),
        ..Default::default()
    })
}

impl NativeEngine {
    pub fn new(config: Arc<RuntimeConfig>) -> Result<Self> {
        if !config.docker_socket.is_absolute() || config.docker_socket.to_str().is_none() {
            return Err(Failure::Invalid("docker_socket"));
        }
        Ok(Self {
            docker: Arc::new(tokio::sync::OnceCell::new()),
            config,
        })
    }

    async fn client(&self) -> Result<&Docker> {
        // Journal/status/tombstones remain usable during daemon downtime. Only
        // actual engine operations create the native client and negotiate its API.
        self.docker
            .get_or_try_init(|| async {
                let socket = self
                    .config
                    .docker_socket
                    .to_str()
                    .ok_or(Failure::Invalid("docker_socket"))?;
                Docker::connect_with_socket(socket, 3, API_DEFAULT_VERSION)
                    .map_err(engine_error)?
                    .negotiate_version()
                    .await
                    .map_err(engine_error)
            })
            .await
    }
    pub async fn image(&self, reference: &str) -> Result<NativeImage> {
        if !self
            .config
            .images
            .iter()
            .any(|image| image.image_ref == reference)
        {
            return Err(Failure::Invalid("image_ref"));
        }
        let image = self
            .client()
            .await?
            .inspect_image(reference)
            .await
            .map_err(engine_error)?;
        let configuration = image.config.as_ref().ok_or(Failure::Integrity)?;
        let labels = configuration
            .labels
            .as_ref()
            .ok_or(Failure::Invalid("native_image_contract"))?;
        if image.os.as_deref() != Some("linux")
            || image.architecture.as_deref() != Some("amd64")
            || labels.get("io.quazonai.native-job").map(String::as_str) != Some("1")
            || labels.get("io.quazonai.native-stack").map(String::as_str) != Some(NATIVE_STACK)
            || configuration.entrypoint.as_deref() != Some(&[JOB_ENTRYPOINT.to_owned()])
            || configuration
                .volumes
                .as_ref()
                .is_some_and(|volumes| !volumes.is_empty())
            || configuration
                .on_build
                .as_ref()
                .is_some_and(|commands| !commands.is_empty())
        {
            return Err(Failure::Invalid("native_image_contract"));
        }
        let id = image.id.ok_or(Failure::Integrity)?;
        if !id.starts_with("sha256:") || id.len() != 71 {
            return Err(Failure::Integrity);
        }
        Ok(NativeImage {
            id,
            versions: BTreeMap::from([
                ("rustc".into(), "1.98.1".into()),
                ("nautilus".into(), "0.63.0".into()),
                ("clarabel".into(), "0.11.1".into()),
                ("wasmi".into(), "2.0.0".into()),
            ]),
        })
    }

    pub async fn capabilities(
        &self,
        catalogs: &[RegisteredCatalog],
    ) -> Result<RuntimeCapabilitiesV1> {
        tokio::time::timeout(Duration::from_secs(8), async {
            let info = self.client().await?.info().await.map_err(engine_error)?;
            let native = serde_json::to_value(&info)?;
            if native["OSType"] != "linux"
                || native["CgroupVersion"] != "2"
                || native["MemoryLimit"] != true
                || native["PidsLimit"] != true
                || native["CpuCfsQuota"] != true
            {
                return Err(Failure::Invalid("native_isolation_unavailable"));
            }
            let cpu = native["NCPU"].as_u64().ok_or(Failure::Integrity)?;
            let memory = native["MemTotal"].as_u64().ok_or(Failure::Integrity)? / (1024 * 1024);
            let version = self
                .client()
                .await?
                .version()
                .await
                .map_err(engine_error)?
                .version
                .ok_or(Failure::Integrity)?;
            let mut versions = BTreeMap::from([("docker".into(), version)]);
            let mut images = Vec::new();
            let mut kinds = Vec::new();
            for registration in &self.config.images {
                let image = self.image(&registration.image_ref).await?;
                versions.extend(image.versions);
                images.push(RuntimeImageV1 {
                    job_kind: registration.job_kind,
                    image_ref: registration.image_ref.clone(),
                });
                kinds.push(registration.job_kind);
            }
            let mut venue_classes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            // Only native source definitions for instrument classes the actual job supports.
            // Unrecognized definitions advertise no venue, never implicit market readiness.
            for catalog in catalogs {
                for definition in &catalog.metadata.universe.instrument_definitions {
                    let Some(class) = definition.get("type").and_then(serde_json::Value::as_str)
                    else {
                        continue;
                    };
                    if !matches!(class, "CurrencyPair" | "Equity") {
                        continue;
                    }
                    let Some(id) = definition.get("id").and_then(serde_json::Value::as_str) else {
                        continue;
                    };
                    let Some((_, venue)) = id.rsplit_once('.') else {
                        continue;
                    };
                    if venue.is_empty() {
                        continue;
                    }
                    venue_classes
                        .entry(venue.to_owned())
                        .or_default()
                        .insert(class.to_owned());
                }
            }
            let result = RuntimeCapabilitiesV1 {
                schema_version: SchemaV1,
                protocol_versions: vec![SchemaV1],
                runtime_version: env!("CARGO_PKG_VERSION").into(),
                engine_versions: versions,
                image_refs: images,
                job_kinds: kinds,
                artifact_schemas: [
                    "qz.wasm_model",
                    "qz.model_compilation",
                    "qz.data_quality",
                    "qz.native_forecast",
                    "qz.native_allocation",
                    "qz.native_simulation",
                ]
                .into_iter()
                .map(|name| RuntimeArtifactSchemaV1 {
                    name: name.into(),
                    version: "1".into(),
                })
                .collect(),
                data_kinds: vec![RuntimeDataKind::Bar],
                venues: venue_classes
                    .into_iter()
                    .map(|(venue, classes)| RuntimeVenueV1 {
                        venue,
                        instrument_classes: classes.into_iter().collect(),
                        data_kinds: vec![RuntimeDataKind::Bar],
                        expiry_and_settlement: false,
                    })
                    .collect(),
                label_interval_support: LabelIntervalSupportV1 {
                    fixed_bars: true,
                    fixed_duration: false,
                    variable_interval: false,
                },
                solver_capabilities: vec!["CONVEX_QP".into(), "SECOND_ORDER_CONE".into()],
                max_cpu: self
                    .config
                    .max_cpu
                    .min(u16::try_from(cpu).unwrap_or(u16::MAX)),
                max_memory_mib: self
                    .config
                    .max_memory_mib
                    .min(u32::try_from(memory).unwrap_or(u32::MAX)),
                max_output_bytes: DbCounter::new(self.config.max_output_bytes)
                    .map_err(|_| Failure::Integrity)?,
                max_wall_seconds: self.config.max_wall_seconds,
                isolation_profile: IsolationProfile::OciResearchV1,
                checked_at: now(),
            };
            domain::runtime::capabilities(&result, now()).map_err(|_| Failure::Integrity)?;
            Ok(result)
        })
        .await
        .map_err(|_| Failure::Engine)?
    }

    pub fn launch(
        instance: Id,
        spec: &JobSpecV1,
        image: &NativeImage,
        mounts: Vec<Mount>,
        barrier: bool,
    ) -> Result<ContainerCreateBody> {
        let memory = i64::from(spec.limits.memory_mib) * 1024 * 1024;
        let tmp = (memory / 4).clamp(16 * 1024 * 1024, 256 * 1024 * 1024);
        let file_limit = i64::try_from(spec.limits.output_bytes.get().max(1024 * 1024))
            .map_err(|_| Failure::Integrity)?;
        let cpu_seconds =
            i64::try_from(spec.limits.cpu_seconds.get()).map_err(|_| Failure::Integrity)?;
        let host_config = HostConfig {
            memory: Some(memory),
            memory_swap: Some(memory),
            nano_cpus: Some(i64::from(spec.limits.cpu) * 1_000_000_000),
            pids_limit: Some(64),
            readonly_rootfs: Some(true),
            privileged: Some(false),
            cap_drop: Some(vec!["ALL".into()]),
            security_opt: Some(vec!["no-new-privileges:true".into()]),
            network_mode: Some("none".into()),
            init: Some(true),
            auto_remove: Some(false),
            mounts: Some(if barrier { vec![] } else { mounts }),
            tmpfs: Some(HashMap::from([(
                "/tmp".into(),
                format!("rw,noexec,nosuid,nodev,size={tmp},mode=1777"),
            )])),
            shm_size: Some(1024 * 1024),
            ulimits: Some(vec![
                ulimit("fsize", file_limit),
                ulimit("core", 0),
                ulimit("nofile", 64),
                ulimit("cpu", cpu_seconds),
            ]),
            log_config: Some(HostConfigLogConfig {
                typ: Some("none".into()),
                config: None,
            }),
            ..Default::default()
        };
        let mut native_labels = labels(instance, spec, if barrier { "TOMBSTONE" } else { "JOB" });
        native_labels.insert(
            "io.quazonai.engine-versions".into(),
            serde_json::to_string(&image.versions)?,
        );
        Ok(ContainerCreateBody {
            image: Some(image.id.clone()),
            user: Some("65532:65532".into()),
            entrypoint: Some(vec![JOB_ENTRYPOINT.into()]),
            cmd: Some(vec![
                if barrier { "--version" } else { "run-bounded" }.into()
            ]),
            working_dir: Some("/tmp".into()),
            env: Some(vec![
                "HOME=/tmp".into(),
                "TMPDIR=/tmp".into(),
                "PATH=/opt/rust/bin:/usr/bin:/bin".into(),
            ]),
            labels: Some(native_labels),
            host_config: Some(host_config),
            network_disabled: Some(true),
            tty: Some(false),
            open_stdin: Some(false),
            attach_stdin: Some(false),
            attach_stdout: Some(false),
            attach_stderr: Some(false),
            ..Default::default()
        })
    }

    pub async fn create(&self, name: &str, launch: ContainerCreateBody) -> Result<Option<String>> {
        let options = CreateContainerOptionsBuilder::default().name(name).build();
        match self
            .client()
            .await?
            .create_container(Some(options), launch)
            .await
        {
            Ok(created) => {
                native_container_id(&created.id)?;
                Ok(Some(created.id))
            }
            Err(error) if collision(&error) => Ok(None),
            Err(error) => Err(engine_error(error)),
        }
    }
    pub async fn inspect(
        &self,
        name_or_id: &str,
        instance: Id,
        spec: &JobSpecV1,
    ) -> Result<Option<ContainerObservation>> {
        let inspected = match self
            .client()
            .await?
            .inspect_container(name_or_id, None)
            .await
        {
            Ok(value) => value,
            Err(error) if missing(&error) => return Ok(None),
            Err(error) => return Err(engine_error(error)),
        };
        let id = inspected.id.ok_or(Failure::Integrity)?;
        native_container_id(&id)?;
        let configuration = inspected.config.ok_or(Failure::Integrity)?;
        let actual = configuration.labels.ok_or(Failure::Integrity)?;
        let role = actual
            .get("io.quazonai.role")
            .ok_or(Failure::Integrity)?
            .clone();
        if !matches!(role.as_str(), "JOB" | "TOMBSTONE")
            || labels(instance, spec, &role)
                .iter()
                .any(|(key, value)| actual.get(key) != Some(value))
            || configuration.user.as_deref() != Some("65532:65532")
            || configuration.entrypoint.as_deref() != Some(&[JOB_ENTRYPOINT.to_owned()])
            || inspected.restart_count.unwrap_or(0) != 0
        {
            return Err(Failure::Integrity);
        }
        let host = inspected.host_config.ok_or(Failure::Integrity)?;
        if host.privileged.unwrap_or(false)
            || host.readonly_rootfs != Some(true)
            || host.network_mode.as_deref() != Some("none")
        {
            return Err(Failure::Integrity);
        }
        let state = inspected.state.ok_or(Failure::Integrity)?;
        if state.paused == Some(true) || state.restarting == Some(true) || state.dead == Some(true)
        {
            return Err(Failure::Engine);
        }
        let started = native_time(state.started_at.as_deref())?;
        let finished = native_time(state.finished_at.as_deref())?;
        let running = state.running.ok_or(Failure::Integrity)?;
        if role == "TOMBSTONE" && (running || started.is_some() || finished.is_some()) {
            return Err(Failure::Integrity);
        }
        Ok(Some(ContainerObservation {
            id,
            role,
            running,
            created_only: started.is_none() && !running,
            started_at: started,
            finished_at: finished,
            exit_code: state.exit_code,
            oom_killed: state.oom_killed.unwrap_or(false),
        }))
    }
    pub async fn start(&self, id: &str) -> Result<()> {
        native_container_id(id)?;
        self.client()
            .await?
            .start_container(id, None)
            .await
            .map_err(engine_error)
    }
    pub async fn kill(&self, id: &str) -> Result<()> {
        native_container_id(id)?;
        match self.client().await?.kill_container(id, None).await {
            Ok(()) => Ok(()),
            Err(error) if missing(&error) || collision(&error) => Ok(()),
            Err(error) => Err(engine_error(error)),
        }
    }
    /// Removes only an already inspected stopped native ID, never a mutable name.
    pub async fn remove_stopped(&self, id: &str) -> Result<()> {
        native_container_id(id)?;
        match self.client().await?.remove_container(id, None).await {
            Ok(()) => Ok(()),
            Err(error) if missing(&error) => Ok(()),
            Err(error) => Err(engine_error(error)),
        }
    }
    pub async fn usage(&self, id: &str) -> Result<NativeUsage> {
        native_container_id(id)?;
        let options = StatsOptionsBuilder::default()
            .stream(false)
            .one_shot(true)
            .build();
        let mut stats = self.client().await?.stats(id, Some(options));
        let value = tokio::time::timeout(Duration::from_secs(2), stats.next())
            .await
            .map_err(|_| Failure::Engine)?
            .ok_or(Failure::Engine)?
            .map_err(engine_error)?;
        let cpu_nanoseconds = value
            .cpu_stats
            .and_then(|cpu| cpu.cpu_usage)
            .and_then(|usage| usage.total_usage);
        // Docker omits cgroup-v2 max_usage. Current usage is not a measured peak.
        let peak_memory_bytes = value.memory_stats.and_then(|memory| memory.max_usage);
        Ok(NativeUsage {
            cpu_nanoseconds,
            peak_memory_bytes,
        })
    }
}
