"""Trusted Promotion writers.

Only the Core can create Promotion Evaluation and Approval facts.  This module
accepts frozen database identities; it never resolves a current policy,
downstream, receipt, candidate or package.
"""

from __future__ import annotations

from collections.abc import Iterable
from datetime import UTC, datetime, timedelta
from decimal import Decimal
from dataclasses import dataclass
from typing import cast
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from candidate_packages import is_trusted_candidate_package
from db.models import (
    ApprovalSnapshot,
    CandidatePackage,
    DownstreamConnectionVersion,
    DownstreamSystem,
    DegradationObservation,
    FeedbackContractVersion,
    FeedbackContractMetricRequirement,
    FeedbackPackage,
    ForwardEvidenceEpisode,
    ForwardEvidenceMetric,
    HandoffOffer,
    Job,
    PortfolioEvaluationAssignment,
    PortfolioEvaluationDisclosure,
    PortfolioEvaluationEpisode,
    PortfolioEvaluationMetric,
    PortfolioCandidate,
    PreflightReceipt,
    PromotionEvaluation,
    PromotionGateResult,
    PromotionPolicyGate,
    PromotionPolicyVersion,
)
from errors import QfError
from events import append_event
from jobs import enqueue_job


_PROMOTION_POLICY_CONTRACT = "PROMOTION_POLICY_V1"
_PROMOTION_EVALUATOR_CONTRACT = "PORTFOLIO_EVALUATION_V1"


@dataclass(frozen=True, slots=True)
class TypedFeedbackMetric:
    """One scalar feedback fact accepted by the frozen Paper contract."""

    metric_code: str
    status: str
    value: Decimal | None = None


@dataclass(frozen=True, slots=True)
class FeedbackHeader:
    observation_start: datetime
    observation_end: datetime
    sample_size: int


def _conflict(code: str, message: str) -> QfError:
    return QfError(code, message, 409)


def _paper_tuple(policy: PromotionPolicyVersion) -> tuple[UUID | None, ...]:
    return (
        policy.paper_downstream_system_id,
        policy.paper_connection_version_id,
        policy.paper_feedback_contract_version_id,
        policy.paper_preflight_receipt_id,
    )


def _require_receipt(
    session: Session,
    *,
    receipt_id: UUID,
    connection: DownstreamConnectionVersion,
    downstream: DownstreamSystem,
    package_contract_version: str,
    environment_type: str = "PAPER",
) -> PreflightReceipt:
    receipt = session.scalar(
        select(PreflightReceipt)
        .where(PreflightReceipt.id == receipt_id)
        .with_for_update()
    )
    now = datetime.now(UTC)
    valid_until = receipt.valid_until if receipt is not None else None
    if valid_until is not None and valid_until.tzinfo is None:
        valid_until = valid_until.replace(tzinfo=UTC)
    if (
        receipt is None
        or receipt.resource_type != "DOWNSTREAM_CONNECTION_VERSION"
        or receipt.resource_id != connection.id
        or receipt.resource_revision != connection.version_no
        or receipt.status != "READY"
        or valid_until is None
        or valid_until <= now
        or receipt.contract_version != connection.package_contract_version
        or connection.state != "ACTIVE"
        or downstream.id != connection.downstream_system_id
        or not downstream.enabled
        or downstream.environment_type != environment_type
        or connection.package_contract_version != package_contract_version
    ):
        raise _conflict(
            "PROMOTION_PREFLIGHT_STALE",
            "The frozen Paper preflight receipt is not valid for its connection and package.",
        )
    return receipt


def validate_typed_paper_approval(
    session: Session, approval: ApprovalSnapshot, package: CandidatePackage | None = None
) -> tuple[DownstreamSystem, DownstreamConnectionVersion, FeedbackContractVersion, PreflightReceipt, PromotionPolicyVersion]:
    """Lock and validate a typed Paper Approval's complete frozen binding."""
    if (
        approval.promotion_evaluation_id is None
        or approval.promotion_purpose != "PORTFOLIO_TO_PAPER"
        or approval.purpose != "PAPER"
        or approval.candidate_package_id is None
        or approval.candidate_package_revision is None
    ):
        raise _conflict("APPROVAL_TYPED_LINEAGE_REQUIRED", "Only a typed Paper Approval is executable.")
    if package is None:
        package = session.scalar(
            select(CandidatePackage)
            .where(CandidatePackage.id == approval.candidate_package_id)
            .with_for_update()
        )
    if (
        package is None
        or package.candidate_id != approval.candidate_id
        or package.revision != approval.candidate_package_revision
        or not is_trusted_candidate_package(session, package)
    ):
        raise _conflict("CANDIDATE_PACKAGE_STALE", "Approval Package is not a trusted available Package.")
    evaluation = session.scalar(
        select(PromotionEvaluation)
        .where(PromotionEvaluation.id == approval.promotion_evaluation_id)
        .with_for_update()
    )
    if (
        evaluation is None
        or evaluation.purpose != "PORTFOLIO_TO_PAPER"
        or evaluation.outcome != "PASS"
        or evaluation.action != "MANUAL_APPROVAL"
        or evaluation.candidate_id != approval.candidate_id
        or evaluation.candidate_package_id != package.id
        or evaluation.package_revision != package.revision
        or evaluation.paper_to_live_policy_version_id != approval.paper_to_live_policy_version_id
        or evaluation.downstream_system_id != approval.downstream_system_id
        or evaluation.downstream_connection_version_id != approval.downstream_connection_version_id
        or evaluation.feedback_contract_version_id != approval.feedback_contract_version_id
        or evaluation.preflight_receipt_id != approval.preflight_receipt_id
    ):
        raise _conflict("APPROVAL_TYPED_LINEAGE_INVALID", "Approval does not copy its Promotion Evaluation.")
    policy = session.scalar(
        select(PromotionPolicyVersion)
        .where(PromotionPolicyVersion.id == evaluation.policy_version_id)
        .with_for_update()
    )
    if policy is None:
        raise _conflict("PROMOTION_POLICY_INVALID", "Approval Promotion Policy is missing.")
    _, downstream, connection, contract, receipt = _strict_p2p_policy(session, policy)
    if (
        approval.downstream_system_id != downstream.id
        or approval.downstream_connection_version_id != connection.id
        or approval.feedback_contract_version_id != contract.id
        or approval.preflight_receipt_id != receipt.id
    ):
        raise _conflict("APPROVAL_TYPED_LINEAGE_INVALID", "Approval downstream binding is not frozen.")
    return downstream, connection, contract, receipt, policy


def approve_typed_paper_handoff(session: Session, approval_id: UUID) -> HandoffOffer:
    """Approve a Core-created Paper Snapshot without accepting a new target."""
    approval = session.scalar(
        select(ApprovalSnapshot).where(ApprovalSnapshot.id == approval_id).with_for_update()
    )
    if approval is None:
        raise QfError("APPROVAL_NOT_FOUND", "Approval Snapshot was not found.", 404)
    if approval.state != "PENDING":
        raise _conflict("APPROVAL_STATE_CONFLICT", "Only a pending typed Approval can be approved.")
    downstream, connection, contract, receipt, _policy = validate_typed_paper_approval(session, approval)
    existing = session.scalar(
        select(HandoffOffer).where(HandoffOffer.approval_id == approval.id).with_for_update()
    )
    if existing is not None:
        return existing
    handoff = HandoffOffer(
        id=uuid4(),
        approval_id=approval.id,
        candidate_package_id=approval.candidate_package_id,
        candidate_package_revision=approval.candidate_package_revision,
        candidate_id=approval.candidate_id,
        promotion_purpose="PORTFOLIO_TO_PAPER",
        purpose="PAPER",
        downstream_system_id=downstream.id,
        downstream_connection_version_id=connection.id,
        feedback_contract_version_id=contract.id,
        preflight_receipt_id=receipt.id,
        paper_to_live_policy_version_id=approval.paper_to_live_policy_version_id,
        state="AVAILABLE",
        claim_deadline=datetime.now(UTC) + timedelta(days=7),
        feedback_state="PENDING",
        feedback_contract_snapshot={"feedback_contract_version_id": str(contract.id)},
    )
    session.add(handoff)
    approval.state = "APPROVED"
    approval.revision += 1
    session.flush()
    append_event(
        session,
        kind="PORTFOLIO_TO_PAPER_HANDOFF_AVAILABLE",
        aggregate_type="HANDOFF",
        aggregate_id=handoff.id,
        payload={"approval_id": str(approval.id), "candidate_id": str(approval.candidate_id)},
    )
    return handoff


def approve_typed_live_handoff(session: Session, approval_id: UUID) -> HandoffOffer:
    """Approve a Core-created manual Live Snapshot without changing its target."""
    approval = session.scalar(
        select(ApprovalSnapshot).where(ApprovalSnapshot.id == approval_id).with_for_update()
    )
    if approval is None:
        raise QfError("APPROVAL_NOT_FOUND", "Approval Snapshot was not found.", 404)
    if approval.state != "PENDING":
        raise _conflict("APPROVAL_STATE_CONFLICT", "Only a pending typed Approval can be approved.")
    if (
        approval.promotion_evaluation_id is None
        or approval.promotion_purpose != "PAPER_TO_LIVE"
        or approval.purpose != "LIVE"
        or approval.candidate_package_id is None
        or approval.candidate_package_revision is None
    ):
        raise _conflict("APPROVAL_TYPED_LINEAGE_REQUIRED", "Only a typed Live Approval is executable.")
    package = session.scalar(
        select(CandidatePackage)
        .where(CandidatePackage.id == approval.candidate_package_id)
        .with_for_update()
    )
    evaluation = session.scalar(
        select(PromotionEvaluation)
        .where(PromotionEvaluation.id == approval.promotion_evaluation_id)
        .with_for_update()
    )
    if (
        package is None
        or package.candidate_id != approval.candidate_id
        or package.revision != approval.candidate_package_revision
        or not is_trusted_candidate_package(session, package)
        or evaluation is None
        or evaluation.purpose != "PAPER_TO_LIVE"
        or evaluation.outcome != "PASS"
        or evaluation.action != "MANUAL_APPROVAL"
        or evaluation.candidate_id != approval.candidate_id
        or evaluation.candidate_package_id != package.id
        or evaluation.package_revision != package.revision
        or evaluation.downstream_system_id != approval.downstream_system_id
        or evaluation.downstream_connection_version_id != approval.downstream_connection_version_id
        or evaluation.feedback_contract_version_id != approval.feedback_contract_version_id
        or evaluation.preflight_receipt_id != approval.preflight_receipt_id
    ):
        raise _conflict("APPROVAL_TYPED_LINEAGE_INVALID", "Approval does not copy its Live Promotion Evaluation.")
    policy, downstream, connection, contract, receipt = _strict_p2l_policy(
        session, evaluation.policy_version_id, package.contract_version
    )
    if policy.mode != "MANUAL_APPROVAL" or (
        approval.downstream_system_id,
        approval.downstream_connection_version_id,
        approval.feedback_contract_version_id,
        approval.preflight_receipt_id,
    ) != (downstream.id, connection.id, contract.id, receipt.id):
        raise _conflict("APPROVAL_TYPED_LINEAGE_INVALID", "Live Approval binding is not frozen.")
    existing = session.scalar(
        select(HandoffOffer).where(HandoffOffer.approval_id == approval.id).with_for_update()
    )
    if existing is not None:
        return existing
    handoff = HandoffOffer(
        id=uuid4(),
        approval_id=approval.id,
        candidate_package_id=approval.candidate_package_id,
        candidate_package_revision=approval.candidate_package_revision,
        candidate_id=approval.candidate_id,
        promotion_purpose="PAPER_TO_LIVE",
        purpose="LIVE",
        downstream_system_id=downstream.id,
        downstream_connection_version_id=connection.id,
        feedback_contract_version_id=contract.id,
        preflight_receipt_id=receipt.id,
        paper_to_live_policy_version_id=None,
        state="AVAILABLE",
        claim_deadline=datetime.now(UTC) + timedelta(days=7),
        feedback_state="PENDING",
        feedback_contract_snapshot={"feedback_contract_version_id": str(contract.id)},
    )
    session.add(handoff)
    approval.state = "APPROVED"
    approval.revision += 1
    session.flush()
    append_event(
        session,
        kind="PAPER_TO_LIVE_HANDOFF_AVAILABLE",
        aggregate_type="HANDOFF",
        aggregate_id=handoff.id,
        payload={"approval_id": str(approval.id), "candidate_id": str(approval.candidate_id)},
    )
    return handoff


def _strict_p2p_policy(
    session: Session, policy: PromotionPolicyVersion
) -> tuple[PromotionPolicyVersion, DownstreamSystem, DownstreamConnectionVersion, FeedbackContractVersion, PreflightReceipt]:
    if (
        policy.policy_contract_version != _PROMOTION_POLICY_CONTRACT
        or policy.purpose != "PORTFOLIO_TO_PAPER"
        or policy.mode != "MANUAL_APPROVAL"
        or policy.state != "ACTIVE"
        or any(value is None for value in _paper_tuple(policy))
        or any(
            value is not None
            for value in (
                policy.live_downstream_system_id,
                policy.live_connection_version_id,
                policy.live_feedback_contract_version_id,
                policy.live_preflight_receipt_id,
            )
        )
        or policy.paper_to_live_policy_version_id is None
    ):
        raise _conflict("PROMOTION_POLICY_INVALID", "The frozen Paper policy is not a typed V1 policy.")
    target = session.scalar(
        select(PromotionPolicyVersion)
        .where(PromotionPolicyVersion.id == policy.paper_to_live_policy_version_id)
        .with_for_update()
    )
    if (
        target is None
        or target.policy_contract_version != _PROMOTION_POLICY_CONTRACT
        or target.purpose != "PAPER_TO_LIVE"
        or target.state != "ACTIVE"
        or target.paper_to_live_policy_version_id is not None
        or _paper_tuple(target) != _paper_tuple(policy)
        or any(
            value is None
            for value in (
                target.live_downstream_system_id,
                target.live_connection_version_id,
                target.live_feedback_contract_version_id,
                target.live_preflight_receipt_id,
            )
        )
    ):
        raise _conflict(
            "PROMOTION_POLICY_LINEAGE_INVALID",
            "The Paper policy does not freeze one matching Paper-to-Live policy.",
        )
    gates = list(
        session.scalars(
            select(PromotionPolicyGate)
            .where(PromotionPolicyGate.policy_version_id == policy.id)
            .with_for_update()
        )
    )
    if not gates or not any(
        gate.metric_code == "MATERIAL_IMPROVEMENT"
        and gate.comparator == "MINIMUM"
        and gate.threshold.is_finite()
        and gate.threshold <= 0
        for gate in gates
    ):
        raise _conflict(
            "PROMOTION_POLICY_INVALID",
            "The Paper policy must require a non-positive material-improvement minimum.",
        )
    paper_downstream = session.scalar(
        select(DownstreamSystem)
        .where(DownstreamSystem.id == policy.paper_downstream_system_id)
        .with_for_update()
    )
    connection = session.scalar(
        select(DownstreamConnectionVersion)
        .where(DownstreamConnectionVersion.id == policy.paper_connection_version_id)
        .with_for_update()
    )
    contract = session.scalar(
        select(FeedbackContractVersion)
        .where(FeedbackContractVersion.id == policy.paper_feedback_contract_version_id)
        .with_for_update()
    )
    if paper_downstream is None or connection is None or contract is None:
        raise _conflict("PROMOTION_POLICY_LINEAGE_INVALID", "Paper policy dependencies are missing.")
    if (
        connection.downstream_system_id != paper_downstream.id
        or connection.feedback_contract_version_id != contract.id
        or contract.downstream_system_id != paper_downstream.id
        or contract.purpose != "PAPER"
        or contract.state != "ACTIVE"
    ):
        raise _conflict("PROMOTION_POLICY_LINEAGE_INVALID", "Paper policy dependencies do not match.")
    receipt = _require_receipt(
        session,
        receipt_id=cast(UUID, policy.paper_preflight_receipt_id),
        connection=connection,
        downstream=paper_downstream,
        package_contract_version=connection.package_contract_version,
    )
    return policy, paper_downstream, connection, contract, receipt


def _metric_value(metric: PortfolioEvaluationMetric) -> Decimal | None:
    if metric.value is None:
        return None
    value = Decimal(str(metric.value))
    return value if value.is_finite() else None


def _promotion_gate(
    gate: PromotionPolicyGate,
    metric: PortfolioEvaluationMetric | None,
) -> PromotionGateResult:
    actual = _metric_value(metric) if metric is not None and metric.status == "AVAILABLE" else None
    expected = Decimal(str(gate.threshold))
    passed = actual is not None and (
        actual >= expected if gate.comparator == "MINIMUM" else actual <= expected
    )
    return PromotionGateResult(
        evaluation_id=UUID(int=0),
        gate_code=gate.metric_code,
        status="PASS" if passed else "FAIL",
        actual=actual,
        expected=expected,
        reason_code=None if passed else "PROMOTION_METRIC_GATE_FAILED",
    )


def _existing_p2p(session: Session, episode_id: UUID) -> PromotionEvaluation | None:
    return session.scalar(
        select(PromotionEvaluation)
        .where(
            PromotionEvaluation.purpose == "PORTFOLIO_TO_PAPER",
            PromotionEvaluation.portfolio_evaluation_episode_id == episode_id,
        )
        .with_for_update()
    )


def maybe_enqueue_p2p(
    session: Session, *, portfolio_evaluation_episode_id: UUID
) -> PromotionEvaluation | None:
    """Create the idempotent typed P2P decision and Paper Approval when eligible.

    Missing Package is deliberately a no-op: the Package worker calls this same
    function after AVAILABLE, so a fast Portfolio evaluator cannot terminalize
    a still-building Candidate.
    """
    episode = session.scalar(
        select(PortfolioEvaluationEpisode)
        .where(PortfolioEvaluationEpisode.id == portfolio_evaluation_episode_id)
        .with_for_update()
    )
    if episode is None or episode.state != "DISCLOSED" or episode.result != "PASS":
        return None
    # Serialize retries on the source episode before checking the unique
    # decision. This keeps Package and evaluator completions idempotent.
    existing = _existing_p2p(session, episode.id)
    if existing is not None:
        return existing
    assignment = session.scalar(
        select(PortfolioEvaluationAssignment)
        .where(PortfolioEvaluationAssignment.id == episode.assignment_id)
        .with_for_update()
    )
    disclosure = session.get(PortfolioEvaluationDisclosure, episode.id)
    if (
        assignment is None
        or assignment.state != "FINALIZED"
        or assignment.outcome != "PASS"
        or assignment.evaluator_contract_version != _PROMOTION_EVALUATOR_CONTRACT
        or disclosure is None
        or disclosure.classification != "QUALIFIED"
    ):
        return None
    # The episode lock serializes decisions. Package state only moves
    # BUILDING -> AVAILABLE, so avoid locking it here while package finalization
    # holds that row before acquiring the episode lock.
    packages = list(
        session.scalars(
            select(CandidatePackage)
            .where(
                CandidatePackage.candidate_id == assignment.candidate_id,
                CandidatePackage.state == "AVAILABLE",
            )
        )
    )
    if len(packages) > 1:
        raise _conflict(
            "CANDIDATE_PACKAGE_AMBIGUOUS",
            "Promotion requires exactly one trusted available Candidate Package.",
        )
    package = packages[0] if packages else None
    if package is None or not is_trusted_candidate_package(session, package):
        return None
    policy = session.scalar(
        select(PromotionPolicyVersion)
        .where(PromotionPolicyVersion.id == assignment.promotion_policy_version_id)
        .with_for_update()
    )
    if policy is None:
        raise _conflict("PROMOTION_POLICY_INVALID", "Portfolio evaluation policy is missing.")
    _, downstream, connection, contract, receipt = _strict_p2p_policy(session, policy)
    metrics = {
        metric.metric_code: metric
        for metric in session.scalars(
            select(PortfolioEvaluationMetric)
            .where(PortfolioEvaluationMetric.episode_id == episode.id)
            .with_for_update()
        )
    }
    p2p_gates = list(
        session.scalars(
            select(PromotionPolicyGate)
            .where(PromotionPolicyGate.policy_version_id == policy.id)
            .order_by(PromotionPolicyGate.ordinal)
            .with_for_update()
        )
    )
    if not p2p_gates:
        raise _conflict("PROMOTION_POLICY_INVALID", "Portfolio-to-Paper policy has no gates.")
    gate_results = [_promotion_gate(gate, metrics.get(gate.metric_code)) for gate in p2p_gates]
    all_pass = all(row.status == "PASS" for row in gate_results)
    evaluation = PromotionEvaluation(
        id=uuid4(),
        purpose="PORTFOLIO_TO_PAPER",
        portfolio_evaluation_episode_id=episode.id,
        forward_evidence_episode_id=None,
        candidate_id=assignment.candidate_id,
        candidate_package_id=package.id,
        package_revision=package.revision,
        policy_version_id=policy.id,
        paper_to_live_policy_version_id=policy.paper_to_live_policy_version_id,
        downstream_system_id=downstream.id,
        downstream_connection_version_id=connection.id,
        feedback_contract_version_id=contract.id,
        preflight_receipt_id=receipt.id,
        outcome="PASS" if all_pass else "FAIL",
        action="MANUAL_APPROVAL" if all_pass else "NO_ACTION",
    )
    session.add(evaluation)
    session.flush()
    for row in gate_results:
        row.evaluation_id = evaluation.id
    session.add_all(gate_results)
    if all_pass:
        session.add(
            ApprovalSnapshot(
                id=uuid4(),
                promotion_evaluation_id=evaluation.id,
                promotion_purpose="PORTFOLIO_TO_PAPER",
                candidate_id=assignment.candidate_id,
                candidate_package_id=package.id,
                candidate_package_revision=package.revision,
                purpose="PAPER",
                state="PENDING",
                downstream_system_id=downstream.id,
                downstream_connection_version_id=connection.id,
                feedback_contract_version_id=contract.id,
                preflight_receipt_id=receipt.id,
                paper_to_live_policy_version_id=policy.paper_to_live_policy_version_id,
                valid_until=receipt.valid_until,
                expires_at=receipt.valid_until,
                human_report={},
                evidence_summary={},
                capital_context={},
                risk_summary={},
                cost_summary={},
                capacity_summary={},
                changes_summary={},
            )
        )
    session.flush()
    append_event(
        session,
        kind="PORTFOLIO_TO_PAPER_PROMOTION_DECIDED",
        aggregate_type="PROMOTION_EVALUATION",
        aggregate_id=evaluation.id,
        payload={
            "candidate_id": str(assignment.candidate_id),
            "outcome": evaluation.outcome,
            "action": evaluation.action,
        },
    )
    if all_pass:
        append_event(
            session,
            kind="PORTFOLIO_TO_PAPER_PROMOTION_READY",
            aggregate_type="PROMOTION_EVALUATION",
            aggregate_id=evaluation.id,
            payload={"candidate_id": str(assignment.candidate_id), "approval": "PENDING"},
        )
    session.flush()
    return evaluation


def maybe_enqueue_p2p_for_candidate(session: Session, candidate_id: UUID) -> PromotionEvaluation | None:
    """Retry the same P2P writer after a Package becomes AVAILABLE."""
    episode = session.scalar(
        select(PortfolioEvaluationEpisode)
        .join(PortfolioEvaluationAssignment, PortfolioEvaluationAssignment.id == PortfolioEvaluationEpisode.assignment_id)
        .where(
            PortfolioEvaluationAssignment.candidate_id == candidate_id,
            PortfolioEvaluationEpisode.state == "DISCLOSED",
            PortfolioEvaluationEpisode.result == "PASS",
        )
        .with_for_update()
    )
    return maybe_enqueue_p2p(
        session, portfolio_evaluation_episode_id=episode.id
    ) if episode is not None else None


def enqueue_p2p_promotion_job(
    session: Session, *, portfolio_evaluation_episode_id: UUID
) -> Job | None:
    """Queue the sole P2P worker after both typed evaluation and Package exist."""
    episode = session.scalar(
        select(PortfolioEvaluationEpisode)
        .where(PortfolioEvaluationEpisode.id == portfolio_evaluation_episode_id)
        .with_for_update()
    )
    if episode is None or episode.state != "DISCLOSED" or episode.result != "PASS":
        return None
    existing = _existing_p2p(session, episode.id)
    if existing is not None:
        return None
    assignment = session.scalar(
        select(PortfolioEvaluationAssignment)
        .where(PortfolioEvaluationAssignment.id == episode.assignment_id)
        .with_for_update()
    )
    if assignment is None or assignment.state != "FINALIZED" or assignment.outcome != "PASS":
        return None
    # Keep the same episode-first order as the worker; package finalization may
    # already hold its immutable row while it queues this job.
    packages = list(
        session.scalars(
            select(CandidatePackage)
            .where(
                CandidatePackage.candidate_id == assignment.candidate_id,
                CandidatePackage.state == "AVAILABLE",
            )
        )
    )
    if len(packages) > 1:
        raise _conflict(
            "CANDIDATE_PACKAGE_AMBIGUOUS",
            "Promotion requires exactly one trusted available Candidate Package.",
        )
    if not packages or not is_trusted_candidate_package(session, packages[0]):
        return None
    active_jobs = list(
        session.scalars(
            select(Job).where(
                Job.kind == "PORTFOLIO_TO_PAPER_PROMOTION",
                Job.resource_type == "portfolio_evaluation_episode",
                Job.resource_id == episode.id,
                Job.state.in_(("READY", "LEASED")),
            )
        )
    )
    if len(active_jobs) > 1 or (active_jobs and active_jobs[0].payload != {}):
        raise _conflict(
            "PORTFOLIO_TO_PAPER_JOB_CONFLICT",
            "Portfolio-to-Paper job is invalid.",
        )
    if active_jobs:
        return active_jobs[0]
    return enqueue_job(
        session,
        kind="PORTFOLIO_TO_PAPER_PROMOTION",
        resource_type="portfolio_evaluation_episode",
        resource_id=episode.id,
        payload={},
    )


def enqueue_p2p_promotion_job_for_candidate(
    session: Session, candidate_id: UUID
) -> Job | None:
    episode = session.scalar(
        select(PortfolioEvaluationEpisode)
        .join(
            PortfolioEvaluationAssignment,
            PortfolioEvaluationAssignment.id == PortfolioEvaluationEpisode.assignment_id,
        )
        .where(
            PortfolioEvaluationAssignment.candidate_id == candidate_id,
            PortfolioEvaluationEpisode.state == "DISCLOSED",
            PortfolioEvaluationEpisode.result == "PASS",
        )
        .with_for_update()
    )
    return (
        enqueue_p2p_promotion_job(session, portfolio_evaluation_episode_id=episode.id)
        if episode is not None
        else None
    )


def _utc(value: datetime) -> datetime:
    if value.tzinfo is None or value.utcoffset() != UTC.utcoffset(value):
        raise _conflict("FEEDBACK_CONTRACT_INVALID", "Feedback timestamps must be UTC.")
    return value.astimezone(UTC)


def _stored_utc(value: datetime) -> datetime:
    """Normalize timezone values returned by SQLite's timezone-less adapter."""
    if value.tzinfo is None:
        return value.replace(tzinfo=UTC)
    return value.astimezone(UTC)


def _typed_feedback_rows(
    session: Session, contract: FeedbackContractVersion, metrics: Iterable[TypedFeedbackMetric]
) -> tuple[TypedFeedbackMetric, ...]:
    requirements = list(
        session.scalars(
            select(FeedbackContractMetricRequirement)
            .where(FeedbackContractMetricRequirement.feedback_contract_version_id == contract.id)
            .order_by(FeedbackContractMetricRequirement.ordinal)
            .with_for_update()
        )
    )
    if not requirements or [row.ordinal for row in requirements] != list(range(1, len(requirements) + 1)):
        raise _conflict("FEEDBACK_CONTRACT_INVALID", "Feedback contract metric requirements are incomplete.")
    provided = tuple(metrics)
    if any(not isinstance(metric, TypedFeedbackMetric) for metric in provided):
        raise QfError("FEEDBACK_CONTRACT_INVALID", "Feedback requires typed scalar metrics.", 422)
    if tuple(metric.metric_code for metric in provided) != tuple(row.metric_code for row in requirements):
        raise _conflict("FEEDBACK_CONTRACT_INVALID", "Feedback metrics must exactly match the frozen contract.")
    for metric in provided:
        if metric.status not in {"AVAILABLE", "NOT_AVAILABLE"}:
            raise QfError("FEEDBACK_CONTRACT_INVALID", "Feedback metric status is invalid.", 422)
        if metric.status == "AVAILABLE":
            if metric.value is None or not metric.value.is_finite():
                raise QfError("FEEDBACK_CONTRACT_INVALID", "Available feedback values must be finite.", 422)
        elif metric.value is not None:
            raise QfError("FEEDBACK_CONTRACT_INVALID", "Unavailable feedback values must be null.", 422)
    return provided


def _typed_paper_handoff(
    session: Session, handoff_id: UUID
) -> tuple[HandoffOffer, ApprovalSnapshot, PromotionEvaluation, CandidatePackage, FeedbackContractVersion]:
    handoff = session.scalar(
        select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()
    )
    if (
        handoff is None
        or handoff.promotion_purpose != "PORTFOLIO_TO_PAPER"
        or handoff.purpose != "PAPER"
        or handoff.candidate_package_revision is None
        or handoff.downstream_connection_version_id is None
        or handoff.feedback_contract_version_id is None
        or handoff.preflight_receipt_id is None
        or handoff.paper_to_live_policy_version_id is None
    ):
        raise _conflict("HANDOFF_TYPED_LINEAGE_REQUIRED", "Only a typed Paper Handoff accepts production feedback.")
    approval = session.scalar(
        select(ApprovalSnapshot).where(ApprovalSnapshot.id == handoff.approval_id).with_for_update()
    )
    if approval is None or approval.state != "APPROVED" or approval.promotion_evaluation_id is None:
        raise _conflict("HANDOFF_TYPED_LINEAGE_REQUIRED", "Paper Handoff Approval is not typed.")
    evaluation = session.scalar(
        select(PromotionEvaluation)
        .where(PromotionEvaluation.id == approval.promotion_evaluation_id)
        .with_for_update()
    )
    package = session.scalar(
        select(CandidatePackage)
        .where(CandidatePackage.id == handoff.candidate_package_id)
        .with_for_update()
    )
    contract = session.scalar(
        select(FeedbackContractVersion)
        .where(FeedbackContractVersion.id == handoff.feedback_contract_version_id)
        .with_for_update()
    )
    if (
        evaluation is None
        or evaluation.purpose != "PORTFOLIO_TO_PAPER"
        or evaluation.outcome != "PASS"
        or evaluation.candidate_id != handoff.candidate_id
        or evaluation.candidate_package_id != handoff.candidate_package_id
        or evaluation.package_revision != handoff.candidate_package_revision
        or evaluation.paper_to_live_policy_version_id != handoff.paper_to_live_policy_version_id
        or approval.candidate_id != handoff.candidate_id
        or approval.candidate_package_id != handoff.candidate_package_id
        or approval.candidate_package_revision != handoff.candidate_package_revision
        or approval.downstream_connection_version_id != handoff.downstream_connection_version_id
        or approval.feedback_contract_version_id != handoff.feedback_contract_version_id
        or approval.preflight_receipt_id != handoff.preflight_receipt_id
        or contract is None
        or contract.purpose != "PAPER"
        or package is None
        or not is_trusted_candidate_package(session, package)
    ):
        raise _conflict("HANDOFF_TYPED_LINEAGE_INVALID", "Paper Handoff facts do not share one frozen lineage.")
    return handoff, approval, evaluation, package, contract


def accept_paper_feedback(
    session: Session,
    *,
    handoff_id: UUID,
    header: FeedbackHeader,
    metrics: Iterable[TypedFeedbackMetric],
) -> ForwardEvidenceEpisode:
    """Persist complete typed Paper feedback and enqueue exactly one P2L job."""
    handoff, _approval, _evaluation, _package, contract = _typed_paper_handoff(session, handoff_id)
    if handoff.state not in {"DOWNSTREAM_ACCEPTED", "FEEDBACK_PENDING", "FEEDBACK_IN_PROGRESS", "FEEDBACK_PARTIAL", "FEEDBACK_COMPLETE"}:
        raise _conflict("HANDOFF_STATE_CONFLICT", "Paper Handoff is not accepting complete feedback.")
    start = _utc(header.observation_start)
    end = _utc(header.observation_end)
    if end <= start or header.sample_size < contract.minimum_valid_sample_size:
        raise _conflict("FEEDBACK_CONTRACT_INVALID", "Feedback observation does not satisfy the frozen contract.")
    if (end - start).total_seconds() < contract.minimum_observation_seconds:
        raise _conflict("FEEDBACK_CONTRACT_INVALID", "Feedback observation is shorter than the frozen contract.")
    typed = _typed_feedback_rows(session, contract, metrics)
    existing = session.scalar(
        select(FeedbackPackage)
        .where(FeedbackPackage.handoff_offer_id == handoff.id)
        .with_for_update()
    )
    if existing is not None:
        episode = session.scalar(
            select(ForwardEvidenceEpisode)
            .where(ForwardEvidenceEpisode.feedback_package_id == existing.id)
            .with_for_update()
        )
        if episode is None or existing.state != "COMPLETE":
            raise _conflict("FEEDBACK_CONTRACT_CONFLICT", "Existing Paper feedback is incomplete.")
        persisted_metrics = {
            row.metric_code: (row.status, row.value)
            for row in session.scalars(
                select(ForwardEvidenceMetric)
                .where(ForwardEvidenceMetric.episode_id == episode.id)
                .with_for_update()
            )
        }
        provided_metrics = {
            metric.metric_code: (metric.status, metric.value) for metric in typed
        }
        if (
            _stored_utc(existing.observation_start) != start
            or _stored_utc(existing.observation_end) != end
            or existing.sample_size != header.sample_size
            or persisted_metrics != provided_metrics
        ):
            raise _conflict(
                "FEEDBACK_CONTRACT_CONFLICT",
                "Retrying Paper feedback must exactly match the immutable submission.",
            )
        return episode
    package = FeedbackPackage(
        id=uuid4(),
        handoff_offer_id=handoff.id,
        feedback_contract_version_id=contract.id,
        state="COMPLETE",
        observation_start=start,
        observation_end=end,
        sample_size=header.sample_size,
        summary_json={},
        relative_path=None,
    )
    session.add(package)
    session.flush()
    episode = ForwardEvidenceEpisode(
        id=uuid4(),
        handoff_id=handoff.id,
        feedback_package_id=package.id,
        state="FEEDBACK_COMPLETE",
        evidence={},
        observation_start=start,
        observation_end=end,
        sample_size=header.sample_size,
        created_at=datetime.now(UTC),
    )
    session.add(episode)
    session.flush()
    session.add_all(
        ForwardEvidenceMetric(
            episode_id=episode.id,
            metric_code=metric.metric_code,
            value=metric.value,
            status=metric.status,
        )
        for metric in typed
    )
    handoff.state = "FEEDBACK_COMPLETE"
    handoff.feedback_state = "FEEDBACK_COMPLETE"
    session.flush()
    active_jobs = list(
        session.scalars(
            select(Job).where(
                Job.kind == "PAPER_TO_LIVE_PROMOTION",
                Job.resource_type == "forward_evidence_episode",
                Job.resource_id == episode.id,
                Job.state.in_(('READY', 'LEASED')),
            )
        )
    )
    if len(active_jobs) > 1 or (active_jobs and active_jobs[0].payload != {}):
        raise _conflict("PAPER_TO_LIVE_JOB_CONFLICT", "Paper-to-Live job is invalid.")
    if not active_jobs:
        enqueue_job(
            session,
            kind="PAPER_TO_LIVE_PROMOTION",
            resource_type="forward_evidence_episode",
            resource_id=episode.id,
            payload={},
        )
    append_event(
        session,
        kind="FORWARD_EVIDENCE_RECORDED",
        aggregate_type="HANDOFF",
        aggregate_id=handoff.id,
        payload={"episode_id": str(episode.id), "state": "FEEDBACK_COMPLETE"},
    )
    return episode


def _strict_p2l_policy(
    session: Session, policy_id: UUID, package_contract_version: str
) -> tuple[PromotionPolicyVersion, DownstreamSystem, DownstreamConnectionVersion, FeedbackContractVersion, PreflightReceipt]:
    policy = session.scalar(
        select(PromotionPolicyVersion).where(PromotionPolicyVersion.id == policy_id).with_for_update()
    )
    if (
        policy is None
        or policy.policy_contract_version != _PROMOTION_POLICY_CONTRACT
        or policy.purpose != "PAPER_TO_LIVE"
        or policy.state != "ACTIVE"
        or policy.paper_to_live_policy_version_id is not None
        or any(value is None for value in (policy.paper_downstream_system_id, policy.paper_connection_version_id, policy.paper_feedback_contract_version_id, policy.paper_preflight_receipt_id, policy.live_downstream_system_id, policy.live_connection_version_id, policy.live_feedback_contract_version_id, policy.live_preflight_receipt_id))
    ):
        raise _conflict("PROMOTION_POLICY_INVALID", "The frozen Live policy is not a complete typed V1 policy.")
    downstream = session.scalar(select(DownstreamSystem).where(DownstreamSystem.id == policy.live_downstream_system_id).with_for_update())
    connection = session.scalar(select(DownstreamConnectionVersion).where(DownstreamConnectionVersion.id == policy.live_connection_version_id).with_for_update())
    contract = session.scalar(select(FeedbackContractVersion).where(FeedbackContractVersion.id == policy.live_feedback_contract_version_id).with_for_update())
    if downstream is None or connection is None or contract is None or connection.downstream_system_id != downstream.id or connection.feedback_contract_version_id != contract.id or contract.downstream_system_id != downstream.id or contract.purpose != "LIVE" or contract.state != "ACTIVE":
        raise _conflict("PROMOTION_POLICY_LINEAGE_INVALID", "Live policy dependencies do not match.")
    receipt = _require_receipt(session, receipt_id=cast(UUID, policy.live_preflight_receipt_id), connection=connection, downstream=downstream, package_contract_version=package_contract_version, environment_type="LIVE")
    return policy, downstream, connection, contract, receipt


def maybe_enqueue_p2l(session: Session, *, forward_evidence_episode_id: UUID) -> PromotionEvaluation | None:
    """Evaluate one complete Paper feedback lineage for Live handoff."""
    episode = session.scalar(select(ForwardEvidenceEpisode).where(ForwardEvidenceEpisode.id == forward_evidence_episode_id).with_for_update())
    if episode is None or episode.state != "FEEDBACK_COMPLETE" or episode.feedback_package_id is None:
        return None
    # Lock the immutable Forward Evidence row before the idempotence lookup so
    # duplicate/reclaimed workers converge on one Live decision.
    existing = session.scalar(select(PromotionEvaluation).where(PromotionEvaluation.purpose == "PAPER_TO_LIVE", PromotionEvaluation.forward_evidence_episode_id == episode.id).with_for_update())
    if existing is not None:
        return existing
    handoff, approval, p2p, package, paper_contract = _typed_paper_handoff(session, episode.handoff_id)
    if handoff.state != "FEEDBACK_COMPLETE":
        return None
    policy, downstream, connection, contract, receipt = _strict_p2l_policy(session, cast(UUID, handoff.paper_to_live_policy_version_id), package.contract_version)
    if p2p.paper_to_live_policy_version_id != policy.id:
        raise _conflict("PROMOTION_LINEAGE_INVALID", "Paper-to-Live policy does not match the Paper decision.")
    candidate = session.scalar(select(PortfolioCandidate).where(PortfolioCandidate.id == handoff.candidate_id).with_for_update())
    if candidate is None:
        raise _conflict("PROMOTION_LINEAGE_INVALID", "Promotion Candidate is missing.")
    degrading = session.scalar(select(DegradationObservation.id).where(DegradationObservation.subject_type == "PORTFOLIO", DegradationObservation.subject_id == candidate.id, DegradationObservation.state.in_(("DEGRADING", "FAILED"))).limit(1))
    if degrading is not None:
        return None
    metrics = {row.metric_code: row for row in session.scalars(select(ForwardEvidenceMetric).where(ForwardEvidenceMetric.episode_id == episode.id).with_for_update())}
    requirements = list(session.scalars(select(FeedbackContractMetricRequirement).where(FeedbackContractMetricRequirement.feedback_contract_version_id == contract.id).order_by(FeedbackContractMetricRequirement.ordinal).with_for_update()))
    if not requirements or set(metrics) != {row.metric_code for row in requirements}:
        raise _conflict("FEEDBACK_CONTRACT_INVALID", "Forward Evidence metrics do not match the Live contract.")
    gates = list(session.scalars(select(PromotionPolicyGate).where(PromotionPolicyGate.policy_version_id == policy.id).order_by(PromotionPolicyGate.ordinal).with_for_update()))
    if not gates:
        raise _conflict("PROMOTION_POLICY_INVALID", "Live policy has no typed gates.")
    gate_results: list[PromotionGateResult] = []
    for gate in gates:
        metric = metrics.get(gate.metric_code)
        actual = metric.value if metric is not None and metric.status == "AVAILABLE" else None
        expected = Decimal(str(gate.threshold))
        passed = actual is not None and (actual >= expected if gate.comparator == "MINIMUM" else actual <= expected)
        gate_results.append(PromotionGateResult(evaluation_id=uuid4(), gate_code=gate.metric_code, status="PASS" if passed else "FAIL", actual=actual, expected=expected, reason_code=None if passed else "PROMOTION_METRIC_GATE_FAILED"))
    all_pass = all(item.status == "PASS" for item in gate_results)
    evaluation = PromotionEvaluation(id=uuid4(), purpose="PAPER_TO_LIVE", portfolio_evaluation_episode_id=None, forward_evidence_episode_id=episode.id, candidate_id=candidate.id, candidate_package_id=package.id, package_revision=package.revision, policy_version_id=policy.id, paper_to_live_policy_version_id=None, downstream_system_id=downstream.id, downstream_connection_version_id=connection.id, feedback_contract_version_id=contract.id, preflight_receipt_id=receipt.id, outcome="PASS" if all_pass else "FAIL", action=policy.mode if all_pass else "NO_ACTION")
    session.add(evaluation)
    session.flush()
    for result in gate_results:
        result.evaluation_id = evaluation.id
    session.add_all(gate_results)
    if all_pass:
        approval_row = ApprovalSnapshot(id=uuid4(), promotion_evaluation_id=evaluation.id, promotion_purpose="PAPER_TO_LIVE", candidate_id=candidate.id, candidate_package_id=package.id, candidate_package_revision=package.revision, purpose="LIVE", state="APPROVED" if policy.mode == "AUTO_HANDOFF" else "PENDING", downstream_system_id=downstream.id, downstream_connection_version_id=connection.id, feedback_contract_version_id=contract.id, preflight_receipt_id=receipt.id, paper_to_live_policy_version_id=None, valid_until=receipt.valid_until, expires_at=receipt.valid_until, human_report={"decision": "SYSTEM_APPROVED"} if policy.mode == "AUTO_HANDOFF" else {}, evidence_summary={}, capital_context={}, risk_summary={}, cost_summary={}, capacity_summary={}, changes_summary={})
        session.add(approval_row)
        session.flush()
        if policy.mode == "AUTO_HANDOFF":
            session.add(HandoffOffer(id=uuid4(), approval_id=approval_row.id, candidate_package_id=package.id, candidate_package_revision=package.revision, candidate_id=candidate.id, promotion_purpose="PAPER_TO_LIVE", purpose="LIVE", downstream_system_id=downstream.id, downstream_connection_version_id=connection.id, feedback_contract_version_id=contract.id, preflight_receipt_id=receipt.id, paper_to_live_policy_version_id=None, state="AVAILABLE", claim_deadline=datetime.now(UTC) + timedelta(days=7), feedback_state="PENDING", feedback_contract_snapshot={"feedback_contract_version_id": str(contract.id)}))
    session.flush()
    append_event(
        session,
        kind="PAPER_TO_LIVE_PROMOTION_DECIDED",
        aggregate_type="PROMOTION_EVALUATION",
        aggregate_id=evaluation.id,
        payload={
            "candidate_id": str(candidate.id),
            "outcome": evaluation.outcome,
            "action": evaluation.action,
        },
    )
    session.flush()
    return evaluation


__all__ = [
    "FeedbackHeader",
    "TypedFeedbackMetric",
    "accept_paper_feedback",
    "approve_typed_live_handoff",
    "enqueue_p2p_promotion_job",
    "enqueue_p2p_promotion_job_for_candidate",
    "maybe_enqueue_p2l",
    "maybe_enqueue_p2p",
    "maybe_enqueue_p2p_for_candidate",
    "validate_typed_paper_approval",
]
