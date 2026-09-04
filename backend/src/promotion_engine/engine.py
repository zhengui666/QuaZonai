"""Fail-closed Paper-to-Live Promotion Policy evaluation."""

from __future__ import annotations

from collections.abc import Iterable
from math import isfinite

from .contracts import (
    GateComparator,
    GateEvaluation,
    GateRequirement,
    GateStatus,
    LivePromotionMode,
    MetricObservation,
    PromotionAction,
    PromotionActionIdentity,
    PromotionDecision,
    PromotionOutcome,
    PromotionPolicy,
    PromotionPurpose,
    PromotionRequest,
)


def evaluate_policy_gates(
    policy: PromotionPolicy, metrics: Iterable[MetricObservation]
) -> tuple[GateEvaluation, ...]:
    """Evaluate required metric thresholds without inventing unavailable evidence."""
    observations = tuple(metrics)
    values = {observation.name: observation.value for observation in observations}
    if len(values) != len(observations):
        raise ValueError("metric names must be unique")
    return tuple(_evaluate_metric_gate(gate, values.get(gate.metric)) for gate in policy.gates)


def _evaluate_metric_gate(gate: GateRequirement, actual: float | None) -> GateEvaluation:
    if actual is None:
        return GateEvaluation(
            gate=gate.name,
            actual=None,
            expected=gate.expected,
            status=GateStatus.INCONCLUSIVE,
            reason_code="MISSING_REQUIRED_METRIC",
        )
    if not isfinite(actual):
        return GateEvaluation(
            gate=gate.name,
            actual=actual,
            expected=gate.expected,
            status=GateStatus.INVALID,
            reason_code="METRIC_NOT_FINITE",
        )
    passed = (
        actual >= gate.threshold
        if gate.comparator is GateComparator.MINIMUM
        else actual <= gate.threshold
    )
    if passed:
        return GateEvaluation(gate.name, actual, gate.expected, GateStatus.PASS)
    reason = "MINIMUM_NOT_MET" if gate.comparator is GateComparator.MINIMUM else "MAXIMUM_EXCEEDED"
    return GateEvaluation(gate.name, actual, gate.expected, GateStatus.FAIL, reason)


def _boolean_gate(
    gate: str,
    actual: bool,
    *,
    status_when_false: GateStatus,
    reason_code: str,
) -> GateEvaluation:
    return GateEvaluation(
        gate=gate,
        actual=actual,
        expected="true",
        status=GateStatus.PASS if actual else status_when_false,
        reason_code=None if actual else reason_code,
    )


def _prerequisite_gates(request: PromotionRequest) -> tuple[GateEvaluation, ...]:
    feedback = request.feedback
    readiness = request.readiness
    if not feedback.complete:
        feedback_gates = (
            _boolean_gate(
                "paper_feedback_complete",
                False,
                status_when_false=GateStatus.INCONCLUSIVE,
                reason_code="PAPER_FEEDBACK_INCOMPLETE",
            ),
            _boolean_gate(
                "paper_feedback_contract_valid",
                False,
                status_when_false=GateStatus.INCONCLUSIVE,
                reason_code="PAPER_FEEDBACK_INCOMPLETE",
            ),
            _boolean_gate(
                "paper_data_quality_valid",
                False,
                status_when_false=GateStatus.INCONCLUSIVE,
                reason_code="PAPER_FEEDBACK_INCOMPLETE",
            ),
        )
    else:
        feedback_gates = (
            _boolean_gate(
                "paper_feedback_complete",
                True,
                status_when_false=GateStatus.INCONCLUSIVE,
                reason_code="PAPER_FEEDBACK_INCOMPLETE",
            ),
            _boolean_gate(
                "paper_feedback_contract_valid",
                feedback.contract_valid,
                status_when_false=GateStatus.FAIL,
                reason_code="PAPER_FEEDBACK_INVALID",
            ),
            _boolean_gate(
                "paper_data_quality_valid",
                feedback.data_quality_valid,
                status_when_false=GateStatus.FAIL,
                reason_code="PAPER_DATA_QUALITY_INVALID",
            ),
        )
    return feedback_gates + (
        _boolean_gate(
            "candidate_current",
            readiness.candidate_current,
            status_when_false=GateStatus.FAIL,
            reason_code="CANDIDATE_NOT_CURRENT",
        ),
        _boolean_gate(
            "candidate_package_current",
            readiness.candidate_package_current,
            status_when_false=GateStatus.FAIL,
            reason_code="CANDIDATE_PACKAGE_NOT_CURRENT",
        ),
        _boolean_gate(
            "promotion_policy_current",
            readiness.promotion_policy_current,
            status_when_false=GateStatus.FAIL,
            reason_code="PROMOTION_POLICY_NOT_CURRENT",
        ),
        _boolean_gate(
            "dataset_revisions_current",
            readiness.dataset_revisions_current,
            status_when_false=GateStatus.FAIL,
            reason_code="DATASET_REVISIONS_NOT_CURRENT",
        ),
        _boolean_gate(
            "runtime_current",
            readiness.runtime_current,
            status_when_false=GateStatus.FAIL,
            reason_code="RUNTIME_NOT_CURRENT",
        ),
        _boolean_gate(
            "live_downstream_ready",
            readiness.live_downstream_ready,
            status_when_false=GateStatus.FAIL,
            reason_code="LIVE_DOWNSTREAM_NOT_READY",
        ),
        _boolean_gate(
            "no_active_degradation",
            not readiness.active_degradation,
            status_when_false=GateStatus.FAIL,
            reason_code="ACTIVE_DEGRADATION",
        ),
    )


def evaluate_paper_to_live(policy: PromotionPolicy, request: PromotionRequest) -> PromotionDecision:
    """Return the single deterministic action allowed by frozen Paper evidence."""
    if policy.purpose is not PromotionPurpose.PAPER_TO_LIVE:
        raise ValueError("policy purpose must be PAPER_TO_LIVE")
    if policy.identity != request.binding.promotion_policy:
        raise ValueError("policy identity must match the frozen promotion binding")

    gates = _prerequisite_gates(request) + evaluate_policy_gates(policy, request.metrics)
    statuses = {gate.status for gate in gates}
    if GateStatus.FAIL in statuses or GateStatus.INVALID in statuses:
        return PromotionDecision(PromotionOutcome.REJECTED, PromotionAction.NONE, gates)
    if GateStatus.INCONCLUSIVE in statuses:
        return PromotionDecision(PromotionOutcome.WAITING_FOR_EVIDENCE, PromotionAction.NONE, gates)

    feedback_identity = request.feedback.identity
    assert feedback_identity is not None
    action_identity = PromotionActionIdentity(request.binding, feedback_identity)
    if policy.live_promotion_mode is LivePromotionMode.MANUAL_APPROVAL:
        return PromotionDecision(
            PromotionOutcome.MANUAL_LIVE_APPROVAL_READY,
            PromotionAction.CREATE_PENDING_LIVE_APPROVAL,
            gates,
            action_identity,
        )
    return PromotionDecision(
        PromotionOutcome.AUTO_LIVE_HANDOFF_AVAILABLE,
        PromotionAction.CREATE_SYSTEM_APPROVED_LIVE_HANDOFF,
        gates,
        action_identity,
    )
