"""System health and operator runtime-configuration endpoints."""

from __future__ import annotations

from datetime import UTC, datetime
from typing import Any, Literal, cast
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
    CODEX_REASONING_EFFORTS,
    codex_api_key_configured,
    get_runtime_configuration,
    update_runtime_configuration,
)
from settings import Settings, SettingsError

router = APIRouter(prefix="/api/v1/system", tags=["system"])
RUNTIME_CONFIGURATION_OPERATION = "system.runtime_configuration.replace"
CodexReasoningEffort = Literal["minimal", "low", "medium", "high", "xhigh"]
MAX_PLUGIN_WHEEL_BYTES = 1_073_741_824
MAX_WORKER_TIMEOUT_SECONDS = 86_400
MAX_JOB_POLL_SECONDS = 3600.0
MAX_JOB_LEASE_SECONDS = 86_400


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
    codex_reasoning_effort: CodexReasoningEffort | None
    codex_fast_mode: bool
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
    codex_reasoning_effort: CodexReasoningEffort | None = None
    codex_fast_mode: bool = False
    codex_base_url: str | None = Field(default=None, max_length=2048)
    codex_api_key: str | None = Field(default=None, max_length=16_384)
    clear_codex_api_key: bool = False
    max_plugin_wheel_bytes: int = Field(gt=0, le=MAX_PLUGIN_WHEEL_BYTES)
    plugin_validation_timeout_seconds: int = Field(gt=0, le=MAX_WORKER_TIMEOUT_SECONDS)
    bundle_build_timeout_seconds: int = Field(gt=0, le=MAX_WORKER_TIMEOUT_SECONDS)
    plugin_job_timeout_seconds: int = Field(gt=0, le=MAX_WORKER_TIMEOUT_SECONDS)
    mission_job_timeout_seconds: int = Field(gt=0, le=MAX_WORKER_TIMEOUT_SECONDS)
    job_poll_seconds: float = Field(ge=0.01, le=MAX_JOB_POLL_SECONDS)
    job_lease_seconds: int = Field(gt=0, le=MAX_JOB_LEASE_SECONDS)

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

    @field_validator("codex_reasoning_effort")
    @classmethod
    def validate_reasoning_effort(cls, value: CodexReasoningEffort | None) -> CodexReasoningEffort | None:
        if value is not None and value not in CODEX_REASONING_EFFORTS:
            raise ValueError("Unsupported Codex reasoning effort")
        return value


def _runtime_view_from_item(
    settings: Settings,
    item: RuntimeConfiguration | None,
) -> RuntimeConfigurationView:
    if item is None:
        return RuntimeConfigurationView(
            revision=0,
            codex_model=settings.codex_model,
            codex_reasoning_effort=cast(CodexReasoningEffort | None, settings.codex_reasoning_effort),
            codex_fast_mode=settings.codex_fast_mode,
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
        codex_reasoning_effort=cast(CodexReasoningEffort | None, item.codex_reasoning_effort),
        codex_fast_mode=item.codex_fast_mode,
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
    """Normalize a runtime save without retaining a write-only secret value.

    The Idempotency-Key identifies the logical save. For Codex credentials, the
    normalized request records only whether the logical action was set, clear, or
    unchanged. Reusing that key never performs another mutation, even if a caller
    later supplies a different secret value with the otherwise-identical payload.
    This preserves retry semantics without retaining historical provider secrets.
    """
    normalized = payload.model_dump(
        mode="json",
        exclude={"codex_api_key", "codex_reasoning_effort", "codex_fast_mode"},
    )
    normalized["codex_api_key_action"] = (
        "clear" if payload.clear_codex_api_key else "set" if payload.codex_api_key else "unchanged"
    )
    normalized["codex_reasoning_effort_action"] = (
        "unchanged"
        if "codex_reasoning_effort" not in payload.model_fields_set
        else "inherit-default"
        if payload.codex_reasoning_effort is None
        else f"set:{payload.codex_reasoning_effort}"
    )
    normalized["codex_fast_mode_action"] = (
        "unchanged"
        if "codex_fast_mode" not in payload.model_fields_set
        else "fast"
        if payload.codex_fast_mode
        else "standard"
    )
    return normalized


def _receipt_matches(
    receipt: PublicMutationReceipt,
    payload: RuntimeConfigurationInput,
) -> bool:
    return (
        receipt.operation_name == RUNTIME_CONFIGURATION_OPERATION
        and receipt.normalized_request == _idempotency_shape(payload)
    )


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
                    if not _receipt_matches(receipt, payload):
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
                codex_reasoning_effort=payload.codex_reasoning_effort,
                replace_codex_reasoning_effort="codex_reasoning_effort" in payload.model_fields_set,
                codex_fast_mode=payload.codex_fast_mode,
                replace_codex_fast_mode="codex_fast_mode" in payload.model_fields_set,
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
                    "codex_reasoning_effort": item.codex_reasoning_effort,
                    "codex_reasoning_effort_action": (
                        "unchanged"
                        if "codex_reasoning_effort" not in payload.model_fields_set
                        else "inherit-default"
                        if payload.codex_reasoning_effort is None
                        else "set"
                    ),
                    "codex_fast_mode": item.codex_fast_mode,
                    "codex_fast_mode_action": (
                        "unchanged"
                        if "codex_fast_mode" not in payload.model_fields_set
                        else "fast"
                        if payload.codex_fast_mode
                        else "standard"
                    ),
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
