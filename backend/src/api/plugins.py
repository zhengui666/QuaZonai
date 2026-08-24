"""Research/data plugin release and runtime-bundle lifecycle API."""

from __future__ import annotations

import platform
import shutil
from collections.abc import Iterable
from pathlib import Path
from typing import Any, Literal
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, File, Query, Request, UploadFile
from pydantic import BaseModel, ConfigDict, Field, model_validator
from sqlalchemy import func, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from quazonai import __version__
from api.dependencies import get_session
from db.models import (
    Job,
    PluginArtifact,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
)
from db.repositories import get_plugin_release, list_plugin_releases, plugin_catalog
from errors import QfError
from events import append_event
from jobs import enqueue_job
from plugins.manager import activate_release, deactivate_release, require_research_data_release
from plugins.storage import stream_upload, validate_upload_filename
from plugins.wheel_metadata import inspect_wheel, validate_wheel_set
from settings import Settings

router = APIRouter(prefix="/api/v1", tags=["plugins"])


class PluginReleaseView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    plugin_id: str
    distribution_name: str
    version: str
    api_version: str
    state: str
    is_default: bool
    descriptor_snapshot: dict[str, Any]
    last_error: str | None


class JobView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    kind: str
    state: str
    resource_type: str
    resource_id: UUID


class PluginStageResponse(BaseModel):
    release: PluginReleaseView
    job: JobView


MemberRole = Literal["RESEARCH", "DATA", "IMPORTER", "AUXILIARY"]


class BundleMemberInput(BaseModel):
    model_config = ConfigDict(extra="forbid")

    plugin_release_id: UUID
    member_role: MemberRole


class BundlePrewarmRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    members: list[BundleMemberInput] = Field(min_length=1, max_length=16)

    @model_validator(mode="after")
    def unique_members(self) -> "BundlePrewarmRequest":
        keys = [(item.plugin_release_id, item.member_role) for item in self.members]
        if len(set(keys)) != len(keys):
            raise ValueError("runtime bundle members must be unique")
        return self


class BundleView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    state: str
    python_version: str
    qf_version: str
    environment_path: str
    last_error: str | None
    members: list[BundleMemberInput]


class BundlePrewarmResponse(BaseModel):
    bundle: BundleView
    job: JobView | None
    reused: bool


class PluginImpactView(BaseModel):
    plugin_release_id: UUID
    runtime_bundles: int

    @property
    def referenced(self) -> bool:
        return self.runtime_bundles > 0


def _release_view(release: PluginRelease) -> PluginReleaseView:
    return PluginReleaseView.model_validate(release, from_attributes=True)


def _job_view(job: Job) -> JobView:
    return JobView.model_validate(job, from_attributes=True)


def _bundle_view(session: Session, bundle: PluginRuntimeBundle) -> BundleView:
    members = list(
        session.scalars(
            select(PluginRuntimeBundleMember)
            .where(PluginRuntimeBundleMember.runtime_bundle_id == bundle.id)
            .order_by(
                PluginRuntimeBundleMember.member_role.asc(),
                PluginRuntimeBundleMember.plugin_release_id.asc(),
            )
        )
    )
    return BundleView(
        id=bundle.id,
        state=bundle.state,
        python_version=bundle.python_version,
        qf_version=bundle.qf_version,
        environment_path=bundle.environment_path,
        last_error=bundle.last_error,
        members=[
            BundleMemberInput(
                plugin_release_id=item.plugin_release_id,
                member_role=item.member_role,  # type: ignore[arg-type]
            )
            for item in members
        ],
    )


def _impact(session: Session, release_id: UUID) -> PluginImpactView:
    return PluginImpactView(
        plugin_release_id=release_id,
        runtime_bundles=int(
            session.scalar(
                select(func.count(func.distinct(PluginRuntimeBundleMember.runtime_bundle_id))).where(
                    PluginRuntimeBundleMember.plugin_release_id == release_id
                )
            )
            or 0
        ),
    )


def _same_members(
    existing: Iterable[PluginRuntimeBundleMember],
    requested: list[BundleMemberInput],
) -> bool:
    left = {(item.plugin_release_id, item.member_role) for item in existing}
    right = {(item.plugin_release_id, item.member_role) for item in requested}
    return left == right


@router.get("/plugins")
def list_plugins(session: Session = Depends(get_session)) -> list[dict[str, Any]]:
    return plugin_catalog(session)


@router.get("/plugin-releases", response_model=list[PluginReleaseView])
def releases(session: Session = Depends(get_session)) -> list[PluginReleaseView]:
    return [_release_view(item) for item in list_plugin_releases(session)]


@router.post("/plugin-releases", response_model=PluginStageResponse, status_code=202)
async def stage_release(
    request: Request,
    primary: UploadFile = File(...),
    dependencies: list[UploadFile] | None = File(default=None),
    session: Session = Depends(get_session),
) -> PluginStageResponse:
    settings: Settings = request.app.state.settings
    release_id = uuid4()
    staging_dir = settings.plugin_root / "staging" / str(release_id)
    uploaded = [primary, *(dependencies or [])]
    names = [validate_upload_filename(item.filename) for item in uploaded]
    if len(set(names)) != len(names):
        raise QfError("PLUGIN_ARTIFACT_INVALID", "Wheel filenames must be unique.", 422)

    try:
        paths: list[Path] = []
        for upload, name in zip(uploaded, names, strict=True):
            destination = staging_dir / name
            await stream_upload(upload, destination, max_bytes=settings.max_plugin_wheel_bytes)
            paths.append(destination)

        metadata = [inspect_wheel(path) for path in paths]
        entry_point = validate_wheel_set(metadata[0], tuple(metadata[1:]))
        with session.begin():
            existing = session.scalar(
                select(PluginRelease.id).where(
                    PluginRelease.plugin_id == entry_point.name,
                    PluginRelease.version == metadata[0].version,
                )
            )
            if existing is not None:
                raise QfError("PLUGIN_VERSION_EXISTS", "Plugin version already exists.", 409)
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
            # Child artifacts and the install job reference the release directly; flush
            # the parent first because these models intentionally have no ORM relationship.
            session.flush()
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
                actor_kind="LOCAL_OPERATOR",
            )
        return PluginStageResponse(release=_release_view(release), job=_job_view(job))
    except IntegrityError as exc:
        session.rollback()
        shutil.rmtree(staging_dir, ignore_errors=True)
        raise QfError("PLUGIN_VERSION_EXISTS", "Plugin version already exists.", 409) from exc
    except Exception:
        session.rollback()
        shutil.rmtree(staging_dir, ignore_errors=True)
        raise


@router.get("/plugin-releases/{release_id}", response_model=PluginReleaseView)
def release(release_id: UUID, session: Session = Depends(get_session)) -> PluginReleaseView:
    item = get_plugin_release(session, release_id)
    if item is None:
        raise QfError("PLUGIN_UNKNOWN", "Plugin release does not exist.", 404)
    return _release_view(item)


@router.get("/plugin-releases/{release_id}/impact", response_model=PluginImpactView)
def release_impact(release_id: UUID, session: Session = Depends(get_session)) -> PluginImpactView:
    if get_plugin_release(session, release_id) is None:
        raise QfError("PLUGIN_UNKNOWN", "Plugin release does not exist.", 404)
    return _impact(session, release_id)


@router.post("/plugin-releases/{release_id}/activate", response_model=PluginReleaseView)
def activate(release_id: UUID, session: Session = Depends(get_session)) -> PluginReleaseView:
    with session.begin():
        item = activate_release(session, release_id)
        append_event(
            session,
            kind="PLUGIN_RELEASE_ACTIVATED",
            aggregate_type="plugin_release",
            aggregate_id=item.id,
            payload={"plugin_id": item.plugin_id, "version": item.version},
            actor_kind="LOCAL_OPERATOR",
        )
    return _release_view(item)


@router.post("/plugin-releases/{release_id}/deactivate", response_model=PluginReleaseView)
def deactivate(release_id: UUID, session: Session = Depends(get_session)) -> PluginReleaseView:
    with session.begin():
        item = deactivate_release(session, release_id)
        append_event(
            session,
            kind="PLUGIN_RELEASE_DRAINING",
            aggregate_type="plugin_release",
            aggregate_id=item.id,
            payload={"plugin_id": item.plugin_id, "version": item.version},
            actor_kind="LOCAL_OPERATOR",
        )
    return _release_view(item)


@router.delete("/plugin-releases/{release_id}", response_model=JobView, status_code=202)
def remove_release(
    release_id: UUID,
    force: bool = Query(default=False),
    session: Session = Depends(get_session),
) -> JobView:
    with session.begin():
        impact = _impact(session, release_id)
        item = session.execute(
            select(PluginRelease).where(PluginRelease.id == release_id).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("PLUGIN_UNKNOWN", "Plugin release does not exist.", 404)
        if impact.referenced and not force:
            raise QfError(
                "PLUGIN_IN_USE",
                "Plugin release still belongs to a runtime bundle.",
                409,
                impact.model_dump(mode="json"),
            )
        if item.state in {"REMOVING", "REMOVED"}:
            raise QfError("PLUGIN_INVALID_STATE", "Plugin is already removing or removed.", 409)
        item.state = "DRAINING"
        item.is_default = False
        job = enqueue_job(
            session,
            kind="PLUGIN_REMOVE",
            resource_type="plugin_release",
            resource_id=release_id,
            payload={"force": force},
        )
        append_event(
            session,
            kind="PLUGIN_RELEASE_REMOVE_REQUESTED",
            aggregate_type="plugin_release",
            aggregate_id=release_id,
            payload={"force": force, "impact": impact.model_dump(mode="json")},
            actor_kind="LOCAL_OPERATOR",
        )
    return _job_view(job)


@router.post("/plugin-runtime-bundles/prewarm", response_model=BundlePrewarmResponse, status_code=202)
def prewarm_bundle(
    payload: BundlePrewarmRequest,
    session: Session = Depends(get_session),
) -> BundlePrewarmResponse:
    release_ids = {item.plugin_release_id for item in payload.members}
    with session.begin():
        releases_by_id = {
            item.id: item
            for item in session.scalars(select(PluginRelease).where(PluginRelease.id.in_(release_ids)))
        }
        if set(releases_by_id) != release_ids:
            raise QfError("PLUGIN_UNKNOWN", "Runtime bundle references missing releases.", 404)
        for item in releases_by_id.values():
            if item.state not in {"STAGED", "ACTIVE", "DRAINING", "INACTIVE"}:
                raise QfError("PLUGIN_INVALID_STATE", "Runtime bundle requires validated releases.", 409)
            require_research_data_release(item)

        compatibility_keys = {
            releases_by_id[item.plugin_release_id].descriptor_snapshot.get("compatibility_key")
            for item in payload.members
            if releases_by_id[item.plugin_release_id].descriptor_snapshot.get("compatibility_key")
        }
        if len(compatibility_keys) > 1:
            raise QfError(
                "PLUGIN_BUNDLE_INCOMPATIBLE",
                "Research/data plugins have incompatible compatibility keys.",
                422,
            )

        ready_bundles = list(
            session.scalars(select(PluginRuntimeBundle).where(PluginRuntimeBundle.state == "READY"))
        )
        for existing in ready_bundles:
            existing_members = list(
                session.scalars(
                    select(PluginRuntimeBundleMember).where(
                        PluginRuntimeBundleMember.runtime_bundle_id == existing.id
                    )
                )
            )
            if _same_members(existing_members, payload.members):
                return BundlePrewarmResponse(
                    bundle=_bundle_view(session, existing), job=None, reused=True
                )

        bundle_id = uuid4()
        bundle = PluginRuntimeBundle(
            id=bundle_id,
            state="BUILDING",
            python_version=platform.python_version(),
            qf_version=__version__,
            environment_path=str(Path("bundles") / str(bundle_id)),
        )
        session.add(bundle)
        session.flush()
        for member in payload.members:
            session.add(
                PluginRuntimeBundleMember(
                    runtime_bundle_id=bundle_id,
                    plugin_release_id=member.plugin_release_id,
                    member_role=member.member_role,
                )
            )
        job = enqueue_job(
            session,
            kind="PLUGIN_BUNDLE_BUILD",
            resource_type="plugin_runtime_bundle",
            resource_id=bundle_id,
        )
        session.flush()
        return BundlePrewarmResponse(bundle=_bundle_view(session, bundle), job=_job_view(job), reused=False)


@router.get("/plugin-runtime-bundles/{bundle_id}", response_model=BundleView)
def get_bundle(bundle_id: UUID, session: Session = Depends(get_session)) -> BundleView:
    bundle = session.get(PluginRuntimeBundle, bundle_id)
    if bundle is None:
        raise QfError("PLUGIN_BUNDLE_UNKNOWN", "Runtime bundle does not exist.", 404)
    return _bundle_view(session, bundle)
