"""Register one already-staged runtime plugin wheel set."""

from __future__ import annotations

from pathlib import Path
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from quantfoundry.db.models import Job, PluginArtifact, PluginRelease
from quantfoundry.errors import QfError
from quantfoundry.events import append_event
from quantfoundry.jobs import enqueue_job
from quantfoundry.plugins.wheel_metadata import inspect_wheel, validate_wheel_set


def register_staged_wheels(
    session: Session,
    *,
    release_id: UUID,
    paths: list[Path],
    actor_kind: str,
) -> tuple[PluginRelease, Job]:
    if not paths:
        raise QfError("PLUGIN_ARTIFACT_INVALID", "A primary wheel is required.", 422)
    metadata = [inspect_wheel(path) for path in paths]
    entry_point = validate_wheel_set(metadata[0], tuple(metadata[1:]))
    existing = session.scalar(
        select(PluginRelease.id).where(
            PluginRelease.plugin_id == entry_point.name,
            PluginRelease.version == metadata[0].version,
        )
    )
    if existing is not None:
        raise QfError(
            "PLUGIN_VERSION_EXISTS",
            "This plugin ID and version already exist.",
            409,
            {"plugin_id": entry_point.name, "version": metadata[0].version},
        )
    release = PluginRelease(
        id=release_id,
        plugin_id=entry_point.name,
        distribution_name=metadata[0].distribution_name,
        version=metadata[0].version,
        api_version="1",
        state="RECEIVED",
        descriptor_snapshot={},
    )
    session.add(release)
    for index, (path, item) in enumerate(zip(paths, metadata, strict=True)):
        session.add(
            PluginArtifact(
                plugin_release_id=release_id,
                role="PRIMARY" if index == 0 else "DEPENDENCY",
                filename=path.name,
                relative_path=str(Path("staging") / str(release_id) / path.name),
                package_name=item.distribution_name,
                package_version=item.version,
            )
        )
    job = enqueue_job(
        session,
        kind="PLUGIN_INSTALL",
        resource_type="plugin_release",
        resource_id=release_id,
    )
    append_event(
        session,
        kind="PLUGIN_RELEASE_RECEIVED",
        aggregate_type="plugin_release",
        aggregate_id=release_id,
        payload={"plugin_id": entry_point.name, "version": metadata[0].version},
        actor_kind=actor_kind,
    )
    session.flush()
    return release, job
