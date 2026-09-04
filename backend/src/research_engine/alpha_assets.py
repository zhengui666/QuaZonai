"""Persist trusted Alpha signal/evaluation facts without any execution surface."""

from __future__ import annotations

from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from math import isfinite
from typing import Any
from uuid import UUID

from pydantic import BaseModel, ConfigDict, ValidationError, field_validator
from sqlalchemy import select
from sqlalchemy.orm import Session

from db.models import (
    AlphaCalibrationVersion,
    AlphaEvaluationEpisode,
    AlphaModel,
    AlphaModelVersion,
    AlphaQualification,
    AlphaSignalArtifact,
    DatasetRevision,
    NautilusCatalogBinding,
    QuantRuntimeRun,
)
from errors import QfError
from quant_runtime.alpha_contracts import AlphaSignalFrameV1
from research_engine.evaluation import AlphaEvaluation, Metric, evaluate_alpha


@dataclass(frozen=True)
class AlphaEvaluationOutcome:
    episode_id: UUID
    result: str
    signal_artifact_id: UUID | None
    qualification_id: UUID | None


class TrustedRealizedReturn(BaseModel):
    """One evaluator-owned realized return keyed to one Alpha signal."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    event_time: datetime
    instrument_id: str
    realized_return: float | None = None

    @field_validator("event_time")
    @classmethod
    def require_utc(cls, value: datetime) -> datetime:
        if value.tzinfo is None or value.utcoffset() != timedelta(0):
            raise ValueError("event_time must be UTC")
        return value.astimezone(UTC)

    @field_validator("instrument_id")
    @classmethod
    def require_instrument_id(cls, value: str) -> str:
        value = value.strip()
        if not value:
            raise ValueError("instrument_id must not be blank")
        return value

    @field_validator("realized_return", mode="before")
    @classmethod
    def require_finite_return(cls, value: object) -> object:
        if value is None:
            return value
        if isinstance(value, bool):
            raise ValueError("realized_return must not be boolean")
        if not isinstance(value, (int, float, str)):
            raise ValueError("realized_return must be finite")
        try:
            numeric = float(value)
        except (TypeError, ValueError, OverflowError) as exc:
            raise ValueError("realized_return must be finite") from exc
        if not isfinite(numeric):
            raise ValueError("realized_return must be finite")
        return numeric


def _outcome(session: Session, episode: AlphaEvaluationEpisode) -> AlphaEvaluationOutcome:
    artifact = session.scalar(
        select(AlphaSignalArtifact).where(
            AlphaSignalArtifact.alpha_model_version_id == episode.alpha_model_version_id,
            AlphaSignalArtifact.dataset_revision_id == episode.sealed_dataset_revision_id,
        )
    )
    qualification = session.scalar(
        select(AlphaQualification).where(AlphaQualification.evaluation_episode_id == episode.id)
    )
    assert episode.result is not None
    return AlphaEvaluationOutcome(
        episode_id=episode.id,
        result=episode.result,
        signal_artifact_id=artifact.id if artifact is not None else None,
        qualification_id=qualification.id if qualification is not None else None,
    )


def _finish(
    session: Session,
    episode: AlphaEvaluationEpisode,
    *,
    result: str,
    gates: dict[str, str],
    metrics: dict[str, Any] | None = None,
    trial_count: int | None = None,
    signal_artifact_id: UUID | None = None,
    qualification_id: UUID | None = None,
) -> AlphaEvaluationOutcome:
    episode.state = "COMPLETED"
    episode.result = result
    episode.gate_results = gates
    if metrics is not None:
        episode.multiple_testing_summary = {
            "trial_count": trial_count,
            "metrics": metrics,
        }
    session.flush()
    return AlphaEvaluationOutcome(
        episode_id=episode.id,
        result=result,
        signal_artifact_id=signal_artifact_id,
        qualification_id=qualification_id,
    )


def _parse_uuid(value: object) -> UUID | None:
    if value is None:
        return None
    try:
        return UUID(str(value))
    except (TypeError, ValueError):
        return None


def _parse_frame(value: object) -> AlphaSignalFrameV1 | None:
    try:
        return value if isinstance(value, AlphaSignalFrameV1) else AlphaSignalFrameV1.model_validate(value)
    except (TypeError, ValidationError, ValueError):
        return None


def _parse_evidence(
    frame: AlphaSignalFrameV1,
    value: object,
) -> tuple[tuple[float | None, ...] | None, str | None]:
    if value is None:
        rows: tuple[object, ...] = ()
    elif isinstance(value, (str, bytes, Mapping)) or not isinstance(value, Iterable):
        return None, "INVALID"
    else:
        rows = tuple(value)

    evidence_by_key: dict[tuple[datetime, str], float | None] = {}
    for item in rows:
        try:
            evidence = (
                item
                if isinstance(item, TrustedRealizedReturn)
                else TrustedRealizedReturn.model_validate(item)
            )
        except (TypeError, ValidationError, ValueError):
            return None, "INVALID"
        key = (evidence.event_time, evidence.instrument_id)
        if key in evidence_by_key:
            return None, "INVALID"
        evidence_by_key[key] = evidence.realized_return

    keys = {(point.event_time, point.instrument_id) for point in frame.points}
    if any(key not in keys for key in evidence_by_key):
        return None, "INVALID"
    returns = tuple(evidence_by_key.get((point.event_time, point.instrument_id)) for point in frame.points)
    return returns, "INCONCLUSIVE" if any(item is None for item in returns) else None


def _metric_summary(evaluation: AlphaEvaluation) -> dict[str, Any]:
    def metric(value: Metric) -> dict[str, Any]:
        return {"value": value.value, "status": value.status}

    return {
        "observation_count": evaluation.observation_count,
        "coverage": metric(evaluation.coverage),
        "ic_mean": metric(evaluation.ic_mean),
        "rank_ic_mean": metric(evaluation.rank_ic_mean),
        "hit_rate": metric(evaluation.hit_rate),
        "net_return": metric(evaluation.net_return),
        "annualized_volatility": metric(evaluation.annualized_volatility),
        "sharpe_ratio": metric(evaluation.sharpe_ratio),
        "max_drawdown": metric(evaluation.max_drawdown),
        "trial_adjusted_sharpe": metric(evaluation.trial_adjusted_sharpe),
    }


def _signal_artifact(
    session: Session,
    *,
    version: AlphaModelVersion,
    dataset: DatasetRevision,
    run: QuantRuntimeRun,
    frame: AlphaSignalFrameV1,
    artifact_uri: str,
) -> AlphaSignalArtifact | None:
    if not frame.points:
        return None
    event_times = [point.event_time for point in frame.points]
    available_times = [point.available_time for point in frame.points]
    existing = session.scalar(
        select(AlphaSignalArtifact).where(
            AlphaSignalArtifact.alpha_model_version_id == version.id,
            AlphaSignalArtifact.dataset_revision_id == dataset.id,
            AlphaSignalArtifact.mode == version.mode,
        )
    )
    if existing is not None:
        if (
            existing.run_id == run.id
            and existing.artifact_uri == artifact_uri
            and existing.row_count == len(frame.points)
            and existing.event_start == min(event_times)
            and existing.event_end == max(event_times)
            and existing.available_start == min(available_times)
            and existing.available_end == max(available_times)
            and existing.schema_version == "AlphaSignalFrameV1"
        ):
            return existing
        raise QfError(
            "ALPHA_SIGNAL_ARTIFACT_CONFLICT",
            "An immutable Alpha Signal Artifact already exists for this Model Version and Dataset.",
            409,
        )
    artifact = AlphaSignalArtifact(
        alpha_model_version_id=version.id,
        dataset_revision_id=dataset.id,
        run_id=run.id,
        mode=version.mode,
        artifact_uri=artifact_uri,
        row_count=len(frame.points),
        event_start=min(event_times),
        event_end=max(event_times),
        available_start=min(available_times),
        available_end=max(available_times),
        schema_version="AlphaSignalFrameV1",
    )
    session.add(artifact)
    session.flush()
    return artifact


def _qualification_state(
    version: AlphaModelVersion,
    calibration: AlphaCalibrationVersion | None,
    role: object,
) -> tuple[str | None, str | None, str | None]:
    if role is None:
        return None, None, None
    if not isinstance(role, str) or not (role := role.strip()):
        return None, None, "INVALID"
    role = role.upper()
    if role == "SHADOW_ALPHA" and version.mode == "RELATIVE_SCORE":
        return role, "SHADOW", None
    if (
        role in {"PRIMARY_ALPHA", "HEDGE_ALPHA", "RISK_SIGNAL"}
        and version.mode == "CALIBRATED_RETURN"
        and calibration is not None
    ):
        return role, "ACTIVE", None
    return None, None, "INVALID"


def _qualification(
    session: Session,
    *,
    episode: AlphaEvaluationEpisode,
    model: AlphaModel,
    version: AlphaModelVersion,
    dataset: DatasetRevision,
    calibration: AlphaCalibrationVersion | None,
    signal_artifact: AlphaSignalArtifact,
    role: str,
    state: str,
    scope_json: dict[str, str],
    metrics: dict[str, Any],
) -> AlphaQualification:
    existing = session.scalar(
        select(AlphaQualification).where(
            AlphaQualification.alpha_model_version_id == version.id,
            AlphaQualification.universe_version_id == dataset.universe_version_id,
            AlphaQualification.horizon == version.horizon,
            AlphaQualification.role == role,
            AlphaQualification.alpha_model_id.is_not(None),
        )
    )
    if existing is not None:
        if existing.evaluation_episode_id == episode.id:
            return existing
        raise QfError(
            "ALPHA_QUALIFICATION_CONFLICT",
            "An immutable Alpha Qualification already exists for this scope.",
            409,
        )
    qualification = AlphaQualification(
        program_id=episode.program_id,
        alpha_model_id=model.id,
        alpha_model_version_id=version.id,
        calibration_version_id=calibration.id if calibration is not None else None,
        universe_version_id=dataset.universe_version_id,
        universe=dataset.universe_name,
        horizon=version.horizon,
        role=role,
        state=state,
        name=model.name,
        scope_json=scope_json,
        evaluation_episode_id=episode.id,
        degradation_state="HEALTHY",
        metrics=metrics,
        lineage=[
            {
                "alpha_model_version_id": str(version.id),
                "signal_artifact_id": str(signal_artifact.id),
                "evaluation_episode_id": str(episode.id),
            }
        ],
    )
    session.add(qualification)
    session.flush()
    return qualification


def persist_trusted_alpha_evaluation(
    session: Session,
    *,
    episode_id: UUID,
    run_id: object,
    artifact_uri: object,
    signal_frame: object,
    realized_returns: object,
    annualization_factor: object,
    trial_count: object,
    calibration_version_id: object = None,
    qualification_role: object = None,
) -> AlphaEvaluationOutcome:
    """Persist one evaluator-owned Alpha result; never creates a Candidate or Approval."""
    episode = session.scalar(
        select(AlphaEvaluationEpisode)
        .where(AlphaEvaluationEpisode.id == episode_id)
        .with_for_update()
    )
    if episode is None:
        raise QfError("ALPHA_EVALUATION_NOT_FOUND", "Alpha Evaluation Episode was not found.", 404)
    if episode.state == "COMPLETED" and episode.result is not None:
        return _outcome(session, episode)
    if episode.state not in {"PENDING", "RUNNING"}:
        raise QfError(
            "ALPHA_EVALUATION_STATE_CONFLICT",
            "Alpha Evaluation Episode is not runnable.",
            409,
            {"state": episode.state},
        )

    dataset = session.get(DatasetRevision, episode.sealed_dataset_revision_id)
    catalog = (
        session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.dataset_revision_id == dataset.id
            )
        )
        if dataset is not None
        else None
    )
    gates = {
        "dataset_quality": dataset.quality_state if dataset is not None else "MISSING",
        "point_in_time": dataset.point_in_time_state if dataset is not None else "MISSING",
        "promotability": dataset.promotability if dataset and dataset.promotability else "UNKNOWN",
        "dataset_partition": dataset.partition if dataset is not None else "MISSING",
        "sealed_catalog": "PASS" if catalog is not None and catalog.sealed else "INVALID",
    }
    if (
        dataset is None
        or dataset.quality_state != "VALID"
        or dataset.point_in_time_state != "VALID"
        or dataset.promotability != "PROMOTABLE"
        or dataset.partition != "SEALED"
        or catalog is None
        or not catalog.sealed
    ):
        return _finish(session, episode, result="INVALID", gates=gates)

    version = session.get(AlphaModelVersion, episode.alpha_model_version_id)
    model = session.get(AlphaModel, version.alpha_model_id) if version is not None else None
    gates["alpha_model_version"] = version.state if version is not None else "MISSING"
    if (
        version is None
        or model is None
        or version.state != "VALIDATED"
        or dataset.universe_version_id != version.universe_version_id
        or episode.program_id != model.owner_program_id
    ):
        gates["model_dataset_scope"] = "INVALID"
        return _finish(session, episode, result="INVALID", gates=gates)
    gates["model_dataset_scope"] = "PASS"

    parsed_run_id = _parse_uuid(run_id)
    run = session.get(QuantRuntimeRun, parsed_run_id) if parsed_run_id is not None else None
    if (
        run is None
        or run.program_id != episode.program_id
        or run.branch_id != episode.branch_id
        or (episode.sealed_run_id is not None and episode.sealed_run_id != run.id)
    ):
        gates["sealed_run"] = "INVALID"
        return _finish(session, episode, result="INVALID", gates=gates)
    episode.sealed_run_id = run.id
    gates["sealed_run"] = "PASS"

    parsed_calibration_id = _parse_uuid(calibration_version_id)
    calibration = (
        session.get(AlphaCalibrationVersion, parsed_calibration_id)
        if parsed_calibration_id is not None
        else None
    )
    if version.mode == "CALIBRATED_RETURN" and (
        calibration is None
        or calibration.alpha_model_version_id != version.id
        or calibration.state != "VALIDATED"
    ):
        gates["calibration"] = "INVALID"
        return _finish(session, episode, result="INVALID", gates=gates)
    if calibration is not None and (
        calibration.alpha_model_version_id != version.id or calibration.state != "VALIDATED"
    ):
        gates["calibration"] = "INVALID"
        return _finish(session, episode, result="INVALID", gates=gates)
    gates["calibration"] = "PASS" if calibration is not None else "NOT_REQUIRED"

    frame = _parse_frame(signal_frame)
    uri = artifact_uri.strip() if isinstance(artifact_uri, str) else ""
    if frame is None or (frame.points and not uri):
        gates["signal_frame"] = "INVALID"
        return _finish(session, episode, result="INVALID", gates=gates)
    gates["signal_frame"] = "PASS"

    if (
        isinstance(annualization_factor, bool)
        or not isinstance(annualization_factor, (int, float, str))
        or isinstance(trial_count, bool)
        or not isinstance(trial_count, int)
    ):
        gates["evidence"] = "INVALID"
        return _finish(session, episode, result="INVALID", gates=gates)
    try:
        annualization = float(annualization_factor)
        trials = trial_count
        if not isfinite(annualization) or annualization <= 0 or trials < 1:
            raise ValueError
    except (TypeError, ValueError, OverflowError):
        gates["evidence"] = "INVALID"
        return _finish(session, episode, result="INVALID", gates=gates)

    returns, evidence_result = _parse_evidence(frame, realized_returns)
    if returns is None:
        gates["evidence"] = "INVALID"
        return _finish(session, episode, result="INVALID", gates=gates)
    signal_artifact = _signal_artifact(
        session,
        version=version,
        dataset=dataset,
        run=run,
        frame=frame,
        artifact_uri=uri,
    )
    try:
        evaluation = evaluate_alpha(
            (point.score for point in frame.points),
            returns,
            annualization_factor=annualization,
            trial_count=trials,
        )
    except ValueError:
        gates["evidence"] = "INVALID"
        return _finish(
            session,
            episode,
            result="INVALID",
            gates=gates,
            signal_artifact_id=signal_artifact.id if signal_artifact is not None else None,
        )
    metrics = _metric_summary(evaluation)
    if not frame.points or any(point.expected_return is None for point in frame.points if version.mode == "CALIBRATED_RETURN"):
        evidence_result = "INCONCLUSIVE"
    if evidence_result == "INCONCLUSIVE":
        gates["evidence"] = "INCONCLUSIVE"
        return _finish(
            session,
            episode,
            result="INCONCLUSIVE",
            gates=gates,
            metrics=metrics,
            trial_count=trials,
            signal_artifact_id=signal_artifact.id if signal_artifact is not None else None,
        )

    gates["evidence"] = "PASS"
    role, qualification_state, qualification_error = _qualification_state(
        version, calibration, qualification_role
    )
    if qualification_error is not None:
        gates["qualification"] = qualification_error
        return _finish(
            session,
            episode,
            result="INVALID",
            gates=gates,
            metrics=metrics,
            trial_count=trials,
            signal_artifact_id=signal_artifact.id if signal_artifact is not None else None,
        )
    scope_json = {"mode": version.mode}
    if role == "PRIMARY_ALPHA":
        instruments = {point.instrument_id for point in frame.points}
        if len(instruments) != 1:
            gates["portfolio_scope"] = "PORTFOLIO_SCOPE_UNSUPPORTED"
            return _finish(
                session,
                episode,
                result="INCONCLUSIVE",
                gates=gates,
                metrics=metrics,
                trial_count=trials,
                signal_artifact_id=signal_artifact.id if signal_artifact is not None else None,
            )
        scope_json["instrument_id"] = next(iter(instruments))
    qualification = (
        _qualification(
            session,
            episode=episode,
            model=model,
            version=version,
            dataset=dataset,
            calibration=calibration,
            signal_artifact=signal_artifact,
            role=role,
            state=qualification_state,
            scope_json=scope_json,
            metrics=metrics,
        )
        if role is not None and qualification_state is not None and signal_artifact is not None
        else None
    )
    return _finish(
        session,
        episode,
        result="PASS",
        gates=gates,
        metrics=metrics,
        trial_count=trials,
        signal_artifact_id=signal_artifact.id if signal_artifact is not None else None,
        qualification_id=qualification.id if qualification is not None else None,
    )
