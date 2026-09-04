"""Idea Draft, Research Program, Cycle, and Mission HTTP contracts."""

from __future__ import annotations

from datetime import UTC, datetime
from typing import Any, Callable
from uuid import UUID

from fastapi import APIRouter, Header, Query, Request
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from db.models import (
    AgentSession,
    AgentTurn,
    ClarificationQuestion,
    IdeaDraft,
    MissionArtifact,
    MissionDependency,
    PublicMutationReceipt,
    ResearchBranch,
    ResearchCharter,
    ResearchCycle,
    ResearchMission,
    ResearchProgram,
)
from errors import QfError
from research_lifecycle import (
    answer_draft,
    create_draft,
    draft_answers,
    draft_questions,
    start_draft,
    transition_program,
)


router = APIRouter(prefix="/api/v1", tags=["research"])


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class ClarificationQuestionView(StrictModel):
    id: UUID
    key: str
    question: str
    answered: bool


class CharterView(StrictModel):
    id: UUID
    original_idea_text: str
    research_question: str
    market_scope: list[str]
    universe_version_ids: list[UUID]
    prediction_horizon: str | None = None
    allowed_data_domains: list[str]
    explicit_exclusions: list[str]
    clarification_transcript: list[dict[str, str]]
    created_at: datetime


class IdeaDraftView(StrictModel):
    id: UUID
    original_idea_text: str
    state: str
    stage: str
    outcome: str | None = None
    next_action: str | None = None
    blocking_reasons: list[str] = Field(default_factory=list)
    revision: int
    clarification_questions: list[ClarificationQuestionView]
    charter: CharterView | None = None


class CreateIdeaDraftInput(StrictModel):
    original_idea_text: str = Field(min_length=12, max_length=20_000)


class AnswerIdeaDraftInput(StrictModel):
    answers: dict[str, str] = Field(min_length=1)
    expected_revision: int = Field(ge=1)


class StartIdeaDraftInput(StrictModel):
    expected_revision: int = Field(ge=1)
    title: str | None = Field(default=None, min_length=1, max_length=240)
    universe_version_ids: list[UUID] | None = None


class ProgramActionInput(StrictModel):
    expected_revision: int | None = Field(default=None, ge=1)
    reason: str | None = Field(default=None, max_length=4_000)


class ResearchProgramView(StrictModel):
    id: UUID
    title: str
    charter_id: UUID
    state: str
    stage: str
    outcome: str | None = None
    next_action: str | None = None
    blocking_reasons: list[str] = Field(default_factory=list)
    revision: int
    current_cycle_id: UUID | None = None
    charter: CharterView
    created_at: datetime
    updated_at: datetime
    branch_count: int
    mission_count: int


class ResearchProgramPage(StrictModel):
    items: list[ResearchProgramView]
    next_cursor: UUID | None = None


class ResearchCycleView(StrictModel):
    id: UUID
    cycle_no: int
    trigger: str
    trigger_ref_id: UUID | None = None
    state: str
    mission_budget: int
    replan_budget: int
    runtime_configuration_revision: int
    started_at: datetime | None = None
    finished_at: datetime | None = None
    summary: dict[str, Any]
    created_at: datetime


class ResearchCyclePage(StrictModel):
    items: list[ResearchCycleView]
    next_cursor: UUID | None = None


class MissionView(StrictModel):
    id: UUID
    program_id: UUID
    cycle_id: UUID | None = None
    branch_id: UUID
    mission_type: str
    role_profile: str | None = None
    state: str
    outcome: str | None = None
    objective: str | None = None
    dependencies: list[UUID] = Field(default_factory=list)
    contract_version: str
    max_turns: int
    max_tool_calls: int
    attempt: int
    revision: int
    started_at: datetime | None = None
    finished_at: datetime | None = None
    error_code: str | None = None
    summary: str | None = None


class MissionGraphView(StrictModel):
    program_id: UUID
    cycle_id: UUID | None = None
    nodes: list[MissionView]


class AgentTurnView(StrictModel):
    id: UUID
    ordinal: int
    kind: str
    codex_turn_id: str
    state: str
    observable_summary: str | None = None
    tool_call_count: int
    started_at: datetime | None = None
    finished_at: datetime | None = None
    error_code: str | None = None


class MissionArtifactView(StrictModel):
    id: UUID
    turn_id: UUID | None = None
    kind: str
    schema_version: str
    revision: int
    state: str
    storage_uri: str
    metadata: dict[str, Any]
    created_at: datetime


_QUESTION_KEYS = ("market_scope", "horizon", "data_scope")
_QUESTION_ALIASES = {
    "scope": "market_scope",
    "universe": "market_scope",
    "prediction_horizon": "horizon",
    "data": "data_scope",
}


def _question_key(question: ClarificationQuestion) -> str:
    return _QUESTION_KEYS[question.ordinal - 1]


def _charter_view(charter: ResearchCharter) -> CharterView:
    market_scope = charter.market_scope
    return CharterView(
        id=charter.id,
        original_idea_text=charter.original_idea_text,
        research_question=charter.research_question,
        market_scope=[str(value) for value in market_scope]
        if isinstance(market_scope, list)
        else [str(market_scope)],
        universe_version_ids=[UUID(str(value)) for value in charter.universe_version_ids],
        prediction_horizon=charter.prediction_horizon,
        allowed_data_domains=[str(value) for value in charter.allowed_data_domains],
        explicit_exclusions=[str(value) for value in charter.explicit_exclusions],
        clarification_transcript=[
            {str(key): str(value) for key, value in item.items()}
            for item in charter.clarification_transcript
        ],
        created_at=charter.created_at,
    )


def _draft_view(session: Session, draft: IdeaDraft) -> IdeaDraftView:
    questions = draft_questions(session, draft.id)
    answers = draft_answers(session, (question.id for question in questions))
    charter = session.scalar(select(ResearchCharter).where(ResearchCharter.idea_draft_id == draft.id))
    complete = bool(questions) and len(answers) == len(questions)
    if draft.state == "STARTED":
        next_action = "AUTONOMOUS_RESEARCH"
        blockers: list[str] = []
    elif complete:
        next_action = "START_PROGRAM"
        blockers = []
    else:
        next_action = "ANSWER_CLARIFICATIONS"
        blockers = ["CLARIFICATION_REQUIRED"]
    return IdeaDraftView(
        id=draft.id,
        original_idea_text=draft.original_idea_text,
        state=draft.state,
        stage=draft.state,
        next_action=next_action,
        blocking_reasons=blockers,
        revision=draft.revision,
        clarification_questions=[
            ClarificationQuestionView(
                id=question.id,
                key=_question_key(question),
                question=question.question_text,
                answered=question.id in answers,
            )
            for question in questions
        ],
        charter=_charter_view(charter) if charter else None,
    )


def _program_status(program: ResearchProgram) -> tuple[str | None, list[str]]:
    if program.state == "ACTIVE":
        return "AUTONOMOUS_RESEARCH", []
    if program.state == "COOLING":
        return "WAIT_FOR_COOLING", ["COOLING"]
    if program.state == "PAUSED":
        return "RESUME_PROGRAM", ["PROGRAM_PAUSED"]
    if program.state == "ARCHIVED":
        return None, ["PROGRAM_ARCHIVED"]
    if program.state == "BLOCKED":
        return "ACTION_REQUIRED", [program.blocked_reason_code or "PROGRAM_BLOCKED"]
    if program.state == "WAITING_FOR_FEEDBACK":
        return "WAIT_FOR_FEEDBACK", ["FORWARD_EVIDENCE_REQUIRED"]
    return "REVIEW_APPROVAL", []


def _program_view(session: Session, program: ResearchProgram) -> ResearchProgramView:
    charter = session.get(ResearchCharter, program.charter_id)
    if charter is None:
        raise QfError("CHARTER_NOT_FOUND", "Research Charter is missing.", 500)
    next_action, blockers = _program_status(program)
    branch_count = session.scalar(
        select(func.count()).select_from(ResearchBranch).where(ResearchBranch.program_id == program.id)
    )
    mission_count = session.scalar(
        select(func.count()).select_from(ResearchMission).where(ResearchMission.program_id == program.id)
    )
    return ResearchProgramView(
        id=program.id,
        title=program.title,
        charter_id=program.charter_id,
        state=program.state,
        stage=program.state,
        next_action=next_action,
        blocking_reasons=blockers,
        revision=program.revision,
        current_cycle_id=program.current_cycle_id,
        charter=_charter_view(charter),
        created_at=program.created_at,
        updated_at=program.updated_at,
        branch_count=int(branch_count or 0),
        mission_count=int(mission_count or 0),
    )


def _mission_view(session: Session, mission: ResearchMission) -> MissionView:
    dependencies = list(
        session.scalars(
            select(MissionDependency.depends_on_mission_id).where(
                MissionDependency.mission_id == mission.id
            )
        )
    )
    return MissionView(
        id=mission.id,
        program_id=mission.program_id,
        cycle_id=mission.cycle_id,
        branch_id=mission.branch_id,
        mission_type=mission.mission_type,
        role_profile=mission.role_profile,
        state=mission.state,
        outcome=mission.outcome,
        objective=mission.objective,
        dependencies=dependencies,
        contract_version=mission.contract_version,
        max_turns=mission.max_turns,
        max_tool_calls=mission.max_tool_calls,
        attempt=mission.attempt,
        revision=mission.revision,
        started_at=mission.started_at,
        finished_at=mission.finished_at,
        error_code=mission.error_code,
        summary=mission.summary,
    )


def _topological_missions(session: Session, missions: list[ResearchMission]) -> list[ResearchMission]:
    """Present the persisted DAG in dependency order without adding an ordering fact."""
    by_id = {mission.id: mission for mission in missions}
    dependencies = {
        mission.id: {
            dependency
            for dependency in session.scalars(
                select(MissionDependency.depends_on_mission_id).where(
                    MissionDependency.mission_id == mission.id
                )
            )
            if dependency in by_id
        }
        for mission in missions
    }
    ordered: list[ResearchMission] = []
    remaining = dict(by_id)
    while remaining:
        ready = sorted(
            (mission_id for mission_id in remaining if dependencies[mission_id] <= {item.id for item in ordered}),
            key=str,
        )
        if not ready:
            return sorted(missions, key=lambda mission: str(mission.id))
        for mission_id in ready:
            ordered.append(remaining.pop(mission_id))
    return ordered


def _normalize(payload: BaseModel) -> dict[str, Any]:
    return payload.model_dump(mode="json", exclude_none=True)


def _idempotent(
    session: Session,
    key: str | None,
    operation: str,
    payload: BaseModel,
    action: Callable[[], dict[str, Any]],
    *,
    status_code: int,
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
                created_at=datetime.now(UTC),
            )
        )
    return result


def _idempotency_key(value: str | None) -> str | None:
    return value.strip() if value and value.strip() else None


def _answer_ids(
    session: Session, draft: IdeaDraft, submitted: dict[str, str]
) -> dict[UUID, str]:
    questions = draft_questions(session, draft.id)
    by_id = {str(question.id): question.id for question in questions}
    by_key = {_question_key(question): question.id for question in questions}
    result: dict[UUID, str] = {}
    for raw_key, answer in submitted.items():
        key = _QUESTION_ALIASES.get(raw_key, raw_key)
        question_id = by_id.get(key) or by_key.get(key)
        if question_id is None:
            raise QfError(
                "CLARIFICATION_QUESTION_INVALID",
                "An answer does not belong to this Idea Draft.",
                422,
                {"key": raw_key},
            )
        if question_id in result:
            raise QfError(
                "CLARIFICATION_QUESTION_DUPLICATED",
                "A clarification question was answered more than once.",
                422,
                {"key": raw_key},
            )
        result[question_id] = answer
    return result


@router.post("/idea-drafts", response_model=IdeaDraftView, status_code=201)
def create_idea_draft(
    payload: CreateIdeaDraftInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    with request.app.state.session_factory() as session, session.begin():
        def action() -> dict[str, Any]:
            draft = create_draft(session, payload.original_idea_text)
            session.flush()
            return _draft_view(session, draft).model_dump(mode="json")

        return _idempotent(
            session,
            _idempotency_key(idempotency_key),
            "idea-draft.create",
            payload,
            action,
            status_code=201,
        )


@router.get("/idea-drafts/{draft_id}", response_model=IdeaDraftView)
def get_idea_draft(draft_id: UUID, request: Request) -> IdeaDraftView:
    with request.app.state.session_factory() as session:
        draft = session.get(IdeaDraft, draft_id)
        if draft is None:
            raise QfError("IDEA_DRAFT_NOT_FOUND", "Idea Draft was not found.", 404)
        return _draft_view(session, draft)


@router.post("/idea-drafts/{draft_id}/answers", response_model=IdeaDraftView)
def answer_idea_draft(
    draft_id: UUID,
    payload: AnswerIdeaDraftInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    with request.app.state.session_factory() as session, session.begin():
        def action() -> dict[str, Any]:
            draft = session.get(IdeaDraft, draft_id)
            if draft is None:
                raise QfError("IDEA_DRAFT_NOT_FOUND", "Idea Draft was not found.", 404)
            answered = answer_draft(
                session,
                draft_id,
                _answer_ids(session, draft, payload.answers),
                expected_revision=payload.expected_revision,
            )
            session.flush()
            return _draft_view(session, answered).model_dump(mode="json")

        return _idempotent(
            session,
            _idempotency_key(idempotency_key),
            f"idea-draft.answers:{draft_id}",
            payload,
            action,
            status_code=200,
        )


@router.post("/idea-drafts/{draft_id}/start", response_model=ResearchProgramView, status_code=201)
def start_idea_draft(
    draft_id: UUID,
    payload: StartIdeaDraftInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    with request.app.state.session_factory() as session, session.begin():
        def action() -> dict[str, Any]:
            program = start_draft(
                session,
                draft_id,
                title=payload.title,
                universe_version_ids=payload.universe_version_ids,
                expected_revision=payload.expected_revision,
            )
            session.flush()
            return _program_view(session, program).model_dump(mode="json")

        return _idempotent(
            session,
            _idempotency_key(idempotency_key),
            f"idea-draft.start:{draft_id}",
            payload,
            action,
            status_code=201,
        )


@router.get("/research-programs", response_model=ResearchProgramPage)
def list_research_programs(
    request: Request,
    limit: int = Query(default=50, ge=1, le=100),
    cursor: UUID | None = None,
) -> ResearchProgramPage:
    with request.app.state.session_factory() as session:
        statement = select(ResearchProgram).order_by(ResearchProgram.id).limit(limit + 1)
        if cursor is not None:
            statement = statement.where(ResearchProgram.id > cursor)
        programs = list(session.scalars(statement))
        page, extra = programs[:limit], programs[limit:]
        return ResearchProgramPage(
            items=[_program_view(session, program) for program in page],
            next_cursor=page[-1].id if extra and page else None,
        )


@router.get("/research-programs/{program_id}", response_model=ResearchProgramView)
def get_research_program(program_id: UUID, request: Request) -> ResearchProgramView:
    with request.app.state.session_factory() as session:
        program = session.get(ResearchProgram, program_id)
        if program is None:
            raise QfError("RESEARCH_PROGRAM_NOT_FOUND", "Research Program was not found.", 404)
        return _program_view(session, program)


def _program_action(
    program_id: UUID,
    payload: ProgramActionInput,
    request: Request,
    idempotency_key: str | None,
    action_name: str,
) -> dict[str, Any]:
    with request.app.state.session_factory() as session, session.begin():
        def action() -> dict[str, Any]:
            program = transition_program(
                session,
                program_id,
                action_name,
                reason=payload.reason,
                expected_revision=payload.expected_revision,
            )
            session.flush()
            return _program_view(session, program).model_dump(mode="json")

        return _idempotent(
            session,
            _idempotency_key(idempotency_key),
            f"research-program.{action_name}:{program_id}",
            payload,
            action,
            status_code=200,
        )


@router.post("/research-programs/{program_id}/pause", response_model=ResearchProgramView)
def pause_research_program(
    program_id: UUID,
    payload: ProgramActionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    return _program_action(program_id, payload, request, idempotency_key, "pause")


@router.post("/research-programs/{program_id}/resume", response_model=ResearchProgramView)
def resume_research_program(
    program_id: UUID,
    payload: ProgramActionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    return _program_action(program_id, payload, request, idempotency_key, "resume")


@router.post("/research-programs/{program_id}/archive", response_model=ResearchProgramView)
def archive_research_program(
    program_id: UUID,
    payload: ProgramActionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    return _program_action(program_id, payload, request, idempotency_key, "archive")


@router.post("/research-programs/{program_id}/wake", response_model=ResearchProgramView)
def wake_research_program(
    program_id: UUID,
    payload: ProgramActionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    return _program_action(program_id, payload, request, idempotency_key, "wake")


@router.get("/research-programs/{program_id}/cycles", response_model=ResearchCyclePage)
def list_research_cycles(
    program_id: UUID,
    request: Request,
    limit: int = Query(default=50, ge=1, le=100),
    cursor: UUID | None = None,
) -> ResearchCyclePage:
    with request.app.state.session_factory() as session:
        if session.get(ResearchProgram, program_id) is None:
            raise QfError("RESEARCH_PROGRAM_NOT_FOUND", "Research Program was not found.", 404)
        statement = (
            select(ResearchCycle)
            .where(ResearchCycle.program_id == program_id)
            .order_by(ResearchCycle.id)
            .limit(limit + 1)
        )
        if cursor is not None:
            statement = statement.where(ResearchCycle.id > cursor)
        cycles = list(session.scalars(statement))
        page, extra = cycles[:limit], cycles[limit:]
        return ResearchCyclePage(
            items=[
                ResearchCycleView(
                    id=cycle.id,
                    cycle_no=cycle.cycle_no,
                    trigger=cycle.trigger,
                    trigger_ref_id=cycle.trigger_ref_id,
                    state=cycle.state,
                    mission_budget=cycle.mission_budget,
                    replan_budget=cycle.replan_budget,
                    runtime_configuration_revision=cycle.runtime_configuration_revision,
                    started_at=cycle.started_at,
                    finished_at=cycle.finished_at,
                    summary=cycle.summary,
                    created_at=cycle.created_at,
                )
                for cycle in page
            ],
            next_cursor=page[-1].id if extra and page else None,
        )


@router.get("/research-programs/{program_id}/mission-graph", response_model=MissionGraphView)
def get_mission_graph(program_id: UUID, request: Request) -> MissionGraphView:
    with request.app.state.session_factory() as session:
        program = session.get(ResearchProgram, program_id)
        if program is None:
            raise QfError("RESEARCH_PROGRAM_NOT_FOUND", "Research Program was not found.", 404)
        missions = list(
            session.scalars(select(ResearchMission).where(ResearchMission.program_id == program_id))
        )
        return MissionGraphView(
            program_id=program.id,
            cycle_id=program.current_cycle_id,
            nodes=[
                _mission_view(session, mission)
                for mission in _topological_missions(session, missions)
            ],
        )


@router.get("/missions/{mission_id}", response_model=MissionView)
def get_mission(mission_id: UUID, request: Request) -> MissionView:
    with request.app.state.session_factory() as session:
        mission = session.get(ResearchMission, mission_id)
        if mission is None:
            raise QfError("MISSION_NOT_FOUND", "Mission was not found.", 404)
        return _mission_view(session, mission)


@router.get("/missions/{mission_id}/turns", response_model=list[AgentTurnView])
def list_mission_turns(mission_id: UUID, request: Request) -> list[AgentTurnView]:
    with request.app.state.session_factory() as session:
        agent_session = session.scalar(select(AgentSession).where(AgentSession.mission_id == mission_id))
        if agent_session is None:
            if session.get(ResearchMission, mission_id) is None:
                raise QfError("MISSION_NOT_FOUND", "Mission was not found.", 404)
            return []
        return [
            AgentTurnView(
                id=turn.id,
                ordinal=turn.ordinal,
                kind=turn.kind,
                codex_turn_id=turn.codex_turn_id,
                state=turn.state,
                observable_summary=turn.observable_summary,
                tool_call_count=turn.tool_call_count,
                started_at=turn.started_at,
                finished_at=turn.finished_at,
                error_code=turn.error_code,
            )
            for turn in session.scalars(
                select(AgentTurn)
                .where(AgentTurn.agent_session_id == agent_session.id)
                .order_by(AgentTurn.ordinal)
            )
        ]


@router.get("/missions/{mission_id}/artifacts", response_model=list[MissionArtifactView])
def list_mission_artifacts(mission_id: UUID, request: Request) -> list[MissionArtifactView]:
    with request.app.state.session_factory() as session:
        if session.get(ResearchMission, mission_id) is None:
            raise QfError("MISSION_NOT_FOUND", "Mission was not found.", 404)
        return [
            MissionArtifactView(
                id=artifact.id,
                turn_id=artifact.turn_id,
                kind=artifact.kind,
                schema_version=artifact.schema_version,
                revision=artifact.revision,
                state=artifact.state,
                storage_uri=artifact.storage_uri,
                metadata=artifact.metadata_json,
                created_at=artifact.created_at,
            )
            for artifact in session.scalars(
                select(MissionArtifact)
                .where(MissionArtifact.mission_id == mission_id)
                .order_by(MissionArtifact.kind, MissionArtifact.revision)
            )
        ]
