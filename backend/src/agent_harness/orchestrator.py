"""Mission outcome transitions and bounded dependency scheduling."""

from __future__ import annotations

from datetime import UTC, datetime
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from agent_harness.contracts import DraftArtifactKind, MissionType
from db.models import (
    AgentSession,
    AlphaDiscoveryEvaluation,
    AlphaDiscoveryEvaluationGate,
    AlphaDiscoveryEvaluationMetric,
    AlphaCalibrationVersion,
    AlphaEvaluationAssignment,
    AlphaEvaluationAssignmentDatasetRevision,
    AlphaEvaluationEpisode,
    AlphaModelVersion,
    EvaluationDatasetSelection,
    EvaluationDesignVersion,
    Event,
    Job,
    MissionArtifact,
    ResearchMission,
    ResearchCycle,
    ResearchProgram,
)
from errors import QfError
from research_lifecycle import queue_eligible_missions


def _now() -> datetime:
    return datetime.now(UTC)


_OUTPUT_KIND_BY_MISSION_TYPE = {
    MissionType.PLAN_RESEARCH: DraftArtifactKind.RESEARCH_PLAN,
    MissionType.DATA_REQUIREMENT: DraftArtifactKind.DATA_REQUIREMENT,
    MissionType.DATA_QUALITY: DraftArtifactKind.DATA_QUALITY_REPORT,
    MissionType.FEATURE_RESEARCH: DraftArtifactKind.FEATURE_PROPOSAL,
    MissionType.ALPHA_DISCOVERY: DraftArtifactKind.ALPHA_PROPOSAL,
    MissionType.ALPHA_CALIBRATION: DraftArtifactKind.CALIBRATION_PROPOSAL,
    MissionType.ROBUSTNESS: DraftArtifactKind.ROBUSTNESS_REPORT,
    MissionType.SEALED_PROMOTION_REVIEW: DraftArtifactKind.PROMOTION_REVIEW,
    MissionType.PORTFOLIO_ASSEMBLY: DraftArtifactKind.PORTFOLIO_PROPOSAL,
    MissionType.PAPER_EVIDENCE_REVIEW: DraftArtifactKind.PAPER_EVIDENCE_REVIEW,
    MissionType.LIVE_PROMOTION_REVIEW: DraftArtifactKind.LIVE_PROMOTION_REVIEW,
    MissionType.DEGRADATION_DIAGNOSIS: DraftArtifactKind.DEGRADATION_REPORT,
    MissionType.REPLAN: DraftArtifactKind.REPLAN_PROPOSAL,
}


def expected_output_kind(mission_type: str) -> DraftArtifactKind:
    """Return the one contract-required output kind for a Mission type."""
    try:
        return _OUTPUT_KIND_BY_MISSION_TYPE[MissionType(mission_type)]
    except (KeyError, ValueError) as exc:
        raise QfError(
            "MISSION_OUTPUT_SCHEMA_UNKNOWN",
            "Mission type has no registered required output artifact.",
            409,
            {"mission_type": mission_type},
        ) from exc


def _require_validated_output(session: Session, mission: ResearchMission) -> MissionArtifact:
    kind = expected_output_kind(mission.mission_type)
    artifact = session.scalar(
        select(MissionArtifact)
        .where(
            MissionArtifact.mission_id == mission.id,
            MissionArtifact.kind == kind.value,
            MissionArtifact.state == "VALIDATED",
        )
        .order_by(MissionArtifact.revision.desc())
        .limit(1)
    )
    if artifact is None:
        raise QfError(
            "MISSION_VALIDATED_ARTIFACT_REQUIRED",
            "Mission success requires a Core-validated required output artifact.",
            409,
            {"artifact_kind": kind.value},
        )
    return artifact


def _require_alpha_discovery_completion(session: Session, mission: ResearchMission) -> None:
    """Require the Core evidence chain that alone closes Alpha Discovery."""
    artifact = _require_validated_output(session, mission)
    discovery = session.scalar(
        select(AlphaDiscoveryEvaluation)
        .where(
            AlphaDiscoveryEvaluation.mission_id == mission.id,
            AlphaDiscoveryEvaluation.source_mission_artifact_id == artifact.id,
            AlphaDiscoveryEvaluation.source_mission_artifact_revision == artifact.revision,
            AlphaDiscoveryEvaluation.state == "VALID",
        )
        .with_for_update()
    )
    if discovery is None:
        raise QfError(
            "ALPHA_DISCOVERY_VALIDATION_REQUIRED",
            "Alpha Discovery success requires a Core-validated Discovery evaluation.",
            409,
            {"mission_id": str(mission.id)},
        )
    version = (
        session.scalar(
            select(AlphaModelVersion)
            .where(AlphaModelVersion.id == discovery.alpha_model_version_id)
            .with_for_update()
        )
    )
    metrics = (
        list(
            session.scalars(
                select(AlphaDiscoveryEvaluationMetric)
                .where(AlphaDiscoveryEvaluationMetric.discovery_evaluation_id == discovery.id)
                .with_for_update()
            )
        )
    )
    gates = (
        list(
            session.scalars(
                select(AlphaDiscoveryEvaluationGate)
                .where(AlphaDiscoveryEvaluationGate.discovery_evaluation_id == discovery.id)
                .with_for_update()
            )
        )
    )
    selection = (
        session.scalar(
            select(EvaluationDatasetSelection)
            .where(EvaluationDatasetSelection.id == discovery.evaluation_dataset_selection_id)
            .with_for_update()
        )
    )
    design = (
        session.scalar(
            select(EvaluationDesignVersion)
            .where(EvaluationDesignVersion.id == discovery.evaluation_design_version_id)
            .with_for_update()
        )
    )
    assignment = (
        session.scalar(
            select(AlphaEvaluationAssignment)
            .where(
                AlphaEvaluationAssignment.discovery_evaluation_id == discovery.id,
                AlphaEvaluationAssignment.mission_id == mission.id,
                AlphaEvaluationAssignment.source_mission_artifact_id == artifact.id,
                AlphaEvaluationAssignment.source_mission_artifact_revision == artifact.revision,
                AlphaEvaluationAssignment.alpha_model_version_id == discovery.alpha_model_version_id,
                AlphaEvaluationAssignment.state == "QUEUED",
            )
            .with_for_update()
        )
    )
    episode = (
        session.scalar(
            select(AlphaEvaluationEpisode)
            .where(
                AlphaEvaluationEpisode.assignment_id == assignment.id,
                AlphaEvaluationEpisode.state == "ASSIGNED",
                AlphaEvaluationEpisode.result.is_(None),
            )
            .with_for_update()
        )
        if assignment is not None
        else None
    )
    assignment_datasets = (
        list(
            session.scalars(
                select(AlphaEvaluationAssignmentDatasetRevision)
                .where(AlphaEvaluationAssignmentDatasetRevision.assignment_id == assignment.id)
                .with_for_update()
            )
        )
        if assignment is not None
        else []
    )
    calibration = (
        session.scalar(
            select(AlphaCalibrationVersion)
            .where(AlphaCalibrationVersion.id == assignment.alpha_calibration_version_id)
            .with_for_update()
        )
        if assignment is not None and assignment.alpha_calibration_version_id is not None
        else None
    )
    jobs = (
        list(
            session.scalars(
                select(Job)
                .where(
                    Job.kind == "ALPHA_EVALUATION",
                    Job.resource_type == "alpha_evaluation_assignment",
                    Job.resource_id == assignment.id,
                )
                .with_for_update()
            )
        )
        if assignment is not None
        else []
    )
    if (
        version is None
        or version.state != "VALIDATED"
        or discovery.outcome_code != "VALID"
        or discovery.private_result_ref is None
        or discovery.evaluated_at is None
        or discovery.completed_at is None
        or discovery.program_id != mission.program_id
        or discovery.branch_id != mission.branch_id
        or discovery.cycle_id != mission.cycle_id
        or version.source_mission_id != mission.id
        or version.source_mission_artifact_id != artifact.id
        or version.source_mission_artifact_revision != artifact.revision
        or len(metrics) != 10
        or len(gates) != 5
        or any(gate.status != "PASS" or gate.reason_code is not None for gate in gates)
        or selection is None
        or selection.universe_version_id != version.universe_version_id
        or selection.discovery_dataset_revision_id != discovery.discovery_dataset_revision_id
        or design is None
        or design.universe_version_id != version.universe_version_id
        or design.contract_version != discovery.evaluator_contract_version
        or design.allowed_model_mode != version.mode
        or assignment is None
        or assignment.program_id != discovery.program_id
        or assignment.cycle_id != discovery.cycle_id
        or assignment.branch_id != discovery.branch_id
        or assignment.mission_id != mission.id
        or assignment.universe_version_id != version.universe_version_id
        or assignment.sealed_dataset_revision_id != selection.sealed_dataset_revision_id
        or assignment.evaluation_design_version_id != design.id
        or assignment.evaluator_contract_version != design.contract_version
        or (
            assignment.alpha_calibration_version_id is not None
            and (
                calibration is None
                or calibration.state != "VALIDATED"
                or calibration.alpha_model_version_id != version.id
                or calibration.source_discovery_evaluation_id != discovery.id
            )
        )
        or (version.mode == "CALIBRATED_RETURN" and calibration is None)
        or {
            (row.phase, row.ordinal): row.dataset_revision_id for row in assignment_datasets
        }
        != {
            ("DISCOVERY", 1): selection.discovery_dataset_revision_id,
            ("VALIDATION", 1): selection.validation_dataset_revision_id,
            ("SEALED", 1): selection.sealed_dataset_revision_id,
        }
        or episode is None
        or episode.program_id != assignment.program_id
        or episode.branch_id != assignment.branch_id
        or episode.alpha_model_version_id != assignment.alpha_model_version_id
        or episode.sealed_dataset_revision_id != assignment.sealed_dataset_revision_id
        or episode.promotion_policy_version_id != assignment.promotion_policy_version_id
        or episode.discovery_run_ids != []
        or episode.validation_run_ids != []
        or episode.sealed_run_id is not None
        or episode.gate_results != {}
        or episode.multiple_testing_summary != {}
        or episode.disclosure != {}
        or len(jobs) != 1
        or jobs[0].payload != {}
    ):
        raise QfError(
            "ALPHA_DISCOVERY_VALIDATION_REQUIRED",
            "Alpha Discovery success requires its valid Core Discovery and assignment facts.",
            409,
        )


def await_mission_validation(
    session: Session,
    mission_id: UUID,
    *,
    summary: str | None = None,
) -> ResearchMission:
    """Finish the Codex child while keeping its Mission blocked on Core evidence."""
    mission = session.execute(
        select(ResearchMission).where(ResearchMission.id == mission_id).with_for_update()
    ).scalar_one_or_none()
    if mission is None:
        raise QfError("MISSION_NOT_FOUND", "Mission was not found.", 404)
    if mission.state != "RUNNING":
        raise QfError(
            "MISSION_STATE_CONFLICT",
            "Only RUNNING Missions can await Core validation.",
            409,
            {"state": mission.state},
        )
    _require_validated_output(session, mission)

    timestamp = _now()
    mission.state = "AWAITING_VALIDATION"
    mission.outcome = None
    mission.finished_at = None
    mission.summary = summary[-12_000:] if summary else None
    mission.error_code = None
    mission.revision += 1
    agent_session = session.scalar(
        select(AgentSession).where(AgentSession.mission_id == mission.id).with_for_update()
    )
    if agent_session is not None and agent_session.state in {"PLANNED", "RUNNING"}:
        agent_session.state = "SUCCEEDED"
        agent_session.finished_at = timestamp
        agent_session.last_event_at = timestamp
    program = session.get(ResearchProgram, mission.program_id)
    if program is None:
        raise QfError("RESEARCH_PROGRAM_NOT_FOUND", "Research Program was not found.", 500)
    session.add(
        Event(
            kind="MISSION_AWAITING_VALIDATION",
            aggregate_type="RESEARCH_PROGRAM",
            aggregate_id=program.id,
            actor_kind="SYSTEM",
            actor_metadata={},
            payload={"mission_id": str(mission.id)},
        )
    )
    return mission


def finish_mission(
    session: Session,
    mission_id: UUID,
    *,
    succeeded: bool,
    summary: str | None = None,
    error_code: str | None = None,
    require_validated_output: bool = False,
) -> ResearchMission:
    """Close one Mission and unlock only its satisfied DAG dependants."""
    mission = session.execute(
        select(ResearchMission).where(ResearchMission.id == mission_id).with_for_update()
    ).scalar_one_or_none()
    if mission is None:
        raise QfError("MISSION_NOT_FOUND", "Mission was not found.", 404)
    if mission.state not in {"RUNNING", "AWAITING_VALIDATION"}:
        raise QfError(
            "MISSION_STATE_CONFLICT",
            "Only RUNNING or AWAITING_VALIDATION Missions can be completed.",
            409,
            {"state": mission.state},
        )
    if succeeded:
        if mission.mission_type == MissionType.ALPHA_DISCOVERY.value:
            _require_alpha_discovery_completion(session, mission)
        elif require_validated_output:
            _require_validated_output(session, mission)
    timestamp = _now()
    mission.state = "SUCCEEDED" if succeeded else "FAILED"
    mission.outcome = mission.state
    mission.finished_at = timestamp
    mission.summary = summary[-12_000:] if summary else None
    mission.error_code = None if succeeded else (error_code or "MISSION_FAILED")[:100]
    mission.revision += 1
    agent_session = session.scalar(
        select(AgentSession).where(AgentSession.mission_id == mission.id).with_for_update()
    )
    if agent_session is not None and agent_session.state in {"PLANNED", "RUNNING"}:
        agent_session.state = mission.state
        agent_session.finished_at = timestamp
        agent_session.last_event_at = timestamp
    program = session.get(ResearchProgram, mission.program_id)
    if program is None:
        raise QfError("RESEARCH_PROGRAM_NOT_FOUND", "Research Program was not found.", 500)
    queued = queue_eligible_missions(session, program) if succeeded else []
    if succeeded and not queued:
        pending = session.scalar(
            select(ResearchMission.id)
            .where(
                ResearchMission.program_id == program.id,
                ResearchMission.state.in_(
                    ("PLANNED", "READY", "RUNNING", "AWAITING_VALIDATION", "INTERRUPTED")
                ),
            )
            .limit(1)
        )
        if pending is None:
            cycle = (
                session.scalar(
                    select(ResearchCycle)
                    .where(ResearchCycle.id == mission.cycle_id)
                    .with_for_update()
                )
                if mission.cycle_id is not None
                else None
            )
            if cycle is not None and cycle.state == "RUNNING":
                cycle.state = "SUCCEEDED"
                cycle.finished_at = timestamp
            if program.state == "ACTIVE":
                program.state = "COOLING"
                program.revision += 1
    session.add(
        Event(
            kind="MISSION_SUCCEEDED" if succeeded else "MISSION_FAILED",
            aggregate_type="RESEARCH_PROGRAM",
            aggregate_id=program.id,
            actor_kind="SYSTEM",
            actor_metadata={},
            payload={
                "mission_id": str(mission.id),
                "error_code": mission.error_code,
                "unlocked_mission_ids": [str(item.id) for item in queued],
            },
        )
    )
    return mission
