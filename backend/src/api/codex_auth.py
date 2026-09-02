"""Operator API for database-owned ChatGPT Device Code authentication."""

from __future__ import annotations

from typing import Any, Literal, cast
from uuid import UUID

from fastapi import APIRouter, Request, Response
from pydantic import BaseModel, ConfigDict

from codex_chatgpt_auth import (
    auth_status,
    cancel_device_login,
    disconnect_chatgpt,
    poll_device_login,
    start_device_login,
)
from errors import QfError
from events import append_event

router = APIRouter(prefix="/api/v1/system/codex-auth", tags=["codex-auth"])
_NO_STORE = {"Cache-Control": "no-store", "Pragma": "no-cache"}


class PendingLoginView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    login_id: UUID
    expires_at: str
    poll_after_seconds: int


class CodexChatgptAuthStatus(BaseModel):
    model_config = ConfigDict(extra="forbid")

    state: Literal["DISCONNECTED", "CONNECTED", "REAUTH_REQUIRED"]
    active: bool
    email: str | None
    plan_type: str | None
    authenticated_at: str | None
    last_refresh_at: str | None
    reauth_required_at: str | None
    pending_login: PendingLoginView | None
    legacy_auth_file_present: bool


class DeviceLoginStartResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    login_id: UUID
    status: Literal["PENDING"]
    verification_url: Literal["https://auth.openai.com/codex/device"]
    user_code: str
    expires_at: str
    poll_after_seconds: int


class DeviceLoginPollResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    status: Literal["PENDING", "SUCCEEDED", "CANCELLED", "EXPIRED", "FAILED"]
    expires_at: str | None = None
    poll_after_seconds: int | None = None
    auth: CodexChatgptAuthStatus | None = None
    error_code: str | None = None


def _no_store(response: Response) -> None:
    response.headers.update(_NO_STORE)


def _require_json_request(request: Request) -> None:
    media_type = request.headers.get("content-type", "").partition(";")[0].strip().casefold()
    if media_type != "application/json":
        raise QfError(
            "CODEX_CHATGPT_JSON_REQUIRED",
            "ChatGPT device login start requires an application/json request.",
            415,
        )


@router.get("", response_model=CodexChatgptAuthStatus)
def get_codex_auth(request: Request, response: Response) -> dict[str, Any]:
    _no_store(response)
    with request.app.state.session_factory() as session:
        return auth_status(session, request.app.state.settings)


@router.post("/chatgpt/device/start", response_model=DeviceLoginStartResponse)
def start_chatgpt_device_login(
    request: Request,
    response: Response,
) -> DeviceLoginStartResponse:
    _no_store(response)
    _require_json_request(request)
    factory = request.app.state.session_factory
    settings = request.app.state.settings
    with factory() as session:
        view = start_device_login(session, settings)
        if view.created:
            append_event(
                session,
                kind="CODEX_CHATGPT_AUTH_LOGIN_STARTED",
                aggregate_type="CODEX_CHATGPT_AUTH",
                aggregate_id=view.login_id,
                payload={"auth_mode": "CHATGPT"},
                actor_kind="LOCAL_OPERATOR",
            )
        session.commit()
    return DeviceLoginStartResponse(
        login_id=view.login_id,
        status="PENDING",
        verification_url=cast(Literal["https://auth.openai.com/codex/device"], view.verification_url),
        user_code=view.user_code,
        expires_at=view.expires_at.isoformat(),
        poll_after_seconds=view.poll_after_seconds,
    )


@router.post("/chatgpt/device/{login_id}/poll", response_model=DeviceLoginPollResponse)
def poll_chatgpt_device_login(
    login_id: UUID,
    request: Request,
    response: Response,
) -> DeviceLoginPollResponse:
    _no_store(response)
    factory = request.app.state.session_factory
    settings = request.app.state.settings
    with factory() as session:
        result = poll_device_login(session, settings, login_id)
    if result.status == "SUCCEEDED":
        with factory.begin() as session:
            append_event(
                session,
                kind="CODEX_CHATGPT_AUTH_CONNECTED",
                aggregate_type="CODEX_CHATGPT_AUTH",
                aggregate_id=login_id,
                payload={"auth_mode": "CHATGPT"},
                actor_kind="LOCAL_OPERATOR",
            )
    return DeviceLoginPollResponse(
        status=cast(Literal["PENDING", "SUCCEEDED", "CANCELLED", "EXPIRED", "FAILED"], result.status),
        expires_at=result.expires_at.isoformat() if result.expires_at else None,
        poll_after_seconds=result.poll_after_seconds,
        auth=CodexChatgptAuthStatus.model_validate(result.auth) if result.auth else None,
        error_code=result.error_code,
    )


@router.delete("/chatgpt/device/{login_id}", response_model=DeviceLoginPollResponse)
def cancel_chatgpt_device_login(
    login_id: UUID,
    request: Request,
    response: Response,
) -> DeviceLoginPollResponse:
    _no_store(response)
    with request.app.state.session_factory.begin() as session:
        result = cancel_device_login(session, login_id)
        if result.status == "CANCELLED":
            append_event(
                session,
                kind="CODEX_CHATGPT_AUTH_LOGIN_CANCELLED",
                aggregate_type="CODEX_CHATGPT_AUTH",
                aggregate_id=login_id,
                payload={"auth_mode": "CHATGPT"},
                actor_kind="LOCAL_OPERATOR",
            )
    return DeviceLoginPollResponse(
        status=cast(Literal["PENDING", "SUCCEEDED", "CANCELLED", "EXPIRED", "FAILED"], result.status),
        expires_at=result.expires_at.isoformat() if result.expires_at else None,
        poll_after_seconds=result.poll_after_seconds,
        error_code=result.error_code,
    )


@router.delete("/chatgpt", response_model=CodexChatgptAuthStatus)
def disconnect_chatgpt_auth(request: Request, response: Response) -> dict[str, Any]:
    _no_store(response)
    settings = request.app.state.settings
    with request.app.state.session_factory() as session:
        disconnect_chatgpt(session)
        append_event(
            session,
            kind="CODEX_CHATGPT_AUTH_DISCONNECTED",
            aggregate_type="CODEX_CHATGPT_AUTH",
            aggregate_id=None,
            payload={"auth_mode": "CHATGPT"},
            actor_kind="LOCAL_OPERATOR",
        )
        session.commit()
    try:
        legacy_path = settings.codex_home / "auth.json"
        if legacy_path.exists():
            legacy_path.unlink()
    except OSError as exc:
        raise QfError(
            "CODEX_LEGACY_AUTH_CLEANUP_FAILED",
            "The legacy Codex auth file could not be removed; official Codex login is disabled.",
            503,
        ) from exc
    with request.app.state.session_factory() as session:
        return auth_status(session, settings)


__all__ = ["router"]
