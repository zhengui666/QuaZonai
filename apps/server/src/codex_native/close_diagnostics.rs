//! Failure-only shutdown diagnostics. No resource names, paths or external text.
use super::NativeFailure;

#[derive(Clone, Copy)]
pub(super) enum Phase {
    BarrierIdentity,
    BarrierMask,
    BarrierReload,
    BarrierVerifyMask,
    StopCommand,
    StopObserve,
    StopVerifyMask,
    StopNotTerminal,
    StopCgroupIdentity,
    StopCgroupRead,
    StopCgroupPopulated,
    StopReloadCollected,
    StopNotFenced,
    HostBarrier,
    HostFreeze,
    HostCpuSample,
    HostCpuCheckpoint,
    HostKill,
    HostStop,
    HostClosedCheckpoint,
    ScopeKill,
    ScopeRelease,
    ClientGroup,
    ClientContainerBeforeShutdown,
    ClientContainerAfterShutdown,
    ClientChildMissing,
    ClientChildWait,
    ClientChildKill,
    AccountCheckpointTimeout,
    AccountCheckpointRejected,
}

impl Phase {
    fn code(self) -> &'static str {
        match self {
            Self::BarrierIdentity => "barrier.identity",
            Self::BarrierMask => "barrier.mask",
            Self::BarrierReload => "barrier.reload",
            Self::BarrierVerifyMask => "barrier.verify-mask",
            Self::StopCommand => "stop.command",
            Self::StopObserve => "stop.observe",
            Self::StopVerifyMask => "stop.verify-mask",
            Self::StopNotTerminal => "stop.not-terminal",
            Self::StopCgroupIdentity => "stop.cgroup-identity",
            Self::StopCgroupRead => "stop.cgroup-read",
            Self::StopCgroupPopulated => "stop.cgroup-populated",
            Self::StopReloadCollected => "stop.reload-collected",
            Self::StopNotFenced => "stop.not-fenced",
            Self::HostBarrier => "host.barrier",
            Self::HostFreeze => "host.freeze",
            Self::HostCpuSample => "host.cpu-sample",
            Self::HostCpuCheckpoint => "host.cpu-checkpoint",
            Self::HostKill => "host.kill",
            Self::HostStop => "host.stop",
            Self::HostClosedCheckpoint => "host.closed-checkpoint",
            Self::ScopeKill => "scope.kill",
            Self::ScopeRelease => "scope.release",
            Self::ClientGroup => "client.group",
            Self::ClientContainerBeforeShutdown => "client.container-before-shutdown",
            Self::ClientContainerAfterShutdown => "client.container-after-shutdown",
            Self::ClientChildMissing => "client.child-missing",
            Self::ClientChildWait => "client.child-wait",
            Self::ClientChildKill => "client.child-kill",
            Self::AccountCheckpointTimeout => "account.checkpoint-timeout",
            Self::AccountCheckpointRejected => "account.checkpoint-rejected",
        }
    }
}

fn class(error: NativeFailure) -> &'static str {
    match error {
        NativeFailure::Configuration => "Configuration",
        NativeFailure::Version => "Version",
        NativeFailure::Unavailable => "Unavailable",
        NativeFailure::Closed => "Closed",
        NativeFailure::Contract => "Contract",
        NativeFailure::Correlation => "Correlation",
        NativeFailure::FrameLimit => "FrameLimit",
        NativeFailure::ObservationLimit => "ObservationLimit",
        NativeFailure::Rejected(_) => "Rejected",
        NativeFailure::ModelUnavailable => "ModelUnavailable",
        NativeFailure::ProfileInstructions => "ProfileInstructions",
        NativeFailure::CpuBudgetExceeded => "CpuBudgetExceeded",
        NativeFailure::ReconciliationOnly => "ReconciliationOnly",
        NativeFailure::UnboundedMissionLifecycleUnavailable => {
            "UnboundedMissionLifecycleUnavailable"
        }
    }
}

/// Both fields are closed static vocabularies; even RPC rejection payloads are
/// excluded. Return the original error so callers retain their failure semantics.
pub(super) fn failure(phase: Phase, error: NativeFailure) -> NativeFailure {
    tracing::warn!(
        target: "quazonai::native_shutdown",
        phase = phase.code(),
        failure_class = class(error),
    );
    error
}

// Diagnostic classification only; the caller's original populated-zero
// predicate remains authoritative. Duplicate keys take precedence over values.
fn events_class(events: &str) -> &'static str {
    if events.is_empty() {
        return "empty";
    }
    let mut populated = events
        .lines()
        .filter(|line| line.split_ascii_whitespace().next() == Some("populated"));
    let first = populated.next();
    if populated.next().is_some() {
        return "duplicate";
    }
    match first {
        None => "missing",
        Some("populated 0") => "zero",
        Some("populated 1") => "one",
        Some(_) => "invalid",
    }
}

fn errno_class(error: &std::io::Error) -> &'static str {
    #[cfg(target_os = "linux")]
    match error.raw_os_error() {
        Some(libc::ENOENT) => return "not-found",
        Some(libc::ENODEV) => return "no-device",
        Some(libc::EACCES) => return "access-denied",
        Some(libc::EPERM) => return "not-permitted",
        Some(libc::EIO) => return "io",
        Some(libc::EINVAL) => return "invalid",
        _ => (),
    }
    match error.raw_os_error() {
        Some(_) => "other-os",
        None => "no-os",
    }
}

/// Failure-only projection of the existing read and observation. No rereads or
/// external text; zero bytes with "unread" means no byte length was observed.
pub(super) fn cgroup_failure(
    events: std::result::Result<&str, &std::io::Error>,
    stop_code: Option<i32>,
    observed: &super::service::Observation,
) -> NativeFailure {
    let (phase, events_class, events_bytes, errno_class) = match events {
        Ok(events) => (
            Phase::StopCgroupPopulated,
            events_class(events),
            events.len(),
            "none",
        ),
        Err(error) => (Phase::StopCgroupRead, "unread", 0, errno_class(error)),
    };
    tracing::warn!(
        target: "quazonai::native_shutdown",
        phase = phase.code(),
        failure_class = class(NativeFailure::Unavailable),
        events_class,
        events_bytes,
        errno_class,
        stop_class = match stop_code {
            Some(0) => "success",
            Some(_) => "failure",
            None => "no-code",
        },
        load_class = match observed.load.as_str() {
            "masked" => FenceLoad::Masked,
            "loaded" => FenceLoad::Loaded,
            "not-found" => FenceLoad::NotFound,
            _ => FenceLoad::Other,
        }.code(),
        active_class = match observed.active.as_str() {
            "inactive" => "inactive",
            "failed" => "failed",
            _ => "other",
        },
        main_pid_zero = observed.main_pid == 0,
        job_present = observed.job,
        group_present = observed.group.is_some(),
    );
    NativeFailure::Unavailable
}

/// Closed classification only: unrecognized native strings become Other and are
/// never copied into the event. This type carries no unit or resource identity.
#[derive(Clone, Copy)]
pub(super) enum FenceLoad {
    Masked,
    Loaded,
    NotFound,
    Other,
}

impl FenceLoad {
    fn code(self) -> &'static str {
        match self {
            Self::Masked => "masked",
            Self::Loaded => "loaded",
            Self::NotFound => "not-found",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct FenceState {
    pub load: FenceLoad,
    pub fragment_present: bool,
}

/// Called only after the original final fence predicate failed. These snapshots
/// diagnose its operands; they neither authorize a stop nor create fence proof.
pub(super) fn not_fenced(
    stop_succeeded: bool,
    refreshed: bool,
    before: FenceState,
    after: FenceState,
) -> NativeFailure {
    tracing::warn!(
        target: "quazonai::native_shutdown",
        phase = Phase::StopNotFenced.code(),
        failure_class = class(NativeFailure::Unavailable),
        stop_succeeded,
        refreshed,
        before_load = before.load.code(),
        before_fragment_present = before.fragment_present,
        after_load = after.load.code(),
        after_fragment_present = after.fragment_present,
    );
    NativeFailure::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };
    use tracing::{
        field::{Field, Visit},
        Event, Subscriber,
    };
    use tracing_subscriber::{
        layer::{Context, SubscriberExt},
        Layer,
    };

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<BTreeMap<String, String>>>>);

    struct Fields(BTreeMap<String, String>);
    impl Visit for Fields {
        fn record_str(&mut self, field: &Field, value: &str) {
            self.0.insert(field.name().into(), value.into());
        }
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0.insert(field.name().into(), format!("{value:?}"));
        }
    }
    impl<S: Subscriber> Layer<S> for Capture {
        fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
            assert_eq!(event.metadata().target(), "quazonai::native_shutdown");
            let mut fields = Fields(BTreeMap::new());
            event.record(&mut fields);
            self.0.lock().unwrap().push(fields.0);
        }
    }

    fn cgroup_event(
        input: std::result::Result<&str, &std::io::Error>,
        stop_code: Option<i32>,
        load: &str,
        active: &str,
        state: (u32, bool, bool),
    ) -> BTreeMap<String, String> {
        let (main_pid, job, group_present) = state;
        let observed = super::super::service::Observation {
            load: load.into(),
            active: active.into(),
            group: group_present.then(|| "/private-group/quazonai-mission-secret.service".into()),
            invocation: "private-invocation".into(),
            main_pid,
            job,
            fragment: "/private-fragment".into(),
        };
        let capture = Capture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        tracing::subscriber::with_default(subscriber, || {
            assert_eq!(
                cgroup_failure(input, stop_code, &observed),
                NativeFailure::Unavailable
            );
        });
        let mut events = capture.0.lock().unwrap();
        assert_eq!(events.len(), 1);
        let event = events.pop().unwrap();
        assert_eq!(
            event.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "active_class",
                "errno_class",
                "events_bytes",
                "events_class",
                "failure_class",
                "group_present",
                "job_present",
                "load_class",
                "main_pid_zero",
                "phase",
                "stop_class",
            ]
        );
        assert_eq!(event["failure_class"], "Unavailable");
        for secret in [
            "private-group",
            "quazonai-mission-secret",
            "private-invocation",
            "123456789",
            "private-fragment",
            "private-error",
            "private-key",
            "private-load",
            "private-active",
        ] {
            assert!(!event.values().any(|value| value.contains(secret)));
        }
        event
    }

    #[test]
    fn cgroup_events_diagnostics_classify_only_existing_bytes() {
        for (input, expected) in [
            ("", "empty"),
            ("populated 0\nfrozen 1\n", "zero"),
            ("populated 1\nfrozen 0\n", "one"),
            ("frozen 0\nprivate-key private-value\n", "missing"),
            ("\n", "missing"),
            ("populated 1\npopulated 1\n", "duplicate"),
            ("populated 0\npopulated 1\n", "duplicate"),
            ("populated 2\npopulated 0\n", "duplicate"),
            ("populated\n", "invalid"),
            ("populated 10\n", "invalid"),
            (" populated 1\n", "invalid"),
            ("populated\t1\n", "invalid"),
            ("populated 1 extra\n", "invalid"),
            ("private-key 值\npopulated 1\n", "one"),
        ] {
            let event = cgroup_event(Ok(input), Some(0), "masked", "inactive", (0, false, false));
            assert_eq!(event["phase"], "stop.cgroup-populated");
            assert_eq!(event["events_class"], expected, "{input:?}");
            assert_eq!(event["events_bytes"], input.len().to_string());
            assert_eq!(event["errno_class"], "none");
        }
    }

    #[test]
    fn cgroup_read_diagnostics_keep_errors_distinct_from_events() {
        let error = std::io::Error::other("private-error /private-group");
        let event = cgroup_event(Err(&error), None, "not-found", "failed", (0, false, false));
        assert_eq!(event["phase"], "stop.cgroup-read");
        assert_eq!(event["errno_class"], "no-os");
        assert_eq!(event["events_class"], "unread");
        assert_eq!(event["events_bytes"], "0");

        #[cfg(target_os = "linux")]
        for (errno, expected) in [
            (libc::ENOENT, "not-found"),
            (libc::ENODEV, "no-device"),
            (libc::EACCES, "access-denied"),
            (libc::EPERM, "not-permitted"),
            (libc::EIO, "io"),
            (libc::EINVAL, "invalid"),
            (libc::EBUSY, "other-os"),
        ] {
            let error = std::io::Error::from_raw_os_error(errno);
            let event = cgroup_event(
                Err(&error),
                Some(1),
                "loaded",
                "inactive",
                (123456789, true, true),
            );
            assert_eq!(event["phase"], "stop.cgroup-read");
            assert_eq!(event["errno_class"], expected);
            assert_eq!(event["events_class"], "unread");
            assert_eq!(event["events_bytes"], "0");
        }
    }

    #[test]
    fn cgroup_context_diagnostics_use_closed_states_and_booleans() {
        for (stop_code, stop_class) in [
            (Some(0), "success"),
            (Some(17), "failure"),
            (Some(-1), "failure"),
            (None, "no-code"),
        ] {
            for (load, load_class) in [
                ("masked", "masked"),
                ("loaded", "loaded"),
                ("not-found", "not-found"),
                ("private-load", "other"),
            ] {
                for (active, active_class) in [
                    ("inactive", "inactive"),
                    ("failed", "failed"),
                    ("private-active", "other"),
                ] {
                    for (main_pid, job, group_present) in [
                        (0, false, false),
                        (123456789, false, false),
                        (0, true, false),
                        (0, false, true),
                        (123456789, true, true),
                    ] {
                        let event = cgroup_event(
                            Ok("populated 1\n"),
                            stop_code,
                            load,
                            active,
                            (main_pid, job, group_present),
                        );
                        assert_eq!(event["stop_class"], stop_class);
                        assert_eq!(event["load_class"], load_class);
                        assert_eq!(event["active_class"], active_class);
                        assert_eq!(event["main_pid_zero"], (main_pid == 0).to_string());
                        assert_eq!(event["job_present"], job.to_string());
                        assert_eq!(event["group_present"], group_present.to_string());
                    }
                }
            }
        }
    }

    #[test]
    fn failed_fence_records_only_closed_states_and_booleans() {
        let loads = [
            FenceLoad::Masked,
            FenceLoad::Loaded,
            FenceLoad::NotFound,
            FenceLoad::Other,
        ];
        let capture = Capture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        tracing::subscriber::with_default(subscriber, || {
            for load in loads {
                assert_eq!(
                    not_fenced(
                        false,
                        true,
                        FenceState {
                            load: FenceLoad::NotFound,
                            fragment_present: false
                        },
                        FenceState {
                            load,
                            fragment_present: true
                        },
                    ),
                    NativeFailure::Unavailable
                );
            }
        });
        let events = capture.0.lock().unwrap();
        assert_eq!(events.len(), loads.len());
        for (event, load) in events.iter().zip(loads) {
            assert_eq!(
                event.keys().map(String::as_str).collect::<Vec<_>>(),
                [
                    "after_fragment_present",
                    "after_load",
                    "before_fragment_present",
                    "before_load",
                    "failure_class",
                    "phase",
                    "refreshed",
                    "stop_succeeded"
                ]
            );
            assert_eq!(event["phase"], "stop.not-fenced");
            assert_eq!(event["failure_class"], "Unavailable");
            assert_eq!(event["stop_succeeded"], "false");
            assert_eq!(event["refreshed"], "true");
            assert_eq!(event["before_load"], "not-found");
            assert_eq!(event["before_fragment_present"], "false");
            assert_eq!(event["after_load"], load.code());
            assert_eq!(event["after_fragment_present"], "true");
        }
    }

    #[test]
    fn shutdown_failure_fields_are_whitelisted_and_original_errors_are_preserved() {
        let errors = [
            NativeFailure::Configuration,
            NativeFailure::Version,
            NativeFailure::Unavailable,
            NativeFailure::Closed,
            NativeFailure::Contract,
            NativeFailure::Correlation,
            NativeFailure::FrameLimit,
            NativeFailure::ObservationLimit,
            NativeFailure::Rejected(9223372036854775806),
            NativeFailure::ModelUnavailable,
            NativeFailure::ProfileInstructions,
            NativeFailure::CpuBudgetExceeded,
            NativeFailure::ReconciliationOnly,
            NativeFailure::UnboundedMissionLifecycleUnavailable,
        ];
        let capture = Capture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        tracing::subscriber::with_default(subscriber, || {
            for error in errors {
                assert_eq!(failure(Phase::HostClosedCheckpoint, error), error);
            }
        });
        let events = capture.0.lock().unwrap();
        assert_eq!(events.len(), errors.len());
        for (event, error) in events.iter().zip(errors) {
            assert_eq!(
                event.keys().map(String::as_str).collect::<Vec<_>>(),
                ["failure_class", "phase"]
            );
            assert_eq!(event["phase"], "host.closed-checkpoint");
            assert_eq!(event["failure_class"], class(error));
            assert!(!event
                .values()
                .any(|value| value.contains("9223372036854775806")));
        }
    }

    #[test]
    fn shutdown_phase_names_are_fixed_unique_and_do_not_carry_resource_identity() {
        use Phase::*;
        let phases = [
            BarrierIdentity,
            BarrierMask,
            BarrierReload,
            BarrierVerifyMask,
            StopCommand,
            StopObserve,
            StopVerifyMask,
            StopNotTerminal,
            StopCgroupIdentity,
            StopCgroupRead,
            StopCgroupPopulated,
            StopReloadCollected,
            StopNotFenced,
            HostBarrier,
            HostFreeze,
            HostCpuSample,
            HostCpuCheckpoint,
            HostKill,
            HostStop,
            HostClosedCheckpoint,
            ScopeKill,
            ScopeRelease,
            ClientGroup,
            ClientContainerBeforeShutdown,
            ClientContainerAfterShutdown,
            ClientChildMissing,
            ClientChildWait,
            ClientChildKill,
            AccountCheckpointTimeout,
            AccountCheckpointRejected,
        ];
        let names = phases.map(Phase::code);
        assert_eq!(
            names
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            names.len()
        );
        assert!(names.iter().all(|name| name.len() <= 40
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'.' || b == b'-')));
    }
}
