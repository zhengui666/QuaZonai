from __future__ import annotations

from degradation_engine import (
    DegradationPolicy,
    ForwardEvidence,
    HealthState,
    ProgramState,
    SubjectType,
    WakeDisposition,
    allows_auto_live_promotion,
    evaluate_degradation,
    reconcile_wake,
    schedule_wake,
)


def _evidence(
    source_id: str,
    severity: float,
    *,
    subject_type: SubjectType = SubjectType.ALPHA,
) -> ForwardEvidence:
    return ForwardEvidence(
        source_id=source_id,
        program_id="program-1",
        subject_type=subject_type,
        subject_id=f"{subject_type.value.lower()}-1",
        metric_name="net_sharpe_decay",
        severity=severity,
    )


def test_consistent_alpha_evidence_crosses_once_and_emits_one_wake() -> None:
    policy = DegradationPolicy("degradation-v1", minimum_consecutive_breaches=2)
    healthy = evaluate_degradation(_evidence("episode-1", 0.10), policy)
    watch = evaluate_degradation(_evidence("episode-2", 0.30), policy, healthy.snapshot)
    first_breach = evaluate_degradation(_evidence("episode-3", 0.60), policy, watch.snapshot)
    crossing = evaluate_degradation(_evidence("episode-4", 0.60), policy, first_breach.snapshot)
    repeated = evaluate_degradation(_evidence("episode-5", 0.60), policy, crossing.snapshot)

    assert healthy.state is HealthState.HEALTHY
    assert watch.state is HealthState.WATCH
    assert first_breach.state is HealthState.WATCH
    assert crossing.state is HealthState.DEGRADING
    assert crossing.wake_request is not None
    assert crossing.wake_request.source_id == "episode-4"
    assert crossing.wake_request.policy_revision == "degradation-v1"
    assert crossing.wake_request.reason_code == "ALPHA_DEGRADATION"
    assert crossing.wake_request.deduplication_fields == (
        "program-1",
        "ALPHA",
        "alpha-1",
        "episode-4",
        "degradation-v1",
        "ALPHA_DEGRADATION",
    )
    assert repeated.wake_request is None


def test_portfolio_failure_recovers_and_reallows_auto_live_promotion() -> None:
    policy = DegradationPolicy("degradation-v3")
    failed = evaluate_degradation(
        _evidence("episode-10", 0.90, subject_type=SubjectType.PORTFOLIO), policy
    )
    recovered = evaluate_degradation(
        _evidence("episode-11", 0.05, subject_type=SubjectType.PORTFOLIO),
        policy,
        failed.snapshot,
    )

    assert failed.state is HealthState.FAILED
    assert failed.wake_request is not None
    assert failed.wake_request.reason_code == "PORTFOLIO_DEGRADATION"
    assert not allows_auto_live_promotion(failed.state)
    assert recovered.state is HealthState.RECOVERED
    assert recovered.wake_request is None
    assert allows_auto_live_promotion(recovered.state)


def test_only_ordinary_lifecycle_states_auto_release_a_wake() -> None:
    policy = DegradationPolicy("degradation-v1")
    result = evaluate_degradation(_evidence("episode-20", 0.60), policy)
    assert result.wake_request is not None

    paused = schedule_wake(result.wake_request, ProgramState.PAUSED)
    archived = schedule_wake(result.wake_request, ProgramState.ARCHIVED)
    blocked = schedule_wake(result.wake_request, ProgramState.BLOCKED)
    approval_pending = schedule_wake(result.wake_request, ProgramState.APPROVAL_PENDING)
    cooling = schedule_wake(result.wake_request, ProgramState.COOLING)
    waiting = schedule_wake(result.wake_request, ProgramState.WAITING_FOR_FEEDBACK)
    resumed = reconcile_wake(paused, ProgramState.ACTIVE)

    assert paused.disposition is WakeDisposition.PENDING
    assert archived.disposition is WakeDisposition.PENDING
    assert blocked.disposition is WakeDisposition.PENDING
    assert approval_pending.disposition is WakeDisposition.PENDING
    assert cooling.disposition is WakeDisposition.READY
    assert waiting.disposition is WakeDisposition.READY
    assert resumed.disposition is WakeDisposition.READY
    assert resumed.request == result.wake_request


def test_low_confidence_evidence_cannot_clear_active_degradation() -> None:
    policy = DegradationPolicy("degradation-v1")
    degraded = evaluate_degradation(_evidence("episode-30", 0.60), policy)
    inconclusive = evaluate_degradation(
        ForwardEvidence(
            source_id="episode-31",
            program_id="program-1",
            subject_type=SubjectType.ALPHA,
            subject_id="alpha-1",
            metric_name="net_sharpe_decay",
            severity=0.0,
            confidence=0.50,
        ),
        policy,
        degraded.snapshot,
    )

    assert inconclusive.state is HealthState.DEGRADING
    assert not inconclusive.observation.evaluated
    assert inconclusive.wake_request is None
