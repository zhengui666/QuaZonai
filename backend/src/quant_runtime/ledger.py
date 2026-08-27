"""Durable Search Ledger orchestration for remote Nautilus experiments."""

from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session, sessionmaker

from db.models import DatasetRevision, ResearchMission, SearchLedgerEntry
from errors import QfError
from quant_runtime.client import NautilusQuantRuntime, RemoteNautilusConfig
from quant_runtime.contracts import (
    BacktestEvidence,
    BacktestExperimentRequest,
    ExperimentMode,
    SealedBacktestResult,
)


def _now() -> datetime:
    return datetime.now(UTC)


class ExperimentCoordinator:
    """Executes one remote experiment without holding a DB transaction over the network."""

    def __init__(self, factory: sessionmaker[Session]) -> None:
        self._factory = factory

    def execute(
        self,
        *,
        mission_id: UUID | None,
        program_id: UUID,
        branch_id: UUID | None,
        request: BacktestExperimentRequest,
        sealed: bool = False,
    ) -> SearchLedgerEntry:
        with self._factory() as session, session.begin():
            existing = session.get(SearchLedgerEntry, request.experiment_id)
            if existing is not None:
                return existing
            dataset = session.get(DatasetRevision, request.dataset_revision_id)
            self._validate_dataset(dataset, sealed=sealed)
            assert dataset is not None
            requested_catalog_uri = f"nautilus-catalog://{request.catalog_key}"
            if dataset.catalog_uri != requested_catalog_uri:
                raise QfError(
                    "DATASET_CATALOG_MISMATCH",
                    "Experiment catalog does not match the governed Dataset Revision.",
                    422,
                    {"expected": dataset.catalog_uri, "requested": requested_catalog_uri},
                )
            if mission_id is not None:
                mission = session.get(ResearchMission, mission_id)
                if mission is None or mission.program_id != program_id:
                    raise QfError(
                        "MISSION_NOT_FOUND",
                        "Experiment Mission does not exist in the requested Program.",
                        404,
                    )
            entry = SearchLedgerEntry(
                id=request.experiment_id,
                program_id=program_id,
                branch_id=branch_id,
                mission_id=mission_id,
                dataset_revision_id=request.dataset_revision_id,
                parent_entry_id=None,
                mode=ExperimentMode.SEALED.value if sealed else request.mode.value,
                state="RUNNING",
                runtime_name="NAUTILUS_TRADER",
                runtime_version=None,
                request_json=request.model_dump(mode="json"),
                evidence_json={},
                disclosure_json={},
                started_at=_now(),
                finished_at=None,
                failure_code=None,
                failure_message=None,
            )
            session.add(entry)

        try:
            config = RemoteNautilusConfig.from_env(sealed=sealed)
            with NautilusQuantRuntime(config) as runtime:
                if sealed:
                    result: BacktestEvidence | SealedBacktestResult = runtime.run_sealed_backtest(
                        request
                    )
                else:
                    result = runtime.run_backtest(request)
        except Exception as exc:
            with self._factory() as session, session.begin():
                entry = session.execute(
                    select(SearchLedgerEntry)
                    .where(SearchLedgerEntry.id == request.experiment_id)
                    .with_for_update()
                ).scalar_one()
                entry.state = "FAILED"
                entry.finished_at = _now()
                entry.failure_code = str(getattr(exc, "code", type(exc).__name__))[:100]
                entry.failure_message = str(exc)[-12000:]
            raise

        with self._factory() as session, session.begin():
            entry = session.execute(
                select(SearchLedgerEntry)
                .where(SearchLedgerEntry.id == request.experiment_id)
                .with_for_update()
            ).scalar_one()
            entry.state = "SUCCEEDED"
            entry.finished_at = _now()
            entry.runtime_version = result.runtime_version
            entry.remote_run_id = result.remote_run_id
            if isinstance(result, SealedBacktestResult):
                entry.evidence_json = {}
                entry.disclosure_json = result.disclosure
            else:
                entry.evidence_json = result.model_dump(mode="json")
                entry.disclosure_json = {}
            session.flush()
            session.expunge(entry)
            return entry

    @staticmethod
    def _validate_dataset(dataset: DatasetRevision | None, *, sealed: bool) -> None:
        if dataset is None:
            raise QfError("DATASET_REVISION_NOT_FOUND", "Dataset Revision does not exist.", 404)
        expected_partition = "SEALED" if sealed else "DISCOVERY"
        if dataset.partition != expected_partition:
            raise QfError(
                "DATASET_PARTITION_MISMATCH",
                "Dataset partition is not permitted for this experiment mode.",
                422,
                {"expected": expected_partition, "actual": dataset.partition},
            )
        if dataset.quality_state != "VALID" or dataset.point_in_time_state != "VALID":
            raise QfError(
                "DATASET_GOVERNANCE_FAILED",
                "Dataset quality and point-in-time checks must pass before an experiment.",
                422,
            )
        if not dataset.catalog_uri:
            raise QfError(
                "NAUTILUS_CATALOG_MISSING",
                "Dataset Revision is not linked to a remote Nautilus catalog.",
                422,
            )
        if dataset.available_start is None or dataset.available_end is None:
            raise QfError(
                "DATASET_AVAILABILITY_MISSING",
                "Dataset Revision must preserve point-in-time availability bounds.",
                422,
            )
        if dataset.event_start and dataset.available_start < dataset.event_start:
            raise QfError(
                "DATASET_POINT_IN_TIME_INVALID",
                "Dataset availability cannot precede its event-time range.",
                422,
            )


def write_evidence(workspace: Path, entry: SearchLedgerEntry) -> Path:
    """Expose only structured, auditable evidence to a Discovery Mission workspace."""
    import json

    evidence_root = workspace / "evidence"
    evidence_root.mkdir(parents=True, exist_ok=True)
    path = evidence_root / f"{entry.id}.json"
    payload = {
        "experiment_id": str(entry.id),
        "state": entry.state,
        "mode": entry.mode,
        "runtime_name": entry.runtime_name,
        "runtime_version": entry.runtime_version,
        "remote_run_id": entry.remote_run_id,
        "evidence": entry.evidence_json,
        "failure_code": entry.failure_code,
        "failure_message": entry.failure_message,
    }
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    return path
