//! Native Linux cgroup/rlimit configuration, not another process supervisor.
use super::{resource_account::ResourceAccount, NativeFailure, Result};
use contracts::{lifecycle::JobLimitsV1, Id};
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    time::{Duration, Instant},
};
use store::{turns::WorkerFence, Store};
use tokio::process::Command;

// Native SQLite/rollout files are not QZ research outputs. A 1 MiB artifact
// budget must not kill the native schema migration's >1 MiB SQLite WAL.
const MAX_NATIVE_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Holds the original kernel control file, never kills a later unit by its name.
pub(super) struct ProcessGroup {
    kill: Option<File>,
    freeze: File,
    directory: PathBuf,
    device: u64,
    inode: u64,
    account: Option<ResourceAccount>,
    final_cpu: Option<Option<u64>>,
    cleanup_on_drop: bool,
}

impl ProcessGroup {
    fn same_group(&self) -> Result<bool> {
        match self.directory.metadata() {
            Ok(metadata) => Ok(metadata.dev() == self.device && metadata.ino() == self.inode),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(NativeFailure::Unavailable),
        }
    }

    fn kill(&mut self) -> Result<()> {
        if let Some(mut file) = self.kill.take() {
            if file.write_all(b"1").is_err() && !matches!(self.same_group(), Ok(false)) {
                self.kill = Some(file);
                return Err(NativeFailure::Unavailable);
            }
        }
        Ok(())
    }

    fn pause_now(&self) -> Result<()> {
        if !self.same_group()? {
            return Err(NativeFailure::Unavailable);
        }
        (&self.freeze)
            .write_all(b"1")
            .map_err(|_| NativeFailure::Unavailable)
    }

    async fn pause(&self) -> Result<()> {
        self.pause_now()?;
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if !self.same_group()? {
                    return Err(NativeFailure::Unavailable);
                }
                let events = std::fs::read_to_string(self.directory.join("cgroup.events"))
                    .map_err(|_| NativeFailure::Unavailable)?;
                if events.lines().any(|line| line == "frozen 1") {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| NativeFailure::Unavailable)?
    }

    fn cpu_nanoseconds(&self) -> Result<u64> {
        if !self.same_group()? {
            return Err(NativeFailure::Unavailable);
        }
        parse_cpu_stat(
            &std::fs::read_to_string(self.directory.join("cpu.stat"))
                .map_err(|_| NativeFailure::Unavailable)?,
        )
    }

    pub(super) fn monitor(&self) -> Result<super::ResourceMonitor> {
        Ok(super::ResourceMonitor::host(Self {
            kill: self
                .kill
                .as_ref()
                .map(File::try_clone)
                .transpose()
                .map_err(|_| NativeFailure::Unavailable)?,
            freeze: self
                .freeze
                .try_clone()
                .map_err(|_| NativeFailure::Unavailable)?,
            directory: self.directory.clone(),
            device: self.device,
            inode: self.inode,
            account: self.account.clone(),
            final_cpu: self.final_cpu,
            cleanup_on_drop: false,
        }))
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
            let nanos = self.cpu_nanoseconds()?;
            if account.exceeds(nanos)? {
                self.close_inner().await?;
                return Err(NativeFailure::CpuBudgetExceeded);
            }
            if account.checkpoint(Some(nanos), false, false).await? {
                self.close_inner().await?;
                return Err(NativeFailure::CpuBudgetExceeded);
            }
            Ok(())
        }
        .await;
        if result.is_err() && self.kill.is_some() {
            let _ = self.pause_now();
        }
        result
    }

    pub(super) async fn close(&mut self) -> Result<()> {
        let account = self.account.clone();
        let _gate = match &account {
            Some(account) => Some(account.gate.lock().await),
            None => None,
        };
        if account.as_ref().is_some_and(ResourceAccount::is_closed) {
            return Ok(());
        }
        self.close_inner().await
    }

    async fn close_inner(&mut self) -> Result<()> {
        use super::close_diagnostics::{failure, Phase};
        if let Some(account) = self.account.clone() {
            super::service::barrier(&account.resource.name())
                .await
                .map_err(|error| failure(Phase::HostBarrier, error))?;
            let nanos = if let Some(nanos) = self.final_cpu {
                nanos
            } else {
                let nanos = self
                    .pause()
                    .await
                    .map_err(|error| failure(Phase::HostFreeze, error))
                    .and_then(|_| {
                        self.cpu_nanoseconds()
                            .map_err(|error| failure(Phase::HostCpuSample, error))
                    })
                    .ok();
                if let Some(nanos) = nanos {
                    account
                        .checkpoint(Some(nanos), true, false)
                        .await
                        .map_err(|error| failure(Phase::HostCpuCheckpoint, error))?;
                }
                self.final_cpu = Some(nanos);
                nanos
            };
            self.kill()
                .map_err(|error| failure(Phase::HostKill, error))?;
            super::service::stop_and_confirm(&account.resource.name())
                .await
                .map_err(|error| failure(Phase::HostStop, error))?;
            account
                .checkpoint(nanos, nanos.is_some(), true)
                .await
                .map_err(|error| failure(Phase::HostClosedCheckpoint, error))?;
            account.mark_closed();
            return Ok(());
        }
        self.kill()
            .map_err(|error| failure(Phase::ScopeKill, error))?;
        self.wait_gone()
            .await
            .map_err(|error| failure(Phase::ScopeRelease, error))
    }

    async fn wait_gone(&self) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(3), async {
            while self.same_group()? {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok(())
        })
        .await
        .map_err(|_| NativeFailure::Unavailable)?
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if !self.cleanup_on_drop
            || self
                .account
                .as_ref()
                .is_some_and(ResourceAccount::is_closed)
        {
            return;
        }
        // On lease loss/owner cancellation, retain the frozen CPU counter for
        // the next fenced owner. SIGKILL is handled by the same recovery path.
        let result = if self.account.is_some() && self.kill.is_some() {
            self.pause_now()
        } else {
            self.kill()
        };
        if result.is_err() {
            tracing::warn!("native Mission process group cleanup remains unconfirmed");
        }
    }
}

pub struct MissionProcess {
    pub(super) server_binary: Option<PathBuf>,
    pub(super) run_id: Id,
    pub(super) limits: JobLimitsV1,
    pub(super) deadline: Option<Instant>,
    pub(super) authority: Option<(Store, WorkerFence)>,
    pub(super) account: Option<ResourceAccount>,
    pub(super) monitor_sender: Option<tokio::sync::watch::Sender<Option<super::ResourceMonitor>>>,
    pub(super) reconciliation_thread_id: Option<String>,
}

impl MissionProcess {
    pub(crate) fn set_monitor(
        &mut self,
        sender: Option<tokio::sync::watch::Sender<Option<super::ResourceMonitor>>>,
    ) {
        self.monitor_sender = sender;
    }
    pub(crate) fn for_reconciliation(mut self, thread_id: String) -> Self {
        self.reconciliation_thread_id = Some(thread_id);
        self
    }
    pub(crate) fn with_authority(mut self, store: &Store, fence: &WorkerFence) -> Self {
        self.authority = Some((store.clone(), fence.clone()));
        self
    }

    pub(super) async fn begin_launch(&self) -> Result<()> {
        if let Some(account) = &self.account {
            account.begin_launch().await?;
        }
        Ok(())
    }
    pub(super) async fn abort_before_spawn(&self) -> Result<()> {
        if let Some(account) = &self.account {
            account.abort_before_spawn().await?;
        }
        Ok(())
    }
    pub(super) fn name(&self) -> String {
        self.account.as_ref().map_or_else(
            || format!("quazonai-mission-{}", self.run_id),
            |a| a.resource.name(),
        )
    }

    /// Reclaim stale exact resources before any terminal/advance shortcut.
    /// This path neither reserves a launch nor opens Codex or a model session.
    pub(crate) async fn recover_previous(
        store: &Store,
        run: Id,
        fence: &WorkerFence,
        backend: Option<&super::ContainerBackend>,
    ) -> Result<()> {
        let kind = if backend.is_some() { "DOCKER" } else { "HOST" };
        for resource in super::resource_account::pending(store, run, fence).await? {
            if resource.backend != kind || !resource.reclaimable_by(run, fence) {
                return Err(NativeFailure::Correlation);
            }
            let account = ResourceAccount {
                store: store.clone(),
                fence: fence.clone(),
                prior_nanoseconds: None,
                grant_nanoseconds: resource
                    .effective_limits
                    .cpu_seconds
                    .map(|cpu| {
                        cpu.get()
                            .checked_mul(1_000_000_000)
                            .ok_or(NativeFailure::Configuration)
                    })
                    .transpose()?,
                resource,
                gate: Default::default(),
                closed: Default::default(),
            };
            if !account.resource.launch_requested {
                account.abort_before_spawn().await?;
                continue;
            }
            if let Some(backend) = backend {
                super::container::recover(backend, account).await?;
            } else {
                recover(account).await?;
            }
        }
        Ok(())
    }

    pub(super) async fn prepare(
        &mut self,
        backend: Option<&super::ContainerBackend>,
    ) -> Result<()> {
        let Some((store, fence)) = self.authority.clone() else {
            return Ok(());
        };
        Self::recover_previous(&store, self.run_id, &fence, backend).await?;
        let kind = if backend.is_some() { "DOCKER" } else { "HOST" };
        let reserve_started = Instant::now();
        let (resource, prior_nanoseconds) = tokio::time::timeout(
            Duration::from_secs(2),
            store.reserve_mission_resource_for(
                self.run_id,
                &fence,
                kind,
                if self.reconciliation_thread_id.is_some() {
                    store::lifecycle::mission::resources::ResourcePurpose::Reconcile
                } else {
                    store::lifecycle::mission::resources::ResourcePurpose::Research
                },
            ),
        )
        .await
        .map_err(|_| NativeFailure::Unavailable)?
        .map_err(|_| NativeFailure::Unavailable)?;
        self.limits = resource.effective_limits.clone();
        self.deadline = resource
            .deadline_at
            .map(|deadline| {
                (deadline - resource.created_at)
                    .to_std()
                    .map(|remaining| reserve_started + remaining)
                    .map_err(|_| NativeFailure::Unavailable)
            })
            .transpose()?;
        self.account = Some(ResourceAccount {
            store,
            fence,
            resource,
            prior_nanoseconds,
            grant_nanoseconds: self
                .limits
                .cpu_seconds
                .map(|cpu| {
                    cpu.get()
                        .checked_mul(1_000_000_000)
                        .ok_or(NativeFailure::Configuration)
                })
                .transpose()?,
            gate: Default::default(),
            closed: Default::default(),
        });
        Ok(())
    }

    /// Drop sends cgroup.kill synchronously, but systemd collects the scope
    /// asynchronously. Never race a new launch against that original unit name.
    /// An actually live scope stays untouched and causes a bounded refusal.
    pub(super) async fn wait_released(&self) -> Result<()> {
        if self.account.is_some() {
            return super::service::probe_fresh(&self.name()).await;
        }
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or(NativeFailure::Unavailable)?;
        let wait = self.deadline.map_or(Duration::from_secs(3), |deadline| {
            Duration::from_secs(3).min(deadline.saturating_duration_since(Instant::now()))
        });
        tokio::time::timeout(wait, async {
            loop {
                let output = Command::new("/usr/bin/systemctl")
                    .args(["--user", "show", "--property=LoadState", "--value"])
                    .arg(format!("{}.scope", self.name()))
                    .env_clear()
                    .env("XDG_RUNTIME_DIR", &runtime)
                    .stdin(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .kill_on_drop(true)
                    .output()
                    .await
                    .map_err(|_| NativeFailure::Unavailable)?;
                if !output.status.success() || output.stdout.len() > 64 {
                    return Err(NativeFailure::Unavailable);
                }
                match output.stdout.as_slice() {
                    b"not-found\n" => return Ok(()),
                    b"loaded\n" => tokio::time::sleep(Duration::from_millis(20)).await,
                    _ => return Err(NativeFailure::Unavailable),
                }
            }
        })
        .await
        .map_err(|_| NativeFailure::Unavailable)?
    }

    pub(super) fn capture(&self, pid: u32) -> Result<ProcessGroup> {
        let memberships = std::fs::read_to_string(format!("/proc/{pid}/cgroup"))
            .map_err(|_| NativeFailure::Unavailable)?;
        let group = memberships
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .ok_or(NativeFailure::Configuration)?;
        if !group.starts_with("/user.slice/")
            || group.contains("..")
            || !group.ends_with(&format!(
                "{}.{}",
                self.name(),
                if self.account.is_some() {
                    "service"
                } else {
                    "scope"
                }
            ))
        {
            return Err(NativeFailure::Configuration);
        }
        let directory = PathBuf::from("/sys/fs/cgroup").join(group.trim_start_matches('/'));
        let metadata = directory
            .metadata()
            .map_err(|_| NativeFailure::Unavailable)?;
        let kill = OpenOptions::new()
            .write(true)
            .open(directory.join("cgroup.kill"))
            .map_err(|_| NativeFailure::Unavailable)?;
        let mut owned = ProcessGroup {
            freeze: OpenOptions::new()
                .write(true)
                .open(directory.join("cgroup.freeze"))
                .map_err(|_| NativeFailure::Unavailable)?,
            kill: None,
            directory,
            device: metadata.dev(),
            inode: metadata.ino(),
            account: self.account.clone(),
            final_cpu: None,
            cleanup_on_drop: true,
        };
        let members = std::fs::read_to_string(owned.directory.join("cgroup.procs"))
            .map_err(|_| NativeFailure::Unavailable)?;
        // Do not grant cleanup authority to a failed duplicate scope launch.
        if !owned.same_group()? || !members.lines().any(|line| line == pid.to_string()) {
            return Err(NativeFailure::Correlation);
        }
        owned.kill = Some(kill);
        Ok(owned)
    }

    pub(super) async fn capture_bound(&self, helper_pid: u32) -> Result<ProcessGroup> {
        let (mut group, invocation) = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if self.account.is_some() {
                    if let Ok(observed) = super::service::observe(&self.name()).await {
                        if observed.main_pid > 0
                            && observed.invocation.len() == 32
                            && observed.active == "active"
                        {
                            if let Ok(group) = self.capture(observed.main_pid) {
                                let after = super::service::observe(&self.name()).await?;
                                if after.invocation != observed.invocation
                                    || after.main_pid != observed.main_pid
                                    || after.group != observed.group
                                {
                                    return Err(NativeFailure::Correlation);
                                }
                                return Ok::<_, NativeFailure>((group, Some(observed.invocation)));
                            }
                        }
                    }
                } else if let Ok(group) = self.capture(helper_pid) {
                    return Ok((group, None));
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| NativeFailure::Unavailable)??;
        if let Some(account) = &group.account {
            let physical = serde_json::to_string(&(
                group.directory.to_string_lossy(),
                group.device,
                group.inode,
                invocation.ok_or(NativeFailure::Correlation)?,
            ))
            .map_err(|_| NativeFailure::Contract)?;
            if let Err(error) = account.bind(&physical).await {
                let _ = group.pause_now();
                return Err(error);
            }
        }
        if let Some(sender) = &self.monitor_sender {
            sender.send_replace(Some(group.monitor()?));
        }
        group.check_cpu().await?;
        Ok(group)
    }

    /// The caller supplies remaining whole seconds from its fresh DB observation.
    pub fn new(run_id: Id, limits: JobLimitsV1, remaining_seconds: Option<u32>) -> Result<Self> {
        // A QZ-owned poller cannot survive SIGKILL. A finite cumulative
        // CPU cap without an independent wall bound still needs an independent
        // enforcer. No such condition applies when CPU is explicitly unlimited.
        if limits.wall_seconds.is_none() && limits.cpu_seconds.is_some() {
            return Err(NativeFailure::UnboundedMissionLifecycleUnavailable);
        }
        if remaining_seconds == Some(0)
            || domain::execution_limits::exceeds(remaining_seconds, limits.wall_seconds)
        {
            return Err(NativeFailure::Configuration);
        }
        Ok(Self {
            server_binary: None,
            authority: None,
            account: None,
            monitor_sender: None,
            reconciliation_thread_id: None,
            run_id,
            limits,
            deadline: remaining_seconds
                .map(|seconds| Instant::now() + Duration::from_secs(u64::from(seconds))),
        })
    }

    pub(crate) fn with_server_binary(mut self, binary: &std::path::Path) -> Result<Self> {
        if !binary.is_absolute() || !binary.is_file() {
            return Err(NativeFailure::Configuration);
        }
        self.server_binary =
            Some(std::fs::canonicalize(binary).map_err(|_| NativeFailure::Configuration)?);
        Ok(self)
    }

    pub(super) fn wrap(&self, native: Command) -> Result<Command> {
        let limits = &self.limits;
        let remaining_seconds = self
            .deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()).as_secs());
        if limits.wall_seconds == Some(0)
            || remaining_seconds == Some(0)
            || limits.memory_mib == 0
            || limits.output_bytes.is_some_and(|bytes| bytes.get() == 0)
        {
            return Err(NativeFailure::Configuration);
        }
        // A rate, not a mistaken conversion of cumulative CPU seconds to cores.
        // systemd's percentage parser accepts hundredths, not thousandths.
        // Floor conservatively; the kernel minimum is 1ms per 1s.
        let quota = limits
            .cpu_seconds
            .zip(limits.wall_seconds)
            .map_or(10_000, |(cpu, wall)| {
                u128::from(cpu.get()) * 10_000 / u128::from(wall)
            });
        if !(10..=100_000_000).contains(&quota) {
            return Err(NativeFailure::Configuration);
        }
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && path.is_dir())
            .ok_or(NativeFailure::Unavailable)?;
        let command = native.as_std();
        let environment: Vec<(OsString, Option<OsString>)> = command
            .get_envs()
            .map(|(name, value)| (name.to_owned(), value.map(ToOwned::to_owned)))
            .collect();
        let mut bounded = Command::new("/usr/bin/systemd-run");
        bounded.args(["--user", "--quiet", "--collect", "--expand-environment=no"]);
        if self.account.is_some() {
            bounded.args([
                "--service-type=exec",
                "--pipe",
                "--wait",
                "--property=Restart=no",
                "--property=KillMode=control-group",
                "--property=TimeoutStopSec=3s",
            ]);
            for (name, value) in &environment {
                if value.is_some() {
                    let name = name
                        .to_str()
                        .filter(|name| super::service_exec::environment_name(name))
                        .ok_or(NativeFailure::Configuration)?;
                    bounded.arg(format!("--setenv={name}"));
                }
            }
            if let Some(directory) = command.get_current_dir() {
                bounded.arg(format!(
                    "--working-directory={}",
                    directory.to_str().ok_or(NativeFailure::Configuration)?
                ));
            }
        } else {
            bounded.arg("--scope");
        }
        bounded
            .arg(format!("--unit={}", self.name()))
            .arg(format!(
                "--property=CPUQuota={}.{:02}%",
                quota / 100,
                quota % 100
            ))
            .arg("--property=CPUQuotaPeriodSec=1s")
            .arg(format!("--property=MemoryMax={}M", limits.memory_mib))
            .args(["--property=MemorySwapMax=0", "--property=TasksMax=128"])
            .args(remaining_seconds.map(|seconds| format!("--property=RuntimeMaxSec={seconds}")))
            .args(["--", "/usr/bin/prlimit", "--core=0:0"])
            .args(
                limits
                    .cpu_seconds
                    .map(|cpu| format!("--cpu={0}:{0}", cpu.get())),
            )
            .args(
                limits
                    .output_bytes
                    .map(|_| format!("--fsize={MAX_NATIVE_FILE_BYTES}:{MAX_NATIVE_FILE_BYTES}")),
            )
            .arg("--");
        if self.account.is_some() {
            bounded
                .arg(
                    self.server_binary
                        .as_ref()
                        .ok_or(NativeFailure::Configuration)?,
                )
                .args(["native-codex-exec", "--binary"])
                .arg(command.get_program());
            for (name, value) in &environment {
                if value.is_some() {
                    bounded.arg("--environment-name").arg(name);
                }
            }
        } else {
            bounded.arg(command.get_program()).args(command.get_args());
        }
        bounded.env_clear();
        for (name, value) in environment {
            if let Some(value) = value {
                bounded.env(name, value);
            }
        }
        bounded.env("XDG_RUNTIME_DIR", runtime);
        if let Some(directory) = command.get_current_dir() {
            bounded.current_dir(directory);
        }
        Ok(bounded)
    }
}

fn parse_cpu_stat(value: &str) -> Result<u64> {
    let mut values = value
        .lines()
        .filter_map(|line| line.strip_prefix("usage_usec "));
    let nanos = values
        .next()
        .ok_or(NativeFailure::Unavailable)?
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(1000))
        .ok_or(NativeFailure::Unavailable)?;
    if values.next().is_some() {
        return Err(NativeFailure::Unavailable);
    }
    Ok(nanos)
}

async fn recover(account: ResourceAccount) -> Result<()> {
    let resource = &account.resource;
    super::service::barrier(&resource.name()).await?;
    let observed = super::service::observe(&resource.name()).await?;
    let Some(path) = observed.group else {
        super::service::stop_and_confirm(&resource.name()).await?;
        account
            .checkpoint(
                if resource.final_accounted {
                    resource.cpu_nanoseconds
                } else {
                    None
                },
                resource.final_accounted,
                true,
            )
            .await?;
        return Ok(());
    };
    if !path.starts_with("/user.slice/")
        || path.contains("..")
        || !path.ends_with(&format!("{}.service", resource.name()))
    {
        return Err(NativeFailure::Correlation);
    }
    let directory = PathBuf::from("/sys/fs/cgroup").join(path.trim_start_matches('/'));
    let metadata = match directory.metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            super::service::stop_and_confirm(&resource.name()).await?;
            account
                .checkpoint(
                    if resource.final_accounted {
                        resource.cpu_nanoseconds
                    } else {
                        None
                    },
                    resource.final_accounted,
                    true,
                )
                .await?;
            return Ok(());
        }
        Err(_) => return Err(NativeFailure::Unavailable),
    };
    if observed.invocation.len() != 32 {
        return Err(NativeFailure::Correlation);
    }
    let physical = serde_json::to_string(&(
        directory.to_string_lossy(),
        metadata.dev(),
        metadata.ino(),
        &observed.invocation,
    ))
    .map_err(|_| NativeFailure::Contract)?;
    if resource
        .physical_id
        .as_ref()
        .is_some_and(|expected| expected != &physical)
    {
        return Err(NativeFailure::Correlation);
    }
    let kill = OpenOptions::new()
        .write(true)
        .open(directory.join("cgroup.kill"))
        .map_err(|_| NativeFailure::Unavailable)?;
    let final_cpu = resource.final_accounted.then_some(resource.cpu_nanoseconds);
    let mut owned = ProcessGroup {
        freeze: OpenOptions::new()
            .write(true)
            .open(directory.join("cgroup.freeze"))
            .map_err(|_| NativeFailure::Unavailable)?,
        kill: Some(kill),
        directory,
        device: metadata.dev(),
        inode: metadata.ino(),
        account: Some(account),
        final_cpu,
        cleanup_on_drop: true,
    };
    // Exact final samples remain immutable. If physical stats disappeared,
    // closure preserves unknown instead of granting a new finite workload.
    let frozen = owned.pause().await;
    if let (Some(expected), Ok(())) = (owned.final_cpu.flatten(), frozen) {
        if let Ok(actual) = owned.cpu_nanoseconds() {
            if actual != expected {
                return Err(NativeFailure::Correlation);
            }
        }
    }
    owned.close().await
}

#[cfg(test)]
mod accounting_tests {
    use super::*;
    #[test]
    fn finite_cpu_without_wall_requires_independent_enforcement_but_unlimited_cpu_does_not() {
        let limits = JobLimitsV1 {
            schema_version: contracts::SchemaV1,
            experiments: 0,
            cpu_seconds: Some(contracts::DbCounter::new(1).unwrap()),
            wall_seconds: None,
            memory_mib: 64,
            output_bytes: Some(contracts::DbCounter::new(1_048_576).unwrap()),
        };
        let mut unlimited = limits.clone();
        unlimited.cpu_seconds = None;
        unlimited.output_bytes = None;
        assert!(MissionProcess::new(Id::new(), unlimited, None).is_ok());
        assert!(matches!(
            MissionProcess::new(Id::new(), limits, None),
            Err(NativeFailure::UnboundedMissionLifecycleUnavailable)
        ));
    }
    #[test]
    fn unknown_service_state_is_never_terminal_cleanup_evidence() {
        for text in [
            "",
            "LoadState=masked\nActiveState=inactive\n",
            "ControlGroup=\n",
        ] {
            assert!(super::super::service::parse(text).is_err());
        }
    }
    #[test]
    fn cumulative_cpu_requires_one_complete_monotonic_kernel_counter() {
        assert_eq!(
            parse_cpu_stat("usage_usec 2400000\nuser_usec 1000000\nsystem_usec 1400000\n").unwrap(),
            2_400_000_000
        );
        for value in [
            "",
            "user_usec 4\n",
            "usage_usec -1\n",
            "usage_usec 2\nusage_usec 3\n",
            "usage_usec 18446744073709551615\n",
        ] {
            assert!(parse_cpu_stat(value).is_err());
        }
    }
}

#[cfg(all(test, feature = "native-codex"))]
mod tests {
    use super::*;
    use contracts::{DbCounter, SchemaV1};
    use std::{os::unix::process::ExitStatusExt, process::Stdio};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    fn limits() -> JobLimitsV1 {
        JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 0,
            cpu_seconds: Some(DbCounter::new(1).unwrap()),
            wall_seconds: Some(5),
            memory_mib: 64,
            output_bytes: Some(DbCounter::new(1048576).unwrap()),
        }
    }

    #[test]
    fn expired_or_sub_kernel_precision_cpu_is_not_silently_relaxed() {
        assert!(MissionProcess::new(Id::new(), limits(), Some(0)).is_err());
        assert!(MissionProcess::new(Id::new(), limits(), Some(6)).is_err());
        let mut limits = limits();
        limits.wall_seconds = Some(1001);
        let bounded = MissionProcess::new(Id::new(), limits, Some(3)).unwrap();
        assert!(matches!(
            bounded.wrap(Command::new("/usr/bin/true")),
            Err(NativeFailure::Configuration)
        ));
    }

    #[tokio::test]
    async fn real_cgroup_cpu_counter_includes_sequential_exited_children() {
        // Disposable mechanism probe only. Each child has a one-second CPU
        // RLIMIT, yet the aggregate tree consumes more than one CPU second.
        // The separate account/driver tests cover applying the cumulative grant.
        let root = tempfile::tempdir().unwrap();
        let mut allocation = limits();
        allocation.wall_seconds = Some(15);
        allocation.cpu_seconds = Some(DbCounter::new(15).unwrap());
        let bound = MissionProcess::new(Id::new(), allocation, Some(15)).unwrap();
        let mut native = Command::new("/usr/bin/sh");
        native.args(["-c", "printf 'READY\\n'; read -r gate; for i in 1 2 3; do /usr/bin/prlimit --cpu=1:1 -- /usr/bin/python3 -c 'import time; start=time.process_time(); exec(\"while time.process_time()-start < 0.6: pass\")' || exit 1; done; printf 'DONE\\n'; read -r gate"])
            .env_clear().current_dir(root.path());
        let mut child = bound
            .wrap(native)
            .unwrap()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(3), output.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(line, "READY\n");
        let mut group = bound.capture(child.id().unwrap()).unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"GO\n")
            .await
            .unwrap();
        line.clear();
        tokio::time::timeout(Duration::from_secs(10), output.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(line, "DONE\n");
        assert!(group.cpu_nanoseconds().unwrap() >= 1_800_000_000);
        group.close().await.unwrap();
        child.wait().await.unwrap();
    }

    #[tokio::test]
    async fn native_scope_drop_is_awaited_before_reuse_and_a_live_owner_is_not_killed() {
        let root = tempfile::tempdir().unwrap();
        let mut allocation = limits();
        allocation.wall_seconds = Some(30);
        allocation.cpu_seconds = Some(DbCounter::new(30).unwrap());
        let bound = MissionProcess::new(Id::new(), allocation, Some(30)).unwrap();
        bound.wait_released().await.unwrap();
        let mut native = Command::new("/usr/bin/sh");
        native
            .args(["-c", "printf 'READY\\n'; read -r line"])
            .env_clear()
            .current_dir(root.path());
        let mut child = bound
            .wrap(native)
            .unwrap()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut ready = String::new();
        tokio::time::timeout(Duration::from_secs(2), output.read_line(&mut ready))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ready, "READY\n");
        let owned = bound.capture(child.id().unwrap()).unwrap();
        assert!(
            bound.wait_released().await.is_err(),
            "a live owner must not be replaced"
        );
        assert!(child.try_wait().unwrap().is_none());
        drop(owned);
        bound.wait_released().await.unwrap();
        child.wait().await.unwrap();
        let mut replacement = Command::new("/usr/bin/true");
        replacement.env_clear().current_dir(root.path());
        assert!(bound
            .wrap(replacement)
            .unwrap()
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status()
            .await
            .unwrap()
            .success());
    }

    #[tokio::test]
    async fn kernel_actually_terminates_deadline_and_memory_exhaustion() {
        // These are disposable kernel probes, not model or scientific results.
        for (seconds, script, signal) in [
            (3, "set -eu; printf 'READY\\n'; read -r line; exec /usr/bin/sleep 30", 15),
            (10, "exec /usr/bin/awk 'BEGIN { print \"READY\"; fflush(); getline; for (i=0; ;i++) a[i]=sprintf(\"%01024d\", i) }'", 9),
        ] {
            let root = tempfile::tempdir().unwrap();
            let mut limits = limits();
            limits.wall_seconds = Some(seconds);
            limits.cpu_seconds = Some(DbCounter::new(u64::from(seconds)).unwrap());
            let bound = MissionProcess::new(Id::new(), limits, Some(seconds)).unwrap();
            let mut native = Command::new("/usr/bin/sh");
            native.args(["-c", script]).env_clear().current_dir(root.path());
            let mut child = bound.wrap(native).unwrap()
                .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
                .kill_on_drop(true).spawn().unwrap();
            let mut output = BufReader::new(child.stdout.take().unwrap());
            let mut ready = String::new();
            tokio::time::timeout(Duration::from_secs(2), output.read_line(&mut ready))
                .await.unwrap().unwrap();
            assert_eq!(ready, "READY\n");
            let mut owned = bound.capture(child.id().unwrap()).unwrap();
            child.stdin.as_mut().unwrap().write_all(b"GO\n").await.unwrap();
            let status = tokio::time::timeout(Duration::from_secs(u64::from(seconds) + 3), child.wait())
                .await.unwrap().unwrap();
            assert_eq!(status.signal(), Some(signal));
            owned.close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn real_scope_preserves_stdio_and_kernel_limits_without_credential_arguments() {
        let root = tempfile::tempdir().unwrap();
        let run = Id::new();
        let bound = MissionProcess::new(run, limits(), Some(5)).unwrap();
        let mut native = Command::new("/usr/bin/sh");
        native.args(["-c", "set -eu; /usr/bin/sleep 30 & printf 'READY\\n'; read -r line; test \"$line\" = QZ_TEST_INPUT; test \"${#QZ_TEST_CANARY}\" = 24; exec /usr/bin/cat /proc/self/limits"])
            .current_dir(root.path()).env_clear().env("PATH", "/usr/bin").env("QZ_TEST_CANARY", "TEST_ONLY_RESOURCE_VALUE");
        let mut command = bound.wrap(native).unwrap();
        // The fake canary is only an inherited value; not a systemd argument.
        assert!(!command
            .as_std()
            .get_args()
            .any(|arg| arg.to_string_lossy().contains("TEST_ONLY_RESOURCE_VALUE")));
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut ready = String::new();
        tokio::time::timeout(Duration::from_secs(3), output.read_line(&mut ready))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ready, "READY\n");
        let mut owned = bound.capture(child.id().unwrap()).unwrap();
        let status = Command::new("/usr/bin/systemctl")
            .args([
                "--user",
                "show",
                &format!("quazonai-mission-{run}.scope"),
                "--property=ControlGroup",
                "--value",
            ])
            .output()
            .await
            .unwrap();
        assert!(status.status.success());
        let group = String::from_utf8(status.stdout).unwrap();
        let group = group.trim();
        assert!(
            group.starts_with("/user.slice/")
                && group.ends_with(&format!("quazonai-mission-{run}.scope"))
                && !group.contains("..")
        );
        let group = PathBuf::from("/sys/fs/cgroup").join(group.trim_start_matches('/'));
        for (name, value) in [
            ("memory.max", "67108864"),
            ("memory.swap.max", "0"),
            ("pids.max", "128"),
            ("cpu.max", "200000 1000000"),
        ] {
            assert_eq!(
                std::fs::read_to_string(group.join(name)).unwrap().trim(),
                value
            );
        }
        let mut input = child.stdin.take().unwrap();
        input.write_all(b"QZ_TEST_INPUT\n").await.unwrap();
        input.shutdown().await.unwrap();
        assert!(child.wait().await.unwrap().success());
        // The original leader has exited, but its sleep descendant still owns
        // stdout. Closing the held cgroup must stop that descendant as well.
        owned.close().await.unwrap();
        let mut values = String::new();
        tokio::time::timeout(Duration::from_secs(3), output.read_to_string(&mut values))
            .await
            .unwrap()
            .unwrap();
        let fields: Vec<_> = values
            .lines()
            .find(|line| line.starts_with("Max file size"))
            .unwrap()
            .split_whitespace()
            .collect();
        assert_eq!(&fields[3..], &["67108864", "67108864", "bytes"]);
        let core: Vec<_> = values
            .lines()
            .find(|line| line.starts_with("Max core file size"))
            .unwrap()
            .split_whitespace()
            .collect();
        assert_eq!(&core[4..], &["0", "0", "bytes"]);
        tokio::time::timeout(Duration::from_secs(3), async {
            while group.exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }
}
