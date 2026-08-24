"""System health and operator runtime-configuration endpoints."""

from __future__ import annotations

import secrets
from datetime import UTC, datetime
from typing import Any
from urllib.parse import urlparse

from fastapi import APIRouter, Header, Request
from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator
from sqlalchemy.exc import IntegrityError, SQLAlchemyError
from sqlalchemy.orm import Session

from db.models import PublicMutationReceipt, RuntimeConfiguration
from db.session import ping_database
from errors import QfError
from events import append_event
from runtime_config import (
    codex_api_key_configured,
    effective_settings,
    get_runtime_configuration,
    update_runtime_configuration,
)
from settings import Settings, SettingsError

router = APIRouter(prefix="/api/v1/system", tags=["system"])
RUNTIME_CONFIGURATION_OPERATION = "system.runtime_configuration.replace"
MAX_JOB_POLL_SECONDS = 3600.0


class HealthResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    live: bool
    ready: bool
    database: str
    master_key: str
    plugin_manager: str
    research_worker: str
    codex: str
    details: dict[str, Any]


class RuntimeConfigurationView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    revision: int
    codex_model: str | None
    codex_base_url: str | None
    codex_api_key_configured: bool
    codex_login_configured: bool
    max_plugin_wheel_bytes: int
    plugin_validation_timeout_seconds: int
    bundle_build_timeout_seconds: int
    plugin_job_timeout_seconds: int
    mission_job_timeout_seconds: int
    job_poll_seconds: float
    job_lease_seconds: int
    updated_at: str | None = None


class RuntimeConfigurationInput(BaseModel):
    model_config = ConfigDict(extra="forbid")

    expected_revision: int = Field(ge=0)
    codex_model: str | None = Field(default=None, max_length=200)
    codex_base_url: str | None = Field(default=None, max_length=2048)
    codex_api_key: str | None = Field(default=None, max_length=16_384)
    clear_codex_api_key: bool = False
    max_plugin_wheel_bytes: int = Field(gt=0)
    plugin_validation_timeout_seconds: int = Field(gt=0)
    bundle_build_timeout_seconds: int = Field(gt=0)
    plugin_job_timeout_seconds: int = Field(gt=0)
    mission_job_timeout_seconds: int = Field(gt=0)
    job_poll_seconds: float = Field(ge=0.01, le=MAX_JOB_POLL_SECONDS)
    job_lease_seconds: int = Field(gt=0)

    @field_validator("codex_model")
    @classmethod
    def normalize_model(cls, value: str | None) -> str | None:
        if value is None:
            return None
        clean = value.strip()
        return clean or None

    @field_validator("codex_base_url")
    @classmethod
    def validate_base_url(cls, value: str | None) -> str | None:
        if value is None:
            return None
        clean = value.strip()
        if not clean:
            return None
        parsed = urlparse(clean)
        if parsed.scheme not in {"http", "https"} or not parsed.netloc:
            raise ValueError("Codex base URL must be an absolute HTTP(S) URL")
        if parsed.username or parsed.password:
            raise ValueError("Codex base URL must not contain embedded credentials")
        if parsed.query or parsed.fragment:
            raise ValueError("Codex base URL must not contain a query string or fragment")
        return clean.rstrip("/")

    @field_validator("codex_api_key")
    @classmethod
    def normalize_api_key(cls, value: str | None) -> str | None:
        if value is None:
            return None
        clean = value.strip()
        return clean or None

    @model_validator(mode="after")
    def validate_secret_action(self) -> "RuntimeConfigurationInput":
        if self.clear_codex_api_key and self.codex_api_key:
            raise ValueError("Cannot set and clear the Codex API key in the same request")
        return self


def _runtime_view_from_item(
    settings: Settings,
    item: RuntimeConfiguration | None,
) -> RuntimeConfigurationView:
    if item is None:
        return RuntimeConfigurationView(
            revision=0,
            codex_model=settings.codex_model,
            codex_base_url=settings.codex_base_url,
            codex_api_key_configured=False,
            codex_login_configured=(settings.codex_home / "auth.json").is_file(),
            max_plugin_wheel_bytes=settings.max_plugin_wheel_bytes,
            plugin_validation_timeout_seconds=settings.plugin_validation_timeout_seconds,
            bundle_build_timeout_seconds=settings.bundle_build_timeout_seconds,
            plugin_job_timeout_seconds=settings.plugin_job_timeout_seconds,
            mission_job_timeout_seconds=settings.mission_job_timeout_seconds,
            job_poll_seconds=settings.job_poll_seconds,
            job_lease_seconds=settings.job_lease_seconds,
        )
    return RuntimeConfigurationView(
        revision=item.revision,
        codex_model=item.codex_model,
        codex_base_url=item.codex_base_url,
        codex_api_key_configured=codex_api_key_configured(item),
        codex_login_configured=(settings.codex_home / "auth.json").is_file(),
        max_plugin_wheel_bytes=item.max_plugin_wheel_bytes,
        plugin_validation_timeout_seconds=item.plugin_validation_timeout_seconds,
        bundle_build_timeout_seconds=item.bundle_build_timeout_seconds,
        plugin_job_timeout_seconds=item.plugin_job_timeout_seconds,
        mission_job_timeout_seconds=item.mission_job_timeout_seconds,
        job_poll_seconds=item.job_poll_seconds,
        job_lease_seconds=item.job_lease_seconds,
        updated_at=item.updated_at.isoformat() if item.updated_at else None,
    )


def _runtime_view(request: Request) -> RuntimeConfigurationView:
    settings: Settings = request.app.state.settings
    factory = request.app.state.session_factory
    with factory() as session:
        return _runtime_view_from_item(settings, get_runtime_configuration(session))


def _idempotency_shape(payload: RuntimeConfigurationInput) -> dict[str, Any]:
    normalized = payload.model_dump(mode="json", exclude={"codex_api_key"})
    normalized["codex_api_key_action"] = (
        "clear" if payload.clear_codex_api_key else "set" if payload.codex_api_key else "unchanged"
    )
    return normalized


def _receipt_matches(
    receipt: PublicMutationReceipt,
    payload: RuntimeConfigurationInput,
    settings: Settings,
    session: Any,
) -> bool:
    if receipt.operation_name != RUNTIME_CONFIGURATION_OPERATION:
        return False
    if receipt.normalized_request != _idempotency_shape(payload):
        return False
    if payload.codex_api_key is None:
        return True

    # Do not persist a second encrypted copy of the provider key just to dedupe a
    # retry. An immediate retry can validate the submitted key against the current
    # runtime revision; if later mutations advanced that revision, reusing the old
    # idempotency key is rejected rather than retaining historical secret material.
    response_revision = receipt.response_json.get("revision")
    item = get_runtime_configuration(session)
    if item is None or item.revision != response_revision:
        return False
    current_key = effective_settings(session, settings).codex_api_key
    return current_key is not None and secrets.compare_digest(current_key, payload.codex_api_key)


def _claim_idempotency_receipt(
    session: Session,
    key: str,
    payload: RuntimeConfigurationInput,
) -> tuple[PublicMutationReceipt, bool]:
    """Atomically claim one public mutation key before touching runtime state.

    The unique primary key is the serialization point. On PostgreSQL, a concurrent
    insert for the same key waits for the first transaction to commit or roll back.
    The loser then reads the committed receipt and returns that original response.
    """
    existing = session.get(PublicMutationReceipt, key)
    if existing is not None:
        return existing, False

    receipt = PublicMutationReceipt(
        idempotency_key=key,
        operation_name=RUNTIME_CONFIGURATION_OPERATION,
        normalized_request=_idempotency_shape(payload),
        response_json={},
        status_code=0,
        created_at=datetime.now(UTC),
    )
    try:
        with session.begin_nested():
            session.add(receipt)
            session.flush()
    except IntegrityError as exc:
        if receipt in session:
            session.expunge(receipt)
        session.expire_all()
        existing = session.get(PublicMutationReceipt, key)
        if existing is None:
            raise QfError(
                "IDEMPOTENCY_RECEIPT_CONFLICT",
                "The idempotency receipt could not be resolved after a concurrent request.",
                409,
            ) from exc
        return existing, False
    return receipt, True


@router.get("/runtime-configuration", response_model=RuntimeConfigurationView)
def runtime_configuration(request: Request) -> RuntimeConfigurationView:
    return _runtime_view(request)


@router.put("/runtime-configuration", response_model=RuntimeConfigurationView)
def replace_runtime_configuration(
    payload: RuntimeConfigurationInput,
    request: Request,
    idempotency_key: str | None = Header(
        default=None,
        alias="Idempotency-Key",
        max_length=200,
    ),
) -> RuntimeConfigurationView:
    settings: Settings = request.app.state.settings
    factory = request.app.state.session_factory
    key = idempotency_key.strip() if idempotency_key and idempotency_key.strip() else None
    try:
        with factory.begin() as session:
            claimed_receipt: PublicMutationReceipt | None = None
            if key is not None:
                receipt, claimed = _claim_idempotency_receipt(session, key, payload)
                if not claimed:
                    if not _receipt_matches(receipt, payload, settings, session):
                        raise QfError(
                            "IDEMPOTENCY_KEY_REUSED",
                            "The idempotency key belongs to a different request.",
                            409,
                        )
                    return RuntimeConfigurationView.model_validate(receipt.response_json)
                claimed_receipt = receipt

            item = update_runtime_configuration(
                session,
                settings,
                expected_revision=payload.expected_revision,
                codex_model=payload.codex_model,
                codex_base_url=payload.codex_base_url,
                codex_api_key=payload.codex_api_key,
                clear_codex_api_key=payload.clear_codex_api_key,
                max_plugin_wheel_bytes=payload.max_plugin_wheel_bytes,
                plugin_validation_timeout_seconds=payload.plugin_validation_timeout_seconds,
                bundle_build_timeout_seconds=payload.bundle_build_timeout_seconds,
                plugin_job_timeout_seconds=payload.plugin_job_timeout_seconds,
                mission_job_timeout_seconds=payload.mission_job_timeout_seconds,
                job_poll_seconds=payload.job_poll_seconds,
                job_lease_seconds=payload.job_lease_seconds,
            )
            append_event(
                session,
                kind="RUNTIME_CONFIGURATION_UPDATED",
                aggregate_type="runtime_configuration",
                aggregate_id=item.id,
                actor_kind="LOCAL_OPERATOR",
                payload={
                    "revision": item.revision,
                    "codex_model": item.codex_model,
                    "codex_base_url": item.codex_base_url,
                    "codex_api_key_action": (
                        "cleared"
                        if payload.clear_codex_api_key
                        else "set"
                        if payload.codex_api_key
                        else "unchanged"
                    ),
                    "worker_limits_updated": True,
                },
            )
            session.flush()
            session.refresh(item)
            view = _runtime_view_from_item(settings, item)
            if claimed_receipt is not None:
                claimed_receipt.response_json = view.model_dump(mode="json")
                claimed_receipt.status_code = 200
            return view
    except SettingsError as exc:
        raise QfError(
            "RUNTIME_CONFIGURATION_KEY_UNAVAILABLE",
            "QUAZONAI_MASTER_KEY must be configured before storing a Codex API key.",
            503,
        ) from exc


@router.get("/health", response_model=HealthResponse)
def health(request: Request) -> HealthResponse:
    database_state = "ready"
    details: dict[str, Any] = {}
    try:
        ping_database(request.app.state.engine)
    except SQLAlchemyError as exc:
        database_state = "unavailable"
        details["database_error"] = type(exc).__name__

    settings = request.app.state.settings
    master_key_state = "configured" if settings.master_key_configured else "missing_or_invalid"
    ready = database_state == "ready" and master_key_state == "configured"
    return HealthResponse(
        live=True,
        ready=ready,
        database=database_state,
        master_key=master_key_state,
        plugin_manager="ready" if database_state == "ready" else "unavailable",
        research_worker="not_observed",
        codex="not_observed",
        details=details,
    )
