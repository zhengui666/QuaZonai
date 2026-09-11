//! Native Linux cgroup/rlimit configuration, not another process supervisor.
use super::{NativeFailure, Result};
use contracts::{lifecycle::JobLimitsV1, Id};
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::process::Command;

// Native SQLite/rollout files are not QZ research outputs. A 1 MiB artifact
// budget must not kill the native schema migration's >1 MiB SQLite WAL.
const MAX_NATIVE_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Holds the original kernel control file, never kills a later unit by its name.
pub(super) struct ProcessGroup {
    kill: Option<File>,
    directory: PathBuf,
    device: u64,
    inode: u64,
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

    pub(super) async fn close(&mut self) -> Result<()> {
        self.kill()?;
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
        if self.kill().is_err() {
            tracing::warn!("native Mission process group cleanup remains unconfirmed");
        }
    }
}

pub struct MissionProcess {
    run_id: Id,
    limits: JobLimitsV1,
    deadline: Instant,
}

impl MissionProcess {
    pub(super) fn capture(&self, pid: u32) -> Result<ProcessGroup> {
        let memberships = std::fs::read_to_string(format!("/proc/{pid}/cgroup"))
            .map_err(|_| NativeFailure::Unavailable)?;
        let group = memberships
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .ok_or(NativeFailure::Configuration)?;
        if !group.starts_with("/user.slice/")
            || group.contains("..")
            || !group.ends_with(&format!("quazonai-mission-{}.scope", self.run_id))
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
            kill: None,
            directory,
            device: metadata.dev(),
            inode: metadata.ino(),
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

    /// The caller supplies remaining whole seconds from its fresh DB observation.
    pub fn new(run_id: Id, limits: JobLimitsV1, remaining_seconds: u32) -> Result<Self> {
        if remaining_seconds == 0 || remaining_seconds > limits.wall_seconds {
            return Err(NativeFailure::Configuration);
        }
        Ok(Self {
            run_id,
            limits,
            deadline: Instant::now() + Duration::from_secs(u64::from(remaining_seconds)),
        })
    }

    pub(super) fn wrap(&self, native: Command) -> Result<Command> {
        let limits = &self.limits;
        let remaining_seconds = self
            .deadline
            .saturating_duration_since(Instant::now())
            .as_secs();
        if limits.wall_seconds == 0
            || remaining_seconds == 0
            || limits.memory_mib == 0
            || limits.output_bytes.get() == 0
        {
            return Err(NativeFailure::Configuration);
        }
        // A rate, not a mistaken conversion of cumulative CPU seconds to cores.
        // systemd's percentage parser accepts hundredths, not thousandths.
        // Floor conservatively; the kernel minimum is 1ms per 1s.
        let quota = u128::from(limits.cpu_seconds.get()) * 10_000 / u128::from(limits.wall_seconds);
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
        bounded
            .args([
                "--user",
                "--scope",
                "--quiet",
                "--collect",
                "--expand-environment=no",
            ])
            .arg(format!("--unit=quazonai-mission-{}", self.run_id))
            .arg(format!(
                "--property=CPUQuota={}.{:02}%",
                quota / 100,
                quota % 100
            ))
            .arg("--property=CPUQuotaPeriodSec=1s")
            .arg(format!("--property=MemoryMax={}M", limits.memory_mib))
            .args(["--property=MemorySwapMax=0", "--property=TasksMax=128"])
            .arg(format!("--property=RuntimeMaxSec={}", remaining_seconds))
            .args(["--", "/usr/bin/prlimit", "--core=0:0"])
            .arg(format!(
                "--fsize={MAX_NATIVE_FILE_BYTES}:{MAX_NATIVE_FILE_BYTES}"
            ))
            .arg("--")
            .arg(command.get_program())
            .args(command.get_args())
            .env_clear();
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
            cpu_seconds: DbCounter::new(1).unwrap(),
            wall_seconds: 5,
            memory_mib: 64,
            output_bytes: DbCounter::new(1048576).unwrap(),
        }
    }

    #[test]
    fn expired_or_sub_kernel_precision_cpu_is_not_silently_relaxed() {
        assert!(MissionProcess::new(Id::new(), limits(), 0).is_err());
        assert!(MissionProcess::new(Id::new(), limits(), 6).is_err());
        let mut limits = limits();
        limits.wall_seconds = 1001;
        let bounded = MissionProcess::new(Id::new(), limits, 3).unwrap();
        assert!(matches!(
            bounded.wrap(Command::new("/usr/bin/true")),
            Err(NativeFailure::Configuration)
        ));
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
            limits.wall_seconds = seconds;
            limits.cpu_seconds = DbCounter::new(u64::from(seconds)).unwrap();
            let bound = MissionProcess::new(Id::new(), limits, seconds).unwrap();
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
        let bound = MissionProcess::new(run, limits(), 5).unwrap();
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
