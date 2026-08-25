from __future__ import annotations

import base64
import time
from dataclasses import replace

import pyotp
import pytest
from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from operator_auth import (
    SESSION_COOKIE_NAME,
    TRUSTED_BROWSER_COOKIE_NAME,
    is_operator_auth_exempt,
)
from settings import Settings, SettingsError


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


def _login(
    client: TestClient,
    settings: Settings,
    *,
    trust_browser: bool = False,
    origin: str = "http://testserver",
    totp_code: str | None = None,
):
    assert settings.operator_totp_secret is not None
    code = totp_code or pyotp.TOTP(settings.operator_totp_secret).now()
    return client.post(
        "/api/v1/auth/login",
        headers={"Origin": origin},
        json={
            "username": "operator",
            "password": "correct horse battery staple",
            "totp_code": code,
            "trust_browser": trust_browser,
        },
    )


def test_public_and_downstream_auth_exemptions_are_method_specific() -> None:
    assert is_operator_auth_exempt("GET", "/api/v1/system/health")
    assert is_operator_auth_exempt("POST", "/api/v1/auth/login")
    assert is_operator_auth_exempt("GET", "/api/v1/auth/session")
    assert is_operator_auth_exempt("POST", "/api/v1/auth/logout")

    assert not is_operator_auth_exempt("POST", "/api/v1/system/health")
    assert not is_operator_auth_exempt("GET", "/api/v1/auth/login")
    assert not is_operator_auth_exempt("POST", "/api/v1/auth/session")
    assert not is_operator_auth_exempt("GET", "/api/v1/auth/future-sensitive-route")

    handoff_id = "00000000-0000-0000-0000-000000000001"
    route = f"/api/v1/handoffs/{handoff_id}"

    assert is_operator_auth_exempt("POST", f"{route}/claim")
    assert is_operator_auth_exempt("POST", f"{route}/accept")
    assert is_operator_auth_exempt("POST", f"{route}/reject")
    assert is_operator_auth_exempt("GET", f"{route}/package")
    assert is_operator_auth_exempt("POST", f"{route}/feedback")

    assert not is_operator_auth_exempt("GET", f"{route}/claim")
    assert not is_operator_auth_exempt("GET", f"{route}/feedback")
    assert not is_operator_auth_exempt("POST", f"{route}/package")
    assert not is_operator_auth_exempt("GET", route)


def test_auth_disabled_preserves_direct_operator_access(
    settings: Settings,
    engine: Engine,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))

    response = client.get("/api/v1/system/runtime-configuration")
    session = client.get("/api/v1/auth/session")

    assert response.status_code == 200
    assert session.status_code == 200
    assert session.json() == {
        "authenticated": True,
        "username": "local-operator",
        "trusted_browser": False,
        "auth_enabled": False,
    }


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


def test_invalid_explicit_machine_token_does_not_fall_back_to_browser_session(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    client = TestClient(create_app(settings=secured, engine=engine))
    assert _login(client, secured).status_code == 200

    response = client.get(
        "/api/v1/system/runtime-configuration",
        headers={"Authorization": "Bearer " + "z" * 40},
    )

    assert response.status_code == 401
    assert response.json()["error"]["code"] == "AUTH_REQUIRED"


def test_machine_token_can_make_operator_mutation_without_browser_origin(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    assert secured.api_token is not None
    client = TestClient(create_app(settings=secured, engine=engine))

    response = client.post(
        "/api/v1/ideas/preview",
        headers={"Authorization": f"Bearer {secured.api_token}"},
        json={"idea": "Test a liquid US equity factor after realistic costs."},
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
    assert "Secure" not in cookie_headers
    assert response.headers["Cache-Control"] == "no-store"


def test_successful_totp_step_cannot_be_replayed_to_mint_trusted_browser(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    assert secured.operator_totp_secret is not None
    client = TestClient(create_app(settings=secured, engine=engine))
    code = pyotp.TOTP(secured.operator_totp_secret).now()

    first = _login(client, secured, trust_browser=False, totp_code=code)
    replay = _login(client, secured, trust_browser=True, totp_code=code)

    assert first.status_code == 200
    assert replay.status_code == 401
    assert replay.json()["error"]["code"] == "AUTH_INVALID"
    assert TRUSTED_BROWSER_COOKIE_NAME not in client.cookies


def test_full_login_without_trust_removes_existing_trusted_browser(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    assert secured.operator_totp_secret is not None
    client = TestClient(create_app(settings=secured, engine=engine))
    assert _login(client, secured, trust_browser=True).status_code == 200
    assert TRUSTED_BROWSER_COOKIE_NAME in client.cookies

    totp = pyotp.TOTP(secured.operator_totp_secret)
    next_step_code = totp.at(int(time.time()) + totp.interval)
    response = _login(client, secured, trust_browser=False, totp_code=next_step_code)

    assert response.status_code == 200
    assert response.json()["trusted_browser"] is False
    assert SESSION_COOKIE_NAME in client.cookies
    assert TRUSTED_BROWSER_COOKIE_NAME not in client.cookies


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
    assert response.headers["Cache-Control"] == "no-store"


def test_unencodable_login_text_returns_generic_auth_failure(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    assert secured.operator_totp_secret is not None
    client = TestClient(create_app(settings=secured, engine=engine))

    response = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver", "Content-Type": "application/json"},
        content=(
            '{"username":"\\ud800","password":"correct horse battery staple",'
            f'"totp_code":"{pyotp.TOTP(secured.operator_totp_secret).now()}",'
            '"trust_browser":false}'
        ),
    )

    assert response.status_code == 401
    assert response.json() == {
        "error": {
            "code": "AUTH_INVALID",
            "message": "Invalid operator credentials.",
            "details": {},
        }
    }


def test_invalid_login_shape_does_not_echo_submitted_secrets(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    client = TestClient(create_app(settings=secured, engine=engine))
    submitted_password = "do-not-echo-this-password"

    response = client.post(
        "/api/v1/auth/login",
        headers={"Origin": "http://testserver"},
        json={
            "username": "operator",
            "password": submitted_password,
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
    assert submitted_password not in response.text
    assert response.headers["Cache-Control"] == "no-store"


def test_browser_mutation_requires_configured_origin(settings: Settings, engine: Engine) -> None:
    secured = _enabled_settings(settings)
    client = TestClient(create_app(settings=secured, engine=engine))
    assert _login(client, secured).status_code == 200

    missing = client.post(
        "/api/v1/ideas/preview",
        json={"idea": "Test a liquid US equity factor after realistic costs."},
    )
    mismatched = client.post(
        "/api/v1/ideas/preview",
        headers={"Origin": "https://attacker.example"},
        json={"idea": "Test a liquid US equity factor after realistic costs."},
    )

    assert missing.status_code == 403
    assert missing.json()["error"]["code"] == "AUTH_ORIGIN_REJECTED"
    assert mismatched.status_code == 403
    assert mismatched.json()["error"]["code"] == "AUTH_ORIGIN_REJECTED"


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
    assert response.headers["Cache-Control"] == "no-store"


def test_invalid_trusted_cookie_is_not_reported_as_trusted(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings)
    client = TestClient(create_app(settings=secured, engine=engine))
    assert _login(client, secured, trust_browser=True).status_code == 200

    client.cookies.delete(TRUSTED_BROWSER_COOKIE_NAME)
    client.cookies.set(TRUSTED_BROWSER_COOKIE_NAME, "tampered")

    response = client.get("/api/v1/auth/session")

    assert response.status_code == 200
    assert response.json()["trusted_browser"] is False


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
    assert response.headers["Cache-Control"] == "no-store"
    assert SESSION_COOKIE_NAME not in client.cookies
    assert TRUSTED_BROWSER_COOKIE_NAME not in client.cookies
    assert client.get("/api/v1/auth/session").status_code == 401


def test_enabled_auth_requires_complete_configuration(settings: Settings) -> None:
    partial = replace(
        settings,
        operator_auth_enabled=True,
        operator_username="operator",
    )

    with pytest.raises(SettingsError, match="enabled but incomplete"):
        partial.validate_operator_auth()


def test_production_can_explicitly_keep_auth_disabled(settings: Settings) -> None:
    production = replace(settings, environment="production", operator_auth_enabled=False)

    production.validate_operator_auth()


def test_production_requires_https_and_sets_secure_cookies(
    settings: Settings,
    engine: Engine,
) -> None:
    insecure = replace(_enabled_settings(settings), environment="production")
    with pytest.raises(SettingsError, match="must use https in production"):
        insecure.validate_operator_auth()

    production = replace(
        insecure,
        auth_public_origin="https://testserver",
    )
    production.validate_operator_auth()
    client = TestClient(create_app(settings=production, engine=engine))

    response = _login(
        client,
        production,
        trust_browser=True,
        origin="https://testserver",
    )

    assert response.status_code == 200
    cookie_headers = "\n".join(response.headers.get_list("set-cookie"))
    assert "Secure" in cookie_headers


def test_https_origin_uses_secure_cookies_outside_production(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = replace(
        _enabled_settings(settings),
        auth_public_origin="https://testserver",
    )
    secured.validate_operator_auth()
    client = TestClient(create_app(settings=secured, engine=engine))

    response = _login(
        client,
        secured,
        trust_browser=True,
        origin="https://testserver",
    )

    assert response.status_code == 200
    cookie_headers = "\n".join(response.headers.get_list("set-cookie"))
    assert "Secure" in cookie_headers
