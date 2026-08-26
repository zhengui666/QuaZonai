"""Single-operator browser authentication endpoints."""

from __future__ import annotations

from fastapi import APIRouter, Request, Response, status
from pydantic import BaseModel, ConfigDict, Field

from errors import QfError
from operator_auth import (
    OperatorAuthRuntime,
    authenticate_browser,
    authenticate_login,
    clear_auth_cookies,
    clear_logout_barrier_cookie,
    clear_trusted_browser_cookie,
    has_valid_trusted_browser,
    login_source_key,
    require_same_origin,
    set_session_cookie,
    set_logout_barrier_cookie,
    set_trusted_browser_cookie,
)
from settings import (
    MAX_OPERATOR_PASSWORD_CHARACTERS,
    MAX_OPERATOR_USERNAME_CHARACTERS,
    Settings,
)

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
    source = login_source_key(request, settings)
    if not runtime.login_limiter.allow_attempt(source):
        raise _invalid_credentials()
    if not authenticate_login(
        settings,
        runtime,
        username=payload.username,
        password=payload.password,
        totp_code=payload.totp_code,
    ):
        runtime.login_limiter.record_failure(source)
        raise _invalid_credentials()
    runtime.login_limiter.record_success(source)

    # Only a successful password + TOTP login clears the browser-local logout
    # barrier; automatic trusted-browser renewal intentionally cannot do this.
    clear_logout_barrier_cookie(response, settings)
    set_session_cookie(response, settings)
    if payload.trust_browser:
        set_trusted_browser_cookie(response, settings)
    else:
        clear_trusted_browser_cookie(response, settings)
    assert settings.operator_username is not None
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
    # Snapshot before parsing the trusted credential. If logout wins while this
    # request is authenticating, its renewal must not write a fresh session cookie.
    renewal_generation = runtime.stream_generation()
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
            generation=renewal_generation,
        ):
            # A logout revoked this trusted-browser credential after it was
            # parsed. Do not report a usable session when its renewal lost the
            # revocation race: AuthGate treats successful probes as current.
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
    if browser_identity is not None:
        runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime
        runtime.revoke_active_streams()
    if settings.auth_enabled:
        # This also covers a request that authenticated a trusted-browser credential
        # just before it expired and is still preparing a renewal response. The
        # barrier is browser-local and same-origin logout is the operator's explicit
        # intent to prevent that response from restoring access.
        set_logout_barrier_cookie(response, settings)
    clear_auth_cookies(response, settings)
