"""Internal Artifact-to-domain operations for the authenticated MCP Gateway."""

from __future__ import annotations

import os
import shutil
from pathlib import Path
from typing import Any
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, Request
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from quantfoundry.api.agent import (
    AgentPrincipal,
    _owned_artifact,
    _require_unexpired,
    _safe_artifact_path,
    require_agent_gateway,
)
from quantfoundry.api.dependencies import get_session
from quantfoundry.db.models import (
    CatalogDataset,
    DataSource,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
    Run,
    Strategy,
    StrategyVersion,
)
from quantfoundry.errors import QfError
from quantfoundry.events import append_event
from quantfoundry.jobs import enqueue_job
from quantfoundry.plugins.staging import register_staged_wheels
from quantfoundry.strategy_contract import (
    decode_strategy_source,
    validate_strategy_source,
)

router = APIRouter(prefix="/api/v1/agent/actions", tags=["agent-internal"])
CHUNK_BYTES = 1024 * 1024


class StrategyArtifactRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    default_config: dict[str, Any] = Field(default_factory=dict)


class PluginArtifactRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    artifact_ids: list[UUID] = Field(min_length=1, max_length=32)


class DatasetArtifactRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    instrument_id: str = Field(min_length=1, max_length=300)
    source_label: str = Field(min_length=1, max_length=300)
    metadata: dict[str, Any] = Field(default_factory=dict)


def _ready_artifact_path(
    session: Session,
    request: Request,
    artifact_id: UUID,
    principal: AgentPrincipal,
    expected_kind: str,
) -> Path:
    item = _owned_artifact(session, artifact_id, principal)
    _require_unexpired(item)
    if item.kind != expected_kind or item.state != "READY":
        raise QfError(
            "AGENT_ARTIFACT_INVALID_STATE",
            "Artifact kind or state is not valid for this operation.",
            409,
            {"kind": item.kind, "state": item.state},
        )
    path = _safe_artifact_path(request.app.state.settings, item.relative_path)
    if not path.is_file() or path.stat().st_size != item.size_declared:
        raise QfError("AGENT_ARTIFACT_INVALID", "Artifact content is unavailable.", 503)
    return path


def _copy_stream(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f".{destination.name}.copying")
    try:
        with source.open("rb") as reader, temporary.open("xb") as writer:
            while chunk := reader.read(CHUNK_BYTES):
                writer.write(chunk)
            writer.flush()
            os.fsync(writer.fileno())
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


@router.post("/strategy-versions/{strategy_id}/from-artifact", status_code=201)
def create_strategy_version_from_artifact(
    strategy_id: UUID,
    artifact_id: UUID,
    payload: StrategyArtifactRequest,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    path = _ready_artifact_path(
        session,
        request,
        artifact_id,
        principal,
        "STRATEGY_SOURCE",
    )
    source_text = decode_strategy_source(path.read_bytes())
    validation = validate_strategy_source(
        source_text,
        payload.default_config,
        staging_root=request.app.state.settings.import_root / "strategy-validation",
        timeout_seconds=request.app.state.settings.strategy_validation_timeout_seconds,
    )
    session.rollback()

    with session.begin():
        artifact = _owned_artifact(session, artifact_id, principal, for_update=True)
        _require_unexpired(artifact)
        if artifact.state != "READY" or artifact.kind != "STRATEGY_SOURCE":
            raise QfError("AGENT_ARTIFACT_INVALID_STATE", "Strategy Artifact is not READY.", 409)
        strategy = session.execute(
            select(Strategy).where(Strategy.id == strategy_id).with_for_update()
        ).scalar_one_or_none()
        if strategy is None:
            raise QfError("STRATEGY_UNKNOWN", "Strategy does not exist.", 404)
        latest = session.scalar(
            select(func.max(StrategyVersion.version_no)).where(
                StrategyVersion.strategy_id == strategy_id
            )
        )
        version = StrategyVersion(
            strategy_id=strategy_id,
            version_no=int(latest or 0) + 1,
            source_text=source_text,
            default_config=payload.default_config,
            objective_directions=list(validation.objective_directions),
        )
        session.add(version)
        session.flush()
        artifact.state = "CONSUMED"
        artifact.consumed_by_type = "strategy_version"
        artifact.consumed_by_id = version.id
        append_event(
            session,
            kind="STRATEGY_VERSION_CREATED",
            aggregate_type="strategy_version",
            aggregate_id=version.id,
            payload={"strategy_id": str(strategy_id), "version_no": version.version_no},
            actor_kind="MCP_AGENT",
            actor_metadata={
                "issuer": principal.issuer,
                "subject": principal.subject,
                "client_id": principal.client_id,
            },
        )
    return {
        "id": str(version.id),
        "strategy_id": str(version.strategy_id),
        "version_no": version.version_no,
        "default_config": version.default_config,
        "objective_directions": version.objective_directions,
    }


@router.post("/plugin-releases/from-artifacts", status_code=202)
def stage_plugin_from_artifacts(
    payload: PluginArtifactRequest,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    if len(set(payload.artifact_ids)) != len(payload.artifact_ids):
        raise QfError("PLUGIN_ARTIFACT_INVALID", "Plugin Artifact IDs must be unique.", 422)
    artifacts = [
        _owned_artifact(session, artifact_id, principal) for artifact_id in payload.artifact_ids
    ]
    for artifact in artifacts:
        _require_unexpired(artifact)
        if artifact.kind != "PLUGIN_WHEEL" or artifact.state != "READY":
            raise QfError(
                "PLUGIN_ARTIFACT_INVALID",
                "All plugin Artifacts must be READY wheel uploads.",
                409,
            )
    filenames = [artifact.filename for artifact in artifacts]
    if len(set(filenames)) != len(filenames):
        raise QfError("PLUGIN_ARTIFACT_INVALID", "Wheel filenames must be unique.", 422)
    source_paths = [
        _safe_artifact_path(request.app.state.settings, artifact.relative_path)
        for artifact in artifacts
    ]
    if any(not path.is_file() for path in source_paths):
        raise QfError("AGENT_ARTIFACT_INVALID", "Plugin Artifact content is unavailable.", 503)
    session.rollback()

    release_id = uuid4()
    staging_dir = request.app.state.settings.plugin_root / "staging" / str(release_id)
    try:
        paths: list[Path] = []
        for source, filename in zip(source_paths, filenames, strict=True):
            destination = staging_dir / filename
            _copy_stream(source, destination)
            paths.append(destination)
        with session.begin():
            locked = [
                _owned_artifact(session, artifact_id, principal, for_update=True)
                for artifact_id in payload.artifact_ids
            ]
            if any(item.state != "READY" or item.kind != "PLUGIN_WHEEL" for item in locked):
                raise QfError(
                    "AGENT_ARTIFACT_INVALID_STATE",
                    "Plugin Artifact state changed before consumption.",
                    409,
                )
            release, job = register_staged_wheels(
                session,
                release_id=release_id,
                paths=paths,
                actor_kind="MCP_AGENT",
            )
            for item in locked:
                item.state = "CONSUMED"
                item.consumed_by_type = "plugin_release"
                item.consumed_by_id = release.id
        return {
            "release": {
                "id": str(release.id),
                "plugin_id": release.plugin_id,
                "version": release.version,
                "state": release.state,
            },
            "job": {"id": str(job.id), "kind": job.kind, "state": job.state},
        }
    except Exception:
        shutil.rmtree(staging_dir, ignore_errors=True)
        raise


def _ready_bundle(session: Session, release_id: UUID) -> PluginRuntimeBundle:
    bundle = session.execute(
        select(PluginRuntimeBundle)
        .join(
            PluginRuntimeBundleMember,
            PluginRuntimeBundleMember.runtime_bundle_id == PluginRuntimeBundle.id,
        )
        .where(
            PluginRuntimeBundleMember.plugin_release_id == release_id,
            PluginRuntimeBundle.state == "READY",
        )
        .order_by(PluginRuntimeBundle.ready_at.desc())
        .limit(1)
    ).scalar_one_or_none()
    if bundle is None:
        raise QfError(
            "PLUGIN_RUNTIME_UNAVAILABLE",
            "Dataset import requires a READY plugin runtime bundle.",
            503,
        )
    return bundle


@router.post("/datasets/{source_id}/from-artifact", status_code=202)
def create_dataset_from_artifact(
    source_id: UUID,
    artifact_id: UUID,
    payload: DatasetArtifactRequest,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    source_path = _ready_artifact_path(
        session,
        request,
        artifact_id,
        principal,
        "PARQUET_L2",
    )
    source = session.get(DataSource, source_id)
    if source is None:
        raise QfError("DATA_SOURCE_UNKNOWN", "Data source does not exist.", 404)
    if source.state != "ACTIVE":
        raise QfError("DATA_SOURCE_INACTIVE", "Data source is not ACTIVE.", 409)
    release = session.get(PluginRelease, source.plugin_release_id)
    if release is None or "HISTORICAL_IMPORT" not in release.descriptor_snapshot.get(
        "capabilities", []
    ):
        raise QfError("CAPABILITY_MISMATCH", "Plugin cannot import historical data.", 422)
    bundle = _ready_bundle(session, source.plugin_release_id)
    session.rollback()

    run_id = uuid4()
    dataset_id = uuid4()
    upload_path = request.app.state.settings.import_root / str(run_id) / "upload.parquet"
    try:
        _copy_stream(source_path, upload_path)
        metadata = {
            **payload.metadata,
            "source_label": payload.source_label.strip(),
            "upload_size_bytes": upload_path.stat().st_size,
            "data_type": str(payload.metadata.get("data_type", "OrderBookDeltas")),
        }
        with session.begin():
            artifact = _owned_artifact(session, artifact_id, principal, for_update=True)
            _require_unexpired(artifact)
            if artifact.state != "READY" or artifact.kind != "PARQUET_L2":
                raise QfError(
                    "AGENT_ARTIFACT_INVALID_STATE",
                    "Parquet Artifact state changed before consumption.",
                    409,
                )
            current_source = session.get(DataSource, source_id)
            current_bundle = session.get(PluginRuntimeBundle, bundle.id)
            if current_source is None or current_source.state != "ACTIVE":
                raise QfError("DATA_SOURCE_INACTIVE", "Data source is not ACTIVE.", 409)
            if current_bundle is None or current_bundle.state != "READY":
                raise QfError("PLUGIN_RUNTIME_UNAVAILABLE", "Runtime bundle is not READY.", 503)
            run = Run(
                id=run_id,
                experiment_id=None,
                runtime_bundle_id=current_bundle.id,
                type="PARQUET_IMPORT",
                state="QUEUED",
                summary={
                    "data_source_id": str(source_id),
                    "instrument_id": payload.instrument_id.strip(),
                    "source_label": payload.source_label.strip(),
                },
            )
            dataset = CatalogDataset(
                id=dataset_id,
                data_source_id=source_id,
                instrument_id=payload.instrument_id.strip(),
                catalog_path=str(Path("datasets") / str(dataset_id)),
                dataset_metadata=metadata,
                state="IMPORTING",
                run_id=run_id,
            )
            session.add_all([run, dataset])
            job = enqueue_job(
                session,
                kind="PARQUET_IMPORT",
                resource_type="run",
                resource_id=run_id,
                payload={"dataset_id": str(dataset_id)},
            )
            artifact.state = "CONSUMED"
            artifact.consumed_by_type = "catalog_dataset"
            artifact.consumed_by_id = dataset.id
            append_event(
                session,
                kind="PARQUET_IMPORT_QUEUED",
                aggregate_type="run",
                aggregate_id=run.id,
                payload={"dataset_id": str(dataset.id), "instrument_id": dataset.instrument_id},
                actor_kind="MCP_AGENT",
                actor_metadata={
                    "issuer": principal.issuer,
                    "subject": principal.subject,
                    "client_id": principal.client_id,
                },
            )
        return {
            "dataset": {"id": str(dataset.id), "state": dataset.state},
            "run": {"id": str(run.id), "state": run.state},
            "job": {"id": str(job.id), "state": job.state},
        }
    except Exception:
        shutil.rmtree(upload_path.parent, ignore_errors=True)
        raise
