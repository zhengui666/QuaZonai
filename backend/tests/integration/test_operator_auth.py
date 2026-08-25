from __future__ import annotations

import base64
from dataclasses import replace

import pyotp
from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from operator_auth import SESSION_COOKIE_NAME, TRUSTED_BROWSER_COOKIE_NAME
from settings import Settings, SettingsError


def _enabled_settings(settings: Settings) -> Settings:
    return replace(
        settings,
        operator_username="operator",
        operator_password="correct horse battery staple",
        operator_totp_secret=pyotp.random_base32(),
        auth_cookie_key=base64.b64encode(b"a" * 32).decode("ascii"),
        api_token="machine-token-" + "x" * 32,
        auth_public_origin="http://testserver",
    )


def _login(client: TestClient, settings: Settings, *, trust_browser: bool = False):
    assert settings.operator_totp_secret is not None
    return client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json={
            "username": "operator",
            "password": "correct horse battery staple",
            "totp_code": pyotp.TOTP(settings.operator_totp_secret).now(),
            "trust_browser": trust_browser,
        },
    )


def test_protected_operator_api_requires_authentication(settings: Settings, engine: Engine) -> None:
    secured = _enabled_settings(settings)
    client = TestClient(create_app(settings=secured, engine=engine))

    health = client.get("/api/v1/system/health")
    assert health.status_code == 200

    protected = client.get("/api/v1/system/runtime-configuration")
    assert protected.status_code == 401
    assert protected.json()["error"]["code"] == "AUTH_REQUIRED"


def test_machine_token_authenticates_cli_style_requests(settings: Settings, engine: Engine) -> None:
    secured = _enabled_settings(settings)
    assert secured.api_token is not None
    client = TestClient(create_app(settings=secured, engine=engine))

    response = client.get(
        "/api/v1/system/runtime-configuration",
        headers={"Authorization": f"Bearer {secured.api_token}"},
    )

    assert response.status_code == 200


def test_password_totp_login_sets_strict_http_only_cookies(settings: Settings, engine: Engine) -> None:
    secured = _enabled_settings(settings)
    client = TestClient(create_app(settings=secured, engine=engine))

    response = _login(client, secured, trust_browser=True)

    assert response.status_code == 200
    assert response.json() == {
        "authenticated": True,
        "username": "operator",
        "trusted_browser": True,
        "auth_enabled": True,
    }
    cookie_headers = "\n".join(response.headers.get_list("set-cookie"))
    assert SESSION_COOKIE_NAME in cookie_headers
    assert TRUSTED_BROWSER_COOKIE_NAME in cookie_headers
    assert "HttpOnly" in cookie_headers
    assert "SameSite=strict" in cookie_headers


def test_invalid_login_does_not_reveal_failed_factor(settings: Settings, engine: Engine) -> None:
    secured = _enabled_settings(settings)
    assert secured.operator_totp_secret is not None
    client = TestClient(create_app(settings=secured, engine=engine))

    response = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json={
            "username": "operator",
            "password": "wrong-password-value",
            "totp_code": pyotp.TOTP(secured.operator_totp_secret).now(),
            "trust_browser": False,
        },
    )

    assert response.status_code == 401
    assert response.json() == {
        "error": {
            "code": "AUTH_INVALID",
            "message": "Invalid operator credentials.",
            "details": {},
        }
    }


def test_trusted_browser_restores_session_without_password_or_totp(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    client = TestClient(create_app(settings=secured, engine=engine))
    assert _login(client, secured, trust_browser=True).status_code == 200

    client.cookies.delete(SESSION_COOKIE_NAME)
    assert TRUSTED_BROWSER_COOKIE_NAME in client.cookies

    response = client.get("/api/v1/auth/session")

    assert response.status_code == 200
    assert response.json()["trusted_browser"] is True
    assert SESSION_COOKIE_NAME in client.cookies


def test_cookie_key_rotation_revokes_session_and_trusted_browser(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    app = create_app(settings=secured, engine=engine)
    client = TestClient(app)
    assert _login(client, secured, trust_browser=True).status_code == 200

    app.state.settings = replace(
        secured,
        auth_cookie_key=base64.b64encode(b"b" * 32).decode("ascii"),
    )

    response = client.get("/api/v1/auth/session")
    assert response.status_code == 401
    assert response.json()["error"]["code"] == "AUTH_REQUIRED"


def test_logout_requires_origin_and_forgets_trusted_browser(settings: Settings, engine: Engine) -> None:
    secured = _enabled_settings(settings)
    client = TestClient(create_app(settings=secured, engine=engine))
    assert _login(client, secured, trust_browser=True).status_code == 200

    rejected = client.post("/api/v1/auth/logout")
    assert rejected.status_code == 403
    assert rejected.json()["error"]["code"] == "AUTH_ORIGIN_REJECTED"

    response = client.post(
        "/api/v1/auth/logout",
        headers={"Origin": "http://testserver"},
    )
    assert response.status_code == 204
    assert SESSION_COOKIE_NAME not in client.cookies
    assert TRUSTED_BROWSER_COOKIE_NAME not in client.cookies
    assert client.get("/api/v1/auth/session").status_code == 401


def test_partial_auth_configuration_is_rejected(settings: Settings) -> None:
    partial = replace(settings, operator_username="operator")

    try:
        partial.validate_operator_auth()
    except SettingsError as exc:
        assert "partially configured" in str(exc)
    else:
        raise AssertionError("partial authentication configuration must fail")


def test_production_requires_auth_configuration(settings: Settings) -> None:
    production = replace(settings, environment="production")

    try:
        production.validate_operator_auth()
    except SettingsError as exc:
        assert "must be configured in production" in str(exc)
    else:
        raise AssertionError("production without authentication must fail")
