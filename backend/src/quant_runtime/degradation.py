"""Turn explicit forward-evidence degradation into bounded research work.

Forward evidence is downstream observation only. It may wake research by
creating a new Mission, but it never controls a paper/live node, broker, order,
or account state.
"""

from __future__ import annotations

from datetime import UTC, datetime
from typing import Any
from uuid import UUID

from sqlalchemy import exists, func, or_, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from db.models import (
    AlphaQualification,
    DegradationFollowup,
    ForwardEvidenceEpisode,
    HandoffOffer,
    PortfolioCandidate,
    ResearchBranch,
    ResearchMission,
    ResearchProgram,
    SearchLedgerEntry,
)
from events import append_event
from jobs import enqueue_job

_DEGRADATION_STATES = {"DEGRADED", "FAILED"}


def _now() -> datetime:
    return datetime.now(UTC)


def _is_explicit_degradation(evidence: dict[str, Any]) -> bool:
    if evidence.get("degraded") is True:
        return True
    return str(evidence.get("degradation_state", "")).strip().upper() in _DEGRADATION_STATES


def _member_alpha_ids(candidate: PortfolioCandidate) -> list[UUID]:
    result: list[UUID] = []
    for member in candidate.members or []:
        raw = member.get("alpha_qualification_id") if isinstance(member, dict) else None
        if not raw:
            continue
        try:
            alpha_id = UUID(str(raw))
        except ValueError:
            continue
        if alpha_id not in result:
            result.append(alpha_id)
    return result



def schedule_degradation_missions(session: Session) -> int:
    """Create at most one research Mission per Alpha and degraded feedback episode."""
    created = 0
    handled_episode = exists(
        select(DegradationFollowup.id).where(
            DegradationFollowup.forward_evidence_episode_id == ForwardEvidenceEpisode.id
        )
    )
    episodes = list(
        session.scalars(
            select(ForwardEvidenceEpisode)
            .where(
                ForwardEvidenceEpisode.state == "FEEDBACK_COMPLETE",
                or_(
                    ForwardEvidenceEpisode.evidence["degraded"].as_boolean().is_(True),
                    func.upper(
                        func.trim(
                            ForwardEvidenceEpisode.evidence["degradation_state"].as_string()
                        )
                    ).in_(sorted(_DEGRADATION_STATES)),
                ),
                ~handled_episode,
            )
            .order_by(ForwardEvidenceEpisode.created_at.asc())
        )
    )
    for episode in episodes:
        evidence = episode.evidence or {}
        if not _is_explicit_degradation(evidence):
            continue
        handoff = session.get(HandoffOffer, episode.handoff_id)
        if handoff is None:
            continue
        candidate = session.get(PortfolioCandidate, handoff.candidate_id)
        if candidate is None:
            continue
        for alpha_id in _member_alpha_ids(candidate):
            alpha = session.get(AlphaQualification, alpha_id)
            if alpha is None or alpha.program_id is None or alpha.source_experiment_id is None:
                continue
            program = session.get(ResearchProgram, alpha.program_id)
            if program is None or program.state in {"PAUSED", "ARCHIVED"}:
                continue
            source = session.get(SearchLedgerEntry, alpha.source_experiment_id)
            if (
                source is None
                or source.program_id != program.id
                or source.branch_id is None
            ):
                continue
            parent = session.get(ResearchBranch, source.branch_id)
            if parent is None or parent.program_id != program.id:
                continue
            followup = DegradationFollowup(
                alpha_qualification_id=alpha.id,
                forward_evidence_episode_id=episode.id,
                source_experiment_id=source.id,
                created_at=_now(),
            )
            try:
                with session.begin_nested():
                    session.add(followup)
                    session.flush()
            except IntegrityError:
                continue
            branch = ResearchBranch(
                program_id=program.id,
                parent_branch_id=parent.id,
                derivation_type="FORWARD_DEGRADATION",
                hypothesis=(
                    f"Investigate forward-evidence degradation for Alpha {alpha.id} without changing "
                    "the frozen Research Charter."
                ),
                changed_assumptions=[
                    f"Forward evidence episode {episode.id} reported explicit degradation."
                ],
                preserved_constraints=["FROZEN_RESEARCH_CHARTER", "NO_LIVE_CONTROL"],
                state="ACTIVE",
                created_at=_now(),
            )
            session.add(branch)
            session.flush()
            mission = ResearchMission(
                program_id=program.id,
                branch_id=branch.id,
                type="ALPHA_DEGRADATION_RESEARCH",
                role="ALPHA_RESEARCHER",
                state="READY",
                objective=(
                    f"Diagnose Alpha {alpha.id} using governed Discovery evidence after degraded "
                    f"Forward Evidence episode {episode.id}; propose a new independently evaluated "
                    "candidate rather than modifying downstream runtime state."
                ),
                dependencies=[str(source.id), str(episode.id)],
                attempt=1,
                summary="Degradation-triggered Mission is ready for the Agent Worker.",
            )
            session.add(mission)
            session.flush()
            followup.branch_id = branch.id
            followup.mission_id = mission.id
            job = enqueue_job(
                session,
                kind="RESEARCH_MISSION",
                resource_type="research_mission",
                resource_id=mission.id,
                payload={
                    "program_id": str(program.id),
                    "branch_id": str(branch.id),
                    "alpha_qualification_id": str(alpha.id),
                    "forward_evidence_episode_id": str(episode.id),
                    "trigger": "EXPLICIT_DEGRADATION",
                },
            )
            followup.job_id = job.id
            append_event(
                session,
                kind="DEGRADATION_MISSION_READY",
                aggregate_type="RESEARCH_PROGRAM",
                aggregate_id=program.id,
                payload={
                    "mission_id": str(mission.id),
                    "branch_id": str(branch.id),
                    "alpha_qualification_id": str(alpha.id),
                    "forward_evidence_episode_id": str(episode.id),
                    "job_id": str(job.id),
                },
            )
            created += 1
    return created
