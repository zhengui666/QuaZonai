from __future__ import annotations

import asyncio
import base64
import time
from collections.abc import Callable
from dataclasses import replace
from threading import Event, Thread
from types import ModuleType

import main as main_module
import operator_auth
import pyotp
import pytest
from api import auth as auth_module
from fastapi import Request
from fastapi.testclient import TestClient
from httpx import Response as HttpxResponse
from sqlalchemy import Engine

from api.events import _stream_authorized, stream_events
from main import create_app
from operator_auth import (
    LOGOUT_BARRIER_COOKIE_NAME,
    OperatorIdentity,
    SESSION_COOKIE_NAME,
    TRUSTED_BROWSER_COOKIE_NAME,
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
    trust_browser: bool = False,
) -> dict[str, object]:
    assert settings.operator_totp_secret is not None
    return {
        "username": "operator",
        "password": password,
        "totp_code": totp_code or pyotp.TOTP(settings.operator_totp_secret).now(),
        "trust_browser": trust_browser,
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


def _trusted_renewal_clients(app: object, client: TestClient) -> tuple[TestClient, TestClient]:
    session_cookie = client.cookies.get(SESSION_COOKIE_NAME)
    trusted_cookie = client.cookies.get(TRUSTED_BROWSER_COOKIE_NAME)
    assert session_cookie is not None
    assert trusted_cookie is not None

    renewal_client = TestClient(app)
    renewal_client.cookies.set(TRUSTED_BROWSER_COOKIE_NAME, trusted_cookie)
    logout_client = TestClient(app)
    logout_client.cookies.set(SESSION_COOKIE_NAME, session_cookie)
    logout_client.cookies.set(TRUSTED_BROWSER_COOKIE_NAME, trusted_cookie)
    return renewal_client, logout_client


def _login_trusted_browser(client: TestClient, settings: Settings) -> None:
    response = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            settings,
            password="correct horse battery staple",
            trust_browser=True,
        ),
    )
    assert response.status_code == 200


def _assert_no_session_cookie(response: HttpxResponse) -> None:
    assert not any(SESSION_COOKIE_NAME in header for header in response.headers.get_list("set-cookie"))


def _start_request_in_thread(
    request: Callable[[], HttpxResponse],
) -> tuple[Thread, list[HttpxResponse], list[BaseException]]:
    responses: list[HttpxResponse] = []
    errors: list[BaseException] = []

    def run() -> None:
        try:
            responses.append(request())
        except BaseException as exc:  # noqa: BLE001 - re-raise in the test thread
            errors.append(exc)

    thread = Thread(target=run)
    thread.start()
    return thread, responses, errors


def _renewal_response_after_logout(
    *,
    monkeypatch: pytest.MonkeyPatch,
    module: ModuleType,
    renew: Callable[[], HttpxResponse],
    logout_client: TestClient,
) -> HttpxResponse:
    trusted_identity_read = Event()
    release_renewal = Event()
    original_authenticate_browser = module.authenticate_browser

    def authenticate_then_wait(
        request: Request,
        configured: Settings,
    ) -> OperatorIdentity | None:
        identity = original_authenticate_browser(request, configured)
        if identity is not None and identity.renew_session:
            trusted_identity_read.set()
            release_renewal.wait(timeout=5)
        return identity

    monkeypatch.setattr(module, "authenticate_browser", authenticate_then_wait)
    renewal_thread, responses, errors = _start_request_in_thread(renew)
    try:
        assert trusted_identity_read.wait(timeout=5)
        logout = logout_client.post(
            "/api/v1/auth/logout",
            headers={"Origin": "http://testserver"},
        )
        assert logout.status_code == 204
    finally:
        release_renewal.set()
        renewal_thread.join(timeout=5)

    assert not renewal_thread.is_alive()
    assert errors == []
    assert len(responses) == 1
    return responses[0]


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


def test_trusted_browser_renews_protected_request(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)

    @app.get("/api/v1/test-trusted-renewal")
    def trusted_renewal_endpoint() -> dict[str, bool]:
        return {"ok": True}

    client = TestClient(app)
    _login_trusted_browser(client, secured)
    renewal_client, _ = _trusted_renewal_clients(app, client)

    response = renewal_client.get("/api/v1/test-trusted-renewal")

    assert response.status_code == 200
    assert response.json() == {"ok": True}
    assert any(SESSION_COOKIE_NAME in header for header in response.headers.get_list("set-cookie"))


def test_session_renewal_skips_cookie_when_logout_wins(
    settings: Settings,
    engine: Engine,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)
    _login_trusted_browser(client, secured)
    renewal_client, logout_client = _trusted_renewal_clients(app, client)
    response = _renewal_response_after_logout(
        monkeypatch=monkeypatch,
        module=auth_module,
        renew=lambda: renewal_client.get("/api/v1/auth/session"),
        logout_client=logout_client,
    )

    assert response.status_code == 200
    _assert_no_session_cookie(response)


def test_middleware_renewal_skips_cookie_when_logout_wins(
    settings: Settings,
    engine: Engine,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)

    @app.get("/api/v1/test-trusted-renewal-race")
    def trusted_renewal_race_endpoint() -> dict[str, bool]:
        return {"ok": True}

    client = TestClient(app)
    _login_trusted_browser(client, secured)
    renewal_client, logout_client = _trusted_renewal_clients(app, client)
    response = _renewal_response_after_logout(
        monkeypatch=monkeypatch,
        module=main_module,
        renew=lambda: renewal_client.get("/api/v1/test-trusted-renewal-race"),
        logout_client=logout_client,
    )

    assert response.status_code == 200
    assert response.json() == {"ok": True}
    _assert_no_session_cookie(response)


def test_logout_barrier_rejects_late_automatic_renewal_cookies(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    assert secured.operator_totp_secret is not None
    app = create_app(settings=secured, engine=engine)

    @app.get("/api/v1/test-late-trusted-renewal")
    def late_trusted_renewal_endpoint() -> dict[str, bool]:
        return {"ok": True}

    client = TestClient(app)
    _login_trusted_browser(client, secured)
    direct_renewal_client, logout_client = _trusted_renewal_clients(app, client)
    middleware_renewal_client, _ = _trusted_renewal_clients(app, client)

    direct_renewal = direct_renewal_client.get("/api/v1/auth/session")
    middleware_renewal = middleware_renewal_client.get("/api/v1/test-late-trusted-renewal")
    direct_session = direct_renewal_client.cookies.get(SESSION_COOKIE_NAME)
    middleware_session = middleware_renewal_client.cookies.get(SESSION_COOKIE_NAME)
    assert direct_renewal.status_code == 200
    assert middleware_renewal.status_code == 200
    assert direct_session is not None
    assert middleware_session is not None

    logout = logout_client.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )
    assert logout.status_code == 204
    assert LOGOUT_BARRIER_COOKIE_NAME in logout_client.cookies
    barrier_headers = [
        header
        for header in logout.headers.get_list("set-cookie")
        if header.startswith(f"{LOGOUT_BARRIER_COOKIE_NAME}=")
    ]
    assert len(barrier_headers) == 1
    assert "HttpOnly" in barrier_headers[0]
    assert "SameSite=strict" in barrier_headers[0]
    assert "Domain=" not in barrier_headers[0]

    # Simulate either automatic-renewal response arriving after the logout
    # response has already cleared the browser's original credentials.
    for late_session in (direct_session, middleware_session):
        logout_client.cookies.set(SESSION_COOKIE_NAME, late_session)
        assert logout_client.get("/api/v1/auth/session").status_code == 401
        assert logout_client.get("/api/v1/test-late-trusted-renewal").status_code == 401

    fresh_login = logout_client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            password="correct horse battery staple",
            totp_code=pyotp.TOTP(secured.operator_totp_secret).at(
                int(time.time()) + pyotp.TOTP(secured.operator_totp_secret).interval
            ),
        ),
    )
    assert fresh_login.status_code == 200
    assert LOGOUT_BARRIER_COOKIE_NAME not in logout_client.cookies
    assert logout_client.get("/api/v1/test-late-trusted-renewal").status_code == 200


def test_expired_logout_still_blocks_an_already_admitted_trusted_renewal(
    settings: Settings,
    engine: Engine,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    now = [1_700_000_000.0]
    monkeypatch.setattr(operator_auth.time, "time", lambda: now[0])
    secured = replace(
        _enabled_settings(settings),
        auth_session_ttl_seconds=300,
        auth_trusted_browser_ttl_days=1,
    )
    assert secured.operator_totp_secret is not None
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)
    login = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            password="correct horse battery staple",
            totp_code=pyotp.TOTP(secured.operator_totp_secret).at(int(now[0])),
            trust_browser=True,
        ),
    )
    assert login.status_code == 200
    renewal_client, logout_client = _trusted_renewal_clients(app, client)
    trusted_identity_read = Event()
    release_renewal = Event()
    original_authenticate_browser = auth_module.authenticate_browser

    def authenticate_then_wait(
        request: Request,
        configured: Settings,
    ) -> OperatorIdentity | None:
        identity = original_authenticate_browser(request, configured)
        if identity is not None and identity.renew_session:
            trusted_identity_read.set()
            release_renewal.wait(timeout=5)
        return identity

    monkeypatch.setattr(auth_module, "authenticate_browser", authenticate_then_wait)
    renewal_thread, responses, errors = _start_request_in_thread(
        lambda: renewal_client.get("/api/v1/auth/session")
    )
    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    generation = runtime.stream_generation()
    try:
        assert trusted_identity_read.wait(timeout=5)
        now[0] += secured.auth_trusted_browser_ttl_days * 24 * 60 * 60 + 1
        logout = logout_client.post(
            "/api/v1/auth/logout",
            headers={"Origin": "http://testserver"},
        )
        assert logout.status_code == 204
        # Expiration makes this logout anonymous for stream revocation, but it
        # must still protect the browser from the already-admitted renewal.
        assert runtime.stream_generation() == generation
        assert LOGOUT_BARRIER_COOKIE_NAME in logout_client.cookies
    finally:
        release_renewal.set()
        renewal_thread.join(timeout=5)

    assert not renewal_thread.is_alive()
    assert errors == []
    assert len(responses) == 1
    assert responses[0].status_code == 200
    assert any(
        SESSION_COOKIE_NAME in header for header in responses[0].headers.get_list("set-cookie")
    )
    late_session = renewal_client.cookies.get(SESSION_COOKIE_NAME)
    assert late_session is not None
    logout_client.cookies.set(SESSION_COOKIE_NAME, late_session)
    assert logout_client.get("/api/v1/auth/session").status_code == 401


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
    assert LOGOUT_BARRIER_COOKIE_NAME in anonymous.cookies


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
