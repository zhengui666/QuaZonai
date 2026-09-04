"""Fixed Core boundary for independently operated Alpha evaluators."""

from __future__ import annotations

import json
import os
import selectors
import stat
import subprocess
import tempfile
import time
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from db.models import (
    AlphaCalibrationVersion,
    AlphaDiscoveryEvaluation,
    AlphaEvaluationAssignment,
    AlphaEvaluationAssignmentDatasetRevision,
    AlphaEvaluationEpisode,
    AlphaEvaluationForecast as AlphaEvaluationForecastRow,
    AlphaEvaluationGate,
    AlphaEvaluationMetric,
    AlphaEvaluationResult,
    AlphaModel,
    AlphaModelVersion,
    AlphaQualification,
    AlphaSignalArtifact,
    DatasetRevision,
    Disclosure,
    EvaluationDatasetSelection,
    EvaluationDesignVersion,
    EvidenceExposure,
    MissionArtifact,
    NautilusCatalogBinding,
    PortfolioEvaluationAssignment,
    PortfolioEvaluationEpisode,
    PortfolioInputEvaluationAssignment,
    PromotionPolicyGate,
    PromotionPolicyVersion,
)
from errors import QfError
from portfolio_evaluation_service import prepare_portfolio_evaluation as prepare_portfolio_evaluation_facts
from portfolio_input_service import (
    prepare_portfolio_input_evaluation as prepare_portfolio_input_facts,
    stage_initial_portfolio_input_evaluations,
)
from research_engine.sealed_evaluator_contracts import (
    AlphaEvaluationInput,
    AlphaForecast,
    AlphaSignalSummary,
    CalibrationMethod,
    DiscoveryCalibrationArtifact,
    DiscoveryEvaluationResult,
    DiscoveryEvaluationStatus,
    DisclosureClassification,
    DisclosureReasonCode,
    EvaluationPhase,
    EvaluationStatus,
    GateCode,
    GateResult,
    GateStatus,
    ImmutableReference,
    LevelOneDisclosure,
    MetricAggregate,
    MetricCode,
    MetricStatus,
    PortfolioCovariance,
    PortfolioCovarianceMethod,
    PortfolioEvaluationInput,
    PortfolioInputEvaluationInput,
    PortfolioInputEvaluationResult,
    SealedEvaluationResult,
)
from settings import Settings


_MAX_RESULT_BYTES = 1_000_000
_EVALUATOR_ENV = {"PATH": os.defpath, "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8"}


@dataclass(frozen=True, slots=True)
class _DiscoveryRequest:
    descriptor: dict[str, object]
    discovery_evaluation_id: UUID
    model_version: ImmutableReference


@dataclass(frozen=True, slots=True)
class _AlphaRequest:
    descriptor: dict[str, object]
    input: AlphaEvaluationInput


@dataclass(frozen=True, slots=True)
class _PortfolioInputRequest:
    descriptor: dict[str, object]
    input: PortfolioInputEvaluationInput


@dataclass(frozen=True, slots=True)
class _PortfolioEvaluationRequest:
    descriptor: dict[str, object]
    input: PortfolioEvaluationInput


@dataclass(frozen=True, slots=True)
class _AlphaContext:
    assignment: AlphaEvaluationAssignment
    episode: AlphaEvaluationEpisode
    discovery: AlphaDiscoveryEvaluation
    source_artifact: MissionArtifact
    model: AlphaModel
    model_version: AlphaModelVersion
    calibration: AlphaCalibrationVersion | None
    datasets: dict[str, DatasetRevision]
    selection: EvaluationDatasetSelection
    design: EvaluationDesignVersion
    policy: PromotionPolicyVersion
    input: AlphaEvaluationInput


def _invalid_frozen_input() -> QfError:
    return QfError(
        "TRUSTED_EVALUATOR_FROZEN_INPUT_INVALID",
        "Trusted evaluator input facts are incomplete or inconsistent.",
        409,
    )


def _reference(identifier: UUID, revision: int) -> ImmutableReference:
    try:
        return ImmutableReference(identifier, revision)
    except (TypeError, ValueError) as exc:
        raise _invalid_frozen_input() from exc


def _reference_payload(reference: ImmutableReference) -> dict[str, object]:
    return {"id": str(reference.id), "revision": reference.revision}


def _locked_discovery_request(session: Session, discovery_id: UUID) -> _DiscoveryRequest:
    discovery = session.scalar(
        select(AlphaDiscoveryEvaluation)
        .where(AlphaDiscoveryEvaluation.id == discovery_id)
        .with_for_update()
    )
    if discovery is None:
        raise QfError(
            "ALPHA_DISCOVERY_EVALUATION_NOT_FOUND",
            "Discovery evaluation was not found.",
            404,
        )
    if discovery.state not in {"QUEUED", "RUNNING"}:
        raise QfError(
            "DISCOVERY_EVALUATION_STATE_CONFLICT",
            "Discovery evaluation is not awaiting evaluator work.",
            409,
        )
    artifact = session.scalar(
        select(MissionArtifact)
        .where(MissionArtifact.id == discovery.source_mission_artifact_id)
        .with_for_update()
    )
    model = session.scalar(
        select(AlphaModelVersion)
        .where(AlphaModelVersion.id == discovery.alpha_model_version_id)
        .with_for_update()
    )
    selection = session.scalar(
        select(EvaluationDatasetSelection)
        .where(EvaluationDatasetSelection.id == discovery.evaluation_dataset_selection_id)
        .with_for_update()
    )
    dataset = session.scalar(
        select(DatasetRevision)
        .where(DatasetRevision.id == discovery.discovery_dataset_revision_id)
        .with_for_update()
    )
    design = session.scalar(
        select(EvaluationDesignVersion)
        .where(EvaluationDesignVersion.id == discovery.evaluation_design_version_id)
        .with_for_update()
    )
    if (
        artifact is None
        or model is None
        or selection is None
        or dataset is None
        or design is None
        or artifact.revision != discovery.source_mission_artifact_revision
        or model.source_mission_artifact_id != artifact.id
        or model.source_mission_artifact_revision != artifact.revision
        or selection.discovery_dataset_revision_id != dataset.id
        or design.universe_version_id != model.universe_version_id
        or design.allowed_model_mode != model.mode
        or design.contract_version != discovery.evaluator_contract_version
    ):
        raise _invalid_frozen_input()
    source_artifact = _reference(artifact.id, artifact.revision)
    model_version = _reference(model.id, model.version_no)
    selection_reference = _reference(selection.id, selection.version_no)
    discovery_dataset = _reference(dataset.id, dataset.revision_no)
    evaluation_design = _reference(design.id, design.version_no)
    return _DiscoveryRequest(
        descriptor={
            "kind": "DISCOVERY_EVALUATION",
            "discovery_evaluation_id": str(discovery.id),
            "source_mission_artifact": _reference_payload(source_artifact),
            "model_version": _reference_payload(model_version),
            "evaluation_dataset_selection": _reference_payload(selection_reference),
            "discovery_dataset": _reference_payload(discovery_dataset),
            "evaluation_design": _reference_payload(evaluation_design),
        },
        discovery_evaluation_id=discovery.id,
        model_version=model_version,
    )


def prepare_discovery_evaluation(session: Session, discovery_id: UUID) -> _DiscoveryRequest:
    """Rebuild the sole Discovery descriptor from frozen Core facts."""
    request = _locked_discovery_request(session, discovery_id)
    discovery = session.get(AlphaDiscoveryEvaluation, discovery_id)
    assert discovery is not None
    discovery.state = "RUNNING"
    return request


def _assignment_datasets(
    session: Session,
    assignment: AlphaEvaluationAssignment,
    discovery: AlphaDiscoveryEvaluation,
    selection: EvaluationDatasetSelection,
) -> dict[str, DatasetRevision]:
    relations = list(
        session.scalars(
            select(AlphaEvaluationAssignmentDatasetRevision)
            .where(AlphaEvaluationAssignmentDatasetRevision.assignment_id == assignment.id)
            .with_for_update()
        )
    )
    expected_ids = {
        "DISCOVERY": discovery.discovery_dataset_revision_id,
        "VALIDATION": selection.validation_dataset_revision_id,
        "SEALED": assignment.sealed_dataset_revision_id,
    }
    if len(relations) != len(expected_ids) or {
        relation.phase: relation.dataset_revision_id for relation in relations
    } != expected_ids or any(relation.ordinal != 1 for relation in relations):
        raise _invalid_frozen_input()
    datasets: dict[str, DatasetRevision] = {}
    for phase, dataset_id in expected_ids.items():
        dataset = session.scalar(
            select(DatasetRevision).where(DatasetRevision.id == dataset_id).with_for_update()
        )
        if dataset is None:
            raise _invalid_frozen_input()
        datasets[phase] = dataset
    if selection.sealed_dataset_revision_id != datasets["SEALED"].id:
        raise _invalid_frozen_input()
    return datasets


def _alpha_context(
    session: Session,
    assignment_id: UUID,
    *,
    allow_finalized: bool,
) -> _AlphaContext:
    assignment = session.scalar(
        select(AlphaEvaluationAssignment)
        .where(AlphaEvaluationAssignment.id == assignment_id)
        .with_for_update()
    )
    if assignment is None:
        raise QfError("ALPHA_EVALUATION_ASSIGNMENT_NOT_FOUND", "Alpha assignment was not found.", 404)
    assignment_states = {"QUEUED", "RUNNING"}
    if allow_finalized:
        assignment_states.add("FINALIZED")
    if assignment.state not in assignment_states:
        raise QfError(
            "ALPHA_EVALUATION_ASSIGNMENT_STATE_CONFLICT",
            "Alpha assignment is not awaiting evaluator work.",
            409,
        )
    episode = session.scalar(
        select(AlphaEvaluationEpisode)
        .where(AlphaEvaluationEpisode.assignment_id == assignment.id)
        .with_for_update()
    )
    discovery = session.scalar(
        select(AlphaDiscoveryEvaluation)
        .where(AlphaDiscoveryEvaluation.id == assignment.discovery_evaluation_id)
        .with_for_update()
    )
    source_artifact = session.scalar(
        select(MissionArtifact)
        .where(MissionArtifact.id == assignment.source_mission_artifact_id)
        .with_for_update()
    )
    model_version = session.scalar(
        select(AlphaModelVersion)
        .where(AlphaModelVersion.id == assignment.alpha_model_version_id)
        .with_for_update()
    )
    design = session.scalar(
        select(EvaluationDesignVersion)
        .where(EvaluationDesignVersion.id == assignment.evaluation_design_version_id)
        .with_for_update()
    )
    policy = session.scalar(
        select(PromotionPolicyVersion)
        .where(PromotionPolicyVersion.id == assignment.promotion_policy_version_id)
        .with_for_update()
    )
    if (
        episode is None
        or discovery is None
        or source_artifact is None
        or model_version is None
        or design is None
        or policy is None
    ):
        raise _invalid_frozen_input()
    episode_states = {"ASSIGNED", "EVALUATING"}
    if allow_finalized:
        episode_states.add("DISCLOSED")
    if episode.state not in episode_states:
        raise QfError(
            "ALPHA_EVALUATION_EPISODE_STATE_CONFLICT",
            "Alpha episode is not awaiting evaluator work.",
            409,
        )
    model = session.scalar(
        select(AlphaModel).where(AlphaModel.id == model_version.alpha_model_id).with_for_update()
    )
    selection = session.scalar(
        select(EvaluationDatasetSelection)
        .where(EvaluationDatasetSelection.id == discovery.evaluation_dataset_selection_id)
        .with_for_update()
    )
    calibration = (
        session.scalar(
            select(AlphaCalibrationVersion)
            .where(AlphaCalibrationVersion.id == assignment.alpha_calibration_version_id)
            .with_for_update()
        )
        if assignment.alpha_calibration_version_id is not None
        else None
    )
    if (
        model is None
        or selection is None
        or discovery.state != "VALID"
        or source_artifact.revision != assignment.source_mission_artifact_revision
        or source_artifact.id != discovery.source_mission_artifact_id
        or source_artifact.revision != discovery.source_mission_artifact_revision
        or model_version.state != "VALIDATED"
        or model_version.source_mission_artifact_id != source_artifact.id
        or model_version.source_mission_artifact_revision != source_artifact.revision
        or discovery.alpha_model_version_id != model_version.id
        or assignment.alpha_model_version_id != model_version.id
        or assignment.evaluation_design_version_id != discovery.evaluation_design_version_id
        or design.universe_version_id != assignment.universe_version_id
        or design.allowed_model_mode != model_version.mode
        or design.contract_version != assignment.evaluator_contract_version
        or policy.purpose != "SEALED_TO_QUALIFIED"
        or selection.discovery_dataset_revision_id != discovery.discovery_dataset_revision_id
        or episode.alpha_model_version_id != model_version.id
        or episode.sealed_dataset_revision_id != assignment.sealed_dataset_revision_id
        or episode.promotion_policy_version_id != policy.id
        or episode.program_id != assignment.program_id
        or episode.branch_id != assignment.branch_id
        or model.owner_program_id != assignment.program_id
    ):
        raise _invalid_frozen_input()
    if calibration is not None and (
        calibration.alpha_model_version_id != model_version.id
        or calibration.source_discovery_evaluation_id != discovery.id
        or calibration.training_dataset_revision_id != discovery.discovery_dataset_revision_id
        or calibration.private_artifact_ref is None
        or calibration.artifact_uri is not None
        or calibration.state != "VALIDATED"
    ):
        raise _invalid_frozen_input()
    datasets = _assignment_datasets(session, assignment, discovery, selection)
    source_reference = _reference(source_artifact.id, source_artifact.revision)
    model_reference = _reference(model_version.id, model_version.version_no)
    calibration_reference = (
        _reference(calibration.id, calibration.version_no) if calibration is not None else None
    )
    input = AlphaEvaluationInput(
        assignment_id=assignment.id,
        episode_id=episode.id,
        source_mission_artifact=source_reference,
        model_version=model_reference,
        calibration_version=calibration_reference,
        discovery_dataset=_reference(datasets["DISCOVERY"].id, datasets["DISCOVERY"].revision_no),
        validation_dataset=_reference(datasets["VALIDATION"].id, datasets["VALIDATION"].revision_no),
        sealed_dataset=_reference(datasets["SEALED"].id, datasets["SEALED"].revision_no),
        evaluation_design=_reference(design.id, design.version_no),
        promotion_policy=_reference(policy.id, policy.version_no),
    )
    return _AlphaContext(
        assignment=assignment,
        episode=episode,
        discovery=discovery,
        source_artifact=source_artifact,
        model=model,
        model_version=model_version,
        calibration=calibration,
        datasets=datasets,
        selection=selection,
        design=design,
        policy=policy,
        input=input,
    )


def prepare_alpha_evaluation(session: Session, assignment_id: UUID) -> _AlphaRequest:
    """Rebuild the sole sealed Alpha descriptor from frozen Core facts."""
    context = _alpha_context(session, assignment_id, allow_finalized=False)
    context.assignment.state = "RUNNING"
    context.episode.state = "EVALUATING"
    input = context.input
    return _AlphaRequest(
        descriptor={
            "kind": "ALPHA_EVALUATION",
            "assignment_id": str(input.assignment_id),
            "episode_id": str(input.episode_id),
            "discovery_evaluation_id": str(context.discovery.id),
            "source_mission_artifact": _reference_payload(input.source_mission_artifact),
            "model_version": _reference_payload(input.model_version),
            "calibration_version": (
                _reference_payload(input.calibration_version)
                if input.calibration_version is not None
                else None
            ),
            "discovery_dataset": _reference_payload(input.discovery_dataset),
            "validation_dataset": _reference_payload(input.validation_dataset),
            "sealed_dataset": _reference_payload(input.sealed_dataset),
            "evaluation_design": _reference_payload(input.evaluation_design),
            "promotion_policy": _reference_payload(input.promotion_policy),
        },
        input=input,
    )


def prepare_portfolio_input_evaluation(
    session: Session, assignment_id: UUID
) -> _PortfolioInputRequest:
    """Rebuild a covariance descriptor from the frozen relational Input Assignment."""
    input_value = prepare_portfolio_input_facts(session, assignment_id)
    return _PortfolioInputRequest(
        descriptor={
            "kind": "PORTFOLIO_INPUT_EVALUATION",
            "assignment_id": str(input_value.assignment_id),
            "portfolio_program_id": str(input_value.portfolio_program_id),
            "mandate_version": _reference_payload(input_value.mandate_version),
            "capital_context_version_id": str(input_value.capital_context_version_id),
            "evaluation_dataset_selection": _reference_payload(
                input_value.evaluation_dataset_selection
            ),
            "sealed_dataset": _reference_payload(input_value.sealed_dataset),
            "promotion_policy": _reference_payload(input_value.promotion_policy),
            "cause_event_id": input_value.cause_event_id,
            "previous_candidate_id": (
                str(input_value.previous_candidate_id)
                if input_value.previous_candidate_id is not None
                else None
            ),
            "axes": [
                {
                    "alpha_qualification_id": str(axis.alpha_qualification_id),
                    "alpha_evaluation_result_id": str(axis.alpha_evaluation_result_id),
                    "alpha_signal_artifact_id": str(axis.alpha_signal_artifact_id),
                }
                for axis in input_value.axes
            ],
        },
        input=input_value,
    )


def prepare_portfolio_evaluation_request(
    session: Session, assignment_id: UUID
) -> _PortfolioEvaluationRequest:
    """Rebuild the Portfolio evidence descriptor from frozen Core facts."""
    input_value = prepare_portfolio_evaluation_facts(session, assignment_id)
    return _PortfolioEvaluationRequest(
        descriptor={
            "kind": "PORTFOLIO_EVALUATION",
            "assignment_id": str(input_value.assignment_id),
            "episode_id": str(input_value.episode_id),
            "candidate_id": str(input_value.candidate_id),
            "candidate_family_id": str(input_value.candidate_family_id),
            "previous_candidate_id": (
                str(input_value.previous_candidate_id)
                if input_value.previous_candidate_id is not None
                else None
            ),
            "assembly_input_id": str(input_value.assembly_input_id),
            "evaluation_dataset_selection": _reference_payload(
                input_value.evaluation_dataset_selection
            ),
            "sealed_dataset": _reference_payload(input_value.sealed_dataset),
            "policy_version": _reference_payload(input_value.policy_version),
            "cause_event_id": input_value.cause_event_id,
        },
        input=input_value,
    )


def trusted_evaluator_assignment_running(
    session: Session, *, kind: str, resource_id: UUID
) -> bool:
    """Identify failures after a descriptor transaction has admitted the frozen fact."""
    models = {
        "DISCOVERY_EVALUATION": AlphaDiscoveryEvaluation,
        "ALPHA_EVALUATION": AlphaEvaluationAssignment,
        "PORTFOLIO_INPUT_EVALUATION": PortfolioInputEvaluationAssignment,
        "PORTFOLIO_EVALUATION": PortfolioEvaluationAssignment,
    }
    model: Any = models.get(kind)
    if model is None:
        return False
    row = session.scalar(select(model).where(model.id == resource_id).with_for_update())
    return row is not None and row.state == "RUNNING"


def terminalize_trusted_evaluator_failure(
    session: Session, *, kind: str, resource_id: UUID, outcome_code: str
) -> bool:
    """Close a fenced evaluator assignment after its bounded retries are spent."""
    models = {
        "DISCOVERY_EVALUATION": AlphaDiscoveryEvaluation,
        "ALPHA_EVALUATION": AlphaEvaluationAssignment,
        "PORTFOLIO_INPUT_EVALUATION": PortfolioInputEvaluationAssignment,
        "PORTFOLIO_EVALUATION": PortfolioEvaluationAssignment,
    }
    model: Any = models.get(kind)
    if model is None:
        return False
    row = session.scalar(select(model).where(model.id == resource_id).with_for_update())
    if row is None or row.state != "RUNNING":
        return False
    completed_at = datetime.now(UTC)
    code = outcome_code.strip()[:100] or "TRUSTED_EVALUATOR_FAILED"
    if kind == "ALPHA_EVALUATION":
        row.state = "INVALIDATED"
        episode = session.scalar(
            select(AlphaEvaluationEpisode)
            .where(AlphaEvaluationEpisode.assignment_id == row.id)
            .with_for_update()
        )
        if episode is not None and episode.state in {"ASSIGNED", "EVALUATING"}:
            episode.state = "INVALIDATED"
            episode.result = "INVALID"
    elif kind == "PORTFOLIO_EVALUATION":
        row.state = "FAILED"
        row.outcome = "RETRIES_EXHAUSTED"
        row.completed_at = completed_at
        portfolio_episode = session.scalar(
            select(PortfolioEvaluationEpisode)
            .where(PortfolioEvaluationEpisode.assignment_id == row.id)
            .with_for_update()
        )
        if portfolio_episode is not None and portfolio_episode.state in {"ASSIGNED", "EVALUATING"}:
            portfolio_episode.state = "DISCLOSED"
            portfolio_episode.result = "INVALID"
            portfolio_episode.evaluated_at = completed_at
            portfolio_episode.disclosed_at = completed_at
    else:
        row.state = "FAILED"
        row.outcome_code = code
        row.completed_at = completed_at
    session.flush()
    return True


def _write_descriptor(root: Path, descriptor: Mapping[str, object]) -> Path:
    path = root / "descriptor.json"
    data = json.dumps(descriptor, sort_keys=True, separators=(",", ":")).encode("utf-8")
    descriptor_fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(descriptor_fd, "wb") as descriptor_file:
            descriptor_file.write(data)
            descriptor_file.flush()
            os.fsync(descriptor_file.fileno())
    except BaseException:
        try:
            os.close(descriptor_fd)
        except OSError:
            pass
        raise
    os.chmod(path, 0o600)
    return path


def _evaluator_command(settings: Settings) -> Path:
    command = settings.trusted_evaluator_command
    try:
        valid = (
            isinstance(command, Path)
            and command.is_absolute()
            and command.is_file()
            and os.access(command, os.X_OK)
            and stat.S_ISREG(command.stat().st_mode)
        )
    except OSError:
        valid = False
    if not valid:
        raise QfError(
            "TRUSTED_EVALUATOR_UNAVAILABLE",
            "Trusted evaluator is not configured or executable.",
            503,
        )
    assert isinstance(command, Path)
    return command


def ensure_trusted_evaluator_available(settings: Settings) -> None:
    """Check the one bootstrap command without exposing its operator-owned path."""
    _evaluator_command(settings)


def _terminate_evaluator(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    try:
        process.terminate()
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def _bounded_stdout(process: subprocess.Popen[bytes], timeout: float) -> bytes:
    stdout = process.stdout
    if stdout is None:
        _terminate_evaluator(process)
        raise _invalid_result()
    output = bytearray()
    deadline = time.monotonic() + timeout
    try:
        os.set_blocking(stdout.fileno(), False)
        with selectors.DefaultSelector() as selector:
            selector.register(stdout, selectors.EVENT_READ)
            while selector.get_map():
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    _terminate_evaluator(process)
                    raise QfError(
                        "TRUSTED_EVALUATOR_FAILED",
                        "Trusted evaluator did not produce a result.",
                        502,
                    )
                events = selector.select(timeout=min(0.1, remaining))
                if not events:
                    # A finished direct evaluator with an open stdout pipe has
                    # left a descendant behind.  Do not wait for the full job
                    # timeout: returning nonzero lets the finite worker clear
                    # its isolated process group immediately.
                    if process.poll() is not None:
                        _terminate_evaluator(process)
                        raise QfError(
                            "TRUSTED_EVALUATOR_FAILED",
                            "Trusted evaluator did not produce a result.",
                            502,
                        )
                    continue
                for key, _ in events:
                    try:
                        chunk = os.read(key.fd, 65_536)
                    except BlockingIOError:
                        continue
                    if not chunk:
                        selector.unregister(key.fileobj)
                    elif len(output) + len(chunk) > _MAX_RESULT_BYTES:
                        _terminate_evaluator(process)
                        raise _invalid_result()
                    else:
                        output.extend(chunk)
    except OSError as exc:
        _terminate_evaluator(process)
        raise QfError(
            "TRUSTED_EVALUATOR_FAILED",
            "Trusted evaluator did not produce a result.",
            502,
        ) from exc
    finally:
        stdout.close()
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        _terminate_evaluator(process)
        raise QfError(
            "TRUSTED_EVALUATOR_FAILED",
            "Trusted evaluator did not produce a result.",
            502,
        )
    try:
        process.wait(timeout=remaining)
    except subprocess.TimeoutExpired as exc:
        _terminate_evaluator(process)
        raise QfError(
            "TRUSTED_EVALUATOR_FAILED",
            "Trusted evaluator did not produce a result.",
            502,
        ) from exc
    if process.returncode != 0:
        raise QfError(
            "TRUSTED_EVALUATOR_FAILED",
            "Trusted evaluator did not produce a result.",
            502,
        )
    return bytes(output)


def run_trusted_evaluator(settings: Settings, descriptor: Mapping[str, object]) -> bytes:
    """Run the one fixed evaluator command without QZ credentials or raw output logging."""
    command = _evaluator_command(settings)
    timeout = float(settings.mission_job_timeout_seconds)
    if timeout <= 0:
        raise QfError("TRUSTED_EVALUATOR_UNAVAILABLE", "Trusted evaluator is unavailable.", 503)
    with tempfile.TemporaryDirectory(prefix="quazonai-evaluator-") as temporary_root:
        root = Path(temporary_root)
        os.chmod(root, 0o700)
        descriptor_path = _write_descriptor(root, descriptor)
        try:
            process = subprocess.Popen(
                [str(command), str(descriptor_path)],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                cwd=root,
                env=_EVALUATOR_ENV,
            )
            output = _bounded_stdout(process, timeout)
        except OSError as exc:
            raise QfError(
                "TRUSTED_EVALUATOR_FAILED",
                "Trusted evaluator did not produce a result.",
                502,
            ) from exc
    return output


def _reject_json_constant(_: str) -> None:
    raise ValueError("non-finite JSON value")


def _unique_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def _json_object(payload: bytes, fields: frozenset[str]) -> dict[str, object]:
    value = json.loads(
        payload.decode("utf-8"),
        object_pairs_hook=_unique_object,
        parse_constant=_reject_json_constant,
    )
    if not isinstance(value, dict) or set(value) != fields:
        raise ValueError("unexpected evaluator result fields")
    return value


def _object(value: object, fields: frozenset[str]) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != fields:
        raise ValueError("unexpected evaluator object fields")
    return value


def _items(value: object) -> Sequence[object]:
    if not isinstance(value, list):
        raise ValueError("evaluator sequence must be an array")
    return value


def _uuid(value: object) -> UUID:
    if not isinstance(value, str):
        raise TypeError("evaluator UUID must be text")
    return UUID(value)


def _text(value: object) -> str:
    if not isinstance(value, str):
        raise TypeError("evaluator text value is invalid")
    return value


def _integer(value: object) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError("evaluator integer value is invalid")
    return value


def _number(value: object) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError("evaluator numeric value is invalid")
    return float(value)


def _number_or_none(value: object) -> float | None:
    return None if value is None else _number(value)


def _datetime(value: object) -> datetime:
    if not isinstance(value, str):
        raise TypeError("evaluator datetime must be text")
    return datetime.fromisoformat(value)


def _result_reference(value: object) -> ImmutableReference:
    item = _object(value, frozenset({"id", "revision"}))
    return ImmutableReference(_uuid(item["id"]), _integer(item["revision"]))


def _metric(value: object) -> MetricAggregate:
    item = _object(value, frozenset({"phase", "code", "status", "value"}))
    return MetricAggregate(
        EvaluationPhase(_text(item["phase"])),
        MetricCode(_text(item["code"])),
        MetricStatus(_text(item["status"])),
        _number_or_none(item["value"]),
    )


def _gate(value: object) -> GateResult:
    item = _object(value, frozenset({"code", "status", "reason_code"}))
    reason_code = (
        DisclosureReasonCode(_text(item["reason_code"]))
        if item["reason_code"] is not None
        else None
    )
    return GateResult(
        GateCode(_text(item["code"])),
        GateStatus(_text(item["status"])),
        reason_code,
    )


def _disclosure(value: object) -> LevelOneDisclosure:
    item = _object(value, frozenset({"classification", "reason_code"}))
    reason_code = (
        DisclosureReasonCode(_text(item["reason_code"]))
        if item["reason_code"] is not None
        else None
    )
    return LevelOneDisclosure(
        DisclosureClassification(_text(item["classification"])),
        reason_code,
    )


def _calibration(value: object) -> DiscoveryCalibrationArtifact | None:
    if value is None:
        return None
    item = _object(value, frozenset({"method", "training_dataset", "private_artifact_ref"}))
    return DiscoveryCalibrationArtifact(
        CalibrationMethod(_text(item["method"])),
        _result_reference(item["training_dataset"]),
        _uuid(item["private_artifact_ref"]),
    )


def _signal(value: object) -> AlphaSignalSummary:
    item = _object(
        value,
        frozenset(
            {"row_count", "event_start", "event_end", "available_start", "available_end"}
        ),
    )
    return AlphaSignalSummary(
        _integer(item["row_count"]),
        _datetime(item["event_start"]),
        _datetime(item["event_end"]),
        _datetime(item["available_start"]),
        _datetime(item["available_end"]),
    )


def _forecast(value: object) -> AlphaForecast:
    item = _object(
        value,
        frozenset(
            {
                "instrument_id",
                "as_of_time",
                "effective_from",
                "effective_until",
                "expected_return",
                "uncertainty",
                "confidence",
                "max_trade_notional",
                "max_position_notional",
                "max_participation_rate",
                "days_to_liquidate",
                "stressed_capacity_notional",
            }
        ),
    )
    effective_until = (
        _datetime(item["effective_until"]) if item["effective_until"] is not None else None
    )
    return AlphaForecast(
        _text(item["instrument_id"]),
        _datetime(item["as_of_time"]),
        _datetime(item["effective_from"]),
        effective_until,
        _number(item["expected_return"]),
        _number(item["uncertainty"]),
        _number(item["confidence"]),
        _number(item["max_trade_notional"]),
        _number(item["max_position_notional"]),
        _number(item["max_participation_rate"]),
        _number(item["days_to_liquidate"]),
        _number(item["stressed_capacity_notional"]),
    )


def _invalid_result() -> QfError:
    return QfError(
        "TRUSTED_EVALUATOR_RESULT_INVALID",
        "Trusted evaluator returned an invalid result.",
        422,
    )


def parse_discovery_evaluation_result(
    payload: bytes,
    request: _DiscoveryRequest,
) -> DiscoveryEvaluationResult:
    try:
        item = _json_object(
            payload,
            frozenset(
                {
                    "kind",
                    "status",
                    "private_result_id",
                    "evaluated_at",
                    "metrics",
                    "gates",
                    "calibration",
                }
            ),
        )
        if item["kind"] != "DISCOVERY_EVALUATION":
            raise ValueError("unexpected evaluator kind")
        return DiscoveryEvaluationResult(
            discovery_evaluation_id=request.discovery_evaluation_id,
            model_version=request.model_version,
            status=DiscoveryEvaluationStatus(_text(item["status"])),
            private_result_id=_uuid(item["private_result_id"]),
            evaluated_at=_datetime(item["evaluated_at"]),
            metrics=tuple(_metric(metric) for metric in _items(item["metrics"])),
            gates=tuple(_gate(gate) for gate in _items(item["gates"])),
            calibration=_calibration(item["calibration"]),
        )
    except (TypeError, ValueError, UnicodeDecodeError) as exc:
        raise _invalid_result() from exc


def parse_alpha_evaluation_result(payload: bytes, request: _AlphaRequest) -> SealedEvaluationResult:
    try:
        item = _json_object(
            payload,
            frozenset(
                {
                    "kind",
                    "status",
                    "private_result_id",
                    "evaluated_at",
                    "metrics",
                    "gates",
                    "disclosure",
                    "signal",
                    "forecasts",
                }
            ),
        )
        if item["kind"] != "ALPHA_EVALUATION":
            raise ValueError("unexpected evaluator kind")
        return SealedEvaluationResult(
            input=request.input,
            status=EvaluationStatus(_text(item["status"])),
            private_result_id=_uuid(item["private_result_id"]),
            evaluated_at=_datetime(item["evaluated_at"]),
            metrics=tuple(_metric(metric) for metric in _items(item["metrics"])),
            gates=tuple(_gate(gate) for gate in _items(item["gates"])),
            disclosure=_disclosure(item["disclosure"]),
            signal=_signal(item["signal"]) if item["signal"] is not None else None,
            forecasts=tuple(_forecast(forecast) for forecast in _items(item["forecasts"])),
        )
    except (TypeError, ValueError, UnicodeDecodeError) as exc:
        raise _invalid_result() from exc


def _portfolio_covariance(value: object) -> PortfolioCovariance:
    item = _object(value, frozenset({"left_axis_index", "right_axis_index", "covariance"}))
    return PortfolioCovariance(
        _integer(item["left_axis_index"]),
        _integer(item["right_axis_index"]),
        _number(item["covariance"]),
    )


def parse_portfolio_input_evaluation_result(
    payload: bytes,
    request: _PortfolioInputRequest,
) -> PortfolioInputEvaluationResult:
    """Parse only typed covariance facts for the exact frozen Input assignment."""
    try:
        item = _json_object(
            payload,
            frozenset(
                {
                    "kind",
                    "assignment_id",
                    "private_result_id",
                    "evaluated_at",
                    "covariance_method",
                    "covariance_observations",
                    "covariance_decay",
                    "covariance_shrinkage",
                    "covariance_upper_triangle",
                }
            ),
        )
        if item["kind"] != "PORTFOLIO_INPUT_EVALUATION":
            raise ValueError("unexpected evaluator kind")
        if _uuid(item["assignment_id"]) != request.input.assignment_id:
            raise ValueError("unexpected evaluator assignment")
        return PortfolioInputEvaluationResult(
            input=request.input,
            private_result_id=_uuid(item["private_result_id"]),
            evaluated_at=_datetime(item["evaluated_at"]),
            covariance_method=PortfolioCovarianceMethod(_text(item["covariance_method"])),
            covariance_observations=_integer(item["covariance_observations"]),
            covariance_decay=_number(item["covariance_decay"]),
            covariance_shrinkage=_number(item["covariance_shrinkage"]),
            covariance_upper_triangle=tuple(
                _portfolio_covariance(value) for value in _items(item["covariance_upper_triangle"])
            ),
        )
    except (TypeError, ValueError, UnicodeDecodeError) as exc:
        raise _invalid_result() from exc


def parse_portfolio_evaluation_result(
    payload: bytes,
    request: _PortfolioEvaluationRequest,
) -> SealedEvaluationResult:
    """Parse the fixed Portfolio metric/gate set for one frozen episode."""
    try:
        item = _json_object(
            payload,
            frozenset(
                {
                    "kind",
                    "status",
                    "private_result_id",
                    "evaluated_at",
                    "metrics",
                    "gates",
                    "disclosure",
                }
            ),
        )
        if item["kind"] != "PORTFOLIO_EVALUATION":
            raise ValueError("unexpected evaluator kind")
        return SealedEvaluationResult(
            input=request.input,
            status=EvaluationStatus(_text(item["status"])),
            private_result_id=_uuid(item["private_result_id"]),
            evaluated_at=_datetime(item["evaluated_at"]),
            metrics=tuple(_metric(metric) for metric in _items(item["metrics"])),
            gates=tuple(_gate(gate) for gate in _items(item["gates"])),
            disclosure=_disclosure(item["disclosure"]),
        )
    except (TypeError, ValueError, UnicodeDecodeError) as exc:
        raise _invalid_result() from exc


def _evidence_validity(status: EvaluationStatus) -> str:
    if status in {EvaluationStatus.PASS, EvaluationStatus.FAIL}:
        return "VALID"
    if status is EvaluationStatus.INCONCLUSIVE:
        return "INCONCLUSIVE"
    return "INVALID"


def _stored_utc(value: datetime | None) -> datetime | None:
    if value is None:
        return None
    return value.replace(tzinfo=UTC) if value.tzinfo is None else value.astimezone(UTC)


def _signal_is_within_dataset(signal: AlphaSignalSummary, dataset: DatasetRevision) -> bool:
    event_start = _stored_utc(dataset.event_start)
    event_end = _stored_utc(dataset.event_end)
    available_start = _stored_utc(dataset.available_start)
    available_end = _stored_utc(dataset.available_end)
    return (
        event_start is not None
        and event_end is not None
        and available_start is not None
        and available_end is not None
        and event_start <= signal.event_start <= signal.event_end <= event_end
        and available_start <= signal.available_start <= signal.available_end <= available_end
    )


def _policy_passes(
    session: Session,
    policy: PromotionPolicyVersion,
    result: SealedEvaluationResult,
) -> bool:
    gates = list(
        session.scalars(
            select(PromotionPolicyGate)
            .where(PromotionPolicyGate.policy_version_id == policy.id)
            .order_by(PromotionPolicyGate.ordinal)
            .with_for_update()
        )
    )
    if not gates or [gate.ordinal for gate in gates] != list(range(1, len(gates) + 1)):
        return False
    metrics = {metric.code.value: metric for metric in result.metrics}
    for gate in gates:
        metric = metrics.get(gate.metric_code)
        if metric is None or metric.status is not MetricStatus.AVAILABLE or metric.value is None:
            return False
        value = Decimal(str(metric.value))
        if (gate.comparator == "MINIMUM" and value < gate.threshold) or (
            gate.comparator == "MAXIMUM" and value > gate.threshold
        ):
            return False
    return True


def _qualification_passes(context: _AlphaContext, result: SealedEvaluationResult, session: Session) -> None:
    if context.calibration is None:
        raise QfError(
            "ALPHA_EVALUATION_CALIBRATION_INVALID",
            "A passing Alpha evaluation requires frozen calibration provenance.",
            409,
        )
    if not all(gate.status is GateStatus.PASS for gate in result.gates):
        raise QfError("ALPHA_EVALUATION_GATE_INVALID", "Alpha evaluation gates did not pass.", 409)
    metric = next(
        (
            item
            for item in result.metrics
            if item.phase is EvaluationPhase.SEALED
            and item.code.value == context.design.qualification_metric_code
        ),
        None,
    )
    if metric is None or metric.status is not MetricStatus.AVAILABLE or metric.value is None:
        raise QfError(
            "ALPHA_EVALUATION_QUALIFICATION_METRIC_INVALID",
            "The frozen qualification metric is unavailable.",
            409,
        )
    metric_value = Decimal(str(metric.value))
    threshold = context.design.qualification_threshold
    comparator = context.design.qualification_comparator
    if (comparator == "MINIMUM" and metric_value < threshold) or (
        comparator == "MAXIMUM" and metric_value > threshold
    ):
        raise QfError(
            "ALPHA_EVALUATION_QUALIFICATION_METRIC_INVALID",
            "The frozen qualification threshold did not pass.",
            409,
        )
    if not _policy_passes(session, context.policy, result):
        raise QfError(
            "ALPHA_EVALUATION_POLICY_INVALID",
            "The frozen promotion policy did not pass.",
            409,
        )


def _disclosure_for(
    context: _AlphaContext,
    result: SealedEvaluationResult,
) -> tuple[str, str | None]:
    if result.status is EvaluationStatus.PASS:
        if context.design.pass_disclosure_code != DisclosureClassification.QUALIFIED.value:
            raise QfError(
                "ALPHA_EVALUATION_DISCLOSURE_POLICY_INVALID",
                "The frozen pass disclosure policy is invalid.",
                409,
            )
        return DisclosureClassification.QUALIFIED.value, None
    reason_by_status = {
        EvaluationStatus.FAIL: context.design.failure_disclosure_code,
        EvaluationStatus.INCONCLUSIVE: context.design.inconclusive_disclosure_code,
        EvaluationStatus.INVALID: context.design.invalid_disclosure_code,
    }
    expected_reason = reason_by_status[result.status]
    first_nonpass = next(gate for gate in result.gates if gate.status is not GateStatus.PASS)
    if result.disclosure.reason_code is not first_nonpass.reason_code:
        raise QfError(
            "ALPHA_EVALUATION_DISCLOSURE_INVALID",
            "Evaluator disclosure does not match its categorical gates.",
            409,
        )
    return result.disclosure.classification.value, expected_reason


def _create_signal(
    session: Session,
    *,
    context: _AlphaContext,
    result: SealedEvaluationResult,
    persisted: AlphaEvaluationResult,
) -> AlphaSignalArtifact:
    signal = result.signal
    assert signal is not None
    sealed_dataset = context.datasets["SEALED"]
    if not _signal_is_within_dataset(signal, sealed_dataset):
        raise QfError(
            "ALPHA_SIGNAL_SUMMARY_INVALID",
            "Alpha signal summary is outside its frozen sealed Dataset.",
            409,
        )
    catalog = session.scalar(
        select(NautilusCatalogBinding)
        .where(NautilusCatalogBinding.dataset_revision_id == sealed_dataset.id)
        .with_for_update()
    )
    if (
        catalog is None
        or not catalog.sealed
        or catalog.quality_state != "VALID"
        or catalog.point_in_time_state != "VALID"
        or signal is None
        or any(
            forecast.instrument_id not in set(catalog.instrument_scope)
            for forecast in result.forecasts
        )
    ):
        raise QfError(
            "ALPHA_SIGNAL_CATALOG_BINDING_INVALID",
            "Alpha forecasts must stay within the frozen sealed catalog scope.",
            409,
        )
    artifact = AlphaSignalArtifact(
        alpha_model_version_id=context.model_version.id,
        dataset_revision_id=sealed_dataset.id,
        run_id=None,
        evaluation_result_id=persisted.id,
        mode="CALIBRATED_RETURN" if context.calibration is not None else context.model_version.mode,
        artifact_uri=f"evaluator-private://alpha-result/{result.private_result_id}/signal",
        row_count=signal.row_count,
        event_start=signal.event_start,
        event_end=signal.event_end,
        available_start=signal.available_start,
        available_end=signal.available_end,
        schema_version="AlphaSignalFrameV1",
    )
    session.add(artifact)
    session.flush()
    return artifact


def _create_qualification(
    session: Session,
    *,
    context: _AlphaContext,
    persisted: AlphaEvaluationResult,
) -> AlphaQualification:
    existing = session.scalar(
        select(AlphaQualification)
        .where(AlphaQualification.evaluation_result_id == persisted.id)
        .with_for_update()
    )
    if existing is not None:
        return existing
    role = context.design.qualification_role
    qualification = AlphaQualification(
        program_id=context.assignment.program_id,
        alpha_model_id=context.model.id,
        alpha_model_version_id=context.model_version.id,
        calibration_version_id=context.calibration.id if context.calibration is not None else None,
        universe_version_id=context.assignment.universe_version_id,
        universe=context.datasets["SEALED"].universe_name,
        horizon=context.model_version.horizon,
        role=role,
        state="SHADOW" if role == "SHADOW_ALPHA" else "ACTIVE",
        name=context.model.name,
        scope_json={},
        evaluation_episode_id=context.episode.id,
        evaluation_result_id=persisted.id,
        degradation_state="HEALTHY",
        metrics={},
        lineage=[],
    )
    session.add(qualification)
    session.flush()
    return qualification


def _expose(
    session: Session,
    *,
    context: _AlphaContext,
    qualification: AlphaQualification | None,
) -> None:
    subjects = [
        ("PROGRAM", context.assignment.program_id),
        ("BRANCH", context.assignment.branch_id),
        ("MISSION", context.assignment.mission_id),
        ("ALPHA_MODEL", context.model.id),
    ]
    if qualification is not None:
        subjects.append(("ALPHA_QUALIFICATION", qualification.id))
    session.add_all(
        EvidenceExposure(
            episode_id=context.episode.id,
            subject_type=subject_type,
            subject_id=subject_id,
            level=1,
        )
        for subject_type, subject_id in subjects
    )


def _stored_metrics(session: Session, result_id: UUID) -> tuple[tuple[str, str, Decimal | None, str], ...]:
    return tuple(
        (row.phase, row.metric_code, row.value, row.status)
        for row in session.scalars(
            select(AlphaEvaluationMetric)
            .where(AlphaEvaluationMetric.result_id == result_id)
            .order_by(AlphaEvaluationMetric.phase, AlphaEvaluationMetric.metric_code)
        )
    )


def _stored_gates(session: Session, result_id: UUID) -> tuple[tuple[str, str, str | None], ...]:
    return tuple(
        (row.gate_code, row.status, row.reason_code)
        for row in session.scalars(
            select(AlphaEvaluationGate)
            .where(AlphaEvaluationGate.result_id == result_id)
            .order_by(AlphaEvaluationGate.gate_code)
        )
    )


def _matches_existing(
    session: Session,
    *,
    context: _AlphaContext,
    persisted: AlphaEvaluationResult,
    result: SealedEvaluationResult,
) -> bool:
    if (
        persisted.evidence_validity != _evidence_validity(result.status)
        or persisted.result != result.status.value
        or persisted.private_result_ref != result.private_result_id
        or _stored_utc(persisted.evaluated_at) != result.evaluated_at
    ):
        return False
    expected_metrics = tuple(
        (metric.phase.value, metric.code.value, Decimal(str(metric.value)) if metric.value is not None else None, metric.status.value)
        for metric in result.metrics
    )
    expected_gates = tuple(
        (gate.code.value, gate.status.value, gate.reason_code.value if gate.reason_code else None)
        for gate in result.gates
    )
    if _stored_metrics(session, persisted.id) != expected_metrics or _stored_gates(session, persisted.id) != expected_gates:
        return False
    disclosure = session.scalar(
        select(Disclosure).where(
            Disclosure.episode_id == context.episode.id,
            Disclosure.audience == "CODEX",
            Disclosure.level == 1,
        )
    )
    expected_classification, expected_reason = _disclosure_for(context, result)
    if (
        disclosure is None
        or disclosure.classification_code != expected_classification
        or disclosure.reason_code != expected_reason
    ):
        return False
    if result.status is not EvaluationStatus.PASS:
        return session.scalar(
            select(AlphaSignalArtifact).where(AlphaSignalArtifact.evaluation_result_id == persisted.id)
        ) is None
    signal = result.signal
    assert signal is not None
    artifact = session.scalar(
        select(AlphaSignalArtifact).where(AlphaSignalArtifact.evaluation_result_id == persisted.id)
    )
    if (
        artifact is None
        or artifact.run_id is not None
        or artifact.artifact_uri
        != f"evaluator-private://alpha-result/{result.private_result_id}/signal"
        or artifact.row_count != signal.row_count
        or _stored_utc(artifact.event_start) != signal.event_start
        or _stored_utc(artifact.event_end) != signal.event_end
        or _stored_utc(artifact.available_start) != signal.available_start
        or _stored_utc(artifact.available_end) != signal.available_end
    ):
        return False
    forecasts = tuple(
        session.scalars(
            select(AlphaEvaluationForecastRow)
            .where(AlphaEvaluationForecastRow.result_id == persisted.id)
            .order_by(AlphaEvaluationForecastRow.instrument_id)
        )
    )
    if len(forecasts) != 1:
        return False
    forecast = result.forecasts[0]
    row = forecasts[0]
    return (
        row.signal_artifact_id == artifact.id
        and row.instrument_id == forecast.instrument_id
        and _stored_utc(row.as_of_time) == forecast.as_of_time
        and _stored_utc(row.effective_from) == forecast.effective_from
        and _stored_utc(row.effective_until) == forecast.effective_until
        and row.expected_return == Decimal(str(forecast.expected_return))
        and row.uncertainty == Decimal(str(forecast.uncertainty))
        and row.confidence == Decimal(str(forecast.confidence))
        and row.max_trade_notional == Decimal(str(forecast.max_trade_notional))
        and row.max_position_notional == Decimal(str(forecast.max_position_notional))
        and row.max_participation_rate == Decimal(str(forecast.max_participation_rate))
        and row.days_to_liquidate == Decimal(str(forecast.days_to_liquidate))
        and row.stressed_capacity_notional == Decimal(str(forecast.stressed_capacity_notional))
        and session.scalar(
            select(AlphaQualification).where(AlphaQualification.evaluation_result_id == persisted.id)
        )
        is not None
    )


def accept_alpha_evaluation_result(
    session: Session,
    result: SealedEvaluationResult,
) -> AlphaEvaluationResult:
    """Persist one typed sealed result; only the frozen Core context can qualify it."""
    if not isinstance(result, SealedEvaluationResult) or not isinstance(
        result.input, AlphaEvaluationInput
    ):
        raise QfError(
            "ALPHA_EVALUATION_RESULT_REQUIRED",
            "Alpha evaluation requires a typed sealed evaluator result.",
            422,
        )
    context = _alpha_context(session, result.input.assignment_id, allow_finalized=True)
    if result.input != context.input:
        raise QfError(
            "ALPHA_EVALUATION_INPUT_MISMATCH",
            "Alpha evaluator result does not match frozen assignment inputs.",
            409,
        )
    existing = session.scalar(
        select(AlphaEvaluationResult)
        .where(AlphaEvaluationResult.episode_id == context.episode.id)
        .with_for_update()
    )
    if existing is not None:
        if _matches_existing(session, context=context, persisted=existing, result=result):
            return existing
        raise QfError(
            "ALPHA_EVALUATION_RESULT_CONFLICT",
            "Alpha evaluation already has different immutable result facts.",
            409,
        )
    if context.assignment.state not in {"QUEUED", "RUNNING"} or context.episode.state not in {
        "ASSIGNED",
        "EVALUATING",
    }:
        raise QfError(
            "ALPHA_EVALUATION_STATE_CONFLICT",
            "Alpha evaluation is not awaiting a result.",
            409,
        )
    if result.status is EvaluationStatus.PASS:
        _qualification_passes(context, result, session)
    classification_code, reason_code = _disclosure_for(context, result)
    persisted = AlphaEvaluationResult(
        episode_id=context.episode.id,
        evidence_validity=_evidence_validity(result.status),
        result=result.status.value,
        private_result_ref=result.private_result_id,
        evaluated_at=result.evaluated_at,
    )
    session.add(persisted)
    session.flush()
    session.add_all(
        AlphaEvaluationMetric(
            result_id=persisted.id,
            metric_code=metric.code.value,
            phase=metric.phase.value,
            value=Decimal(str(metric.value)) if metric.value is not None else None,
            status=metric.status.value,
        )
        for metric in result.metrics
    )
    session.add_all(
        AlphaEvaluationGate(
            result_id=persisted.id,
            gate_code=gate.code.value,
            status=gate.status.value,
            reason_code=gate.reason_code.value if gate.reason_code is not None else None,
        )
        for gate in result.gates
    )
    qualification: AlphaQualification | None = None
    if result.status is EvaluationStatus.PASS:
        artifact = _create_signal(session, context=context, result=result, persisted=persisted)
        session.add_all(
            AlphaEvaluationForecastRow(
                result_id=persisted.id,
                signal_artifact_id=artifact.id,
                instrument_id=forecast.instrument_id,
                as_of_time=forecast.as_of_time,
                effective_from=forecast.effective_from,
                effective_until=forecast.effective_until,
                expected_return=Decimal(str(forecast.expected_return)),
                uncertainty=Decimal(str(forecast.uncertainty)),
                confidence=Decimal(str(forecast.confidence)),
                max_trade_notional=Decimal(str(forecast.max_trade_notional)),
                max_position_notional=Decimal(str(forecast.max_position_notional)),
                max_participation_rate=Decimal(str(forecast.max_participation_rate)),
                days_to_liquidate=Decimal(str(forecast.days_to_liquidate)),
                stressed_capacity_notional=Decimal(str(forecast.stressed_capacity_notional)),
            )
            for forecast in result.forecasts
        )
        qualification = _create_qualification(session, context=context, persisted=persisted)
    session.add(
        Disclosure(
            episode_id=context.episode.id,
            audience="CODEX",
            level=1,
            classification_code=classification_code,
            reason_code=reason_code,
        )
    )
    _expose(session, context=context, qualification=qualification)
    context.assignment.state = "FINALIZED"
    context.episode.state = "DISCLOSED"
    context.episode.result = result.status.value
    context.episode.evaluated_at = result.evaluated_at
    context.episode.disclosed_at = datetime.now(UTC)
    session.flush()
    if qualification is not None:
        stage_initial_portfolio_input_evaluations(session, qualification_id=qualification.id)
    return persisted


__all__ = [
    "accept_alpha_evaluation_result",
    "ensure_trusted_evaluator_available",
    "parse_alpha_evaluation_result",
    "parse_discovery_evaluation_result",
    "parse_portfolio_evaluation_result",
    "parse_portfolio_input_evaluation_result",
    "prepare_alpha_evaluation",
    "prepare_portfolio_evaluation_request",
    "prepare_portfolio_input_evaluation",
    "prepare_discovery_evaluation",
    "run_trusted_evaluator",
    "terminalize_trusted_evaluator_failure",
    "trusted_evaluator_assignment_running",
]
