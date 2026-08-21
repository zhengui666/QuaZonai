"""Data-source and execution-connection control-plane API."""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path
from typing import Any
from uuid import UUID

from fastapi import APIRouter, Depends, Request
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from quantfoundry.api.credentials import decrypt_credential_secrets
from quantfoundry.api.dependencies import get_session
from quantfoundry.db.models import (
    CredentialSet,
    DataSource,
    ExecutionConnection,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
)
from quantfoundry.errors import QfError
from quantfoundry.events import append_event
from quantfoundry.plugins.contract import Capability
from quantfoundry.plugins.runtime import resolve_plugin_path
from quantfoundry.schema_validation import validate_schema_value
from quantfoundry.settings import Settings

router = APIRouter(prefix="/api/v1", tags=["integrations"])


class IntegrationWrite(BaseModel):
    model_config = ConfigDict(extra="forbid")

    plugin_release_id: UUID
    credential_set_id: UUID | None = None
    name: str = Field(min_length=1, max_length=200)
    config: dict[str, Any] = Field(default_factory=dict)


class DataSourceView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    plugin_release_id: UUID
    credential_set_id: UUID | None
    name: str
    config: dict[str, Any]
    state: str


class ExecutionConnectionView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    plugin_release_id: UUID
    credential_set_id: UUID
    name: str
    config: dict[str, Any]
    state: str


class PreflightResult(BaseModel):
    model_config = ConfigDict(extra="forbid")

    ok: bool
    plugin_release_id: UUID
    runtime_bundle_id: UUID
    capability: str
    constructed_type: str
    details: dict[str, Any] | None = None


def _release_with_capability(
    session: Session,
    release_id: UUID,
    capabilities: set[Capability],
    *,
    allow_draining: bool = False,
) -> PluginRelease:
    release = session.get(PluginRelease, release_id)
    if release is None:
        raise QfError("PLUGIN_UNKNOWN", "Plugin release does not exist.", 404)
    allowed_states = {"ACTIVE", "DRAINING"} if allow_draining else {"ACTIVE"}
    if release.state not in allowed_states:
        raise QfError(
            "PLUGIN_NOT_ACTIVE",
            "Integration resources require an active plugin release.",
            409,
            {"state": release.state},
        )
    actual = set(release.descriptor_snapshot.get("capabilities", []))
    requested = {item.value for item in capabilities}
    if actual.isdisjoint(requested):
        raise QfError(
            "CAPABILITY_MISMATCH",
            "Plugin release does not provide the required capability.",
            422,
            {"required": sorted(requested), "actual": sorted(actual)},
        )
    return release


def _credential(
    session: Session,
    credential_id: UUID | None,
    release_id: UUID,
    *,
    required: bool,
) -> CredentialSet | None:
    if credential_id is None:
        if required:
            raise QfError(
                "CREDENTIAL_INVALID",
                "Execution connections require a credential set.",
                422,
            )
        return None
    item = session.get(CredentialSet, credential_id)
    if item is None:
        raise QfError("CREDENTIAL_UNKNOWN", "Credential set does not exist.", 404)
    if item.plugin_release_id != release_id:
        raise QfError(
            "CREDENTIAL_INVALID",
            "Credential set and integration must reference the same plugin release.",
            422,
        )
    return item


def _validate_config(release: PluginRelease, config: dict[str, Any]) -> None:
    validate_schema_value(
        config,
        release.descriptor_snapshot.get("public_config_schema", {"type": "object"}),
    )


def _raise_bundle_unavailable(release_id: UUID) -> PluginRuntimeBundle:
    raise QfError(
        "PLUGIN_RUNTIME_UNAVAILABLE",
        "No ready runtime bundle contains this plugin release.",
        503,
        {"plugin_release_id": str(release_id)},
    )


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
        return _raise_bundle_unavailable(release_id)
    return bundle


def _bundle_python(settings: Settings, bundle: PluginRuntimeBundle) -> Path:
    root = resolve_plugin_path(settings.plugin_root, bundle.environment_path)
    python = root / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    if not python.is_file():
        raise QfError(
            "PLUGIN_RUNTIME_UNAVAILABLE",
            "Runtime bundle Python executable is unavailable.",
            503,
            {"bundle_id": str(bundle.id)},
        )
    return python


def _run_preflight(
    *,
    session: Session,
    settings: Settings,
    release: PluginRelease,
    credential_set: CredentialSet | None,
    config: dict[str, Any],
    capability: Capability,
) -> PreflightResult:
    bundle = _ready_bundle(session, release.id)
    secrets = (
        decrypt_credential_secrets(session, settings, credential_set)
        if credential_set is not None
        else {}
    )
    request_payload = json.dumps(
        {
            "plugin_id": release.plugin_id,
            "capability": capability.value,
            "public_config": config,
            "secret_config": secrets,
        },
        separators=(",", ":"),
    )
    try:
        result = subprocess.run(
            [str(_bundle_python(settings, bundle)), "-m", "quantfoundry.plugins.runtime_call"],
            input=request_payload,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=settings.integration_preflight_timeout_seconds,
            env=os.environ.copy(),
        )
    except subprocess.TimeoutExpired as exc:
        raise QfError(
            "PLUGIN_RUNTIME_UNAVAILABLE",
            "Integration preflight exceeded its time limit.",
            503,
        ) from exc
    except subprocess.CalledProcessError as exc:
        raise QfError(
            "PLUGIN_CONFIG_INVALID",
            "Integration preflight failed inside the runtime bundle.",
            422,
            {"exit_code": exc.returncode},
        ) from exc
    try:
        output = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise QfError(
            "PLUGIN_RUNTIME_UNAVAILABLE",
            "Integration preflight returned an invalid response.",
            503,
        ) from exc
    return PreflightResult(
        ok=True,
        plugin_release_id=release.id,
        runtime_bundle_id=bundle.id,
        capability=capability.value,
        constructed_type=str(output.get("constructed_type", "unknown")),
        details={"preflight_performed": bool(output.get("preflight_performed"))},
    )


def _data_view(item: DataSource) -> DataSourceView:
    return DataSourceView.model_validate(item, from_attributes=True)


def _execution_view(item: ExecutionConnection) -> ExecutionConnectionView:
    return ExecutionConnectionView.model_validate(item, from_attributes=True)


@router.get("/data-sources", response_model=list[DataSourceView])
def list_data_sources(session: Session = Depends(get_session)) -> list[DataSourceView]:
    return [
        _data_view(item)
        for item in session.scalars(select(DataSource).order_by(DataSource.name.asc()))
    ]


@router.get("/data-sources/{source_id}", response_model=DataSourceView)
def show_data_source(
    source_id: UUID,
    session: Session = Depends(get_session),
) -> DataSourceView:
    item = session.get(DataSource, source_id)
    if item is None:
        raise QfError("DATA_SOURCE_UNKNOWN", "Data source does not exist.", 404)
    return _data_view(item)


@router.post("/data-sources", response_model=DataSourceView, status_code=201)
def create_data_source(
    payload: IntegrationWrite,
    session: Session = Depends(get_session),
) -> DataSourceView:
    try:
        with session.begin():
            release = _release_with_capability(
                session,
                payload.plugin_release_id,
                {Capability.HISTORICAL_IMPORT, Capability.LIVE_DATA},
            )
            _validate_config(release, payload.config)
            _credential(session, payload.credential_set_id, release.id, required=False)
            item = DataSource(
                plugin_release_id=release.id,
                credential_set_id=payload.credential_set_id,
                name=payload.name.strip(),
                config=payload.config,
                state="ACTIVE",
            )
            session.add(item)
            session.flush()
            append_event(
                session,
                kind="DATA_SOURCE_CREATED",
                aggregate_type="data_source",
                aggregate_id=item.id,
                payload={"plugin_release_id": str(release.id)},
                actor_kind="LOCAL_OPERATOR",
            )
    except IntegrityError as exc:
        session.rollback()
        raise QfError("RESOURCE_CONFLICT", "Data source name already exists.", 409) from exc
    return _data_view(item)


@router.put("/data-sources/{source_id}", response_model=DataSourceView)
def update_data_source(
    source_id: UUID,
    payload: IntegrationWrite,
    session: Session = Depends(get_session),
) -> DataSourceView:
    with session.begin():
        item = session.execute(
            select(DataSource).where(DataSource.id == source_id).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("DATA_SOURCE_UNKNOWN", "Data source does not exist.", 404)
        if item.plugin_release_id != payload.plugin_release_id:
            raise QfError(
                "RESOURCE_REFERENCED",
                "Create a new data source to migrate to another plugin release.",
                409,
            )
        release = _release_with_capability(
            session,
            item.plugin_release_id,
            {Capability.HISTORICAL_IMPORT, Capability.LIVE_DATA},
        )
        _validate_config(release, payload.config)
        _credential(session, payload.credential_set_id, release.id, required=False)
        item.name = payload.name.strip()
        item.credential_set_id = payload.credential_set_id
        item.config = payload.config
        append_event(
            session,
            kind="DATA_SOURCE_UPDATED",
            aggregate_type="data_source",
            aggregate_id=item.id,
            payload={},
            actor_kind="LOCAL_OPERATOR",
        )
    return _data_view(item)


@router.post("/data-sources/{source_id}/preflight", response_model=PreflightResult)
def preflight_data_source(
    source_id: UUID,
    request: Request,
    session: Session = Depends(get_session),
) -> PreflightResult:
    item = session.get(DataSource, source_id)
    if item is None:
        raise QfError("DATA_SOURCE_UNKNOWN", "Data source does not exist.", 404)
    release = _release_with_capability(
        session,
        item.plugin_release_id,
        {Capability.HISTORICAL_IMPORT, Capability.LIVE_DATA},
        allow_draining=True,
    )
    credential_set = _credential(
        session,
        item.credential_set_id,
        release.id,
        required=False,
    )
    capability = (
        Capability.HISTORICAL_IMPORT
        if Capability.HISTORICAL_IMPORT.value
        in release.descriptor_snapshot.get("capabilities", [])
        else Capability.LIVE_DATA
    )
    return _run_preflight(
        session=session,
        settings=request.app.state.settings,
        release=release,
        credential_set=credential_set,
        config=item.config,
        capability=capability,
    )


@router.get("/execution-connections", response_model=list[ExecutionConnectionView])
def list_execution_connections(
    session: Session = Depends(get_session),
) -> list[ExecutionConnectionView]:
    return [
        _execution_view(item)
        for item in session.scalars(
            select(ExecutionConnection).order_by(ExecutionConnection.name.asc())
        )
    ]


@router.get(
    "/execution-connections/{connection_id}",
    response_model=ExecutionConnectionView,
)
def show_execution_connection(
    connection_id: UUID,
    session: Session = Depends(get_session),
) -> ExecutionConnectionView:
    item = session.get(ExecutionConnection, connection_id)
    if item is None:
        raise QfError(
            "EXECUTION_CONNECTION_UNKNOWN",
            "Execution connection does not exist.",
            404,
        )
    return _execution_view(item)


@router.post("/execution-connections", response_model=ExecutionConnectionView, status_code=201)
def create_execution_connection(
    payload: IntegrationWrite,
    session: Session = Depends(get_session),
) -> ExecutionConnectionView:
    try:
        with session.begin():
            release = _release_with_capability(
                session,
                payload.plugin_release_id,
                {Capability.EXECUTION},
            )
            _validate_config(release, payload.config)
            credential_set = _credential(
                session,
                payload.credential_set_id,
                release.id,
                required=True,
            )
            assert credential_set is not None
            item = ExecutionConnection(
                plugin_release_id=release.id,
                credential_set_id=credential_set.id,
                name=payload.name.strip(),
                config=payload.config,
                state="ACTIVE",
            )
            session.add(item)
            session.flush()
            append_event(
                session,
                kind="EXECUTION_CONNECTION_CREATED",
                aggregate_type="execution_connection",
                aggregate_id=item.id,
                payload={"plugin_release_id": str(release.id)},
                actor_kind="LOCAL_OPERATOR",
            )
    except IntegrityError as exc:
        session.rollback()
        raise QfError(
            "RESOURCE_CONFLICT",
            "Execution connection name already exists.",
            409,
        ) from exc
    return _execution_view(item)


@router.put(
    "/execution-connections/{connection_id}",
    response_model=ExecutionConnectionView,
)
def update_execution_connection(
    connection_id: UUID,
    payload: IntegrationWrite,
    session: Session = Depends(get_session),
) -> ExecutionConnectionView:
    with session.begin():
        item = session.execute(
            select(ExecutionConnection)
            .where(ExecutionConnection.id == connection_id)
            .with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError(
                "EXECUTION_CONNECTION_UNKNOWN",
                "Execution connection does not exist.",
                404,
            )
        if item.plugin_release_id != payload.plugin_release_id:
            raise QfError(
                "RESOURCE_REFERENCED",
                "Create a new execution connection to migrate plugin releases.",
                409,
            )
        release = _release_with_capability(
            session,
            item.plugin_release_id,
            {Capability.EXECUTION},
        )
        _validate_config(release, payload.config)
        credential_set = _credential(
            session,
            payload.credential_set_id,
            release.id,
            required=True,
        )
        assert credential_set is not None
        item.name = payload.name.strip()
        item.credential_set_id = credential_set.id
        item.config = payload.config
        append_event(
            session,
            kind="EXECUTION_CONNECTION_UPDATED",
            aggregate_type="execution_connection",
            aggregate_id=item.id,
            payload={},
            actor_kind="LOCAL_OPERATOR",
        )
    return _execution_view(item)


@router.post(
    "/execution-connections/{connection_id}/preflight",
    response_model=PreflightResult,
)
def preflight_execution_connection(
    connection_id: UUID,
    request: Request,
    session: Session = Depends(get_session),
) -> PreflightResult:
    item = session.get(ExecutionConnection, connection_id)
    if item is None:
        raise QfError(
            "EXECUTION_CONNECTION_UNKNOWN",
            "Execution connection does not exist.",
            404,
        )
    release = _release_with_capability(
        session,
        item.plugin_release_id,
        {Capability.EXECUTION},
        allow_draining=True,
    )
    credential_set = _credential(
        session,
        item.credential_set_id,
        release.id,
        required=True,
    )
    return _run_preflight(
        session=session,
        settings=request.app.state.settings,
        release=release,
        credential_set=credential_set,
        config=item.config,
        capability=Capability.EXECUTION,
    )
