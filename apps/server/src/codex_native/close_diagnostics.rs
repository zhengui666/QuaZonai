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
