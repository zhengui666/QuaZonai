"""The single writer for Idea Draft, Program, Cycle, and Mission facts."""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import asdict, dataclass
from datetime import UTC, datetime
from decimal import Decimal, InvalidOperation
from typing import Any
from uuid import UUID

from sqlalchemy import func, select
from sqlalchemy.orm import Session

from agent_harness.contracts import (
    FIXED_AUTONOMY_BUDGET,
    MissionType,
    RoleProfile,
    role_allowed_tools,
)
from degradation_engine import (
    DegradationPolicy,
    ForwardEvidence,
    HealthSnapshot,
    HealthState,
    ProgramState,
    SubjectType,
    WakeDisposition,
    evaluate_degradation,
    schedule_wake,
)
from db.models import (
    AlphaQualification,
    ClarificationAnswer,
    ClarificationQuestion,
    DegradationObservation,
    DatasetRevision,
    Event,
    ForwardEvidenceEpisode,
    HandoffOffer,
    IdeaDraft,
    Job,
    MissionDependency,
    MarketUniverseVersion,
    PortfolioCandidate,
    ResearchBranch,
    ResearchCharter,
    ResearchCycle,
    ResearchMission,
    ResearchProgram,
    ResearchWakeEvent,
    RuntimeConfiguration,
)
from errors import QfError
from jobs import enqueue_job


MAX_PARALLEL_MISSIONS = 3
MAX_CYCLE_MISSIONS = 20
MAX_REPLANS = 3
VISIBLE_MISSION_TURN_COUNT = 5  # PLAN → IMPLEMENT → VALIDATE → EXECUTE → REVIEW
MAX_MISSION_TURNS = VISIBLE_MISSION_TURN_COUNT + FIXED_AUTONOMY_BUDGET.max_repair_turns
DEGRADATION_MISSION_TURNS = 2 + FIXED_AUTONOMY_BUDGET.max_repair_turns
_DEGRADATION_POLICY = DegradationPolicy("degradation-v1")


@dataclass(frozen=True, slots=True)
class MissionSpec:
    key: str
    mission_type: MissionType
    role: RoleProfile
    objective: str
    depends_on: tuple[str, ...] = ()


_INITIAL_MISSION_GRAPH = (
    MissionSpec(
        "plan",
        MissionType.PLAN_RESEARCH,
        RoleProfile.RESEARCH_PLANNER,
        "Create a bounded research plan from the frozen Charter.",
    ),
    MissionSpec(
        "data",
        MissionType.DATA_QUALITY,
        RoleProfile.DATA_STEWARD,
        "Validate point-in-time datasets and disclose only permitted evidence.",
        ("plan",),
    ),
    MissionSpec(
        "alpha",
        MissionType.ALPHA_DISCOVERY,
        RoleProfile.ALPHA_RESEARCHER,
        "Produce a typed Alpha proposal and inspect its honest discovery evidence.",
        ("data",),
    ),
    MissionSpec(
        "robustness",
        MissionType.ROBUSTNESS,
        RoleProfile.ROBUSTNESS_VALIDATOR,
        "Review the proposed Alpha for robustness and invalid evidence.",
        ("alpha",),
    ),
    MissionSpec(
        "portfolio",
        MissionType.PORTFOLIO_ASSEMBLY,
        RoleProfile.PORTFOLIO_ARCHITECT,
        "Assemble eligible Alpha sleeves into a constrained target portfolio proposal.",
        ("robustness",),
    ),
    MissionSpec(
        "review",
        MissionType.SEALED_PROMOTION_REVIEW,
        RoleProfile.INDEPENDENT_REVIEWER,
        "Independently review sealed-promotion evidence without changing formal facts.",
        ("portfolio",),
    ),
)

_DEGRADATION_MISSION_GRAPH = (
    MissionSpec(
        "diagnose",
        MissionType.DEGRADATION_DIAGNOSIS,
        RoleProfile.DEGRADATION_INVESTIGATOR,
        "Diagnose the persisted degradation observation without changing formal facts.",
    ),
    MissionSpec(
        "replan",
        MissionType.REPLAN,
        RoleProfile.RESEARCH_PLANNER,
        "Propose a bounded replan from the persisted degradation observation.",
        ("diagnose",),
    ),
)


def now() -> datetime:
    return datetime.now(UTC)


def _event(
    session: Session,
    kind: str,
    program_id: UUID | None,
    payload: dict[str, Any],
    *,
    actor_kind: str = "SYSTEM",
) -> None:
    session.add(
        Event(
            kind=kind,
            aggregate_type="RESEARCH_PROGRAM" if program_id else "IDEA_DRAFT",
            aggregate_id=program_id,
            actor_kind=actor_kind,
            actor_metadata={},
            payload=payload,
        )
    )


def clarification_questions() -> tuple[str, str, str]:
    """The bounded planner intake questions; each changes a Charter boundary."""
    return (
        "Which market universe and instruments are in scope?",
        "What prediction horizon and rebalance cadence are required?",
        "Which data domains are permitted, and which are explicitly excluded?",
    )


def create_draft(session: Session, idea: str) -> IdeaDraft:
    clean = idea.strip()
    if not clean:
        raise QfError("IDEA_EMPTY", "Idea text must not be blank.", 422)
    draft = IdeaDraft(
        original_idea_text=clean,
        state="CLARIFYING",
        clarification_round=1,
        revision=1,
    )
    session.add(draft)
    session.flush()
    timestamp = now()
    session.add_all(
        ClarificationQuestion(
            idea_draft_id=draft.id,
            round_no=1,
            ordinal=ordinal,
            question_text=question,
            created_at=timestamp,
        )
        for ordinal, question in enumerate(clarification_questions(), start=1)
    )
    _event(
        session,
        "IDEA_DRAFT_CREATED",
        None,
        {"idea_draft_id": str(draft.id), "clarification_count": 3},
        actor_kind="HUMAN",
    )
    return draft


def draft_questions(session: Session, draft_id: UUID) -> list[ClarificationQuestion]:
    return list(
        session.scalars(
            select(ClarificationQuestion)
            .where(ClarificationQuestion.idea_draft_id == draft_id)
            .order_by(ClarificationQuestion.ordinal)
        )
    )


def draft_answers(session: Session, question_ids: Iterable[UUID]) -> dict[UUID, ClarificationAnswer]:
    identifiers = tuple(question_ids)
    if not identifiers:
        return {}
    return {
        answer.question_id: answer
        for answer in session.scalars(
            select(ClarificationAnswer).where(ClarificationAnswer.question_id.in_(identifiers))
        )
    }


def answer_draft(
    session: Session,
    draft_id: UUID,
    answers: dict[UUID, str],
    *,
    expected_revision: int | None = None,
) -> IdeaDraft:
    draft = session.execute(
        select(IdeaDraft).where(IdeaDraft.id == draft_id).with_for_update()
    ).scalar_one_or_none()
    if draft is None:
        raise QfError("IDEA_DRAFT_NOT_FOUND", "Idea Draft was not found.", 404)
    if expected_revision is not None and draft.revision != expected_revision:
        raise QfError(
            "IDEA_DRAFT_REVISION_CONFLICT",
            "Idea Draft has changed.",
            409,
            {"expected_revision": expected_revision, "actual_revision": draft.revision},
        )
    if draft.state in {"STARTED", "DISCARDED"}:
        raise QfError("IDEA_DRAFT_STATE_CONFLICT", "Idea Draft can no longer accept answers.", 409)
    questions = draft_questions(session, draft.id)
    known_questions = {question.id for question in questions}
    if set(answers) - known_questions:
        raise QfError(
            "CLARIFICATION_QUESTION_INVALID",
            "An answer does not belong to this Idea Draft.",
            422,
        )
    existing = draft_answers(session, known_questions)
    timestamp = now()
    for question_id, raw_answer in answers.items():
        answer = raw_answer.strip()
        if not answer:
            raise QfError("CLARIFICATION_ANSWER_EMPTY", "Clarification answers must not be blank.", 422)
        prior = existing.get(question_id)
        if prior is not None and prior.answer_text != answer:
            raise QfError(
                "CLARIFICATION_ANSWER_IMMUTABLE",
                "A submitted clarification answer cannot be changed.",
                409,
            )
        if prior is None:
            session.add(
                ClarificationAnswer(
                    question_id=question_id,
                    answer_text=answer,
                    created_at=timestamp,
                )
            )
    session.flush()
    completed = len(draft_answers(session, known_questions)) == len(questions)
    draft.state = "READY" if completed else "CLARIFYING"
    draft.revision += 1
    _event(
        session,
        "IDEA_DRAFT_ANSWERED",
        None,
        {
            "idea_draft_id": str(draft.id),
            "answered_count": len(draft_answers(session, known_questions)),
            "clarification_complete": completed,
        },
        actor_kind="HUMAN",
    )
    return draft


def _runtime_configuration_revision(session: Session) -> int:
    value = session.scalar(
        select(RuntimeConfiguration.revision)
        .where(RuntimeConfiguration.scope == "SYSTEM")
        .order_by(RuntimeConfiguration.updated_at.desc())
        .limit(1)
    )
    return int(value or 1)


def _charter_universe_versions(
    session: Session, universe_version_ids: list[UUID] | None
) -> list[str]:
    if universe_version_ids is None:
        active_ids = list(
            session.scalars(
                select(MarketUniverseVersion.id)
                .where(MarketUniverseVersion.state == "ACTIVE")
                .order_by(MarketUniverseVersion.id)
            )
        )
        if len(active_ids) != 1:
            raise QfError(
                "UNIVERSE_SELECTION_REQUIRED",
                "Create exactly one active Universe Version or select the Charter scope explicitly.",
                409,
                {"active_universe_count": len(active_ids)},
            )
        return [str(active_ids[0])]
    selected = list(dict.fromkeys(universe_version_ids))
    if not selected:
        raise QfError(
            "UNIVERSE_SELECTION_REQUIRED",
            "A Research Charter must bind at least one active Universe Version.",
            422,
        )
    selected_active_ids = set(
        session.scalars(
            select(MarketUniverseVersion.id).where(
                MarketUniverseVersion.id.in_(selected),
                MarketUniverseVersion.state == "ACTIVE",
            )
        )
    )
    if selected_active_ids != set(selected):
        raise QfError(
            "UNIVERSE_NOT_ACTIVE",
            "Every selected Universe Version must be active.",
            409,
        )
    return [str(item) for item in selected]


def _charter_transcript(
    questions: list[ClarificationQuestion], answers: dict[UUID, ClarificationAnswer]
) -> list[dict[str, str]]:
    return [
        {
            "question_id": str(question.id),
            "question": question.question_text,
            "answer": answers[question.id].answer_text,
        }
        for question in questions
    ]


def _create_initial_missions(
    session: Session,
    *,
    program: ResearchProgram,
    cycle: ResearchCycle,
    branch: ResearchBranch,
    charter: ResearchCharter,
) -> dict[str, ResearchMission]:
    missions: dict[str, ResearchMission] = {}
    charter_snapshot = {
        "charter_id": str(charter.id),
        "research_question": charter.research_question,
        "market_scope": charter.market_scope,
        "universe_version_ids": charter.universe_version_ids,
        "prediction_horizon": charter.prediction_horizon,
        "allowed_data_domains": charter.allowed_data_domains,
        "explicit_exclusions": charter.explicit_exclusions,
    }
    branch_snapshot = {
        "branch_id": str(branch.id),
        "hypothesis": branch.hypothesis,
        "preserved_constraints": branch.preserved_constraints,
    }
    universe_ids = tuple(
        UUID(str(value))
        for value in charter.universe_version_ids
        if value is not None
    )
    discovery_dataset_ids = tuple(
        str(dataset_id)
        for dataset_id in session.scalars(
            select(DatasetRevision.id)
            .where(
                DatasetRevision.partition == "DISCOVERY",
                DatasetRevision.universe_version_id.in_(universe_ids),
            )
            .order_by(DatasetRevision.id)
        )
    ) if universe_ids else ()
    for spec in _INITIAL_MISSION_GRAPH:
        mission = ResearchMission(
            program_id=program.id,
            cycle_id=cycle.id,
            branch_id=branch.id,
            type=spec.mission_type.value,
            role=spec.role.value,
            state="PLANNED",
            objective=spec.objective,
            contract_version="v1",
            input_snapshot={"charter": charter_snapshot, "branch": branch_snapshot},
            capability_snapshot={
                "allowed_tools": sorted(tool.value for tool in role_allowed_tools(spec.role)),
                "allowed_dataset_revision_ids": discovery_dataset_ids,
            },
            runtime_snapshot={"runtime_configuration_revision": cycle.runtime_configuration_revision},
            prompt_version="v1",
            max_turns=MAX_MISSION_TURNS,
            max_tool_calls=20,
            attempt=1,
            revision=1,
        )
        session.add(mission)
        missions[spec.key] = mission
    session.flush()
    for spec in _INITIAL_MISSION_GRAPH:
        for dependency_key in spec.depends_on:
            session.add(
                MissionDependency(
                    mission_id=missions[spec.key].id,
                    depends_on_mission_id=missions[dependency_key].id,
                    required_outcome="SUCCEEDED",
                )
            )
    return missions


def _fraction(value: Decimal | float | int, field_name: str) -> Decimal:
    if isinstance(value, bool):
        raise QfError("DEGRADATION_OBSERVATION_INVALID", f"{field_name} must be numeric.", 422)
    try:
        result = Decimal(str(value))
    except (InvalidOperation, ValueError) as exc:
        raise QfError(
            "DEGRADATION_OBSERVATION_INVALID", f"{field_name} must be finite.", 422
        ) from exc
    if not result.is_finite() or not Decimal("0") <= result <= Decimal("1"):
        raise QfError(
            "DEGRADATION_OBSERVATION_INVALID", f"{field_name} must be in [0, 1].", 422
        )
    return result


def _completed_forward_evidence(
    session: Session, handoff_id: UUID
) -> tuple[HandoffOffer, ForwardEvidenceEpisode]:
    handoff = session.execute(
        select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()
    ).scalar_one_or_none()
    if handoff is None:
        raise QfError("HANDOFF_NOT_FOUND", "Handoff was not found.", 404)
    if handoff.state != "FEEDBACK_COMPLETE":
        raise QfError(
            "FORWARD_EVIDENCE_REQUIRED",
            "Completed Forward Evidence is required before recording degradation.",
            409,
        )
    episodes = list(
        session.scalars(
            select(ForwardEvidenceEpisode)
            .where(
                ForwardEvidenceEpisode.handoff_id == handoff.id,
                ForwardEvidenceEpisode.state == "FEEDBACK_COMPLETE",
            )
        )
    )
    if len(episodes) != 1:
        raise QfError(
            "FORWARD_EVIDENCE_AMBIGUOUS",
            "Handoff must have exactly one completed Forward Evidence episode.",
            409,
        )
    return handoff, episodes[0]


def _candidate_qualification_ids(candidate: PortfolioCandidate) -> tuple[UUID, ...]:
    if not isinstance(candidate.members, list):
        raise QfError(
            "DEGRADATION_SUBJECT_INVALID",
            "Candidate members cannot establish degradation scope.",
            409,
        )
    identifiers: list[UUID] = []
    for member in candidate.members:
        raw_identifier = member.get("alpha_qualification_id") if isinstance(member, dict) else None
        try:
            identifiers.append(UUID(str(raw_identifier)))
        except (TypeError, ValueError, AttributeError) as exc:
            raise QfError(
                "DEGRADATION_SUBJECT_INVALID",
                "Candidate members cannot establish degradation scope.",
                409,
            ) from exc
    if not identifiers or len(identifiers) != len(set(identifiers)):
        raise QfError(
            "DEGRADATION_SUBJECT_INVALID",
            "Candidate members cannot establish degradation scope.",
            409,
        )
    return tuple(identifiers)


def _locked_subject_program(
    session: Session,
    handoff: HandoffOffer,
    subject_type: SubjectType,
    subject_id: UUID,
) -> tuple[ResearchProgram, AlphaQualification | None]:
    candidate = session.get(PortfolioCandidate, handoff.candidate_id)
    if candidate is None:
        raise QfError("CANDIDATE_NOT_FOUND", "Handoff Candidate is missing.", 500)
    qualification_ids = _candidate_qualification_ids(candidate)
    alpha: AlphaQualification | None = None
    if subject_type is SubjectType.ALPHA:
        if subject_id not in qualification_ids:
            raise QfError(
                "DEGRADATION_SUBJECT_MISMATCH",
                "Alpha is not a member of this Handoff Candidate.",
                409,
            )
        alpha = session.get(AlphaQualification, subject_id)
        if alpha is None or alpha.program_id is None:
            raise QfError(
                "DEGRADATION_SUBJECT_UNSCOPED",
                "Alpha Qualification has no Research Program scope.",
                409,
            )
        program_id = alpha.program_id
    else:
        if subject_id != candidate.id:
            raise QfError(
                "DEGRADATION_SUBJECT_MISMATCH",
                "Portfolio must match the Handoff Candidate.",
                409,
            )
        qualifications = list(
            session.scalars(
                select(AlphaQualification).where(AlphaQualification.id.in_(qualification_ids))
            )
        )
        program_ids = {item.program_id for item in qualifications}
        if len(qualifications) != len(qualification_ids) or len(program_ids) != 1 or None in program_ids:
            raise QfError(
                "DEGRADATION_SUBJECT_AMBIGUOUS",
                "Portfolio Candidate does not map to one Research Program.",
                409,
            )
        candidate_program_id = next(iter(program_ids))
        if candidate_program_id is None:
            raise QfError(
                "DEGRADATION_SUBJECT_AMBIGUOUS",
                "Portfolio Candidate does not map to one Research Program.",
                409,
            )
        program_id = candidate_program_id
    program = session.execute(
        select(ResearchProgram).where(ResearchProgram.id == program_id).with_for_update()
    ).scalar_one_or_none()
    if program is None:
        raise QfError("RESEARCH_PROGRAM_NOT_FOUND", "Research Program was not found.", 404)
    return program, alpha


def _previous_degradation_snapshot(
    session: Session,
    *,
    program_id: UUID,
    subject_type: SubjectType,
    subject_id: UUID,
) -> HealthSnapshot:
    prior = session.scalar(
        select(DegradationObservation)
        .join(
            ForwardEvidenceEpisode,
            ForwardEvidenceEpisode.id == DegradationObservation.forward_evidence_episode_id,
        )
        .where(
            DegradationObservation.program_id == program_id,
            DegradationObservation.subject_type == subject_type.value,
            DegradationObservation.subject_id == subject_id,
            DegradationObservation.policy_revision == _DEGRADATION_POLICY.policy_revision,
            DegradationObservation.evaluated.is_(True),
        )
        .order_by(
            ForwardEvidenceEpisode.observation_end.desc(),
            DegradationObservation.id.desc(),
        )
        .limit(1)
    )
    if prior is None:
        return HealthSnapshot()
    try:
        return HealthSnapshot(HealthState(prior.state), prior.consecutive_breaches)
    except ValueError as exc:
        raise QfError(
            "DEGRADATION_STATE_INVALID",
            "Stored degradation state is invalid.",
            500,
        ) from exc


def _forward_evidence_is_in_order(
    session: Session,
    *,
    episode: ForwardEvidenceEpisode,
    program_id: UUID,
    subject_type: SubjectType,
    subject_id: UUID,
) -> bool:
    later = session.scalar(
        select(DegradationObservation.id)
        .join(
            ForwardEvidenceEpisode,
            ForwardEvidenceEpisode.id == DegradationObservation.forward_evidence_episode_id,
        )
        .where(
            DegradationObservation.program_id == program_id,
            DegradationObservation.subject_type == subject_type.value,
            DegradationObservation.subject_id == subject_id,
            DegradationObservation.policy_revision == _DEGRADATION_POLICY.policy_revision,
            DegradationObservation.evaluated.is_(True),
            ForwardEvidenceEpisode.observation_end >= episode.observation_end,
        )
        .limit(1)
    )
    return later is None


def _degradation_cycle(
    session: Session,
    *,
    program: ResearchProgram,
    observation: DegradationObservation,
    wake: ResearchWakeEvent,
) -> ResearchCycle:
    charter = session.get(ResearchCharter, program.charter_id)
    parent_branch = session.scalar(
        select(ResearchBranch)
        .where(ResearchBranch.program_id == program.id)
        .order_by(ResearchBranch.revision_no.desc(), ResearchBranch.created_at.desc())
        .limit(1)
    )
    if charter is None or parent_branch is None:
        raise QfError(
            "DEGRADATION_REPLAN_CONTEXT_MISSING",
            "Degradation replan requires the Program Charter and Branch.",
            409,
        )
    cycle_no = int(
        session.scalar(
            select(func.coalesce(func.max(ResearchCycle.cycle_no), 0)).where(
                ResearchCycle.program_id == program.id
            )
        )
        or 0
    ) + 1
    branch_revision = int(
        session.scalar(
            select(func.coalesce(func.max(ResearchBranch.revision_no), 0)).where(
                ResearchBranch.program_id == program.id
            )
        )
        or 0
    ) + 1
    timestamp = now()
    cycle = ResearchCycle(
        program_id=program.id,
        cycle_no=cycle_no,
        trigger="DEGRADATION_WAKE",
        trigger_ref_id=wake.id,
        state="RUNNING",
        mission_budget=len(_DEGRADATION_MISSION_GRAPH),
        replan_budget=1,
        runtime_configuration_revision=_runtime_configuration_revision(session),
        started_at=timestamp,
        summary={
            "wake_event_id": str(wake.id),
            "degradation_observation_id": str(observation.id),
            "subject_type": observation.subject_type,
            "subject_id": str(observation.subject_id),
            "reason_code": observation.reason_code,
        },
        created_at=timestamp,
    )
    session.add(cycle)
    session.flush()
    branch = ResearchBranch(
        program_id=program.id,
        cycle_id=cycle.id,
        parent_branch_id=parent_branch.id,
        derivation_type="DEGRADATION_REPLAN",
        hypothesis="Diagnose the persisted degradation before proposing a bounded replan.",
        changed_assumptions=["Forward Evidence reported a degradation condition."],
        preserved_constraints=charter.explicit_exclusions,
        state="ACTIVE",
        revision_no=branch_revision,
        created_at=timestamp,
    )
    session.add(branch)
    session.flush()
    charter_snapshot = {
        "charter_id": str(charter.id),
        "research_question": charter.research_question,
        "market_scope": charter.market_scope,
        "universe_version_ids": charter.universe_version_ids,
        "prediction_horizon": charter.prediction_horizon,
        "allowed_data_domains": charter.allowed_data_domains,
        "explicit_exclusions": charter.explicit_exclusions,
    }
    branch_snapshot = {
        "branch_id": str(branch.id),
        "hypothesis": branch.hypothesis,
        "preserved_constraints": branch.preserved_constraints,
    }
    degradation_snapshot = {
        "degradation_observation_id": str(observation.id),
        "forward_evidence_episode_id": str(observation.forward_evidence_episode_id),
        "subject_type": observation.subject_type,
        "subject_id": str(observation.subject_id),
        "metric_name": observation.metric_name,
        "severity": str(observation.severity),
        "confidence": str(observation.confidence),
        "policy_revision": observation.policy_revision,
        "policy_snapshot": observation.policy_snapshot,
        "reason_code": observation.reason_code,
        "state": observation.state,
        "consecutive_breaches": observation.consecutive_breaches,
        "evaluated": observation.evaluated,
    }
    universe_ids = tuple(
        UUID(str(value))
        for value in charter_snapshot.get("universe_version_ids", ())
        if value is not None
    )
    discovery_dataset_ids = tuple(
        str(dataset_id)
        for dataset_id in session.scalars(
            select(DatasetRevision.id)
            .where(
                DatasetRevision.partition == "DISCOVERY",
                DatasetRevision.universe_version_id.in_(universe_ids),
            )
            .order_by(DatasetRevision.id)
        )
    ) if universe_ids else ()
    missions: dict[str, ResearchMission] = {}
    for spec in _DEGRADATION_MISSION_GRAPH:
        mission = ResearchMission(
            program_id=program.id,
            cycle_id=cycle.id,
            branch_id=branch.id,
            type=spec.mission_type.value,
            role=spec.role.value,
            state="PLANNED",
            objective=spec.objective,
            contract_version="v1",
            input_snapshot={
                "charter": charter_snapshot,
                "branch": branch_snapshot,
                "degradation": degradation_snapshot,
            },
            capability_snapshot={
                "allowed_tools": sorted(tool.value for tool in role_allowed_tools(spec.role)),
                "allowed_dataset_revision_ids": discovery_dataset_ids,
            },
            runtime_snapshot={"runtime_configuration_revision": cycle.runtime_configuration_revision},
            prompt_version="v1",
            max_turns=DEGRADATION_MISSION_TURNS,
            max_tool_calls=20,
            attempt=1,
            revision=1,
        )
        session.add(mission)
        missions[spec.key] = mission
    session.flush()
    for spec in _DEGRADATION_MISSION_GRAPH:
        for dependency_key in spec.depends_on:
            session.add(
                MissionDependency(
                    mission_id=missions[spec.key].id,
                    depends_on_mission_id=missions[dependency_key].id,
                    required_outcome="SUCCEEDED",
                )
            )
    wake.state = "CONSUMED"
    wake.cycle_id = cycle.id
    wake.consumed_at = timestamp
    program.current_cycle_id = cycle.id
    program.wake_reason = observation.reason_code
    _event(
        session,
        "RESEARCH_WAKE_CONSUMED",
        program.id,
        {"wake_event_id": str(wake.id), "cycle_id": str(cycle.id)},
    )
    _event(
        session,
        "DEGRADATION_REPLAN_CYCLE_CREATED",
        program.id,
        {
            "wake_event_id": str(wake.id),
            "degradation_observation_id": str(observation.id),
            "cycle_id": str(cycle.id),
        },
    )
    return cycle


def _consume_pending_wakes(session: Session, program: ResearchProgram) -> list[ResearchCycle]:
    """Call only while holding the Program row lock after it became ACTIVE."""
    if program.state != "ACTIVE":
        return []
    pending = list(
        session.scalars(
            select(ResearchWakeEvent)
            .where(
                ResearchWakeEvent.program_id == program.id,
                ResearchWakeEvent.state == "PENDING",
            )
            .order_by(ResearchWakeEvent.created_at, ResearchWakeEvent.id)
            .with_for_update()
        )
    )
    cycles: list[ResearchCycle] = []
    for wake in pending:
        observation = session.get(DegradationObservation, wake.degradation_observation_id)
        if observation is None:
            raise QfError(
                "DEGRADATION_OBSERVATION_NOT_FOUND",
                "Pending Wake has no degradation observation.",
                500,
            )
        cycles.append(
            _degradation_cycle(
                session,
                program=program,
                observation=observation,
                wake=wake,
            )
        )
    return cycles


def record_degradation_observation(
    session: Session,
    *,
    handoff_id: UUID,
    subject_type: SubjectType | str,
    subject_id: UUID,
    metric_name: str,
    severity: Decimal | float | int,
    confidence: Decimal | float | int,
) -> tuple[DegradationObservation, ResearchWakeEvent | None, ResearchCycle | None]:
    """Persist one trusted Observation and only create bounded research work."""
    try:
        normalized_subject_type = SubjectType(subject_type)
    except ValueError as exc:
        raise QfError(
            "DEGRADATION_SUBJECT_INVALID", "Degradation subject type is invalid.", 422
        ) from exc
    normalized_metric = metric_name.strip()
    if not normalized_metric:
        raise QfError("DEGRADATION_OBSERVATION_INVALID", "Metric name must not be blank.", 422)
    normalized_severity = _fraction(severity, "severity")
    normalized_confidence = _fraction(confidence, "confidence")
    handoff, episode = _completed_forward_evidence(session, handoff_id)
    program, alpha = _locked_subject_program(
        session,
        handoff,
        normalized_subject_type,
        subject_id,
    )
    existing = session.scalar(
        select(DegradationObservation).where(
            DegradationObservation.forward_evidence_episode_id == episode.id,
            DegradationObservation.subject_type == normalized_subject_type.value,
            DegradationObservation.subject_id == subject_id,
            DegradationObservation.metric_name == normalized_metric,
            DegradationObservation.policy_revision == _DEGRADATION_POLICY.policy_revision,
        )
    )
    if existing is not None:
        if existing.severity != normalized_severity or existing.confidence != normalized_confidence:
            raise QfError(
                "DEGRADATION_OBSERVATION_IMMUTABLE",
                "Forward Evidence already has an immutable degradation observation.",
                409,
            )
        wake = session.scalar(
            select(ResearchWakeEvent).where(
                ResearchWakeEvent.degradation_observation_id == existing.id
            )
        )
        cycle = session.get(ResearchCycle, wake.cycle_id) if wake and wake.cycle_id else None
        return existing, wake, cycle

    if not _forward_evidence_is_in_order(
        session,
        episode=episode,
        program_id=program.id,
        subject_type=normalized_subject_type,
        subject_id=subject_id,
    ):
        raise QfError(
            "FORWARD_EVIDENCE_OUT_OF_ORDER",
            "Forward Evidence observation_end is not strictly newer than evaluated evidence.",
            409,
        )

    previous = _previous_degradation_snapshot(
        session,
        program_id=program.id,
        subject_type=normalized_subject_type,
        subject_id=subject_id,
    )
    result = evaluate_degradation(
        ForwardEvidence(
            source_id=str(episode.id),
            program_id=str(program.id),
            subject_type=normalized_subject_type,
            subject_id=str(subject_id),
            metric_name=normalized_metric,
            severity=float(normalized_severity),
            confidence=float(normalized_confidence),
        ),
        _DEGRADATION_POLICY,
        previous,
    )
    timestamp = now()
    observation = DegradationObservation(
        program_id=program.id,
        forward_evidence_episode_id=episode.id,
        subject_type=normalized_subject_type.value,
        subject_id=subject_id,
        metric_name=normalized_metric,
        severity=normalized_severity,
        confidence=normalized_confidence,
        policy_revision=_DEGRADATION_POLICY.policy_revision,
        policy_snapshot=asdict(_DEGRADATION_POLICY),
        reason_code=result.observation.reason_code,
        state=result.state.value,
        consecutive_breaches=result.snapshot.consecutive_breaches,
        evaluated=result.observation.evaluated,
        created_at=timestamp,
    )
    session.add(observation)
    session.flush()
    if alpha is not None:
        alpha.degradation_state = result.state.value
    _event(
        session,
        "DEGRADATION_OBSERVATION_RECORDED",
        program.id,
        {
            "degradation_observation_id": str(observation.id),
            "forward_evidence_episode_id": str(episode.id),
            "state": observation.state,
            "evaluated": observation.evaluated,
        },
        actor_kind="HUMAN",
    )
    if result.wake_request is None:
        return observation, None, None

    scheduled = schedule_wake(result.wake_request, ProgramState(program.state))
    wake = ResearchWakeEvent(
        program_id=program.id,
        degradation_observation_id=observation.id,
        forward_evidence_episode_id=episode.id,
        subject_type=normalized_subject_type.value,
        subject_id=subject_id,
        policy_revision=result.wake_request.policy_revision,
        reason_code=result.wake_request.reason_code,
        state="PENDING",
        created_at=timestamp,
    )
    session.add(wake)
    session.flush()
    if scheduled.disposition is WakeDisposition.PENDING:
        _event(
            session,
            "RESEARCH_WAKE_PENDING",
            program.id,
            {"wake_event_id": str(wake.id), "reason_code": wake.reason_code},
        )
        return observation, wake, None

    if program.state != "ACTIVE":
        transition_program(
            session,
            program.id,
            "wake",
            reason=result.wake_request.reason_code,
        )
        cycle = session.get(ResearchCycle, wake.cycle_id)
        if cycle is None:
            raise QfError(
                "DEGRADATION_WAKE_NOT_CONSUMED",
                "Allowed degradation Wake did not create its Research Cycle.",
                500,
            )
        return observation, wake, cycle

    cycle = _degradation_cycle(
        session,
        program=program,
        observation=observation,
        wake=wake,
    )
    program.revision += 1
    queue_eligible_missions(session, program)
    return observation, wake, cycle


def _dependencies_satisfied(session: Session, mission: ResearchMission) -> bool:
    dependencies = session.scalars(
        select(MissionDependency).where(MissionDependency.mission_id == mission.id)
    )
    for dependency in dependencies:
        required = dependency.required_outcome or "SUCCEEDED"
        parent = session.get(ResearchMission, dependency.depends_on_mission_id)
        if parent is None or parent.state != required:
            return False
    return True


def queue_eligible_missions(session: Session, program: ResearchProgram) -> list[ResearchMission]:
    """Queue at most the fixed parallel budget; callers hold the Program lock."""
    if program.state != "ACTIVE":
        return []
    running = int(
        session.scalar(
            select(func.count())
            .select_from(ResearchMission)
            .where(
                ResearchMission.program_id == program.id,
                ResearchMission.state.in_(("READY", "RUNNING")),
            )
        )
        or 0
    )
    slots = max(0, MAX_PARALLEL_MISSIONS - running)
    if not slots:
        return []
    candidates = list(
        session.scalars(
            select(ResearchMission).where(
                ResearchMission.program_id == program.id,
                ResearchMission.state == "PLANNED",
            )
        )
    )
    ready: list[ResearchMission] = []
    for mission in candidates:
        if len(ready) == slots:
            break
        if not _dependencies_satisfied(session, mission):
            continue
        mission.state = "READY"
        mission.revision += 1
        job = enqueue_job(
            session,
            kind="RESEARCH_MISSION",
            resource_type="research_mission",
            resource_id=mission.id,
            payload={
                "program_id": str(program.id),
                "cycle_id": str(mission.cycle_id) if mission.cycle_id else None,
                "mission_revision": mission.revision,
            },
        )
        _event(
            session,
            "MISSION_READY",
            program.id,
            {"mission_id": str(mission.id), "job_id": str(job.id)},
        )
        ready.append(mission)
    return ready


def start_draft(
    session: Session,
    draft_id: UUID,
    *,
    title: str | None = None,
    universe_version_ids: list[UUID] | None = None,
    expected_revision: int | None = None,
) -> ResearchProgram:
    draft = session.execute(
        select(IdeaDraft).where(IdeaDraft.id == draft_id).with_for_update()
    ).scalar_one_or_none()
    if draft is None:
        raise QfError("IDEA_DRAFT_NOT_FOUND", "Idea Draft was not found.", 404)
    existing = session.scalar(select(ResearchCharter).where(ResearchCharter.idea_draft_id == draft.id))
    if existing is not None:
        program = session.scalar(select(ResearchProgram).where(ResearchProgram.charter_id == existing.id))
        if program is None:
            raise QfError("RESEARCH_PROGRAM_NOT_FOUND", "Frozen Charter has no Program.", 500)
        return program
    if expected_revision is not None and draft.revision != expected_revision:
        raise QfError(
            "IDEA_DRAFT_REVISION_CONFLICT",
            "Idea Draft has changed.",
            409,
            {"expected_revision": expected_revision, "actual_revision": draft.revision},
        )
    questions = draft_questions(session, draft.id)
    answers = draft_answers(session, (question.id for question in questions))
    if draft.state != "READY" or len(answers) != len(questions):
        raise QfError(
            "CLARIFICATION_REQUIRED",
            "Every clarification question must be answered before the Charter is frozen.",
            409,
        )
    transcript = _charter_transcript(questions, answers)
    answer_by_ordinal = {question.ordinal: answers[question.id].answer_text for question in questions}
    charter_universe_version_ids = _charter_universe_versions(session, universe_version_ids)
    timestamp = now()
    charter = ResearchCharter(
        idea_draft_id=draft.id,
        original_idea_text=draft.original_idea_text,
        research_question=draft.original_idea_text,
        market_scope=[answer_by_ordinal[1]],
        universe_version_ids=charter_universe_version_ids,
        prediction_horizon=answer_by_ordinal[2],
        allowed_data_domains=[answer_by_ordinal[3]],
        explicit_exclusions=[],
        material_assumptions=[],
        system_assumptions=[],
        clarification_transcript=transcript,
        created_at=timestamp,
    )
    session.add(charter)
    session.flush()
    program = ResearchProgram(
        charter_id=charter.id,
        title=(title or draft.original_idea_text.splitlines()[0]).strip()[:240],
        state="ACTIVE",
        revision=1,
    )
    session.add(program)
    session.flush()
    cycle = ResearchCycle(
        program_id=program.id,
        cycle_no=1,
        trigger="IDEA_START",
        state="RUNNING",
        mission_budget=MAX_CYCLE_MISSIONS,
        replan_budget=MAX_REPLANS,
        runtime_configuration_revision=_runtime_configuration_revision(session),
        started_at=timestamp,
        summary={},
        created_at=timestamp,
    )
    session.add(cycle)
    session.flush()
    program.current_cycle_id = cycle.id
    branch = ResearchBranch(
        program_id=program.id,
        cycle_id=cycle.id,
        derivation_type="ROOT",
        hypothesis=charter.research_question,
        changed_assumptions=[],
        preserved_constraints=charter.explicit_exclusions,
        state="ACTIVE",
        revision_no=1,
        created_at=timestamp,
    )
    session.add(branch)
    session.flush()
    _create_initial_missions(
        session,
        program=program,
        cycle=cycle,
        branch=branch,
        charter=charter,
    )
    draft.state = "STARTED"
    draft.revision += 1
    queued = queue_eligible_missions(session, program)
    _event(
        session,
        "PROGRAM_CREATED",
        program.id,
        {
            "idea_draft_id": str(draft.id),
            "charter_id": str(charter.id),
            "cycle_id": str(cycle.id),
            "initial_mission_ids": [str(mission.id) for mission in queued],
        },
        actor_kind="HUMAN",
    )
    return program


def _cancel_queued_program_jobs(session: Session, program_id: UUID) -> None:
    mission_ids = list(
        session.scalars(select(ResearchMission.id).where(ResearchMission.program_id == program_id))
    )
    if not mission_ids:
        return
    for job in session.scalars(
        select(Job).where(
            Job.resource_type == "research_mission",
            Job.resource_id.in_(mission_ids),
            Job.state == "READY",
        )
    ):
        job.state = "CANCELLED"
        job.last_error = "PROGRAM_PAUSED"


def transition_program(
    session: Session,
    program_id: UUID,
    action: str,
    *,
    reason: str | None = None,
    expected_revision: int | None = None,
) -> ResearchProgram:
    program = session.execute(
        select(ResearchProgram).where(ResearchProgram.id == program_id).with_for_update()
    ).scalar_one_or_none()
    if program is None:
        raise QfError("RESEARCH_PROGRAM_NOT_FOUND", "Research Program was not found.", 404)
    if expected_revision is not None and expected_revision != program.revision:
        raise QfError(
            "PROGRAM_REVISION_CONFLICT",
            "Research Program has changed.",
            409,
            {"expected_revision": expected_revision, "actual_revision": program.revision},
        )
    transitions = {
        "pause": (None, "PAUSED"),
        "resume": ("PAUSED", "ACTIVE"),
        "archive": (None, "ARCHIVED"),
        "wake": (None, "ACTIVE"),
    }
    required, target = transitions[action]
    if required is not None and program.state != required:
        raise QfError("PROGRAM_STATE_CONFLICT", f"Program must be {required} for {action}.", 409)
    if action in {"pause", "archive"} and program.state == "ARCHIVED":
        raise QfError("PROGRAM_STATE_CONFLICT", "Archived Program cannot be changed.", 409)
    if action == "wake" and program.state in {"PAUSED", "ARCHIVED"}:
        raise QfError("PROGRAM_STATE_CONFLICT", "Paused or archived Program cannot be auto-woken.", 409)
    program.state = target
    program.revision += 1
    if action == "pause":
        program.pause_reason = reason
        _cancel_queued_program_jobs(session, program.id)
        for mission in session.scalars(
            select(ResearchMission).where(
                ResearchMission.program_id == program.id,
                ResearchMission.state == "READY",
            )
        ):
            mission.state = "PLANNED"
            mission.revision += 1
    elif action in {"resume", "wake"}:
        program.wake_reason = reason
        _consume_pending_wakes(session, program)
        queue_eligible_missions(session, program)
    elif action == "archive":
        _cancel_queued_program_jobs(session, program.id)
    _event(
        session,
        f"PROGRAM_{program.state}",
        program.id,
        {"reason": reason, "revision": program.revision},
        actor_kind="HUMAN" if action != "wake" else "SYSTEM",
    )
    return program
