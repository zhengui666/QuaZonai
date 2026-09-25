//! Docker supplies the process boundary; the existing native JSONL wire owns RPC.
use super::{Launch, MissionProcess, NativeFailure, Result};
use bollard::{
    errors::Error,
    models::{
        ContainerCreateBody, HostConfig, HostConfigLogConfig, Mount, MountType, ResourcesUlimits,
    },
    query_parameters::{
        AttachContainerOptionsBuilder, CreateContainerOptionsBuilder, KillContainerOptionsBuilder,
        ListContainersOptionsBuilder, RemoveContainerOptionsBuilder,
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
        let quota = u128::from(limits.cpu_seconds.get()) * 1_000_000
            / u128::from(limits.wall_seconds.max(1));
        if limits.wall_seconds == 0
            || limits.memory_mib == 0
            || limits.output_bytes.get() == 0
            || !(1_000..=10_000_000_000).contains(&quota)
        {
            return Err(NativeFailure::Configuration);
        }
        host.cpu_period = Some(1_000_000);
        host.cpu_quota = Some(i64::try_from(quota).map_err(|_| NativeFailure::Configuration)?);
        host.memory = Some(i64::from(limits.memory_mib) * 1024 * 1024);
        host.memory_swap = host.memory;
        host.ulimits = Some(vec![
            ResourcesUlimits {
                name: Some("core".into()),
                soft: Some(0),
                hard: Some(0),
            },
            ResourcesUlimits {
                name: Some("fsize".into()),
                soft: Some(64 * 1024 * 1024),
                hard: Some(64 * 1024 * 1024),
            },
        ]);
        labels.insert(RUN_LABEL.into(), process.run_id.to_string());
        process.deadline.saturating_duration_since(Instant::now())
    } else {
        Duration::from_secs(960)
    };
    if remaining.as_secs() == 0 {
        return Err(NativeFailure::Configuration);
    }
    let deadline = SystemTime::now()
        .checked_add(remaining)
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .ok_or(NativeFailure::Configuration)?
        .as_secs();
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
        working_dir: Some(launch.working_directory.to_str().ok_or(NativeFailure::Configuration)?.into()),
        env: Some(env),
        entrypoint: Some(vec!["/bin/sh".into()]),
        // Absolute time includes daemon queue/start latency and survives QZ death.
        // No user strings are interpolated into the shell program.
        cmd: Some(vec!["-c".into(),
            "remaining=$(($1 - $(date +%s))); [ \"$remaining\" -gt 0 ] || exit 124; exec /usr/bin/timeout --signal=KILL \"${remaining}s\" /opt/codex/bin/codex -c 'cli_auth_credentials_store=\"file\"' app-server".into(),
            "codex-deadline".into(), deadline.to_string()]),
        attach_stdin: Some(true), attach_stdout: Some(true), attach_stderr: Some(false),
        open_stdin: Some(true), stdin_once: Some(true), tty: Some(false),
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
        startup_deadline.min(limits.deadline)
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
    limits: Option<MissionProcess>,
    deadline: Instant,
) -> Result<(Container, Reader, Writer)> {
    let backend = launch
        .container
        .as_ref()
        .ok_or(NativeFailure::Configuration)?;
    backend.validate()?;
    let _lock = backend.lock(deadline).await?;
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
        |value| format!("quazonai-codex-mission-{}", value.run_id),
    );
    if let Some(limits) = &limits {
        match docker.inspect_container(&name, None).await {
            Ok(existing) => {
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
    };
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
    owned
        .docker
        .start_container(id, None)
        .await
        .map_err(unavailable)?;
    let output = attached.output.map(|result| {
        result
            .map(|value| value.into_bytes())
            .map_err(std::io::Error::other)
    });
    Ok((owned, Box::pin(StreamReader::new(output)), attached.input))
}

impl Container {
    pub(super) async fn close(&mut self) -> Result<()> {
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
        if self.removed {
            return;
        }
        let mut cleanup = Self {
            docker: self.docker.clone(),
            name: self.name.clone(),
            owner: self.owner.clone(),
            id: self.id.clone(),
            removed: false,
        };
        self.removed = true;
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if cleanup.close().await.is_err() {
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
