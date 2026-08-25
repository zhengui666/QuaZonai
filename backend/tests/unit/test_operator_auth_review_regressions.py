from __future__ import annotations

import base64
from dataclasses import replace

import pytest
from fastapi import Request

from operator_auth import require_same_origin
from settings import Settings, SettingsError


def _enabled_settings(settings: Settings, **overrides: object) -> Settings:
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


def _mutation_request(origin: str) -> Request:
    return Request(
        {
            "type": "http",
            "http_version": "1.1",
            "method": "POST",
            "scheme": "https",
            "path": "/api/v1/auth/login",
            "raw_path": b"/api/v1/auth/login",
            "query_string": b"",
            "headers": [(b"origin", origin.encode("ascii"))],
            "client": ("203.0.113.10", 43210),
            "server": ("example.com", 443),
        }
    )


def test_enabled_auth_rejects_cookie_key_reuse_with_master_key(settings: Settings) -> None:
    assert settings.master_key is not None
    configured = _enabled_settings(settings, auth_cookie_key=settings.master_key)

    with pytest.raises(SettingsError, match="different from QUAZONAI_MASTER_KEY"):
        configured.validate_operator_auth()


@pytest.mark.parametrize(
    "token",
    [
        "x" * 16 + "\r" + "y" * 16,
        "x" * 16 + "\n" + "y" * 16,
        "x" * 16 + " " + "y" * 16,
        "x" * 32 + "é",
    ],
)
def test_enabled_auth_rejects_machine_tokens_that_cannot_be_bearer_headers(
    settings: Settings,
    token: str,
) -> None:
    configured = _enabled_settings(settings, api_token=token)

    with pytest.raises(SettingsError, match="b64token ASCII"):
        configured.validate_operator_auth()


@pytest.mark.parametrize(
    ("configured_origin", "browser_origin"),
    [
        ("https://EXAMPLE.com:443/", "https://example.com"),
        ("https://éxample.com", "https://xn--xample-9ua.com"),
        ("http://LOCALHOST:80", "http://localhost"),
    ],
)
def test_same_origin_uses_browser_canonical_serialization(
    settings: Settings,
    configured_origin: str,
    browser_origin: str,
) -> None:
    configured = _enabled_settings(settings, auth_public_origin=configured_origin)

    configured.validate_operator_auth()
    assert configured.canonical_auth_public_origin == browser_origin
    require_same_origin(_mutation_request(browser_origin), configured)
