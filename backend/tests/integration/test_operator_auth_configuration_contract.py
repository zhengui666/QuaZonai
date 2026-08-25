from __future__ import annotations

import base64
from dataclasses import replace

import pyotp
import pytest

from settings import Settings, SettingsError


def _complete_auth(settings: Settings, *, environment: str = "test") -> Settings:
    origin = (
        "https://quazonai.example.com"
        if environment.casefold() == "production"
        else "http://testserver"
    )
    return replace(
        settings,
        environment=environment,
        operator_auth_enabled=True,
        operator_username="operator",
        operator_password="correct horse battery staple",
        operator_totp_secret=pyotp.random_base32(),
        auth_cookie_key=base64.b64encode(b"a" * 32).decode("ascii"),
        api_token="machine-token-" + "x" * 32,
        auth_public_origin=origin,
    )


def test_production_cannot_disable_operator_authentication(settings: Settings) -> None:
    production = replace(settings, environment="production", operator_auth_enabled=False)

    with pytest.raises(SettingsError, match="must be enabled in production"):
        production.validate_operator_auth()


def test_disabled_auth_rejects_partially_configured_credentials(settings: Settings) -> None:
    partial = replace(
        settings,
        operator_auth_enabled=False,
        operator_username="operator",
    )

    with pytest.raises(SettingsError, match="disabled but authentication values are configured"):
        partial.validate_operator_auth()


def test_complete_development_auth_configuration_is_valid(settings: Settings) -> None:
    configured = _complete_auth(settings)

    configured.validate_operator_auth()
    assert configured.auth_enabled is True
    assert configured.auth_cookie_secure is False


def test_https_origin_uses_secure_cookies_outside_production(settings: Settings) -> None:
    configured = replace(
        _complete_auth(settings),
        environment="staging",
        auth_public_origin="https://staging.quazonai.example.com",
    )

    configured.validate_operator_auth()
    assert configured.auth_cookie_secure is True


def test_complete_production_auth_configuration_is_valid_and_secure(
    settings: Settings,
) -> None:
    configured = _complete_auth(settings, environment="production")

    configured.validate_operator_auth()
    assert configured.auth_cookie_secure is True


def test_production_rejects_non_https_public_origin(settings: Settings) -> None:
    configured = replace(
        _complete_auth(settings, environment="production"),
        auth_public_origin="http://quazonai.example.com",
    )

    with pytest.raises(SettingsError, match="must use https in production"):
        configured.validate_operator_auth()
