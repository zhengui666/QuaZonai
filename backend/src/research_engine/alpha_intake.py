"""Core-only intake for typed Alpha proposals and Discovery outcomes.

This module deliberately never imports or executes Mission-owned code.  It
copies one structurally validated source file into QZ-owned storage, freezes
the typed references needed by the independent evaluators, and accepts only a
bounded Discovery outcome from their future Core boundary.
"""

from __future__ import annotations

import os
import shutil
import stat
from collections.abc import Sequence
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal, InvalidOperation, ROUND_HALF_UP, localcontext
from pathlib import Path, PurePosixPath
from uuid import UUID, uuid4

from sqlalchemy import func, select
from sqlalchemy.orm import Session

from agent_harness.contracts import (
    AlphaArtifactDraftV1,
    DraftArtifactKind,
    MissionType,
    validate_alpha_artifact_summary,
)
from agent_harness.orchestrator import finish_mission
from db.models import (
    AlphaCalibrationVersion,
    AlphaDiscoveryEvaluation,
    AlphaDiscoveryEvaluationGate,
    AlphaDiscoveryEvaluationMetric,
    AlphaEvaluationAssignment,
    AlphaEvaluationAssignmentDatasetRevision,
    AlphaEvaluationEpisode,
    AlphaModel,
    AlphaModelVersion,
    DatasetRevision,
    EvaluationDatasetSelection,
    EvaluationDesignVersion,
    FeaturePipelineVersion,
    GovernedDataSource,
    MarketUniverseVersion,
    MissionArtifact,
    NautilusCatalogBinding,
    PromotionPolicyGate,
    PromotionPolicyVersion,
    ResearchBranch,
    ResearchCharter,
    ResearchMission,
    ResearchProgram,
)
from errors import QfError
from events import append_event
from jobs import enqueue_job
from research_engine.sealed_evaluator_contracts import (
    DiscoveryEvaluationResult,
    DiscoveryEvaluationStatus,
)

_MAX_ALPHA_SOURCE_BYTES = 1 * 1024 * 1024
_COPY_CHUNK_BYTES = 64 * 1024


@dataclass(frozen=True, slots=True)
class AlphaProposalIntake:
    """The committed result of validating one Mission proposal."""

    discovery_evaluation_id: UUID | None
    error_code: str | None

    @property
    def accepted(self) -> bool:
        return self.discovery_evaluation_id is not None


def _now() -> datetime:
    return datetime.now(UTC)


def _reject_proposal(
    session: Session,
    *,
    mission: ResearchMission,
    artifact: MissionArtifact | None,
    error_code: str,
) -> AlphaProposalIntake:
    if artifact is not None and artifact.state == "DRAFT":
        artifact.state = "REJECTED"
    append_event(
        session,
        kind="ALPHA_PROPOSAL_REJECTED",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=mission.program_id,
        payload={
            "mission_id": str(mission.id),
            "artifact_id": str(artifact.id) if artifact is not None else None,
            "error_code": error_code,
        },
    )
    return AlphaProposalIntake(discovery_evaluation_id=None, error_code=error_code)


def _one[T](rows: Sequence[T]) -> T | None:
    return rows[0] if len(rows) == 1 else None


def _active_selection(
    session: Session, universe_version_id: UUID
) -> EvaluationDatasetSelection | None:
    rows = list(
        session.scalars(
            select(EvaluationDatasetSelection)
            .where(
                EvaluationDatasetSelection.universe_version_id == universe_version_id,
                EvaluationDatasetSelection.state == "ENABLED",
            )
            .with_for_update()
        )
    )
    selected = _one(rows)
    return selected if isinstance(selected, EvaluationDatasetSelection) else None


def _active_design(session: Session, universe_version_id: UUID) -> EvaluationDesignVersion | None:
    rows = list(
        session.scalars(
            select(EvaluationDesignVersion)
            .where(
                EvaluationDesignVersion.universe_version_id == universe_version_id,
                EvaluationDesignVersion.state == "ACTIVE",
            )
            .with_for_update()
        )
    )
    selected = _one(rows)
    return selected if isinstance(selected, EvaluationDesignVersion) else None


def _trusted_dataset(
    session: Session,
    dataset_revision_id: UUID,
    *,
    universe_version_id: UUID,
    phase: str,
    sealed: bool,
) -> DatasetRevision | None:
    dataset = session.scalar(
        select(DatasetRevision)
        .where(DatasetRevision.id == dataset_revision_id)
        .with_for_update()
    )
    if (
        dataset is None
        or dataset.universe_version_id != universe_version_id
        or dataset.partition != phase
        or dataset.data_source_id is None
        or dataset.data_class not in {"VENDOR", "PRODUCTION"}
        or dataset.promotability != "PROMOTABLE"
        or dataset.quality_state != "VALID"
        or dataset.point_in_time_state != "VALID"
    ):
        return None
    source = session.scalar(
        select(GovernedDataSource)
        .where(GovernedDataSource.id == dataset.data_source_id)
        .with_for_update()
    )
    catalog = session.scalar(
        select(NautilusCatalogBinding)
        .where(NautilusCatalogBinding.dataset_revision_id == dataset.id)
        .with_for_update()
    )
    if (
        source is None
        or source.state != "ACTIVE"
        or source.preflight_state != "READY"
        or catalog is None
        or catalog.sealed is not sealed
        or catalog.quality_state != "VALID"
        or catalog.point_in_time_state != "VALID"
    ):
        return None
    return dataset


def _frozen_charter_accepts(
    mission: ResearchMission,
    draft: AlphaArtifactDraftV1,
) -> bool:
    snapshot = mission.input_snapshot
    charter = snapshot.get("charter") if isinstance(snapshot, dict) else None
    if not isinstance(charter, dict):
        return False
    universe_ids = charter.get("universe_version_ids")
    horizon = charter.get("prediction_horizon")
    if not isinstance(universe_ids, list) or not isinstance(horizon, str):
        return False
    return str(draft.universe_version_id) in {str(item) for item in universe_ids} and (
        draft.horizon == horizon.strip().upper()
    )


def _proposal_source(workspace: Path, draft: AlphaArtifactDraftV1) -> Path | None:
    """Resolve one proposal file without following it outside the finished worktree."""
    try:
        if workspace.is_symlink():
            return None
        root = workspace.resolve(strict=True)
        candidate = root
        for component in PurePosixPath(draft.source_path).parts:
            candidate /= component
            if candidate.is_symlink():
                return None
        source = candidate.resolve(strict=True)
        source.relative_to(root)
    except (OSError, ValueError):
        return None
    if candidate.is_symlink() or not source.is_file() or source.suffix != ".py":
        return None
    module, _, _callable = draft.entrypoint.partition(":")
    expected_module = PurePosixPath(draft.source_path).with_suffix("").as_posix().replace("/", ".")
    if module != expected_module:
        return None
    return source


def _copy_owned_source(
    source: Path,
    *,
    artifact_root: Path,
    artifact_id: UUID,
    source_path: str,
) -> str:
    """Copy a regular source file to a private QZ path without trusting agent URIs."""
    if artifact_root.is_symlink():
        raise OSError("artifact root must not be a symbolic link")
    artifact_root.mkdir(parents=True, exist_ok=True)
    root = artifact_root.resolve(strict=True)
    os.chmod(root, 0o700)
    alpha_root = root / "alpha-models"
    if alpha_root.is_symlink():
        raise OSError("alpha artifact root must not be a symbolic link")
    alpha_root.mkdir(exist_ok=True)
    os.chmod(alpha_root, 0o700)
    model_root = alpha_root / str(artifact_id)
    if model_root.is_symlink():
        raise OSError("model artifact root must not be a symbolic link")
    model_root.mkdir(exist_ok=True)
    os.chmod(model_root, 0o700)
    copy_directory = model_root / str(uuid4())
    copy_directory.mkdir(parents=True, exist_ok=False)
    os.chmod(copy_directory, 0o700)
    try:
        destination = copy_directory / Path(PurePosixPath(source_path))
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.parent.resolve().is_relative_to(root):
            raise OSError("artifact destination escapes its root")
        read_flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
        source_fd = os.open(source, read_flags)
        try:
            if not stat.S_ISREG(os.fstat(source_fd).st_mode):
                raise OSError("proposal source must remain a regular file")
            destination_fd = os.open(
                destination,
                os.O_WRONLY | os.O_CREAT | os.O_EXCL,
                0o600,
            )
            try:
                with os.fdopen(source_fd, "rb", closefd=False) as reader, os.fdopen(
                    destination_fd, "wb", closefd=False
                ) as writer:
                    copied_bytes = 0
                    while chunk := reader.read(_COPY_CHUNK_BYTES):
                        copied_bytes += len(chunk)
                        if copied_bytes > _MAX_ALPHA_SOURCE_BYTES:
                            raise OSError("proposal source exceeds the 1 MiB V1 limit")
                        writer.write(chunk)
            finally:
                os.close(destination_fd)
        finally:
            os.close(source_fd)
    except BaseException:
        shutil.rmtree(copy_directory, ignore_errors=True)
        raise
    os.chmod(destination, 0o600)
    return f"artifact://mission-artifacts/{artifact_id}/{copy_directory.name}/{source_path}"


def _proposal_artifact(
    session: Session, mission: ResearchMission
) -> tuple[MissionArtifact | None, AlphaProposalIntake | None]:
    artifacts = list(
        session.scalars(
            select(MissionArtifact)
            .where(
                MissionArtifact.mission_id == mission.id,
                MissionArtifact.kind == DraftArtifactKind.ALPHA_PROPOSAL.value,
            )
            .with_for_update()
        )
    )
    validated = [artifact for artifact in artifacts if artifact.state == "VALIDATED"]
    drafts = [artifact for artifact in artifacts if artifact.state == "DRAFT"]
    if len(validated) == 1 and not drafts:
        discovery = session.scalar(
            select(AlphaDiscoveryEvaluation).where(
                AlphaDiscoveryEvaluation.source_mission_artifact_id == validated[0].id
            )
        )
        if discovery is not None:
            return None, AlphaProposalIntake(discovery.id, None)
        return None, _reject_proposal(
            session,
            mission=mission,
            artifact=None,
            error_code="ALPHA_DISCOVERY_EVIDENCE_MISSING",
        )
    if len(drafts) != 1 or validated:
        for artifact in drafts:
            artifact.state = "REJECTED"
        return None, _reject_proposal(
            session,
            mission=mission,
            artifact=None,
            error_code="ALPHA_PROPOSAL_ARTIFACT_AMBIGUOUS",
        )
    return drafts[0], None


def stage_alpha_discovery_evaluation(
    session: Session,
    *,
    mission_id: UUID,
    workspace: Path,
    artifact_root: Path,
) -> AlphaProposalIntake:
    """Validate and materialize an Alpha proposal, then queue Discovery evaluation.

    Expected structural failures are recorded as a rejected proposal and returned
    to the Mission runner so it can finish the Mission without unlocking its DAG.
    """
    mission = session.scalar(
        select(ResearchMission).where(ResearchMission.id == mission_id).with_for_update()
    )
    if mission is None:
        raise QfError("MISSION_NOT_FOUND", "Mission was not found.", 404)
    if mission.mission_type != MissionType.ALPHA_DISCOVERY.value or mission.state != "RUNNING":
        raise QfError(
            "MISSION_STATE_CONFLICT",
            "Only a RUNNING ALPHA_DISCOVERY Mission can enter Discovery evaluation.",
            409,
            {"state": mission.state, "mission_type": mission.mission_type},
        )
    artifact, prior = _proposal_artifact(session, mission)
    if prior is not None:
        return prior
    assert artifact is not None
    if artifact.schema_version != "v1" or not isinstance(artifact.metadata_json, dict):
        return _reject_proposal(
            session,
            mission=mission,
            artifact=artifact,
            error_code="ALPHA_PROPOSAL_INVALID",
        )
    try:
        summary = artifact.metadata_json["summary"]
        if not isinstance(summary, str):
            raise TypeError("Alpha proposal summary must be text")
        validate_alpha_artifact_summary(summary)
        draft = AlphaArtifactDraftV1.model_validate(artifact.metadata_json["payload"])
    except (KeyError, TypeError, ValueError):
        return _reject_proposal(
            session,
            mission=mission,
            artifact=artifact,
            error_code="ALPHA_PROPOSAL_INVALID",
        )
    if mission.cycle_id is None or not _frozen_charter_accepts(mission, draft):
        return _reject_proposal(
            session,
            mission=mission,
            artifact=artifact,
            error_code="ALPHA_PROPOSAL_SCOPE_INVALID",
        )
    program = session.get(ResearchProgram, mission.program_id)
    branch = session.get(ResearchBranch, mission.branch_id)
    charter = session.get(ResearchCharter, program.charter_id) if program is not None else None
    universe = session.get(MarketUniverseVersion, draft.universe_version_id)
    feature = (
        session.get(FeaturePipelineVersion, draft.feature_pipeline_ref)
        if draft.feature_pipeline_ref is not None
        else None
    )
    if (
        program is None
        or branch is None
        or branch.program_id != program.id
        or charter is None
        or universe is None
        or universe.state != "ACTIVE"
        or (feature is not None and feature.universe_version_id != universe.id)
        or len(draft.family_key) > 120
    ):
        return _reject_proposal(
            session,
            mission=mission,
            artifact=artifact,
            error_code="ALPHA_PROPOSAL_SCOPE_INVALID",
        )
    source = _proposal_source(workspace, draft)
    if source is None:
        return _reject_proposal(
            session,
            mission=mission,
            artifact=artifact,
            error_code="ALPHA_PROPOSAL_SOURCE_INVALID",
        )
    selection = _active_selection(session, universe.id)
    design = _active_design(session, universe.id)
    if (
        selection is None
        or design is None
        or design.allowed_model_mode != "RELATIVE_SCORE"
    ):
        return _reject_proposal(
            session,
            mission=mission,
            artifact=artifact,
            error_code="ALPHA_DISCOVERY_CONFIGURATION_UNAVAILABLE",
        )
    discovery_dataset = _trusted_dataset(
        session,
        selection.discovery_dataset_revision_id,
        universe_version_id=universe.id,
        phase="DISCOVERY",
        sealed=False,
    )
    validation_dataset = _trusted_dataset(
        session,
        selection.validation_dataset_revision_id,
        universe_version_id=universe.id,
        phase="VALIDATION",
        sealed=False,
    )
    sealed_dataset = _trusted_dataset(
        session,
        selection.sealed_dataset_revision_id,
        universe_version_id=universe.id,
        phase="SEALED",
        sealed=True,
    )
    if discovery_dataset is None or validation_dataset is None or sealed_dataset is None:
        return _reject_proposal(
            session,
            mission=mission,
            artifact=artifact,
            error_code="ALPHA_DISCOVERY_DATA_UNAVAILABLE",
        )
    model = session.scalar(
        select(AlphaModel).where(AlphaModel.alpha_key == draft.family_key).with_for_update()
    )
    if model is not None and model.owner_program_id != program.id:
        return _reject_proposal(
            session,
            mission=mission,
            artifact=artifact,
            error_code="ALPHA_MODEL_OWNER_CONFLICT",
        )
    try:
        artifact_uri = _copy_owned_source(
            source,
            artifact_root=artifact_root,
            artifact_id=artifact.id,
            source_path=draft.source_path,
        )
    except OSError:
        return _reject_proposal(
            session,
            mission=mission,
            artifact=artifact,
            error_code="ALPHA_PROPOSAL_MATERIALIZATION_FAILED",
        )
    if model is None:
        model = AlphaModel(
            alpha_key=draft.family_key,
            name=draft.family_key,
            family=draft.family_key,
            description=draft.hypothesis,
            owner_program_id=program.id,
            state="RESEARCHING",
        )
        session.add(model)
        session.flush()
    version_no = int(
        session.scalar(
            select(func.coalesce(func.max(AlphaModelVersion.version_no), 0)).where(
                AlphaModelVersion.alpha_model_id == model.id
            )
        )
        or 0
    ) + 1
    artifact.storage_uri = artifact_uri
    artifact.state = "VALIDATED"
    model_version = AlphaModelVersion(
        alpha_model_id=model.id,
        version_no=version_no,
        source_mission_id=mission.id,
        source_mission_artifact_id=artifact.id,
        source_mission_artifact_revision=artifact.revision,
        universe_version_id=universe.id,
        feature_pipeline_version_id=feature.id if feature is not None else None,
        horizon=draft.horizon,
        mode="RELATIVE_SCORE",
        artifact_uri=artifact_uri,
        entrypoint=draft.entrypoint,
        parameters=draft.parameters,
        input_contract=draft.input_contract,
        output_contract={"schema": draft.output_contract},
        state="DRAFT",
    )
    session.add(model_version)
    session.flush()
    cause = append_event(
        session,
        kind="ALPHA_PROPOSAL_VALIDATED",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=program.id,
        payload={
            "mission_id": str(mission.id),
            "artifact_id": str(artifact.id),
            "alpha_model_version_id": str(model_version.id),
        },
    )
    discovery = AlphaDiscoveryEvaluation(
        source_mission_artifact_id=artifact.id,
        source_mission_artifact_revision=artifact.revision,
        alpha_model_version_id=model_version.id,
        program_id=program.id,
        cycle_id=mission.cycle_id,
        branch_id=branch.id,
        mission_id=mission.id,
        discovery_dataset_revision_id=discovery_dataset.id,
        evaluation_dataset_selection_id=selection.id,
        evaluation_design_version_id=design.id,
        cause_event_id=cause.id,
        evaluator_contract_version=design.contract_version,
        state="QUEUED",
    )
    session.add(discovery)
    session.flush()
    enqueue_job(
        session,
        kind="DISCOVERY_EVALUATION",
        resource_type="alpha_discovery_evaluation",
        resource_id=discovery.id,
        payload={},
    )
    return AlphaProposalIntake(discovery_evaluation_id=discovery.id, error_code=None)


_TERMINAL_DISCOVERY_STATES = frozenset(
    status.value
    for status in (
        DiscoveryEvaluationStatus.VALID,
        DiscoveryEvaluationStatus.INCONCLUSIVE,
        DiscoveryEvaluationStatus.INVALID,
    )
)
_METRIC_QUANTUM = Decimal("0.00000001")
_MAX_METRIC_VALUE = Decimal("999999999999.99999999")


def _utc(value: datetime | None) -> datetime | None:
    if value is None:
        return None
    return value.replace(tzinfo=UTC) if value.tzinfo is None else value.astimezone(UTC)


def _metric_value(value: float | None) -> Decimal | None:
    """Match the fixed Numeric(20, 8) persistence boundary on every retry."""
    if value is None:
        return None
    try:
        decimal_value = Decimal(str(value))
        if abs(decimal_value) > _MAX_METRIC_VALUE:
            raise InvalidOperation
        with localcontext() as context:
            context.prec = 28
            return decimal_value.quantize(_METRIC_QUANTUM, rounding=ROUND_HALF_UP)
    except (InvalidOperation, ValueError) as exc:
        raise QfError(
            "DISCOVERY_EVALUATION_RESULT_INVALID",
            "Discovery metric cannot be persisted as a finite fixed-scale aggregate.",
            422,
        ) from exc


def _result_matches(
    session: Session,
    *,
    discovery: AlphaDiscoveryEvaluation,
    version: AlphaModelVersion,
    result: DiscoveryEvaluationResult,
) -> bool:
    if (
        discovery.state != result.status.value
        or discovery.outcome_code != result.outcome_code
        or discovery.private_result_ref != result.private_result_id
        or _utc(discovery.evaluated_at) != result.evaluated_at
        or result.model_version.id != version.id
        or result.model_version.revision != version.version_no
    ):
        return False
    expected_metrics = tuple(
        (
            metric.code.value,
            _metric_value(metric.value),
            metric.status.value,
        )
        for metric in result.metrics
    )
    actual_metrics = tuple(
        (metric.metric_code, metric.value, metric.status)
        for metric in session.scalars(
            select(AlphaDiscoveryEvaluationMetric)
            .where(AlphaDiscoveryEvaluationMetric.discovery_evaluation_id == discovery.id)
            .order_by(AlphaDiscoveryEvaluationMetric.metric_code)
        )
    )
    if actual_metrics != expected_metrics:
        return False
    expected_gates = tuple(
        (gate.code.value, gate.status.value, gate.reason_code.value if gate.reason_code else None)
        for gate in result.gates
    )
    actual_gates = tuple(
        (gate.gate_code, gate.status, gate.reason_code)
        for gate in session.scalars(
            select(AlphaDiscoveryEvaluationGate)
            .where(AlphaDiscoveryEvaluationGate.discovery_evaluation_id == discovery.id)
            .order_by(AlphaDiscoveryEvaluationGate.gate_code)
        )
    )
    if actual_gates != expected_gates:
        return False
    calibrations = list(
        session.scalars(
            select(AlphaCalibrationVersion)
            .where(AlphaCalibrationVersion.source_discovery_evaluation_id == discovery.id)
            .with_for_update()
        )
    )
    if result.calibration is None:
        return not calibrations
    if len(calibrations) != 1:
        return False
    calibration = calibrations[0]
    training_dataset = session.get(DatasetRevision, calibration.training_dataset_revision_id)
    return (
        training_dataset is not None
        and calibration.alpha_model_version_id == version.id
        and calibration.method == result.calibration.method.value
        and calibration.training_dataset_revision_id == result.calibration.training_dataset.id
        and training_dataset.revision_no == result.calibration.training_dataset.revision
        and calibration.private_artifact_ref == result.calibration.private_artifact_ref
        and calibration.artifact_uri is None
        and calibration.training_dataset_revision_ids == []
        and calibration.parameters == {}
        and calibration.metrics == {}
        and calibration.state == "VALIDATED"
    )


def _frozen_evaluation_inputs(
    session: Session,
    *,
    discovery: AlphaDiscoveryEvaluation,
    version: AlphaModelVersion,
) -> tuple[
    EvaluationDatasetSelection,
    EvaluationDesignVersion,
    DatasetRevision,
    DatasetRevision,
    DatasetRevision,
] | None:
    selection = session.scalar(
        select(EvaluationDatasetSelection)
        .where(EvaluationDatasetSelection.id == discovery.evaluation_dataset_selection_id)
        .with_for_update()
    )
    design = session.scalar(
        select(EvaluationDesignVersion)
        .where(EvaluationDesignVersion.id == discovery.evaluation_design_version_id)
        .with_for_update()
    )
    if (
        selection is None
        or design is None
        or selection.universe_version_id != version.universe_version_id
        or selection.discovery_dataset_revision_id != discovery.discovery_dataset_revision_id
        or design.universe_version_id != version.universe_version_id
        or design.contract_version != discovery.evaluator_contract_version
        or design.allowed_model_mode != version.mode
    ):
        return None
    discovery_dataset = _trusted_dataset(
        session,
        selection.discovery_dataset_revision_id,
        universe_version_id=version.universe_version_id,
        phase="DISCOVERY",
        sealed=False,
    )
    validation_dataset = _trusted_dataset(
        session,
        selection.validation_dataset_revision_id,
        universe_version_id=version.universe_version_id,
        phase="VALIDATION",
        sealed=False,
    )
    sealed_dataset = _trusted_dataset(
        session,
        selection.sealed_dataset_revision_id,
        universe_version_id=version.universe_version_id,
        phase="SEALED",
        sealed=True,
    )
    if discovery_dataset is None or validation_dataset is None or sealed_dataset is None:
        return None
    return selection, design, discovery_dataset, validation_dataset, sealed_dataset


def _active_sealed_policy(session: Session) -> PromotionPolicyVersion | None:
    policies = list(
        session.scalars(
            select(PromotionPolicyVersion)
            .where(
                PromotionPolicyVersion.purpose == "SEALED_TO_QUALIFIED",
                PromotionPolicyVersion.state == "ACTIVE",
            )
            .with_for_update()
        )
    )
    policy = _one(policies)
    if not isinstance(policy, PromotionPolicyVersion):
        return None
    gates = list(
        session.scalars(
            select(PromotionPolicyGate)
            .where(PromotionPolicyGate.policy_version_id == policy.id)
            .order_by(PromotionPolicyGate.ordinal)
            .with_for_update()
        )
    )
    return (
        policy
        if gates and [gate.ordinal for gate in gates] == list(range(1, len(gates) + 1))
        else None
    )


def _persist_discovery_result(
    session: Session,
    *,
    discovery: AlphaDiscoveryEvaluation,
    result: DiscoveryEvaluationResult,
) -> None:
    if (
        session.scalar(
            select(AlphaDiscoveryEvaluationMetric)
            .where(AlphaDiscoveryEvaluationMetric.discovery_evaluation_id == discovery.id)
            .limit(1)
        )
        is not None
        or session.scalar(
            select(AlphaDiscoveryEvaluationGate)
            .where(AlphaDiscoveryEvaluationGate.discovery_evaluation_id == discovery.id)
            .limit(1)
        )
        is not None
    ):
        raise QfError(
            "DISCOVERY_EVALUATION_STATE_CONFLICT",
            "Discovery evaluation already has immutable result details.",
            409,
        )
    discovery.state = result.status.value
    discovery.outcome_code = result.outcome_code
    discovery.private_result_ref = result.private_result_id
    discovery.evaluated_at = result.evaluated_at
    discovery.completed_at = _now()
    session.add_all(
        AlphaDiscoveryEvaluationMetric(
            discovery_evaluation_id=discovery.id,
            metric_code=metric.code.value,
            value=_metric_value(metric.value),
            status=metric.status.value,
        )
        for metric in result.metrics
    )
    session.add_all(
        AlphaDiscoveryEvaluationGate(
            discovery_evaluation_id=discovery.id,
            gate_code=gate.code.value,
            status=gate.status.value,
            reason_code=gate.reason_code.value if gate.reason_code else None,
        )
        for gate in result.gates
    )
    session.flush()


def _create_calibration(
    session: Session,
    *,
    discovery: AlphaDiscoveryEvaluation,
    version: AlphaModelVersion,
    discovery_dataset: DatasetRevision,
    result: DiscoveryEvaluationResult,
) -> AlphaCalibrationVersion | None:
    descriptor = result.calibration
    if descriptor is None:
        return None
    _validate_calibration_descriptor(
        session,
        discovery=discovery,
        version=version,
        discovery_dataset=discovery_dataset,
        result=result,
    )
    version_no = int(
        session.scalar(
            select(func.coalesce(func.max(AlphaCalibrationVersion.version_no), 0)).where(
                AlphaCalibrationVersion.alpha_model_version_id == version.id
            )
        )
        or 0
    ) + 1
    calibration = AlphaCalibrationVersion(
        alpha_model_version_id=version.id,
        version_no=version_no,
        method=descriptor.method.value,
        training_dataset_revision_ids=[],
        artifact_uri=None,
        source_discovery_evaluation_id=discovery.id,
        training_dataset_revision_id=discovery_dataset.id,
        private_artifact_ref=descriptor.private_artifact_ref,
        parameters={},
        metrics={},
        state="VALIDATED",
    )
    session.add(calibration)
    session.flush()
    return calibration


def _validate_calibration_descriptor(
    session: Session,
    *,
    discovery: AlphaDiscoveryEvaluation,
    version: AlphaModelVersion,
    discovery_dataset: DatasetRevision,
    result: DiscoveryEvaluationResult,
) -> None:
    """Reject a forged calibration before accepting any terminal Discovery facts."""
    descriptor = result.calibration
    if descriptor is None:
        return
    if (
        descriptor.training_dataset.id != discovery_dataset.id
        or descriptor.training_dataset.revision != discovery_dataset.revision_no
    ):
        raise QfError(
            "ALPHA_DISCOVERY_CALIBRATION_INVALID",
            "Calibration must use the frozen Discovery Dataset.",
            409,
        )
    existing = list(
        session.scalars(
            select(AlphaCalibrationVersion)
            .where(AlphaCalibrationVersion.source_discovery_evaluation_id == discovery.id)
            .with_for_update()
        )
    )
    if existing:
        raise QfError(
            "ALPHA_DISCOVERY_CALIBRATION_CONFLICT",
            "Discovery evaluation already has calibration provenance.",
            409,
        )


def _validated_result(result: DiscoveryEvaluationResult) -> DiscoveryEvaluationResult:
    """Rebuild the boundary value so malformed in-memory subclasses cannot bypass its checks."""
    try:
        return DiscoveryEvaluationResult(
            discovery_evaluation_id=result.discovery_evaluation_id,
            model_version=result.model_version,
            status=result.status,
            private_result_id=result.private_result_id,
            evaluated_at=result.evaluated_at,
            metrics=result.metrics,
            gates=result.gates,
            calibration=result.calibration,
        )
    except (AttributeError, TypeError, ValueError) as exc:
        raise QfError(
            "DISCOVERY_EVALUATION_RESULT_INVALID",
            "Discovery evaluator result does not satisfy its typed contract.",
            422,
        ) from exc


def _failure_code(status: DiscoveryEvaluationStatus) -> str:
    return (
        "DISCOVERY_EVIDENCE_INCONCLUSIVE"
        if status is DiscoveryEvaluationStatus.INCONCLUSIVE
        else "DISCOVERY_EVIDENCE_INVALID"
    )


def accept_discovery_evaluation_result(
    session: Session,
    result: DiscoveryEvaluationResult,
) -> AlphaEvaluationAssignment | None:
    """Persist one evaluator-only typed Discovery result and, only if valid, assign Sealed work."""
    if not isinstance(result, DiscoveryEvaluationResult):
        raise QfError(
            "DISCOVERY_EVALUATION_RESULT_REQUIRED",
            "Discovery evaluation requires a typed evaluator result.",
            422,
        )
    result = _validated_result(result)
    discovery = session.scalar(
        select(AlphaDiscoveryEvaluation)
        .where(AlphaDiscoveryEvaluation.id == result.discovery_evaluation_id)
        .with_for_update()
    )
    if discovery is None:
        raise QfError("ALPHA_DISCOVERY_EVALUATION_NOT_FOUND", "Discovery evaluation was not found.", 404)
    version = session.scalar(
        select(AlphaModelVersion)
        .where(AlphaModelVersion.id == discovery.alpha_model_version_id)
        .with_for_update()
    )
    if version is None:
        raise QfError(
            "ALPHA_DISCOVERY_PROVENANCE_INVALID",
            "Discovery evaluation cannot establish model provenance.",
            409,
        )
    if discovery.state in _TERMINAL_DISCOVERY_STATES:
        if not _result_matches(session, discovery=discovery, version=version, result=result):
            raise QfError(
                "DISCOVERY_EVALUATION_RESULT_CONFLICT",
                "Discovery evaluation already has different immutable result facts.",
                409,
            )
        if result.status is not DiscoveryEvaluationStatus.VALID:
            return None
        assignments = list(
            session.scalars(
                select(AlphaEvaluationAssignment)
                .where(AlphaEvaluationAssignment.discovery_evaluation_id == discovery.id)
                .with_for_update()
            )
        )
        if not assignments:
            mission = session.scalar(
                select(ResearchMission)
                .where(ResearchMission.id == discovery.mission_id)
                .with_for_update()
            )
            if (
                mission is not None
                and mission.state == "FAILED"
                and mission.error_code
                in {"ALPHA_EVALUATION_ASSIGNMENT_UNAVAILABLE", "ALPHA_CALIBRATION_REQUIRED"}
            ):
                return None
        if len(assignments) != 1:
            raise QfError(
                "ALPHA_DISCOVERY_ASSIGNMENT_MISSING",
                "A valid Discovery evaluation requires one immutable assignment.",
                409,
            )
        return assignments[0]
    if discovery.state not in {"FROZEN", "QUEUED", "RUNNING"}:
        raise QfError(
            "DISCOVERY_EVALUATION_STATE_CONFLICT",
            "Discovery evaluation is not awaiting an evaluator result.",
            409,
            {"state": discovery.state},
        )
    mission = session.scalar(
        select(ResearchMission)
        .where(ResearchMission.id == discovery.mission_id)
        .with_for_update()
    )
    model = session.scalar(
        select(AlphaModel)
        .where(AlphaModel.id == version.alpha_model_id)
        .with_for_update()
    )
    artifact = session.scalar(
        select(MissionArtifact)
        .where(MissionArtifact.id == discovery.source_mission_artifact_id)
        .with_for_update()
    )
    if (
        mission is None
        or mission.state != "AWAITING_VALIDATION"
        or mission.mission_type != MissionType.ALPHA_DISCOVERY.value
        or model is None
        or model.state != "RESEARCHING"
        or artifact is None
        or artifact.mission_id != mission.id
        or artifact.kind != DraftArtifactKind.ALPHA_PROPOSAL.value
        or artifact.state != "VALIDATED"
        or artifact.revision != discovery.source_mission_artifact_revision
        or version.state != "DRAFT"
        or result.model_version.id != version.id
        or result.model_version.revision != version.version_no
        or version.source_mission_id != mission.id
        or version.source_mission_artifact_id != artifact.id
        or version.source_mission_artifact_revision != artifact.revision
        or model.owner_program_id != discovery.program_id
        or mission.program_id != discovery.program_id
        or mission.branch_id != discovery.branch_id
        or mission.cycle_id != discovery.cycle_id
    ):
        raise QfError(
            "ALPHA_DISCOVERY_PROVENANCE_INVALID",
            "Discovery result does not match its frozen Alpha provenance.",
            409,
        )
    inputs = _frozen_evaluation_inputs(session, discovery=discovery, version=version)
    if inputs is None:
        raise QfError(
            "ALPHA_DISCOVERY_FROZEN_INPUT_UNAVAILABLE",
            "Frozen Discovery evaluation inputs are no longer trusted.",
            409,
        )
    _, design, discovery_dataset, validation_dataset, sealed_dataset = inputs
    _validate_calibration_descriptor(
        session,
        discovery=discovery,
        version=version,
        discovery_dataset=discovery_dataset,
        result=result,
    )
    _persist_discovery_result(session, discovery=discovery, result=result)
    cause = append_event(
        session,
        kind=f"ALPHA_DISCOVERY_EVALUATION_{result.status.value}",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=discovery.program_id,
        payload={
            "mission_id": str(mission.id),
            "discovery_evaluation_id": str(discovery.id),
            "outcome_code": result.outcome_code,
        },
    )
    if result.status is not DiscoveryEvaluationStatus.VALID:
        version.state = "REJECTED"
        finish_mission(
            session,
            mission.id,
            succeeded=False,
            error_code=_failure_code(result.status),
        )
        return None
    calibration = _create_calibration(
        session,
        discovery=discovery,
        version=version,
        discovery_dataset=discovery_dataset,
        result=result,
    )
    version.state = "VALIDATED"
    if version.mode == "CALIBRATED_RETURN" and calibration is None:
        finish_mission(
            session,
            mission.id,
            succeeded=False,
            error_code="ALPHA_CALIBRATION_REQUIRED",
        )
        return None
    policy = _active_sealed_policy(session)
    if policy is None:
        finish_mission(
            session,
            mission.id,
            succeeded=False,
            error_code="ALPHA_EVALUATION_ASSIGNMENT_UNAVAILABLE",
        )
        return None
    assignment_no = int(
        session.scalar(
            select(func.coalesce(func.max(AlphaEvaluationAssignment.assignment_no), 0)).where(
                AlphaEvaluationAssignment.alpha_model_version_id == version.id,
                AlphaEvaluationAssignment.cycle_id == discovery.cycle_id,
            )
        )
        or 0
    ) + 1
    assignment = AlphaEvaluationAssignment(
        source_mission_artifact_id=artifact.id,
        source_mission_artifact_revision=artifact.revision,
        discovery_evaluation_id=discovery.id,
        program_id=discovery.program_id,
        cycle_id=discovery.cycle_id,
        branch_id=discovery.branch_id,
        mission_id=mission.id,
        alpha_model_version_id=version.id,
        alpha_calibration_version_id=calibration.id if calibration is not None else None,
        universe_version_id=version.universe_version_id,
        sealed_dataset_revision_id=sealed_dataset.id,
        evaluation_design_version_id=design.id,
        promotion_policy_version_id=policy.id,
        cause_event_id=cause.id,
        assignment_no=assignment_no,
        evaluator_contract_version=design.contract_version,
        state="QUEUED",
    )
    session.add(assignment)
    session.flush()
    session.add_all(
        (
            AlphaEvaluationAssignmentDatasetRevision(
                assignment_id=assignment.id,
                dataset_revision_id=dataset.id,
                phase=phase,
                ordinal=1,
            )
            for dataset, phase in (
                (discovery_dataset, "DISCOVERY"),
                (validation_dataset, "VALIDATION"),
                (sealed_dataset, "SEALED"),
            )
        )
    )
    session.add(
        AlphaEvaluationEpisode(
            program_id=assignment.program_id,
            branch_id=assignment.branch_id,
            alpha_model_version_id=assignment.alpha_model_version_id,
            assignment_id=assignment.id,
            discovery_run_ids=[],
            validation_run_ids=[],
            sealed_run_id=None,
            sealed_dataset_revision_id=assignment.sealed_dataset_revision_id,
            promotion_policy_version_id=assignment.promotion_policy_version_id,
            state="ASSIGNED",
            result=None,
            gate_results={},
            multiple_testing_summary={},
            disclosure={},
        )
    )
    session.flush()
    enqueue_job(
        session,
        kind="ALPHA_EVALUATION",
        resource_type="alpha_evaluation_assignment",
        resource_id=assignment.id,
        payload={},
    )
    finish_mission(
        session,
        mission.id,
        succeeded=True,
        require_validated_output=True,
    )
    return assignment
