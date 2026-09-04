"""Public Research Intelligence and Portfolio Construction API."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from decimal import Decimal
from typing import Any, Callable
from uuid import UUID

from fastapi import APIRouter, Header, Request
from fastapi.responses import FileResponse
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from candidate_packages import is_trusted_candidate_package, resolve_package_archive
from codex_chatgpt_auth import codex_auth_readiness
from degradation_engine import SubjectType
from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    CandidatePackage,
    DatasetRevision,
    DownstreamConnectionVersion,
    DownstreamSystem,
    Event,
    ForwardEvidenceEpisode,
    HandoffOffer,
    PortfolioCandidate,
    PortfolioCandidateMember,
    PortfolioMandate,
    PortfolioProgram,
    PreflightReceipt,
    FeedbackContractVersion,
    PublicMutationReceipt,
)
from downstream_auth import authenticate_downstream
from downstream_contracts import feedback_contract_snapshot, is_current_downstream_preflight
from errors import QfError
from research_lifecycle import record_degradation_observation
from promotion_service import (
    FeedbackHeader,
    TypedFeedbackMetric,
    accept_live_feedback,
    accept_paper_feedback,
    approve_typed_live_handoff,
    approve_typed_paper_handoff,
)

router = APIRouter(prefix="/api/v1", tags=["domain"])


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class AlphaView(StrictModel):
    id: UUID
    alpha_model_version_id: UUID | None = None
    calibration_version_id: UUID | None = None
    universe_version_id: UUID | None = None
    universe: str | None = None
    horizon: str | None = None
    role: str
    state: str
    name: str | None = None
    scope_json: dict[str, Any] = Field(default_factory=dict)
    evaluation_episode_id: UUID | None = None
    created_at: str | None = None
    degradation_state: str | None = None
    metrics: dict[str, Any] = Field(default_factory=dict)
    lineage: list[dict[str, Any]] = Field(default_factory=list)


class MandateView(StrictModel):
    id: UUID
    key: str
    name: str
    enabled: bool
    latest_version_id: UUID
    spec_json: dict[str, Any] = Field(default_factory=dict)
    state: str


class PortfolioProgramView(StrictModel):
    id: UUID
    mandate_version_id: UUID
    mandate_name: str | None = None
    state: str
    created_at: str | None = None
    updated_at: str | None = None
    candidate_count: int = 0
    current_candidate_id: UUID | None = None


class CandidateView(StrictModel):
    id: UUID
    candidate_family_id: UUID | None = None
    portfolio_program_id: UUID
    mandate_version_id: UUID | None = None
    mandate_name: str | None = None
    capital_context_version_id: UUID | None = None
    universe_set_json: list[str] | dict[str, Any] = Field(default_factory=list)
    policy_version: str | None = None
    risk_model_version: str | None = None
    cost_model_version: str | None = None
    capacity_model_version: str | None = None
    constraint_set_version: str | None = None
    rebalance_policy_version: str | None = None
    evaluation_episode_id: UUID | None = None
    state: str
    created_at: str | None = None
    members: list[dict[str, Any]] = Field(default_factory=list)
    metrics: dict[str, Any] = Field(default_factory=dict)


class ApprovalView(StrictModel):
    id: UUID
    candidate_id: UUID
    candidate_package_id: UUID | None = None
    candidate_package_revision: int | None = None
    promotion_evaluation_id: UUID | None = None
    promotion_purpose: str | None = None
    candidate: CandidateView
    purpose: str
    state: str
    downstream_system_id: UUID | None = None
    downstream_name: str | None = None
    created_at: str | None = None
    valid_until: str | None = None
    expires_at: str | None = None
    stale_reason: str | None = None
    recommendation_rationale: str | None = None
    human_report: dict[str, Any] | str | None = None
    evidence_summary: dict[str, Any] = Field(default_factory=dict)
    capital_context: dict[str, Any] = Field(default_factory=dict)
    risk_summary: dict[str, Any] = Field(default_factory=dict)
    cost_summary: dict[str, Any] = Field(default_factory=dict)
    capacity_summary: dict[str, Any] = Field(default_factory=dict)
    changes_summary: dict[str, Any] = Field(default_factory=dict)


class ApprovalApproveInput(StrictModel):
    # Kept optional for legacy clients; typed Approvals reject any attempted
    # target override and use the frozen Core binding instead.
    downstream_system_id: UUID | None = None
    expected_state: str


class ApprovalRejectInput(StrictModel):
    reason_code: str = Field(min_length=1, max_length=100)
    note: str | None = Field(default=None, max_length=4000)
    expected_state: str


class HandoffView(StrictModel):
    id: UUID
    approval_id: UUID
    candidate_package_id: UUID
    candidate_id: UUID
    purpose: str
    downstream_system_id: UUID
    downstream_name: str | None = None
    state: str
    claim_deadline: str | None = None
    package_contract_version: str
    feedback_contract_version: str
    created_at: str | None = None
    updated_at: str | None = None
    stale_reason: str | None = None
    feedback_state: str | None = None


class HandoffRevokeInput(StrictModel):
    reason_code: str = Field(min_length=1, max_length=100)


class HandoffStateInput(StrictModel):
    expected_state: str | None = None
    reason_code: str | None = None


class FeedbackMetricInput(StrictModel):
    metric_code: str = Field(min_length=1, max_length=100)
    status: str
    value: Decimal | None = None


class FeedbackInput(StrictModel):
    state: str = "FEEDBACK_COMPLETE"
    observation_start: datetime | None = None
    observation_end: datetime | None = None
    sample_size: int | None = Field(default=None, ge=0)
    metrics: list[FeedbackMetricInput] = Field(default_factory=list)
    # Legacy display-only evidence is accepted only on historical handoffs;
    # typed production handoffs reject it before any state mutation.
    evidence: dict[str, Any] = Field(default_factory=dict)


class DegradationObservationInput(StrictModel):
    subject_type: SubjectType
    subject_id: UUID
    metric_name: str = Field(min_length=1, max_length=100)
    severity: Decimal = Field(ge=0, le=1)
    confidence: Decimal = Field(default=Decimal("1"), ge=0, le=1)


class DegradationObservationView(StrictModel):
    id: UUID
    forward_evidence_episode_id: UUID
    subject_type: SubjectType
    subject_id: UUID
    metric_name: str
    state: str
    evaluated: bool
    wake_event_id: UUID | None = None
    wake_state: str | None = None
    cycle_id: UUID | None = None


def _now() -> datetime:
    return datetime.now(UTC)


def _stored_utc(value: datetime) -> datetime:
    return value.replace(tzinfo=UTC) if value.tzinfo is None else value.astimezone(UTC)


def _iso(value: datetime | None) -> str | None:
    if value is None:
        return None
    if value.tzinfo is None:
        value = value.replace(tzinfo=UTC)
    return value.isoformat()


def _is_past(value: datetime | None) -> bool:
    if value is None:
        return False
    if value.tzinfo is None:
        return value < datetime.now().replace(tzinfo=None)
    return value < _now()


def _normalize(value: BaseModel | dict[str, Any]) -> dict[str, Any]:
    if isinstance(value, BaseModel):
        return value.model_dump(mode="json", exclude_none=True)
    return value


def _event(
    session: Session,
    kind: str,
    aggregate_type: str,
    aggregate_id: UUID | None,
    payload: dict[str, Any] | None = None,
    *,
    actor_kind: str = "SYSTEM",
) -> None:
    session.add(
        Event(
            kind=kind,
            aggregate_type=aggregate_type,
            aggregate_id=aggregate_id,
            actor_kind=actor_kind,
            actor_metadata={},
            payload=payload or {},
        )
    )


def _idempotent(
    session: Session,
    key: str | None,
    operation: str,
    payload: BaseModel | dict[str, Any],
    action: Callable[[], dict[str, Any]],
    *,
    status_code: int = 200,
) -> dict[str, Any]:
    normalized = _normalize(payload)
    if key:
        existing = session.get(PublicMutationReceipt, key)
        if existing is not None:
            if existing.operation_name != operation or existing.normalized_request != normalized:
                raise QfError(
                    "IDEMPOTENCY_KEY_REUSED",
                    "The idempotency key belongs to a different request.",
                    409,
                )
            return existing.response_json
    result = action()
    if key:
        session.add(
            PublicMutationReceipt(
                idempotency_key=key,
                operation_name=operation,
                normalized_request=normalized,
                response_json=result,
                status_code=status_code,
                created_at=_now(),
            )
        )
    return result


def _candidate_view(session: Session, item: PortfolioCandidate) -> CandidateView:
    universe_set = item.universe_set_json
    if not isinstance(universe_set, (list, dict)):
        universe_set = []
    relational_members = list(
        session.scalars(
            select(PortfolioCandidateMember)
            .where(PortfolioCandidateMember.candidate_id == item.id)
            .order_by(PortfolioCandidateMember.alpha_qualification_id)
        )
    )
    members = [
        {
            "alpha_qualification_id": str(member.alpha_qualification_id),
            "role": member.role,
            "target_weight": float(member.target_weight),
        }
        for member in relational_members
    ]
    if not members and not (item.state == "ASSEMBLED" and item.assembly_input_id is not None):
        members = item.members
    return CandidateView(
        id=item.id,
        candidate_family_id=item.candidate_family_id,
        portfolio_program_id=item.portfolio_program_id,
        mandate_version_id=item.mandate_version_id,
        mandate_name=item.mandate_name,
        capital_context_version_id=item.capital_context_version_id,
        universe_set_json=universe_set,
        policy_version=item.policy_version,
        risk_model_version=item.risk_model_version,
        cost_model_version=item.cost_model_version,
        capacity_model_version=item.capacity_model_version,
        constraint_set_version=item.constraint_set_version,
        rebalance_policy_version=item.rebalance_policy_version,
        evaluation_episode_id=item.evaluation_episode_id,
        state=item.state,
        created_at=_iso(item.created_at),
        members=members,
        metrics=item.metrics,
    )


def _alpha_view(item: AlphaQualification) -> AlphaView:
    return AlphaView(
        id=item.id,
        alpha_model_version_id=item.alpha_model_version_id,
        calibration_version_id=item.calibration_version_id,
        universe_version_id=item.universe_version_id,
        universe=item.universe,
        horizon=item.horizon,
        role=item.role,
        state=item.state,
        name=item.name,
        scope_json=item.scope_json,
        evaluation_episode_id=item.evaluation_episode_id,
        created_at=_iso(item.created_at),
        degradation_state=item.degradation_state,
        metrics=item.metrics,
        lineage=item.lineage,
    )


def _approval_view(session: Session, item: ApprovalSnapshot) -> ApprovalView:
    candidate = session.get(PortfolioCandidate, item.candidate_id)
    if candidate is None:
        raise QfError("CANDIDATE_NOT_FOUND", "Approval candidate is missing.", 500)
    downstream_name = None
    if item.downstream_system_id:
        downstream = session.get(DownstreamSystem, item.downstream_system_id)
        downstream_name = downstream.name if downstream else None
    return ApprovalView(
        id=item.id,
        candidate_id=item.candidate_id,
        candidate_package_id=item.candidate_package_id,
        candidate_package_revision=item.candidate_package_revision,
        promotion_evaluation_id=item.promotion_evaluation_id,
        promotion_purpose=item.promotion_purpose,
        candidate=_candidate_view(session, candidate),
        purpose=item.purpose,
        state=item.state,
        downstream_system_id=item.downstream_system_id,
        downstream_name=downstream_name,
        created_at=_iso(item.created_at),
        valid_until=_iso(item.valid_until),
        expires_at=_iso(item.expires_at),
        stale_reason=item.stale_reason,
        recommendation_rationale=item.recommendation_rationale,
        human_report=item.human_report,
        evidence_summary=item.evidence_summary,
        capital_context=item.capital_context,
        risk_summary=item.risk_summary,
        cost_summary=item.cost_summary,
        capacity_summary=item.capacity_summary,
        changes_summary=item.changes_summary,
    )


def _handoff_view(session: Session, item: HandoffOffer) -> HandoffView:
    downstream = session.get(DownstreamSystem, item.downstream_system_id)
    package = session.get(CandidatePackage, item.candidate_package_id)
    package_version = package.contract_version if package else "1"
    contract = item.feedback_contract_snapshot or {}
    feedback_version = str(
        contract.get(
            "feedback_contract_version_id",
            downstream.feedback_contract_version if downstream else "1",
        )
    )
    return HandoffView(
        id=item.id,
        approval_id=item.approval_id,
        candidate_package_id=item.candidate_package_id,
        candidate_id=item.candidate_id,
        purpose=item.purpose,
        downstream_system_id=item.downstream_system_id,
        downstream_name=downstream.name if downstream else None,
        state=item.state,
        claim_deadline=_iso(item.claim_deadline),
        package_contract_version=package_version,
        feedback_contract_version=feedback_version,
        created_at=_iso(item.created_at),
        updated_at=_iso(item.updated_at),
        stale_reason=item.stale_reason,
        feedback_state=item.feedback_state,
    )


@router.get("/alpha-library", response_model=list[AlphaView])
def list_alphas(request: Request) -> list[AlphaView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [
            _alpha_view(item)
            for item in session.scalars(
                select(AlphaQualification).order_by(AlphaQualification.created_at.desc())
            )
        ]


@router.get("/alpha-library/{qualification_id}", response_model=AlphaView)
def get_alpha(qualification_id: UUID, request: Request) -> AlphaView:
    factory = request.app.state.session_factory
    with factory() as session:
        item = session.get(AlphaQualification, qualification_id)
        if item is None:
            raise QfError("ALPHA_QUALIFICATION_NOT_FOUND", "Alpha Qualification was not found.", 404)
        return _alpha_view(item)


def _toggle_mandate(mandate_id: UUID, request: Request, idempotency_key: str | None, enabled: bool) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():
        def action() -> dict[str, Any]:
            item = session.execute(select(PortfolioMandate).where(PortfolioMandate.id == mandate_id).with_for_update()).scalar_one_or_none()
            if item is None:
                raise QfError("MANDATE_NOT_FOUND", "Portfolio Mandate was not found.", 404)
            item.enabled = enabled
            _event(session, "MANDATE_ENABLED" if enabled else "MANDATE_DISABLED", "PORTFOLIO_MANDATE", item.id, {}, actor_kind="HUMAN")
            session.flush()
            return MandateView(id=item.id, key=item.key, name=item.name, enabled=item.enabled, latest_version_id=item.latest_version_id, spec_json=item.spec_json, state=item.state).model_dump(mode="json")

        return _idempotent(session, idempotency_key, f"portfolio-mandate.{'enable' if enabled else 'disable'}:{mandate_id}", {}, action)


@router.post("/portfolio-mandates/{mandate_id}/enable", response_model=MandateView)
def enable_mandate(mandate_id: UUID, request: Request, idempotency_key: str | None = Header(default=None, alias="Idempotency-Key")) -> dict[str, Any]:
    return _toggle_mandate(mandate_id, request, idempotency_key, True)


@router.post("/portfolio-mandates/{mandate_id}/disable", response_model=MandateView)
def disable_mandate(mandate_id: UUID, request: Request, idempotency_key: str | None = Header(default=None, alias="Idempotency-Key")) -> dict[str, Any]:
    return _toggle_mandate(mandate_id, request, idempotency_key, False)


@router.get("/portfolio-programs", response_model=list[PortfolioProgramView])
def list_portfolio_programs(request: Request) -> list[PortfolioProgramView]:
    factory = request.app.state.session_factory
    with factory() as session:
        result: list[PortfolioProgramView] = []
        for item in session.scalars(select(PortfolioProgram).order_by(PortfolioProgram.created_at.desc())):
            count = session.scalar(select(func.count()).select_from(PortfolioCandidate).where(PortfolioCandidate.portfolio_program_id == item.id))
            result.append(PortfolioProgramView(id=item.id, mandate_version_id=item.mandate_version_id, mandate_name=item.mandate_name, state=item.state, created_at=_iso(item.created_at), updated_at=_iso(item.updated_at), candidate_count=int(count or 0), current_candidate_id=item.current_candidate_id))
        return result


@router.get("/portfolio-candidates/{candidate_id}", response_model=CandidateView)
def get_candidate(candidate_id: UUID, request: Request) -> CandidateView:
    factory = request.app.state.session_factory
    with factory() as session:
        item = session.get(PortfolioCandidate, candidate_id)
        if item is None:
            raise QfError("CANDIDATE_NOT_FOUND", "Portfolio Candidate was not found.", 404)
        return _candidate_view(session, item)


@router.get("/approvals", response_model=list[ApprovalView])
def list_approvals(request: Request) -> list[ApprovalView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [_approval_view(session, item) for item in session.scalars(select(ApprovalSnapshot).order_by(ApprovalSnapshot.created_at.desc()))]


@router.get("/approvals/{approval_id}", response_model=ApprovalView)
def get_approval(approval_id: UUID, request: Request) -> ApprovalView:
    factory = request.app.state.session_factory
    with factory() as session:
        item = session.get(ApprovalSnapshot, approval_id)
        if item is None:
            raise QfError("APPROVAL_NOT_FOUND", "Approval Snapshot was not found.", 404)
        return _approval_view(session, item)


def _expire_approval_if_needed(request: Request, approval_id: UUID) -> bool:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():
        approval = session.execute(select(ApprovalSnapshot).where(ApprovalSnapshot.id == approval_id).with_for_update()).scalar_one_or_none()
        if approval is None:
            return False
        if approval.state == "PENDING" and _is_past(approval.valid_until):
            approval.state = "EXPIRED"
            approval.revision += 1
            _event(session, "APPROVAL_EXPIRED", "APPROVAL", approval.id, {}, actor_kind="SYSTEM")
            return True
    return False


def _has_current_downstream_preflight(session: Session, downstream: DownstreamSystem) -> bool:
    receipt = session.scalar(
        select(PreflightReceipt)
        .where(
            PreflightReceipt.resource_type == "DOWNSTREAM_SYSTEM",
            PreflightReceipt.resource_id == downstream.id,
        )
        .order_by(PreflightReceipt.revision.desc())
        .limit(1)
    )
    return is_current_downstream_preflight(receipt, downstream, _now())


def _has_typed_connection_preflight(session: Session, environment_type: str) -> bool:
    now = _now()
    rows = session.execute(
        select(DownstreamSystem, DownstreamConnectionVersion, FeedbackContractVersion, PreflightReceipt)
        .join(
            DownstreamConnectionVersion,
            DownstreamConnectionVersion.downstream_system_id == DownstreamSystem.id,
        )
        .join(
            FeedbackContractVersion,
            FeedbackContractVersion.id == DownstreamConnectionVersion.feedback_contract_version_id,
        )
        .join(
            PreflightReceipt,
            (PreflightReceipt.resource_id == DownstreamConnectionVersion.id)
            & (PreflightReceipt.resource_type == "DOWNSTREAM_CONNECTION_VERSION")
            & (PreflightReceipt.resource_revision == DownstreamConnectionVersion.version_no),
        )
        .where(
            DownstreamSystem.enabled.is_(True),
            DownstreamSystem.environment_type == environment_type,
            DownstreamSystem.service_token_ciphertext.is_not(None),
            DownstreamConnectionVersion.state == "ACTIVE",
            FeedbackContractVersion.state == "ACTIVE",
            FeedbackContractVersion.purpose == environment_type,
            PreflightReceipt.status == "READY",
            PreflightReceipt.contract_version == DownstreamConnectionVersion.package_contract_version,
        )
    ).all()
    return any(_stored_utc(receipt.valid_until) > now for _, _, _, receipt in rows)


def _bound_candidate_package(
    session: Session,
    request: Request,
    approval: ApprovalSnapshot,
) -> CandidatePackage:
    package_id = approval.candidate_package_id
    package_revision = approval.candidate_package_revision
    if package_id is None or package_revision is None:
        raise QfError(
            "CANDIDATE_PACKAGE_REQUIRED",
            "Approval must bind an available Candidate Package before approval.",
            409,
        )
    package = session.get(CandidatePackage, package_id)
    if (
        package is None
        or package.candidate_id != approval.candidate_id
        or package.revision != package_revision
        or package.state != "AVAILABLE"
        or not is_trusted_candidate_package(session, package)
    ):
        raise QfError(
            "CANDIDATE_PACKAGE_STALE",
            "Approval Candidate Package is no longer available at the expected revision.",
            409,
        )
    manifest = package.manifest_json
    if not isinstance(manifest, dict) or (
        manifest.get("candidate_id"),
        manifest.get("candidate_package_id"),
        manifest.get("candidate_package_revision"),
    ) != (str(approval.candidate_id), str(package.id), package.revision):
        raise QfError(
            "CANDIDATE_PACKAGE_STALE",
            "Approval Candidate Package does not match its immutable binding.",
            409,
        )
    resolve_package_archive(request.app.state.settings, package.relative_path)
    return package


@router.post("/approvals/{approval_id}/approve", response_model=ApprovalView)
def approve_candidate(approval_id: UUID, payload: ApprovalApproveInput, request: Request, idempotency_key: str | None = Header(default=None, alias="Idempotency-Key")) -> dict[str, Any]:
    if _expire_approval_if_needed(request, approval_id):
        raise QfError("APPROVAL_EXPIRED", "Approval validity window has expired.", 409)
    factory = request.app.state.session_factory
    with factory() as session, session.begin():
        def action() -> dict[str, Any]:
            approval = session.execute(select(ApprovalSnapshot).where(ApprovalSnapshot.id == approval_id).with_for_update()).scalar_one_or_none()
            if approval is None:
                raise QfError("APPROVAL_NOT_FOUND", "Approval Snapshot was not found.", 404)
            if approval.state != payload.expected_state or approval.state != "PENDING":
                raise QfError("APPROVAL_STATE_CONFLICT", "Approval state changed before the decision.", 409, {"expected": payload.expected_state, "actual": approval.state})
            if approval.promotion_evaluation_id is not None:
                if (
                    payload.downstream_system_id is not None
                    and payload.downstream_system_id != approval.downstream_system_id
                ):
                    raise QfError(
                        "APPROVAL_TARGET_IMMUTABLE",
                        "Typed Approval downstream binding cannot be changed.",
                        409,
                    )
                if approval.promotion_purpose == "PORTFOLIO_TO_PAPER":
                    approve_typed_paper_handoff(session, approval.id)
                elif approval.promotion_purpose == "PAPER_TO_LIVE":
                    approve_typed_live_handoff(session, approval.id)
                else:
                    raise QfError(
                        "APPROVAL_TYPED_LINEAGE_REQUIRED",
                        "Approval purpose is not a typed production handoff.",
                        409,
                    )
                return _approval_view(session, approval).model_dump(mode="json")
            downstream = session.execute(
                select(DownstreamSystem)
                .where(DownstreamSystem.id == payload.downstream_system_id)
                .with_for_update()
            ).scalar_one_or_none()
            if downstream is None or not _has_current_downstream_preflight(session, downstream):
                raise QfError("DOWNSTREAM_NOT_READY", "Selected downstream is not ready.", 409)
            if downstream.environment_type != approval.purpose:
                raise QfError("DOWNSTREAM_INCOMPATIBLE", "Downstream environment does not match Approval purpose.", 409)
            if downstream.service_token_ciphertext is None:
                raise QfError("DOWNSTREAM_CREDENTIAL_NOT_CONFIGURED", "Selected downstream has no service credential.", 409)
            package = _bound_candidate_package(session, request, approval)
            if package.contract_version != downstream.package_contract_version:
                raise QfError(
                    "CANDIDATE_PACKAGE_CONTRACT_MISMATCH",
                    "Candidate Package contract is incompatible with the selected downstream.",
                    409,
                )
            contract = feedback_contract_snapshot(downstream, approval.purpose)
            handoff = HandoffOffer(
                approval_id=approval.id,
                candidate_package_id=package.id,
                candidate_id=approval.candidate_id,
                purpose=approval.purpose,
                downstream_system_id=downstream.id,
                state="AVAILABLE",
                claim_deadline=_now() + timedelta(days=7),
                feedback_state="PENDING",
                feedback_contract_snapshot=contract,
            )
            session.add(handoff)
            session.flush()
            approval.state = "APPROVED"
            approval.downstream_system_id = downstream.id
            approval.revision += 1
            _event(session, "APPROVAL_APPROVED", "APPROVAL", approval.id, {"candidate_id": str(approval.candidate_id), "handoff_id": str(handoff.id)}, actor_kind="HUMAN")
            _event(session, "HANDOFF_AVAILABLE", "HANDOFF", handoff.id, {"candidate_id": str(approval.candidate_id), "approval_id": str(approval.id)})
            session.flush()
            return _approval_view(session, approval).model_dump(mode="json")

        return _idempotent(session, idempotency_key, f"approval.approve:{approval_id}", payload, action)


@router.post("/approvals/{approval_id}/reject", response_model=ApprovalView)
def reject_candidate(approval_id: UUID, payload: ApprovalRejectInput, request: Request, idempotency_key: str | None = Header(default=None, alias="Idempotency-Key")) -> dict[str, Any]:
    if _expire_approval_if_needed(request, approval_id):
        raise QfError("APPROVAL_EXPIRED", "Approval validity window has expired.", 409)
    factory = request.app.state.session_factory
    with factory() as session, session.begin():
        def action() -> dict[str, Any]:
            approval = session.execute(select(ApprovalSnapshot).where(ApprovalSnapshot.id == approval_id).with_for_update()).scalar_one_or_none()
            if approval is None:
                raise QfError("APPROVAL_NOT_FOUND", "Approval Snapshot was not found.", 404)
            if approval.state != payload.expected_state or approval.state != "PENDING":
                raise QfError("APPROVAL_STATE_CONFLICT", "Approval state changed.", 409)
            approval.state = "REJECTED"
            approval.revision += 1
            approval.human_report = {"decision": "REJECT", "reason_code": payload.reason_code, "note": payload.note}
            _event(session, "APPROVAL_REJECTED", "APPROVAL", approval.id, {"reason_code": payload.reason_code}, actor_kind="HUMAN")
            session.flush()
            return _approval_view(session, approval).model_dump(mode="json")

        return _idempotent(session, idempotency_key, f"approval.reject:{approval_id}", payload, action)


@router.get("/handoffs", response_model=list[HandoffView])
def list_handoffs(request: Request) -> list[HandoffView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [_handoff_view(session, item) for item in session.scalars(select(HandoffOffer).order_by(HandoffOffer.created_at.desc()))]


@router.post("/handoffs/{handoff_id}/revoke", response_model=HandoffView)
def revoke_handoff(handoff_id: UUID, payload: HandoffRevokeInput, request: Request, idempotency_key: str | None = Header(default=None, alias="Idempotency-Key")) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():
        def action() -> dict[str, Any]:
            handoff = session.execute(select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()).scalar_one_or_none()
            if handoff is None:
                raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
            if handoff.state not in {"APPROVED", "PUBLISHING", "AVAILABLE"}:
                raise QfError("HANDOFF_ALREADY_OWNED", "Claimed or terminal Handoff cannot be revoked by QuaZonai.", 409)
            handoff.state = "REVOKED"
            handoff.stale_reason = payload.reason_code
            _event(session, "HANDOFF_REVOKED", "HANDOFF", handoff.id, {"reason_code": payload.reason_code}, actor_kind="HUMAN")
            session.flush()
            return _handoff_view(session, handoff).model_dump(mode="json")

        return _idempotent(session, idempotency_key, f"handoff.revoke:{handoff_id}", payload, action)


def _authenticate_handoff(session: Session, request: Request, handoff: HandoffOffer, authorization: str | None) -> DownstreamSystem:
    downstream = session.get(DownstreamSystem, handoff.downstream_system_id)
    if downstream is None:
        raise QfError("DOWNSTREAM_NOT_FOUND", "Handoff Downstream System is missing.", 500)
    authenticate_downstream(request.app.state.settings, downstream, authorization)
    return downstream


def _expire_handoff_if_needed(request: Request, handoff_id: UUID) -> bool:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():
        handoff = session.execute(select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()).scalar_one_or_none()
        if handoff is None:
            return False
        if handoff.state == "AVAILABLE" and _is_past(handoff.claim_deadline):
            handoff.state = "EXPIRED"
            _event(session, "HANDOFF_EXPIRED", "HANDOFF", handoff.id, {})
            return True
    return False


@router.post("/handoffs/{handoff_id}/claim", response_model=HandoffView)
def claim_handoff(handoff_id: UUID, payload: HandoffStateInput, request: Request, authorization: str | None = Header(default=None, alias="Authorization"), idempotency_key: str | None = Header(default=None, alias="Idempotency-Key")) -> dict[str, Any]:
    if _expire_handoff_if_needed(request, handoff_id):
        raise QfError("HANDOFF_EXPIRED", "Handoff claim deadline expired.", 409)
    factory = request.app.state.session_factory
    with factory() as session, session.begin():
        def action() -> dict[str, Any]:
            handoff = session.execute(select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()).scalar_one_or_none()
            if handoff is None:
                raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
            _authenticate_handoff(session, request, handoff, authorization)
            if handoff.state != "AVAILABLE":
                raise QfError("HANDOFF_STATE_CONFLICT", "Only Available Handoffs can be claimed.", 409)
            if payload.expected_state and payload.expected_state != handoff.state:
                raise QfError("HANDOFF_STATE_CONFLICT", "Handoff state changed before claim.", 409)
            handoff.state = "CLAIMED"
            handoff.claimed_at = _now()
            _event(session, "HANDOFF_CLAIMED", "HANDOFF", handoff.id, {})
            session.flush()
            return _handoff_view(session, handoff).model_dump(mode="json")

        return _idempotent(session, idempotency_key, f"handoff.claim:{handoff_id}", payload, action)


@router.post("/handoffs/{handoff_id}/accept", response_model=HandoffView)
def accept_handoff(handoff_id: UUID, payload: HandoffStateInput, request: Request, authorization: str | None = Header(default=None, alias="Authorization"), idempotency_key: str | None = Header(default=None, alias="Idempotency-Key")) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():
        def action() -> dict[str, Any]:
            handoff = session.execute(select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()).scalar_one_or_none()
            if handoff is None:
                raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
            _authenticate_handoff(session, request, handoff, authorization)
            if handoff.state != "CLAIMED":
                raise QfError("HANDOFF_STATE_CONFLICT", "Only Claimed Handoffs can be accepted.", 409)
            if payload.expected_state and payload.expected_state != handoff.state:
                raise QfError("HANDOFF_STATE_CONFLICT", "Handoff state changed before acceptance.", 409)
            handoff.state = "DOWNSTREAM_ACCEPTED"
            handoff.accepted_at = _now()
            handoff.feedback_state = "FEEDBACK_PENDING"
            _event(session, "HANDOFF_ACCEPTED", "HANDOFF", handoff.id, {})
            session.flush()
            return _handoff_view(session, handoff).model_dump(mode="json")

        return _idempotent(session, idempotency_key, f"handoff.accept:{handoff_id}", payload, action)


@router.post("/handoffs/{handoff_id}/reject", response_model=HandoffView)
def downstream_reject_handoff(handoff_id: UUID, payload: HandoffStateInput, request: Request, authorization: str | None = Header(default=None, alias="Authorization"), idempotency_key: str | None = Header(default=None, alias="Idempotency-Key")) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():
        def action() -> dict[str, Any]:
            handoff = session.execute(select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()).scalar_one_or_none()
            if handoff is None:
                raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
            _authenticate_handoff(session, request, handoff, authorization)
            if handoff.state not in {"AVAILABLE", "CLAIMED"}:
                raise QfError("HANDOFF_STATE_CONFLICT", "Handoff cannot be rejected now.", 409)
            handoff.state = "DOWNSTREAM_REJECTED"
            handoff.stale_reason = payload.reason_code
            _event(session, "HANDOFF_DOWNSTREAM_REJECTED", "HANDOFF", handoff.id, {"reason_code": payload.reason_code})
            session.flush()
            return _handoff_view(session, handoff).model_dump(mode="json")

        return _idempotent(session, idempotency_key, f"handoff.downstream-reject:{handoff_id}", payload, action)


@router.get("/handoffs/{handoff_id}/package", response_class=FileResponse)
def get_handoff_package(handoff_id: UUID, request: Request, authorization: str | None = Header(default=None, alias="Authorization")) -> FileResponse:
    factory = request.app.state.session_factory
    with factory() as session:
        handoff = session.get(HandoffOffer, handoff_id)
        if handoff is None:
            raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
        _authenticate_handoff(session, request, handoff, authorization)
        if handoff.state not in {"CLAIMED", "DOWNSTREAM_ACCEPTED", "FEEDBACK_PENDING", "FEEDBACK_IN_PROGRESS", "FEEDBACK_PARTIAL", "FEEDBACK_COMPLETE"}:
            raise QfError("HANDOFF_PACKAGE_UNAVAILABLE", "Package is not available in this state.", 409)
        package = session.get(CandidatePackage, handoff.candidate_package_id)
        if (
            package is None
            or package.candidate_id != handoff.candidate_id
            or not is_trusted_candidate_package(session, package)
        ):
            raise QfError(
                "CANDIDATE_PACKAGE_STALE",
                "Handoff Candidate Package is no longer a trusted assembled Package.",
                409,
            )
        archive = resolve_package_archive(request.app.state.settings, package.relative_path)
        return FileResponse(archive, media_type="application/zip", filename=f"candidate-package-{package.id}.zip")


def _validate_complete_feedback(handoff: HandoffOffer, payload: FeedbackInput) -> tuple[datetime, datetime, int]:
    problems: list[str] = []
    if payload.observation_start is None:
        problems.append("observation_start is required")
    if payload.observation_end is None:
        problems.append("observation_end is required")
    if payload.sample_size is None:
        problems.append("sample_size is required")
    if problems:
        raise QfError("FEEDBACK_CONTRACT_INVALID", "Complete feedback does not satisfy the frozen Feedback Contract.", 422, {"problems": problems})
    assert payload.observation_start is not None
    assert payload.observation_end is not None
    assert payload.sample_size is not None
    start = payload.observation_start
    end = payload.observation_end
    if end <= start:
        problems.append("observation_end must be after observation_start")
    contract = handoff.feedback_contract_snapshot or {}
    minimum_duration = contract.get("minimum_observation_duration_seconds")
    minimum_sample = contract.get("minimum_valid_sample_size")
    if (
        isinstance(minimum_duration, bool)
        or not isinstance(minimum_duration, int)
        or isinstance(minimum_sample, bool)
        or not isinstance(minimum_sample, int)
        or minimum_duration < 0
        or minimum_sample < 1
    ):
        problems.append("frozen feedback contract minimums are invalid")
    else:
        if end > start and (end - start).total_seconds() < minimum_duration:
            problems.append(f"observation duration must be at least {minimum_duration} seconds")
        if payload.sample_size < minimum_sample:
            problems.append(f"sample_size must be at least {minimum_sample}")
    required_fields = contract.get("required_fields")
    if (
        not isinstance(required_fields, list)
        or not required_fields
        or any(not isinstance(field, str) or not field.strip() for field in required_fields)
    ):
        problems.append("frozen required_fields is invalid")
    else:
        missing = [field for field in required_fields if field not in payload.evidence or payload.evidence[field] is None]
        if missing:
            problems.append(f"missing required evidence fields: {', '.join(missing)}")
    if problems:
        raise QfError("FEEDBACK_CONTRACT_INVALID", "Complete feedback does not satisfy the frozen Feedback Contract.", 422, {"problems": problems})
    return start, end, payload.sample_size


@router.post("/handoffs/{handoff_id}/feedback", response_model=HandoffView)
def submit_feedback(handoff_id: UUID, payload: FeedbackInput, request: Request, authorization: str | None = Header(default=None, alias="Authorization"), idempotency_key: str | None = Header(default=None, alias="Idempotency-Key")) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():
        def action() -> dict[str, Any]:
            handoff = session.execute(select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()).scalar_one_or_none()
            if handoff is None:
                raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
            _authenticate_handoff(session, request, handoff, authorization)
            if handoff.state not in {"DOWNSTREAM_ACCEPTED", "FEEDBACK_PENDING", "FEEDBACK_IN_PROGRESS", "FEEDBACK_PARTIAL"}:
                raise QfError("HANDOFF_STATE_CONFLICT", "Handoff is not accepting feedback.", 409)
            if handoff.promotion_purpose == "PORTFOLIO_TO_PAPER":
                if (
                    payload.state != "FEEDBACK_COMPLETE"
                    or payload.observation_start is None
                    or payload.observation_end is None
                    or payload.sample_size is None
                    or payload.evidence
                    or not payload.metrics
                ):
                    raise QfError(
                        "FEEDBACK_CONTRACT_INVALID",
                        "Typed Paper feedback requires complete scalar metrics and no JSON evidence.",
                        422,
                    )
                accept_paper_feedback(
                    session,
                    handoff_id=handoff.id,
                    header=FeedbackHeader(
                        observation_start=payload.observation_start,
                        observation_end=payload.observation_end,
                        sample_size=payload.sample_size,
                    ),
                    metrics=(
                        TypedFeedbackMetric(
                            metric_code=metric.metric_code,
                            status=metric.status,
                            value=metric.value,
                        )
                        for metric in payload.metrics
                    ),
                )
                session.flush()
                return _handoff_view(session, handoff).model_dump(mode="json")
            if handoff.promotion_purpose == "PAPER_TO_LIVE":
                if (
                    payload.state != "FEEDBACK_COMPLETE"
                    or payload.observation_start is None
                    or payload.observation_end is None
                    or payload.sample_size is None
                    or payload.evidence
                    or not payload.metrics
                ):
                    raise QfError(
                        "FEEDBACK_CONTRACT_INVALID",
                        "Typed Live feedback requires complete scalar metrics and no JSON evidence.",
                        422,
                    )
                accept_live_feedback(
                    session,
                    handoff_id=handoff.id,
                    header=FeedbackHeader(
                        observation_start=payload.observation_start,
                        observation_end=payload.observation_end,
                        sample_size=payload.sample_size,
                    ),
                    metrics=(
                        TypedFeedbackMetric(
                            metric_code=metric.metric_code,
                            status=metric.status,
                            value=metric.value,
                        )
                        for metric in payload.metrics
                    ),
                )
                session.flush()
                return _handoff_view(session, handoff).model_dump(mode="json")
            allowed = {"FEEDBACK_IN_PROGRESS", "FEEDBACK_PARTIAL", "FEEDBACK_COMPLETE"}
            if payload.state not in allowed:
                raise QfError("FEEDBACK_STATE_INVALID", "Feedback state is invalid.", 422)
            if payload.state == "FEEDBACK_COMPLETE":
                start, end, sample_size = _validate_complete_feedback(handoff, payload)
                episode = ForwardEvidenceEpisode(
                    handoff_id=handoff.id,
                    state="FEEDBACK_COMPLETE",
                    evidence=payload.evidence,
                    observation_start=start,
                    observation_end=end,
                    sample_size=sample_size,
                    created_at=_now(),
                )
                session.add(episode)
                session.flush()
                handoff.state = "FEEDBACK_COMPLETE"
                handoff.feedback_state = "FEEDBACK_COMPLETE"
                _event(session, "FORWARD_EVIDENCE_RECORDED", "HANDOFF", handoff.id, {"episode_id": str(episode.id), "state": "FEEDBACK_COMPLETE"})
            else:
                handoff.state = payload.state
                handoff.feedback_state = payload.state
                _event(session, "HANDOFF_FEEDBACK_STATUS", "HANDOFF", handoff.id, {"state": payload.state})
            session.flush()
            return _handoff_view(session, handoff).model_dump(mode="json")

        return _idempotent(session, idempotency_key, f"handoff.feedback:{handoff_id}", payload, action)


@router.post(
    "/handoffs/{handoff_id}/degradation-observations",
    response_model=DegradationObservationView,
    status_code=201,
)
def record_handoff_degradation(
    handoff_id: UUID,
    payload: DegradationObservationInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    """Record an operator observation; it never calls or controls the downstream."""
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            observation, wake, cycle = record_degradation_observation(
                session,
                handoff_id=handoff_id,
                subject_type=payload.subject_type,
                subject_id=payload.subject_id,
                metric_name=payload.metric_name,
                severity=payload.severity,
                confidence=payload.confidence,
            )
            session.flush()
            return DegradationObservationView(
                id=observation.id,
                forward_evidence_episode_id=observation.forward_evidence_episode_id,
                subject_type=SubjectType(observation.subject_type),
                subject_id=observation.subject_id,
                metric_name=observation.metric_name,
                state=observation.state,
                evaluated=observation.evaluated,
                wake_event_id=wake.id if wake else None,
                wake_state=wake.state if wake else None,
                cycle_id=cycle.id if cycle else None,
            ).model_dump(mode="json")

        return _idempotent(
            session,
            idempotency_key,
            f"handoff.degradation-observation:{handoff_id}",
            payload,
            action,
            status_code=201,
        )


@router.get("/readiness", response_model=dict[str, Any])
def readiness(request: Request) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session:
        data_ready = session.scalar(
            select(DatasetRevision.id)
            .where(
                DatasetRevision.quality_state == "VALID",
                DatasetRevision.point_in_time_state == "VALID",
                DatasetRevision.promotability == "PROMOTABLE",
            )
            .limit(1)
        ) is not None
        codex_ready, codex_state = codex_auth_readiness(session, request.app.state.settings)
        paper_ready = _has_typed_connection_preflight(session, "PAPER")
        live_downstream_ready = _has_typed_connection_preflight(session, "LIVE")
        paper_feedback_ready = bool(session.scalar(select(func.count()).select_from(ForwardEvidenceEpisode).join(HandoffOffer, ForwardEvidenceEpisode.handoff_id == HandoffOffer.id).where(HandoffOffer.purpose == "PAPER", ForwardEvidenceEpisode.state == "FEEDBACK_COMPLETE")))
        return {
            "SYSTEM_READY": True,
            "RESEARCH_READY": data_ready and codex_ready,
            "RESEARCH_READY_REASONS": [] if data_ready and codex_ready else [
                reason
                for reason, missing in (
                    ("PROMOTABLE_DATASET_REQUIRED", not data_ready),
                    (f"CODEX_AUTH_{codex_state}", not codex_ready),
                )
                if missing
            ],
            "PAPER_HANDOFF_READY": paper_ready,
            "LIVE_HANDOFF_READY": live_downstream_ready and paper_feedback_ready,
        }
