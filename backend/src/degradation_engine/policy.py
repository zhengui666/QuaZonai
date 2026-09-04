"""Pure, deterministic degradation-state and research-wake evaluation."""

from __future__ import annotations

from dataclasses import replace

from .contracts import (
    DegradationObservation,
    DegradationPolicy,
    EvaluationResult,
    ForwardEvidence,
    HealthSnapshot,
    HealthState,
    ProgramState,
    ScheduledWake,
    SubjectType,
    WakeDisposition,
    WakeRequest,
)


_ACTIVE_DEGRADATION = frozenset({HealthState.DEGRADING, HealthState.FAILED})
_AUTO_EXECUTION_READY = frozenset(
    {
        ProgramState.ACTIVE,
        ProgramState.COOLING,
        ProgramState.WAITING_FOR_FEEDBACK,
    }
)


def _reason_code(subject_type: SubjectType) -> str:
    return f"{subject_type.value}_DEGRADATION"


def has_active_degradation(state: HealthState) -> bool:
    return state in _ACTIVE_DEGRADATION


def allows_auto_live_promotion(state: HealthState) -> bool:
    return not has_active_degradation(state)


def _next_snapshot(
    evidence: ForwardEvidence,
    policy: DegradationPolicy,
    previous: HealthSnapshot,
) -> tuple[HealthSnapshot, bool]:
    if evidence.confidence < policy.minimum_confidence:
        return previous, False

    breaches = (
        previous.consecutive_breaches + 1
        if evidence.severity >= policy.degrading_threshold
        else 0
    )
    if evidence.severity <= policy.recovery_threshold and has_active_degradation(previous.state):
        return HealthSnapshot(HealthState.RECOVERED), True
    if breaches >= policy.minimum_consecutive_breaches:
        state = (
            HealthState.FAILED
            if evidence.severity >= policy.failed_threshold
            else HealthState.DEGRADING
        )
        return HealthSnapshot(state, breaches), True
    if has_active_degradation(previous.state):
        return HealthSnapshot(previous.state, breaches), True
    state = (
        HealthState.WATCH
        if evidence.severity >= policy.watch_threshold
        else HealthState.HEALTHY
    )
    return HealthSnapshot(state, breaches), True


def evaluate_degradation(
    evidence: ForwardEvidence,
    policy: DegradationPolicy,
    previous: HealthSnapshot = HealthSnapshot(),
) -> EvaluationResult:
    """Evaluate one source episode and emit a wake only on an active-state crossing."""
    snapshot, evaluated = _next_snapshot(evidence, policy, previous)
    reason_code = _reason_code(evidence.subject_type)
    observation = DegradationObservation(
        source_id=evidence.source_id,
        subject_type=evidence.subject_type,
        subject_id=evidence.subject_id,
        metric_name=evidence.metric_name,
        severity=evidence.severity,
        confidence=evidence.confidence,
        policy_revision=policy.policy_revision,
        reason_code=reason_code,
        state=snapshot.state,
        consecutive_breaches=snapshot.consecutive_breaches,
        evaluated=evaluated,
    )
    wake_request = None
    if not has_active_degradation(previous.state) and has_active_degradation(snapshot.state):
        wake_request = WakeRequest(
            program_id=evidence.program_id,
            subject_type=evidence.subject_type,
            subject_id=evidence.subject_id,
            source_id=evidence.source_id,
            policy_revision=policy.policy_revision,
            reason_code=reason_code,
        )
    return EvaluationResult(snapshot.state, snapshot, observation, wake_request)


def auto_execution_allowed(program_state: ProgramState) -> bool:
    """Only ordinary lifecycle states may automatically restart bounded research."""
    return program_state in _AUTO_EXECUTION_READY


def schedule_wake(request: WakeRequest, program_state: ProgramState) -> ScheduledWake:
    disposition = (
        WakeDisposition.READY if auto_execution_allowed(program_state) else WakeDisposition.PENDING
    )
    return ScheduledWake(request, disposition)


def reconcile_wake(wake: ScheduledWake, program_state: ProgramState) -> ScheduledWake:
    """Retain held wakes until an allowed lifecycle transition releases them."""
    disposition = (
        WakeDisposition.READY if auto_execution_allowed(program_state) else WakeDisposition.PENDING
    )
    return replace(wake, disposition=disposition)
