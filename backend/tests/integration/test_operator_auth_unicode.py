from __future__ import annotations

import base64
from dataclasses import replace

import pyotp
from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from operator_auth import SESSION_COOKIE_NAME, TRUSTED_BROWSER_COOKIE_NAME
from settings import Settings


def test_unicode_operator_credentials_login_and_trusted_restore(
    settings: Settings,
    engine: Engine,
) -> None:
    username = "操作员"
    password = "正确马电池订书钉-安全密码"
    secured = replace(
        settings,
        operator_auth_enabled=True,
        operator_username=username,
        operator_password=password,
        operator_totp_secret=pyotp.random_base32(),
        auth_cookie_key=base64.b64encode(b"a" * 32).decode("ascii"),
        api_token="machine-token-" + "x" * 32,
        auth_public_origin="http://testserver",
    )
    secured.validate_operator_auth()
    assert secured.operator_totp_secret is not None

    client = TestClient(create_app(settings=secured, engine=engine))
    response = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json={
            "username": username,
            "password": password,
            "totp_code": pyotp.TOTP(secured.operator_totp_secret).now(),
            "trust_browser": True,
        },
    )

    assert response.status_code == 200
    assert response.json()["username"] == username
    assert TRUSTED_BROWSER_COOKIE_NAME in client.cookies

    client.cookies.delete(SESSION_COOKIE_NAME)
    restored = client.get("/api/v1/auth/session")

    assert restored.status_code == 200
    assert restored.json()["authenticated"] is True
    assert restored.json()["username"] == username
