"""System health and operator runtime-configuration endpoints."""

from __future__ import annotations

from typing import Any
from urllib.parse import urlparse

from fastapi import APIRouter, Request
from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator
from sqlalchemy.exc import SQLAlchemyError

from db.session import ping_database
from errors import QfError
from events import append_event
from runtime_config import (
    codex_api_key_configured,
    get_runtime_configuration,
    update_runtime_configuration,
)
from settings import Settings, SettingsError

router = APIRouter(prefix="/api/v1/system", tags=["system"])


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

    codex_model: str | None = Field(default=None, max_length=200)
    codex_base_url: str | None = Field(default=None, max_length=2048)
    codex_api_key: str | None = Field(default=None, max_length=16_384)
    clear_codex_api_key: bool = False
    max_plugin_wheel_bytes: int = Field(gt=0)
    plugin_validation_timeout_seconds: int = Field(gt=0)
    bundle_build_timeout_seconds: int = Field(gt=0)
    plugin_job_timeout_seconds: int = Field(gt=0)
    mission_job_timeout_seconds: int = Field(gt=0)
    job_poll_seconds: float = Field(gt=0)
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


def _runtime_view(request: Request) -> RuntimeConfigurationView:
    settings: Settings = request.app.state.settings
    factory = request.app.state.session_factory
    with factory() as session:
        item = get_runtime_configuration(session)
        if item is None:
            return RuntimeConfigurationView(
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


@router.get("/runtime-configuration", response_model=RuntimeConfigurationView)
def runtime_configuration(request: Request) -> RuntimeConfigurationView:
    return _runtime_view(request)


@router.put("/runtime-configuration", response_model=RuntimeConfigurationView)
def replace_runtime_configuration(
    payload: RuntimeConfigurationInput,
    request: Request,
) -> RuntimeConfigurationView:
    settings: Settings = request.app.state.settings
    factory = request.app.state.session_factory
    try:
        with factory.begin() as session:
            item = update_runtime_configuration(
                session,
                settings,
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
    except SettingsError as exc:
        raise QfError(
            "RUNTIME_CONFIGURATION_KEY_UNAVAILABLE",
            "QUAZONAI_MASTER_KEY must be configured before storing a Codex API key.",
            503,
        ) from exc
    return _runtime_view(request)


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
