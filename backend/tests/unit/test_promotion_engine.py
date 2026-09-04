from __future__ import annotations

from dataclasses import replace
from math import nan

import pytest

from promotion_engine import (
    GateComparator,
    GateStatus,
    GateRequirement,
    LivePromotionMode,
    MetricObservation,
    PaperFeedback,
    PromotionAction,
    PromotionBinding,
    PromotionOutcome,
    PromotionPolicy,
    PromotionPurpose,
    PromotionReadiness,
    PromotionRequest,
    RevisionRef,
    evaluate_paper_to_live,
)


def _ref(name: str, revision: int = 1) -> RevisionRef:
    return RevisionRef(name, revision)


def _policy(mode: LivePromotionMode) -> PromotionPolicy:
    return PromotionPolicy(
        identity=_ref("promotion-policy", 3),
        purpose=PromotionPurpose.PAPER_TO_LIVE,
        live_promotion_mode=mode,
        gates=(
            GateRequirement("minimum_net_return", "net_return", GateComparator.MINIMUM, 0.02),
            GateRequirement(
                "maximum_realized_drawdown", "realized_drawdown", GateComparator.MAXIMUM, 0.1
            ),
        ),
    )


def _readiness(**overrides: bool) -> PromotionReadiness:
    values = {
        "candidate_current": True,
        "candidate_package_current": True,
        "promotion_policy_current": True,
        "dataset_revisions_current": True,
        "runtime_current": True,
        "live_downstream_ready": True,
        "active_degradation": False,
    }
    values.update(overrides)
    return PromotionReadiness(**values)


def _request(**overrides: object) -> PromotionRequest:
    values: dict[str, object] = {
        "binding": PromotionBinding(
            candidate_id="candidate",
            candidate_package=_ref("candidate-package", 2),
            promotion_policy=_ref("promotion-policy", 3),
            runtime=_ref("runtime", 7),
            downstream=_ref("live-downstream", 4),
            dataset_revisions=(_ref("paper-dataset", 6),),
        ),
        "feedback": PaperFeedback(_ref("paper-feedback", 9), True, True, True),
        "readiness": _readiness(),
        "metrics": (
            MetricObservation("net_return", 0.03),
            MetricObservation("realized_drawdown", 0.08),
        ),
    }
    values.update(overrides)
    return PromotionRequest(**values)  # type: ignore[arg-type]


def test_binding_uses_candidate_id_without_a_synthetic_revision() -> None:
    binding = _request().binding

    assert binding.candidate_id == "candidate"
    assert binding.candidate_package == _ref("candidate-package", 2)
    assert not hasattr(binding, "candidate")


def test_manual_policy_creates_only_a_pending_live_approval_action() -> None:
    decision = evaluate_paper_to_live(_policy(LivePromotionMode.MANUAL_APPROVAL), _request())

    assert decision.outcome is PromotionOutcome.MANUAL_LIVE_APPROVAL_READY
    assert decision.action is PromotionAction.CREATE_PENDING_LIVE_APPROVAL
    assert decision.action_identity is not None
    assert all(gate.status is GateStatus.PASS for gate in decision.gates)


def test_auto_policy_is_deterministic_and_creates_the_system_handoff_action() -> None:
    policy = _policy(LivePromotionMode.AUTO_HANDOFF)
    request = _request()

    first = evaluate_paper_to_live(policy, request)
    second = evaluate_paper_to_live(policy, request)

    assert first == second
    assert first.outcome is PromotionOutcome.AUTO_LIVE_HANDOFF_AVAILABLE
    assert first.action is PromotionAction.CREATE_SYSTEM_APPROVED_LIVE_HANDOFF
    assert first.action_identity is not None
    assert first.action_identity.binding == request.binding
    assert first.action_identity.feedback == _ref("paper-feedback", 9)


def test_missing_required_metric_waits_for_evidence() -> None:
    decision = evaluate_paper_to_live(
        _policy(LivePromotionMode.AUTO_HANDOFF),
        _request(metrics=(MetricObservation("net_return", 0.03),)),
    )

    missing = next(gate for gate in decision.gates if gate.gate == "maximum_realized_drawdown")
    assert missing.status is GateStatus.INCONCLUSIVE
    assert missing.reason_code == "MISSING_REQUIRED_METRIC"
    assert decision.outcome is PromotionOutcome.WAITING_FOR_EVIDENCE
    assert decision.action is PromotionAction.NONE


@pytest.mark.parametrize(
    ("promotion_request", "reason_code"),
    [
        (
            _request(feedback=PaperFeedback(_ref("paper-feedback", 9), True, False, True)),
            "PAPER_FEEDBACK_INVALID",
        ),
        (
            _request(readiness=_readiness(live_downstream_ready=False)),
            "LIVE_DOWNSTREAM_NOT_READY",
        ),
        (
            _request(readiness=_readiness(candidate_current=False)),
            "CANDIDATE_NOT_CURRENT",
        ),
        (
            _request(readiness=_readiness(candidate_package_current=False)),
            "CANDIDATE_PACKAGE_NOT_CURRENT",
        ),
        (
            _request(readiness=_readiness(promotion_policy_current=False)),
            "PROMOTION_POLICY_NOT_CURRENT",
        ),
        (
            _request(readiness=_readiness(dataset_revisions_current=False)),
            "DATASET_REVISIONS_NOT_CURRENT",
        ),
        (
            _request(readiness=_readiness(runtime_current=False)),
            "RUNTIME_NOT_CURRENT",
        ),
        (
            _request(readiness=_readiness(active_degradation=True)),
            "ACTIVE_DEGRADATION",
        ),
    ],
)
def test_invalid_or_stale_prerequisites_never_promote(
    promotion_request: PromotionRequest, reason_code: str
) -> None:
    decision = evaluate_paper_to_live(_policy(LivePromotionMode.AUTO_HANDOFF), promotion_request)

    assert decision.outcome is PromotionOutcome.REJECTED
    assert decision.action is PromotionAction.NONE
    assert reason_code in decision.reason_codes


def test_policy_revision_must_match_the_frozen_binding() -> None:
    request = _request()
    mismatched = replace(
        request,
        binding=replace(request.binding, promotion_policy=_ref("promotion-policy", 4)),
    )

    with pytest.raises(ValueError, match="frozen promotion binding"):
        evaluate_paper_to_live(_policy(LivePromotionMode.AUTO_HANDOFF), mismatched)


def test_nonfinite_metric_is_invalid_not_a_passing_value() -> None:
    decision = evaluate_paper_to_live(
        _policy(LivePromotionMode.AUTO_HANDOFF),
        _request(
            metrics=(
                MetricObservation("net_return", nan),
                MetricObservation("realized_drawdown", 0.08),
            )
        ),
    )

    assert (
        next(gate for gate in decision.gates if gate.gate == "minimum_net_return").status
        is GateStatus.INVALID
    )
    assert decision.outcome is PromotionOutcome.REJECTED
