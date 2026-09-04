"""Short-lived implementations for research/data plugin lifecycle jobs."""

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
    Job,
    PluginArtifact,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
)
from db.session import SessionFactory, create_database_engine
from errors import QfError
from events import append_event
from jobs import JobLease, create_lease_fenced_session_factory
from plugins.contract import DescriptorSnapshot
from plugins.runtime import build_bundle_environment, resolve_plugin_path, validate_release_environment
from plugins.wheel_metadata import inspect_wheel, validate_wheel_set
from runtime_config import load_effective_settings
from settings import Settings


def _load_job(factory: SessionFactory, job_id: UUID) -> Job:
    with factory() as session:
        job = session.get(Job, job_id)
        if job is None:
            raise QfError("JOB_NOT_FOUND", "Plugin job does not exist.", 404)
        session.expunge(job)
        return job


def _mark_release_failed(factory: SessionFactory, release_id: UUID, message: str) -> None:
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


def install_plugin(settings: Settings, lease: JobLease) -> None:
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    job = _load_job(factory, lease.job_id)
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
                raise QfError("PLUGIN_ARTIFACT_INVALID", "Plugin release has no wheel artifacts.", 422)

        resolved = [resolve_plugin_path(settings.plugin_root, item.relative_path) for item in artifacts]
        primary_index = next(
            (index for index, item in enumerate(artifacts) if item.role == "PRIMARY"), None
        )
        if primary_index is None:
            raise QfError("PLUGIN_ARTIFACT_INVALID", "Plugin release is missing primary wheel.", 422)
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
                    "Primary wheel entry point does not match declared plugin ID.",
                    422,
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
            raise QfError("PLUGIN_INSTALL_FAILED", "Plugin release destination exists.", 409)
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
                    select(PluginArtifact).where(PluginArtifact.plugin_release_id == release_id)
                )
            )
            for item in stored_artifacts:
                item.relative_path = str(Path("releases") / str(release_id) / item.filename)
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
    finally:
        engine.dispose()


def build_bundle(settings: Settings, lease: JobLease) -> None:
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    job = _load_job(factory, lease.job_id)
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
                raise QfError("PLUGIN_BUNDLE_BUILD_FAILED", "Runtime bundle has no members.", 422)
            release_ids = {item.plugin_release_id for item in members}
            releases = list(
                session.scalars(select(PluginRelease).where(PluginRelease.id.in_(release_ids)))
            )
            artifacts = list(
                session.scalars(
                    select(PluginArtifact).where(PluginArtifact.plugin_release_id.in_(release_ids))
                )
            )
        release_by_id = {item.id: item for item in releases}
        if set(release_by_id) != release_ids:
            raise QfError("PLUGIN_BUNDLE_BUILD_FAILED", "Bundle references missing release.", 422)
        if any(
            release.state not in {"STAGED", "ACTIVE", "DRAINING", "INACTIVE"}
            for release in releases
        ):
            raise QfError("PLUGIN_BUNDLE_BUILD_FAILED", "Bundle contains unusable release.", 409)

        wheel_paths = tuple(
            resolve_plugin_path(settings.plugin_root, artifact.relative_path)
            for artifact in sorted(
                artifacts,
                key=lambda item: (str(item.plugin_release_id), item.role, item.filename),
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
    finally:
        engine.dispose()


def remove_plugin(settings: Settings, lease: JobLease) -> None:
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    job = _load_job(factory, lease.job_id)
    release_id = job.resource_id
    force = bool(job.payload.get("force", False))
    with factory.begin() as session:
        release = session.execute(
            select(PluginRelease).where(PluginRelease.id == release_id).with_for_update()
        ).scalar_one()
        if release.state == "REMOVED":
            return
        if release.state == "ACTIVE" and not force:
            raise QfError("PLUGIN_IN_USE", "Active plugin cannot be removed without force.", 409)
        bundle_ids = list(
            session.scalars(
                select(PluginRuntimeBundleMember.runtime_bundle_id).where(
                    PluginRuntimeBundleMember.plugin_release_id == release_id
                )
            )
        )
        if bundle_ids and not force:
            raise QfError("PLUGIN_IN_USE", "Plugin still belongs to runtime bundles.", 409)
        if force and bundle_ids:
            for bundle in session.scalars(
                select(PluginRuntimeBundle).where(PluginRuntimeBundle.id.in_(bundle_ids))
            ):
                if bundle.state not in {"REMOVED", "FAILED"}:
                    bundle.state = "STALE"
        release.state = "REMOVING"
        release.is_default = False

    shutil.rmtree(settings.plugin_root / "releases" / str(release_id), ignore_errors=True)
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
    engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one isolated plugin job")
    parser.add_argument("action", choices=["install", "build", "remove"])
    parser.add_argument("job_id")
    parser.add_argument("--lease-owner", required=True)
    parser.add_argument("--lease-attempt", required=True, type=int)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    settings = load_effective_settings(Settings.from_env())
    settings.ensure_worker_directories()
    lease = JobLease(
        job_id=UUID(args.job_id),
        owner=args.lease_owner,
        attempt=args.lease_attempt,
    )
    if args.action == "install":
        install_plugin(settings, lease)
    elif args.action == "build":
        build_bundle(settings, lease)
    else:
        remove_plugin(settings, lease)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
