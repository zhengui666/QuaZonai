"""Single-operator browser authentication endpoints."""

from __future__ import annotations

from fastapi import APIRouter, Request, Response, status
from pydantic import BaseModel, ConfigDict, Field

from errors import QfError
from operator_auth import (
    OperatorAuthRuntime,
    authenticate_browser,
    browser_cookie_epoch,
    has_valid_trusted_browser,
    login_source_key,
    require_same_origin,
)
from settings import (
    MAX_OPERATOR_PASSWORD_CHARACTERS,
    MAX_OPERATOR_USERNAME_CHARACTERS,
    Settings,
)
from totp_core import constant_time_text_equal, verify_totp_once

router = APIRouter(prefix="/api/v1/auth", tags=["auth"])


class LoginInput(BaseModel):
    model_config = ConfigDict(extra="forbid")

    username: str = Field(min_length=1, max_length=MAX_OPERATOR_USERNAME_CHARACTERS)
    password: str = Field(min_length=1, max_length=MAX_OPERATOR_PASSWORD_CHARACTERS)
    totp_code: str = Field(min_length=6, max_length=32)
    trust_browser: bool = False


class SessionView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    authenticated: bool
    username: str
    trusted_browser: bool
    auth_enabled: bool


def _prevent_auth_response_caching(response: Response) -> None:
    response.headers["Cache-Control"] = "no-store"
    response.headers["Pragma"] = "no-cache"


def _invalid_credentials() -> QfError:
    return QfError(
        "AUTH_INVALID",
        "Invalid operator credentials.",
        401,
    )


@router.post("/login", response_model=SessionView)
def login(payload: LoginInput, request: Request, response: Response) -> SessionView:
    settings: Settings = request.app.state.settings
    _prevent_auth_response_caching(response)
    require_same_origin(request, settings)
    if not settings.auth_enabled:
        return SessionView(
            authenticated=True,
            username="local-operator",
            trusted_browser=False,
            auth_enabled=False,
        )

    runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime
    login_cookie_issuance = runtime.cookie_issuance()
    login_browser_epoch = browser_cookie_epoch(request, settings)
    source = login_source_key(request, settings)
    if not runtime.login_limiter.allow_attempt(source):
        raise _invalid_credentials()

    assert settings.operator_username is not None
    assert settings.operator_password is not None
    primary_factors_valid = constant_time_text_equal(
        payload.username,
        settings.operator_username,
    ) and constant_time_text_equal(payload.password, settings.operator_password)
    if not primary_factors_valid or not verify_totp_once(settings, runtime, payload.totp_code):
        runtime.login_limiter.record_failure(source)
        raise _invalid_credentials()

    if not runtime.complete_login_if_current(
        response,
        settings,
        cookie_issuance=login_cookie_issuance,
        browser_epoch=login_browser_epoch,
        trust_browser=payload.trust_browser,
    ):
        raise _invalid_credentials()
    runtime.login_limiter.record_success(source)
    return SessionView(
        authenticated=True,
        username=settings.operator_username,
        trusted_browser=payload.trust_browser,
        auth_enabled=True,
    )


@router.get("/session", response_model=SessionView)
def session(request: Request, response: Response) -> SessionView:
    settings: Settings = request.app.state.settings
    _prevent_auth_response_caching(response)
    if not settings.auth_enabled:
        return SessionView(
            authenticated=True,
            username="local-operator",
            trusted_browser=False,
            auth_enabled=False,
        )
    runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime
    renewal_cookie_issuance = runtime.cookie_issuance()
    renewal_browser_epoch = browser_cookie_epoch(request, settings)
    identity = authenticate_browser(request, settings)
    if identity is None:
        raise QfError(
            "AUTH_REQUIRED",
            "Operator authentication is required.",
            401,
        )
    if identity.renew_session:
        if not runtime.renew_session_if_current(
            response,
            settings,
            cookie_issuance=renewal_cookie_issuance,
            browser_epoch=renewal_browser_epoch,
        ):
            raise QfError(
                "AUTH_REQUIRED",
                "Operator authentication is required.",
                401,
            )
    return SessionView(
        authenticated=True,
        username=identity.username,
        trusted_browser=has_valid_trusted_browser(request, settings),
        auth_enabled=True,
    )


@router.post("/logout", status_code=status.HTTP_204_NO_CONTENT)
def logout(request: Request, response: Response) -> None:
    settings: Settings = request.app.state.settings
    _prevent_auth_response_caching(response)
    require_same_origin(request, settings)
    browser_identity = authenticate_browser(request, settings)
    runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime
    runtime.complete_logout(
        response,
        settings,
        revoke_streams=browser_identity is not None,
    )
