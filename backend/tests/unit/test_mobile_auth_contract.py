from __future__ import annotations

import base64
from dataclasses import replace
from typing import Any

import pyotp
from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from settings import Settings

TOTP_SECRET = base64.b32encode(b"native-auth-contract-secret-material").decode("ascii")
COOKIE_KEY = base64.b64encode(b"m" * 32).decode("ascii")
MACHINE_TOKEN = "native-contract-machine-token-0123456789abcdef"


def auth_settings(settings: Settings) -> Settings:
    return replace(
        settings,
        operator_auth_enabled=True,
        operator_totp_secret=TOTP_SECRET,
        auth_cookie_key=COOKIE_KEY,
        api_token=MACHINE_TOKEN,
        auth_public_origin="https://quazonai.test",
    )


def client(settings: Settings, engine: Engine) -> TestClient:
    return TestClient(create_app(settings=settings, engine=engine))


def login_payload(*, trust_device: bool = True) -> dict[str, object]:
    return {
        "totp_code": pyotp.TOTP(TOTP_SECRET).now(),
        "installation_id": "4f3ab4f8-1179-41bd-8209-7a51b75fd39a",
        "device_name": "Contract iPhone",
        "device_family": "IPHONE",
        "os_version": "18.6",
        "app_version": "1.0.0",
        "app_build": "100",
        "trust_device": trust_device,
    }


def resolve_schema(schema: dict[str, Any], document: dict[str, Any]) -> dict[str, Any]:
    reference = schema.get("$ref")
    if not isinstance(reference, str):
        return schema
    name = reference.rsplit("/", 1)[-1]
    return resolve_schema(document["components"]["schemas"][name], document)


def test_mobile_login_openapi_is_totp_only(settings: Settings, engine: Engine) -> None:
    document = create_app(settings=auth_settings(settings), engine=engine).openapi()
    operation = document["paths"]["/api/v1/auth/mobile/login"]["post"]
    schema = resolve_schema(
        operation["requestBody"]["content"]["application/json"]["schema"],
        document,
    )
    properties = set(schema["properties"])
    assert "totp_code" in properties
    assert "username" not in properties
    assert "password" not in properties
    assert schema.get("additionalProperties") is False


def test_mobile_login_rejects_legacy_fields_without_echoing_secrets(
    settings: Settings,
    engine: Engine,
) -> None:
    payload = login_payload()
    payload["username"] = "must-not-be-accepted"
    payload["password"] = "must-not-be-accepted-either"
    response = client(auth_settings(settings), engine).post(
        "/api/v1/auth/mobile/login",
        json=payload,
    )
    assert response.status_code in {401, 422}
    assert response.headers["cache-control"] == "no-store"
    body = response.text
    assert "must-not-be-accepted" not in body
    assert str(payload["totp_code"]) not in body


def test_mobile_trusted_device_refresh_rotation_and_revoke(
    settings: Settings,
    engine: Engine,
) -> None:
    api = client(auth_settings(settings), engine)
    login = api.post("/api/v1/auth/mobile/login", json=login_payload())
    assert login.status_code == 200, login.text
    assert login.headers["cache-control"] == "no-store"
    tokens = login.json()
    access = tokens["access_token"]
    refresh = tokens["refresh_credential"]
    assert access.startswith("qzm1.")
    assert refresh.startswith("qzm1.")

    authorization = {"Authorization": f"Bearer {access}"}
    session = api.get("/api/v1/auth/mobile/session", headers=authorization)
    assert session.status_code == 200
    assert session.json()["authenticated"] is True
    assert session.json().get("operator_subject") == "operator"

    devices = api.get("/api/v1/auth/mobile/devices", headers=authorization)
    assert devices.status_code == 200
    items = devices.json()
    assert len(items) == 1
    device_id = items[0]["id"]
    assert "username" not in items[0]

    rotated = api.post(
        "/api/v1/auth/mobile/refresh",
        headers={"Authorization": f"Bearer {refresh}"},
    )
    assert rotated.status_code == 200, rotated.text
    rotated_tokens = rotated.json()
    assert rotated_tokens["refresh_credential"] != refresh

    replay = api.post(
        "/api/v1/auth/mobile/refresh",
        headers={"Authorization": f"Bearer {refresh}"},
    )
    assert replay.status_code == 401

    new_access = rotated_tokens["access_token"]
    revoke = api.post(
        f"/api/v1/auth/mobile/devices/{device_id}/revoke",
        headers={"Authorization": f"Bearer {new_access}"},
    )
    assert revoke.status_code in {200, 204}
    rejected = api.get(
        "/api/v1/auth/mobile/session",
        headers={"Authorization": f"Bearer {new_access}"},
    )
    assert rejected.status_code == 401


def test_mobile_untrusted_login_does_not_issue_refresh_credential(
    settings: Settings,
    engine: Engine,
) -> None:
    response = client(auth_settings(settings), engine).post(
        "/api/v1/auth/mobile/login",
        json=login_payload(trust_device=False),
    )
    assert response.status_code == 200, response.text
    body = response.json()
    assert body["access_token"].startswith("qzm1.")
    assert body.get("refresh_credential") is None


def test_direct_access_bootstrap_requires_no_totp(settings: Settings, engine: Engine) -> None:
    api = client(settings, engine)
    response = api.get("/api/v1/client/bootstrap")
    assert response.status_code == 200
    body = response.json()
    assert body["auth_enabled"] is False
    assert body["operator_client_capability_epoch"] >= body["minimum_ios_capability_epoch"]
