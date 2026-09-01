from __future__ import annotations

import base64
import uuid
from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace

import pyotp
import pytest
from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from mobile_auth import decode_mobile_credential
from operator_auth import OperatorLoginLimiter, authenticate_machine
from settings import Settings

_TOTP_SECRET = base64.b32encode(b"0123456789abcdefghij").decode("ascii").rstrip("=")
_COOKIE_KEY = base64.b64encode(b"m" * 32).decode("ascii")
_MACHINE_TOKEN = "M" * 48


def _enabled(settings: Settings) -> Settings:
    return replace(
        settings,
        operator_auth_enabled=True,
        operator_totp_secret=_TOTP_SECRET,
        auth_cookie_key=_COOKIE_KEY,
        api_token=_MACHINE_TOKEN,
        auth_public_origin="http://testserver",
    )


def _app(settings: Settings, engine: Engine):
    app = create_app(settings=_enabled(settings), engine=engine)
    app.state.operator_auth_runtime.login_limiter = OperatorLoginLimiter(
        minimum_interval_seconds=0,
        base_backoff_seconds=0,
        maximum_backoff_seconds=0,
    )
    return app


def _login_payload(*, installation_id: uuid.UUID | None = None, trust: bool = True) -> dict[str, object]:
    return {
        "totp_code": pyotp.TOTP(_TOTP_SECRET).now(),
        "installation_id": str(installation_id or uuid.uuid4()),
        "device_name": "Test iPhone",
        "device_family": "IPHONE",
        "os_version": "26.0",
        "app_version": "1.0.0",
        "app_build": "100",
        "trust_device": trust,
    }


def test_bootstrap_and_openapi_expose_totp_only_native_login(settings: Settings, engine: Engine) -> None:
    app = _app(settings, engine)
    with TestClient(app) as client:
        bootstrap = client.get("/api/v1/client/bootstrap")
        assert bootstrap.status_code == 200
        assert bootstrap.json()["auth_enabled"] is True
        assert bootstrap.headers["cache-control"] == "no-store"

        schema = client.get("/api/v1/openapi.json").json()
        properties = schema["components"]["schemas"]["MobileLoginInput"]["properties"]
        assert "totp_code" in properties
        assert "username" not in properties
        assert "password" not in properties
        refresh_security = schema["paths"]["/api/v1/auth/mobile/refresh"]["post"]["security"]
        assert refresh_security == [{"MobileRefreshBearer": []}]

        schemes = schema["components"]["securitySchemes"]
        assert "DownstreamBearer" in schemes
        downstream_operations = {
            ("post", "/api/v1/handoffs/{handoff_id}/claim"),
            ("post", "/api/v1/handoffs/{handoff_id}/accept"),
            ("post", "/api/v1/handoffs/{handoff_id}/reject"),
            ("get", "/api/v1/handoffs/{handoff_id}/package"),
            ("post", "/api/v1/handoffs/{handoff_id}/feedback"),
        }
        for method, path in downstream_operations:
            assert schema["paths"][path][method]["security"] == [{"DownstreamBearer": []}]

        claim_responses = schema["paths"]["/api/v1/handoffs/{handoff_id}/claim"]["post"]["responses"]
        assert claim_responses["422"]["content"]["application/json"]["schema"] == {
            "$ref": "#/components/schemas/ErrorEnvelope"
        }
        assert "422" not in schema["paths"]["/api/v1/auth/mobile/login"]["post"]["responses"]

        rejected = _login_payload()
        rejected["username"] = "operator@example.test"
        rejected["password"] = "must-not-be-accepted"
        response = client.post("/api/v1/auth/mobile/login", json=rejected)
        assert response.status_code == 401
        assert response.json() == {
            "error": {
                "code": "AUTH_INVALID",
                "message": "Operator authentication failed.",
                "details": {},
            }
        }


def test_mobile_login_refresh_rotation_logout_and_credential_isolation(
    settings: Settings,
    engine: Engine,
) -> None:
    enabled = _enabled(settings)
    app = _app(settings, engine)
    with TestClient(app) as client:
        login = client.post("/api/v1/auth/mobile/login", json=_login_payload())
        assert login.status_code == 200
        body = login.json()
        access = body["access_token"]
        refresh = body["refresh_credential"]
        assert access and refresh
        assert body["operator_subject"] == "operator"
        assert authenticate_machine(enabled, f"Bearer {access}") is None
        assert decode_mobile_credential(enabled, _MACHINE_TOKEN, expected_kind="access") is None

        session = client.get(
            "/api/v1/auth/mobile/session",
            headers={"Authorization": f"Bearer {access}"},
        )
        assert session.status_code == 200
        assert session.json()["device"]["device_family"] == "IPHONE"

        rotated = client.post(
            "/api/v1/auth/mobile/refresh",
            headers={"Authorization": f"Bearer {refresh}"},
        )
        assert rotated.status_code == 200
        access2 = rotated.json()["access_token"]
        refresh2 = rotated.json()["refresh_credential"]
        assert refresh2 != refresh

        replay = client.post(
            "/api/v1/auth/mobile/refresh",
            headers={"Authorization": f"Bearer {refresh}"},
        )
        assert replay.status_code == 401

        logout = client.post(
            "/api/v1/auth/mobile/logout",
            headers={"Authorization": f"Bearer {access2}"},
        )
        assert logout.status_code == 204
        denied = client.get(
            "/api/v1/auth/mobile/session",
            headers={"Authorization": f"Bearer {access2}"},
        )
        assert denied.status_code == 401
        assert refresh2


def test_browser_and_native_share_totp_replay_core(settings: Settings, engine: Engine) -> None:
    app = _app(settings, engine)
    code = pyotp.TOTP(_TOTP_SECRET).now()
    with TestClient(app) as client:
        browser = client.post(
            "/api/v1/auth/login",
            headers={"Origin": "http://testserver"},
            json={
                "totp_code": code,
                "trust_browser": False,
            },
        )
        assert browser.status_code == 200
        payload = _login_payload()
        payload["totp_code"] = code
        native = client.post("/api/v1/auth/mobile/login", json=payload)
        assert native.status_code == 401


def test_untrusted_device_receives_no_refresh_credential(settings: Settings, engine: Engine) -> None:
    app = _app(settings, engine)
    with TestClient(app) as client:
        response = client.post(
            "/api/v1/auth/mobile/login",
            json=_login_payload(trust=False),
        )
        assert response.status_code == 200
        assert response.json()["refresh_credential"] is None
        assert response.json()["refresh_expires_at"] is None


def test_concurrent_same_step_totp_login_accepts_exactly_once(settings: Settings, engine: Engine) -> None:
    if engine.dialect.name == "sqlite":
        pytest.skip("row/concurrency test requires PostgreSQL")
    app = _app(settings, engine)
    code = pyotp.TOTP(_TOTP_SECRET).now()

    def attempt() -> int:
        payload = _login_payload()
        payload["totp_code"] = code
        with TestClient(app) as client:
            return client.post("/api/v1/auth/mobile/login", json=payload).status_code

    with ThreadPoolExecutor(max_workers=2) as pool:
        statuses = sorted(pool.map(lambda _: attempt(), range(2)))
    assert statuses == [200, 401]


def test_concurrent_refresh_rotates_once(settings: Settings, engine: Engine) -> None:
    if engine.dialect.name == "sqlite":
        pytest.skip("row/concurrency test requires PostgreSQL")
    app = _app(settings, engine)
    with TestClient(app) as client:
        login = client.post("/api/v1/auth/mobile/login", json=_login_payload())
        refresh = login.json()["refresh_credential"]

    def rotate() -> int:
        with TestClient(app) as client:
            return client.post(
                "/api/v1/auth/mobile/refresh",
                headers={"Authorization": f"Bearer {refresh}"},
            ).status_code

    with ThreadPoolExecutor(max_workers=2) as pool:
        statuses = sorted(pool.map(lambda _: rotate(), range(2)))
    assert statuses == [200, 401]


def test_direct_access_bootstrap_requires_no_totp(settings: Settings, engine: Engine) -> None:
    app = create_app(settings=settings, engine=engine)
    with TestClient(app) as client:
        bootstrap = client.get("/api/v1/client/bootstrap")
        assert bootstrap.status_code == 200
        assert bootstrap.json()["auth_enabled"] is False
        assert client.get("/api/v1/readiness").status_code == 200
