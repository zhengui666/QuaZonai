from __future__ import annotations

import base64
from dataclasses import replace

import pyotp
from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
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


def _login(client: TestClient, settings: Settings) -> None:
    assert settings.operator_totp_secret is not None
    response = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json={
            "username": "operator",
            "password": "correct horse battery staple",
            "totp_code": pyotp.TOTP(settings.operator_totp_secret).now(),
            "trust_browser": True,
        },
    )
    assert response.status_code == 200


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
