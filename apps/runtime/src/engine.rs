//! Thin native Docker API adapter. No Docker CLI, ambient socket discovery or retrying START.
use crate::{
    Failure, Result,
    config::{RegisteredCatalog, RuntimeConfig},
    now,
};
use bollard::{
    API_DEFAULT_VERSION, Docker,
    errors::Error as DockerError,
    models::*,
    query_parameters::{CreateContainerOptionsBuilder, StatsOptionsBuilder},
};
use chrono::{DateTime, Utc};
use contracts::{DbCounter, Id, SchemaV1, runtime::*, runtime_jobs::JobSpecV1};
use futures_util::StreamExt;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
    sync::Arc,
    time::Duration,
};

pub const NATIVE_STACK: &str = "rust/1.98.1;nautilus/0.63.0;clarabel/0.11.1;wasmi/2.0.0;solow-cv/0.7.3;ndarray-stats/0.7.0;linregress/0.5.4;alpha-validation/1;alpha-sealed/1;portfolio-ensemble/1;portfolio-models/4;strategy-composition/1;portfolio-build-rolling/1;simulation-models/1;portfolio-weights/1;portfolio-variance-bound/1;portfolio-cvar/1;portfolio-risk-budget/1;portfolio-cvar-risk-budget/1;bar-notional/1;portfolio-liquidity/1;portfolio-cost-source/1;portfolio-slippage/1;candidate-simulation/2;portfolio-sequence/1;portfolio-study/6;portfolio-calendar/2;portfolio-rolling-liquidity/1;portfolio-history/1;polymarket-research/1;static-instruments/1;optional-execution-budgets/1;polymarket-target-policy/1";
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

/// Optional operator caps never manufacture host capacity. Explicit values above
/// physical capacity are configuration errors instead of silently lowered grants.
fn observed_grant(selected: Option<u32>, observed: u64) -> Result<u32> {
    let observed = u32::try_from(observed).map_err(|_| Failure::Invalid("host_resource_capacity"))?;
    if observed == 0 || selected.is_some_and(|value| value == 0 || value > observed) {
        return Err(Failure::Invalid("host_resource_capacity"));
    }
    Ok(selected.unwrap_or(observed))
}

const OPTIONAL_RESOURCE_CAPABILITIES: [&str; 2] = ["optional-cpu-rate", "optional-memory-limit"];

fn add_optional_resource_versions(
    versions: &mut BTreeMap<String, String>,
    labels: &HashMap<String, String>,
) {
    if labels
        .get("io.quazonai.optional-resource-quotas")
        .map(String::as_str)
        == Some("1")
    {
        versions.extend(OPTIONAL_RESOURCE_CAPABILITIES.map(|key| (key.into(), "1".into())));
    }
}

fn optional_resource_intersection(
    images: &[BTreeMap<String, String>],
    cpu_ceiling: Option<u32>,
    memory_ceiling: Option<u32>,
) -> BTreeMap<String, String> {
    OPTIONAL_RESOURCE_CAPABILITIES
        .into_iter()
        .zip([cpu_ceiling, memory_ceiling])
        .filter(|(key, ceiling)| {
            ceiling.is_none()
                && !images.is_empty()
                && images
                    .iter()
                    .all(|image| image.get(*key).map(String::as_str) == Some("1"))
        })
        .map(|(key, _)| (key.into(), "1".into()))
        .collect()
}

fn tmpfs_quota(memory: Option<i64>) -> String {
    // Linux tmpfs defines size=0,nr_inodes=0 as no mount-specific ceiling.
    // These native adapter flags never become a zero-valued application budget.
    memory.map_or_else(
        || "size=0,nr_inodes=0".into(),
        |bytes| format!("size={bytes}"),
    )
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
        let mut versions = BTreeMap::from([
            ("rustc".into(), "1.98.1".into()),
            ("optional-wall-time".into(), "1".into()),
            ("optional-cpu-budget".into(), "1".into()),
            ("optional-output-budget".into(), "1".into()),
            ("nautilus".into(), "0.63.0".into()),
            ("clarabel".into(), "0.11.1".into()),
            ("wasmi".into(), "2.0.0".into()),
            ("solow-cv".into(), "0.7.3".into()),
            ("ndarray-stats".into(), "0.7.0".into()),
            ("ndarray".into(), "0.17.1".into()),
            ("portfolio-ensemble".into(), "1".into()),
            ("portfolio-models".into(), "4".into()),
            ("strategy-composition".into(), "1".into()),
            ("portfolio-build-rolling".into(), "1".into()),
            ("simulation-models".into(), "1".into()),
            ("portfolio-weights".into(), "1".into()),
            ("portfolio-variance-bound".into(), "1".into()),
            ("portfolio-cvar".into(), "1".into()),
            ("portfolio-risk-budget".into(), "1".into()),
            ("portfolio-cvar-risk-budget".into(), "1".into()),
            ("bar-notional".into(), "1".into()),
            ("portfolio-liquidity".into(), "1".into()),
            ("portfolio-cost-source".into(), "1".into()),
            ("portfolio-slippage".into(), "1".into()),
            ("candidate-simulation".into(), "2".into()),
            ("portfolio-sequence".into(), "1".into()),
            ("portfolio-study".into(), "6".into()),
            ("portfolio-calendar".into(), "2".into()),
            ("portfolio-rolling-liquidity".into(), "1".into()),
            ("portfolio-history".into(), "1".into()),
            ("polymarket-research".into(), "1".into()),
            (
                contracts::settlement::BINARY_OPTION_V2_CAPABILITY.into(),
                "1".into(),
            ),
            ("linregress".into(), "0.5.4".into()),
        ]);
        add_optional_resource_versions(&mut versions, labels);
        Ok(NativeImage { id, versions })
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
            // Optional wall requires an absent cumulative CPU cap. A finite
            // CPU grant without wall remains unsupported by admission.
            let mut versions = BTreeMap::from([("docker".into(), version)]);
            let mut images = Vec::new();
            let mut kinds = Vec::new();
            let mut resource_versions = Vec::new();
            for registration in &self.config.images {
                let image = self.image(&registration.image_ref).await?;
                resource_versions.push(image.versions.clone());
                versions.extend(
                    image
                        .versions
                        .into_iter()
                        .filter(|(key, _)| !OPTIONAL_RESOURCE_CAPABILITIES.contains(&key.as_str())),
                );
                images.push(RuntimeImageV1 {
                    job_kind: registration.job_kind,
                    image_ref: registration.image_ref.clone(),
                });
                kinds.push(registration.job_kind);
            }
            versions.extend(optional_resource_intersection(
                &resource_versions,
                self.config.max_cpu,
                self.config.max_memory_mib,
            ));
            let mut venue_classes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            // Only native source definitions for instrument classes the actual job supports.
            // Unrecognized definitions advertise no venue, never implicit market readiness.
            for catalog in catalogs {
                for definition in &catalog.metadata.universe.instrument_definitions {
                    let Ok((class, definition)) =
                        domain::catalogs::instrument_definition(definition)
                    else {
                        continue;
                    };
                    if !(matches!(class, "CurrencyPair" | "Equity")
                        || (class == "BinaryOption"
                            && domain::prediction::instrument(definition).is_ok()))
                    {
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
                artifact_schemas: contracts::runtime_jobs::NATIVE_OUTPUT_CONTRACTS
                    .iter()
                    .map(|contract| RuntimeArtifactSchemaV1 {
                        name: contract.name.into(),
                        version: "1".into(),
                    })
                    .collect(),
                data_kinds: vec![RuntimeDataKind::Bar],
                venues: venue_classes
                    .into_iter()
                    .map(|(venue, classes)| RuntimeVenueV1 {
                        venue,
                        expiry_and_settlement: classes.contains("BinaryOption"),
                        instrument_classes: classes.into_iter().collect(),
                        data_kinds: vec![RuntimeDataKind::Bar],
                    })
                    .collect(),
                label_interval_support: LabelIntervalSupportV1 {
                    fixed_bars: true,
                    fixed_duration: false,
                    variable_interval: false,
                },
                solver_capabilities: vec![
                    "CONVEX_QP".into(),
                    "SECOND_ORDER_CONE".into(),
                    "LINEAR_PROGRAM".into(),
                    "POWER_CONE".into(),
                ],
                max_cpu: observed_grant(self.config.max_cpu, cpu)?,
                max_memory_mib: observed_grant(self.config.max_memory_mib, memory)?,
                max_output_bytes: self.config.max_output_bytes.map(DbCounter::new)
                    .transpose().map_err(|_| Failure::Integrity)?,
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
        // Recheck the exact image selected for this launch, including callers that
        // bypass aggregate capability discovery. Old numeric-only images still work.
        if spec.limits.cpu == Some(0) || spec.limits.memory_mib == Some(0) {
            return Err(Failure::Invalid("native_resource_quota"));
        }
        for (absent, capability) in [
            (spec.limits.cpu.is_none(), "optional-cpu-rate"),
            (spec.limits.memory_mib.is_none(), "optional-memory-limit"),
        ] {
            if absent && image.versions.get(capability).map(String::as_str) != Some("1") {
                return Err(Failure::Invalid(
                    "native_image_optional_resource_quotas_unsupported",
                ));
            }
        }
        let memory = spec
            .limits
            .memory_mib
            .map(|memory| i64::from(memory) * 1024 * 1024);
        let mut tmpfs = HashMap::from([(
            "/tmp".into(),
            format!("rw,noexec,nosuid,nodev,{},mode=1777", tmpfs_quota(memory)),
        )]);
        if memory.is_none() {
            // ShmSize=0 means Docker's default, not unlimited. An explicit private
            // tmpfs overrides /dev/shm without changing IPC or parent cgroups.
            tmpfs.insert(
                "/dev/shm".into(),
                "rw,noexec,nosuid,nodev,size=0,nr_inodes=0,mode=1777".into(),
            );
        }
        let cpu_seconds = spec
            .limits
            .cpu_seconds
            .map(|cpu| i64::try_from(cpu.get()))
            .transpose()
            .map_err(|_| Failure::Integrity)?;
        let mut ulimits = vec![ulimit("core", 0)];
        if let Some(maximum) = cpu_seconds {
            ulimits.push(ulimit("cpu", maximum));
        }
        let host_config = HostConfig {
            memory,
            memory_swap: memory,
            cgroupns_mode: Some(HostConfigCgroupnsModeEnum::PRIVATE),
            nano_cpus: spec.limits.cpu.map(|cpu| i64::from(cpu) * 1_000_000_000),
            ipc_mode: Some("private".into()),
            readonly_rootfs: Some(true),
            privileged: Some(false),
            cap_drop: Some(vec!["ALL".into()]),
            security_opt: Some(vec!["no-new-privileges:true".into()]),
            network_mode: Some("none".into()),
            init: Some(true),
            auto_remove: Some(false),
            mounts: Some(if barrier { vec![] } else { mounts }),
            tmpfs: Some(tmpfs),
            shm_size: memory,
            ulimits: Some(ulimits),
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
                if barrier { "--version" } else { "run-bounded" }.into(),
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

#[cfg(test)]
mod resource_capacity_tests {
    use super::*;

    #[test]
    fn optional_operator_ceiling_uses_observed_capacity_without_clamping_choices() {
        assert_eq!(observed_grant(None, 2048).unwrap(), 2048);
        assert_eq!(observed_grant(Some(1536), 2048).unwrap(), 1536);
        assert!(observed_grant(Some(2049), 2048).is_err());
        assert!(observed_grant(Some(0), 2048).is_err());
        assert!(observed_grant(None, 0).is_err());
        assert!(observed_grant(None, u64::from(u32::MAX) + 1).is_err());
    }
}

#[cfg(test)]
mod optional_quota_tests {
    use super::*;
    use serde_json::json;

    fn image(optional: bool) -> NativeImage {
        let mut versions = BTreeMap::new();
        let labels = if optional {
            HashMap::from([("io.quazonai.optional-resource-quotas".into(), "1".into())])
        } else {
            HashMap::new()
        };
        add_optional_resource_versions(&mut versions, &labels);
        NativeImage {
            id: format!("sha256:{}", "a".repeat(64)),
            versions,
        }
    }
    fn spec(cpu: Option<u32>, memory: Option<u32>) -> JobSpecV1 {
        let run = Id::new();
        serde_json::from_value(json!({
            "schema_version":1,"run_id":run,"attempt_no":1,"owner_epoch":"1",
            "external_job_id":format!("{run}/1"),"job_kind":"DATA_VALIDATE",
            "image_ref":format!("sha256:{}", "a".repeat(64)),"input_set_id":Id::new(),
            "inputs":[],"parameters_artifact_id":Id::new(),
            "limits":{"cpu":cpu,"cpu_seconds":null,"memory_mib":memory,"wall_seconds":null,"output_bytes":null},
            "deadline_at":null,"requested_output_schemas":[]
        })).unwrap()
    }
    #[test]
    fn resource_capabilities_require_exact_image_label_and_every_configured_image() {
        for label in [None, Some("0"), Some("2"), Some("true"), Some("")] {
            let labels = label
                .map(|label| {
                    HashMap::from([("io.quazonai.optional-resource-quotas".into(), label.into())])
                })
                .unwrap_or_default();
            let mut versions = BTreeMap::from([("optional-wall-time".into(), "1".into())]);
            add_optional_resource_versions(&mut versions, &labels);
            assert_eq!(versions.len(), 1);
        }
        let new = image(true).versions;
        let old = image(false).versions;
        assert_eq!(
            optional_resource_intersection(&[new.clone()], None, None),
            new
        );
        for images in [
            vec![old.clone(), new.clone()],
            vec![new.clone(), old],
            vec![],
        ] {
            assert!(optional_resource_intersection(&images, None, None).is_empty());
        }
        let cpu_capped = optional_resource_intersection(&[new.clone()], Some(1), None);
        assert_eq!(
            cpu_capped,
            BTreeMap::from([("optional-memory-limit".into(), "1".into())])
        );
        let memory_capped = optional_resource_intersection(&[new.clone()], None, Some(512));
        assert_eq!(
            memory_capped,
            BTreeMap::from([("optional-cpu-rate".into(), "1".into())])
        );
        assert!(optional_resource_intersection(&[new], Some(1), Some(512)).is_empty());
    }
    #[test]
    fn selected_image_is_rechecked_and_historical_numeric_quotas_are_unchanged() {
        let old = image(false);
        let job = spec(Some(1), Some(64));
        let host = NativeEngine::launch(Id::new(), &job, &old, vec![], false)
            .unwrap()
            .host_config
            .unwrap();
        assert_eq!(host.nano_cpus, Some(1_000_000_000));
        assert_eq!(host.memory, Some(64 * 1024 * 1024));
        assert_eq!(host.memory_swap, host.memory);
        assert_eq!(host.shm_size, host.memory);
        let finite_tmpfs = host.tmpfs.as_ref().unwrap();
        assert_eq!(finite_tmpfs["/tmp"], "rw,noexec,nosuid,nodev,size=67108864,mode=1777");
        assert!(!finite_tmpfs.contains_key("/dev/shm"));
        for (cpu, memory) in [(None, Some(64)), (Some(1), None), (None, None)] {
            assert!(matches!(
                NativeEngine::launch(Id::new(), &spec(cpu, memory), &old, vec![], false),
                Err(Failure::Invalid(
                    "native_image_optional_resource_quotas_unsupported"
                ))
            ));
        }
    }
    #[test]
    fn absent_quotas_omit_native_limits_and_use_private_unlimited_tmpfs() {
        let host = NativeEngine::launch(Id::new(), &spec(None, None), &image(true), vec![], false)
            .unwrap()
            .host_config
            .unwrap();
        assert_eq!(host.nano_cpus, None);
        assert_eq!(host.memory, None);
        assert_eq!(host.memory_swap, None);
        assert_eq!(host.shm_size, None);
        assert_eq!(host.ipc_mode.as_deref(), Some("private"));
        assert_eq!(
            host.cgroupns_mode,
            Some(HostConfigCgroupnsModeEnum::PRIVATE)
        );
        assert_eq!(host.readonly_rootfs, Some(true));
        assert_eq!(host.network_mode.as_deref(), Some("none"));
        assert_eq!(host.privileged, Some(false));
        assert_eq!(host.cap_drop, Some(vec!["ALL".into()]));
        assert_eq!(host.security_opt, Some(vec!["no-new-privileges:true".into()]));
        for (cpu, memory) in [(Some(1), None), (None, Some(64))] {
            let partial = NativeEngine::launch(Id::new(), &spec(cpu, memory), &image(true), vec![], false).unwrap().host_config.unwrap();
            assert_eq!(partial.nano_cpus, cpu.map(|cpu| i64::from(cpu) * 1_000_000_000));
            assert_eq!(partial.memory, memory.map(|memory| i64::from(memory) * 1024 * 1024));
        }
        let tmpfs = host.tmpfs.as_ref().unwrap();
        for path in ["/tmp", "/dev/shm"] {
            assert_eq!(
                tmpfs[path],
                "rw,noexec,nosuid,nodev,size=0,nr_inodes=0,mode=1777"
            );
        }
        let native = serde_json::to_value(&host).unwrap();
        for name in ["NanoCpus", "Memory", "MemorySwap", "ShmSize"] {
            assert!(
                native.get(name).is_none_or(serde_json::Value::is_null),
                "{name}"
            );
        }
        for (cpu, memory) in [(Some(0), None), (None, Some(0))] {
            assert!(
                NativeEngine::launch(Id::new(), &spec(cpu, memory), &image(true), vec![], false)
                    .is_err()
            );
        }
    }
}
