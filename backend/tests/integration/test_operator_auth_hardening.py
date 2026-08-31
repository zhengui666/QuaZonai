from __future__ import annotations

import asyncio
import base64
import ipaddress
import time
from collections.abc import Callable
from dataclasses import replace
from threading import Event, Thread
from types import ModuleType
from typing import Any

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
    BROWSER_EPOCH_COOKIE_NAME,
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
        operator_totp_secret=pyotp.random_base32(),
        auth_cookie_key=base64.b64encode(b"a" * 32).decode("ascii"),
        api_token="machine-token-" + "x" * 32,
        auth_public_origin="http://testserver",
    )


def _payload(
    settings: Settings,
    *,
    totp_code: str | None = None,
    trust_browser: bool = False,
) -> dict[str, object]:
    assert settings.operator_totp_secret is not None
    return {
        "totp_code": totp_code or pyotp.TOTP(settings.operator_totp_secret).now(),
        "trust_browser": trust_browser,
    }


def _wrong_totp(settings: Settings) -> str:
    assert settings.operator_totp_secret is not None
    current = pyotp.TOTP(settings.operator_totp_secret).now()
    return "000000" if current != "000000" else "000001"


def _request_with_session(app: object, session_cookie: str) -> Request:
    return _request_with_cookie_header(
        app,
        f"{SESSION_COOKIE_NAME}={session_cookie}",
    )


def _request_with_cookie_header(app: object, cookie_header: str) -> Request:
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
                    cookie_header.encode("ascii"),
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
    browser_epoch = client.cookies.get(BROWSER_EPOCH_COOKIE_NAME)
    assert session_cookie is not None
    assert trusted_cookie is not None

    renewal_client = TestClient(app)
    renewal_client.cookies.set(TRUSTED_BROWSER_COOKIE_NAME, trusted_cookie)
    logout_client = TestClient(app)
    logout_client.cookies.set(SESSION_COOKIE_NAME, session_cookie)
    logout_client.cookies.set(TRUSTED_BROWSER_COOKIE_NAME, trusted_cookie)
    if browser_epoch is not None:
        renewal_client.cookies.set(BROWSER_EPOCH_COOKIE_NAME, browser_epoch)
        logout_client.cookies.set(BROWSER_EPOCH_COOKIE_NAME, browser_epoch)
    return renewal_client, logout_client


def _login_trusted_browser(
    client: TestClient,
    settings: Settings,
    *,
    origin: str = "http://testserver",
) -> None:
    response = client.post(
        "/api/v1/auth/login",
        headers={"Origin": origin},
        json=_payload(
            settings,
            trust_browser=True,
        ),
    )
    assert response.status_code == 200


def _assert_no_session_cookie(response: HttpxResponse) -> None:
    assert not any(SESSION_COOKIE_NAME in header for header in response.headers.get_list("set-cookie"))


def _set_cookie_value(response: Any, name: str) -> str:
    matching = [
        header
        for header in response.headers.get_list("set-cookie")
        if header.startswith(f"{name}=")
    ]
    assert len(matching) == 1
    cookie_name, separator, value = matching[0].partition(";")[0].partition("=")
    assert cookie_name == name
    assert separator
    return value


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
        maximum_backoff_seconds=30.0,
    )
    app = create_app(settings=secured, engine=engine)
    app.state.operator_auth_runtime = OperatorAuthRuntime(login_limiter=limiter)
    client = TestClient(app)

    invalid = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(secured, totp_code=_wrong_totp(secured)),
    )
    throttled = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(secured),
    )

    assert invalid.status_code == 401
    assert throttled.status_code == 401
    assert throttled.json() == invalid.json()

    now[0] += 1.01
    recovered = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(secured),
    )
    assert recovered.status_code == 200


def test_login_backoff_is_bounded_without_durable_lockout() -> None:
    now = [0.0]
    limiter = OperatorLoginLimiter(
        clock=lambda: now[0],
        minimum_interval_seconds=1.0,
        base_backoff_seconds=1.0,
        maximum_backoff_seconds=30.0,
    )

    for _ in range(8):
        assert limiter.allow_attempt("203.0.113.10")
        limiter.record_failure("203.0.113.10")
        now[0] += 30.01

    assert limiter.allow_attempt("203.0.113.10")
    limiter.record_failure("203.0.113.10")
    now[0] += 29.99
    assert not limiter.allow_attempt("203.0.113.10")
    now[0] += 0.02
    assert limiter.allow_attempt("203.0.113.10")


def test_trusted_proxy_separates_login_backoff_sources(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = replace(
        _enabled_settings(settings),
        auth_trusted_proxy_cidrs=(ipaddress.ip_network("10.20.30.0/24"),),
    )
    now = [100.0]
    limiter = OperatorLoginLimiter(
        clock=lambda: now[0],
        minimum_interval_seconds=1.0,
        base_backoff_seconds=1.0,
        maximum_backoff_seconds=30.0,
    )
    app = create_app(settings=secured, engine=engine)
    app.state.operator_auth_runtime = OperatorAuthRuntime(login_limiter=limiter)
    attacker = TestClient(app, client=("10.20.30.2", 50000))
    operator = TestClient(app, client=("10.20.30.2", 50001))

    rejected = attacker.post(
        "/api/v1/auth/login",
        headers={
            "Origin": "http://testserver",
            "X-Forwarded-For": "198.51.100.11, 10.20.30.3",
        },
        json=_payload(secured, totp_code=_wrong_totp(secured)),
    )
    accepted = operator.post(
        "/api/v1/auth/login",
        headers={
            "Origin": "http://testserver",
            "X-Forwarded-For": "203.0.113.12, 10.20.30.3",
        },
        json=_payload(secured),
    )

    assert rejected.status_code == 401
    assert accepted.status_code == 200


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
        json=_payload(secured),
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


def test_session_renewal_returns_auth_required_when_logout_wins(
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

    assert response.status_code == 401
    assert response.json()["error"]["code"] == "AUTH_REQUIRED"
    assert response.headers["Cache-Control"] == "no-store"
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
            totp_code=pyotp.TOTP(secured.operator_totp_secret).at(
                int(time.time()) + pyotp.TOTP(secured.operator_totp_secret).interval
            ),
        ),
    )
    assert fresh_login.status_code == 200
    assert LOGOUT_BARRIER_COOKIE_NAME not in logout_client.cookies
    assert logout_client.get("/api/v1/test-late-trusted-renewal").status_code == 200


def test_forged_logout_barrier_does_not_block_valid_browser_credentials(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)
    _login_trusted_browser(client, secured)
    session_cookie = client.cookies.get(SESSION_COOKIE_NAME)
    trusted_cookie = client.cookies.get(TRUSTED_BROWSER_COOKIE_NAME)
    assert session_cookie is not None
    assert trusted_cookie is not None

    request = _request_with_cookie_header(
        app,
        "; ".join(
            (
                f"{LOGOUT_BARRIER_COOKIE_NAME}=1",
                f"{SESSION_COOKIE_NAME}={session_cookie}",
                f"{TRUSTED_BROWSER_COOKIE_NAME}={trusted_cookie}",
            )
        ),
    )

    identity = operator_auth.authenticate_browser(request, secured)
    assert identity is not None
    assert identity.source == "session"
    assert operator_auth.has_valid_trusted_browser(request, secured)


@pytest.mark.parametrize("forged_parent_first", (True, False))
def test_parent_domain_session_cookie_cannot_shadow_host_only_session(
    settings: Settings,
    engine: Engine,
    forged_parent_first: bool,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)
    _login_trusted_browser(client, secured)
    session_cookie = client.cookies.get(SESSION_COOKIE_NAME)
    assert session_cookie is not None

    # A browser can send a sibling's parent-Domain cookie beside the host-only
    # credential in either order. Cookie metadata is absent from the request,
    # so model the two same-name values exactly as the server receives them.
    cookie_values = (
        ("forged-parent-domain-value", session_cookie)
        if forged_parent_first
        else (session_cookie, "forged-parent-domain-value")
    )
    request = _request_with_cookie_header(
        app,
        "; ".join(
            f"{SESSION_COOKIE_NAME}={value}" for value in cookie_values
        ),
    )

    identity = operator_auth.authenticate_browser(request, secured)
    assert identity is not None
    assert identity.source == "session"


@pytest.mark.parametrize("forged_parent_first", (True, False))
def test_parent_domain_trusted_cookie_cannot_shadow_host_only_credential(
    settings: Settings,
    engine: Engine,
    forged_parent_first: bool,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)
    _login_trusted_browser(client, secured)
    trusted_cookie = client.cookies.get(TRUSTED_BROWSER_COOKIE_NAME)
    assert trusted_cookie is not None

    cookie_values = (
        ("forged-parent-domain-value", trusted_cookie)
        if forged_parent_first
        else (trusted_cookie, "forged-parent-domain-value")
    )
    request = _request_with_cookie_header(
        app,
        "; ".join(
            (
                # An injected session cookie must not prevent valid trusted-
                # browser authentication after no valid session is found.
                f"{SESSION_COOKIE_NAME}=forged-parent-domain-value",
                *(
                    f"{TRUSTED_BROWSER_COOKIE_NAME}={value}"
                    for value in cookie_values
                ),
            )
        ),
    )

    identity = operator_auth.authenticate_browser(request, secured)
    assert identity is not None
    assert identity.source == "trusted_browser"
    assert identity.renew_session
    assert operator_auth.has_valid_trusted_browser(request, secured)


def test_valid_logout_barrier_wins_over_forged_duplicate_cookie(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)
    _login_trusted_browser(client, secured)
    session_cookie = client.cookies.get(SESSION_COOKIE_NAME)
    trusted_cookie = client.cookies.get(TRUSTED_BROWSER_COOKIE_NAME)
    assert session_cookie is not None
    assert trusted_cookie is not None

    logout = client.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )
    assert logout.status_code == 204
    barrier_cookie = _set_cookie_value(logout, LOGOUT_BARRIER_COOKIE_NAME)
    assert barrier_cookie != "1"

    request = _request_with_cookie_header(
        app,
        "; ".join(
            (
                f"{LOGOUT_BARRIER_COOKIE_NAME}={barrier_cookie}",
                f"{LOGOUT_BARRIER_COOKIE_NAME}=1",
                f"{SESSION_COOKIE_NAME}={session_cookie}",
                f"{TRUSTED_BROWSER_COOKIE_NAME}={trusted_cookie}",
            )
        ),
    )

    assert operator_auth.authenticate_browser(request, secured) is None
    assert not operator_auth.has_valid_trusted_browser(request, secured)


def test_login_ignores_forged_parent_domain_barrier_left_after_host_clear(
    settings: Settings,
    engine: Engine,
) -> None:
    origin = "https://quazonai.example.com"
    secured = replace(_enabled_settings(settings), auth_public_origin=origin)
    assert secured.operator_totp_secret is not None
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app, base_url=origin)
    _login_trusted_browser(client, secured, origin=origin)
    client.cookies.set(
        LOGOUT_BARRIER_COOKIE_NAME,
        "1",
        domain=".example.com",
        path="/",
    )

    logout = client.post("/api/v1/auth/logout", headers={"Origin": origin})
    assert logout.status_code == 204
    assert _set_cookie_value(logout, LOGOUT_BARRIER_COOKIE_NAME) != "1"

    totp = pyotp.TOTP(secured.operator_totp_secret)
    login = client.post(
        "/api/v1/auth/login",
        headers={"Origin": origin},
        json=_payload(
            secured,
            totp_code=totp.at(int(time.time()) + totp.interval),
        ),
    )
    assert login.status_code == 200
    remaining_barriers = [
        (cookie.domain, cookie.value)
        for cookie in client.cookies.jar
        if cookie.name == LOGOUT_BARRIER_COOKIE_NAME
    ]
    assert remaining_barriers == [(".example.com", "1")]

    response = client.get("/api/v1/system/runtime-configuration")
    assert response.status_code == 200


def test_anonymous_logout_does_not_block_an_already_admitted_trusted_renewal_in_another_browser(
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
    stream_generation = runtime.stream_generation()
    cookie_generation = runtime.cookie_generation()
    try:
        assert trusted_identity_read.wait(timeout=5)
        now[0] += secured.auth_trusted_browser_ttl_days * 24 * 60 * 60 + 1
        logout = logout_client.post(
            "/api/v1/auth/logout",
            headers={"Origin": "http://testserver"},
        )
        assert logout.status_code == 204
        # Expiration makes this logout anonymous. It writes a caller-local
        # barrier/epoch, but cannot be allowed to advance the global issuance
        # epoch and block an unrelated browser's renewal.
        assert runtime.stream_generation() == stream_generation
        assert runtime.cookie_generation() == cookie_generation
        assert LOGOUT_BARRIER_COOKIE_NAME in logout_client.cookies
        assert BROWSER_EPOCH_COOKIE_NAME in logout_client.cookies
    finally:
        release_renewal.set()
        renewal_thread.join(timeout=5)

    assert not renewal_thread.is_alive()
    assert errors == []
    assert len(responses) == 1
    assert responses[0].status_code == 200
    assert renewal_client.cookies.get(SESSION_COOKIE_NAME) is not None


def test_anonymous_logout_does_not_block_a_concurrent_login_in_another_browser(
    settings: Settings,
    engine: Engine,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    now = [1_700_000_000.0]
    monkeypatch.setattr(operator_auth.time, "time", lambda: now[0])
    secured = _enabled_settings(settings)
    assert secured.operator_totp_secret is not None
    app = create_app(settings=secured, engine=engine)
    app.state.operator_auth_runtime = OperatorAuthRuntime(
        login_limiter=OperatorLoginLimiter(
            minimum_interval_seconds=0,
            base_backoff_seconds=0,
            maximum_backoff_seconds=0,
        )
    )
    login_client = TestClient(app)
    logout_client = TestClient(app)
    login_authorized = Event()
    release_login = Event()
    original_authenticate_totp_login = auth_module.authenticate_totp_login

    def authenticate_then_wait(
        configured: Settings,
        configured_runtime: OperatorAuthRuntime,
        *,
        totp_code: str,
    ) -> bool:
        accepted = original_authenticate_totp_login(
            configured,
            configured_runtime,
            totp_code=totp_code,
        )
        if accepted:
            login_authorized.set()
            release_login.wait(timeout=5)
        return accepted

    monkeypatch.setattr(auth_module, "authenticate_totp_login", authenticate_then_wait)
    totp = pyotp.TOTP(secured.operator_totp_secret)
    login_thread, responses, errors = _start_request_in_thread(
        lambda: login_client.post(
            "/api/v1/auth/login",
            headers={"Origin": "http://testserver"},
            json=_payload(
                secured,
                totp_code=totp.at(int(now[0])),
                trust_browser=True,
            ),
        )
    )
    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    stream_generation = runtime.stream_generation()
    cookie_generation = runtime.cookie_generation()
    try:
        assert login_authorized.wait(timeout=5)
        logout = logout_client.post(
            "/api/v1/auth/logout",
            headers={"Origin": "http://testserver"},
        )
        assert logout.status_code == 204
        assert runtime.stream_generation() == stream_generation
        assert runtime.cookie_generation() == cookie_generation
        assert LOGOUT_BARRIER_COOKIE_NAME in logout_client.cookies
        assert BROWSER_EPOCH_COOKIE_NAME in logout_client.cookies
    finally:
        release_login.set()
        login_thread.join(timeout=5)

    assert not login_thread.is_alive()
    assert errors == []
    assert len(responses) == 1
    completed_login = responses[0]
    assert completed_login.status_code == 200
    assert login_client.cookies.get(SESSION_COOKIE_NAME) is not None

    # A login in the browser that performed anonymous logout binds credentials
    # to its caller-local epoch and clears its barrier as intended.
    now[0] += totp.interval
    fresh_login = logout_client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            totp_code=totp.at(int(now[0])),
        ),
    )
    assert fresh_login.status_code == 200
    assert LOGOUT_BARRIER_COOKIE_NAME not in logout_client.cookies
    assert logout_client.get("/api/v1/auth/session").status_code == 200


def test_full_login_cannot_override_a_concurrent_authenticated_logout(
    settings: Settings,
    engine: Engine,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    now = [1_700_000_000.0]
    monkeypatch.setattr(operator_auth.time, "time", lambda: now[0])
    secured = _enabled_settings(settings)
    assert secured.operator_totp_secret is not None
    app = create_app(settings=secured, engine=engine)
    # This test intentionally performs three immediate logins from TestClient's
    # same peer. Remove timing from the limiter so the synchronization below is
    # testing only the logout-generation contract.
    app.state.operator_auth_runtime = OperatorAuthRuntime(
        login_limiter=OperatorLoginLimiter(
            minimum_interval_seconds=0,
            base_backoff_seconds=0,
            maximum_backoff_seconds=0,
        )
    )
    bootstrap_client = TestClient(app)
    totp = pyotp.TOTP(secured.operator_totp_secret)
    bootstrap = bootstrap_client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            totp_code=totp.at(int(now[0])),
        ),
    )
    assert bootstrap.status_code == 200
    bootstrap_session = bootstrap_client.cookies.get(SESSION_COOKIE_NAME)
    assert bootstrap_session is not None

    logout_client = TestClient(app)
    logout_client.cookies.set(SESSION_COOKIE_NAME, bootstrap_session)
    login_client = TestClient(app)
    login_authorized = Event()
    release_login = Event()
    original_authenticate_totp_login = auth_module.authenticate_totp_login

    def authenticate_then_wait(
        configured: Settings,
        configured_runtime: OperatorAuthRuntime,
        *,
        totp_code: str,
    ) -> bool:
        accepted = original_authenticate_totp_login(
            configured,
            configured_runtime,
            totp_code=totp_code,
        )
        if accepted:
            login_authorized.set()
            release_login.wait(timeout=5)
        return accepted

    monkeypatch.setattr(auth_module, "authenticate_totp_login", authenticate_then_wait)
    now[0] += totp.interval
    login_thread, responses, errors = _start_request_in_thread(
        lambda: login_client.post(
            "/api/v1/auth/login",
            headers={"Origin": "http://testserver"},
            json=_payload(
                secured,
                totp_code=totp.at(int(now[0])),
                trust_browser=True,
            ),
        )
    )
    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    generation = runtime.stream_generation()
    cookie_generation = runtime.cookie_generation()
    try:
        assert login_authorized.wait(timeout=5)
        logout = logout_client.post(
            "/api/v1/auth/logout",
            headers={"Origin": "http://testserver"},
        )
        assert logout.status_code == 204
        assert runtime.stream_generation() == generation + 1
        assert runtime.cookie_generation() == cookie_generation + 1
        assert LOGOUT_BARRIER_COOKIE_NAME in logout_client.cookies
        assert BROWSER_EPOCH_COOKIE_NAME in logout_client.cookies
    finally:
        release_login.set()
        login_thread.join(timeout=5)

    assert not login_thread.is_alive()
    assert errors == []
    assert len(responses) == 1
    lost_login = responses[0]
    assert lost_login.status_code == 401
    assert lost_login.json()["error"]["code"] == "AUTH_INVALID"
    assert lost_login.headers.get_list("set-cookie") == []
    assert login_client.cookies.get(SESSION_COOKIE_NAME) is None
    assert logout_client.get("/api/v1/auth/session").status_code == 401

    # A login that starts after logout snapshots the new generation and can
    # deliberately clear its browser-local barrier as intended.
    now[0] += totp.interval
    fresh_login = logout_client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            totp_code=totp.at(int(now[0])),
        ),
    )
    assert fresh_login.status_code == 200
    assert LOGOUT_BARRIER_COOKIE_NAME not in logout_client.cookies
    assert logout_client.get("/api/v1/auth/session").status_code == 200


def test_authenticated_logout_invalidates_older_browser_cookie_generation(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    logout_client = TestClient(app)
    login = logout_client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(secured),
    )
    assert login.status_code == 200
    session_cookie = logout_client.cookies.get(SESSION_COOKIE_NAME)
    assert session_cookie is not None

    stale_cookie_client = TestClient(app)
    stale_cookie_client.cookies.set(SESSION_COOKIE_NAME, session_cookie)
    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    cookie_generation = runtime.cookie_generation()

    logout = logout_client.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )

    assert logout.status_code == 204
    assert runtime.cookie_generation() == cookie_generation + 1
    # The raw request deliberately omits the caller-local epoch and barrier. A
    # pre-logout credential must still fail from its embedded global generation.
    assert stale_cookie_client.get("/api/v1/auth/session").status_code == 401


def test_restart_cannot_revive_pre_logout_browser_credentials_with_same_local_epoch(
    settings: Settings,
    engine: Engine,
) -> None:
    """A reset counter must not re-authorize cookies issued before logout."""
    secured = _enabled_settings(settings)
    first_app = create_app(settings=secured, engine=engine)
    browser = TestClient(first_app)

    # Establish the caller-local epoch first. The stale credentials below are
    # therefore not rejected merely because their browser-local epoch is absent.
    anonymous_logout = browser.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )
    assert anonymous_logout.status_code == 204
    browser_epoch = browser.cookies.get(BROWSER_EPOCH_COOKIE_NAME)
    assert browser_epoch is not None

    login = browser.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            trust_browser=True,
        ),
    )
    assert login.status_code == 200
    session_cookie = browser.cookies.get(SESSION_COOKIE_NAME)
    trusted_cookie = browser.cookies.get(TRUSTED_BROWSER_COOKIE_NAME)
    assert session_cookie is not None
    assert trusted_cookie is not None

    first_runtime: OperatorAuthRuntime = first_app.state.operator_auth_runtime
    pre_logout_issuance = first_runtime.cookie_issuance()
    assert pre_logout_issuance.generation == 0

    # A different browser profile performs an authenticated logout. It advances
    # only the global generation; the stale profile retains its old local epoch.
    logout_browser = TestClient(first_app)
    logout_browser.cookies.set(SESSION_COOKIE_NAME, session_cookie)
    logout_browser.cookies.set(BROWSER_EPOCH_COOKIE_NAME, browser_epoch)
    logout = logout_browser.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )
    assert logout.status_code == 204
    assert first_runtime.cookie_generation() == pre_logout_issuance.generation + 1

    # Model an API restart with the same cookie key and retained stale browser
    # epoch. Its counter starts at zero again, just like the original runtime.
    restarted_app = create_app(settings=secured, engine=engine)
    restarted_runtime: OperatorAuthRuntime = restarted_app.state.operator_auth_runtime
    restarted_issuance = restarted_runtime.cookie_issuance()
    assert restarted_issuance.generation == pre_logout_issuance.generation
    assert restarted_issuance.process_epoch != pre_logout_issuance.process_epoch

    for name, value in (
        (SESSION_COOKIE_NAME, session_cookie),
        (TRUSTED_BROWSER_COOKIE_NAME, trusted_cookie),
    ):
        stale_browser = TestClient(restarted_app)
        stale_browser.cookies.set(BROWSER_EPOCH_COOKIE_NAME, browser_epoch)
        stale_browser.cookies.set(name, value)
        assert stale_browser.get("/api/v1/auth/session").status_code == 401

    # Reauthentication under the restarted runtime mints credentials with its
    # new process epoch and retains the caller-local epoch as before.
    reauthenticated_browser = TestClient(restarted_app)
    reauthenticated_browser.cookies.set(BROWSER_EPOCH_COOKIE_NAME, browser_epoch)
    reauthenticated = reauthenticated_browser.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            trust_browser=True,
        ),
    )
    assert reauthenticated.status_code == 200
    assert reauthenticated_browser.get("/api/v1/auth/session").status_code == 200


def test_network_reordered_login_response_cannot_restore_after_anonymous_logout(
    settings: Settings,
    engine: Engine,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    now = [1_700_000_000.0]
    monkeypatch.setattr(operator_auth.time, "time", lambda: now[0])
    secured = _enabled_settings(settings)
    assert secured.operator_totp_secret is not None
    app = create_app(settings=secured, engine=engine)
    delayed_login_client = TestClient(app)
    delayed_login = delayed_login_client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            totp_code=pyotp.TOTP(secured.operator_totp_secret).at(int(now[0])),
            trust_browser=True,
        ),
    )
    assert delayed_login.status_code == 200
    stale_session = _set_cookie_value(delayed_login, SESSION_COOKIE_NAME)
    stale_trusted_browser = _set_cookie_value(delayed_login, TRUSTED_BROWSER_COOKIE_NAME)
    assert not any(
        header.startswith(f"{BROWSER_EPOCH_COOKIE_NAME}=")
        for header in delayed_login.headers.get_list("set-cookie")
    )

    # Model the same browser applying its anonymous logout response first, then
    # a previously emitted full-login response after the network reorders them.
    browser = TestClient(app)
    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    cookie_generation = runtime.cookie_generation()
    logout = browser.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )
    assert logout.status_code == 204
    assert runtime.cookie_generation() == cookie_generation
    browser_epoch = browser.cookies.get(BROWSER_EPOCH_COOKIE_NAME)
    assert browser_epoch is not None
    assert LOGOUT_BARRIER_COOKIE_NAME in browser.cookies

    # The stale response clears the newer barrier and replays its session and
    # trusted-browser values, but it never touches the durable local epoch.
    browser.cookies.delete(LOGOUT_BARRIER_COOKIE_NAME)
    browser.cookies.set(SESSION_COOKIE_NAME, stale_session)
    browser.cookies.set(TRUSTED_BROWSER_COOKIE_NAME, stale_trusted_browser)
    assert LOGOUT_BARRIER_COOKIE_NAME not in browser.cookies
    assert browser.cookies.get(BROWSER_EPOCH_COOKIE_NAME) == browser_epoch
    assert browser.get("/api/v1/auth/session").status_code == 401

    # A fresh factor-backed login binds new credentials to the retained local
    # epoch and restores access normally.
    now[0] += pyotp.TOTP(secured.operator_totp_secret).interval
    fresh_login = browser.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            totp_code=pyotp.TOTP(secured.operator_totp_secret).at(int(now[0])),
            trust_browser=True,
        ),
    )
    assert fresh_login.status_code == 200
    assert browser.get("/api/v1/auth/session").status_code == 200


def test_network_reordered_trusted_renewal_cannot_restore_after_anonymous_logout(
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
    initial_browser = TestClient(app)
    initial_login = initial_browser.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            totp_code=pyotp.TOTP(secured.operator_totp_secret).at(int(now[0])),
            trust_browser=True,
        ),
    )
    assert initial_login.status_code == 200
    trusted_cookie = initial_browser.cookies.get(TRUSTED_BROWSER_COOKIE_NAME)
    assert trusted_cookie is not None

    # The trusted-only request finishes before logout, but model its Set-Cookie
    # response as delayed in the network rather than applying it to the browser.
    renewal_client = TestClient(app)
    renewal_client.cookies.set(TRUSTED_BROWSER_COOKIE_NAME, trusted_cookie)
    delayed_renewal = renewal_client.get("/api/v1/auth/session")
    assert delayed_renewal.status_code == 200
    stale_session = _set_cookie_value(delayed_renewal, SESSION_COOKIE_NAME)

    # A valid caller-local barrier makes a logout anonymous even while a
    # trusted-browser cookie is present. This models a profile that has already
    # signed out while an automatic renewal response remains in flight, without
    # expiring the old renewal's short session before we can test it.
    barrier_source = TestClient(app)
    barrier_response = barrier_source.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )
    assert barrier_response.status_code == 204
    preexisting_barrier = _set_cookie_value(barrier_response, LOGOUT_BARRIER_COOKIE_NAME)
    browser = TestClient(app)
    browser.cookies.set(TRUSTED_BROWSER_COOKIE_NAME, trusted_cookie)
    browser.cookies.set(
        LOGOUT_BARRIER_COOKIE_NAME,
        preexisting_barrier,
        domain="testserver.local",
        path="/",
    )
    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    cookie_generation = runtime.cookie_generation()
    # The valid barrier makes this a public/anonymous logout despite a still
    # valid trusted credential, so it cannot advance global issuance.
    logout = browser.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )
    assert logout.status_code == 204
    assert runtime.cookie_generation() == cookie_generation
    assert browser.cookies.get(BROWSER_EPOCH_COOKIE_NAME) is not None

    # A new factor-backed login is valid for the retained local epoch and clears
    # the barrier. A delayed old trusted-only renewal then overwrites its session
    # header, which must remain unusable even without a barrier to stop it.
    now[0] += pyotp.TOTP(secured.operator_totp_secret).interval
    fresh_login = browser.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            totp_code=pyotp.TOTP(secured.operator_totp_secret).at(int(now[0])),
        ),
    )
    assert fresh_login.status_code == 200
    assert LOGOUT_BARRIER_COOKIE_NAME not in browser.cookies
    for cookie in list(browser.cookies.jar):
        if cookie.name == SESSION_COOKIE_NAME:
            browser.cookies.jar.clear(cookie.domain, cookie.path, cookie.name)
    browser.cookies.set(SESSION_COOKIE_NAME, stale_session)
    assert browser.get("/api/v1/auth/session").status_code == 401


@pytest.mark.parametrize("forged_epoch_first", (True, False))
def test_forged_browser_epoch_cookie_cannot_shadow_the_valid_host_epoch(
    settings: Settings,
    engine: Engine,
    forged_epoch_first: bool,
) -> None:
    secured = _enabled_settings(settings)
    assert secured.operator_totp_secret is not None
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)
    logout = client.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )
    assert logout.status_code == 204
    browser_epoch = client.cookies.get(BROWSER_EPOCH_COOKIE_NAME)
    assert browser_epoch is not None
    login = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json=_payload(
            secured,
            trust_browser=True,
        ),
    )
    assert login.status_code == 200
    session_cookie = client.cookies.get(SESSION_COOKIE_NAME)
    assert session_cookie is not None

    epochs = (
        ("forged-parent-domain-value", browser_epoch)
        if forged_epoch_first
        else (browser_epoch, "forged-parent-domain-value")
    )
    request = _request_with_cookie_header(
        app,
        "; ".join(
            (
                *(f"{BROWSER_EPOCH_COOKIE_NAME}={epoch}" for epoch in epochs),
                f"{SESSION_COOKIE_NAME}={session_cookie}",
            )
        ),
    )

    identity = operator_auth.authenticate_browser(request, secured)
    assert identity is not None
    assert identity.source == "session"


def test_browser_credentials_without_an_issuance_generation_are_rejected(
    settings: Settings,
    engine: Engine,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    missing_generation = operator_auth._issue_cookie(
        secured,
        kind="session",
        ttl_seconds=secured.auth_session_ttl_seconds,
    )
    with monkeypatch.context() as legacy:
        legacy.setattr(operator_auth, "COOKIE_VERSION", 1)
        version_one = operator_auth._issue_cookie(
            secured,
            kind="session",
            ttl_seconds=secured.auth_session_ttl_seconds,
        )

    client = TestClient(app)
    for stale_cookie in (missing_generation, version_one):
        client.cookies.set(SESSION_COOKIE_NAME, stale_cookie)
        assert client.get("/api/v1/auth/session").status_code == 401


def test_anonymous_logout_does_not_revoke_other_active_streams(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    runtime: OperatorAuthRuntime = app.state.operator_auth_runtime
    generation = runtime.stream_generation()
    cookie_generation = runtime.cookie_generation()
    anonymous = TestClient(app)

    response = anonymous.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )

    assert response.status_code == 204
    assert runtime.stream_generation() == generation
    assert runtime.cookie_generation() == cookie_generation
    assert LOGOUT_BARRIER_COOKIE_NAME in anonymous.cookies
    assert BROWSER_EPOCH_COOKIE_NAME in anonymous.cookies


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
        json=_payload(secured),
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
        json=_payload(secured),
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
        json=_payload(secured),
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
