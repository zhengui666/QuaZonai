"""Durable Search Ledger orchestration for remote Nautilus experiments."""

from __future__ import annotations

import json
import os
import stat
from datetime import UTC, datetime
from pathlib import Path
from typing import Any
from uuid import UUID, uuid4

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


def _normalized_request(request: BacktestExperimentRequest) -> dict[str, Any]:
    return request.model_dump(mode="json")


def _same_identity(
    entry: SearchLedgerEntry,
    *,
    mission_id: UUID | None,
    program_id: UUID,
    branch_id: UUID | None,
    parent_entry_id: UUID | None,
    request: BacktestExperimentRequest,
    sealed: bool,
) -> bool:
    expected_mode = ExperimentMode.SEALED.value if sealed else request.mode.value
    return (
        entry.program_id == program_id
        and entry.branch_id == branch_id
        and entry.mission_id == mission_id
        and entry.parent_entry_id == parent_entry_id
        and entry.dataset_revision_id == request.dataset_revision_id
        and entry.mode == expected_mode
        and entry.request_json == _normalized_request(request)
    )


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
        parent_entry_id: UUID | None = None,
    ) -> SearchLedgerEntry:
        with self._factory() as session, session.begin():
            existing = session.get(SearchLedgerEntry, request.experiment_id)
            if existing is not None:
                if not _same_identity(
                    existing,
                    mission_id=mission_id,
                    program_id=program_id,
                    branch_id=branch_id,
                    parent_entry_id=parent_entry_id,
                    request=request,
                    sealed=sealed,
                ):
                    raise QfError(
                        "EXPERIMENT_ID_REUSED",
                        "Experiment id is already bound to a different immutable contract or lineage.",
                        409,
                    )
                if existing.state == "RUNNING":
                    raise QfError(
                        "EXPERIMENT_ALREADY_RUNNING",
                        "The exact experiment is already running.",
                        409,
                    )
                session.expunge(existing)
                return existing

            dataset = session.get(DatasetRevision, request.dataset_revision_id)
            self._validate_dataset(dataset, request=request, sealed=sealed)
            if mission_id is not None:
                mission = session.get(ResearchMission, mission_id)
                if mission is None or mission.program_id != program_id:
                    raise QfError(
                        "MISSION_NOT_FOUND",
                        "Experiment Mission does not exist in the requested Program.",
                        404,
                    )

            if parent_entry_id is not None:
                parent = session.execute(
                    select(SearchLedgerEntry)
                    .where(SearchLedgerEntry.id == parent_entry_id)
                    .with_for_update()
                ).scalar_one_or_none()
                if parent is None or parent.program_id != program_id:
                    raise QfError(
                        "EXPERIMENT_PARENT_INVALID",
                        "Experiment parent does not exist in the requested Program.",
                        422,
                    )
                if sealed:
                    exposure = session.scalar(
                        select(SearchLedgerEntry).where(
                            SearchLedgerEntry.parent_entry_id == parent_entry_id,
                            SearchLedgerEntry.mode == ExperimentMode.SEALED.value,
                            SearchLedgerEntry.state.in_(["RUNNING", "SUCCEEDED"]),
                        )
                    )
                    if exposure is not None:
                        raise QfError(
                            "SEALED_EXPOSURE_ALREADY_CONSUMED",
                            "This source experiment already has a sealed evaluation exposure.",
                            409,
                            {"sealed_experiment_id": str(exposure.id)},
                        )

            entry = SearchLedgerEntry(
                id=request.experiment_id,
                program_id=program_id,
                branch_id=branch_id,
                mission_id=mission_id,
                dataset_revision_id=request.dataset_revision_id,
                parent_entry_id=parent_entry_id,
                mode=ExperimentMode.SEALED.value if sealed else request.mode.value,
                state="RUNNING",
                runtime_name="NAUTILUS_TRADER",
                runtime_version=None,
                request_json=_normalized_request(request),
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
            expected_mode = ExperimentMode.SEALED if sealed else request.mode
            if result.experiment_id != request.experiment_id or result.mode != expected_mode:
                raise QfError(
                    "NAUTILUS_RUNTIME_RESULT_IDENTITY_MISMATCH",
                    "Remote result does not match the immutable experiment id and mode.",
                    502,
                    {
                        "expected_experiment_id": str(request.experiment_id),
                        "received_experiment_id": str(result.experiment_id),
                        "expected_mode": expected_mode.value,
                        "received_mode": str(result.mode),
                    },
                )
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
    def _validate_dataset(
        dataset: DatasetRevision | None,
        *,
        request: BacktestExperimentRequest,
        sealed: bool,
    ) -> None:
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
        expected_catalog_uri = f"nautilus-catalog://{request.catalog_key}"
        if dataset.catalog_uri != expected_catalog_uri:
            raise QfError(
                "DATASET_CATALOG_MISMATCH",
                "Experiment catalog does not match the governed Dataset Revision.",
                422,
                {"expected": dataset.catalog_uri, "requested": expected_catalog_uri},
            )
        governed_scope = set(dataset.instrument_scope or [])
        requested_scope = set(request.instrument_ids)
        if not governed_scope or not requested_scope.issubset(governed_scope):
            raise QfError(
                "DATASET_INSTRUMENT_SCOPE_MISMATCH",
                "Experiment instruments are outside the governed Dataset Revision scope.",
                422,
                {
                    "governed": sorted(governed_scope),
                    "requested": sorted(requested_scope),
                },
            )
        if dataset.nautilus_data_type and dataset.nautilus_data_type != "QuoteTick":
            raise QfError(
                "DATASET_NAUTILUS_TYPE_MISMATCH",
                "The current Backtest contract requires a QuoteTick Dataset Revision.",
                422,
                {"actual": dataset.nautilus_data_type},
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
        if request.start_time is not None and dataset.event_start is not None:
            if request.start_time < dataset.event_start:
                raise QfError(
                    "DATASET_TIME_RANGE_MISMATCH",
                    "Experiment starts before the governed Dataset Revision.",
                    422,
                )
        if request.end_time is not None and dataset.event_end is not None:
            if request.end_time > dataset.event_end:
                raise QfError(
                    "DATASET_TIME_RANGE_MISMATCH",
                    "Experiment ends after the governed Dataset Revision.",
                    422,
                )
        if (
            request.start_time is not None
            and request.end_time is not None
            and request.start_time >= request.end_time
        ):
            raise QfError(
                "EXPERIMENT_TIME_RANGE_INVALID",
                "Experiment start_time must precede end_time.",
                422,
            )


def _safe_directory(root: Path, name: str) -> Path:
    workspace = root.resolve(strict=True)
    candidate = workspace / name
    try:
        info = os.lstat(candidate)
    except FileNotFoundError:
        os.mkdir(candidate, mode=0o700)
        info = os.lstat(candidate)
    if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode):
        raise QfError(
            "MISSION_WORKSPACE_PATH_UNSAFE",
            "Mission-controlled evidence path must be a real directory, not a link.",
            422,
            {"path": name},
        )
    if candidate.parent.resolve(strict=True) != workspace:
        raise QfError(
            "MISSION_WORKSPACE_PATH_UNSAFE",
            "Mission evidence directory escaped its worktree.",
            422,
        )
    return candidate


def write_workspace_json(workspace: Path, relative_path: str, payload: Any) -> Path:
    """Atomically write parent-worker evidence without following Mission symlinks."""
    relative = Path(relative_path)
    if relative.is_absolute() or ".." in relative.parts or len(relative.parts) != 2:
        raise QfError("MISSION_WORKSPACE_PATH_UNSAFE", "Evidence path is invalid.", 422)
    directory = _safe_directory(workspace, relative.parts[0])
    destination = directory / relative.parts[1]
    temporary = directory / f".{relative.parts[1]}.{uuid4().hex}.tmp"
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    descriptor = os.open(temporary, flags, 0o600)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(payload, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
        # os.replace replaces a destination symlink itself; it never writes through it.
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)
    return destination


def write_evidence(workspace: Path, entry: SearchLedgerEntry) -> Path:
    """Expose only structured, auditable evidence to a Discovery Mission workspace."""
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
    return write_workspace_json(workspace, f"evidence/{entry.id}.json", payload)
