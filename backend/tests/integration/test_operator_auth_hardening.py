from __future__ import annotations

import asyncio
import base64
from dataclasses import replace

import main as main_module
import operator_auth
import pyotp
import pytest
from fastapi import Request
from fastapi.testclient import TestClient
from sqlalchemy import Engine

from api.events import _stream_authorized, stream_events
from main import create_app
from operator_auth import (
    SESSION_COOKIE_NAME,
    OperatorAuthRuntime,
    OperatorLoginLimiter,
    STREAM_ADMISSION_GENERATION_STATE_ATTRIBUTE,
)
from settings import Settings


def _enabled_settings(settings: Settings) -> Settings:
    return replace(
        settings,
        operator_auth_enabled=True,
        operator_username="operator",
        operator_password="correct horse battery staple",
        operator_totp_secret=pyotp.random_base32(),
        auth_cookie_key=base64.b64encode(b"a" * 32).decode("ascii"),
        api_token="machine-token-" + "x" * 32,
        auth_public_origin="http://testserver",
    )


def _payload(
    settings: Settings,
    *,
    password: str,
    totp_code: str | None = None,
) -> dict[str, object]:
    assert settings.operator_totp_secret is not None
    return {
        "username": "operator",
        "password": password,
        "totp_code": totp_code or pyotp.TOTP(settings.operator_totp_secret).now(),
        "trust_browser": False,
    }


def _request_with_session(app: object, session_cookie: str) -> Request:
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
                )
            ],
            "client": ("203.0.113.10", 43210),
            "server": ("testserver", 80),
            "app": app,
        },
        receive=receive,
    )


def test_login_backoff_uses_same_generic_failure_and_recovers(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    now = [100.0]
    limiter = OperatorLoginLimiter(
        clock=lambda: now[0],
        minimum_interval_seconds=1.0,
        base_backoff_seconds=1.0,
        maximum_backoff_seconds=5.0,
    )
    app = create_app(settings=secured, engine=engine)
    app.state.operator_auth_runtime = OperatorAuthRuntime(login_limiter=limiter)
    client = TestClient(app)

    invalid = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(secured, password="wrong-password-value"),
    )
    throttled = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(secured, password="correct horse battery staple"),
    )

    assert invalid.status_code == 401
    assert throttled.status_code == 401
    assert throttled.json() == invalid.json()

    now[0] += 1.01
    recovered = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(secured, password="correct horse battery staple"),
    )
    assert recovered.status_code == 200


def test_login_backoff_is_bounded_without_durable_lockout() -> None:
    now = [0.0]
    limiter = OperatorLoginLimiter(
        clock=lambda: now[0],
        minimum_interval_seconds=1.0,
        base_backoff_seconds=1.0,
        maximum_backoff_seconds=5.0,
    )

    for _ in range(8):
        assert limiter.allow_attempt("203.0.113.10")
        limiter.record_failure("203.0.113.10")
        now[0] += 5.01

    assert limiter.allow_attempt("203.0.113.10")
    limiter.record_failure("203.0.113.10")
    now[0] += 4.99
    assert not limiter.allow_attempt("203.0.113.10")
    now[0] += 0.02
    assert limiter.allow_attempt("203.0.113.10")


def test_logout_terminates_preexisting_stream_authorization(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)

    login = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(secured, password="correct horse battery staple"),
    )
    assert login.status_code == 200
    session_cookie = client.cookies.get(SESSION_COOKIE_NAME)
    assert session_cookie is not None

    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    generation = runtime.stream_generation()
    stream_request = _request_with_session(app, session_cookie)
    assert _stream_authorized(stream_request, generation)

    logout = client.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )
    assert logout.status_code == 204
    assert not _stream_authorized(stream_request, generation)


def test_anonymous_logout_does_not_revoke_other_active_streams(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    generation = runtime.stream_generation()
    anonymous = TestClient(app)

    response = anonymous.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )

    assert response.status_code == 204
    assert runtime.stream_generation() == generation


def test_stream_admission_generation_captured_by_middleware_closes_logout_race(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)
    login = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(secured, password="correct horse battery staple"),
    )
    assert login.status_code == 200
    session_cookie = client.cookies.get(SESSION_COOKIE_NAME)
    assert session_cookie is not None

    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    stream_request = _request_with_session(app, session_cookie)
    # Model the authenticated middleware admission, then let logout complete
    # before the endpoint constructs its generator.
    setattr(
        stream_request.state,
        STREAM_ADMISSION_GENERATION_STATE_ATTRIBUTE,
        runtime.stream_generation(),
    )
    runtime.revoke_active_streams()
    response = stream_events(stream_request, cursor=0)

    async def read_first_frame() -> bytes | str:
        return await anext(response.body_iterator)

    with pytest.raises(StopAsyncIteration):
        asyncio.run(read_first_frame())


def test_middleware_captures_stream_generation_before_authentication_race(
    settings: Settings,
    engine: Engine,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)
    login = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(secured, password="correct horse battery staple"),
    )
    assert login.status_code == 200

    @app.get("/api/v1/test-auth-admission")
    def auth_admission(request: Request) -> dict[str, int]:
        return {
            "generation": getattr(
                request.state,
                STREAM_ADMISSION_GENERATION_STATE_ATTRIBUTE,
            )
        }

    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    admission_generation = runtime.stream_generation()
    original_authenticate_browser = main_module.authenticate_browser

    def authenticate_then_logout(
        request: Request,
        configured: Settings,
    ) -> object:
        identity = original_authenticate_browser(request, configured)
        runtime.revoke_active_streams()
        return identity

    monkeypatch.setattr(main_module, "authenticate_browser", authenticate_then_logout)
    response = client.get("/api/v1/test-auth-admission")

    assert response.status_code == 200
    assert response.json() == {"generation": admission_generation}
    assert runtime.stream_generation() == admission_generation + 1


def test_stream_revalidation_observes_cookie_key_rotation(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)

    login = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(secured, password="correct horse battery staple"),
    )
    assert login.status_code == 200
    session_cookie = client.cookies.get(SESSION_COOKIE_NAME)
    assert session_cookie is not None

    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    generation = runtime.stream_generation()
    stream_request = _request_with_session(app, session_cookie)
    assert _stream_authorized(stream_request, generation)

    app.state.settings = replace(
        secured,
        auth_cookie_key=base64.b64encode(b"b" * 32).decode("ascii"),
    )
    assert not _stream_authorized(stream_request, generation)


def test_stream_revalidation_observes_session_expiry(
    settings: Settings,
    engine: Engine,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    now = [1_700_000_000.0]
    monkeypatch.setattr(operator_auth.time, "time", lambda: now[0])
    secured = replace(_enabled_settings(settings), auth_session_ttl_seconds=300)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)

    login = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            password="correct horse battery staple",
            totp_code=pyotp.TOTP(secured.operator_totp_secret).at(int(now[0])),
        ),
    )
    assert login.status_code == 200
    session_cookie = client.cookies.get(SESSION_COOKIE_NAME)
    assert session_cookie is not None

    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    generation = runtime.stream_generation()
    stream_request = _request_with_session(app, session_cookie)
    assert _stream_authorized(stream_request, generation)

    now[0] += 301
    assert not _stream_authorized(stream_request, generation)
