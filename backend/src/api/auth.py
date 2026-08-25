"""Single-operator browser authentication endpoints."""

from __future__ import annotations

from fastapi import APIRouter, Request, Response, status
from pydantic import BaseModel, ConfigDict, Field

from errors import QfError
from operator_auth import (
    TRUSTED_BROWSER_COOKIE_NAME,
    authenticate_browser,
    authenticate_login,
    clear_auth_cookies,
    clear_trusted_browser_cookie,
    require_same_origin,
    set_session_cookie,
    set_trusted_browser_cookie,
)
from settings import Settings

router = APIRouter(prefix="/api/v1/auth", tags=["auth"])


class LoginInput(BaseModel):
    model_config = ConfigDict(extra="forbid")

    username: str = Field(min_length=1, max_length=200)
    password: str = Field(min_length=1, max_length=4096)
    totp_code: str = Field(min_length=6, max_length=32)
    trust_browser: bool = False


class SessionView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    authenticated: bool
    username: str
    trusted_browser: bool
    auth_enabled: bool


@router.post("/login", response_model=SessionView)
def login(payload: LoginInput, request: Request, response: Response) -> SessionView:
    settings: Settings = request.app.state.settings
    require_same_origin(request, settings)
    if not settings.auth_enabled:
        return SessionView(
            authenticated=True,
            username="local-operator",
            trusted_browser=False,
            auth_enabled=False,
        )
    if not authenticate_login(
        settings,
        username=payload.username,
        password=payload.password,
        totp_code=payload.totp_code,
    ):
        raise QfError(
            "AUTH_INVALID",
            "Invalid operator credentials.",
            401,
        )
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
    if not settings.auth_enabled:
        return SessionView(
            authenticated=True,
            username="local-operator",
            trusted_browser=False,
            auth_enabled=False,
        )
    identity = authenticate_browser(request, settings)
    if identity is None:
        raise QfError(
            "AUTH_REQUIRED",
            "Operator authentication is required.",
            401,
        )
    if identity.renew_session:
        set_session_cookie(response, settings)
    return SessionView(
        authenticated=True,
        username=identity.username,
        trusted_browser=(
            identity.source == "trusted_browser"
            or bool(request.cookies.get(TRUSTED_BROWSER_COOKIE_NAME))
        ),
        auth_enabled=True,
    )


@router.post("/logout", status_code=status.HTTP_204_NO_CONTENT)
def logout(request: Request, response: Response) -> None:
    settings: Settings = request.app.state.settings
    require_same_origin(request, settings)
    clear_auth_cookies(response, settings)
