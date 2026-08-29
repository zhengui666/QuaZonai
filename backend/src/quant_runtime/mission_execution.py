"""Trusted Mission-side bridge from Codex artifacts to remote Nautilus experiments."""

from __future__ import annotations

import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any
from uuid import UUID

from sqlalchemy import select

from db.models import (
    EvaluationEpisode,
    DatasetRevision,
    Event,
    NautilusCatalogBinding,
    QuantRuntimeRun,
    ResearchMission,
    ResearchProgram,
    ResearchCharter,
    SearchLedgerEntry,
)
from db.session import create_database_engine, create_session_factory
from errors import QfError
from jobs import enqueue_job
from optimization import TrialPoint, select_compromise
from quant_runtime.config import RemoteNautilusConfig
from quant_runtime.contracts import ExperimentSpec, MissionExperimentEnvelope, RunEvidence
from quant_runtime.evidence import extract_statistics, persistable_evidence
from quant_runtime.remote import NautilusQuantRuntime
from settings import Settings

_EXPERIMENT_FILE = "EXPERIMENTS.json"
_EVIDENCE_FILE = "EVIDENCE.json"


def _now() -> datetime:
    return datetime.now(UTC)


def _statistics(evidence: RunEvidence) -> tuple[float, float, float, int]:
    metrics = extract_statistics(evidence)
    if metrics is None:
        raise QfError(
            "INVALID_RUNTIME_STATISTICS",
            "Nautilus evidence is missing complete finite statistics.",
            422,
        )
    return metrics[:4]


def _summary(evidence: RunEvidence) -> dict[str, Any]:
    metrics = extract_statistics(evidence)
    summary: dict[str, Any] = {
        "state": evidence.state,
        "mode": evidence.mode,
        "external_run_id": evidence.external_run_id,
        "nautilus_version": evidence.nautilus_version,
        "metrics_valid": metrics is not None,
        "total_fills": len(evidence.fills),
        "total_positions": len(evidence.positions),
    }
    if metrics is not None:
        sharpe, max_drawdown, turnover, total_orders, _ = metrics
        summary.update(
            {
                "total_orders": total_orders,
                "sharpe_ratio": sharpe,
                "max_drawdown": max_drawdown,
                "turnover": turnover,
            }
        )
    return summary


def build_mission_quant_context(settings: Settings) -> str:
    """Return only discovery catalog capabilities which the Mission may reference."""
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory() as session:
            catalogs = session.scalars(
                select(NautilusCatalogBinding)
                .where(
                    NautilusCatalogBinding.sealed.is_(False),
                    NautilusCatalogBinding.quality_state == "VALID",
                    NautilusCatalogBinding.point_in_time_state == "VALID",
                )
                .order_by(NautilusCatalogBinding.created_at.asc())
            ).all()
    finally:
        engine.dispose()

    if not catalogs:
        return (
            "\n## Canonical Quant Runtime\n\n"
            "No governed Nautilus discovery catalog is currently available. "
            "Do not invent data or write synthetic performance evidence.\n"
        )

    catalog_rows = [
        {
            "catalog_uri": item.catalog_uri,
            "nautilus_data_type": item.nautilus_data_type,
            "instrument_scope": item.instrument_scope,
            "event_time_range": item.event_time_range,
            "available_time_range": item.available_time_range,
        }
        for item in catalogs
    ]
    example = {
        "experiments": [
            {
                "experiment_key": "ema-cross-v1",
                "family": "EMA_CROSS",
                "catalog_uri": catalogs[0].catalog_uri,
                "strategy": {
                    "strategy_path": "strategy.ema_cross:EMACross",
                    "config_path": "strategy.ema_cross:EMACrossConfig",
                    "config": {
                        "instrument_id": catalogs[0].instrument_scope[0]
                        if catalogs[0].instrument_scope
                        else "INSTRUMENT.VENUE",
                        "bar_type": "INSTRUMENT.VENUE-1-MINUTE-LAST-EXTERNAL",
                        "trade_size": "1",
                        "fast_ema_period": 10,
                        "slow_ema_period": 20,
                    },
                    "source_files": {
                        "strategy/__init__.py": "",
                        "strategy/ema_cross.py": "# Nautilus Strategy and StrategyConfig",
                    },
                    "requirements": ["nautilus-trader==1.231.0"],
                },
                "parameters": {"hypothesis": "bounded experiment description"},
            }
        ]
    }
    return (
        "\n## Canonical Quant Runtime\n\n"
        "QuaZonai uses a remote, pinned NautilusTrader runtime. You do not receive its "
        "service token, sealed catalogs, broker credentials, or direct network access. "
        "For an ALPHA_DISCOVERY Mission, write `EXPERIMENTS.json` containing one to "
        "twenty bounded experiments. Every listed experiment is executed by the trusted "
        "parent after the Codex turn and every success or failure enters Search Ledger. "
        "Use only the following governed discovery catalogs:\n\n"
        f"```json\n{json.dumps(catalog_rows, ensure_ascii=False, indent=2)}\n```\n\n"
        "Required artifact shape:\n\n"
        f"```json\n{json.dumps(example, ensure_ascii=False, indent=2)}\n```\n"
    )


def _persist_started_run(
    settings: Settings,
    *,
    mission: ResearchMission,
    experiment: ExperimentSpec,
) -> UUID:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            run = QuantRuntimeRun(
                program_id=mission.program_id,
                branch_id=mission.branch_id,
                mission_id=mission.id,
                mode="DISCOVERY",
                state="RUNNING",
                experiment_key=experiment.experiment_key,
                family=experiment.family,
                catalog_uri=experiment.catalog_uri,
                runtime_name="NautilusTrader",
                strategy_artifact=experiment.strategy.model_dump(mode="json"),
                parameters=experiment.parameters,
                started_at=_now(),
            )
            session.add(run)
            session.flush()
            return run.id
    finally:
        engine.dispose()


def _finish_run(
    settings: Settings,
    *,
    run_id: UUID,
    evidence: RunEvidence | None,
    error: Exception | None,
) -> None:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            run = session.get(QuantRuntimeRun, run_id)
            if run is None:
                raise QfError("QUANT_RUN_NOT_FOUND", "Quant runtime run disappeared.", 500)
            now = _now()
            run.finished_at = now
            if evidence is not None:
                run.state = evidence.state
                run.external_run_id = evidence.external_run_id
                run.runtime_name = evidence.runtime_name
                run.runtime_version = evidence.nautilus_version
                run.contract_version = evidence.contract_version
                run.evidence = persistable_evidence(evidence)
                run.error_code = evidence.error_code
                run.error_message = evidence.error_message
                metrics = extract_statistics(evidence)
                unusable = evidence.state == "SUCCEEDED" and (
                    metrics is None or metrics[3] == 0
                )
                outcome = "SUCCEEDED" if evidence.state == "SUCCEEDED" and not unusable else "FAILED"
                failure_code = (
                    "INVALID_RUNTIME_STATISTICS"
                    if metrics is None
                    else "NO_TRADING_EVIDENCE"
                    if unusable
                    else evidence.error_code
                )
                evidence_summary = _summary(evidence)
                if unusable:
                    evidence_summary["state"] = "FAILED"
                    evidence_summary["error_code"] = failure_code
            else:
                run.state = "FAILED"
                run.error_code = str(getattr(error, "code", type(error).__name__))[:100]
                run.error_message = str(error)[-4000:] if error is not None else "Unknown error"
                outcome = "FAILED"
                failure_code = run.error_code
                evidence_summary = {"state": "FAILED", "error_code": run.error_code}

            session.add(
                SearchLedgerEntry(
                    program_id=run.program_id,
                    branch_id=run.branch_id,
                    mission_id=run.mission_id,
                    run_id=run.id,
                    family=run.family,
                    parameters=run.parameters,
                    outcome=outcome,
                    failure_code=failure_code,
                    disclosure_level="DISCOVERY_FULL",
                    evidence_summary=evidence_summary,
                    created_at=now,
                )
            )
    finally:
        engine.dispose()


def _load_mission_and_catalogs(
    settings: Settings,
    mission_id: UUID,
) -> tuple[ResearchMission, set[str], NautilusCatalogBinding | None]:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory() as session:
            mission = session.get(ResearchMission, mission_id)
            if mission is None:
                raise QfError("MISSION_NOT_FOUND", "Research Mission does not exist.", 404)
            session.expunge(mission)
            program = session.get(ResearchProgram, mission.program_id)
            charter = session.get(ResearchCharter, program.charter_id) if program else None
            discovery = set()
            for binding in session.scalars(
                select(NautilusCatalogBinding).where(
                    NautilusCatalogBinding.sealed.is_(False),
                    NautilusCatalogBinding.quality_state == "VALID",
                    NautilusCatalogBinding.point_in_time_state == "VALID",
                )
            ).all():
                revision = session.get(DatasetRevision, binding.dataset_revision_id)
                if revision is None or charter is None:
                    continue
                allowed_ids = {str(value) for value in charter.universe_version_ids}
                if allowed_ids and str(revision.universe_version_id) not in allowed_ids:
                    continue
                if not allowed_ids and charter.market_scope != "System inferred" and revision.universe_name != charter.market_scope:
                    continue
                discovery.add(binding.catalog_uri)
            sealed = session.scalar(
                select(NautilusCatalogBinding)
                .where(
                    NautilusCatalogBinding.sealed.is_(True),
                    NautilusCatalogBinding.quality_state == "VALID",
                    NautilusCatalogBinding.point_in_time_state == "VALID",
                )
                .order_by(NautilusCatalogBinding.created_at.asc())
            )
            if sealed is not None:
                session.expunge(sealed)
            return mission, discovery, sealed
    finally:
        engine.dispose()


def _queue_sealed_evaluation(
    settings: Settings,
    *,
    mission: ResearchMission,
    discovery_run_id: UUID,
) -> UUID:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            discovery = session.get(QuantRuntimeRun, discovery_run_id)
            if discovery is None:
                raise QfError("DISCOVERY_RUN_NOT_FOUND", "Selected discovery run does not exist.", 500)
            discovery_binding = session.scalar(
                select(NautilusCatalogBinding).where(
                    NautilusCatalogBinding.catalog_uri == discovery.catalog_uri,
                    NautilusCatalogBinding.sealed.is_(False),
                    NautilusCatalogBinding.quality_state == "VALID",
                    NautilusCatalogBinding.point_in_time_state == "VALID",
                )
            )
            if discovery_binding is None:
                raise QfError(
                    "DISCOVERY_DATASET_NOT_FOUND",
                    "Selected discovery run is not bound to a Dataset Revision.",
                    422,
                )
            discovery_revision = session.get(DatasetRevision, discovery_binding.dataset_revision_id)
            sealed_bindings = session.scalars(
                select(NautilusCatalogBinding).where(
                    NautilusCatalogBinding.sealed.is_(True),
                    NautilusCatalogBinding.quality_state == "VALID",
                    NautilusCatalogBinding.point_in_time_state == "VALID",
                )
            ).all()
            sealed_revision_id: UUID | None = None
            discovery_scope = set(discovery_binding.instrument_scope)
            for binding in sealed_bindings:
                revision = session.get(DatasetRevision, binding.dataset_revision_id)
                if (
                    discovery_revision is not None
                    and revision is not None
                    and discovery_revision.universe_version_id is not None
                    and revision.universe_version_id == discovery_revision.universe_version_id
                    and set(binding.instrument_scope) == discovery_scope
                    and binding.nautilus_data_type == discovery_binding.nautilus_data_type
                ):
                    sealed_revision_id = revision.id
                    break
            if sealed_revision_id is None:
                raise QfError(
                    "SEALED_CATALOG_INCOMPATIBLE",
                    "No valid sealed Dataset Revision matches the selected discovery scope.",
                    422,
                )
            episode = EvaluationEpisode(
                program_id=mission.program_id,
                branch_id=mission.branch_id,
                discovery_run_id=discovery_run_id,
                sealed_dataset_revision_id=sealed_revision_id,
                state="SEALED_PENDING",
                disclosure={},
            )
            session.add(episode)
            session.flush()
            enqueue_job(
                session,
                kind="SEALED_EVALUATION",
                resource_type="EVALUATION_EPISODE",
                resource_id=episode.id,
            )
            session.add(
                Event(
                    kind="SEALED_EVALUATION_QUEUED",
                    aggregate_type="RESEARCH_PROGRAM",
                    aggregate_id=mission.program_id,
                    actor_kind="SYSTEM",
                    actor_metadata={},
                    payload={
                        "mission_id": str(mission.id),
                        "evaluation_episode_id": str(episode.id),
                        "discovery_run_id": str(discovery_run_id),
                    },
                )
            )
            return episode.id
    finally:
        engine.dispose()


def execute_mission_experiments(
    settings: Settings,
    *,
    mission_id: UUID,
    workspace: Path,
) -> dict[str, Any] | None:
    """Execute the Mission's typed experiments without exposing remote credentials to Codex."""
    mission, allowed_catalogs, sealed_catalog = _load_mission_and_catalogs(settings, mission_id)
    artifact_path = workspace / _EXPERIMENT_FILE
    if not artifact_path.is_file():
        if mission.type == "ALPHA_DISCOVERY":
            raise QfError(
                "MISSION_EXPERIMENT_ARTIFACT_MISSING",
                "ALPHA_DISCOVERY must produce EXPERIMENTS.json for the canonical runtime.",
                422,
            )
        return None

    try:
        envelope = MissionExperimentEnvelope.model_validate_json(
            artifact_path.read_text(encoding="utf-8")
        )
    except (OSError, ValueError) as exc:
        raise QfError(
            "MISSION_EXPERIMENT_ARTIFACT_INVALID",
            "EXPERIMENTS.json does not satisfy the quant-runtime contract.",
            422,
        ) from exc

    for experiment in envelope.experiments:
        if experiment.catalog_uri not in allowed_catalogs:
            raise QfError(
                "MISSION_CATALOG_SCOPE_VIOLATION",
                "A Mission experiment references an unavailable or sealed catalog.",
                403,
                {"catalog_uri": experiment.catalog_uri},
            )

    config = RemoteNautilusConfig.from_env(required=True)
    assert config is not None
    runtime = NautilusQuantRuntime(config)
    completed: list[tuple[UUID, RunEvidence]] = []
    failures: list[dict[str, str]] = []

    for experiment in envelope.experiments:
        run_id = _persist_started_run(settings, mission=mission, experiment=experiment)
        try:
            evidence = runtime.run_backtest(experiment)
        except Exception as exc:  # noqa: BLE001 - each failed trial must enter Search Ledger
            _finish_run(settings, run_id=run_id, evidence=None, error=exc)
            failures.append(
                {
                    "experiment_key": experiment.experiment_key,
                    "error_code": str(getattr(exc, "code", type(exc).__name__)),
                }
            )
            continue
        _finish_run(settings, run_id=run_id, evidence=evidence, error=None)
        metrics = extract_statistics(evidence) if evidence.state == "SUCCEEDED" else None
        if evidence.state != "SUCCEEDED":
            failures.append(
                {
                    "experiment_key": experiment.experiment_key,
                    "error_code": evidence.error_code or "NAUTILUS_RUN_FAILED",
                }
            )
        elif metrics is None:
            failures.append(
                {
                    "experiment_key": experiment.experiment_key,
                    "error_code": "INVALID_RUNTIME_STATISTICS",
                }
            )
        elif metrics[3] > 0:
            completed.append((run_id, evidence))
        else:
            failures.append(
                {
                    "experiment_key": experiment.experiment_key,
                    "error_code": "NO_TRADING_EVIDENCE",
                }
            )

    if not completed:
        result: dict[str, Any] = {
            "runs": [],
            "failures": failures,
            "promotion": "NO_COMPLETED_RUN",
        }
        (workspace / _EVIDENCE_FILE).write_text(
            json.dumps(result, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        raise QfError(
            "MISSION_EXPERIMENTS_FAILED",
            "No Mission experiment produced usable Nautilus run evidence.",
            422,
            {"failed_experiments": failures},
        )

    points: list[TrialPoint] = []
    for index, (_, evidence) in enumerate(completed):
        sharpe, max_drawdown, turnover, _ = _statistics(evidence)
        points.append(
            TrialPoint(
                trial_no=index,
                values=(sharpe, max_drawdown, turnover),
            )
        )
    winner_index = select_compromise(
        points,
        ("maximize", "minimize", "minimize"),
    ).trial_no
    winner_run_id, winner_evidence = completed[winner_index]
    episode_id = None
    if sealed_catalog is not None:
        episode_id = _queue_sealed_evaluation(
            settings,
            mission=mission,
            discovery_run_id=winner_run_id,
        )

    result = {
        "runs": [
            {"run_id": str(run_id), **_summary(evidence)}
            for run_id, evidence in completed
        ],
        "failures": failures,
        "selected_discovery_run_id": str(winner_run_id),
        "selected_evidence": _summary(winner_evidence),
        "sealed_evaluation_episode_id": str(episode_id) if episode_id else None,
        "promotion": "SEALED_EVALUATION_QUEUED" if episode_id else "SEALED_CATALOG_REQUIRED",
    }
    (workspace / _EVIDENCE_FILE).write_text(
        json.dumps(result, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    return result
