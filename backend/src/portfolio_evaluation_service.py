"""Core-owned Portfolio evaluation facts for one assembled Candidate."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from db.models import (
    DatasetRevision,
    EvaluationDatasetSelection,
    Job,
    PortfolioAssemblyInput,
    PortfolioCandidate,
    PortfolioEvaluationAssignment,
    PortfolioEvaluationDisclosure,
    PortfolioEvaluationEpisode,
    PortfolioEvaluationGate,
    PortfolioEvaluationMetric,
    PortfolioInputEvaluationAssignment,
    PromotionPolicyVersion,
)
from errors import QfError
from jobs import enqueue_job
from research_engine.sealed_evaluator_contracts import (
    EvaluationPhase,
    EvaluationStatus,
    ImmutableReference,
    PortfolioEvaluationInput,
    SealedEvaluationResult,
)


_EVALUATOR_CONTRACT = "PORTFOLIO_EVALUATION_V1"
_PROMOTION_POLICY_CONTRACT = "PROMOTION_POLICY_V1"
_NUMERIC_QUANTUM = Decimal("0.00000001")


@dataclass(frozen=True, slots=True)
class _CandidateSource:
    candidate: PortfolioCandidate
    assembly_input: PortfolioAssemblyInput
    input_assignment: PortfolioInputEvaluationAssignment
    selection: EvaluationDatasetSelection
    sealed_dataset: DatasetRevision
    policy: PromotionPolicyVersion


@dataclass(frozen=True, slots=True)
class _EvaluationContext:
    assignment: PortfolioEvaluationAssignment
    episode: PortfolioEvaluationEpisode
    source: _CandidateSource
    input: PortfolioEvaluationInput


def _conflict(code: str, message: str) -> QfError:
    return QfError(code, message, 409)


def _typed_p2p_policy(policy: PromotionPolicyVersion) -> bool:
    return (
        policy.policy_contract_version == _PROMOTION_POLICY_CONTRACT
        and policy.purpose == "PORTFOLIO_TO_PAPER"
        and policy.mode == "MANUAL_APPROVAL"
        and policy.state == "ACTIVE"
        and all(
            value is not None
            for value in (
                policy.paper_downstream_system_id,
                policy.paper_connection_version_id,
                policy.paper_feedback_contract_version_id,
                policy.paper_preflight_receipt_id,
                policy.paper_to_live_policy_version_id,
            )
        )
        and all(
            value is None
            for value in (
                policy.live_downstream_system_id,
                policy.live_connection_version_id,
                policy.live_feedback_contract_version_id,
                policy.live_preflight_receipt_id,
            )
        )
    )


def _ensure_empty_job(session: Session, assignment_id: UUID) -> None:
    # The Candidate is already locked by the caller; workers lock Job first.
    jobs = list(
        session.scalars(
            select(Job).where(
                Job.kind == "PORTFOLIO_EVALUATION",
                Job.resource_type == "portfolio_evaluation_assignment",
                Job.resource_id == assignment_id,
                Job.state.in_(("READY", "LEASED")),
            )
        )
    )
    if len(jobs) > 1 or (jobs and jobs[0].payload != {}):
        raise _conflict("PORTFOLIO_EVALUATION_JOB_CONFLICT", "Portfolio evaluation job is invalid.")
    if not jobs:
        enqueue_job(
            session,
            kind="PORTFOLIO_EVALUATION",
            resource_type="portfolio_evaluation_assignment",
            resource_id=assignment_id,
            payload={},
        )


def _candidate_source(session: Session, candidate_id: UUID) -> _CandidateSource:
    candidate = session.scalar(
        select(PortfolioCandidate)
        .where(PortfolioCandidate.id == candidate_id)
        .with_for_update()
    )
    if (
        candidate is None
        or candidate.state != "ASSEMBLED"
        or candidate.candidate_family_id is None
        or candidate.assembly_input_id is None
    ):
        raise _conflict(
            "PORTFOLIO_EVALUATION_CANDIDATE_INVALID",
            "Portfolio evaluation requires an assembled relational Candidate.",
        )
    assembly_input = session.scalar(
        select(PortfolioAssemblyInput)
        .where(PortfolioAssemblyInput.id == candidate.assembly_input_id)
        .with_for_update()
    )
    input_assignment = (
        session.scalar(
            select(PortfolioInputEvaluationAssignment)
            .where(
                PortfolioInputEvaluationAssignment.id
                == assembly_input.portfolio_input_evaluation_assignment_id
            )
            .with_for_update()
        )
        if assembly_input is not None
        else None
    )
    selection = (
        session.scalar(
            select(EvaluationDatasetSelection)
            .where(EvaluationDatasetSelection.id == input_assignment.evaluation_dataset_selection_id)
            .with_for_update()
        )
        if input_assignment is not None
        else None
    )
    sealed_dataset = (
        session.scalar(
            select(DatasetRevision)
            .where(DatasetRevision.id == input_assignment.sealed_dataset_revision_id)
            .with_for_update()
        )
        if input_assignment is not None
        else None
    )
    policy = (
        session.scalar(
            select(PromotionPolicyVersion)
            .where(PromotionPolicyVersion.id == input_assignment.promotion_policy_version_id)
            .with_for_update()
        )
        if input_assignment is not None
        else None
    )
    if (
        assembly_input is None
        or input_assignment is None
        or selection is None
        or sealed_dataset is None
        or policy is None
        or assembly_input.state != "ASSEMBLED"
        or input_assignment.state != "VALID"
        or candidate.portfolio_program_id != assembly_input.portfolio_program_id
        or candidate.mandate_version_id != assembly_input.mandate_version_id
        or candidate.capital_context_version_id != assembly_input.capital_context_version_id
        or candidate.universe_version_id != assembly_input.universe_version_id
        or assembly_input.portfolio_input_evaluation_assignment_id != input_assignment.id
        or assembly_input.portfolio_program_id != input_assignment.portfolio_program_id
        or assembly_input.mandate_version_id != input_assignment.mandate_version_id
        or assembly_input.promotion_policy_version_id != input_assignment.promotion_policy_version_id
        or assembly_input.cause_event_id != input_assignment.cause_event_id
        or assembly_input.previous_candidate_id != input_assignment.previous_candidate_id
        or assembly_input.universe_version_id != selection.universe_version_id
        or selection.id != input_assignment.evaluation_dataset_selection_id
        or selection.sealed_dataset_revision_id != input_assignment.sealed_dataset_revision_id
        or sealed_dataset.id != input_assignment.sealed_dataset_revision_id
        or sealed_dataset.universe_version_id != selection.universe_version_id
        or policy.id != input_assignment.promotion_policy_version_id
        or not _typed_p2p_policy(policy)
    ):
        raise _conflict(
            "PORTFOLIO_EVALUATION_SOURCE_INVALID",
            "Portfolio evaluation source facts do not match the frozen Input lineage.",
        )
    return _CandidateSource(
        candidate=candidate,
        assembly_input=assembly_input,
        input_assignment=input_assignment,
        selection=selection,
        sealed_dataset=sealed_dataset,
        policy=policy,
    )


def _matches_source(assignment: PortfolioEvaluationAssignment, source: _CandidateSource) -> bool:
    candidate = source.candidate
    input_assignment = source.input_assignment
    return (
        assignment.portfolio_program_id == candidate.portfolio_program_id
        and assignment.candidate_id == candidate.id
        and assignment.candidate_family_id == candidate.candidate_family_id
        and assignment.mandate_version_id == candidate.mandate_version_id
        and assignment.assembly_input_id == source.assembly_input.id
        and assignment.evaluation_dataset_selection_id == input_assignment.evaluation_dataset_selection_id
        and assignment.sealed_dataset_revision_id == input_assignment.sealed_dataset_revision_id
        and assignment.promotion_policy_version_id == input_assignment.promotion_policy_version_id
        and assignment.cause_event_id == input_assignment.cause_event_id
        and assignment.previous_candidate_id == input_assignment.previous_candidate_id
        and assignment.evaluator_contract_version == _EVALUATOR_CONTRACT
    )


def ensure_portfolio_evaluation(
    session: Session, *, candidate_id: UUID
) -> PortfolioEvaluationAssignment:
    """Freeze one evaluator Assignment/Episode and its empty leased-job resource."""
    if not isinstance(candidate_id, UUID):
        raise QfError("PORTFOLIO_CANDIDATE_ID_INVALID", "Portfolio Candidate ID is invalid.", 422)
    source = _candidate_source(session, candidate_id)
    assignments = list(
        session.scalars(
            select(PortfolioEvaluationAssignment)
            .where(PortfolioEvaluationAssignment.candidate_id == candidate_id)
            .with_for_update()
        )
    )
    if len(assignments) > 1:
        raise _conflict(
            "PORTFOLIO_EVALUATION_ASSIGNMENT_CONFLICT",
            "Candidate has more than one Portfolio evaluation Assignment.",
        )
    if assignments:
        assignment = assignments[0]
        if not _matches_source(assignment, source):
            raise _conflict(
                "PORTFOLIO_EVALUATION_ASSIGNMENT_CONFLICT",
                "Candidate evaluation Assignment does not match frozen source facts.",
            )
    else:
        assignment = PortfolioEvaluationAssignment(
            id=uuid4(),
            portfolio_program_id=source.candidate.portfolio_program_id,
            candidate_id=source.candidate.id,
            candidate_family_id=source.candidate.candidate_family_id,
            mandate_version_id=source.candidate.mandate_version_id,
            assembly_input_id=source.assembly_input.id,
            evaluation_dataset_selection_id=source.input_assignment.evaluation_dataset_selection_id,
            sealed_dataset_revision_id=source.input_assignment.sealed_dataset_revision_id,
            promotion_policy_version_id=source.input_assignment.promotion_policy_version_id,
            cause_event_id=source.input_assignment.cause_event_id,
            previous_candidate_id=source.input_assignment.previous_candidate_id,
            evaluator_contract_version=_EVALUATOR_CONTRACT,
            state="QUEUED",
        )
        session.add(assignment)
        session.flush()
    episodes = list(
        session.scalars(
            select(PortfolioEvaluationEpisode)
            .where(PortfolioEvaluationEpisode.assignment_id == assignment.id)
            .with_for_update()
        )
    )
    if len(episodes) > 1:
        raise _conflict(
            "PORTFOLIO_EVALUATION_EPISODE_CONFLICT",
            "Portfolio evaluation Assignment has more than one Episode.",
        )
    if episodes:
        episode = episodes[0]
        if episode.candidate_id != source.candidate.id:
            raise _conflict(
                "PORTFOLIO_EVALUATION_EPISODE_CONFLICT",
                "Portfolio evaluation Episode does not match its Candidate.",
            )
    elif assignment.state == "FINALIZED":
        raise _conflict(
            "PORTFOLIO_EVALUATION_EPISODE_CONFLICT",
            "Finalized Portfolio evaluation Assignment is missing its Episode.",
        )
    else:
        session.add(
            PortfolioEvaluationEpisode(
                id=uuid4(),
                assignment_id=assignment.id,
                candidate_id=source.candidate.id,
                state="ASSIGNED",
            )
        )
        session.flush()
    if assignment.state == "FROZEN":
        assignment.state = "QUEUED"
    if assignment.state in {"QUEUED", "RUNNING"}:
        _ensure_empty_job(session, assignment.id)
    return assignment


def _evaluation_input(
    assignment: PortfolioEvaluationAssignment,
    episode: PortfolioEvaluationEpisode,
    source: _CandidateSource,
) -> PortfolioEvaluationInput:
    candidate_family_id = source.candidate.candidate_family_id
    if candidate_family_id is None:
        raise _conflict(
            "PORTFOLIO_EVALUATION_SOURCE_INVALID",
            "Portfolio evaluation Candidate is missing its Family.",
        )
    return PortfolioEvaluationInput(
        assignment_id=assignment.id,
        episode_id=episode.id,
        candidate_id=source.candidate.id,
        candidate_family_id=candidate_family_id,
        previous_candidate_id=source.input_assignment.previous_candidate_id,
        assembly_input_id=source.assembly_input.id,
        evaluation_dataset_selection=ImmutableReference(
            source.selection.id, source.selection.version_no
        ),
        sealed_dataset=ImmutableReference(
            source.sealed_dataset.id, source.sealed_dataset.revision_no
        ),
        policy_version=ImmutableReference(source.policy.id, source.policy.version_no),
        cause_event_id=source.input_assignment.cause_event_id,
    )


def _evaluation_context(
    session: Session,
    assignment_id: UUID,
    *,
    allow_finalized: bool,
) -> _EvaluationContext:
    assignment = session.scalar(
        select(PortfolioEvaluationAssignment)
        .where(PortfolioEvaluationAssignment.id == assignment_id)
        .with_for_update()
    )
    if assignment is None:
        raise QfError(
            "PORTFOLIO_EVALUATION_ASSIGNMENT_NOT_FOUND",
            "Portfolio evaluation Assignment was not found.",
            404,
        )
    if assignment.state == "FINALIZED" and not allow_finalized:
        raise _conflict(
            "PORTFOLIO_EVALUATION_STATE_CONFLICT",
            "Portfolio evaluation Assignment is already finalized.",
        )
    if assignment.state not in {"QUEUED", "RUNNING", "FINALIZED"}:
        raise _conflict(
            "PORTFOLIO_EVALUATION_STATE_CONFLICT",
            "Portfolio evaluation Assignment is not awaiting trusted work.",
        )
    source = _candidate_source(session, assignment.candidate_id)
    if not _matches_source(assignment, source):
        raise _conflict(
            "PORTFOLIO_EVALUATION_SOURCE_INVALID",
            "Portfolio evaluation Assignment no longer matches frozen source facts.",
        )
    episode = session.scalar(
        select(PortfolioEvaluationEpisode)
        .where(PortfolioEvaluationEpisode.assignment_id == assignment.id)
        .with_for_update()
    )
    if episode is None or episode.candidate_id != source.candidate.id:
        raise _conflict(
            "PORTFOLIO_EVALUATION_EPISODE_CONFLICT",
            "Portfolio evaluation Episode is missing or has invalid lineage.",
        )
    return _EvaluationContext(
        assignment=assignment,
        episode=episode,
        source=source,
        input=_evaluation_input(assignment, episode, source),
    )


def prepare_portfolio_evaluation(
    session: Session, assignment_id: UUID
) -> PortfolioEvaluationInput:
    """Rebuild the sole Portfolio descriptor input from frozen relational facts."""
    if not isinstance(assignment_id, UUID):
        raise QfError(
            "PORTFOLIO_EVALUATION_ASSIGNMENT_ID_INVALID",
            "Portfolio evaluation Assignment ID is invalid.",
            422,
        )
    context = _evaluation_context(session, assignment_id, allow_finalized=False)
    if context.episode.state not in {"ASSIGNED", "EVALUATING"}:
        raise _conflict(
            "PORTFOLIO_EVALUATION_STATE_CONFLICT",
            "Portfolio evaluation Episode is not awaiting trusted work.",
        )
    context.assignment.state = "RUNNING"
    context.episode.state = "EVALUATING"
    return context.input


def _matches_existing(
    session: Session,
    context: _EvaluationContext,
    result: SealedEvaluationResult,
) -> bool:
    metrics = tuple(
        session.scalars(
            select(PortfolioEvaluationMetric)
            .where(PortfolioEvaluationMetric.episode_id == context.episode.id)
            .order_by(PortfolioEvaluationMetric.metric_code)
            .with_for_update()
        )
    )
    gates = tuple(
        session.scalars(
            select(PortfolioEvaluationGate)
            .where(PortfolioEvaluationGate.episode_id == context.episode.id)
            .order_by(PortfolioEvaluationGate.gate_code)
            .with_for_update()
        )
    )
    disclosure = session.get(
        PortfolioEvaluationDisclosure,
        context.episode.id,
        with_for_update=True,
    )
    return (
        context.assignment.private_result_ref == result.private_result_id
        and _stored_utc(context.assignment.evaluated_at) == result.evaluated_at
        and context.assignment.outcome == result.status.value
        and context.episode.state == "DISCLOSED"
        and context.episode.result == result.status.value
        and _stored_utc(context.episode.evaluated_at) == result.evaluated_at
        and tuple((row.metric_code, row.status, row.value) for row in metrics)
        == tuple(
            (metric.code.value, metric.status.value, _metric_value(metric)) for metric in result.metrics
        )
        and tuple((row.gate_code, row.status, row.reason_code) for row in gates)
        == tuple(
            (gate.code.value, gate.status.value, _reason(gate.reason_code)) for gate in result.gates
        )
        and disclosure is not None
        and disclosure.candidate_id == context.source.candidate.id
        and disclosure.classification == result.disclosure.classification.value
        and disclosure.reason_code == _reason(result.disclosure.reason_code)
    )


def _metric_value(metric: object) -> Decimal | None:
    value = getattr(metric, "value", None)
    if value is None:
        return None
    return Decimal(str(value)).quantize(_NUMERIC_QUANTUM)


def _stored_utc(value: datetime | None) -> datetime | None:
    if value is None:
        return None
    return value.replace(tzinfo=UTC) if value.tzinfo is None else value.astimezone(UTC)


def _reason(reason: object) -> str | None:
    return getattr(reason, "value", None)


def _first_family_baseline(result: SealedEvaluationResult) -> bool:
    metric = next(
        (item for item in result.metrics if item.code.value == "MATERIAL_IMPROVEMENT"), None
    )
    gate = next(
        (item for item in result.gates if item.code.value == "MATERIAL_IMPROVEMENT_VALID"), None
    )
    return (
        metric is not None
        and metric.status.value == "AVAILABLE"
        and _metric_value(metric) == Decimal("0")
        and gate is not None
        and gate.status.value == "PASS"
    )


def accept_portfolio_evaluation_result(
    session: Session, result: SealedEvaluationResult
) -> PortfolioEvaluationEpisode:
    """Persist one complete typed Portfolio result, never a JSON gate fallback."""
    if not isinstance(result, SealedEvaluationResult) or not isinstance(
        result.input, PortfolioEvaluationInput
    ):
        raise QfError(
            "PORTFOLIO_EVALUATION_RESULT_REQUIRED",
            "Portfolio evaluation requires a typed sealed evaluator result.",
            422,
        )
    context = _evaluation_context(session, result.input.assignment_id, allow_finalized=True)
    if result.input != context.input:
        raise _conflict(
            "PORTFOLIO_EVALUATION_INPUT_MISMATCH",
            "Portfolio evaluator result does not match frozen assignment inputs.",
        )
    if context.assignment.state == "FINALIZED":
        if _matches_existing(session, context, result):
            return context.episode
        raise _conflict(
            "PORTFOLIO_EVALUATION_RESULT_CONFLICT",
            "Portfolio evaluation already has different immutable result facts.",
        )
    if context.episode.state not in {"ASSIGNED", "EVALUATING"}:
        raise _conflict(
            "PORTFOLIO_EVALUATION_STATE_CONFLICT",
            "Portfolio evaluation Episode is not awaiting a trusted result.",
        )
    if result.status is EvaluationStatus.PASS and (
        context.assignment.previous_candidate_id is None and not _first_family_baseline(result)
    ):
        raise _conflict(
            "PORTFOLIO_EVALUATION_BASELINE_INVALID",
            "First-family Portfolio PASS must carry the fixed material-improvement baseline.",
        )
    if any(metric.phase is not EvaluationPhase.SEALED for metric in result.metrics):
        raise QfError(
            "PORTFOLIO_EVALUATION_RESULT_REQUIRED",
            "Portfolio evaluation metrics must be sealed.",
            422,
        )
    context.assignment.state = "FINALIZED"
    context.assignment.private_result_ref = result.private_result_id
    context.assignment.evaluated_at = result.evaluated_at
    context.assignment.outcome = result.status.value
    context.assignment.completed_at = datetime.now(UTC)
    context.episode.state = "DISCLOSED"
    context.episode.result = result.status.value
    context.episode.evaluated_at = result.evaluated_at
    context.episode.disclosed_at = datetime.now(UTC)
    session.add_all(
        PortfolioEvaluationMetric(
            episode_id=context.episode.id,
            metric_code=metric.code.value,
            status=metric.status.value,
            value=_metric_value(metric),
        )
        for metric in result.metrics
    )
    session.add_all(
        PortfolioEvaluationGate(
            episode_id=context.episode.id,
            gate_code=gate.code.value,
            status=gate.status.value,
            reason_code=_reason(gate.reason_code),
        )
        for gate in result.gates
    )
    session.add(
        PortfolioEvaluationDisclosure(
            episode_id=context.episode.id,
            candidate_id=context.source.candidate.id,
            classification=result.disclosure.classification.value,
            reason_code=_reason(result.disclosure.reason_code),
        )
    )
    session.flush()
    # Package completion may race the evaluator.  Both completions converge on
    # the same idempotent Core writer; a missing Package is intentionally a no-op.
    from promotion_service import enqueue_p2p_promotion_job

    enqueue_p2p_promotion_job(session, portfolio_evaluation_episode_id=context.episode.id)
    return context.episode


__all__ = [
    "accept_portfolio_evaluation_result",
    "ensure_portfolio_evaluation",
    "prepare_portfolio_evaluation",
]
