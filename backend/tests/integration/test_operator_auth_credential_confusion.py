from __future__ import annotations

import base64
from dataclasses import replace

import pyotp
from fastapi import Request
from fastapi.testclient import TestClient
from sqlalchemy import Engine

from api.events import _stream_authorized
from main import create_app
from operator_auth import SESSION_COOKIE_NAME, OperatorAuthRuntime
from settings import Settings


def _enabled_settings(settings: Settings) -> Settings:
    return replace(
        settings,
        operator_auth_enabled=True,
        operator_totp_secret=pyotp.random_base32(),
        auth_cookie_key=base64.b64encode(b"a" * 32).decode("ascii"),
        api_token="machine-token-" + "x" * 32,
        auth_public_origin="http://testserver",
    )


def _login(client: TestClient, settings: Settings) -> None:
    assert settings.operator_totp_secret is not None
    response = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json={
            "totp_code": pyotp.TOTP(settings.operator_totp_secret).now(),
            "trust_browser": True,
        },
    )
    assert response.status_code == 200


def _stream_request(app: object, *, session_cookie: str, authorization: str) -> Request:
    async def receive() -> dict[str, object]:
        return {"type": "http.request", "body": b"", "more_body": False}

    return Request(
        {
            "type": "http",
            "http_version": "1.1",
            "method": "GET",
            "scheme": "http",
            "path": "/api/v1/events/stream",
            "raw_path": b"/api/v1/events/stream",
            "query_string": b"",
            "headers": [
                (
                    b"cookie",
                    f"{SESSION_COOKIE_NAME}={session_cookie}".encode("ascii"),
                ),
                (b"authorization", authorization.encode("ascii")),
            ],
            "client": ("203.0.113.10", 43210),
            "server": ("testserver", 80),
            "app": app,
        },
        receive=receive,
    )


def test_invalid_explicit_bearer_does_not_fall_back_to_valid_browser_cookie(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    client = TestClient(create_app(settings=secured, engine=engine))
    _login(client, secured)

    response = client.get(
        "/api/v1/system/runtime-configuration",
        headers={"Authorization": "Bearer not-the-machine-token"},
    )

    assert response.status_code == 401
    assert response.json()["error"]["code"] == "AUTH_REQUIRED"


def test_malformed_explicit_authorization_does_not_use_ambient_cookies(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    client = TestClient(create_app(settings=secured, engine=engine))
    _login(client, secured)

    response = client.get(
        "/api/v1/system/runtime-configuration",
        headers={"Authorization": "Basic accidental-credential"},
    )

    assert response.status_code == 401
    assert response.json()["error"]["code"] == "AUTH_REQUIRED"


def test_machine_authenticated_stream_does_not_fall_back_to_cookie_after_token_rotation(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)
    _login(client, secured)
    session_cookie = client.cookies.get(SESSION_COOKIE_NAME)
    assert session_cookie is not None
    assert secured.api_token is not None

    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    generation = runtime.stream_generation()
    request = _stream_request(
        app,
        session_cookie=session_cookie,
        authorization=f"Bearer {secured.api_token}",
    )
    assert _stream_authorized(request, generation)

    app.state.settings = replace(
        secured,
        api_token="machine-token-" + "y" * 32,
    )
    assert not _stream_authorized(request, generation)
