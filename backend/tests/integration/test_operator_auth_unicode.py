from __future__ import annotations

import base64
from dataclasses import replace

import pyotp
import pytest
from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
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


@pytest.mark.parametrize("totp_code", ["１２３４５６", "١٢٣٤٥٦", "12345", "1234567", "12 3456"])
def test_login_rejects_non_ascii_or_non_six_digit_totp(
    settings: Settings,
    engine: Engine,
    totp_code: str,
) -> None:
    secured = _enabled_settings(settings)
    client = TestClient(create_app(settings=secured, engine=engine))

    response = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json={"totp_code": totp_code, "trust_browser": False},
    )

    assert response.status_code == 401
    assert response.json()["error"]["code"] == "AUTH_INVALID"
    assert totp_code not in response.text
