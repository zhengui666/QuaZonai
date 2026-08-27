"""Public Research Intelligence and Portfolio Construction API."""

from __future__ import annotations

import re
from datetime import UTC, datetime, timedelta
from typing import Any, Callable
from uuid import UUID, uuid4

from fastapi import APIRouter, Header, Request
from fastapi.responses import FileResponse
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from candidate_bundles import (
    BUNDLE_CONTRACT_VERSION,
    build_candidate_bundle,
    resolve_bundle_archive,
)
from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    CandidateBundle,
    DatasetRevision,
    DownstreamSystem,
    Event,
    ForwardEvidenceEpisode,
    GovernedDataSource,
    HandoffOffer,
    IdeaContribution,
    MarketUniverseVersion,
    PortfolioCandidate,
    PortfolioMandate,
    PortfolioProgram,
    PublicMutationReceipt,
    ResearchBranch,
    ResearchCharter,
    ResearchMission,
    ResearchProgram,
)
from downstream_auth import authenticate_downstream, install_service_token, issue_service_token
from errors import QfError
from jobs import enqueue_job
from quant_runtime.data_scope import dataset_revision_domains, market_scope_matches_universe

router = APIRouter(prefix="/api/v1", tags=["domain"])


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class CharterView(StrictModel):
    id: UUID | None = None
    original_idea_text: str
    research_question: str | None = None
    market_scope: str | list[str] | None = None
    universe_version_ids: list[UUID] = Field(default_factory=list)
    prediction_horizon: str | None = None
    allowed_data_domains: list[str] = Field(default_factory=list)
    explicit_exclusions: list[str] = Field(default_factory=list)
    material_assumptions: list[str] = Field(default_factory=list)
    system_assumptions: list[str] = Field(default_factory=list)
    created_at: str | None = None


class OverlapView(StrictModel):
    kind: str
    program_id: UUID | None = None
    program_title: str | None = None
    rationale: str | None = None
    recommendation: str | None = None


class IdeaPreviewInput(StrictModel):
    idea: str = Field(min_length=12, max_length=20_000)


class IdeaPreviewView(StrictModel):
    charter: CharterView
    clarification_required: bool = False
    clarification_questions: list[dict[str, str]] = Field(default_factory=list)
    overlap: OverlapView | None = None
    assumptions: list[str] = Field(default_factory=list)


class ResearchProgramInput(StrictModel):
    idea: str = Field(min_length=12, max_length=20_000)
    answers: dict[str, str] = Field(default_factory=dict)
    overlap_action: str | None = None


class ResearchProgramView(StrictModel):
    id: UUID
    title: str
    charter_id: UUID
    charter: CharterView
    state: str
    cooling_reason: str | None = None
    blocked_reason: str | None = None
    wake_reason: str | None = None
    created_at: str | None = None
    updated_at: str | None = None
    branch_count: int = 0
    mission_count: int = 0
    alpha_count: int = 0


class ProgramActionInput(StrictModel):
    reason: str | None = None


class MissionView(StrictModel):
    id: UUID
    branch_id: UUID
    program_id: UUID
    type: str
    role: str | None = None
    state: str
    objective: str | None = None
    dependencies: list[UUID] = Field(default_factory=list)
    started_at: str | None = None
    finished_at: str | None = None
    attempt: int = 1
    error_code: str | None = None
    summary: str | None = None


class ActivityView(StrictModel):
    id: int
    kind: str
    aggregate_type: str
    aggregate_id: UUID | None = None
    mission_id: UUID | None = None
    payload: dict[str, Any] = Field(default_factory=dict)
    created_at: str


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
    downstream_system_id: UUID
    expected_state: str


class ApprovalRejectInput(StrictModel):
    reason_code: str = Field(min_length=1, max_length=100)
    note: str | None = Field(default=None, max_length=4000)
    expected_state: str


class HandoffView(StrictModel):
    id: UUID
    approval_id: UUID
    candidate_bundle_id: UUID
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


class FeedbackInput(StrictModel):
    state: str = "FEEDBACK_COMPLETE"
    observation_start: datetime | None = None
    observation_end: datetime | None = None
    sample_size: int | None = Field(default=None, ge=0)
    evidence: dict[str, Any] = Field(default_factory=dict)


class DataSourceInput(StrictModel):
    name: str = Field(min_length=1, max_length=200)
    provider: str | None = Field(default=None, max_length=200)
    universe_scope: list[str] = Field(default_factory=list)
    fields: list[str] = Field(default_factory=list)
    update_cadence: str | None = Field(default=None, max_length=100)
    state: str | None = None
    public_config: dict[str, Any] = Field(default_factory=dict)


class DataSourceView(StrictModel):
    id: UUID
    name: str
    provider: str | None = None
    state: str
    universe_scope: list[str] = Field(default_factory=list)
    fields: list[str] = Field(default_factory=list)
    update_cadence: str | None = None
    preflight_state: str


class DownstreamInput(StrictModel):
    name: str = Field(min_length=1, max_length=200)
    environment_type: str
    enabled: bool = True
    package_contract_version: str = "2"
    feedback_contract_version: str = "1"
    compatibility: list[str] = Field(default_factory=list)
    public_config: dict[str, Any] = Field(default_factory=dict)


class DownstreamView(StrictModel):
    id: UUID
    name: str
    environment_type: str
    enabled: bool
    package_contract_version: str
    feedback_contract_version: str
    compatibility: list[str] = Field(default_factory=list)
    preflight_state: str


class DownstreamRegistrationView(DownstreamView):
    service_token: str


class DownstreamTokenView(StrictModel):
    downstream_system_id: UUID
    service_token: str


class UniverseView(StrictModel):
    id: UUID
    universe_key: str
    version_no: int
    name: str
    state: str
    spec_json: dict[str, Any] = Field(default_factory=dict)


class DatasetView(StrictModel):
    id: UUID
    data_source_id: UUID | None = None
    universe_version_id: UUID | None = None
    universe_name: str | None = None
    revision_no: int
    schema_version: str | None = None
    event_start: str | None = None
    event_end: str | None = None
    available_start: str | None = None
    available_end: str | None = None
    row_count: int | None = None
    quality_state: str
    point_in_time_state: str
    partition: str
    created_at: str


def _now() -> datetime:
    return datetime.now(UTC)


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


def _infer_scope(idea: str) -> str:
    lowered = idea.lower()
    if "crypto" in lowered:
        return "Crypto Spot"
    if "option" in lowered:
        return "US Options"
    if "future" in lowered:
        return "Futures"
    if " fx " in f" {lowered} " or "foreign exchange" in lowered:
        return "FX"
    if any(word in lowered for word in ("equity", "stock", "earnings", "us ")):
        return "US Equities"
    return "System inferred"


def _infer_horizon(idea: str) -> str:
    match = re.search(r"\b(\d+)\s*(minute|min|hour|hr|day|d|h|m)s?\b", idea.lower())
    if not match:
        # A frozen Charter must never carry an unresolved sentinel into
        # qualification. Daily is the explicit V1 default when the Idea does
        # not state a horizon; users can state another concrete horizon.
        return "1D"
    number, unit = match.groups()
    return f"{number}{unit[0].upper()}"


def _charter_preview(idea: str) -> CharterView:
    clean = idea.strip()
    return CharterView(
        original_idea_text=clean,
        research_question=clean.rstrip(".") + ("" if clean.rstrip().endswith("?") else "?"),
        market_scope=_infer_scope(clean),
        prediction_horizon=_infer_horizon(clean),
        system_assumptions=["Implementation details are selected by the research system."],
    )


def _resolve_frozen_research_scope(
    session: Session,
    preview: CharterView,
) -> tuple[list[UUID], list[str], str | list[str]]:
    """Resolve an Idea to concrete executable governed Discovery scope."""
    revisions = list(
        session.scalars(
            select(DatasetRevision)
            .where(
                DatasetRevision.partition == "DISCOVERY",
                DatasetRevision.quality_state == "VALID",
                DatasetRevision.point_in_time_state == "VALID",
                DatasetRevision.catalog_uri.is_not(None),
                DatasetRevision.universe_version_id.is_not(None),
                DatasetRevision.data_source_id.is_not(None),
            )
            .order_by(DatasetRevision.created_at.desc())
        )
    )
    universe_ids = {
        item.universe_version_id for item in revisions if item.universe_version_id is not None
    }
    source_ids = {item.data_source_id for item in revisions if item.data_source_id is not None}
    universes = (
        list(
            session.scalars(
                select(MarketUniverseVersion).where(
                    MarketUniverseVersion.id.in_(universe_ids),
                    MarketUniverseVersion.state == "ACTIVE",
                )
            )
        )
        if universe_ids
        else []
    )
    sources = (
        list(
            session.scalars(
                select(GovernedDataSource).where(
                    GovernedDataSource.id.in_(source_ids),
                    GovernedDataSource.state == "ACTIVE",
                    GovernedDataSource.preflight_state == "READY",
                )
            )
        )
        if source_ids
        else []
    )
    latest_by_key: dict[str, MarketUniverseVersion] = {}
    for universe in universes:
        if (
            universe.universe_key not in latest_by_key
            or universe.version_no > latest_by_key[universe.universe_key].version_no
        ):
            latest_by_key[universe.universe_key] = universe
    active_universes = {item.id: item for item in latest_by_key.values()}
    active_sources = {item.id: item for item in sources}

    eligible: list[tuple[DatasetRevision, MarketUniverseVersion, set[str]]] = []
    for revision in revisions:
        if revision.universe_version_id is None or revision.data_source_id is None:
            continue
        matched_universe = active_universes.get(revision.universe_version_id)
        matched_source = active_sources.get(revision.data_source_id)
        if matched_universe is None or matched_source is None:
            continue
        if not market_scope_matches_universe(preview.market_scope, matched_universe):
            continue
        catalog_uri = revision.catalog_uri or ""
        if not catalog_uri.startswith("nautilus-catalog://") or not revision.instrument_scope:
            continue
        domains = dataset_revision_domains(revision, matched_source)
        if not domains:
            continue
        eligible.append((revision, matched_universe, domains))

    if not eligible:
        raise QfError(
            "RESEARCH_SCOPE_UNAVAILABLE",
            "No executable governed Discovery dataset matches the inferred Research scope.",
            422,
            {"market_scope": preview.market_scope},
        )

    concrete_universes = {universe.id: universe for _, universe, _ in eligible}
    requested_scope = preview.market_scope
    if requested_scope is None or requested_scope == "System inferred":
        names = sorted({item.name for item in concrete_universes.values()})
        if len(names) != 1:
            raise QfError(
                "RESEARCH_SCOPE_AMBIGUOUS",
                "The Idea does not resolve to one configured executable market scope.",
                422,
                {"available_market_scopes": names},
            )
        frozen_market_scope: str | list[str] = names[0]
    else:
        frozen_market_scope = requested_scope

    frozen_universe_ids = sorted(concrete_universes, key=str)
    frozen_domains = sorted({domain for _, _, domains in eligible for domain in domains})
    return frozen_universe_ids, frozen_domains, frozen_market_scope


def _charter_view(item: ResearchCharter) -> CharterView:
    universe_ids: list[UUID] = []
    for value in item.universe_version_ids:
        try:
            universe_ids.append(UUID(str(value)))
        except ValueError:
            continue
    return CharterView(
        id=item.id,
        original_idea_text=item.original_idea_text,
        research_question=item.research_question,
        market_scope=item.market_scope,
        universe_version_ids=universe_ids,
        prediction_horizon=item.prediction_horizon,
        allowed_data_domains=item.allowed_data_domains,
        explicit_exclusions=item.explicit_exclusions,
        material_assumptions=item.material_assumptions,
        system_assumptions=item.system_assumptions,
        created_at=_iso(item.created_at),
    )


def _candidate_view(item: PortfolioCandidate) -> CandidateView:
    universe_set = item.universe_set_json
    if not isinstance(universe_set, (list, dict)):
        universe_set = []
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
        members=item.members,
        metrics=item.metrics,
    )


def _program_view(session: Session, item: ResearchProgram) -> ResearchProgramView:
    charter = session.get(ResearchCharter, item.charter_id)
    if charter is None:
        raise QfError("CHARTER_NOT_FOUND", "Research Charter is missing.", 500)
    branch_count = session.scalar(
        select(func.count()).select_from(ResearchBranch).where(ResearchBranch.program_id == item.id)
    )
    mission_count = session.scalar(
        select(func.count())
        .select_from(ResearchMission)
        .where(ResearchMission.program_id == item.id)
    )
    alpha_count = session.scalar(
        select(func.count())
        .select_from(AlphaQualification)
        .where(AlphaQualification.program_id == item.id)
    )
    return ResearchProgramView(
        id=item.id,
        title=item.title,
        charter_id=item.charter_id,
        charter=_charter_view(charter),
        state=item.state,
        cooling_reason=item.cooling_reason,
        blocked_reason=item.blocked_reason,
        wake_reason=item.wake_reason,
        created_at=_iso(item.created_at),
        updated_at=_iso(item.updated_at),
        branch_count=int(branch_count or 0),
        mission_count=int(mission_count or 0),
        alpha_count=int(alpha_count or 0),
    )


def _mission_view(item: ResearchMission) -> MissionView:
    dependencies: list[UUID] = []
    for value in item.dependencies:
        try:
            dependencies.append(UUID(str(value)))
        except ValueError:
            continue
    return MissionView(
        id=item.id,
        branch_id=item.branch_id,
        program_id=item.program_id,
        type=item.type,
        role=item.role,
        state=item.state,
        objective=item.objective,
        dependencies=dependencies,
        started_at=_iso(item.started_at),
        finished_at=_iso(item.finished_at),
        attempt=item.attempt,
        error_code=item.error_code,
        summary=item.summary,
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
        candidate=_candidate_view(candidate),
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
    package = session.get(CandidateBundle, item.candidate_bundle_id)
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
        candidate_bundle_id=item.candidate_bundle_id,
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


def _find_duplicate_program(session: Session, idea: str) -> ResearchProgram | None:
    normalized = idea.strip().casefold()
    for program in session.scalars(
        select(ResearchProgram).order_by(ResearchProgram.created_at.desc())
    ):
        charter = session.get(ResearchCharter, program.charter_id)
        if charter and charter.original_idea_text.strip().casefold() == normalized:
            return program
    return None


def _latest_branch(session: Session, program_id: UUID) -> ResearchBranch | None:
    return session.scalar(
        select(ResearchBranch)
        .where(ResearchBranch.program_id == program_id)
        .order_by(ResearchBranch.created_at.desc())
        .limit(1)
    )


def _queue_mission(
    session: Session,
    *,
    program: ResearchProgram,
    branch: ResearchBranch,
    objective: str,
) -> ResearchMission:
    mission = ResearchMission(
        program_id=program.id,
        branch_id=branch.id,
        type="ALPHA_DISCOVERY",
        role="ALPHA_RESEARCHER",
        state="READY",
        objective=objective,
        dependencies=[],
        attempt=1,
        summary="Mission is ready and queued for the Agent Worker.",
    )
    session.add(mission)
    session.flush()
    job = enqueue_job(
        session,
        kind="RESEARCH_MISSION",
        resource_type="research_mission",
        resource_id=mission.id,
        payload={"program_id": str(program.id), "branch_id": str(branch.id)},
    )
    _event(
        session,
        "MISSION_READY",
        "RESEARCH_PROGRAM",
        program.id,
        {
            "program_id": str(program.id),
            "mission_id": str(mission.id),
            "job_id": str(job.id),
            "summary": mission.summary,
        },
    )
    return mission


@router.post("/ideas/preview", response_model=IdeaPreviewView)
def preview_idea(payload: IdeaPreviewInput, request: Request) -> IdeaPreviewView:
    preview = _charter_preview(payload.idea)
    factory = request.app.state.session_factory
    with factory() as session:
        program = _find_duplicate_program(session, payload.idea)
        overlap = None
        if program is not None:
            overlap = OverlapView(
                kind="DUPLICATE",
                program_id=program.id,
                program_title=program.title,
                rationale="An existing Program has the same submitted idea.",
                recommendation="Wake the existing Program unless independent treatment is required.",
            )
        return IdeaPreviewView(charter=preview, overlap=overlap)


@router.post("/research-programs", response_model=ResearchProgramView, status_code=201)
def create_program(
    payload: ResearchProgramInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    allowed_actions = {None, "recommended", "new-program", "independent-program"}
    if payload.overlap_action not in allowed_actions:
        raise QfError("OVERLAP_ACTION_INVALID", "Overlap action is invalid.", 422)
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            duplicate = _find_duplicate_program(session, payload.idea)
            if duplicate is not None and payload.overlap_action in {None, "recommended"}:
                session.add(
                    IdeaContribution(
                        program_id=duplicate.id,
                        idea_text=payload.idea.strip(),
                        action="WAKE_EXISTING",
                        created_at=_now(),
                    )
                )
                duplicate.wake_reason = "Overlapping IdeaContribution received."
                duplicate.revision += 1
                if duplicate.state == "COOLING":
                    duplicate.state = "ACTIVE"
                active_count = session.scalar(
                    select(func.count())
                    .select_from(ResearchMission)
                    .where(
                        ResearchMission.program_id == duplicate.id,
                        ResearchMission.state.in_(["READY", "RUNNING"]),
                    )
                )
                branch = _latest_branch(session, duplicate.id)
                if (
                    branch is not None
                    and duplicate.state not in {"PAUSED", "ARCHIVED"}
                    and not active_count
                ):
                    _queue_mission(
                        session,
                        program=duplicate,
                        branch=branch,
                        objective="Revisit the Charter hypothesis after a materially overlapping IdeaContribution.",
                    )
                _event(
                    session,
                    "IDEA_CONTRIBUTED",
                    "RESEARCH_PROGRAM",
                    duplicate.id,
                    {"action": "WAKE_EXISTING"},
                    actor_kind="HUMAN",
                )
                session.flush()
                return _program_view(session, duplicate).model_dump(mode="json")

            preview = _charter_preview(payload.idea)
            universe_version_ids, allowed_data_domains, market_scope = (
                _resolve_frozen_research_scope(session, preview)
            )
            now = _now()
            charter = ResearchCharter(
                original_idea_text=preview.original_idea_text,
                research_question=preview.research_question or preview.original_idea_text,
                market_scope=market_scope,
                universe_version_ids=[str(value) for value in universe_version_ids],
                prediction_horizon=preview.prediction_horizon,
                allowed_data_domains=allowed_data_domains,
                explicit_exclusions=preview.explicit_exclusions,
                material_assumptions=preview.material_assumptions,
                system_assumptions=[
                    *preview.system_assumptions,
                    "Universe Versions and data domains were frozen from executable governed Discovery revisions at Program creation.",
                ],
                created_at=now,
            )
            session.add(charter)
            session.flush()
            relationship_type = None
            inherited_from = None
            source_program_id = None
            if duplicate is not None:
                source_program_id = duplicate.id
                relationship_type = (
                    "INDEPENDENT_WITH_INHERITED_EVIDENCE"
                    if payload.overlap_action == "independent-program"
                    else "RELATED_PROGRAM"
                )
                inherited_from = duplicate.id
            program = ResearchProgram(
                charter_id=charter.id,
                title=payload.idea.strip().splitlines()[0][:120],
                state="ACTIVE",
                source_program_id=source_program_id,
                relationship_type=relationship_type,
                evidence_inherited_from_program_id=inherited_from,
            )
            session.add(program)
            session.flush()
            branch = ResearchBranch(
                program_id=program.id,
                derivation_type="ROOT",
                hypothesis=preview.research_question or payload.idea.strip(),
                changed_assumptions=[],
                preserved_constraints=preview.explicit_exclusions,
                state="ACTIVE",
                created_at=now,
            )
            session.add(branch)
            session.flush()
            _queue_mission(
                session,
                program=program,
                branch=branch,
                objective=f"Test the Charter hypothesis within {market_scope}.",
            )
            _event(
                session,
                "PROGRAM_CREATED",
                "RESEARCH_PROGRAM",
                program.id,
                {
                    "program_id": str(program.id),
                    "charter_id": str(charter.id),
                    "source_program_id": str(source_program_id) if source_program_id else None,
                    "evidence_inherited_from_program_id": (
                        str(inherited_from) if inherited_from else None
                    ),
                },
                actor_kind="HUMAN",
            )
            if duplicate is not None:
                session.add(
                    IdeaContribution(
                        program_id=duplicate.id,
                        idea_text=payload.idea.strip(),
                        action=relationship_type or "RELATED_PROGRAM",
                        created_at=now,
                    )
                )
            session.flush()
            return _program_view(session, program).model_dump(mode="json")

        return _idempotent(
            session,
            idempotency_key,
            "research-program.create",
            payload,
            action,
            status_code=201,
        )


@router.get("/research-programs", response_model=list[ResearchProgramView])
def list_programs(request: Request) -> list[ResearchProgramView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [
            _program_view(session, item)
            for item in session.scalars(
                select(ResearchProgram).order_by(ResearchProgram.created_at.desc())
            )
        ]


@router.get("/research-programs/{program_id}", response_model=ResearchProgramView)
def get_program(program_id: UUID, request: Request) -> ResearchProgramView:
    factory = request.app.state.session_factory
    with factory() as session:
        item = session.get(ResearchProgram, program_id)
        if item is None:
            raise QfError("RESEARCH_PROGRAM_NOT_FOUND", "Research Program was not found.", 404)
        return _program_view(session, item)


def _program_action(
    program_id: UUID,
    payload: ProgramActionInput,
    request: Request,
    idempotency_key: str | None,
    action_name: str,
) -> dict[str, Any]:
    transitions = {
        "pause": (None, "PAUSED"),
        "resume": ("PAUSED", "ACTIVE"),
        "archive": (None, "ARCHIVED"),
        "restore": ("ARCHIVED", "ACTIVE"),
    }
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            item = session.execute(
                select(ResearchProgram).where(ResearchProgram.id == program_id).with_for_update()
            ).scalar_one_or_none()
            if item is None:
                raise QfError("RESEARCH_PROGRAM_NOT_FOUND", "Research Program was not found.", 404)
            required, target = transitions[action_name]
            if required and item.state != required:
                raise QfError(
                    "PROGRAM_STATE_CONFLICT",
                    f"Program must be {required} for {action_name}.",
                    409,
                    {"state": item.state},
                )
            if action_name == "pause" and item.state == "ARCHIVED":
                raise QfError("PROGRAM_STATE_CONFLICT", "Archived Program cannot be paused.", 409)
            item.state = target
            item.revision += 1
            if action_name == "pause":
                item.blocked_reason = payload.reason
            if action_name in {"resume", "restore"}:
                item.wake_reason = payload.reason
            _event(
                session,
                f"PROGRAM_{target}",
                "RESEARCH_PROGRAM",
                item.id,
                {"program_id": str(item.id), "reason": payload.reason},
                actor_kind="HUMAN",
            )
            session.flush()
            return _program_view(session, item).model_dump(mode="json")

        return _idempotent(
            session,
            idempotency_key,
            f"research-program.{action_name}:{program_id}",
            payload,
            action,
        )


@router.post("/research-programs/{program_id}/pause", response_model=ResearchProgramView)
def pause_program(
    program_id: UUID,
    payload: ProgramActionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    return _program_action(program_id, payload, request, idempotency_key, "pause")


@router.post("/research-programs/{program_id}/resume", response_model=ResearchProgramView)
def resume_program(
    program_id: UUID,
    payload: ProgramActionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    return _program_action(program_id, payload, request, idempotency_key, "resume")


@router.post("/research-programs/{program_id}/archive", response_model=ResearchProgramView)
def archive_program(
    program_id: UUID,
    payload: ProgramActionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    return _program_action(program_id, payload, request, idempotency_key, "archive")


@router.post("/research-programs/{program_id}/restore", response_model=ResearchProgramView)
def restore_program(
    program_id: UUID,
    payload: ProgramActionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    return _program_action(program_id, payload, request, idempotency_key, "restore")


@router.get("/research-programs/{program_id}/missions", response_model=list[MissionView])
def list_program_missions(program_id: UUID, request: Request) -> list[MissionView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [
            _mission_view(item)
            for item in session.scalars(
                select(ResearchMission)
                .where(ResearchMission.program_id == program_id)
                .order_by(ResearchMission.id.asc())
            )
        ]


@router.get("/research-programs/{program_id}/activity", response_model=list[ActivityView])
def list_program_activity(program_id: UUID, request: Request) -> list[ActivityView]:
    factory = request.app.state.session_factory
    with factory() as session:
        result: list[ActivityView] = []
        for item in session.scalars(
            select(Event)
            .where(Event.aggregate_type == "RESEARCH_PROGRAM", Event.aggregate_id == program_id)
            .order_by(Event.id.desc())
            .limit(500)
        ):
            raw_mission = item.payload.get("mission_id")
            mission_id = None
            if raw_mission:
                try:
                    mission_id = UUID(str(raw_mission))
                except ValueError:
                    mission_id = None
            result.append(
                ActivityView(
                    id=item.id,
                    kind=item.kind,
                    aggregate_type=item.aggregate_type,
                    aggregate_id=item.aggregate_id,
                    mission_id=mission_id,
                    payload=item.payload,
                    created_at=_iso(item.created_at) or "",
                )
            )
        return result


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
            raise QfError(
                "ALPHA_QUALIFICATION_NOT_FOUND", "Alpha Qualification was not found.", 404
            )
        return _alpha_view(item)


@router.get("/portfolio-mandates", response_model=list[MandateView])
def list_mandates(request: Request) -> list[MandateView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [
            MandateView(
                id=item.id,
                key=item.key,
                name=item.name,
                enabled=item.enabled,
                latest_version_id=item.latest_version_id,
                spec_json=item.spec_json,
                state=item.state,
            )
            for item in session.scalars(select(PortfolioMandate).order_by(PortfolioMandate.name))
        ]


def _toggle_mandate(
    mandate_id: UUID, request: Request, idempotency_key: str | None, enabled: bool
) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            item = session.execute(
                select(PortfolioMandate).where(PortfolioMandate.id == mandate_id).with_for_update()
            ).scalar_one_or_none()
            if item is None:
                raise QfError("MANDATE_NOT_FOUND", "Portfolio Mandate was not found.", 404)
            item.enabled = enabled
            _event(
                session,
                "MANDATE_ENABLED" if enabled else "MANDATE_DISABLED",
                "PORTFOLIO_MANDATE",
                item.id,
                {},
                actor_kind="HUMAN",
            )
            session.flush()
            return MandateView(
                id=item.id,
                key=item.key,
                name=item.name,
                enabled=item.enabled,
                latest_version_id=item.latest_version_id,
                spec_json=item.spec_json,
                state=item.state,
            ).model_dump(mode="json")

        return _idempotent(
            session,
            idempotency_key,
            f"portfolio-mandate.{'enable' if enabled else 'disable'}:{mandate_id}",
            {},
            action,
        )


@router.post("/portfolio-mandates/{mandate_id}/enable", response_model=MandateView)
def enable_mandate(
    mandate_id: UUID,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    return _toggle_mandate(mandate_id, request, idempotency_key, True)


@router.post("/portfolio-mandates/{mandate_id}/disable", response_model=MandateView)
def disable_mandate(
    mandate_id: UUID,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    return _toggle_mandate(mandate_id, request, idempotency_key, False)


@router.get("/portfolio-programs", response_model=list[PortfolioProgramView])
def list_portfolio_programs(request: Request) -> list[PortfolioProgramView]:
    factory = request.app.state.session_factory
    with factory() as session:
        result: list[PortfolioProgramView] = []
        for item in session.scalars(
            select(PortfolioProgram).order_by(PortfolioProgram.created_at.desc())
        ):
            count = session.scalar(
                select(func.count())
                .select_from(PortfolioCandidate)
                .where(PortfolioCandidate.portfolio_program_id == item.id)
            )
            result.append(
                PortfolioProgramView(
                    id=item.id,
                    mandate_version_id=item.mandate_version_id,
                    mandate_name=item.mandate_name,
                    state=item.state,
                    created_at=_iso(item.created_at),
                    updated_at=_iso(item.updated_at),
                    candidate_count=int(count or 0),
                    current_candidate_id=item.current_candidate_id,
                )
            )
        return result


@router.get("/portfolio-candidates/{candidate_id}", response_model=CandidateView)
def get_candidate(candidate_id: UUID, request: Request) -> CandidateView:
    factory = request.app.state.session_factory
    with factory() as session:
        item = session.get(PortfolioCandidate, candidate_id)
        if item is None:
            raise QfError("CANDIDATE_NOT_FOUND", "Portfolio Candidate was not found.", 404)
        return _candidate_view(item)


@router.get("/approvals", response_model=list[ApprovalView])
def list_approvals(request: Request) -> list[ApprovalView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [
            _approval_view(session, item)
            for item in session.scalars(
                select(ApprovalSnapshot).order_by(ApprovalSnapshot.created_at.desc())
            )
        ]


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
        approval = session.execute(
            select(ApprovalSnapshot).where(ApprovalSnapshot.id == approval_id).with_for_update()
        ).scalar_one_or_none()
        if approval is None:
            return False
        if approval.state == "PENDING" and _is_past(approval.valid_until):
            approval.state = "EXPIRED"
            approval.revision += 1
            _event(session, "APPROVAL_EXPIRED", "APPROVAL", approval.id, {}, actor_kind="SYSTEM")
            return True
    return False


def _feedback_contract_snapshot(downstream: DownstreamSystem, purpose: str) -> dict[str, Any]:
    configured = downstream.public_config.get("feedback_contract", {})
    if not isinstance(configured, dict):
        configured = {}
    required_fields = configured.get("required_fields", [])
    if not isinstance(required_fields, list) or not all(
        isinstance(item, str) for item in required_fields
    ):
        raise QfError(
            "FEEDBACK_CONTRACT_INVALID",
            "feedback_contract.required_fields must be a list of strings.",
            422,
        )
    try:
        minimum_duration = int(configured.get("minimum_observation_duration_seconds", 1))
        minimum_sample = int(configured.get("minimum_valid_sample_size", 1))
    except (TypeError, ValueError) as exc:
        raise QfError(
            "FEEDBACK_CONTRACT_INVALID", "Feedback contract minimums must be integers.", 422
        ) from exc
    if minimum_duration < 0 or minimum_sample < 1:
        raise QfError("FEEDBACK_CONTRACT_INVALID", "Feedback contract minimums are invalid.", 422)
    return {
        "feedback_contract_version_id": downstream.feedback_contract_version,
        "purpose": purpose,
        "minimum_observation_duration_seconds": minimum_duration,
        "minimum_valid_sample_size": minimum_sample,
        "required_fields": required_fields,
        "accepted_package_contracts": configured.get(
            "accepted_package_contracts", [downstream.package_contract_version]
        ),
        "accepted_arrow_contracts": configured.get(
            "accepted_arrow_contracts", ["arrow-ipc-file-v1"]
        ),
        "disclosure_policy": configured.get("disclosure_policy", "FULL"),
    }


@router.post("/approvals/{approval_id}/approve", response_model=ApprovalView)
def approve_candidate(
    approval_id: UUID,
    payload: ApprovalApproveInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    if _expire_approval_if_needed(request, approval_id):
        raise QfError("APPROVAL_EXPIRED", "Approval validity window has expired.", 409)
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            approval = session.execute(
                select(ApprovalSnapshot).where(ApprovalSnapshot.id == approval_id).with_for_update()
            ).scalar_one_or_none()
            if approval is None:
                raise QfError("APPROVAL_NOT_FOUND", "Approval Snapshot was not found.", 404)
            if approval.state != payload.expected_state or approval.state != "PENDING":
                raise QfError(
                    "APPROVAL_STATE_CONFLICT",
                    "Approval state changed before the decision.",
                    409,
                    {"expected": payload.expected_state, "actual": approval.state},
                )
            downstream = session.get(DownstreamSystem, payload.downstream_system_id)
            if (
                downstream is None
                or not downstream.enabled
                or downstream.preflight_state != "READY"
            ):
                raise QfError("DOWNSTREAM_NOT_READY", "Selected downstream is not ready.", 409)
            if downstream.environment_type != approval.purpose:
                raise QfError(
                    "DOWNSTREAM_INCOMPATIBLE",
                    "Downstream environment does not match Approval purpose.",
                    409,
                )
            if downstream.service_token_ciphertext is None:
                raise QfError(
                    "DOWNSTREAM_CREDENTIAL_NOT_CONFIGURED",
                    "Selected downstream has no service credential.",
                    409,
                )
            candidate = session.get(PortfolioCandidate, approval.candidate_id)
            if candidate is None:
                raise QfError("CANDIDATE_NOT_FOUND", "Approval candidate was not found.", 500)
            built = build_candidate_bundle(
                request.app.state.settings,
                approval=approval,
                candidate=candidate,
                downstream=downstream,
            )
            package = CandidateBundle(
                approval_id=approval.id,
                candidate_id=candidate.id,
                contract_version=BUNDLE_CONTRACT_VERSION,
                state="AVAILABLE",
                manifest_json=built.manifest,
                relative_path=built.relative_path,
                payload=built.operator_summary,
                created_at=_now(),
            )
            session.add(package)
            session.flush()
            contract = _feedback_contract_snapshot(downstream, approval.purpose)
            handoff = HandoffOffer(
                approval_id=approval.id,
                candidate_bundle_id=package.id,
                candidate_id=candidate.id,
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
            _event(
                session,
                "APPROVAL_APPROVED",
                "APPROVAL",
                approval.id,
                {"candidate_id": str(candidate.id), "handoff_id": str(handoff.id)},
                actor_kind="HUMAN",
            )
            _event(
                session,
                "HANDOFF_AVAILABLE",
                "HANDOFF",
                handoff.id,
                {"candidate_id": str(candidate.id), "approval_id": str(approval.id)},
            )
            session.flush()
            return _approval_view(session, approval).model_dump(mode="json")

        return _idempotent(
            session, idempotency_key, f"approval.approve:{approval_id}", payload, action
        )


@router.post("/approvals/{approval_id}/reject", response_model=ApprovalView)
def reject_candidate(
    approval_id: UUID,
    payload: ApprovalRejectInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    if _expire_approval_if_needed(request, approval_id):
        raise QfError("APPROVAL_EXPIRED", "Approval validity window has expired.", 409)
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            approval = session.execute(
                select(ApprovalSnapshot).where(ApprovalSnapshot.id == approval_id).with_for_update()
            ).scalar_one_or_none()
            if approval is None:
                raise QfError("APPROVAL_NOT_FOUND", "Approval Snapshot was not found.", 404)
            if approval.state != payload.expected_state or approval.state != "PENDING":
                raise QfError("APPROVAL_STATE_CONFLICT", "Approval state changed.", 409)
            approval.state = "REJECTED"
            approval.revision += 1
            approval.human_report = {
                "decision": "REJECT",
                "reason_code": payload.reason_code,
                "note": payload.note,
            }
            _event(
                session,
                "APPROVAL_REJECTED",
                "APPROVAL",
                approval.id,
                {"reason_code": payload.reason_code},
                actor_kind="HUMAN",
            )
            session.flush()
            return _approval_view(session, approval).model_dump(mode="json")

        return _idempotent(
            session, idempotency_key, f"approval.reject:{approval_id}", payload, action
        )


@router.get("/handoffs", response_model=list[HandoffView])
def list_handoffs(request: Request) -> list[HandoffView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [
            _handoff_view(session, item)
            for item in session.scalars(
                select(HandoffOffer).order_by(HandoffOffer.created_at.desc())
            )
        ]


@router.post("/handoffs/{handoff_id}/revoke", response_model=HandoffView)
def revoke_handoff(
    handoff_id: UUID,
    payload: HandoffRevokeInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            handoff = session.execute(
                select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()
            ).scalar_one_or_none()
            if handoff is None:
                raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
            if handoff.state not in {"APPROVED", "PUBLISHING", "AVAILABLE"}:
                raise QfError(
                    "HANDOFF_ALREADY_OWNED",
                    "Claimed or terminal Handoff cannot be revoked by QuaZonai.",
                    409,
                )
            handoff.state = "REVOKED"
            handoff.stale_reason = payload.reason_code
            _event(
                session,
                "HANDOFF_REVOKED",
                "HANDOFF",
                handoff.id,
                {"reason_code": payload.reason_code},
                actor_kind="HUMAN",
            )
            session.flush()
            return _handoff_view(session, handoff).model_dump(mode="json")

        return _idempotent(
            session, idempotency_key, f"handoff.revoke:{handoff_id}", payload, action
        )


def _authenticate_handoff(
    session: Session, request: Request, handoff: HandoffOffer, authorization: str | None
) -> DownstreamSystem:
    downstream = session.get(DownstreamSystem, handoff.downstream_system_id)
    if downstream is None:
        raise QfError("DOWNSTREAM_NOT_FOUND", "Handoff Downstream System is missing.", 500)
    authenticate_downstream(request.app.state.settings, downstream, authorization)
    return downstream


def _expire_handoff_if_needed(request: Request, handoff_id: UUID) -> bool:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():
        handoff = session.execute(
            select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()
        ).scalar_one_or_none()
        if handoff is None:
            return False
        if handoff.state == "AVAILABLE" and _is_past(handoff.claim_deadline):
            handoff.state = "EXPIRED"
            _event(session, "HANDOFF_EXPIRED", "HANDOFF", handoff.id, {})
            return True
    return False


@router.post("/handoffs/{handoff_id}/claim", response_model=HandoffView)
def claim_handoff(
    handoff_id: UUID,
    payload: HandoffStateInput,
    request: Request,
    authorization: str | None = Header(default=None, alias="Authorization"),
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    if _expire_handoff_if_needed(request, handoff_id):
        raise QfError("HANDOFF_EXPIRED", "Handoff claim deadline expired.", 409)
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            handoff = session.execute(
                select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()
            ).scalar_one_or_none()
            if handoff is None:
                raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
            _authenticate_handoff(session, request, handoff, authorization)
            if handoff.state != "AVAILABLE":
                raise QfError(
                    "HANDOFF_STATE_CONFLICT", "Only Available Handoffs can be claimed.", 409
                )
            if payload.expected_state and payload.expected_state != handoff.state:
                raise QfError("HANDOFF_STATE_CONFLICT", "Handoff state changed before claim.", 409)
            handoff.state = "CLAIMED"
            handoff.claimed_at = _now()
            _event(session, "HANDOFF_CLAIMED", "HANDOFF", handoff.id, {})
            session.flush()
            return _handoff_view(session, handoff).model_dump(mode="json")

        return _idempotent(session, idempotency_key, f"handoff.claim:{handoff_id}", payload, action)


@router.post("/handoffs/{handoff_id}/accept", response_model=HandoffView)
def accept_handoff(
    handoff_id: UUID,
    payload: HandoffStateInput,
    request: Request,
    authorization: str | None = Header(default=None, alias="Authorization"),
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            handoff = session.execute(
                select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()
            ).scalar_one_or_none()
            if handoff is None:
                raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
            _authenticate_handoff(session, request, handoff, authorization)
            if handoff.state != "CLAIMED":
                raise QfError(
                    "HANDOFF_STATE_CONFLICT", "Only Claimed Handoffs can be accepted.", 409
                )
            if payload.expected_state and payload.expected_state != handoff.state:
                raise QfError(
                    "HANDOFF_STATE_CONFLICT", "Handoff state changed before acceptance.", 409
                )
            handoff.state = "DOWNSTREAM_ACCEPTED"
            handoff.accepted_at = _now()
            handoff.feedback_state = "FEEDBACK_PENDING"
            _event(session, "HANDOFF_ACCEPTED", "HANDOFF", handoff.id, {})
            session.flush()
            return _handoff_view(session, handoff).model_dump(mode="json")

        return _idempotent(
            session, idempotency_key, f"handoff.accept:{handoff_id}", payload, action
        )


@router.post("/handoffs/{handoff_id}/reject", response_model=HandoffView)
def downstream_reject_handoff(
    handoff_id: UUID,
    payload: HandoffStateInput,
    request: Request,
    authorization: str | None = Header(default=None, alias="Authorization"),
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            handoff = session.execute(
                select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()
            ).scalar_one_or_none()
            if handoff is None:
                raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
            _authenticate_handoff(session, request, handoff, authorization)
            if handoff.state not in {"AVAILABLE", "CLAIMED"}:
                raise QfError("HANDOFF_STATE_CONFLICT", "Handoff cannot be rejected now.", 409)
            handoff.state = "DOWNSTREAM_REJECTED"
            handoff.stale_reason = payload.reason_code
            _event(
                session,
                "HANDOFF_DOWNSTREAM_REJECTED",
                "HANDOFF",
                handoff.id,
                {"reason_code": payload.reason_code},
            )
            session.flush()
            return _handoff_view(session, handoff).model_dump(mode="json")

        return _idempotent(
            session, idempotency_key, f"handoff.downstream-reject:{handoff_id}", payload, action
        )


@router.get("/handoffs/{handoff_id}/package", response_class=FileResponse)
def get_handoff_package(
    handoff_id: UUID,
    request: Request,
    authorization: str | None = Header(default=None, alias="Authorization"),
) -> FileResponse:
    factory = request.app.state.session_factory
    with factory() as session:
        handoff = session.get(HandoffOffer, handoff_id)
        if handoff is None:
            raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
        _authenticate_handoff(session, request, handoff, authorization)
        if handoff.state not in {
            "CLAIMED",
            "DOWNSTREAM_ACCEPTED",
            "FEEDBACK_PENDING",
            "FEEDBACK_IN_PROGRESS",
            "FEEDBACK_PARTIAL",
            "FEEDBACK_COMPLETE",
        }:
            raise QfError(
                "HANDOFF_PACKAGE_UNAVAILABLE", "Package is not available in this state.", 409
            )
        package = session.get(CandidateBundle, handoff.candidate_bundle_id)
        if package is None:
            raise QfError("CANDIDATE_PACKAGE_NOT_FOUND", "Candidate Package was not found.", 500)
        archive = resolve_bundle_archive(request.app.state.settings, package.relative_path)
        return FileResponse(
            archive, media_type="application/zip", filename=f"candidate-bundle-{package.id}.zip"
        )


def _validate_complete_feedback(
    handoff: HandoffOffer, payload: FeedbackInput
) -> tuple[datetime, datetime, int]:
    problems: list[str] = []
    if payload.observation_start is None:
        problems.append("observation_start is required")
    if payload.observation_end is None:
        problems.append("observation_end is required")
    if payload.sample_size is None:
        problems.append("sample_size is required")
    if problems:
        raise QfError(
            "FEEDBACK_CONTRACT_INVALID",
            "Complete feedback does not satisfy the frozen Feedback Contract.",
            422,
            {"problems": problems},
        )
    assert payload.observation_start is not None
    assert payload.observation_end is not None
    assert payload.sample_size is not None
    start = payload.observation_start
    end = payload.observation_end
    if end <= start:
        problems.append("observation_end must be after observation_start")
    contract = handoff.feedback_contract_snapshot or {}
    minimum_duration = int(contract.get("minimum_observation_duration_seconds", 1))
    minimum_sample = int(contract.get("minimum_valid_sample_size", 1))
    if end > start and (end - start).total_seconds() < minimum_duration:
        problems.append(f"observation duration must be at least {minimum_duration} seconds")
    if payload.sample_size < minimum_sample:
        problems.append(f"sample_size must be at least {minimum_sample}")
    required_fields = contract.get("required_fields", [])
    if not isinstance(required_fields, list):
        problems.append("frozen required_fields is invalid")
    else:
        missing = [
            field
            for field in required_fields
            if field not in payload.evidence or payload.evidence[field] is None
        ]
        if missing:
            problems.append(
                f"missing required evidence fields: {', '.join(str(item) for item in missing)}"
            )
    if problems:
        raise QfError(
            "FEEDBACK_CONTRACT_INVALID",
            "Complete feedback does not satisfy the frozen Feedback Contract.",
            422,
            {"problems": problems},
        )
    return start, end, payload.sample_size


@router.post("/handoffs/{handoff_id}/feedback", response_model=HandoffView)
def submit_feedback(
    handoff_id: UUID,
    payload: FeedbackInput,
    request: Request,
    authorization: str | None = Header(default=None, alias="Authorization"),
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            handoff = session.execute(
                select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()
            ).scalar_one_or_none()
            if handoff is None:
                raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
            _authenticate_handoff(session, request, handoff, authorization)
            if handoff.state not in {
                "DOWNSTREAM_ACCEPTED",
                "FEEDBACK_PENDING",
                "FEEDBACK_IN_PROGRESS",
                "FEEDBACK_PARTIAL",
            }:
                raise QfError("HANDOFF_STATE_CONFLICT", "Handoff is not accepting feedback.", 409)
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
                _event(
                    session,
                    "FORWARD_EVIDENCE_RECORDED",
                    "HANDOFF",
                    handoff.id,
                    {"episode_id": str(episode.id), "state": "FEEDBACK_COMPLETE"},
                )
            else:
                handoff.state = payload.state
                handoff.feedback_state = payload.state
                _event(
                    session,
                    "HANDOFF_FEEDBACK_STATUS",
                    "HANDOFF",
                    handoff.id,
                    {"state": payload.state},
                )
            session.flush()
            return _handoff_view(session, handoff).model_dump(mode="json")

        return _idempotent(
            session, idempotency_key, f"handoff.feedback:{handoff_id}", payload, action
        )


@router.get("/data-sources", response_model=list[DataSourceView])
def list_data_sources(request: Request) -> list[DataSourceView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [
            DataSourceView(
                id=item.id,
                name=item.name,
                provider=item.provider,
                state=item.state,
                universe_scope=item.universe_scope,
                fields=item.fields,
                update_cadence=item.update_cadence,
                preflight_state=item.preflight_state,
            )
            for item in session.scalars(
                select(GovernedDataSource).order_by(GovernedDataSource.name)
            )
        ]


@router.get("/data-sources/{source_id}", response_model=DataSourceView)
def get_data_source(source_id: UUID, request: Request) -> DataSourceView:
    factory = request.app.state.session_factory
    with factory() as session:
        item = session.get(GovernedDataSource, source_id)
        if item is None:
            raise QfError("DATA_SOURCE_NOT_FOUND", "Data Source was not found.", 404)
        return DataSourceView(
            id=item.id,
            name=item.name,
            provider=item.provider,
            state=item.state,
            universe_scope=item.universe_scope,
            fields=item.fields,
            update_cadence=item.update_cadence,
            preflight_state=item.preflight_state,
        )


@router.post("/data-sources", response_model=DataSourceView, status_code=201)
def create_data_source(
    payload: DataSourceInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            duplicate = session.scalar(
                select(GovernedDataSource).where(GovernedDataSource.name == payload.name.strip())
            )
            if duplicate:
                raise QfError("DATA_SOURCE_NAME_CONFLICT", "Data Source name already exists.", 409)
            item = GovernedDataSource(
                name=payload.name.strip(),
                provider=payload.provider,
                state="ACTIVE",
                universe_scope=payload.universe_scope,
                fields=payload.fields,
                update_cadence=payload.update_cadence,
                preflight_state="READY",
                public_config=payload.public_config,
            )
            session.add(item)
            session.flush()
            _event(
                session, "DATA_SOURCE_REGISTERED", "DATA_SOURCE", item.id, {}, actor_kind="HUMAN"
            )
            return DataSourceView(
                id=item.id,
                name=item.name,
                provider=item.provider,
                state=item.state,
                universe_scope=item.universe_scope,
                fields=item.fields,
                update_cadence=item.update_cadence,
                preflight_state=item.preflight_state,
            ).model_dump(mode="json")

        return _idempotent(
            session, idempotency_key, "data-source.create", payload, action, status_code=201
        )


@router.get("/datasets", response_model=list[DatasetView])
def list_datasets(request: Request) -> list[DatasetView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [
            DatasetView(
                id=item.id,
                data_source_id=item.data_source_id,
                universe_version_id=item.universe_version_id,
                universe_name=item.universe_name,
                revision_no=item.revision_no,
                schema_version=item.schema_version,
                event_start=_iso(item.event_start),
                event_end=_iso(item.event_end),
                available_start=_iso(item.available_start),
                available_end=_iso(item.available_end),
                row_count=item.row_count,
                quality_state=item.quality_state,
                point_in_time_state=item.point_in_time_state,
                partition=item.partition,
                created_at=_iso(item.created_at) or "",
            )
            for item in session.scalars(
                select(DatasetRevision).order_by(DatasetRevision.created_at.desc())
            )
        ]


@router.get("/universes", response_model=list[UniverseView])
def list_universes(request: Request) -> list[UniverseView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [
            UniverseView(
                id=item.id,
                universe_key=item.universe_key,
                version_no=item.version_no,
                name=item.name,
                state=item.state,
                spec_json=item.spec_json,
            )
            for item in session.scalars(
                select(MarketUniverseVersion).order_by(
                    MarketUniverseVersion.universe_key, MarketUniverseVersion.version_no.desc()
                )
            )
        ]


def _downstream_view(item: DownstreamSystem) -> DownstreamView:
    return DownstreamView(
        id=item.id,
        name=item.name,
        environment_type=item.environment_type,
        enabled=item.enabled,
        package_contract_version=item.package_contract_version,
        feedback_contract_version=item.feedback_contract_version,
        compatibility=item.compatibility,
        preflight_state=item.preflight_state,
    )


@router.get("/downstream-systems", response_model=list[DownstreamView])
def list_downstreams(request: Request) -> list[DownstreamView]:
    factory = request.app.state.session_factory
    with factory() as session:
        return [
            _downstream_view(item)
            for item in session.scalars(select(DownstreamSystem).order_by(DownstreamSystem.name))
        ]


@router.get("/downstream-systems/{downstream_id}", response_model=DownstreamView)
def get_downstream(downstream_id: UUID, request: Request) -> DownstreamView:
    factory = request.app.state.session_factory
    with factory() as session:
        item = session.get(DownstreamSystem, downstream_id)
        if item is None:
            raise QfError("DOWNSTREAM_NOT_FOUND", "Downstream System was not found.", 404)
        return _downstream_view(item)


@router.post("/downstream-systems", response_model=DownstreamRegistrationView, status_code=201)
def create_downstream(
    payload: DownstreamInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    if payload.environment_type not in {"PAPER", "LIVE", "EXTERNAL_BACKTEST"}:
        raise QfError("DOWNSTREAM_ENVIRONMENT_INVALID", "Downstream environment is invalid.", 422)
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            duplicate = session.scalar(
                select(DownstreamSystem).where(DownstreamSystem.name == payload.name.strip())
            )
            if duplicate:
                raise QfError("DOWNSTREAM_NAME_CONFLICT", "Downstream name already exists.", 409)
            downstream_id = uuid4()
            issued = issue_service_token(request.app.state.settings, downstream_id)
            item = DownstreamSystem(
                id=downstream_id,
                name=payload.name.strip(),
                environment_type=payload.environment_type,
                enabled=payload.enabled,
                package_contract_version=payload.package_contract_version,
                feedback_contract_version=payload.feedback_contract_version,
                compatibility=payload.compatibility,
                preflight_state="READY",
                public_config=payload.public_config,
            )
            install_service_token(item, issued)
            _feedback_contract_snapshot(item, payload.environment_type)
            session.add(item)
            session.flush()
            _event(
                session,
                "DOWNSTREAM_REGISTERED",
                "DOWNSTREAM_SYSTEM",
                item.id,
                {},
                actor_kind="HUMAN",
            )
            return DownstreamRegistrationView(
                **_downstream_view(item).model_dump(), service_token=issued.token
            ).model_dump(mode="json")

        return _idempotent(
            session, idempotency_key, "downstream.create", payload, action, status_code=201
        )


@router.post(
    "/downstream-systems/{downstream_id}/rotate-service-token", response_model=DownstreamTokenView
)
def rotate_downstream_token(
    downstream_id: UUID,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session, session.begin():

        def action() -> dict[str, Any]:
            item = session.execute(
                select(DownstreamSystem)
                .where(DownstreamSystem.id == downstream_id)
                .with_for_update()
            ).scalar_one_or_none()
            if item is None:
                raise QfError("DOWNSTREAM_NOT_FOUND", "Downstream System was not found.", 404)
            issued = issue_service_token(request.app.state.settings, item.id)
            install_service_token(item, issued)
            _event(
                session,
                "DOWNSTREAM_SERVICE_TOKEN_ROTATED",
                "DOWNSTREAM_SYSTEM",
                item.id,
                {},
                actor_kind="HUMAN",
            )
            return DownstreamTokenView(
                downstream_system_id=item.id, service_token=issued.token
            ).model_dump(mode="json")

        return _idempotent(
            session, idempotency_key, f"downstream.rotate-service-token:{downstream_id}", {}, action
        )


@router.get("/readiness", response_model=dict[str, Any])
def readiness(request: Request) -> dict[str, Any]:
    factory = request.app.state.session_factory
    with factory() as session:
        data_ready = bool(
            session.scalar(
                select(func.count())
                .select_from(GovernedDataSource)
                .where(
                    GovernedDataSource.state == "ACTIVE",
                    GovernedDataSource.preflight_state == "READY",
                )
            )
            or session.scalar(select(func.count()).select_from(DatasetRevision))
        )
        paper_ready = bool(
            session.scalar(
                select(func.count())
                .select_from(DownstreamSystem)
                .where(
                    DownstreamSystem.environment_type == "PAPER",
                    DownstreamSystem.enabled.is_(True),
                    DownstreamSystem.preflight_state == "READY",
                    DownstreamSystem.service_token_ciphertext.is_not(None),
                )
            )
        )
        live_downstream_ready = bool(
            session.scalar(
                select(func.count())
                .select_from(DownstreamSystem)
                .where(
                    DownstreamSystem.environment_type == "LIVE",
                    DownstreamSystem.enabled.is_(True),
                    DownstreamSystem.preflight_state == "READY",
                    DownstreamSystem.service_token_ciphertext.is_not(None),
                )
            )
        )
        paper_feedback_ready = bool(
            session.scalar(
                select(func.count())
                .select_from(ForwardEvidenceEpisode)
                .join(HandoffOffer, ForwardEvidenceEpisode.handoff_id == HandoffOffer.id)
                .where(
                    HandoffOffer.purpose == "PAPER",
                    ForwardEvidenceEpisode.state == "FEEDBACK_COMPLETE",
                )
            )
        )
        return {
            "SYSTEM_READY": True,
            "RESEARCH_READY": data_ready,
            "PAPER_HANDOFF_READY": paper_ready,
            "LIVE_HANDOFF_READY": live_downstream_ready and paper_feedback_ready,
        }
