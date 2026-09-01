"""Single-operator browser authentication endpoints."""

from __future__ import annotations

from urllib.parse import urlparse

import pyotp
from fastapi import APIRouter, Request, Response, status
from fastapi.responses import JSONResponse
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy.exc import IntegrityError

from db.session import SessionFactory
from errors import QfError
from operator_auth import (
    OPERATOR_SUBJECT,
    OperatorAuthRuntime,
    authenticate_browser,
    authenticate_totp_login,
    browser_cookie_epoch,
    has_valid_trusted_browser,
    clear_setup_cookie,
    issue_setup_cookie,
    login_source_key,
    read_setup_cookie,
    require_same_origin,
)
from operator_auth_store import (
    create_binding_if_absent,
    load_canonical_secret,
)
from settings import Settings
from totp_core import matching_totp_step

router = APIRouter(prefix="/api/v1/auth", tags=["auth"])


class LoginInput(BaseModel):
    model_config = ConfigDict(extra="forbid")

    totp_code: str = Field(min_length=6, max_length=6, pattern=r"^[0-9]{6}$")
    trust_browser: bool = False


class SessionView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    authenticated: bool
    username: str
    trusted_browser: bool
    auth_enabled: bool


class AuthBootstrapView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    auth_enabled: bool
    setup_required: bool


class SetupStartView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    issuer: str
    account_name: str
    otpauth_uri: str
    manual_key: str
    expires_in_seconds: int


class SetupConfirmInput(BaseModel):
    model_config = ConfigDict(extra="forbid")

    totp_code: str = Field(min_length=6, max_length=6, pattern=r"^[0-9]{6}$")
    trust_browser: bool = False


def _prevent_auth_response_caching(response: Response) -> None:
    response.headers["Cache-Control"] = "no-store"
    response.headers["Pragma"] = "no-cache"


def _invalid_authentication() -> QfError:
    return QfError(
        "AUTH_INVALID",
        "Operator authentication failed.",
        401,
    )


def _canonical_totp_secret(request: Request) -> str:
    settings: Settings = request.app.state.settings
    factory: SessionFactory = request.app.state.session_factory
    try:
        with factory() as session:
            secret = load_canonical_secret(session, settings)
    except Exception as exc:
        raise QfError(
            "AUTH_CONFIGURATION_INVALID",
            "The Operator authentication configuration is unavailable.",
            500,
        ) from exc
    if secret is None:
        raise QfError(
            "AUTH_SETUP_REQUIRED",
            "Operator authenticator setup is required.",
            409,
        )
    request.app.state.operator_auth_runtime.set_totp_secret(secret)
    return secret


def _setup_completed_response(settings: Settings) -> JSONResponse:
    result = JSONResponse(
        status_code=409,
        content={
            "error": {
                "code": "AUTH_SETUP_ALREADY_COMPLETED",
                "message": "Operator authenticator setup has already been completed.",
                "details": {},
            }
        },
        headers={"Cache-Control": "no-store", "Pragma": "no-cache"},
    )
    clear_setup_cookie(result, settings)
    return result


@router.get("/bootstrap", response_model=AuthBootstrapView)
def auth_bootstrap(request: Request, response: Response) -> AuthBootstrapView:
    settings: Settings = request.app.state.settings
    _prevent_auth_response_caching(response)
    if not settings.auth_enabled:
        return AuthBootstrapView(auth_enabled=False, setup_required=False)
    try:
        with request.app.state.session_factory() as session:
            setup_required = load_canonical_secret(session, settings) is None
    except Exception as exc:
        raise QfError(
            "AUTH_CONFIGURATION_INVALID",
            "The Operator authentication configuration is unavailable.",
            500,
        ) from exc
    return AuthBootstrapView(auth_enabled=True, setup_required=setup_required)


@router.post("/setup/start", response_model=SetupStartView)
def setup_start(request: Request, response: Response) -> SetupStartView | JSONResponse:
    settings: Settings = request.app.state.settings
    _prevent_auth_response_caching(response)
    require_same_origin(request, settings)
    if not settings.auth_enabled:
        raise QfError(
            "AUTH_SETUP_NOT_AVAILABLE",
            "Operator authenticator setup is not available while authentication is disabled.",
            409,
    )
    try:
        with request.app.state.session_factory() as session:
            if load_canonical_secret(session, settings) is not None:
                return _setup_completed_response(settings)
    except Exception as exc:
        if isinstance(exc, QfError):
            raise
        raise QfError(
            "AUTH_CONFIGURATION_INVALID",
            "The Operator authentication configuration is unavailable.",
            500,
        ) from exc

    secret = pyotp.random_base32()
    origin = settings.canonical_auth_public_origin
    host = urlparse(origin or "http://localhost").hostname or "localhost"
    account_name = f"{OPERATOR_SUBJECT}@{host}"
    otpauth_uri = pyotp.TOTP(secret).provisioning_uri(
        name=account_name,
        issuer_name="QuaZonai",
    )
    issue_setup_cookie(response, settings, secret=secret)
    return SetupStartView(
        issuer="QuaZonai",
        account_name=account_name,
        otpauth_uri=otpauth_uri,
        manual_key=secret,
        expires_in_seconds=10 * 60,
    )


@router.post("/setup/confirm", response_model=SessionView)
def setup_confirm(
    payload: SetupConfirmInput,
    request: Request,
    response: Response,
) -> SessionView | JSONResponse:
    settings: Settings = request.app.state.settings
    _prevent_auth_response_caching(response)
    require_same_origin(request, settings)
    if not settings.auth_enabled:
        raise QfError(
            "AUTH_SETUP_NOT_AVAILABLE",
            "Operator authenticator setup is not available while authentication is disabled.",
            409,
        )
    candidate = read_setup_cookie(request, settings)
    if candidate is None:
        raise QfError(
            "AUTH_SETUP_EXPIRED",
            "The authenticator setup has expired. Start setup again.",
            401,
        )

    runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime
    login_cookie_issuance = runtime.cookie_issuance()
    login_browser_epoch = browser_cookie_epoch(request, settings)
    source = login_source_key(request, settings)
    if not runtime.login_limiter.allow_attempt(source):
        raise _invalid_authentication()
    matched = matching_totp_step(settings, payload.totp_code, secret=candidate.secret)
    if matched is None:
        runtime.login_limiter.record_failure(source)
        raise _invalid_authentication()
    step, current_step = matched
    if not runtime.consume_totp_step(
        step,
        current_step=current_step,
        replay_key=candidate.setup_id,
    ):
        runtime.login_limiter.record_failure(source)
        raise _invalid_authentication()

    try:
        with request.app.state.session_factory.begin() as session:
            if load_canonical_secret(session, settings) is not None:
                return _setup_completed_response(settings)
            create_binding_if_absent(session, settings, candidate.secret)
    except IntegrityError:
        return _setup_completed_response(settings)
    except Exception as exc:
        if isinstance(exc, QfError):
            raise
        raise QfError(
            "AUTH_CONFIGURATION_INVALID",
            "The Operator authentication configuration is unavailable.",
            500,
        ) from exc

    runtime.set_totp_secret(candidate.secret)
    # The code that wins enrollment has also authenticated the canonical
    # credential. Consume the same accepted step under the durable credential
    # key so it cannot be replayed immediately through ordinary login.
    if not runtime.consume_totp_step(
        step,
        current_step=current_step,
        replay_key=candidate.secret,
    ):
        raise _invalid_authentication()
    if not runtime.complete_login_if_current(
        response,
        settings,
        cookie_issuance=login_cookie_issuance,
        browser_epoch=login_browser_epoch,
        trust_browser=payload.trust_browser,
    ):
        raise _invalid_authentication()
    runtime.login_limiter.record_success(source)
    clear_setup_cookie(response, settings)
    return SessionView(
        authenticated=True,
        username=OPERATOR_SUBJECT,
        trusted_browser=payload.trust_browser,
        auth_enabled=True,
    )


@router.post("/login", response_model=SessionView)
def login(payload: LoginInput, request: Request, response: Response) -> SessionView:
    settings: Settings = request.app.state.settings
    _prevent_auth_response_caching(response)
    require_same_origin(request, settings)
    if not settings.auth_enabled:
        return SessionView(
            authenticated=True,
            username=OPERATOR_SUBJECT,
            trusted_browser=False,
            auth_enabled=False,
        )

    _canonical_totp_secret(request)
    runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime
    # Snapshot before TOTP verification. A logout that completes while the
    # code is being checked must prevent this request from clearing its
    # barrier or minting a replacement browser session.
    login_cookie_issuance = runtime.cookie_issuance()
    login_browser_epoch = browser_cookie_epoch(request, settings)
    source = login_source_key(request, settings)
    if not runtime.login_limiter.allow_attempt(source):
        raise _invalid_authentication()
    if not authenticate_totp_login(settings, runtime, totp_code=payload.totp_code):
        runtime.login_limiter.record_failure(source)
        raise _invalid_authentication()
    if not runtime.complete_login_if_current(
        response,
        settings,
        cookie_issuance=login_cookie_issuance,
        browser_epoch=login_browser_epoch,
        trust_browser=payload.trust_browser,
    ):
        raise _invalid_authentication()
    runtime.login_limiter.record_success(source)
    return SessionView(
        authenticated=True,
        username=OPERATOR_SUBJECT,
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
            username=OPERATOR_SUBJECT,
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
