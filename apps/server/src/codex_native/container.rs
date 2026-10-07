//! Docker supplies the process boundary; the existing native JSONL wire owns RPC.
use super::{resource_account::ResourceAccount, Launch, MissionProcess, NativeFailure, Result};
use bollard::{
    errors::Error,
    models::{
        ContainerCreateBody, HostConfig, HostConfigLogConfig, Mount, MountType, ResourcesUlimits,
    },
    query_parameters::{
        AttachContainerOptionsBuilder, CreateContainerOptionsBuilder, KillContainerOptionsBuilder,
        ListContainersOptionsBuilder, RemoveContainerOptionsBuilder, StatsOptionsBuilder,
    },
    Docker, API_DEFAULT_VERSION,
};
use futures_util::StreamExt;
use std::{
    collections::HashMap,
    fs::File,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    pin::Pin,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::io::StreamReader;

pub const BINARY: &str = "/opt/codex/bin/codex";
pub const PATH: &str =
    "/opt/codex/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
const IMAGE_LABEL: &str = "io.quazonai.codex.image";
const OWNER_LABEL: &str = "io.quazonai.codex.owner";
const RUN_LABEL: &str = "io.quazonai.codex.run";
const ATTEMPT_LABEL: &str = "io.quazonai.codex.attempt";
const EPOCH_LABEL: &str = "io.quazonai.codex.owner-epoch";
const RESOURCE_LABEL: &str = "io.quazonai.codex.resource";

/// Trusted installation settings, never supplied by a public request.
#[derive(Clone)]
pub struct ContainerBackend {
    pub image: String,
    pub socket: PathBuf,
    pub lock_file: PathBuf,
}

pub(super) type Reader = Pin<Box<dyn AsyncRead + Send>>;
pub(super) type Writer = Pin<Box<dyn AsyncWrite + Send>>;

pub(super) struct Container {
    docker: Docker,
    name: String,
    owner: String,
    id: Option<String>,
    removed: bool,
    account: Option<ResourceAccount>,
    final_cpu: Option<Option<u64>>,
    cleanup_on_drop: bool,
}

fn unavailable(_: Error) -> NativeFailure {
    NativeFailure::Unavailable
}
fn missing(error: &Error) -> bool {
    matches!(
        error,
        Error::DockerResponseServerError {
            status_code: 404,
            ..
        }
    )
}

impl ContainerBackend {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.image.is_empty()
            || self.image.len() > 255
            || self
                .image
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte == 0)
            || !self.socket.is_absolute()
            || !self.lock_file.is_absolute()
            || !self
                .socket
                .metadata()
                .is_ok_and(|meta| meta.file_type().is_socket())
            || !self.lock_file.is_file()
        {
            return Err(NativeFailure::Configuration);
        }
        Ok(())
    }

    async fn lock(&self, deadline: Instant) -> Result<File> {
        let file = File::open(&self.lock_file).map_err(|_| NativeFailure::Configuration)?;
        loop {
            if Instant::now() >= deadline {
                return Err(NativeFailure::Unavailable);
            }
            // Only startup holds this lock. It also lets recovery distinguish
            // CREATED orphans from another launch still preparing its container.
            match file.try_lock() {
                Ok(()) => return Ok(file),
                Err(std::fs::TryLockError::WouldBlock) => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(_) => return Err(NativeFailure::Unavailable),
            }
        }
    }
}

fn bind(path: &Path, read_only: bool) -> Result<Mount> {
    if !path.is_absolute() || !path.exists() {
        return Err(NativeFailure::Configuration);
    }
    let value = path
        .to_str()
        .ok_or(NativeFailure::Configuration)?
        .to_owned();
    Ok(Mount {
        typ: Some(MountType::BIND),
        source: Some(value.clone()),
        target: Some(value),
        read_only: Some(read_only),
        ..Default::default()
    })
}

fn configuration(
    launch: &Launch,
    backend: &ContainerBackend,
    limits: Option<&MissionProcess>,
    image: String,
    owner: &str,
) -> Result<ContainerCreateBody> {
    for path in [&launch.codex_home, &launch.working_directory] {
        if !path.is_absolute() || !path.is_dir() {
            return Err(NativeFailure::Configuration);
        }
    }
    let metadata = launch
        .codex_home
        .metadata()
        .map_err(|_| NativeFailure::Configuration)?;
    if metadata.uid() == 0 {
        return Err(NativeFailure::Configuration);
    }
    let uid = metadata.uid();
    let gid = metadata.gid();
    let mut mounts = vec![bind(&launch.codex_home, false)?];
    if launch.codex_home != launch.working_directory {
        mounts.push(bind(&launch.working_directory, limits.is_none())?);
    }
    if limits.is_some() {
        mounts.push(bind(
            &limits
                .and_then(|limits| limits.server_binary.clone())
                .unwrap_or(std::env::current_exe().map_err(|_| NativeFailure::Configuration)?),
            true,
        )?);
    }
    let mut labels = HashMap::from([
        (IMAGE_LABEL.into(), backend.image.clone()),
        (OWNER_LABEL.into(), owner.into()),
    ]);
    let mut host = HostConfig {
        network_mode: Some("host".into()),
        cap_drop: Some(vec!["ALL".into()]),
        // Codex's native Linux sandbox creates unprivileged user namespaces and
        // installs its own syscall filter. Docker's defaults block that nesting.
        security_opt: Some(vec![
            "no-new-privileges:true".into(),
            "seccomp=unconfined".into(),
            "apparmor=unconfined".into(),
        ]),
        init: Some(true),
        pids_limit: Some(128),
        readonly_rootfs: Some(true),
        tmpfs: Some(HashMap::from([
            ("/tmp".into(), "rw,nosuid,nodev,size=256m,mode=1777".into()),
            (
                "/home/codex".into(),
                format!("rw,nosuid,nodev,size=64m,uid={uid},gid={gid},mode=700"),
            ),
        ])),
        log_config: Some(HostConfigLogConfig {
            typ: Some("none".into()),
            ..Default::default()
        }),
        mounts: Some(mounts),
        ..Default::default()
    };
    // Account owners already have a 930-second deadline. The additional native
    // watchdog also bounds a disconnected or killed QZ owner.
    let remaining = if let Some(process) = limits {
        let limits = &process.limits;
        let quota = limits
            .cpu_seconds
            .zip(limits.wall_seconds)
            .map_or(1_000_000, |(cpu, wall)| {
                u128::from(cpu.get()) * 1_000_000 / u128::from(wall.max(1))
            });
        if limits.wall_seconds == Some(0)
            || limits.memory_mib == 0
            || limits.output_bytes.is_some_and(|bytes| bytes.get() == 0)
            || !(1_000..=10_000_000_000).contains(&quota)
        {
            return Err(NativeFailure::Configuration);
        }
        host.cpu_period = Some(1_000_000);
        host.cpu_quota = Some(i64::try_from(quota).map_err(|_| NativeFailure::Configuration)?);
        host.memory = Some(i64::from(limits.memory_mib) * 1024 * 1024);
        host.memory_swap = host.memory;
        let mut ulimits = vec![ResourcesUlimits {
            name: Some("core".into()),
            soft: Some(0),
            hard: Some(0),
        }];
        if let Some(cpu) = limits.cpu_seconds {
            let value = i64::try_from(cpu.get()).map_err(|_| NativeFailure::Configuration)?;
            ulimits.push(ResourcesUlimits {
                name: Some("cpu".into()),
                soft: Some(value),
                hard: Some(value),
            });
        }
        if limits.output_bytes.is_some() {
            ulimits.push(ResourcesUlimits {
                name: Some("fsize".into()),
                soft: Some(64 * 1024 * 1024),
                hard: Some(64 * 1024 * 1024),
            });
        }
        host.ulimits = Some(ulimits);
        labels.insert(RUN_LABEL.into(), process.run_id.to_string());
        if let Some(account) = &process.account {
            labels.insert(
                ATTEMPT_LABEL.into(),
                account.resource.attempt_id.to_string(),
            );
            labels.insert(
                EPOCH_LABEL.into(),
                account.resource.owner_epoch.get().to_string(),
            );
            labels.insert(RESOURCE_LABEL.into(), account.resource.id.to_string());
        }
        process
            .deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    } else {
        Some(Duration::from_secs(960))
    };
    if remaining.is_some_and(|remaining| remaining.as_secs() == 0) {
        return Err(NativeFailure::Configuration);
    }
    let deadline = remaining
        .map(|remaining| {
            SystemTime::now()
                .checked_add(remaining)
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|time| time.as_secs())
                .ok_or(NativeFailure::Configuration)
        })
        .transpose()?;
    let mut environment = launch.native_environment.clone();
    // Host session paths have no meaning in the isolated filesystem.
    for name in [
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
    ] {
        environment.remove(std::ffi::OsStr::new(name));
    }
    for (key, value) in [
        ("HOME", "/home/codex"),
        (
            "CODEX_HOME",
            launch
                .codex_home
                .to_str()
                .ok_or(NativeFailure::Configuration)?,
        ),
        ("PATH", PATH),
        ("RUST_LOG", "off"),
    ] {
        environment.insert(key.into(), value.into());
    }
    let env = environment
        .into_iter()
        .map(|(key, value)| {
            let key = key.to_str().ok_or(NativeFailure::Configuration)?;
            let value = value.to_str().ok_or(NativeFailure::Configuration)?;
            if key.is_empty() || key.contains(['=', '\0']) || value.contains('\0') {
                return Err(NativeFailure::Configuration);
            }
            Ok(format!("{key}={value}"))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ContainerCreateBody {
        image: Some(image),
        user: Some(format!("{uid}:{gid}")),
        labels: Some(labels),
        host_config: Some(host),
        working_dir: Some(
            launch
                .working_directory
                .to_str()
                .ok_or(NativeFailure::Configuration)?
                .into(),
        ),
        env: Some(env),
        entrypoint: Some(vec!["/bin/sh".into()]),
        // Absolute time includes daemon queue/start latency and survives QZ death.
        // No user strings are interpolated into the shell program.
        cmd: Some(if let Some(deadline) = deadline {
            vec!["-c".into(),
            "remaining=$(($1 - $(date +%s))); [ \"$remaining\" -gt 0 ] || exit 124; exec /usr/bin/timeout --signal=KILL \"${remaining}s\" /opt/codex/bin/codex -c 'cli_auth_credentials_store=\"file\"' app-server".into(),
            "codex-deadline".into(), deadline.to_string()]
        } else {
            vec![
                "-c".into(),
                "exec /opt/codex/bin/codex -c 'cli_auth_credentials_store=\"file\"' app-server"
                    .into(),
            ]
        }),
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(false),
        open_stdin: Some(true),
        stdin_once: Some(limits.is_none_or(|limits| limits.account.is_none())),
        tty: Some(false),
        ..Default::default()
    })
}

pub(super) async fn start(
    launch: Launch,
    limits: Option<MissionProcess>,
) -> Result<(Container, Reader, Writer)> {
    // Keep create/start alive when a request deadline drops its caller. The task
    // retains cleanup ownership even before Docker returns the actual ID.
    let startup_deadline = Instant::now() + Duration::from_secs(20);
    let deadline = limits.as_ref().map_or(startup_deadline, |limits| {
        limits
            .deadline
            .map_or(startup_deadline, |deadline| startup_deadline.min(deadline))
    });
    tokio::spawn(async move {
        // Bollard's request timeout covers headers, not every body/upgrade read.
        tokio::time::timeout_at(deadline.into(), start_owned(launch, limits, deadline))
            .await
            .map_err(|_| NativeFailure::Unavailable)?
    })
    .await
    .map_err(|_| NativeFailure::Unavailable)?
}

async fn start_owned(
    launch: Launch,
    mut limits: Option<MissionProcess>,
    deadline: Instant,
) -> Result<(Container, Reader, Writer)> {
    let backend = launch
        .container
        .as_ref()
        .ok_or(NativeFailure::Configuration)?;
    backend.validate()?;
    let _lock = backend.lock(deadline).await?;
    if let Some(limits) = &mut limits {
        limits.prepare(Some(backend)).await?;
    }
    let docker = Docker::connect_with_unix(
        backend
            .socket
            .to_str()
            .ok_or(NativeFailure::Configuration)?,
        10,
        API_DEFAULT_VERSION,
    )
    .map_err(unavailable)?
    .negotiate_version()
    .await
    .map_err(unavailable)?;
    if limits.as_ref().is_some_and(|p| p.account.is_some())
        && docker.version().await.map_err(unavailable)?.os.as_deref() != Some("linux")
    {
        return Err(NativeFailure::Configuration);
    }
    let image = docker
        .inspect_image(&backend.image)
        .await
        .map_err(unavailable)?
        .id
        .ok_or(NativeFailure::Configuration)?;
    // A dead QZ owner or a delayed create response can leave an unstarted
    // container. No current launcher can still own CREATED under this lock.
    let filters = HashMap::from([
        (
            "label".to_owned(),
            vec![format!("{IMAGE_LABEL}={}", backend.image)],
        ),
        ("status".to_owned(), vec!["created".into()]),
    ]);
    for candidate in docker
        .list_containers(Some(
            ListContainersOptionsBuilder::default()
                .all(true)
                .filters(&filters)
                .build(),
        ))
        .await
        .map_err(unavailable)?
    {
        let id = candidate.id.ok_or(NativeFailure::Unavailable)?;
        match docker.inspect_container(&id, None).await {
            Ok(value)
                if value.state.as_ref().is_some_and(|state| {
                    state.status == Some(bollard::models::ContainerStateStatusEnum::CREATED)
                        && state.running == Some(false)
                }) && value
                    .config
                    .as_ref()
                    .and_then(|config| config.labels.as_ref())
                    .is_some_and(|labels| {
                        labels.get(IMAGE_LABEL) == Some(&backend.image)
                            && labels.contains_key(OWNER_LABEL)
                            && !labels.contains_key(RESOURCE_LABEL)
                    }) =>
            {
                match docker.remove_container(&id, None).await {
                    Ok(()) => {}
                    Err(error) if missing(&error) => {}
                    Err(error) => return Err(unavailable(error)),
                }
            }
            Ok(_) => {}
            Err(error) if missing(&error) => {}
            Err(error) => return Err(unavailable(error)),
        }
    }
    let owner = contracts::Id::new().to_string();
    let name = limits.as_ref().map_or_else(
        || format!("quazonai-codex-session-{owner}"),
        |value| {
            value.account.as_ref().map_or_else(
                || format!("quazonai-codex-mission-{}", value.run_id),
                |a| a.resource.name(),
            )
        },
    );
    if let Some(limits) = &limits {
        match docker.inspect_container(&name, None).await {
            Ok(existing) => {
                if limits.account.is_some() {
                    return Err(NativeFailure::Correlation);
                }
                let labels = existing
                    .config
                    .and_then(|value| value.labels)
                    .unwrap_or_default();
                let state = existing.state.ok_or(NativeFailure::Unavailable)?;
                if labels.get(IMAGE_LABEL) != Some(&backend.image)
                    || labels.get(RUN_LABEL) != Some(&limits.run_id.to_string())
                    || state.running != Some(false)
                    || state.paused == Some(true)
                    || state.restarting == Some(true)
                    || !matches!(
                        state.status,
                        Some(
                            bollard::models::ContainerStateStatusEnum::CREATED
                                | bollard::models::ContainerStateStatusEnum::EXITED
                                | bollard::models::ContainerStateStatusEnum::DEAD
                        )
                    )
                {
                    return Err(NativeFailure::Unavailable);
                }
                let id = existing.id.ok_or(NativeFailure::Unavailable)?;
                docker
                    .remove_container(&id, None)
                    .await
                    .map_err(unavailable)?;
            }
            Err(error) if missing(&error) => {}
            Err(error) => return Err(unavailable(error)),
        }
    }
    let config = configuration(&launch, backend, limits.as_ref(), image, &owner)?;
    let mut owned = Container {
        docker,
        name,
        owner,
        id: None,
        removed: false,
        account: limits.as_ref().and_then(|p| p.account.clone()),
        final_cpu: None,
        cleanup_on_drop: true,
    };
    if let Some(account) = &owned.account {
        account.begin_launch().await?;
    }
    let created = owned
        .docker
        .create_container(
            Some(
                CreateContainerOptionsBuilder::default()
                    .name(&owned.name)
                    .build(),
            ),
            config,
        )
        .await
        .map_err(unavailable)?;
    owned.id = Some(created.id);
    if let Some(account) = &owned.account {
        account
            .bind(owned.id.as_deref().ok_or(NativeFailure::Unavailable)?)
            .await?;
    }
    let id = owned.id.as_deref().ok_or(NativeFailure::Unavailable)?;
    let attached = owned
        .docker
        .attach_container(
            id,
            Some(
                AttachContainerOptionsBuilder::default()
                    .stdin(true)
                    .stdout(true)
                    .stderr(false)
                    .stream(true)
                    .logs(false)
                    .build(),
            ),
        )
        .await
        .map_err(unavailable)?;
    // Do not start a Mission whose authorization deadline expired during create.
    if Instant::now() >= deadline {
        return Err(NativeFailure::Unavailable);
    }
    if let Some(account) = &owned.account {
        account.begin_execution().await?;
    }
    owned
        .docker
        .start_container(id, None)
        .await
        .map_err(unavailable)?;
    if let Some(sender) = limits.as_ref().and_then(|p| p.monitor_sender.as_ref()) {
        sender.send_replace(Some(owned.monitor()));
    }
    owned.check_cpu().await?;
    let output = attached.output.map(|result| {
        result
            .map(|value| value.into_bytes())
            .map_err(std::io::Error::other)
    });
    Ok((owned, Box::pin(StreamReader::new(output)), attached.input))
}

impl Container {
    pub(super) fn monitor(&self) -> super::ResourceMonitor {
        super::ResourceMonitor::docker(Self {
            docker: self.docker.clone(),
            name: self.name.clone(),
            owner: self.owner.clone(),
            id: self.id.clone(),
            removed: self.removed,
            account: self.account.clone(),
            final_cpu: self.final_cpu,
            cleanup_on_drop: false,
        })
    }
    pub(super) async fn check_cpu(&mut self) -> Result<()> {
        let Some(account) = self.account.clone() else {
            return Ok(());
        };
        let _gate = account.gate.lock().await;
        if account.is_closed() || account.grant_nanoseconds.is_none() {
            return Ok(());
        }
        let result = async {
            let nanos = self.cpu_nanoseconds().await?;
            if account.exceeds(nanos)? {
                self.close_accounted().await?;
                return Err(NativeFailure::CpuBudgetExceeded);
            }
            if account.checkpoint(Some(nanos), false, false).await? {
                self.close_accounted().await?;
                return Err(NativeFailure::CpuBudgetExceeded);
            }
            Ok(())
        }
        .await;
        if result.is_err() && !self.removed {
            let _ = self.pause().await;
        }
        result
    }

    async fn cpu_nanoseconds(&self) -> Result<u64> {
        let id = self.id.as_deref().ok_or(NativeFailure::Unavailable)?;
        let stats = tokio::time::timeout(Duration::from_secs(2), async {
            self.docker
                .stats(
                    id,
                    Some(
                        StatsOptionsBuilder::default()
                            .stream(false)
                            .one_shot(true)
                            .build(),
                    ),
                )
                .next()
                .await
        })
        .await
        .map_err(|_| NativeFailure::Unavailable)?
        .ok_or(NativeFailure::Unavailable)?
        .map_err(unavailable)?;
        cpu_nanoseconds(stats, id)
    }

    async fn pause(&self) -> Result<()> {
        let id = self.id.as_deref().ok_or(NativeFailure::Unavailable)?;
        tokio::time::timeout(Duration::from_secs(3), async {
            let state = self
                .docker
                .inspect_container(id, None)
                .await
                .map_err(unavailable)?
                .state
                .ok_or(NativeFailure::Unavailable)?;
            if state.paused != Some(true) {
                self.docker.pause_container(id).await.map_err(unavailable)?;
            }
            let state = self
                .docker
                .inspect_container(id, None)
                .await
                .map_err(unavailable)?
                .state
                .ok_or(NativeFailure::Unavailable)?;
            if state.running != Some(true) || state.paused != Some(true) {
                return Err(NativeFailure::Unavailable);
            }
            Ok(())
        })
        .await
        .map_err(|_| NativeFailure::Unavailable)?
    }

    async fn close_accounted(&mut self) -> Result<()> {
        if self.removed {
            return Ok(());
        }
        let account = self.account.clone().ok_or(NativeFailure::Configuration)?;
        let nanos = if let Some(nanos) = self.final_cpu {
            nanos
        } else {
            let frozen = self.pause().await;
            let observed = match frozen {
                Ok(()) => self.cpu_nanoseconds().await,
                Err(error) => Err(error),
            };
            let nanos = observed.ok();
            if let Some(nanos) = nanos {
                account.checkpoint(Some(nanos), true, false).await?;
            }
            self.final_cpu = Some(nanos);
            nanos
        };
        let id = self.id.as_deref().ok_or(NativeFailure::Unavailable)?;
        // Force-remove targets the immutable daemon ID and kills the frozen
        // process tree. Never unpause the workload between final sample and kill.
        match self
            .docker
            .remove_container(
                id,
                Some(RemoveContainerOptionsBuilder::default().force(true).build()),
            )
            .await
        {
            Ok(()) => {}
            Err(error) if missing(&error) => {}
            Err(error) => return Err(unavailable(error)),
        }
        match self.docker.inspect_container(id, None).await {
            Err(error) if missing(&error) => {}
            _ => return Err(NativeFailure::Unavailable),
        }
        account.checkpoint(nanos, nanos.is_some(), true).await?;
        account.mark_closed();
        self.removed = true;
        Ok(())
    }

    pub(super) async fn close(&mut self) -> Result<()> {
        if let Some(account) = self.account.clone() {
            let _gate = account.gate.lock().await;
            if account.is_closed() {
                self.removed = true;
                return Ok(());
            }
            return tokio::time::timeout(Duration::from_secs(20), self.close_accounted())
                .await
                .map_err(|_| NativeFailure::Unavailable)?;
        }
        tokio::time::timeout(Duration::from_secs(20), self.close_inner())
            .await
            .map_err(|_| NativeFailure::Unavailable)?
    }

    async fn close_inner(&mut self) -> Result<()> {
        if self.removed {
            return Ok(());
        }
        let discovery_deadline = Instant::now() + Duration::from_secs(3);
        while self.id.is_none() {
            match self.docker.inspect_container(&self.name, None).await {
                Ok(value) => {
                    let labels = value
                        .config
                        .and_then(|value| value.labels)
                        .unwrap_or_default();
                    if labels.get(OWNER_LABEL) != Some(&self.owner) {
                        self.removed = true;
                        return Ok(());
                    }
                    self.id = value.id;
                }
                Err(error) if missing(&error) => {
                    // Without a returned ID, an absent name does not exclude a
                    // daemon create still in flight. Later startup also collects it.
                    if Instant::now() >= discovery_deadline {
                        return Err(NativeFailure::Unavailable);
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => return Err(unavailable(error)),
            }
        }
        let id = self.id.as_deref().ok_or(NativeFailure::Unavailable)?;
        // A successful signal is not terminal proof. Inspect the same actual ID.
        let _ = self
            .docker
            .kill_container(
                id,
                Some(
                    KillContainerOptionsBuilder::default()
                        .signal("SIGKILL")
                        .build(),
                ),
            )
            .await;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match self.docker.inspect_container(id, None).await {
                Ok(value) => {
                    let state = value.state.ok_or(NativeFailure::Unavailable)?;
                    if state.running == Some(false)
                        && state.paused != Some(true)
                        && state.restarting != Some(true)
                    {
                        break;
                    }
                }
                Err(error) if missing(&error) => {
                    self.removed = true;
                    return Ok(());
                }
                Err(error) => return Err(unavailable(error)),
            }
            if Instant::now() >= deadline {
                return Err(NativeFailure::Unavailable);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        match self
            .docker
            .remove_container(id, Some(RemoveContainerOptionsBuilder::default().build()))
            .await
        {
            Ok(()) => {}
            Err(error) if missing(&error) => {}
            Err(error) => return Err(unavailable(error)),
        }
        self.removed = true;
        Ok(())
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        if self.removed
            || !self.cleanup_on_drop
            || self
                .account
                .as_ref()
                .is_some_and(ResourceAccount::is_closed)
        {
            return;
        }
        let mut cleanup = Self {
            docker: self.docker.clone(),
            name: self.name.clone(),
            owner: self.owner.clone(),
            id: self.id.clone(),
            removed: false,
            account: self.account.clone(),
            final_cpu: self.final_cpu,
            cleanup_on_drop: false,
        };
        self.removed = true;
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let result = if cleanup.account.is_some() {
                    cleanup.pause().await
                } else {
                    cleanup.close().await
                };
                if result.is_err() {
                    tracing::warn!("Codex container cleanup remains unconfirmed");
                }
                // Never recursively reschedule an unavailable Docker daemon.
                cleanup.removed = true;
                drop(cleanup);
            });
        } else {
            cleanup.removed = true;
            tracing::warn!("Codex container cleanup unavailable outside runtime");
        }
    }
}

fn labels_match(
    labels: &HashMap<String, String>,
    r: &store::lifecycle::mission::resources::MissionResource,
    image: &str,
) -> bool {
    labels.get(IMAGE_LABEL).is_some_and(|v| v == image)
        && labels.get(RUN_LABEL) == Some(&r.run_id.to_string())
        && labels.get(ATTEMPT_LABEL) == Some(&r.attempt_id.to_string())
        && labels.get(EPOCH_LABEL) == Some(&r.owner_epoch.get().to_string())
        && labels.get(RESOURCE_LABEL) == Some(&r.id.to_string())
        && labels.contains_key(OWNER_LABEL)
}
fn cpu_nanoseconds(stats: bollard::models::ContainerStatsResponse, id: &str) -> Result<u64> {
    // The daemon is verified as Linux at creation/recovery. Older Docker API
    // versions omit os_type from individual samples; a contrary value is invalid.
    if stats.id.as_deref() != Some(id) || stats.os_type.as_deref().is_some_and(|os| os != "linux") {
        return Err(NativeFailure::Correlation);
    }
    stats
        .cpu_stats
        .and_then(|cpu| cpu.cpu_usage)
        .and_then(|cpu| cpu.total_usage)
        .ok_or(NativeFailure::Unavailable)
}

pub(super) async fn recover(backend: &ContainerBackend, account: ResourceAccount) -> Result<()> {
    let docker = Docker::connect_with_unix(
        backend
            .socket
            .to_str()
            .ok_or(NativeFailure::Configuration)?,
        10,
        API_DEFAULT_VERSION,
    )
    .map_err(unavailable)?
    .negotiate_version()
    .await
    .map_err(unavailable)?;
    if docker.version().await.map_err(unavailable)?.os.as_deref() != Some("linux") {
        return Err(NativeFailure::Configuration);
    }
    let name = account.resource.name();
    let target = account.resource.physical_id.as_deref().unwrap_or(&name);
    let existing = match docker.inspect_container(target, None).await {
        Ok(value) => value,
        Err(error) if missing(&error) && !account.resource.execution_requested => {
            // The old owner never committed a start permit. A delayed create
            // can only leave an unstarted container, never a running workload.
            account.abort_before_spawn().await?;
            return Ok(());
        }
        Err(error) if missing(&error) && account.resource.final_accounted => {
            account
                .checkpoint(account.resource.cpu_nanoseconds, true, true)
                .await?;
            return Ok(());
        }
        Err(error) if missing(&error) && account.resource.physical_id.is_some() => {
            account.checkpoint(None, false, true).await?;
            return Ok(());
        }
        Err(error) => return Err(unavailable(error)),
    };
    let labels = existing
        .config
        .and_then(|c| c.labels)
        .ok_or(NativeFailure::Correlation)?;
    let expected_name = format!("/{name}");
    if !labels_match(&labels, &account.resource, &backend.image)
        || existing.name.as_deref() != Some(expected_name.as_str())
    {
        return Err(NativeFailure::Correlation);
    }
    let id = existing.id.ok_or(NativeFailure::Unavailable)?;
    if account
        .resource
        .physical_id
        .as_ref()
        .is_some_and(|expected| expected != &id)
    {
        return Err(NativeFailure::Correlation);
    }
    if !account.resource.execution_requested {
        let state = existing.state.ok_or(NativeFailure::Unavailable)?;
        if state.status != Some(bollard::models::ContainerStateStatusEnum::CREATED)
            || state.running != Some(false)
        {
            return Err(NativeFailure::Correlation);
        }
        account.checkpoint(Some(0), true, false).await?;
        docker
            .remove_container(&id, None)
            .await
            .map_err(unavailable)?;
        account.abort_before_spawn().await?;
        return Ok(());
    }
    let final_cpu = account
        .resource
        .final_accounted
        .then_some(account.resource.cpu_nanoseconds);
    let mut owned = Container {
        docker,
        name,
        owner: labels
            .get(OWNER_LABEL)
            .ok_or(NativeFailure::Correlation)?
            .clone(),
        id: Some(id),
        removed: false,
        account: Some(account),
        final_cpu,
        cleanup_on_drop: true,
    };
    let frozen = owned.pause().await;
    if let (Some(expected), Ok(())) = (final_cpu.flatten(), frozen) {
        if let Ok(actual) = owned.cpu_nanoseconds().await {
            if actual != expected {
                return Err(NativeFailure::Correlation);
            }
        }
    }
    owned.close().await
}

#[cfg(test)]
mod cpu_tests {
    use super::*;
    #[test]
    fn stale_container_matching_requires_every_identity_label() {
        let r = store::lifecycle::mission::resources::MissionResource {
            id: contracts::Id::new(),
            run_id: contracts::Id::new(),
            attempt_id: contracts::Id::new(),
            owner_epoch: contracts::Revision::INITIAL,
            backend: "DOCKER".into(),
            purpose: store::lifecycle::mission::resources::ResourcePurpose::Research,
            effective_limits: serde_json::from_value(serde_json::json!({"schema_version":1,"experiments":0,"cpu_seconds":null,"wall_seconds":null,"memory_mib":64,"output_bytes":null})).unwrap(),
            deadline_at: None,
            created_at: chrono::Utc::now(),
            physical_id: None,
            launch_requested: true,
            execution_requested: true,
            cpu_nanoseconds: Some(0),
            accounting_unknown: false,
            final_accounted: false,
            closed: false,
        };
        let labels = HashMap::from([
            (IMAGE_LABEL.into(), "image".into()),
            (OWNER_LABEL.into(), "owner".into()),
            (RUN_LABEL.into(), r.run_id.to_string()),
            (ATTEMPT_LABEL.into(), r.attempt_id.to_string()),
            (EPOCH_LABEL.into(), r.owner_epoch.get().to_string()),
            (RESOURCE_LABEL.into(), r.id.to_string()),
        ]);
        assert!(labels_match(&labels, &r, "image"));
        for key in [
            IMAGE_LABEL,
            OWNER_LABEL,
            RUN_LABEL,
            ATTEMPT_LABEL,
            EPOCH_LABEL,
            RESOURCE_LABEL,
        ] {
            let mut incomplete = labels.clone();
            incomplete.remove(key);
            assert!(!labels_match(&incomplete, &r, "image"));
        }
        for key in [RUN_LABEL, ATTEMPT_LABEL, EPOCH_LABEL, RESOURCE_LABEL] {
            let mut wrong = labels.clone();
            wrong.insert(key.into(), "different".into());
            assert!(!labels_match(&wrong, &r, "image"));
        }
    }
    #[test]
    fn docker_cpu_requires_exact_identity_linux_and_total_not_percent() {
        let data = serde_json::json!({"id":"exact","os_type":"linux","cpu_stats":{"cpu_usage":{"total_usage":2400000000_u64}}});
        let stats = serde_json::from_value(data.clone()).unwrap();
        assert_eq!(cpu_nanoseconds(stats, "exact").unwrap(), 2_400_000_000);
        assert!(cpu_nanoseconds(serde_json::from_value(data.clone()).unwrap(), "other").is_err());
        let legacy = serde_json::json!({"id":"exact","cpu_stats":{"cpu_usage":{"total_usage":2400000000_u64}}});
        assert_eq!(
            cpu_nanoseconds(serde_json::from_value(legacy).unwrap(), "exact").unwrap(),
            2_400_000_000
        );
        for invalid in [
            serde_json::json!({"id":"exact","os_type":"linux"}),
            serde_json::json!({"id":"exact","os_type":"windows","cpu_stats":{"cpu_usage":{"total_usage":0}}}),
        ] {
            assert!(cpu_nanoseconds(serde_json::from_value(invalid).unwrap(), "exact").is_err());
        }
    }
}
