"""Trusted relational Portfolio input and Candidate assembly boundary.

This module accepts no caller-supplied returns, covariance matrix, capital, or
JSON payload.  It freezes trusted Alpha facts first, then accepts only the
typed covariance upper triangle returned for that frozen assignment.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from decimal import Decimal, InvalidOperation, ROUND_HALF_UP
from math import isclose, isfinite
from typing import TypeVar
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from candidate_packages import enqueue_candidate_package_build
from db.models import (
    AlphaEvaluationAssignment,
    AlphaEvaluationEpisode,
    AlphaEvaluationForecast,
    AlphaEvaluationResult,
    AlphaQualification,
    AlphaSignalArtifact,
    CapitalContextVersion,
    DatasetRevision,
    Event,
    EvaluationDatasetSelection,
    Job,
    PortfolioAssemblyInput,
    PortfolioAssemblyInputCovariance,
    PortfolioAssemblyInputMember,
    PortfolioCandidate,
    PortfolioCandidateFamily,
    PortfolioCandidateMember,
    PortfolioInputEvaluationAssignment,
    PortfolioInputEvaluationAssignmentMember,
    PortfolioMandate,
    PortfolioMandateVersion,
    PortfolioProgram,
    PortfolioSearchLedgerEntry,
    PromotionPolicyGate,
    PromotionPolicyVersion,
)
from errors import QfError
from jobs import enqueue_job
from portfolio_evaluation_service import ensure_portfolio_evaluation
from research_engine.sealed_evaluator_contracts import (
    ImmutableReference,
    PortfolioCovarianceMethod,
    PortfolioInputAxis,
    PortfolioInputEvaluationInput,
    PortfolioInputEvaluationResult as EvaluatorPortfolioInputEvaluationResult,
)


_EVALUATOR_CONTRACT = "PORTFOLIO_INPUT_EVALUATION_V1"
_INPUT_CONTRACT = "LONG_ONLY_MEAN_VARIANCE_V1"
_PROMOTION_POLICY_CONTRACT = "PROMOTION_POLICY_V1"
_COVARIANCE_METHOD = PortfolioCovarianceMethod.EWMA_SHRINKAGE.value
_FINITE_WORDS = frozenset({"nan", "inf", "-inf", "infinity", "-infinity"})
_NUMERIC_QUANTUM = Decimal("0.00000001")
_Value = TypeVar("_Value")


@dataclass(frozen=True, slots=True)
class PortfolioInputEvaluationRequest:
    mandate_version_id: UUID
    cause_event_id: int
    alpha_qualification_ids: tuple[UUID, ...]
    previous_candidate_id: UUID | None = None


@dataclass(frozen=True, slots=True)
class PortfolioCovariance:
    left_axis_index: int
    right_axis_index: int
    covariance: Decimal


@dataclass(frozen=True, slots=True)
class PortfolioInputEvaluationResult:
    assignment_id: UUID
    private_result_ref: UUID
    evaluated_at: datetime
    covariance_method: str
    covariance_observations: int
    covariance_decay: Decimal
    covariance_shrinkage: Decimal
    covariance_upper_triangle: tuple[PortfolioCovariance, ...]


@dataclass(frozen=True, slots=True)
class _TrustedAxis:
    qualification: AlphaQualification
    result: AlphaEvaluationResult
    signal: AlphaSignalArtifact
    forecast: AlphaEvaluationForecast


@dataclass(frozen=True, slots=True)
class _InputDependencies:
    capital: CapitalContextVersion
    selection: EvaluationDatasetSelection
    policy: PromotionPolicyVersion
    axes: tuple[_TrustedAxis, ...]
    as_of: datetime


def _conflict(code: str, message: str) -> QfError:
    return QfError(code, message, 409)


def _finite_decimal(value: object) -> Decimal | None:
    if not isinstance(value, Decimal) or not value.is_finite():
        return None
    if value.to_eng_string().lower() in _FINITE_WORDS:
        return None
    return value


def _canonical_numeric(value: object) -> Decimal | None:
    decimal = _finite_decimal(value)
    if decimal is None:
        return None
    try:
        canonical = decimal.quantize(_NUMERIC_QUANTUM, rounding=ROUND_HALF_UP)
    except InvalidOperation:
        return None
    return canonical if canonical.is_zero() or canonical.adjusted() <= 11 else None


def _valid_datetime(value: object) -> datetime | None:
    if not isinstance(value, datetime) or value.tzinfo is None or value.utcoffset() != timedelta():
        return None
    return value


def _stored_utc(value: object) -> datetime | None:
    if not isinstance(value, datetime):
        return None
    return value.replace(tzinfo=UTC) if value.tzinfo is None else value.astimezone(UTC)


def _only(rows: Sequence[_Value]) -> _Value | None:
    return rows[0] if len(rows) == 1 else None


def _record_ledger(
    session: Session,
    *,
    portfolio_program_id: UUID,
    cause_event_id: int,
    attempt_type: str,
    outcome_class: str,
    reason_code: str,
    portfolio_assembly_input_id: UUID | None = None,
) -> None:
    existing = session.scalar(
        select(PortfolioSearchLedgerEntry).where(
            PortfolioSearchLedgerEntry.portfolio_program_id == portfolio_program_id,
            PortfolioSearchLedgerEntry.cause_event_id == cause_event_id,
            PortfolioSearchLedgerEntry.attempt_type == attempt_type,
        )
    )
    if existing is None:
        session.add(
            PortfolioSearchLedgerEntry(
                portfolio_program_id=portfolio_program_id,
                cause_event_id=cause_event_id,
                portfolio_assembly_input_id=portfolio_assembly_input_id,
                attempt_type=attempt_type,
                outcome_class=outcome_class,
                reason_code=reason_code,
            )
        )


def _v1_mandate(mandate: PortfolioMandateVersion, *, active: bool) -> bool:
    scalar_names = (
        "minimum_weight",
        "maximum_weight",
        "gross_exposure_limit",
        "net_exposure_target",
        "cash_reserve",
        "turnover_limit",
        "variance_limit",
        "risk_aversion",
        "cost_aversion",
        "uncertainty_aversion",
        "commission_rate",
        "half_spread_rate",
        "slippage_rate",
        "impact_rate",
        "impact_breakpoint",
    )
    return (
        mandate.policy_family == _INPUT_CONTRACT
        and mandate.objective == "MAXIMIZE_NET_RETURN"
        and (not active or mandate.state == "ACTIVE")
        and mandate.universe_version_id is not None
        and mandate.eligible_alpha_role == "PRIMARY_ALPHA"
        and isinstance(mandate.minimum_alpha_count, int)
        and not isinstance(mandate.minimum_alpha_count, bool)
        and mandate.minimum_alpha_count >= 2
        and all(_finite_decimal(getattr(mandate, name)) is not None for name in scalar_names)
    )


def _valid_capital(
    capital: CapitalContextVersion, mandate: PortfolioMandateVersion, as_of: datetime
) -> bool:
    observed_at = _stored_utc(capital.observed_at)
    valid_until = _stored_utc(capital.valid_until)
    normalized_as_of = _stored_utc(as_of)
    return (
        capital.configuration_contract_version == "CAPITAL_CONTEXT_V1"
        and capital.source_type == "ADMIN"
        and capital.source_downstream_system_id is None
        and capital.base_currency == mandate.base_currency
        and observed_at is not None
        and valid_until is not None
        and normalized_as_of is not None
        and observed_at <= normalized_as_of <= valid_until
        and _finite_decimal(capital.deployable_capital) is not None
        and capital.deployable_capital > 0
    )


def _paper_tuple(policy: PromotionPolicyVersion) -> tuple[UUID | None, UUID | None, UUID | None, UUID | None]:
    return (
        policy.paper_downstream_system_id,
        policy.paper_connection_version_id,
        policy.paper_feedback_contract_version_id,
        policy.paper_preflight_receipt_id,
    )


def _strict_p2p_policy(session: Session, policy: PromotionPolicyVersion) -> bool:
    paper_tuple = _paper_tuple(policy)
    if (
        policy.policy_contract_version != _PROMOTION_POLICY_CONTRACT
        or policy.purpose != "PORTFOLIO_TO_PAPER"
        or policy.mode != "MANUAL_APPROVAL"
        or policy.state != "ACTIVE"
        or any(value is None for value in paper_tuple)
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
        return False
    paper_to_live = session.scalar(
        select(PromotionPolicyVersion)
        .where(PromotionPolicyVersion.id == policy.paper_to_live_policy_version_id)
        .with_for_update()
    )
    if (
        paper_to_live is None
        or paper_to_live.policy_contract_version != _PROMOTION_POLICY_CONTRACT
        or paper_to_live.purpose != "PAPER_TO_LIVE"
        or paper_to_live.state != "ACTIVE"
        or paper_to_live.paper_to_live_policy_version_id is not None
        or _paper_tuple(paper_to_live) != paper_tuple
        or any(
            value is None
            for value in (
                paper_to_live.live_downstream_system_id,
                paper_to_live.live_connection_version_id,
                paper_to_live.live_feedback_contract_version_id,
                paper_to_live.live_preflight_receipt_id,
            )
        )
    ):
        return False
    gates = list(
        session.scalars(
            select(PromotionPolicyGate)
            .where(PromotionPolicyGate.policy_version_id == policy.id)
            .with_for_update()
        )
    )
    return any(
        gate.metric_code == "MATERIAL_IMPROVEMENT"
        and gate.comparator == "MINIMUM"
        and (threshold := _finite_decimal(gate.threshold)) is not None
        and threshold <= 0
        for gate in gates
    )


def _input_dependencies(
    session: Session,
    mandate: PortfolioMandateVersion,
    qualification_ids: tuple[UUID, ...],
) -> _InputDependencies | None:
    selections = list(
        session.scalars(
            select(EvaluationDatasetSelection)
            .where(
                EvaluationDatasetSelection.universe_version_id == mandate.universe_version_id,
                EvaluationDatasetSelection.state == "ENABLED",
            )
            .with_for_update()
        )
    )
    selection = _only(selections)
    if not isinstance(selection, EvaluationDatasetSelection):
        return None
    axes = _trusted_axes(
        session,
        mandate,
        qualification_ids,
        selection.sealed_dataset_revision_id,
    )
    if axes is None:
        return None
    as_of = _stored_utc(axes[0].forecast.as_of_time)
    if as_of is None:
        return None
    capitals = list(
        session.scalars(
            select(CapitalContextVersion)
            .where(
                CapitalContextVersion.configuration_contract_version == "CAPITAL_CONTEXT_V1",
                CapitalContextVersion.source_type == "ADMIN",
                CapitalContextVersion.source_downstream_system_id.is_(None),
                CapitalContextVersion.base_currency == mandate.base_currency,
                CapitalContextVersion.observed_at <= as_of,
                CapitalContextVersion.valid_until >= as_of,
            )
            .with_for_update()
        )
    )
    policies = list(
        session.scalars(
            select(PromotionPolicyVersion)
            .where(
                PromotionPolicyVersion.policy_contract_version == _PROMOTION_POLICY_CONTRACT,
                PromotionPolicyVersion.purpose == "PORTFOLIO_TO_PAPER",
                PromotionPolicyVersion.mode == "MANUAL_APPROVAL",
                PromotionPolicyVersion.state == "ACTIVE",
            )
            .with_for_update()
        )
    )
    capital = _only(capitals)
    policy = _only(policies)
    if (
        not isinstance(capital, CapitalContextVersion)
        or not isinstance(policy, PromotionPolicyVersion)
        or not _valid_capital(capital, mandate, as_of)
        or not _strict_p2p_policy(session, policy)
    ):
        return None
    return _InputDependencies(
        capital=capital,
        selection=selection,
        policy=policy,
        axes=axes,
        as_of=as_of,
    )


def _ensure_empty_job(
    session: Session,
    *,
    kind: str,
    resource_type: str,
    resource_id: UUID,
) -> None:
    # Writers lock their resource first; do not invert that order by locking Job here.
    jobs = list(
        session.scalars(
            select(Job).where(
                Job.kind == kind,
                Job.resource_type == resource_type,
                Job.resource_id == resource_id,
                Job.state.in_(("READY", "LEASED")),
            )
        )
    )
    if len(jobs) > 1 or (jobs and jobs[0].payload != {}):
        raise _conflict("PORTFOLIO_JOB_CONFLICT", "Portfolio work has an invalid active job.")
    if not jobs:
        enqueue_job(
            session,
            kind=kind,
            resource_type=resource_type,
            resource_id=resource_id,
            payload={},
        )


def _locked_program(session: Session, mandate: PortfolioMandateVersion) -> PortfolioProgram | None:
    programs = list(
        session.scalars(
            select(PortfolioProgram)
            .where(PortfolioProgram.mandate_version_id == mandate.id)
            .with_for_update()
        )
    )
    if len(programs) > 1:
        raise _conflict(
            "PORTFOLIO_PROGRAM_LINEAGE_CONFLICT",
            "A V1 Mandate Version has more than one Portfolio Program.",
        )
    return programs[0] if programs else None


def _locked_program_and_family(
    session: Session, mandate: PortfolioMandateVersion
) -> tuple[PortfolioProgram, PortfolioCandidateFamily]:
    program = _locked_program(session, mandate)
    if program is None:
        try:
            with session.begin_nested():
                program = PortfolioProgram(
                    mandate_version_id=mandate.id,
                    state="WAITING_FOR_ALPHA",
                )
                session.add(program)
                session.flush()
        except IntegrityError:
            concurrent_program = session.scalar(
                select(PortfolioProgram)
                .where(PortfolioProgram.mandate_version_id == mandate.id)
                .with_for_update()
            )
            if concurrent_program is None:
                raise
            program = concurrent_program

    families = list(
        session.scalars(
            select(PortfolioCandidateFamily)
            .where(PortfolioCandidateFamily.portfolio_program_id == program.id)
            .with_for_update()
        )
    )
    if len(families) > 1:
        raise _conflict(
            "PORTFOLIO_CANDIDATE_FAMILY_CONFLICT",
            "A Portfolio Program has more than one Candidate Family.",
        )
    if families:
        family = families[0]
        if family.mandate_version_id != mandate.id:
            raise _conflict(
                "PORTFOLIO_CANDIDATE_FAMILY_CONFLICT",
                "Candidate Family does not match its Portfolio Program Mandate.",
            )
    else:
        family = PortfolioCandidateFamily(
            portfolio_program_id=program.id,
            mandate_version_id=mandate.id,
        )
        session.add(family)
        session.flush()
    return program, family


def _finalized_episode_assignment(
    session: Session,
    episode: AlphaEvaluationEpisode,
    mandate: PortfolioMandateVersion,
) -> AlphaEvaluationAssignment | None:
    if episode.assignment_id is None:
        return None
    assignment = session.scalar(
        select(AlphaEvaluationAssignment)
        .where(AlphaEvaluationAssignment.id == episode.assignment_id)
        .with_for_update()
    )
    if (
        assignment is None
        or assignment.state != "FINALIZED"
        or episode.state != "DISCLOSED"
        or episode.result != "PASS"
        or assignment.program_id != episode.program_id
        or assignment.branch_id != episode.branch_id
        or assignment.alpha_model_version_id != episode.alpha_model_version_id
        or assignment.sealed_dataset_revision_id != episode.sealed_dataset_revision_id
        or assignment.promotion_policy_version_id != episode.promotion_policy_version_id
        or assignment.universe_version_id != mandate.universe_version_id
    ):
        return None
    return assignment


def _trusted_axes(
    session: Session,
    mandate: PortfolioMandateVersion,
    qualification_ids: tuple[UUID, ...],
    sealed_dataset_revision_id: UUID,
) -> tuple[_TrustedAxis, ...] | None:
    if len(qualification_ids) < mandate.minimum_alpha_count or len(set(qualification_ids)) != len(
        qualification_ids
    ):
        return None
    qualifications = list(
        session.scalars(
            select(AlphaQualification)
            .where(AlphaQualification.id.in_(qualification_ids))
            .with_for_update()
        )
    )
    by_id = {qualification.id: qualification for qualification in qualifications}
    if len(by_id) != len(qualification_ids):
        return None

    axes: list[_TrustedAxis] = []
    for qualification_id in qualification_ids:
        qualification = by_id[qualification_id]
        if (
            qualification.state != "ACTIVE"
            or qualification.role != mandate.eligible_alpha_role
            or qualification.universe_version_id != mandate.universe_version_id
            or qualification.evaluation_result_id is None
        ):
            return None
        result = session.scalar(
            select(AlphaEvaluationResult)
            .where(AlphaEvaluationResult.id == qualification.evaluation_result_id)
            .with_for_update()
        )
        episode = (
            session.scalar(
                select(AlphaEvaluationEpisode)
                .where(AlphaEvaluationEpisode.id == result.episode_id)
                .with_for_update()
            )
            if result is not None
            else None
        )
        finalized_assignment = (
            _finalized_episode_assignment(session, episode, mandate)
            if episode is not None
            else None
        )
        if (
            result is None
            or episode is None
            or finalized_assignment is None
            or qualification.evaluation_episode_id != episode.id
            or episode.sealed_dataset_revision_id != sealed_dataset_revision_id
            or result.evidence_validity != "VALID"
            or result.result != "PASS"
        ):
            return None
        signals = list(
            session.scalars(
                select(AlphaSignalArtifact)
                .where(AlphaSignalArtifact.evaluation_result_id == result.id)
                .with_for_update()
            )
        )
        signal = _only(signals)
        if (
            not isinstance(signal, AlphaSignalArtifact)
            or signal.mode != "CALIBRATED_RETURN"
            or signal.dataset_revision_id != sealed_dataset_revision_id
        ):
            return None
        forecasts = list(
            session.scalars(
                select(AlphaEvaluationForecast)
                .where(AlphaEvaluationForecast.result_id == result.id)
                .with_for_update()
            )
        )
        forecast = _only(forecasts)
        if (
            not isinstance(forecast, AlphaEvaluationForecast)
            or forecast.signal_artifact_id != signal.id
            or not forecast.instrument_id.strip()
            or any(
                _finite_decimal(getattr(forecast, name)) is None
                for name in (
                    "expected_return",
                    "uncertainty",
                    "confidence",
                    "max_trade_notional",
                    "max_position_notional",
                    "max_participation_rate",
                    "days_to_liquidate",
                    "stressed_capacity_notional",
                )
            )
        ):
            return None
        axes.append(_TrustedAxis(qualification, result, signal, forecast))

    as_of = _stored_utc(axes[0].forecast.as_of_time)
    if (
        as_of is None
        or any(_stored_utc(axis.forecast.as_of_time) != as_of for axis in axes)
        or len({axis.forecast.instrument_id for axis in axes}) != len(axes)
    ):
        return None
    return tuple(axes)


def _assignment_members(
    session: Session, assignment_id: UUID
) -> tuple[PortfolioInputEvaluationAssignmentMember, ...] | None:
    rows = list(
        session.scalars(
            select(PortfolioInputEvaluationAssignmentMember)
            .where(PortfolioInputEvaluationAssignmentMember.assignment_id == assignment_id)
            .with_for_update()
        )
    )
    rows.sort(key=lambda row: row.axis_index)
    if not rows or [row.axis_index for row in rows] != list(range(len(rows))):
        return None
    return tuple(rows)


def _assignment_axes(
    session: Session,
    assignment: PortfolioInputEvaluationAssignment,
    mandate: PortfolioMandateVersion,
    sealed_dataset_revision_id: UUID,
) -> tuple[_TrustedAxis, ...] | None:
    members = _assignment_members(session, assignment.id)
    if members is None:
        return None
    axes: list[_TrustedAxis] = []
    for member in members:
        qualification = session.scalar(
            select(AlphaQualification)
            .where(AlphaQualification.id == member.alpha_qualification_id)
            .with_for_update()
        )
        result = session.scalar(
            select(AlphaEvaluationResult)
            .where(AlphaEvaluationResult.id == member.alpha_evaluation_result_id)
            .with_for_update()
        )
        episode = (
            session.scalar(
                select(AlphaEvaluationEpisode)
                .where(AlphaEvaluationEpisode.id == result.episode_id)
                .with_for_update()
            )
            if result is not None
            else None
        )
        finalized_assignment = (
            _finalized_episode_assignment(session, episode, mandate)
            if episode is not None
            else None
        )
        signal = session.scalar(
            select(AlphaSignalArtifact)
            .where(
                AlphaSignalArtifact.id == member.alpha_signal_artifact_id,
                AlphaSignalArtifact.evaluation_result_id == member.alpha_evaluation_result_id,
            )
            .with_for_update()
        )
        forecast = session.get(
            AlphaEvaluationForecast,
            (member.alpha_evaluation_result_id, member.instrument_id),
            with_for_update=True,
        )
        if (
            qualification is None
            or result is None
            or episode is None
            or signal is None
            or forecast is None
            or qualification.state != "ACTIVE"
            or finalized_assignment is None
            or qualification.role != mandate.eligible_alpha_role
            or qualification.universe_version_id != mandate.universe_version_id
            or qualification.evaluation_result_id != result.id
            or qualification.evaluation_episode_id != episode.id
            or episode.sealed_dataset_revision_id != sealed_dataset_revision_id
            or result.evidence_validity != "VALID"
            or result.result != "PASS"
            or signal.mode != "CALIBRATED_RETURN"
            or signal.dataset_revision_id != sealed_dataset_revision_id
            or forecast.signal_artifact_id != signal.id
            or any(
                _finite_decimal(getattr(forecast, name)) is None
                for name in (
                    "expected_return",
                    "uncertainty",
                    "confidence",
                    "max_trade_notional",
                    "max_position_notional",
                    "max_participation_rate",
                    "days_to_liquidate",
                    "stressed_capacity_notional",
                )
            )
        ):
            return None
        axes.append(_TrustedAxis(qualification, result, signal, forecast))
    if len(axes) < mandate.minimum_alpha_count:
        return None
    return tuple(axes)


def _assignment_context(
    session: Session,
    assignment: PortfolioInputEvaluationAssignment,
) -> (
    tuple[
        PortfolioMandateVersion,
        CapitalContextVersion,
        EvaluationDatasetSelection,
        PromotionPolicyVersion,
        tuple[_TrustedAxis, ...],
    ]
    | None
):
    mandate = session.scalar(
        select(PortfolioMandateVersion)
        .where(PortfolioMandateVersion.id == assignment.mandate_version_id)
        .with_for_update()
    )
    capital = session.scalar(
        select(CapitalContextVersion)
        .where(CapitalContextVersion.id == assignment.capital_context_version_id)
        .with_for_update()
    )
    selection = session.scalar(
        select(EvaluationDatasetSelection)
        .where(EvaluationDatasetSelection.id == assignment.evaluation_dataset_selection_id)
        .with_for_update()
    )
    policy = session.scalar(
        select(PromotionPolicyVersion)
        .where(PromotionPolicyVersion.id == assignment.promotion_policy_version_id)
        .with_for_update()
    )
    if (
        mandate is None
        or capital is None
        or selection is None
        or policy is None
        or not _v1_mandate(mandate, active=False)
        or assignment.mandate_version_id != mandate.id
        or assignment.portfolio_program_id is None
        or selection.universe_version_id != mandate.universe_version_id
        or selection.sealed_dataset_revision_id != assignment.sealed_dataset_revision_id
        or not _strict_p2p_policy(session, policy)
        or not _valid_capital(capital, mandate, assignment.as_of_time)
    ):
        return None
    axes = _assignment_axes(session, assignment, mandate, selection.sealed_dataset_revision_id)
    assignment_as_of = _stored_utc(assignment.as_of_time)
    if (
        axes is None
        or assignment_as_of is None
        or any(_stored_utc(axis.forecast.as_of_time) != assignment_as_of for axis in axes)
    ):
        return None
    return mandate, capital, selection, policy, axes


def _valid_predecessor(
    session: Session, program_id: UUID, candidate_id: UUID | None
) -> PortfolioCandidate | None:
    if candidate_id is None:
        return None
    candidate = session.scalar(
        select(PortfolioCandidate)
        .where(
            PortfolioCandidate.id == candidate_id,
            PortfolioCandidate.portfolio_program_id == program_id,
        )
        .with_for_update()
    )
    if (
        candidate is None
        or candidate.state != "ASSEMBLED"
        or candidate.assembly_input_id is None
        or candidate.candidate_family_id is None
    ):
        raise _conflict(
            "PORTFOLIO_PREDECESSOR_INVALID",
            "The frozen predecessor is not an ASSEMBLED Candidate of this Program.",
        )
    return candidate


def _same_assignment_request(
    session: Session,
    assignment: PortfolioInputEvaluationAssignment,
    request: PortfolioInputEvaluationRequest,
) -> bool:
    members = _assignment_members(session, assignment.id)
    return (
        members is not None
        and assignment.previous_candidate_id == request.previous_candidate_id
        and tuple(member.alpha_qualification_id for member in members)
        == request.alpha_qualification_ids
    )


def _assignment_for_cause(
    session: Session, *, program_id: UUID, cause_event_id: int
) -> PortfolioInputEvaluationAssignment | None:
    assignments = list(
        session.scalars(
            select(PortfolioInputEvaluationAssignment)
            .where(
                PortfolioInputEvaluationAssignment.portfolio_program_id == program_id,
                PortfolioInputEvaluationAssignment.cause_event_id == cause_event_id,
            )
            .with_for_update()
        )
    )
    if len(assignments) > 1:
        raise _conflict(
            "PORTFOLIO_INPUT_ASSIGNMENT_CONFLICT",
            "Cause Event has more than one frozen Portfolio input Assignment.",
        )
    return assignments[0] if assignments else None


def _resume_input_assignment(
    session: Session, assignment: PortfolioInputEvaluationAssignment
) -> None:
    if assignment.state == "FROZEN":
        assignment.state = "QUEUED"
    if assignment.state in {"QUEUED", "RUNNING"}:
        _ensure_empty_job(
            session,
            kind="PORTFOLIO_INPUT_EVALUATION",
            resource_type="portfolio_input_evaluation_assignment",
            resource_id=assignment.id,
        )


def _freeze_input_assignment(
    session: Session,
    *,
    program: PortfolioProgram,
    mandate: PortfolioMandateVersion,
    request: PortfolioInputEvaluationRequest,
    dependencies: _InputDependencies,
) -> PortfolioInputEvaluationAssignment:
    _valid_predecessor(session, program.id, request.previous_candidate_id)
    assignment = PortfolioInputEvaluationAssignment(
        id=uuid4(),
        portfolio_program_id=program.id,
        mandate_version_id=mandate.id,
        capital_context_version_id=dependencies.capital.id,
        evaluation_dataset_selection_id=dependencies.selection.id,
        sealed_dataset_revision_id=dependencies.selection.sealed_dataset_revision_id,
        promotion_policy_version_id=dependencies.policy.id,
        cause_event_id=request.cause_event_id,
        previous_candidate_id=request.previous_candidate_id,
        as_of_time=dependencies.as_of,
        evaluator_contract_version=_EVALUATOR_CONTRACT,
        state="QUEUED",
    )
    members = tuple(
        PortfolioInputEvaluationAssignmentMember(
            assignment_id=assignment.id,
            axis_index=index,
            alpha_qualification_id=axis.qualification.id,
            alpha_evaluation_result_id=axis.result.id,
            alpha_signal_artifact_id=axis.signal.id,
            instrument_id=axis.forecast.instrument_id,
        )
        for index, axis in enumerate(dependencies.axes)
    )
    session.add_all((assignment, *members))
    session.flush()
    _ensure_empty_job(
        session,
        kind="PORTFOLIO_INPUT_EVALUATION",
        resource_type="portfolio_input_evaluation_assignment",
        resource_id=assignment.id,
    )
    return assignment


def _initial_work(
    session: Session, program_id: UUID
) -> tuple[PortfolioInputEvaluationAssignment | None, bool]:
    if session.scalar(
        select(PortfolioCandidate.id)
        .where(PortfolioCandidate.portfolio_program_id == program_id)
        .with_for_update()
    ) is not None:
        return None, True
    assignments = list(
        session.scalars(
            select(PortfolioInputEvaluationAssignment)
            .where(
                PortfolioInputEvaluationAssignment.portfolio_program_id == program_id,
                PortfolioInputEvaluationAssignment.previous_candidate_id.is_(None),
                PortfolioInputEvaluationAssignment.state.in_(("FROZEN", "QUEUED", "RUNNING")),
            )
            .with_for_update()
        )
    )
    if len(assignments) > 1:
        raise _conflict(
            "PORTFOLIO_INITIAL_ASSIGNMENT_CONFLICT",
            "Portfolio Program has more than one active initial input Assignment.",
        )
    if assignments:
        return assignments[0], True
    pending_inputs = list(
        session.scalars(
            select(PortfolioAssemblyInput)
            .where(
                PortfolioAssemblyInput.portfolio_program_id == program_id,
                PortfolioAssemblyInput.previous_candidate_id.is_(None),
                PortfolioAssemblyInput.state == "PENDING",
            )
            .with_for_update()
        )
    )
    if len(pending_inputs) > 1:
        raise _conflict(
            "PORTFOLIO_INITIAL_INPUT_CONFLICT",
            "Portfolio Program has more than one pending initial Assembly Input.",
        )
    return None, bool(pending_inputs)


def _new_passed_primary_qualification(
    session: Session, qualification_id: UUID
) -> AlphaQualification | None:
    qualification = session.scalar(
        select(AlphaQualification)
        .where(AlphaQualification.id == qualification_id)
        .with_for_update()
    )
    if (
        qualification is None
        or qualification.state != "ACTIVE"
        or qualification.role != "PRIMARY_ALPHA"
        or qualification.universe_version_id is None
        or qualification.evaluation_result_id is None
        or qualification.evaluation_episode_id is None
    ):
        return None
    result = session.scalar(
        select(AlphaEvaluationResult)
        .where(AlphaEvaluationResult.id == qualification.evaluation_result_id)
        .with_for_update()
    )
    if result is None or result.episode_id != qualification.evaluation_episode_id:
        return None
    episode = session.scalar(
        select(AlphaEvaluationEpisode)
        .where(AlphaEvaluationEpisode.id == result.episode_id)
        .with_for_update()
    )
    assignment = (
        session.scalar(
            select(AlphaEvaluationAssignment)
            .where(AlphaEvaluationAssignment.id == episode.assignment_id)
            .with_for_update()
        )
        if episode is not None and episode.assignment_id is not None
        else None
    )
    if (
        episode is None
        or assignment is None
        or result.evidence_validity != "VALID"
        or result.result != "PASS"
        or assignment.state != "FINALIZED"
        or episode.state != "DISCLOSED"
        or episode.result != "PASS"
        or assignment.universe_version_id != qualification.universe_version_id
    ):
        return None
    return qualification


def stage_initial_portfolio_input_evaluations(
    session: Session, *, qualification_id: UUID
) -> tuple[PortfolioInputEvaluationAssignment, ...]:
    """Stage only first-family Inputs after one flushed, disclosed Alpha PASS."""
    if not isinstance(qualification_id, UUID):
        raise QfError("ALPHA_QUALIFICATION_ID_INVALID", "Alpha Qualification ID is invalid.", 422)
    qualification = _new_passed_primary_qualification(session, qualification_id)
    if qualification is None:
        return ()
    mandates = tuple(
        session.scalars(
            select(PortfolioMandateVersion)
            .where(
                PortfolioMandateVersion.state == "ACTIVE",
                PortfolioMandateVersion.universe_version_id == qualification.universe_version_id,
                PortfolioMandateVersion.eligible_alpha_role == qualification.role,
            )
            .order_by(PortfolioMandateVersion.id)
            .with_for_update()
        )
    )
    staged: list[PortfolioInputEvaluationAssignment] = []
    for mandate in mandates:
        if not _v1_mandate(mandate, active=True):
            continue
        parent_mandate = session.scalar(
            select(PortfolioMandate)
            .where(PortfolioMandate.id == mandate.portfolio_mandate_id)
            .with_for_update()
        )
        if parent_mandate is None or not parent_mandate.enabled:
            continue
        existing_program = _locked_program(session, mandate)
        if existing_program is not None:
            existing, blocked = _initial_work(session, existing_program.id)
            if existing is not None:
                _resume_input_assignment(session, existing)
                staged.append(existing)
                continue
            if blocked:
                continue
        qualification_ids = tuple(
            session.scalars(
                select(AlphaQualification.id)
                .where(
                    AlphaQualification.state == "ACTIVE",
                    AlphaQualification.role == mandate.eligible_alpha_role,
                    AlphaQualification.universe_version_id == mandate.universe_version_id,
                )
                .order_by(AlphaQualification.id)
                .with_for_update()
            )
        )
        dependencies = _input_dependencies(session, mandate, qualification_ids)
        if dependencies is None:
            continue
        program, _family = _locked_program_and_family(session, mandate)
        existing, blocked = _initial_work(session, program.id)
        if existing is not None:
            _resume_input_assignment(session, existing)
            staged.append(existing)
            continue
        if blocked:
            continue
        event = Event(
            kind="PORTFOLIO_MANDATE",
            aggregate_type="PORTFOLIO_MANDATE",
            aggregate_id=parent_mandate.id,
        )
        session.add(event)
        session.flush()
        staged.append(
            _freeze_input_assignment(
                session,
                program=program,
                mandate=mandate,
                request=PortfolioInputEvaluationRequest(
                    mandate_version_id=mandate.id,
                    cause_event_id=event.id,
                    alpha_qualification_ids=qualification_ids,
                ),
                dependencies=dependencies,
            )
        )
    return tuple(staged)


def stage_portfolio_input_evaluation(
    session: Session, request: PortfolioInputEvaluationRequest
) -> PortfolioInputEvaluationAssignment | None:
    """Freeze trusted Alpha axes and all non-covariance Portfolio inputs."""
    if (
        not isinstance(request.mandate_version_id, UUID)
        or not isinstance(request.cause_event_id, int)
        or isinstance(request.cause_event_id, bool)
        or request.cause_event_id <= 0
        or request.previous_candidate_id is not None
        or any(not isinstance(item, UUID) for item in request.alpha_qualification_ids)
    ):
        raise QfError("PORTFOLIO_INPUT_REQUEST_INVALID", "Portfolio input request is invalid.", 422)
    mandate = session.scalar(
        select(PortfolioMandateVersion)
        .where(PortfolioMandateVersion.id == request.mandate_version_id)
        .with_for_update()
    )
    if mandate is None:
        raise QfError(
            "PORTFOLIO_MANDATE_VERSION_NOT_FOUND", "Portfolio Mandate Version was not found.", 404
        )
    parent_mandate = session.scalar(
        select(PortfolioMandate)
        .where(PortfolioMandate.id == mandate.portfolio_mandate_id)
        .with_for_update()
    )
    event = session.scalar(
        select(Event).where(Event.id == request.cause_event_id).with_for_update()
    )
    if parent_mandate is None or not parent_mandate.enabled:
        raise _conflict("PORTFOLIO_MANDATE_INPUT_INVALID", "Portfolio Mandate is not enabled.")
    if event is None:
        raise QfError(
            "PORTFOLIO_CAUSE_EVENT_NOT_FOUND", "Portfolio cause Event was not found.", 404
        )
    if event.aggregate_type != "PORTFOLIO_MANDATE" or event.aggregate_id != parent_mandate.id:
        raise _conflict(
            "PORTFOLIO_CAUSE_EVENT_INVALID",
            "Portfolio cause Event does not belong to the frozen Mandate.",
        )
    if not _v1_mandate(mandate, active=True):
        raise _conflict(
            "PORTFOLIO_MANDATE_INPUT_INVALID",
            "Portfolio Mandate Version is not a complete active V1 input.",
        )
    existing_program = _locked_program(session, mandate)
    if existing_program is not None:
        existing = _assignment_for_cause(
            session,
            program_id=existing_program.id,
            cause_event_id=request.cause_event_id,
        )
        if existing is not None:
            if _same_assignment_request(session, existing, request):
                _resume_input_assignment(session, existing)
                return existing
            raise _conflict(
                "PORTFOLIO_INPUT_ASSIGNMENT_CONFLICT",
                "Cause Event is already frozen with different Portfolio input facts.",
            )
    dependencies = _input_dependencies(session, mandate, request.alpha_qualification_ids)
    if dependencies is None:
        return None
    program, _family = _locked_program_and_family(session, mandate)
    existing = _assignment_for_cause(
        session,
        program_id=program.id,
        cause_event_id=request.cause_event_id,
    )
    if existing is not None:
        if _same_assignment_request(session, existing, request):
            _resume_input_assignment(session, existing)
            return existing
        raise _conflict(
            "PORTFOLIO_INPUT_ASSIGNMENT_CONFLICT",
            "Cause Event is already frozen with different Portfolio input facts.",
        )
    _existing_initial, blocked = _initial_work(session, program.id)
    if blocked:
        return None
    return _freeze_input_assignment(
        session,
        program=program,
        mandate=mandate,
        request=request,
        dependencies=dependencies,
    )


def _input_evaluation_input(
    session: Session, assignment: PortfolioInputEvaluationAssignment
) -> PortfolioInputEvaluationInput:
    context = _assignment_context(session, assignment)
    if context is None:
        raise _conflict(
            "PORTFOLIO_INPUT_EVALUATION_SOURCE_INVALID",
            "Portfolio input evaluation no longer matches its frozen source facts.",
        )
    mandate, capital, selection, policy, axes = context
    program = session.scalar(
        select(PortfolioProgram)
        .where(PortfolioProgram.id == assignment.portfolio_program_id)
        .with_for_update()
    )
    sealed_dataset = session.scalar(
        select(DatasetRevision)
        .where(DatasetRevision.id == assignment.sealed_dataset_revision_id)
        .with_for_update()
    )
    event = session.scalar(select(Event).where(Event.id == assignment.cause_event_id).with_for_update())
    as_of_time = _stored_utc(assignment.as_of_time)
    if (
        assignment.evaluator_contract_version != _EVALUATOR_CONTRACT
        or program is None
        or program.mandate_version_id != mandate.id
        or sealed_dataset is None
        or sealed_dataset.universe_version_id != selection.universe_version_id
        or event is None
        or event.aggregate_type != "PORTFOLIO_MANDATE"
        or event.aggregate_id != mandate.portfolio_mandate_id
        or as_of_time is None
    ):
        raise _conflict(
            "PORTFOLIO_INPUT_EVALUATION_SOURCE_INVALID",
            "Portfolio input evaluation no longer matches its frozen source facts.",
        )
    return PortfolioInputEvaluationInput(
        assignment_id=assignment.id,
        portfolio_program_id=program.id,
        mandate_version=ImmutableReference(mandate.id, mandate.version_no),
        capital_context_version_id=capital.id,
        evaluation_dataset_selection=ImmutableReference(selection.id, selection.version_no),
        sealed_dataset=ImmutableReference(sealed_dataset.id, sealed_dataset.revision_no),
        promotion_policy=ImmutableReference(policy.id, policy.version_no),
        cause_event_id=assignment.cause_event_id,
        previous_candidate_id=assignment.previous_candidate_id,
        as_of_time=as_of_time,
        axes=tuple(
            PortfolioInputAxis(
                axis_index=index,
                alpha_qualification_id=axis.qualification.id,
                alpha_evaluation_result_id=axis.result.id,
                alpha_signal_artifact_id=axis.signal.id,
                instrument_id=axis.forecast.instrument_id,
            )
            for index, axis in enumerate(axes)
        ),
    )


def prepare_portfolio_input_evaluation(
    session: Session, assignment_id: UUID
) -> PortfolioInputEvaluationInput:
    """Rebuild the sole covariance descriptor from locked, frozen Core facts."""
    if not isinstance(assignment_id, UUID):
        raise QfError(
            "PORTFOLIO_INPUT_EVALUATION_ASSIGNMENT_ID_INVALID",
            "Portfolio input evaluation Assignment ID is invalid.",
            422,
        )
    assignment = session.scalar(
        select(PortfolioInputEvaluationAssignment)
        .where(PortfolioInputEvaluationAssignment.id == assignment_id)
        .with_for_update()
    )
    if assignment is None:
        raise QfError(
            "PORTFOLIO_INPUT_EVALUATION_ASSIGNMENT_NOT_FOUND",
            "Portfolio input evaluation Assignment was not found.",
            404,
        )
    if assignment.state not in {"QUEUED", "RUNNING"}:
        raise _conflict(
            "PORTFOLIO_INPUT_EVALUATION_STATE_CONFLICT",
            "Portfolio input evaluation is not awaiting trusted work.",
        )
    input = _input_evaluation_input(session, assignment)
    assignment.state = "RUNNING"
    return input


def accept_portfolio_input_evaluation_result(
    session: Session, result: EvaluatorPortfolioInputEvaluationResult
) -> PortfolioAssemblyInput | None:
    """Accept only the exact typed covariance result for the frozen descriptor."""
    if not isinstance(result, EvaluatorPortfolioInputEvaluationResult):
        raise QfError(
            "PORTFOLIO_INPUT_EVALUATOR_RESULT_INVALID", "Evaluator result is invalid.", 422
        )
    assignment = session.scalar(
        select(PortfolioInputEvaluationAssignment)
        .where(PortfolioInputEvaluationAssignment.id == result.input.assignment_id)
        .with_for_update()
    )
    if assignment is None:
        raise QfError(
            "PORTFOLIO_INPUT_EVALUATION_ASSIGNMENT_NOT_FOUND",
            "Portfolio input evaluation Assignment was not found.",
            404,
        )
    if assignment.state not in {"QUEUED", "RUNNING", "VALID"}:
        raise _conflict(
            "PORTFOLIO_INPUT_EVALUATION_STATE_CONFLICT",
            "Portfolio input evaluation is not awaiting a trusted result.",
        )
    if result.input != _input_evaluation_input(session, assignment):
        raise _conflict(
            "PORTFOLIO_INPUT_EVALUATION_INPUT_MISMATCH",
            "Portfolio evaluator result does not match frozen assignment inputs.",
        )
    return persist_portfolio_input_evaluation(
        session,
        PortfolioInputEvaluationResult(
            assignment_id=result.input.assignment_id,
            private_result_ref=result.private_result_id,
            evaluated_at=result.evaluated_at,
            covariance_method=result.covariance_method.value,
            covariance_observations=result.covariance_observations,
            covariance_decay=Decimal(str(result.covariance_decay)),
            covariance_shrinkage=Decimal(str(result.covariance_shrinkage)),
            covariance_upper_triangle=tuple(
                PortfolioCovariance(
                    item.left_axis_index,
                    item.right_axis_index,
                    Decimal(str(item.covariance)),
                )
                for item in result.covariance_upper_triangle
            ),
        ),
    )


def _result_header_valid(result: PortfolioInputEvaluationResult) -> bool:
    decay = _canonical_numeric(result.covariance_decay)
    shrinkage = _canonical_numeric(result.covariance_shrinkage)
    return (
        isinstance(result.assignment_id, UUID)
        and isinstance(result.private_result_ref, UUID)
        and _valid_datetime(result.evaluated_at) is not None
        and result.covariance_method == _COVARIANCE_METHOD
        and isinstance(result.covariance_observations, int)
        and not isinstance(result.covariance_observations, bool)
        and result.covariance_observations >= 2
        and decay is not None
        and Decimal("0") < decay < Decimal("1")
        and shrinkage is not None
        and Decimal("0") <= shrinkage <= Decimal("1")
    )


def _covariance_matrix(
    items: tuple[PortfolioCovariance, ...], count: int
) -> tuple[tuple[Decimal, ...], ...] | None:
    expected = {(left, right) for left in range(count) for right in range(left, count)}
    values: dict[tuple[int, int], Decimal] = {}
    for item in items:
        if (
            not isinstance(item, PortfolioCovariance)
            or not isinstance(item.left_axis_index, int)
            or isinstance(item.left_axis_index, bool)
            or not isinstance(item.right_axis_index, int)
            or isinstance(item.right_axis_index, bool)
            or item.left_axis_index < 0
            or item.right_axis_index < item.left_axis_index
            or item.right_axis_index >= count
            or _canonical_numeric(item.covariance) is None
        ):
            return None
        covariance = _canonical_numeric(item.covariance)
        assert covariance is not None
        key = (item.left_axis_index, item.right_axis_index)
        if key in values or key not in expected or (key[0] == key[1] and covariance < 0):
            return None
        values[key] = covariance
    if set(values) != expected:
        return None
    matrix = tuple(
        tuple(values[(min(left, right), max(left, right))] for right in range(count))
        for left in range(count)
    )
    return matrix


def _complete_assignment(
    session: Session,
    assignment: PortfolioInputEvaluationAssignment,
    result: PortfolioInputEvaluationResult,
    *,
    state: str,
    outcome_code: str,
    input_id: UUID | None = None,
) -> None:
    assignment.state = state
    assignment.private_result_ref = result.private_result_ref
    assignment.evaluated_at = result.evaluated_at
    assignment.outcome_code = outcome_code
    assignment.completed_at = datetime.now(UTC)
    _record_ledger(
        session,
        portfolio_program_id=assignment.portfolio_program_id,
        cause_event_id=assignment.cause_event_id,
        attempt_type="INPUT_EVALUATION",
        outcome_class=state,
        reason_code=outcome_code,
        portfolio_assembly_input_id=input_id,
    )


def _previous_weights(
    session: Session,
    predecessor: PortfolioCandidate | None,
    axes: tuple[_TrustedAxis, ...],
) -> tuple[Decimal, ...] | None:
    if predecessor is None:
        return tuple(Decimal("0") for _ in axes)
    rows = list(
        session.scalars(
            select(PortfolioCandidateMember)
            .where(PortfolioCandidateMember.candidate_id == predecessor.id)
            .with_for_update()
        )
    )
    weights = {row.alpha_qualification_id: row.target_weight for row in rows}
    if any(
        _finite_decimal(weight) is None or weight < 0 or weight > 1 for weight in weights.values()
    ):
        return None
    return tuple(weights.get(axis.qualification.id, Decimal("0")) for axis in axes)


def _same_persisted_result(
    session: Session,
    assignment: PortfolioInputEvaluationAssignment,
    input_row: PortfolioAssemblyInput,
    result: PortfolioInputEvaluationResult,
) -> bool:
    members = _input_members(session, input_row.id)
    proposed = _covariance_matrix(result.covariance_upper_triangle, len(members or ()))
    stored = _input_matrix(session, input_row.id, len(members or ()))
    decay = _canonical_numeric(result.covariance_decay)
    shrinkage = _canonical_numeric(result.covariance_shrinkage)
    return (
        members is not None
        and proposed is not None
        and stored == proposed
        and assignment.private_result_ref == result.private_result_ref
        and _stored_utc(assignment.evaluated_at) == result.evaluated_at
        and input_row.covariance_method == result.covariance_method
        and input_row.covariance_observations == result.covariance_observations
        and input_row.covariance_decay == decay
        and input_row.covariance_shrinkage == shrinkage
    )


def persist_portfolio_input_evaluation(
    session: Session, result: PortfolioInputEvaluationResult
) -> PortfolioAssemblyInput | None:
    """Accept one typed evaluator result and write a complete immutable Input."""
    if not _result_header_valid(result):
        raise QfError(
            "PORTFOLIO_INPUT_EVALUATOR_RESULT_INVALID", "Evaluator result is invalid.", 422
        )
    assignment = session.scalar(
        select(PortfolioInputEvaluationAssignment)
        .where(PortfolioInputEvaluationAssignment.id == result.assignment_id)
        .with_for_update()
    )
    if assignment is None:
        raise QfError(
            "PORTFOLIO_INPUT_EVALUATION_ASSIGNMENT_NOT_FOUND",
            "Portfolio input evaluation Assignment was not found.",
            404,
        )
    existing_inputs = list(
        session.scalars(
            select(PortfolioAssemblyInput)
            .where(PortfolioAssemblyInput.portfolio_input_evaluation_assignment_id == assignment.id)
            .with_for_update()
        )
    )
    if assignment.state == "VALID":
        input_row = _only(existing_inputs)
        if isinstance(input_row, PortfolioAssemblyInput) and _same_persisted_result(
            session, assignment, input_row, result
        ):
            if input_row.state == "PENDING":
                _ensure_empty_job(
                    session,
                    kind="PORTFOLIO_ASSEMBLY",
                    resource_type="portfolio_assembly_input",
                    resource_id=input_row.id,
                )
            return input_row
        raise _conflict(
            "PORTFOLIO_INPUT_EVALUATION_RESULT_CONFLICT",
            "Portfolio input evaluation already completed with a different result.",
        )
    if assignment.state not in {"FROZEN", "QUEUED", "RUNNING"} or existing_inputs:
        raise _conflict(
            "PORTFOLIO_INPUT_EVALUATION_STATE_CONFLICT",
            "Portfolio input evaluation is not awaiting a trusted result.",
        )
    context = _assignment_context(session, assignment)
    if context is None:
        _complete_assignment(
            session,
            assignment,
            result,
            state="INVALID",
            outcome_code="FROZEN_INPUT_INVALID",
        )
        return None
    mandate, _capital, _selection, _policy, axes = context
    program = session.scalar(
        select(PortfolioProgram)
        .where(PortfolioProgram.id == assignment.portfolio_program_id)
        .with_for_update()
    )
    if program is None:
        raise _conflict("PORTFOLIO_PROGRAM_MISSING", "Frozen Portfolio Program is missing.")
    covariance = _covariance_matrix(result.covariance_upper_triangle, len(axes))
    if covariance is None:
        _complete_assignment(
            session,
            assignment,
            result,
            state="INVALID",
            outcome_code="COVARIANCE_INCOMPLETE_OR_INVALID",
        )
        return None
    predecessor = _valid_predecessor(
        session, assignment.portfolio_program_id, assignment.previous_candidate_id
    )
    previous_weights = _previous_weights(session, predecessor, axes)
    if previous_weights is None:
        _complete_assignment(
            session,
            assignment,
            result,
            state="INVALID",
            outcome_code="PREDECESSOR_MEMBERS_INVALID",
        )
        return None
    effective_from_values = [_stored_utc(axis.forecast.effective_from) for axis in axes]
    finite_until_values = [
        value
        for axis in axes
        if axis.forecast.effective_until is not None
        for value in (_stored_utc(axis.forecast.effective_until),)
    ]
    if any(value is None for value in effective_from_values) or any(
        value is None for value in finite_until_values
    ):
        _complete_assignment(
            session,
            assignment,
            result,
            state="INVALID",
            outcome_code="FORECAST_WINDOW_EMPTY",
        )
        return None
    effective_from = max(value for value in effective_from_values if value is not None)
    finite_until = [value for value in finite_until_values if value is not None]
    effective_until = min(finite_until) if finite_until else None
    if effective_until is not None and effective_until < effective_from:
        _complete_assignment(
            session,
            assignment,
            result,
            state="INVALID",
            outcome_code="FORECAST_WINDOW_EMPTY",
        )
        return None
    covariance_decay = _canonical_numeric(result.covariance_decay)
    covariance_shrinkage = _canonical_numeric(result.covariance_shrinkage)
    assert covariance_decay is not None
    assert covariance_shrinkage is not None
    input_row = PortfolioAssemblyInput(
        id=uuid4(),
        portfolio_input_evaluation_assignment_id=assignment.id,
        portfolio_program_id=assignment.portfolio_program_id,
        mandate_version_id=mandate.id,
        capital_context_version_id=assignment.capital_context_version_id,
        universe_version_id=mandate.universe_version_id,
        promotion_policy_version_id=assignment.promotion_policy_version_id,
        cause_event_id=assignment.cause_event_id,
        snapshot_no=assignment.cause_event_id,
        input_contract_version=_INPUT_CONTRACT,
        as_of_time=assignment.as_of_time,
        effective_from=effective_from,
        effective_until=effective_until,
        previous_candidate_id=assignment.previous_candidate_id,
        covariance_method=result.covariance_method,
        covariance_observations=result.covariance_observations,
        covariance_decay=covariance_decay,
        covariance_shrinkage=covariance_shrinkage,
        minimum_alpha_count=mandate.minimum_alpha_count,
        minimum_weight=mandate.minimum_weight,
        maximum_weight=mandate.maximum_weight,
        gross_exposure_limit=mandate.gross_exposure_limit,
        net_exposure_target=mandate.net_exposure_target,
        cash_reserve=mandate.cash_reserve,
        turnover_limit=mandate.turnover_limit,
        variance_limit=mandate.variance_limit,
        risk_aversion=mandate.risk_aversion,
        cost_aversion=mandate.cost_aversion,
        uncertainty_aversion=mandate.uncertainty_aversion,
        commission_rate=mandate.commission_rate,
        half_spread_rate=mandate.half_spread_rate,
        slippage_rate=mandate.slippage_rate,
        impact_rate=mandate.impact_rate,
        impact_breakpoint=mandate.impact_breakpoint,
        state="PENDING",
    )
    input_members = [
        PortfolioAssemblyInputMember(
            input_id=input_row.id,
            axis_index=index,
            alpha_qualification_id=axis.qualification.id,
            alpha_evaluation_result_id=axis.result.id,
            alpha_signal_artifact_id=axis.signal.id,
            instrument_id=axis.forecast.instrument_id,
            expected_return=axis.forecast.expected_return,
            uncertainty=axis.forecast.uncertainty,
            confidence=axis.forecast.confidence,
            previous_weight=previous_weight,
            max_trade_notional=axis.forecast.max_trade_notional,
            max_position_notional=axis.forecast.max_position_notional,
            max_participation_rate=axis.forecast.max_participation_rate,
            days_to_liquidate=axis.forecast.days_to_liquidate,
            stressed_capacity=axis.forecast.stressed_capacity_notional,
        )
        for index, (axis, previous_weight) in enumerate(zip(axes, previous_weights, strict=True))
    ]
    covariance_rows = [
        PortfolioAssemblyInputCovariance(
            input_id=input_row.id,
            left_axis_index=left,
            right_axis_index=right,
            covariance=covariance[left][right],
        )
        for left in range(len(axes))
        for right in range(left, len(axes))
    ]
    session.add_all((input_row, *input_members))
    session.flush()
    assignment.state = "VALID"
    assignment.private_result_ref = result.private_result_ref
    assignment.evaluated_at = result.evaluated_at
    assignment.outcome_code = "INPUT_COMPLETE"
    assignment.completed_at = datetime.now(UTC)
    program.state = "ACTIVE"
    session.add_all(covariance_rows)
    session.flush()
    _ensure_empty_job(
        session,
        kind="PORTFOLIO_ASSEMBLY",
        resource_type="portfolio_assembly_input",
        resource_id=input_row.id,
    )
    return input_row


def _input_members(
    session: Session, input_id: UUID
) -> tuple[PortfolioAssemblyInputMember, ...] | None:
    rows = list(
        session.scalars(
            select(PortfolioAssemblyInputMember)
            .where(PortfolioAssemblyInputMember.input_id == input_id)
            .with_for_update()
        )
    )
    rows.sort(key=lambda row: row.axis_index)
    if not rows or [row.axis_index for row in rows] != list(range(len(rows))):
        return None
    return tuple(rows)


def _input_matrix(
    session: Session, input_id: UUID, count: int
) -> tuple[tuple[Decimal, ...], ...] | None:
    rows = list(
        session.scalars(
            select(PortfolioAssemblyInputCovariance)
            .where(PortfolioAssemblyInputCovariance.input_id == input_id)
            .with_for_update()
        )
    )
    return _covariance_matrix(
        tuple(
            PortfolioCovariance(row.left_axis_index, row.right_axis_index, row.covariance)
            for row in rows
        ),
        count,
    )


def _complete_input(
    session: Session,
    input_row: PortfolioAssemblyInput,
    *,
    state: str,
    outcome_code: str,
) -> None:
    input_row.state = state
    input_row.outcome_code = outcome_code
    input_row.completed_at = datetime.now(UTC)
    _record_ledger(
        session,
        portfolio_program_id=input_row.portfolio_program_id,
        cause_event_id=input_row.cause_event_id,
        attempt_type="ASSEMBLY",
        outcome_class=state,
        reason_code=outcome_code,
        portfolio_assembly_input_id=input_row.id,
    )


def assemble_trusted_portfolio_input(session: Session, input_id: UUID) -> PortfolioCandidate | None:
    """Run the deterministic optimizer only from one complete relational Input."""
    if not isinstance(input_id, UUID):
        raise QfError(
            "PORTFOLIO_ASSEMBLY_INPUT_INVALID", "Portfolio Assembly Input ID is invalid.", 422
        )
    input_row = session.scalar(
        select(PortfolioAssemblyInput)
        .where(PortfolioAssemblyInput.id == input_id)
        .with_for_update()
    )
    if input_row is None:
        raise QfError(
            "PORTFOLIO_ASSEMBLY_INPUT_NOT_FOUND", "Portfolio Assembly Input was not found.", 404
        )
    candidates = list(
        session.scalars(
            select(PortfolioCandidate)
            .where(PortfolioCandidate.assembly_input_id == input_row.id)
            .with_for_update()
        )
    )
    existing = _only(candidates)
    if isinstance(existing, PortfolioCandidate):
        if input_row.state == "ASSEMBLED" and existing.state == "ASSEMBLED":
            ensure_portfolio_evaluation(session, candidate_id=existing.id)
            enqueue_candidate_package_build(session, existing.id)
            return existing
        raise _conflict(
            "PORTFOLIO_ASSEMBLY_CONFLICT", "Input and Candidate terminal states disagree."
        )
    if input_row.state != "PENDING":
        raise _conflict(
            "PORTFOLIO_ASSEMBLY_STATE_CONFLICT", "Portfolio Assembly Input is not pending."
        )
    assignment = session.scalar(
        select(PortfolioInputEvaluationAssignment)
        .where(
            PortfolioInputEvaluationAssignment.id
            == input_row.portfolio_input_evaluation_assignment_id
        )
        .with_for_update()
    )
    context = _assignment_context(session, assignment) if assignment is not None else None
    if (
        context is None
        or assignment is None
        or assignment.state != "VALID"
        or input_row.portfolio_program_id != assignment.portfolio_program_id
        or input_row.mandate_version_id != assignment.mandate_version_id
        or input_row.capital_context_version_id != assignment.capital_context_version_id
        or input_row.promotion_policy_version_id != assignment.promotion_policy_version_id
        or input_row.cause_event_id != assignment.cause_event_id
        or input_row.previous_candidate_id != assignment.previous_candidate_id
        or _stored_utc(input_row.as_of_time) != _stored_utc(assignment.as_of_time)
    ):
        _complete_input(session, input_row, state="INVALID", outcome_code="FROZEN_INPUT_INVALID")
        return None
    mandate, capital, _selection, _policy, axes = context
    members = _input_members(session, input_row.id)
    if (
        members is None
        or len(members) != len(axes)
        or tuple(
            (
                member.alpha_qualification_id,
                member.alpha_evaluation_result_id,
                member.alpha_signal_artifact_id,
                member.instrument_id,
            )
            for member in members
        )
        != tuple(
            (axis.qualification.id, axis.result.id, axis.signal.id, axis.forecast.instrument_id)
            for axis in axes
        )
    ):
        _complete_input(session, input_row, state="INVALID", outcome_code="INPUT_MEMBERS_INVALID")
        return None
    matrix = _input_matrix(session, input_row.id, len(members))
    if matrix is None:
        _complete_input(
            session, input_row, state="INVALID", outcome_code="COVARIANCE_INCOMPLETE_OR_INVALID"
        )
        return None
    predecessor = _valid_predecessor(
        session, input_row.portfolio_program_id, input_row.previous_candidate_id
    )
    expected_previous = _previous_weights(session, predecessor, axes)
    if (
        expected_previous is None
        or tuple(member.previous_weight for member in members) != expected_previous
    ):
        _complete_input(
            session, input_row, state="INVALID", outcome_code="PREDECESSOR_MEMBERS_INVALID"
        )
        return None
    if any(
        (
            member.expected_return != axis.forecast.expected_return
            or member.uncertainty != axis.forecast.uncertainty
            or member.confidence != axis.forecast.confidence
            or member.max_trade_notional != axis.forecast.max_trade_notional
            or member.max_position_notional != axis.forecast.max_position_notional
            or member.max_participation_rate != axis.forecast.max_participation_rate
            or member.days_to_liquidate != axis.forecast.days_to_liquidate
            or member.stressed_capacity != axis.forecast.stressed_capacity_notional
        )
        for member, axis in zip(members, axes, strict=True)
    ):
        _complete_input(session, input_row, state="INVALID", outcome_code="INPUT_MEMBERS_INVALID")
        return None
    try:
        from portfolio_engine import (
            CapacityEstimate,
            CostModel,
            EligibleAlpha,
            OptimizationInput,
            OptimizationStatus,
            PortfolioConstraints,
            optimize_portfolio,
        )
    except ImportError as error:
        raise QfError(
            "PORTFOLIO_ASSEMBLY_ENGINE_UNAVAILABLE",
            "Portfolio assembly requires the research runtime.",
            503,
        ) from error
    try:
        optimization = optimize_portfolio(
            OptimizationInput(
                eligible_alphas=tuple(
                    EligibleAlpha(
                        str(member.alpha_qualification_id),
                        float(member.expected_return),
                        float(member.uncertainty),
                    )
                    for member in members
                ),
                covariance=tuple(tuple(float(value) for value in row) for row in matrix),
                capital=float(capital.deployable_capital),
                previous_weights=tuple(float(member.previous_weight) for member in members),
                constraints=PortfolioConstraints(
                    minimum_alpha_count=input_row.minimum_alpha_count,
                    minimum_weight=float(input_row.minimum_weight),
                    maximum_weight=float(input_row.maximum_weight),
                    gross_exposure_limit=float(input_row.gross_exposure_limit),
                    cash_reserve=float(input_row.cash_reserve),
                    net_exposure_target=float(input_row.net_exposure_target),
                    turnover_limit=float(input_row.turnover_limit),
                    variance_limit=float(input_row.variance_limit),
                ),
                cost_model=CostModel(
                    commission_rate=float(input_row.commission_rate),
                    half_spread_rate=float(input_row.half_spread_rate),
                    slippage_rate=float(input_row.slippage_rate),
                    impact_rate=float(input_row.impact_rate),
                    impact_breakpoint=float(input_row.impact_breakpoint),
                ),
                capacities=tuple(
                    CapacityEstimate(
                        max_trade_notional=float(member.max_trade_notional),
                        max_position_notional=float(member.max_position_notional),
                        max_participation_rate=float(member.max_participation_rate),
                        days_to_liquidate=float(member.days_to_liquidate),
                        stressed_capacity=float(member.stressed_capacity),
                    )
                    for member in members
                ),
                risk_aversion=float(input_row.risk_aversion),
                cost_aversion=float(input_row.cost_aversion),
                uncertainty_aversion=float(input_row.uncertainty_aversion),
            )
        )
    except TypeError, ValueError:
        _complete_input(session, input_row, state="INVALID", outcome_code="OPTIMIZER_INPUT_INVALID")
        return None
    if any(item.code in {"CVXPY_UNAVAILABLE", "SOLVER_ERROR"} for item in optimization.diagnostics):
        raise QfError(
            "PORTFOLIO_ASSEMBLY_ENGINE_UNAVAILABLE",
            "Portfolio assembly engine is unavailable; the Input remains pending.",
            503,
        )
    if optimization.status is not OptimizationStatus.OPTIMAL:
        _complete_input(session, input_row, state="INFEASIBLE", outcome_code="OPTIMIZER_INFEASIBLE")
        return None
    weights = {item.alpha_id: item.target_weight for item in optimization.target_weights}
    expected_ids = tuple(str(member.alpha_qualification_id) for member in members)
    if (
        set(weights) != set(expected_ids)
        or len(weights) != len(expected_ids)
        or any(not isfinite(weight) or weight < 0 or weight > 1 for weight in weights.values())
        or not isclose(sum(weights.values()), 1.0, rel_tol=0.0, abs_tol=1e-6)
    ):
        _complete_input(
            session, input_row, state="INVALID", outcome_code="OPTIMIZER_OUTPUT_INVALID"
        )
        return None
    families = list(
        session.scalars(
            select(PortfolioCandidateFamily)
            .where(
                PortfolioCandidateFamily.portfolio_program_id == input_row.portfolio_program_id,
                PortfolioCandidateFamily.mandate_version_id == input_row.mandate_version_id,
            )
            .with_for_update()
        )
    )
    family = _only(families)
    if not isinstance(family, PortfolioCandidateFamily):
        _complete_input(
            session, input_row, state="INVALID", outcome_code="CANDIDATE_FAMILY_INVALID"
        )
        return None
    candidate = PortfolioCandidate(
        id=uuid4(),
        candidate_family_id=family.id,
        portfolio_program_id=input_row.portfolio_program_id,
        mandate_version_id=input_row.mandate_version_id,
        capital_context_version_id=input_row.capital_context_version_id,
        assembly_input_id=input_row.id,
        universe_version_id=input_row.universe_version_id,
        state="ASSEMBLED",
        created_at=datetime.now(UTC),
    )
    candidate_members = [
        PortfolioCandidateMember(
            candidate_id=candidate.id,
            alpha_qualification_id=member.alpha_qualification_id,
            role="PRIMARY_ALPHA",
            target_weight=Decimal(str(weights[str(member.alpha_qualification_id)])),
        )
        for member in members
    ]
    input_row.state = "ASSEMBLED"
    input_row.outcome_code = "OPTIMAL"
    input_row.completed_at = datetime.now(UTC)
    session.add_all((candidate, *candidate_members))
    session.flush()
    ensure_portfolio_evaluation(session, candidate_id=candidate.id)
    enqueue_candidate_package_build(session, candidate.id)
    return candidate
