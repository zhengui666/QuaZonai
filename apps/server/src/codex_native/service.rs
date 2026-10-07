//! Native user-service creation revocation. No polling daemon or model session.
use super::{
    close_diagnostics::{failure, not_fenced, FenceLoad, FenceState, Phase},
    NativeFailure, Result,
};
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
    let observed = parse(std::str::from_utf8(&out.stdout).map_err(|_| NativeFailure::Contract)?);
    #[cfg(all(test, feature = "native-codex"))]
    if observed.is_err()
        || (!out.status.success() && observed.as_ref().is_ok_and(|v| v.load != "not-found"))
    {
        native_tests::trace_show(&name, out.status.code(), &out.stdout);
    }
    let observed = observed?;
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
        #[cfg(all(test, feature = "native-codex"))]
        native_tests::trace_install_state(&name, out.status.code(), &out.stdout);
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
    let full = unit(name).map_err(|error| failure(Phase::BarrierIdentity, error))?;
    // Direct native manager methods: systemctl's offline/client-side file
    // installation fallback is not evidence that this manager is fenced.
    let result = manager_call("MaskUnitFiles", &["asbb", "1", &full, "false", "false"]).await;
    #[cfg(all(test, feature = "native-codex"))]
    native_tests::trace_error(name, "barrier.mask", &result);
    result.map_err(|error| failure(Phase::BarrierMask, error))?;
    let result = manager_call("Reload", &[]).await;
    #[cfg(all(test, feature = "native-codex"))]
    native_tests::trace_error(name, "barrier.reload", &result);
    result.map_err(|error| failure(Phase::BarrierReload, error))?;
    let result = masked(name).await;
    #[cfg(all(test, feature = "native-codex"))]
    native_tests::trace_error(name, "barrier.verify-mask", &result);
    result.map_err(|error| failure(Phase::BarrierVerifyMask, error))
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
fn terminal(observed: &Observation) -> bool {
    !observed.job
        && observed.main_pid == 0
        && matches!(observed.active.as_str(), "inactive" | "failed")
}
fn exact_group(full: &str, group: &str) -> Result<PathBuf> {
    if !group.starts_with("/user.slice/") || group.contains("..") || !group.ends_with(full) {
        return Err(NativeFailure::Correlation);
    }
    Ok(PathBuf::from("/sys/fs/cgroup").join(group.trim_start_matches('/')))
}
fn unpopulated(events: &str) -> bool {
    events.lines().any(|line| line == "populated 0")
}
fn fenced(stopped: bool, observed: &Observation) -> bool {
    observed.load == "masked"
        || (stopped && observed.load == "loaded" && !observed.fragment.is_empty())
}
async fn observe_stopped(name: &str, _stop_code: Option<i32>) -> Result<Observation> {
    let full = unit(name)?;
    let observed = observe(name).await;
    #[cfg(all(test, feature = "native-codex"))]
    if let Err(error) = &observed {
        native_tests::trace(
            name,
            "stop.observe",
            format_args!("stop_code={:?} error={error:?}", _stop_code),
        );
    }
    let observed = observed.map_err(|error| failure(Phase::StopObserve, error))?;
    let mask = masked(name).await;
    #[cfg(all(test, feature = "native-codex"))]
    if mask.is_err() {
        native_tests::trace(
            name,
            "stop.verify-mask",
            format_args!(
                "stop_code={:?} observed={observed:?} error={mask:?}",
                _stop_code
            ),
        );
    }
    mask.map_err(|error| failure(Phase::StopVerifyMask, error))?;
    if !terminal(&observed) {
        #[cfg(all(test, feature = "native-codex"))]
        native_tests::trace(
            name,
            "stop.not-terminal",
            format_args!("stop_code={:?} observed={observed:?}", _stop_code),
        );
        return Err(failure(Phase::StopNotTerminal, NativeFailure::Unavailable));
    }
    if let Some(group) = &observed.group {
        let path =
            exact_group(&full, group).map_err(|error| failure(Phase::StopCgroupIdentity, error))?;
        if path.exists() {
            let events = std::fs::read_to_string(path.join("cgroup.events"));
            #[cfg(all(test, feature = "native-codex"))]
            if let Err(error) = &events {
                native_tests::trace(
                    name,
                    "stop.cgroup-read",
                    format_args!(
                        "stop_code={:?} os_code={:?} kind={:?}",
                        _stop_code,
                        error.raw_os_error(),
                        error.kind()
                    ),
                );
            }
            let events =
                events.map_err(|_| failure(Phase::StopCgroupRead, NativeFailure::Unavailable))?;
            if !unpopulated(&events) {
                #[cfg(all(test, feature = "native-codex"))]
                native_tests::trace(
                    name,
                    "stop.cgroup-populated",
                    format_args!("populated_zero=false stop_code={:?}", _stop_code),
                );
                return Err(failure(
                    Phase::StopCgroupPopulated,
                    NativeFailure::Unavailable,
                ));
            }
        }
    }
    Ok(observed)
}

fn fence_state(observed: &Observation) -> FenceState {
    FenceState {
        load: match observed.load.as_str() {
            "masked" => FenceLoad::Masked,
            "loaded" => FenceLoad::Loaded,
            "not-found" => FenceLoad::NotFound,
            _ => FenceLoad::Other,
        },
        fragment_present: !observed.fragment.is_empty(),
    }
}
struct StopObservation {
    observed: Observation,
    before: FenceState,
    refreshed: bool,
}

// `observation` performs the complete native mask/terminal/cgroup confirmation
// on every call. A failed stop may mean a previously killed --collect unit was
// already gone; refresh its stale name map only after this complete confirmation.
// One cache refresh is an action, never a successful fence.
async fn refresh_collected_observation<O, F, R>(
    mut observation: O,
    reload: R,
) -> Result<StopObservation>
where
    O: FnMut() -> F,
    F: std::future::Future<Output = Result<Observation>>,
    R: std::future::Future<Output = Result<()>>,
{
    let observed = observation().await?;
    let before = fence_state(&observed);
    if observed.load == "not-found" {
        reload.await?;
        return Ok(StopObservation {
            observed: observation().await?,
            before,
            refreshed: true,
        });
    }
    Ok(StopObservation {
        observed,
        before,
        refreshed: false,
    })
}

pub(super) async fn stop_and_confirm(name: &str) -> Result<()> {
    barrier(name).await?;
    let full = unit(name)?;
    // replace explicitly supersedes an earlier queued JOB_START. A mask alone
    // does not cancel a start that the native manager already accepted.
    let stopped = command(&["stop", "--job-mode=replace", "--", &full]).await;
    #[cfg(all(test, feature = "native-codex"))]
    if let Err(error) = &stopped {
        native_tests::trace(name, "stop.command", format_args!("error={error:?}"));
    }
    let stopped = stopped.map_err(|error| failure(Phase::StopCommand, error))?;
    let confirmation =
        refresh_collected_observation(|| observe_stopped(name, stopped.status.code()), async {
            // systemd 255 retains a name-map entry for a collected transient;
            // transient-directory changes do not invalidate that map. Refresh
            // only after confirmed termination, without unmasking this identity.
            let reload = manager_call("Reload", &[]).await;
            #[cfg(all(test, feature = "native-codex"))]
            native_tests::trace_error(name, "stop.reload-collected", &reload);
            reload.map_err(|error| failure(Phase::StopReloadCollected, error))
        })
        .await?;
    let observed = confirmation.observed;
    // A stopped transient may remain referenced by systemd-run --wait. Its
    // fragment makes it non-pristine; after GC the persistent mask is loaded.
    if !fenced(stopped.status.success(), &observed) {
        #[cfg(all(test, feature = "native-codex"))]
        native_tests::trace(
            name,
            "stop.not-fenced",
            format_args!(
                "stop_code={:?} load={:?} fragment={:?}",
                stopped.status.code(),
                observed.load,
                observed.fragment
            ),
        );
        return Err(not_fenced(
            stopped.status.success(),
            confirmation.refreshed,
            confirmation.before,
            fence_state(&observed),
        ));
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
    fn collected() -> Observation {
        parse("LoadState=not-found\nActiveState=inactive\nControlGroup=\nInvocationID=\nMainPID=0\nJob=\nFragmentPath=\n").unwrap()
    }
    fn confirmed_fixture(value: Observation, events: &str) -> Result<Observation> {
        if !terminal(&value) {
            return Err(NativeFailure::Unavailable);
        }
        if let Some(group) = &value.group {
            exact_group("quazonai-mission-fixture.service", group)?;
            if !unpopulated(events) {
                return Err(NativeFailure::Unavailable);
            }
        }
        Ok(value)
    }
    async fn refresh_fixture(
        observations: Vec<Result<Observation>>,
        reload_result: Result<()>,
    ) -> (Result<Observation>, usize, usize) {
        use std::{cell::Cell, collections::VecDeque, future::ready};
        let reads = Cell::new(0);
        let reloads = Cell::new(0);
        let mut observations = VecDeque::from(observations);
        let result = refresh_collected_observation(
            || {
                reads.set(reads.get() + 1);
                ready(
                    observations
                        .pop_front()
                        .expect("unexpected extra observation"),
                )
            },
            async {
                reloads.set(reloads.get() + 1);
                reload_result
            },
        )
        .await;
        (
            result.map(|confirmation| confirmation.observed),
            reads.get(),
            reloads.get(),
        )
    }
    #[tokio::test]
    async fn collected_transient_refresh_is_once_and_never_replaces_the_fence() {
        let mut mask = collected();
        mask.load = "masked".into();
        let (result, reads, reloads) =
            refresh_fixture(vec![Ok(collected()), Ok(mask.clone())], Ok(())).await;
        assert_eq!((reads, reloads), (2, 1));
        assert!(fenced(true, &result.unwrap()));

        // A repeated not-found cannot trigger another reload or become success.
        let (result, reads, reloads) =
            refresh_fixture(vec![Ok(collected()), Ok(collected())], Ok(())).await;
        assert_eq!((reads, reloads), (2, 1));
        assert!(!fenced(true, &result.unwrap()));

        for (stopped, observed, accepted) in [(false, mask.clone(), true), (true, mask, true)] {
            let (result, reads, reloads) = refresh_fixture(vec![Ok(observed)], Ok(())).await;
            assert_eq!((reads, reloads), (1, 0));
            assert_eq!(fenced(stopped, &result.unwrap()), accepted);
        }
        let mut loaded = collected();
        loaded.load = "loaded".into();
        assert!(!fenced(true, &loaded));
        loaded.fragment = "/run/user/1/systemd/transient/quazonai-mission-fixture.service".into();
        assert!(fenced(true, &loaded));
        assert!(!fenced(false, &loaded));
    }
    #[tokio::test]
    async fn collected_unit_after_failed_stop_still_requires_the_original_fence() {
        let mut mask = collected();
        mask.load = "masked".into();
        let mut loaded = collected();
        loaded.load = "loaded".into();
        loaded.fragment = "/run/user/1/systemd/transient/quazonai-mission-fixture.service".into();
        for (after, accepted) in [(mask, true), (collected(), false), (loaded.clone(), false)] {
            let (result, reads, reloads) =
                refresh_fixture(vec![Ok(collected()), Ok(after)], Ok(())).await;
            assert_eq!((reads, reloads), (2, 1));
            // Stop failure is never itself accepted. Only the original final
            // masked state can satisfy the unchanged fence in this branch.
            assert_eq!(fenced(false, &result.unwrap()), accepted);
        }
        let (result, reads, reloads) = refresh_fixture(vec![Ok(loaded)], Ok(())).await;
        assert_eq!((reads, reloads), (1, 0));
        assert!(!fenced(false, &result.unwrap()));
    }
    #[tokio::test]
    async fn collected_unit_diagnostics_preserve_initial_and_final_closed_states() {
        use std::{collections::VecDeque, future::ready};
        let mut mask = collected();
        mask.load = "masked".into();
        let mut observations = VecDeque::from([Ok(collected()), Ok(mask)]);
        let result =
            refresh_collected_observation(|| ready(observations.pop_front().unwrap()), async {
                Ok(())
            })
            .await
            .unwrap();
        assert!(result.refreshed);
        assert!(matches!(result.before.load, FenceLoad::NotFound));
        assert!(!result.before.fragment_present);
        assert!(matches!(
            fence_state(&result.observed).load,
            FenceLoad::Masked
        ));
        let mut unknown = collected();
        unknown.load = "outside-closed-vocabulary".into();
        unknown.fragment = "/path-that-must-not-be-logged".into();
        let snapshot = fence_state(&unknown);
        assert!(matches!(snapshot.load, FenceLoad::Other));
        assert!(snapshot.fragment_present);
    }
    #[tokio::test]
    async fn collected_transient_refresh_propagates_reload_and_confirmation_failures() {
        let (result, reads, reloads) =
            refresh_fixture(vec![Ok(collected())], Err(NativeFailure::Unavailable)).await;
        assert!(matches!(result, Err(NativeFailure::Unavailable)));
        assert_eq!((reads, reloads), (1, 1));
        for error in [
            NativeFailure::Unavailable,
            NativeFailure::Contract,
            NativeFailure::Correlation,
        ] {
            let (result, reads, reloads) = refresh_fixture(vec![Err(error)], Ok(())).await;
            assert!(matches!(result, Err(actual) if actual == error));
            assert_eq!((reads, reloads), (1, 0));
            let (result, reads, reloads) =
                refresh_fixture(vec![Ok(collected()), Err(error)], Ok(())).await;
            assert!(matches!(result, Err(actual) if actual == error));
            assert_eq!((reads, reloads), (2, 1));
        }
    }
    #[tokio::test]
    async fn collected_transient_refresh_rejects_reactivated_and_populated_observations() {
        for changed in 0..5 {
            let mut revived = collected();
            revived.load = "masked".into();
            match changed {
                0 => revived.job = true,
                1 => revived.main_pid = 42,
                2 => revived.active = "active".into(),
                3 => revived.group = Some("/user.slice/foreign.service".into()),
                4 => {
                    revived.group =
                        Some("/user.slice/user-1.slice/quazonai-mission-fixture.service".into())
                }
                _ => unreachable!(),
            }
            let second = confirmed_fixture(revived, "populated 1\nfrozen 0\n");
            assert!(second.is_err());
            let (result, reads, reloads) =
                refresh_fixture(vec![Ok(collected()), second], Ok(())).await;
            assert!(result.is_err());
            assert_eq!((reads, reloads), (2, 1));
        }
    }
    #[test]
    fn stopped_group_confirmation_keeps_exact_identity_and_empty_events() {
        let full = "quazonai-mission-fixture.service";
        let group = format!("/user.slice/user-1.slice/{full}");
        assert_eq!(
            exact_group(full, &group).unwrap(),
            PathBuf::from(format!("/sys/fs/cgroup{group}"))
        );
        for invalid in [
            "/system.slice/quazonai-mission-fixture.service",
            "/user.slice/../quazonai-mission-fixture.service",
            "/user.slice/another.service",
        ] {
            assert!(exact_group(full, invalid).is_err());
        }
        assert!(unpopulated("populated 0\nfrozen 0\n"));
        for events in ["", "populated 1\n", "populated 10\n", "frozen 0\n"] {
            assert!(!unpopulated(events));
        }
    }
}

#[cfg(all(test, feature = "native-codex"))]
mod native_tests {
    use super::*;
    use std::{
        collections::BTreeSet,
        fmt,
        sync::{Mutex, OnceLock},
    };

    fn owned_units() -> &'static Mutex<BTreeSet<String>> {
        static OWNED: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
        OWNED.get_or_init(|| Mutex::new(BTreeSet::new()))
    }

    /// Only units generated by these native fixtures can emit diagnostics.
    /// Never log environment, native stderr, service commands or other units.
    fn trace_line(name: &str, stage: &str, detail: fmt::Arguments<'_>) -> Option<String> {
        let name = name.strip_suffix(".service").unwrap_or(name);
        if owned_units().lock().is_ok_and(|owned| owned.contains(name)) {
            Some(format!(
                "native_stop_trace unit={name} stage={stage} {detail}"
            ))
        } else {
            None
        }
    }
    pub(super) fn trace(name: &str, stage: &str, detail: fmt::Arguments<'_>) {
        if let Some(line) = trace_line(name, stage, detail) {
            eprintln!("{line}");
        }
    }
    pub(super) fn trace_error(name: &str, stage: &str, result: &Result<()>) {
        if let Err(error) = result {
            trace(name, stage, format_args!("error={error:?}"));
        }
    }
    pub(super) fn trace_show(name: &str, code: Option<i32>, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        let fields = observed_fields(&text);
        trace(
            name,
            "show.properties",
            format_args!("exit_code={code:?} fields={fields:?}"),
        );
    }
    fn observed_fields(text: &str) -> Vec<&str> {
        text.lines()
            .filter(|line| {
                [
                    "LoadState=",
                    "ActiveState=",
                    "ControlGroup=",
                    "InvocationID=",
                    "MainPID=",
                    "Job=",
                    "FragmentPath=",
                ]
                .iter()
                .any(|prefix| line.starts_with(*prefix))
            })
            .collect()
    }
    pub(super) fn trace_install_state(name: &str, code: Option<i32>, bytes: &[u8]) {
        let state = std::str::from_utf8(bytes)
            .ok()
            .map(str::trim)
            .filter(|state| {
                !state.is_empty()
                    && state.len() <= 32
                    && state.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
            });
        trace(
            name,
            "mask.install-state",
            format_args!("exit_code={code:?} state={state:?}"),
        );
    }

    fn name() -> String {
        let name = format!(
            "quazonai-mission-{}-{}-1-{}",
            contracts::Id::new(),
            contracts::Id::new(),
            contracts::Id::new()
        );
        owned_units().lock().unwrap().insert(name.clone());
        name
    }
    async fn unmask_test_unit(name: &str) -> Result<()> {
        let full = unit(name)?;
        let unmask = command(&["unmask", "--", &full]).await?;
        if !unmask.status.success() {
            trace(
                name,
                "cleanup.unmask",
                format_args!("exit_code={:?}", unmask.status.code()),
            );
            return Err(NativeFailure::Unavailable);
        }
        let reload = command(&["daemon-reload"]).await?;
        if !reload.status.success() {
            trace(
                name,
                "cleanup.reload",
                format_args!("exit_code={:?}", reload.status.code()),
            );
            return Err(NativeFailure::Unavailable);
        }
        Ok(())
    }
    fn finish_test(phase: &str, result: Result<()>, cleanup: &[(&str, Result<()>)]) {
        assert!(
            result.is_ok(),
            "native test primary phase={phase} error={result:?}; independent cleanup={cleanup:?}"
        );
        assert!(
            cleanup.iter().all(|(_, outcome)| outcome.is_ok()),
            "native test succeeded; cleanup remains unconfirmed: {cleanup:?}"
        );
    }
    fn confirm_queued_workload_absent(marker_exists: bool, confirmation: Result<()>) -> Result<()> {
        confirmation?;
        if marker_exists {
            return Err(NativeFailure::Correlation);
        }
        Ok(())
    }
    #[test]
    fn queued_completion_requires_absent_workload_and_confirmed_native_fence() {
        assert!(confirm_queued_workload_absent(false, Ok(())).is_ok());
        assert_eq!(
            confirm_queued_workload_absent(true, Ok(())),
            Err(NativeFailure::Correlation)
        );
        for marker in [false, true] {
            assert_eq!(
                confirm_queued_workload_absent(marker, Err(NativeFailure::Unavailable)),
                Err(NativeFailure::Unavailable)
            );
            assert_eq!(
                confirm_queued_workload_absent(marker, Err(NativeFailure::Correlation)),
                Err(NativeFailure::Correlation)
            );
        }
    }
    #[test]
    fn diagnostics_preserve_primary_error_and_only_project_owned_unit_fields() {
        assert!(trace_line(
            "quazonai-mission-not-this-fixture",
            "check",
            format_args!("unavailable")
        )
        .is_none());
        let owned = name();
        let line = trace_line(
            &owned,
            "check",
            format_args!("error={:?}", NativeFailure::Unavailable),
        )
        .unwrap();
        assert!(line.contains(&owned));
        assert!(line.contains("stage=check"));
        let fields = observed_fields(
            "LoadState=loaded\nEnvironment=do-not-log\nMainPID=7\nExecStart=do-not-log\nJob=31\n",
        );
        assert_eq!(fields, ["LoadState=loaded", "MainPID=7", "Job=31"]);
        let panic = std::panic::catch_unwind(|| {
            finish_test(
                "stop-queued-start",
                Err(NativeFailure::Unavailable),
                &[("blocker-stop", Err(NativeFailure::Correlation))],
            );
        })
        .unwrap_err();
        let text = panic.downcast_ref::<String>().unwrap();
        assert!(text.contains("primary phase=stop-queued-start error=Err(Unavailable)"));
        assert!(text.contains("blocker-stop"));
        assert!(text.contains("Err(Correlation)"));
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
        let unmask = unmask_test_unit(&name).await;
        finish_test("late-launcher", result, &[("unmask", unmask)]);
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
        let mut phase = "wait-existing-start";
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
            phase = "stop-existing-start";
            stop_and_confirm(&name).await?;
            phase = "wait-stopped-launcher";
            tokio::time::timeout(Duration::from_secs(5), child.wait())
                .await
                .map_err(|_| NativeFailure::Unavailable)?
                .map_err(|_| NativeFailure::Unavailable)?;
            phase = "confirm-existing-stop-replay";
            stop_and_confirm(&name).await?;
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        let unmask = unmask_test_unit(&name).await;
        finish_test(phase, result, &[("unmask", unmask)]);
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
        let mut phase = "wait-queued-start";
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
            phase = "stop-queued-start";
            stop_and_confirm(&target).await?;
            phase = "wait-queued-launcher";
            let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
                .await
                .map_err(|_| NativeFailure::Unavailable)?
                .map_err(|_| NativeFailure::Unavailable)?;
            let marker_exists = marker.exists();
            // With --pipe, systemd-run reports the service Result rather than
            // proving that its queued start job executed. A cancelled unstarted
            // service can exit zero. Keep the actual exit as fixture evidence.
            trace(
                &target,
                "queued.launcher-outcome",
                format_args!(
                    "exit_code={:?} marker_exists={marker_exists}",
                    status.code()
                ),
            );
            if marker_exists {
                return Err(NativeFailure::Correlation);
            }
            phase = "confirm-queued-stop-replay";
            let confirmation = stop_and_confirm(&target).await;
            confirm_queued_workload_absent(marker.exists(), confirmation)?;
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        let blocker_stop = stop_and_confirm(&blocker).await;
        let mut cleanup = vec![("blocker-stop", blocker_stop)];
        // Preserve the original stop-before-unmask prerequisite. A diagnostic
        // must not release either identity after unconfirmed blocker cleanup.
        if cleanup[0].1.is_ok() {
            cleanup.push(("target-unmask", unmask_test_unit(&target).await));
            if cleanup[1].1.is_ok() {
                cleanup.push(("blocker-unmask", unmask_test_unit(&blocker).await));
            } else {
                trace(
                    &blocker,
                    "cleanup.unmask-not-attempted",
                    format_args!("target_unmask_unconfirmed=true"),
                );
            }
        } else {
            trace(
                &target,
                "cleanup.unmask-not-attempted",
                format_args!("blocker_stop_unconfirmed=true"),
            );
        }
        finish_test(phase, result, &cleanup);
    }
}
