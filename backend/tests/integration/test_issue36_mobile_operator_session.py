from __future__ import annotations

import base64
from dataclasses import replace
from typing import Any

import pyotp
from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from settings import Settings


def _authenticated_settings(settings: Settings) -> Settings:
    return replace(
        settings,
        operator_auth_enabled=True,
        operator_username="legacy-browser-operator",
        operator_password="correct horse battery staple",
        operator_totp_secret=base64.b32encode(b"issue-36-native-totp-secret-material").decode(),
        auth_cookie_key=base64.b64encode(b"c" * 32).decode(),
        api_token="issue36-machine-operator-token-000000000000000000000000000000000000",
        auth_public_origin="http://testserver",
    )


def _mobile_login_path(document: dict[str, Any]) -> str:
    assert "/api/v1/auth/mobile/login" in document["paths"]
    return "/api/v1/auth/mobile/login"


def _device_revoke_path(document: dict[str, Any], device_id: str) -> str:
    matches = [
        path
        for path in document["paths"]
        if path.startswith("/api/v1/auth/mobile/devices/{") and path.endswith("}/revoke")
    ]
    assert len(matches) == 1
    return matches[0].replace(matches[0][matches[0].index("{") : matches[0].index("}") + 1], device_id)


def _login_payload(settings: Settings, *, trust_device: bool) -> dict[str, object]:
    assert settings.operator_totp_secret is not None
    return {
        "totp_code": pyotp.TOTP(settings.operator_totp_secret).now(),
        "installation_id": "72f668c0-f2f9-4bc2-adc2-b49f307eb9ef",
        "device_name": "Issue 36 iPhone",
        "device_family": "IPHONE",
        "os_version": "18.0",
        "app_version": "1.0.0",
        "app_build": "100",
        "trust_device": trust_device,
    }


def test_mobile_session_rotation_and_revoke(settings: Settings, engine: Engine) -> None:
    runtime_settings = _authenticated_settings(settings)
    app = create_app(settings=runtime_settings, engine=engine)
    document = app.openapi()
    client = TestClient(app, base_url="http://testserver")

    bootstrap = client.get("/api/v1/client/bootstrap")
    assert bootstrap.status_code == 200
    assert bootstrap.headers["cache-control"] == "no-store"
    assert bootstrap.json()["auth_enabled"] is True

    login = client.post(_mobile_login_path(document), json=_login_payload(runtime_settings, trust_device=True))
    assert login.status_code == 200, login.text
    assert login.headers["cache-control"] == "no-store"
    payload = login.json()
    access = payload["access_token"]
    refresh = payload["refresh_credential"]
    device_id = payload["device"]["id"]
    assert payload["token_type"] == "Bearer"
    assert access.startswith("qzm1.")
    assert refresh.startswith("qzm1.")
    assert access != refresh

    session = client.get(
        "/api/v1/auth/mobile/session",
        headers={"Authorization": f"Bearer {access}"},
    )
    assert session.status_code == 200
    assert session.json()["operator_subject"] == "operator"
    assert session.json()["device"]["id"] == device_id

    devices = client.get(
        "/api/v1/auth/mobile/devices",
        headers={"Authorization": f"Bearer {access}"},
    )
    assert devices.status_code == 200
    assert [item["id"] for item in devices.json()] == [device_id]
    assert all("username" not in item for item in devices.json())

    rotated = client.post(
        "/api/v1/auth/mobile/refresh",
        headers={"Authorization": f"Bearer {refresh}"},
    )
    assert rotated.status_code == 200, rotated.text
    assert rotated.headers["cache-control"] == "no-store"
    rotated_payload = rotated.json()
    rotated_access = rotated_payload["access_token"]
    rotated_refresh = rotated_payload["refresh_credential"]
    assert rotated_refresh != refresh

    old_refresh = client.post(
        "/api/v1/auth/mobile/refresh",
        headers={"Authorization": f"Bearer {refresh}"},
    )
    assert old_refresh.status_code == 401

    old_access = client.get(
        "/api/v1/auth/mobile/session",
        headers={"Authorization": f"Bearer {access}"},
    )
    assert old_access.status_code == 401

    revoke = client.post(
        _device_revoke_path(document, device_id),
        headers={"Authorization": f"Bearer {rotated_access}"},
        json={},
    )
    assert revoke.status_code in {200, 204}, revoke.text

    revoked_access = client.get(
        "/api/v1/auth/mobile/session",
        headers={"Authorization": f"Bearer {rotated_access}"},
    )
    assert revoked_access.status_code == 401
    revoked_refresh = client.post(
        "/api/v1/auth/mobile/refresh",
        headers={"Authorization": f"Bearer {rotated_refresh}"},
    )
    assert revoked_refresh.status_code == 401


def test_mobile_login_schema_rejects_legacy_factors_without_echo(
    settings: Settings,
    engine: Engine,
) -> None:
    runtime_settings = _authenticated_settings(settings)
    app = create_app(settings=runtime_settings, engine=engine)
    client = TestClient(app, base_url="http://testserver")
    payload = _login_payload(runtime_settings, trust_device=False)
    payload["username"] = "must-not-be-accepted"
    payload["password"] = "must-not-be-accepted-password"

    response = client.post("/api/v1/auth/mobile/login", json=payload)
    assert response.status_code in {401, 422}
    assert response.headers["cache-control"] == "no-store"
    assert str(payload["totp_code"]) not in response.text
    assert str(payload["password"]) not in response.text


def test_untrusted_mobile_login_does_not_issue_refresh_credential(
    settings: Settings,
    engine: Engine,
) -> None:
    runtime_settings = _authenticated_settings(settings)
    client = TestClient(create_app(settings=runtime_settings, engine=engine), base_url="http://testserver")
    response = client.post(
        "/api/v1/auth/mobile/login",
        json=_login_payload(runtime_settings, trust_device=False),
    )
    assert response.status_code == 200, response.text
    assert response.json().get("refresh_credential") is None
    assert response.json().get("refresh_expires_at") is None


def test_direct_access_bootstrap_remains_full_operator_mode(
    settings: Settings,
    engine: Engine,
) -> None:
    app = create_app(settings=settings, engine=engine)
    client = TestClient(app, base_url="http://testserver")
    bootstrap = client.get("/api/v1/client/bootstrap")
    assert bootstrap.status_code == 200
    assert bootstrap.json()["auth_enabled"] is False
    readiness = client.get("/api/v1/readiness")
    assert readiness.status_code == 200
