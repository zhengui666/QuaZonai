//! Native GNU timeout owns the wall deadline even after the Runtime gateway exits.
//! The OCI init/cgroup owns process-tree cleanup; there is no application watchdog daemon.
use anyhow::{ensure, Result};
use contracts::runtime_jobs::JobSpecV1;
use std::{fs::OpenOptions, io::Read, process::Command};

#[cfg(unix)]
pub fn run() -> Result<()> {
    use std::os::unix::{fs::OpenOptionsExt, process::CommandExt};
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open("/input/spec.json")?;
    ensure!(file.metadata()?.is_file(), "NATIVE_SPEC_FILE");
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "NATIVE_SPEC_SIZE");
    let spec: JobSpecV1 = serde_json::from_slice(&bytes)?;
    domain::runtime_jobs::spec_shape(&spec)?;
    let remaining = (spec.deadline_at - chrono::Utc::now())
        .num_milliseconds()
        .min(i64::from(spec.limits.wall_seconds) * 1000);
    ensure!(remaining > 0, "NATIVE_JOB_DEADLINE");
    let duration = format!("{}.{:03}s", remaining / 1000, remaining % 1000);
    let error = Command::new("/usr/bin/timeout")
        .args([
            "--signal=TERM",
            "--kill-after=1s",
            &duration,
            "/usr/local/bin/job",
            "execute",
        ])
        .env_clear()
        .env("PATH", "/opt/rust/bin:/usr/bin:/bin")
        .env("HOME", "/tmp")
        .env("TMPDIR", "/tmp")
        .exec();
    Err(error.into())
}

#[cfg(not(unix))]
pub fn run() -> Result<()> {
    anyhow::bail!("NATIVE_OCI_PLATFORM_REQUIRED")
}
