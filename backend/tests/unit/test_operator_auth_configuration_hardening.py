from __future__ import annotations

import base64
import secrets
from dataclasses import replace

import pytest

from settings import Settings, SettingsError, canonicalize_http_origin


def _enabled_auth(settings: Settings, **overrides: object) -> Settings:
    values: dict[str, object] = {
        "operator_auth_enabled": True,
        "operator_totp_secret": "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",
        "auth_cookie_key": base64.b64encode(b"a" * 32).decode("ascii"),
        "api_token": secrets.token_urlsafe(32),
        "auth_public_origin": "https://quazonai.example.com",
    }
    values.update(overrides)
    return replace(settings, **values)


@pytest.mark.parametrize(
    ("raw", "canonical"),
    [
        ("https://EXAMPLE.com:443", "https://example.com"),
        ("http://LOCALHOST:80/", "http://localhost"),
        ("https://bücher.example", "https://xn--bcher-kva.example"),
        ("https://[2001:0DB8:0:0:0:0:0:1]:443", "https://[2001:db8::1]"),
        ("https://[::ffff:127.0.0.1]:443", "https://[::ffff:7f00:1]"),
        ("https://[0:0:0:0:0:ffff:7f00:1]:443", "https://[::ffff:7f00:1]"),
        ("https://Example.com:8443", "https://example.com:8443"),
        ("http://127.0.0.1:8080", "http://127.0.0.1:8080"),
    ],
)
def test_canonicalize_http_origin_matches_browser_serialization(
    raw: str,
    canonical: str,
) -> None:
    assert canonicalize_http_origin(raw) == canonical


@pytest.mark.parametrize(
    "origin",
    [
        "https://user@example.com",
        "https://@example.com",
        "https://:@example.com",
        "https://example.com/path",
        "https://example.com?",
        "https://example.com#",
        "https://example.com/;",
        "https://example.com?query=1",
        "https://example.com#fragment",
        "https://example.com:",
        "https://example.com: ",
        "https://[::1]:",
        "https://[::1]: ",
        "https://example.com:0",
        "https://example.com:70000",
        "https://[fe80::1%25eth0]",
        "https://999.999.999.999",
        "http://0x7f.1",
        "http://0177.0.0.1",
        "http://127.1",
        "http://2130706433",
        "http://0x",
        "http://0X",
        "https://example.com\u00a0",
        "https://[::1",
        "https://[abc]",
    ],
)
def test_canonicalize_http_origin_rejects_non_origin_values(origin: str) -> None:
    with pytest.raises(SettingsError):
        canonicalize_http_origin(origin)


def test_enabled_auth_requires_cookie_key_separate_from_master_key(
    settings: Settings,
) -> None:
    assert settings.master_key is not None
    configured = _enabled_auth(settings, auth_cookie_key=settings.master_key)

    with pytest.raises(SettingsError, match="must be different from QUAZONAI_MASTER_KEY"):
        configured.validate_operator_auth()


def test_enabled_auth_accepts_independent_cookie_and_master_keys(settings: Settings) -> None:
    configured = _enabled_auth(settings)

    configured.validate_operator_auth()


@pytest.mark.parametrize(
    "token",
    [
        secrets.token_urlsafe(32),
        "Abcdefghijklmnopqrstuvwxyz012345+/==",
        "machine-token_~.0123456789ABCDEFGHIJK",
    ],
)
def test_enabled_auth_accepts_header_safe_bearer_tokens(
    settings: Settings,
    token: str,
) -> None:
    configured = _enabled_auth(settings, api_token=token)

    configured.validate_operator_auth()


@pytest.mark.parametrize(
    "token",
    [
        "x" * 31,
        "x" * 32 + " ",
        "x" * 32 + "\n",
        "x" * 16 + "\r\n" + "x" * 16,
        "x" * 31 + "é",
        "x" * 31 + ":",
        "x" * 16 + "=" + "x" * 16,
        "x" * 31 + "\x00",
    ],
)
def test_enabled_auth_rejects_tokens_unsafe_for_authorization_header(
    settings: Settings,
    token: str,
) -> None:
    configured = _enabled_auth(settings, api_token=token)

    with pytest.raises(SettingsError, match="API_TOKEN"):
        configured.validate_operator_auth()


def test_from_env_preserves_invalid_machine_token_for_fail_closed_validation(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "true")
    monkeypatch.setenv(
        "QUAZONAI_AUTH_TOTP_SECRET",
        "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",
    )
    monkeypatch.setenv(
        "QUAZONAI_AUTH_COOKIE_KEY",
        base64.b64encode(b"a" * 32).decode("ascii"),
    )
    monkeypatch.setenv("QUAZONAI_API_TOKEN", "x" * 32 + "\n")
    monkeypatch.setenv("QUAZONAI_AUTH_PUBLIC_ORIGIN", "https://example.com")

    configured = Settings.from_env()

    assert configured.api_token == "x" * 32 + "\n"
    with pytest.raises(SettingsError, match="b64token"):
        configured.validate_operator_auth()


def test_settings_exposes_canonical_origin_and_secure_cookie_semantics(
    settings: Settings,
) -> None:
    configured = _enabled_auth(
        settings,
        auth_public_origin="HTTPS://EXAMPLE.com:443",
    )

    configured.validate_operator_auth()
    assert configured.canonical_auth_public_origin == "https://example.com"
    assert configured.auth_cookie_secure is True
