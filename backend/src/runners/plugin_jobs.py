"""Short-lived implementations for plugin install, bundle build, and removal jobs."""

from __future__ import annotations

import argparse
import os
import shutil
from collections.abc import Sequence
from datetime import UTC, datetime
from pathlib import Path
from uuid import UUID

from sqlalchemy import select

from db.models import (
    DataSource,
    ExecutionConnection,
    Job,
    PluginArtifact,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
)
from db.session import (
    SessionFactory,
    create_database_engine,
    create_session_factory,
)
from errors import QfError
from events import append_event
from plugins.contract import DescriptorSnapshot
from plugins.runtime import (
    build_bundle_environment,
    resolve_plugin_path,
    validate_release_environment,
)
from plugins.wheel_metadata import inspect_wheel, validate_wheel_set
from settings import Settings


def _load_job(factory: SessionFactory, job_id: UUID) -> Job:
    with factory() as session:
        job = session.get(Job, job_id)
        if job is None:
            raise QfError("JOB_NOT_FOUND", "Plugin job does not exist.", 404)
        session.expunge(job)
        return job


def _mark_release_failed(
    factory: SessionFactory, release_id: UUID, message: str
) -> None:
    with factory.begin() as session:
        release = session.get(PluginRelease, release_id)
        if release is None or release.state == "REMOVED":
            return
        release.state = "FAILED"
        release.is_default = False
        release.last_error = message[-4000:]
        append_event(
            session,
            kind="PLUGIN_RELEASE_FAILED",
            aggregate_type="plugin_release",
            aggregate_id=release.id,
            payload={"message": release.last_error},
        )


def install_plugin(settings: Settings, job_id: UUID) -> None:
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    job = _load_job(factory, job_id)
    release_id = job.resource_id

    try:
        with factory.begin() as session:
            release = session.execute(
                select(PluginRelease).where(PluginRelease.id == release_id).with_for_update()
            ).scalar_one()
            if release.state not in {"RECEIVED", "FAILED"}:
                raise QfError(
                    "PLUGIN_INVALID_STATE",
                    "Plugin install requires RECEIVED or FAILED state.",
                    409,
                    {"state": release.state},
                )
            release.state = "INSTALLING"
            release.last_error = None
            artifacts = list(
                session.scalars(
                    select(PluginArtifact)
                    .where(PluginArtifact.plugin_release_id == release_id)
                    .order_by(PluginArtifact.role.desc(), PluginArtifact.filename.asc())
                )
            )
            if not artifacts:
                raise QfError(
                    "PLUGIN_ARTIFACT_INVALID",
                    "Plugin release has no wheel artifacts.",
                    422,
                )
            session.flush()

        resolved = [resolve_plugin_path(settings.plugin_root, item.relative_path) for item in artifacts]
        primary_index = next(
            (index for index, item in enumerate(artifacts) if item.role == "PRIMARY"),
            None,
        )
        if primary_index is None:
            raise QfError(
                "PLUGIN_ARTIFACT_INVALID",
                "Plugin release is missing its primary wheel.",
                422,
            )
        metadata = [inspect_wheel(path) for path in resolved]
        primary_metadata = metadata[primary_index]
        dependency_metadata = tuple(
            item for index, item in enumerate(metadata) if index != primary_index
        )
        entry_point = validate_wheel_set(primary_metadata, dependency_metadata)

        with factory.begin() as session:
            release = session.get(PluginRelease, release_id)
            assert release is not None
            if release.plugin_id != entry_point.name:
                raise QfError(
                    "PLUGIN_ARTIFACT_INVALID",
                    "Primary wheel entry point does not match the declared plugin ID.",
                    422,
                    {"declared": release.plugin_id, "entry_point": entry_point.name},
                )
            release.state = "VALIDATING"

        snapshot = validate_release_environment(
            staging_root=settings.plugin_root / "validation",
            release_id=release_id,
            wheel_paths=tuple(resolved),
            plugin_id=entry_point.name,
            version=primary_metadata.version,
            timeout_seconds=settings.plugin_validation_timeout_seconds,
        )

        staging_dir = settings.plugin_root / "staging" / str(release_id)
        final_dir = settings.plugin_root / "releases" / str(release_id)
        final_dir.parent.mkdir(parents=True, exist_ok=True)
        if final_dir.exists():
            raise QfError(
                "PLUGIN_INSTALL_FAILED",
                "Plugin release destination already exists.",
                409,
            )
        os.replace(staging_dir, final_dir)

        with factory.begin() as session:
            release = session.get(PluginRelease, release_id)
            assert release is not None
            release.distribution_name = primary_metadata.distribution_name
            release.version = primary_metadata.version
            release.api_version = snapshot.api_version
            release.descriptor_snapshot = snapshot.model_dump(mode="json")
            release.state = "STAGED"
            release.last_error = None
            stored_artifacts = list(
                session.scalars(
                    select(PluginArtifact).where(
                        PluginArtifact.plugin_release_id == release_id
                    )
                )
            )
            by_name = {item.filename: item for item in artifacts}
            for item in stored_artifacts:
                item.relative_path = str(Path("releases") / str(release_id) / item.filename)
                source = by_name[item.filename]
                item.package_name = source.package_name
                item.package_version = source.package_version
            append_event(
                session,
                kind="PLUGIN_RELEASE_STAGED",
                aggregate_type="plugin_release",
                aggregate_id=release.id,
                payload={"plugin_id": release.plugin_id, "version": release.version},
            )
    except Exception as exc:
        _mark_release_failed(factory, release_id, str(exc))
        raise


def build_bundle(settings: Settings, job_id: UUID) -> None:
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    job = _load_job(factory, job_id)
    bundle_id = job.resource_id
    try:
        with factory() as session:
            bundle = session.get(PluginRuntimeBundle, bundle_id)
            if bundle is None:
                raise QfError("PLUGIN_BUNDLE_UNKNOWN", "Runtime bundle does not exist.", 404)
            members = list(
                session.scalars(
                    select(PluginRuntimeBundleMember).where(
                        PluginRuntimeBundleMember.runtime_bundle_id == bundle_id
                    )
                )
            )
            if not members:
                raise QfError(
                    "PLUGIN_BUNDLE_BUILD_FAILED",
                    "Runtime bundle has no plugin members.",
                    422,
                )
            release_ids = {item.plugin_release_id for item in members}
            releases = list(
                session.scalars(select(PluginRelease).where(PluginRelease.id.in_(release_ids)))
            )
            artifacts = list(
                session.scalars(
                    select(PluginArtifact).where(
                        PluginArtifact.plugin_release_id.in_(release_ids)
                    )
                )
            )

        release_by_id = {item.id: item for item in releases}
        if set(release_by_id) != release_ids:
            raise QfError(
                "PLUGIN_BUNDLE_BUILD_FAILED",
                "Runtime bundle references a missing plugin release.",
                422,
            )
        for release in releases:
            if release.state not in {"STAGED", "ACTIVE", "DRAINING", "INACTIVE"}:
                raise QfError(
                    "PLUGIN_BUNDLE_BUILD_FAILED",
                    "Runtime bundle contains an unusable plugin release.",
                    409,
                    {"release_id": str(release.id), "state": release.state},
                )

        wheel_paths = tuple(
            resolve_plugin_path(settings.plugin_root, artifact.relative_path)
            for artifact in sorted(
                artifacts,
                key=lambda item: (
                    str(item.plugin_release_id),
                    item.role,
                    item.filename,
                ),
            )
        )
        snapshots = tuple(
            DescriptorSnapshot.model_validate(release.descriptor_snapshot)
            for release in sorted(releases, key=lambda item: (item.plugin_id, item.version))
        )
        result = build_bundle_environment(
            plugin_root=settings.plugin_root,
            bundle_id=bundle_id,
            wheel_paths=wheel_paths,
            expected_snapshots=snapshots,
            timeout_seconds=settings.bundle_build_timeout_seconds,
        )
        with factory.begin() as session:
            bundle = session.get(PluginRuntimeBundle, bundle_id)
            assert bundle is not None
            bundle.state = "READY"
            bundle.environment_path = str(Path("bundles") / str(bundle_id))
            bundle.python_version = result.python_version
            bundle.qf_version = result.qf_version
            bundle.nautilus_version = result.nautilus_version
            bundle.ready_at = datetime.now(UTC)
            bundle.last_error = None
            append_event(
                session,
                kind="PLUGIN_BUNDLE_READY",
                aggregate_type="plugin_runtime_bundle",
                aggregate_id=bundle.id,
                payload={"release_ids": sorted(str(value) for value in release_ids)},
            )
    except Exception as exc:
        with factory.begin() as session:
            bundle = session.get(PluginRuntimeBundle, bundle_id)
            if bundle is not None and bundle.state != "REMOVED":
                bundle.state = "FAILED"
                bundle.last_error = str(exc)[-4000:]
        raise


def remove_plugin(settings: Settings, job_id: UUID) -> None:
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    job = _load_job(factory, job_id)
    release_id = job.resource_id
    force = bool(job.payload.get("force", False))

    with factory.begin() as session:
        release = session.execute(
            select(PluginRelease).where(PluginRelease.id == release_id).with_for_update()
        ).scalar_one()
        if release.state == "REMOVED":
            return
        if release.state == "ACTIVE" and not force:
            raise QfError(
                "PLUGIN_IN_USE",
                "An active plugin release cannot be removed without force.",
                409,
            )
        release.state = "REMOVING"
        release.is_default = False
        if force:
            for source in session.scalars(
                select(DataSource).where(DataSource.plugin_release_id == release_id)
            ):
                source.state = "BLOCKED_PLUGIN_REMOVED"
            for connection in session.scalars(
                select(ExecutionConnection).where(
                    ExecutionConnection.plugin_release_id == release_id
                )
            ):
                connection.state = "BLOCKED_PLUGIN_REMOVED"

    release_dir = settings.plugin_root / "releases" / str(release_id)
    shutil.rmtree(release_dir, ignore_errors=True)

    with factory.begin() as session:
        release = session.get(PluginRelease, release_id)
        assert release is not None
        release.state = "REMOVED"
        release.removed_at = datetime.now(UTC)
        release.last_error = None
        append_event(
            session,
            kind="PLUGIN_RELEASE_REMOVED",
            aggregate_type="plugin_release",
            aggregate_id=release.id,
            payload={"force": force},
        )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one isolated plugin job")
    parser.add_argument("action", choices=["install", "build", "remove"])
    parser.add_argument("job_id")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    settings = Settings.from_env()
    settings.ensure_worker_directories()
    job_id = UUID(args.job_id)
    if args.action == "install":
        install_plugin(settings, job_id)
    elif args.action == "build":
        build_bundle(settings, job_id)
    else:
        remove_plugin(settings, job_id)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
