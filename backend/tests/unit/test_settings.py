from __future__ import annotations

import base64
from dataclasses import replace

import pytest

from settings import (
    DEFAULT_AUTH_SESSION_TTL_SECONDS,
    DEFAULT_AUTH_TRUSTED_BROWSER_TTL_DAYS,
    Settings,
    SettingsError,
)


def _enabled_auth(settings: Settings, **overrides: object) -> Settings:
    values: dict[str, object] = {
        "operator_auth_enabled": True,
        "operator_username": "operator",
        "operator_password": "correct horse battery staple",
        "operator_totp_secret": "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",
        "auth_cookie_key": base64.b64encode(b"a" * 32).decode("ascii"),
        "api_token": "machine-token-" + "x" * 32,
        "auth_public_origin": "https://quazonai.example.com",
    }
    values.update(overrides)
    return replace(settings, **values)


def test_master_key_requires_exactly_32_decoded_bytes(settings: Settings) -> None:
    assert settings.master_key_configured is True

    invalid = replace(settings, master_key=base64.b64encode(b"short").decode("ascii"))
    assert invalid.master_key_configured is False


def test_database_scheme_rejects_remote_style_unknown_driver(settings: Settings) -> None:
    invalid = replace(settings, database_url="mysql://localhost/quazonai")
    try:
        invalid.validate_database_scheme()
    except SettingsError as exc:
        assert "postgresql+psycopg" in str(exc)
    else:
        raise AssertionError("unsupported database scheme should fail")


def test_disabled_auth_ignores_dormant_ttl_values(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "false")
    monkeypatch.setenv("QUAZONAI_AUTH_SESSION_TTL_SECONDS", "not-an-integer")
    monkeypatch.setenv("QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS", "0")

    configured = Settings.from_env()

    assert configured.auth_enabled is False
    assert configured.auth_session_ttl_seconds == DEFAULT_AUTH_SESSION_TTL_SECONDS
    assert configured.auth_trusted_browser_ttl_days == DEFAULT_AUTH_TRUSTED_BROWSER_TTL_DAYS


def test_enabled_auth_rejects_invalid_ttl_values(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "true")
    monkeypatch.setenv("QUAZONAI_AUTH_SESSION_TTL_SECONDS", "not-an-integer")

    with pytest.raises(SettingsError, match="must be an integer"):
        Settings.from_env()


def test_environment_is_trimmed_before_security_policy(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("QUAZONAI_ENV", " production ")

    configured = Settings.from_env()

    assert configured.environment == "production"


def test_production_whitespace_cannot_bypass_https_requirement(settings: Settings) -> None:
    configured = _enabled_auth(
        settings,
        environment="production ",
        auth_public_origin="http://quazonai.example.com",
    )

    with pytest.raises(SettingsError, match="must use https in production"):
        configured.validate_operator_auth()


def test_enabled_auth_rejects_password_longer_than_login_schema(settings: Settings) -> None:
    configured = _enabled_auth(settings, operator_password="x" * 4097)

    with pytest.raises(SettingsError, match="between 12 and 4096"):
        configured.validate_operator_auth()


def test_enabled_auth_rejects_unencodable_configured_credentials(settings: Settings) -> None:
    configured = _enabled_auth(settings, operator_username="operator\ud800")

    with pytest.raises(SettingsError, match="valid Unicode text"):
        configured.validate_operator_auth()


@pytest.mark.parametrize(
    "origin",
    [
        "https://exa mple.com",
        "https://example.com:abc",
        "https://example.com:70000",
        "https://-bad.example.com",
        "https://bad-.example.com",
        "https://example..com",
        "https://999.999.999.999",
        "https://exa\nmple.com",
        "https://example.\tcom",
        "ht\ntps://example.com",
        "https://example.com\r",
    ],
)
def test_enabled_auth_rejects_invalid_origin_host_or_port(
    settings: Settings,
    origin: str,
) -> None:
    configured = _enabled_auth(settings, auth_public_origin=origin)

    with pytest.raises(SettingsError, match="AUTH_PUBLIC_ORIGIN"):
        configured.validate_operator_auth()


@pytest.mark.parametrize(
    "origin",
    [
        "https://quazonai.example.com",
        "https://localhost:8443",
        "https://127.0.0.1:8443",
        "https://[::1]:8443",
    ],
)
def test_enabled_auth_accepts_valid_origin_hosts(settings: Settings, origin: str) -> None:
    configured = _enabled_auth(settings, auth_public_origin=origin)

    configured.validate_operator_auth()
