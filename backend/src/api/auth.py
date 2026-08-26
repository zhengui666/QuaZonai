"""Single-operator browser authentication endpoints."""

from __future__ import annotations

from fastapi import APIRouter, Request, Response, status
from pydantic import BaseModel, ConfigDict, Field

from errors import QfError
from operator_auth import (
    OperatorAuthRuntime,
    authenticate_browser,
    authenticate_login,
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
    # Snapshot before credential verification. A logout that completes while the
    # factors are being checked must prevent this request from clearing its
    # barrier or minting a replacement browser session.
    login_cookie_generation = runtime.cookie_generation()
    login_browser_epoch = browser_cookie_epoch(request, settings)
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
    if not runtime.complete_login_if_current(
        response,
        settings,
        cookie_generation=login_cookie_generation,
        browser_epoch=login_browser_epoch,
        trust_browser=payload.trust_browser,
    ):
        # Keep the public failure shape identical to incorrect credentials. In
        # particular, do not return a successful SessionView without its cookie.
        raise _invalid_credentials()
    runtime.login_limiter.record_success(source)
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
    renewal_cookie_generation = runtime.cookie_generation()
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
            cookie_generation=renewal_cookie_generation,
            browser_epoch=renewal_browser_epoch,
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
    runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime
    # This also covers an anonymous request or a request that authenticated a
    # trusted-browser credential just before expiry. `complete_logout` commits
    # the barrier and cookie epoch atomically, while only a valid browser
    # identity revokes active streams.
    runtime.complete_logout(
        response,
        settings,
        revoke_streams=browser_identity is not None,
    )
