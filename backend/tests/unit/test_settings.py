from __future__ import annotations

import base64
import ipaddress
from dataclasses import replace

import pytest

from settings import (
    DEFAULT_AUTH_SESSION_TTL_SECONDS,
    DEFAULT_AUTH_TRUSTED_BROWSER_TTL_DAYS,
    MAX_AUTH_SESSION_TTL_SECONDS,
    MAX_AUTH_TRUSTED_BROWSER_TTL_DAYS,
    MIN_AUTH_SESSION_TTL_SECONDS,
    MIN_AUTH_TRUSTED_BROWSER_TTL_DAYS,
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


@pytest.mark.parametrize(
    ("field", "value", "setting_name"),
    [
        (
            "auth_session_ttl_seconds",
            MIN_AUTH_SESSION_TTL_SECONDS - 1,
            "QUAZONAI_AUTH_SESSION_TTL_SECONDS",
        ),
        (
            "auth_session_ttl_seconds",
            MAX_AUTH_SESSION_TTL_SECONDS + 1,
            "QUAZONAI_AUTH_SESSION_TTL_SECONDS",
        ),
        (
            "auth_trusted_browser_ttl_days",
            MIN_AUTH_TRUSTED_BROWSER_TTL_DAYS - 1,
            "QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS",
        ),
        (
            "auth_trusted_browser_ttl_days",
            MAX_AUTH_TRUSTED_BROWSER_TTL_DAYS + 1,
            "QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS",
        ),
    ],
)
def test_direct_enabled_auth_rejects_out_of_bounds_ttls(
    settings: Settings,
    field: str,
    value: int,
    setting_name: str,
) -> None:
    configured = _enabled_auth(settings, **{field: value})

    with pytest.raises(SettingsError, match=f"{setting_name} must be between"):
        configured.validate_operator_auth()


@pytest.mark.parametrize(
    ("field", "value", "setting_name"),
    [
        ("auth_session_ttl_seconds", 300.0, "QUAZONAI_AUTH_SESSION_TTL_SECONDS"),
        ("auth_trusted_browser_ttl_days", True, "QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS"),
    ],
)
def test_direct_enabled_auth_rejects_non_integer_ttls(
    settings: Settings,
    field: str,
    value: object,
    setting_name: str,
) -> None:
    configured = _enabled_auth(settings, **{field: value})

    with pytest.raises(SettingsError, match=f"{setting_name} must be an integer"):
        configured.validate_operator_auth()


def test_environment_defaults_to_development(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("QUAZONAI_ENV", raising=False)

    configured = Settings.from_env()

    assert configured.environment == "development"


@pytest.mark.parametrize(
    ("configured_value", "expected"),
    [
        (" development ", "development"),
        ("TEST", "test"),
        (" production ", "production"),
    ],
)
def test_environment_is_normalized_to_an_allowed_value(
    monkeypatch: pytest.MonkeyPatch,
    configured_value: str,
    expected: str,
) -> None:
    monkeypatch.setenv("QUAZONAI_ENV", configured_value)

    configured = Settings.from_env()

    assert configured.environment == expected


@pytest.mark.parametrize("configured_value", ["prod", "production-like", "staging"])
def test_from_env_rejects_unknown_security_environment(
    monkeypatch: pytest.MonkeyPatch,
    configured_value: str,
) -> None:
    monkeypatch.setenv("QUAZONAI_ENV", configured_value)

    with pytest.raises(SettingsError, match="QUAZONAI_ENV must be one of"):
        Settings.from_env()


def test_production_whitespace_cannot_bypass_https_requirement(settings: Settings) -> None:
    configured = _enabled_auth(
        settings,
        environment="production ",
        auth_public_origin="http://quazonai.example.com",
    )

    with pytest.raises(SettingsError, match="must use https in production"):
        configured.validate_operator_auth()


def test_unknown_environment_cannot_bypass_production_security_policy(settings: Settings) -> None:
    configured = _enabled_auth(
        settings,
        environment="production-like",
        auth_public_origin="http://quazonai.example.com",
    )

    with pytest.raises(SettingsError, match="QUAZONAI_ENV must be one of"):
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
        # WHATWG special-URL host parsing sends any hostname ending in a
        # number to the IPv4 parser, so these are rejected by browsers rather
        # than being valid DNS origins.
        "https://example.127",
        "https://example.0127",
        "https://example.0x",
        "https://example.0x7f",
        "https://example.\uff11\uff12\uff17",
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
        # A number must be the final label to trigger WHATWG's IPv4 parser.
        "https://127.example",
        "https://example.127abc",
        "https://example.0xg",
        "https://example.127.example",
    ],
)
def test_enabled_auth_accepts_valid_origin_hosts(settings: Settings, origin: str) -> None:
    configured = _enabled_auth(settings, auth_public_origin=origin)

    configured.validate_operator_auth()


def test_disabled_auth_ignores_dormant_trusted_proxy_configuration(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "false")
    monkeypatch.setenv("QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS", "not-a-cidr")

    configured = Settings.from_env()

    assert configured.auth_trusted_proxy_cidrs == ()


@pytest.mark.parametrize(
    "configured_value",
    [
        "not-a-cidr",
        "10.0.0.1/24",
        "192.0.2.1,,2001:db8::1",
        "0.0.0.0/0",
        "::/0",
    ],
)
def test_enabled_auth_rejects_unsafe_trusted_proxy_cidrs(
    monkeypatch: pytest.MonkeyPatch,
    configured_value: str,
) -> None:
    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "true")
    monkeypatch.setenv("QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS", configured_value)

    with pytest.raises(SettingsError, match="QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS"):
        Settings.from_env()


def test_enabled_auth_parses_exact_trusted_proxy_cidrs(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "true")
    monkeypatch.setenv(
        "QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS",
        "127.0.0.1/32, 2001:db8:1::/64",
    )

    configured = Settings.from_env()

    assert tuple(str(network) for network in configured.auth_trusted_proxy_cidrs) == (
        "127.0.0.1/32",
        "2001:db8:1::/64",
    )


@pytest.mark.parametrize(
    "configured_cidrs",
    [
        (ipaddress.ip_network("0.0.0.0/0"),),
        ("not-an-ip-network",),
        [ipaddress.ip_network("127.0.0.1/32")],
    ],
)
def test_direct_enabled_auth_validates_trusted_proxy_networks(
    settings: Settings,
    configured_cidrs: object,
) -> None:
    configured = _enabled_auth(settings, auth_trusted_proxy_cidrs=configured_cidrs)

    with pytest.raises(SettingsError, match="QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS"):
        configured.validate_operator_auth()
