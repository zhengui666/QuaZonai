//! Native user-service creation revocation. No polling daemon or model session.
use super::{NativeFailure, Result};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::process::Command;

#[derive(Clone, Debug)]
pub(super) struct Observation {
    pub load: String,
    pub active: String,
    pub group: Option<String>,
    pub invocation: String,
    pub main_pid: u32,
    pub job: bool,
    pub fragment: String,
}
fn field<'a>(text: &'a str, name: &str) -> Result<&'a str> {
    let prefix = format!("{name}=");
    let mut values = text.lines().filter_map(|line| line.strip_prefix(&prefix));
    let value = values.next().ok_or(NativeFailure::Unavailable)?;
    if values.next().is_some() {
        return Err(NativeFailure::Unavailable);
    }
    Ok(value)
}
pub(super) fn parse(text: &str) -> Result<Observation> {
    let group = field(text, "ControlGroup")?;
    let job = field(text, "Job")?;
    Ok(Observation {
        load: field(text, "LoadState")?.into(),
        active: field(text, "ActiveState")?.into(),
        group: (!group.is_empty()).then(|| group.to_owned()),
        invocation: field(text, "InvocationID")?.into(),
        main_pid: field(text, "MainPID")?
            .parse()
            .map_err(|_| NativeFailure::Unavailable)?,
        // systemctl prints either an empty property or the native job ID/path.
        job: !job.is_empty() && job != "0",
        fragment: field(text, "FragmentPath")?.into(),
    })
}
fn unit(name: &str) -> Result<String> {
    if !name.starts_with("quazonai-mission-")
        || name.len() > 230
        || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err(NativeFailure::Configuration);
    }
    Ok(format!("{name}.service"))
}
async fn command(args: &[&str]) -> Result<std::process::Output> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_dir())
        .ok_or(NativeFailure::Unavailable)?;
    tokio::time::timeout(
        Duration::from_secs(5),
        Command::new("/usr/bin/systemctl")
            .args(["--user", "--no-ask-password"])
            .args(args)
            .env_clear()
            .env("XDG_RUNTIME_DIR", runtime)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| NativeFailure::Unavailable)?
    .map_err(|_| NativeFailure::Unavailable)
}
pub(super) async fn observe(name: &str) -> Result<Observation> {
    let name = unit(name)?;
    let out = command(&[
        "show",
        "--property=LoadState,ActiveState,ControlGroup,InvocationID,MainPID,Job,FragmentPath",
        "--",
        &name,
    ])
    .await?;
    if out.stdout.len() > 16384 {
        return Err(NativeFailure::ObservationLimit);
    }
    let observed = parse(std::str::from_utf8(&out.stdout).map_err(|_| NativeFailure::Contract)?)?;
    if !out.status.success() && observed.load != "not-found" {
        return Err(NativeFailure::Unavailable);
    }
    Ok(observed)
}
async fn masked(name: &str) -> Result<()> {
    let name = unit(name)?;
    let out = command(&["is-enabled", "--", &name]).await?;
    // mask is intentionally persistent: runtime user masks disappear on logout.
    // Neither this helper nor normal cleanup unmask an old launch identity.
    // A live transient fragment has lookup priority over the persistent mask.
    // Only callers with a successful exact-name mask + reload acknowledgement
    // may accept this state; stopped transient identity remains non-pristine.
    if !matches!(out.stdout.as_slice(), b"masked\n" | b"transient\n") {
        return Err(NativeFailure::Unavailable);
    }
    Ok(())
}
async fn manager_call(method: &str, arguments: &[&str]) -> Result<()> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_dir())
        .ok_or(NativeFailure::Unavailable)?;
    let out = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new("/usr/bin/busctl")
            .args([
                "--user",
                "--allow-interactive-authorization=no",
                "--timeout=5s",
                "call",
                "org.freedesktop.systemd1",
                "/org/freedesktop/systemd1",
                "org.freedesktop.systemd1.Manager",
                method,
            ])
            .args(arguments)
            .env_clear()
            .env("XDG_RUNTIME_DIR", runtime)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| NativeFailure::Unavailable)?
    .map_err(|_| NativeFailure::Unavailable)?;
    if !out.status.success() || out.stdout.len() > 16384 {
        return Err(NativeFailure::Unavailable);
    }
    Ok(())
}
pub(super) async fn barrier(name: &str) -> Result<()> {
    let full = unit(name)?;
    // Direct native manager methods: systemctl's offline/client-side file
    // installation fallback is not evidence that this manager is fenced.
    manager_call("MaskUnitFiles", &["asbb", "1", &full, "false", "false"]).await?;
    manager_call("Reload", &[]).await?;
    masked(name).await
}
/// Only a brand-new, unpermitted identity can probe and remove its own mask.
pub(super) async fn probe_fresh(name: &str) -> Result<()> {
    let observed = observe(name).await?;
    if observed.load != "not-found"
        || observed.job
        || observed.group.is_some()
        || observed.main_pid != 0
    {
        return Err(NativeFailure::Correlation);
    }
    barrier(name).await?;
    let full = unit(name)?;
    manager_call("UnmaskUnitFiles", &["asb", "1", &full, "false"]).await?;
    manager_call("Reload", &[]).await?;
    let observed = observe(name).await?;
    if observed.load != "not-found"
        || observed.job
        || observed.group.is_some()
        || observed.main_pid != 0
    {
        return Err(NativeFailure::Correlation);
    }
    Ok(())
}
pub(super) async fn stop_and_confirm(name: &str) -> Result<()> {
    barrier(name).await?;
    let full = unit(name)?;
    // replace explicitly supersedes an earlier queued JOB_START. A mask alone
    // does not cancel a start that the native manager already accepted.
    let stopped = command(&["stop", "--job-mode=replace", "--", &full]).await?;
    let observed = observe(name).await?;
    masked(name).await?;
    if observed.job
        || observed.main_pid != 0
        || !matches!(observed.active.as_str(), "inactive" | "failed")
    {
        return Err(NativeFailure::Unavailable);
    }
    if let Some(group) = observed.group {
        if !group.starts_with("/user.slice/") || group.contains("..") || !group.ends_with(&full) {
            return Err(NativeFailure::Correlation);
        }
        let path = PathBuf::from("/sys/fs/cgroup").join(group.trim_start_matches('/'));
        if path.exists() {
            let events = std::fs::read_to_string(path.join("cgroup.events"))
                .map_err(|_| NativeFailure::Unavailable)?;
            if !events.lines().any(|line| line == "populated 0") {
                return Err(NativeFailure::Unavailable);
            }
        }
    }
    // A stopped transient may remain referenced by systemd-run --wait. Its
    // fragment makes it non-pristine; after GC the persistent mask is loaded.
    let fenced = observed.load == "masked"
        || (stopped.status.success() && observed.load == "loaded" && !observed.fragment.is_empty());
    if !fenced {
        return Err(NativeFailure::Unavailable);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manager_identity_requires_all_fields_and_preserves_pending_jobs() {
        let value = parse("LoadState=loaded\nActiveState=activating\nControlGroup=\nInvocationID=\nMainPID=0\nJob=31\nFragmentPath=/run/user/1/systemd/transient/x.service\n").unwrap();
        assert!(value.job);
        assert_eq!(value.main_pid, 0);
        assert_eq!(value.group, None);
        for invalid in [
            "",
            "LoadState=masked\nActiveState=inactive\n",
            "LoadState=masked\nLoadState=loaded\n",
        ] {
            assert!(parse(invalid).is_err());
        }
        assert!(unit("unrelated.service").is_err());
        assert!(unit("quazonai-mission-../other").is_err());
    }
}

#[cfg(all(test, feature = "native-codex"))]
mod native_tests {
    use super::*;
    fn name() -> String {
        format!(
            "quazonai-mission-{}-{}-1-{}",
            contracts::Id::new(),
            contracts::Id::new(),
            contracts::Id::new()
        )
    }
    async fn unmask_test_unit(name: &str) {
        let full = unit(name).unwrap();
        assert!(command(&["unmask", "--", &full])
            .await
            .unwrap()
            .status
            .success());
        assert!(command(&["daemon-reload"]).await.unwrap().status.success());
    }
    #[tokio::test]
    async fn native_persistent_mask_rejects_a_sigstopped_late_launcher() {
        let name = name();
        probe_fresh(&name).await.unwrap();
        let mut launcher = Command::new("/usr/bin/bash");
        launcher
            .args([
                "--noprofile",
                "--norc",
                "-c",
                "kill -STOP $$; exec \"$@\"",
                "qz-late-launch-test",
                "/usr/bin/systemd-run",
                "--user",
                "--quiet",
                "--pipe",
                "--wait",
                "--collect",
                "--service-type=exec",
                "--expand-environment=no",
            ])
            .arg(format!("--unit={name}"))
            .args(["--property=RuntimeMaxSec=20s", "/usr/bin/sleep", "20"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = launcher.spawn().unwrap();
        let pid = child.id().unwrap();
        let result: Result<()> = async {
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let status = std::fs::read_to_string(format!("/proc/{pid}/status"))
                        .map_err(|_| NativeFailure::Unavailable)?;
                    if status
                        .lines()
                        .any(|line| line.starts_with("State:") && line.contains('T'))
                    {
                        return Ok::<_, NativeFailure>(());
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .map_err(|_| NativeFailure::Unavailable)??;
            stop_and_confirm(&name).await?;
            // Owned unreaped SIGSTOP child: only this test's launcher is resumed.
            let status = Command::new("/usr/bin/kill")
                .args(["-CONT", &pid.to_string()])
                .status()
                .await
                .map_err(|_| NativeFailure::Unavailable)?;
            if !status.success() {
                return Err(NativeFailure::Unavailable);
            }
            let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
                .await
                .map_err(|_| NativeFailure::Unavailable)?
                .map_err(|_| NativeFailure::Unavailable)?;
            if status.success() {
                return Err(NativeFailure::Correlation);
            }
            stop_and_confirm(&name).await?;
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        unmask_test_unit(&name).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn native_stop_replace_cancels_an_existing_start_and_leaves_no_populated_group() {
        let name = name();
        probe_fresh(&name).await.unwrap();
        let mut child = Command::new("/usr/bin/systemd-run")
            .args([
                "--user",
                "--quiet",
                "--pipe",
                "--wait",
                "--collect",
                "--service-type=exec",
                "--expand-environment=no",
            ])
            .arg(format!("--unit={name}"))
            .args(["--property=RuntimeMaxSec=20s", "/usr/bin/sleep", "20"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let result: Result<()> = async {
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let observed = observe(&name).await?;
                    if observed.job || observed.main_pid > 0 {
                        return Ok::<_, NativeFailure>(());
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .map_err(|_| NativeFailure::Unavailable)??;
            stop_and_confirm(&name).await?;
            tokio::time::timeout(Duration::from_secs(5), child.wait())
                .await
                .map_err(|_| NativeFailure::Unavailable)?
                .map_err(|_| NativeFailure::Unavailable)?;
            stop_and_confirm(&name).await?;
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        unmask_test_unit(&name).await;
        result.unwrap();
    }
    #[tokio::test]
    async fn native_stop_replace_cancels_a_deterministically_queued_start() {
        let blocker = name();
        let target = name();
        probe_fresh(&blocker).await.unwrap();
        probe_fresh(&target).await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("workload-started");
        let blocking = Command::new("/usr/bin/systemd-run")
            .args([
                "--user",
                "--quiet",
                "--no-block",
                "--collect",
                "--service-type=oneshot",
                "--property=TimeoutStartSec=20s",
            ])
            .arg(format!("--unit={blocker}"))
            .args(["/usr/bin/sleep", "20"])
            .status()
            .await
            .unwrap();
        assert!(blocking.success());
        let mut child = Command::new("/usr/bin/systemd-run")
            .args([
                "--user",
                "--quiet",
                "--pipe",
                "--wait",
                "--collect",
                "--service-type=exec",
                "--expand-environment=no",
            ])
            .arg(format!("--unit={target}"))
            .arg(format!("--property=After={blocker}.service"))
            .arg(format!("--property=Requires={blocker}.service"))
            .arg("/usr/bin/touch")
            .arg(&marker)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let result: Result<()> = async {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let observed = observe(&target).await?;
                    if observed.job && observed.main_pid == 0 {
                        return Ok::<_, NativeFailure>(());
                    }
                    if marker.exists() {
                        return Err(NativeFailure::Correlation);
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .map_err(|_| NativeFailure::Unavailable)??;
            stop_and_confirm(&target).await?;
            let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
                .await
                .map_err(|_| NativeFailure::Unavailable)?
                .map_err(|_| NativeFailure::Unavailable)?;
            if status.success() || marker.exists() {
                return Err(NativeFailure::Correlation);
            }
            stop_and_confirm(&target).await?;
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        stop_and_confirm(&blocker).await.unwrap();
        unmask_test_unit(&target).await;
        unmask_test_unit(&blocker).await;
        result.unwrap();
    }
}
