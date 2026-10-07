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
