from __future__ import annotations

import base64
import json
import secrets
import time
from dataclasses import replace

import pytest
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from api.auth import LoginInput
from operator_auth import (
    COOKIE_NONCE_BYTES,
    COOKIE_VERSION,
    OPERATOR_SUBJECT,
    _read_cookie,
    _urlsafe_encode,
    authenticate_machine,
)
from settings import Settings, SettingsError


def _enabled_settings(settings: Settings) -> Settings:
    return replace(
        settings,
        operator_auth_enabled=True,
        operator_totp_secret="JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",
        auth_cookie_key=base64.b64encode(b"b" * 32).decode("ascii"),
        api_token="machine-token-" + "x" * 32,
        auth_public_origin="https://quazonai.example.com",
    )


def _configure_enabled_env(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("QUAZONAI_ENV", "test")
    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "true")
    monkeypatch.setenv(
        "QUAZONAI_AUTH_TOTP_SECRET",
        "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",
    )
    monkeypatch.setenv(
        "QUAZONAI_AUTH_COOKIE_KEY",
        base64.b64encode(b"b" * 32).decode("ascii"),
    )
    monkeypatch.setenv("QUAZONAI_API_TOKEN", "machine-token-" + "x" * 32)
    monkeypatch.setenv("QUAZONAI_AUTH_PUBLIC_ORIGIN", "https://quazonai.example.com")


def test_login_schema_is_totp_only_and_forbids_legacy_fields() -> None:
    assert set(LoginInput.model_fields) == {"totp_code", "trust_browser"}
    assert LoginInput(totp_code="123456").trust_browser is False

    with pytest.raises(ValueError):
        LoginInput.model_validate(
            {
                "username": "legacy",
                "password": "legacy",
                "totp_code": "123456",
            }
        )


@pytest.mark.parametrize("legacy_name", ["QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_PASSWORD"])
def test_enabled_auth_rejects_nonempty_legacy_browser_credentials_without_echo(
    monkeypatch: pytest.MonkeyPatch,
    legacy_name: str,
) -> None:
    _configure_enabled_env(monkeypatch)
    legacy_value = "legacy-secret-must-not-appear"
    monkeypatch.setenv(legacy_name, legacy_value)

    with pytest.raises(SettingsError) as raised:
        Settings.from_env()

    message = str(raised.value)
    assert legacy_name in message
    assert legacy_value not in message


@pytest.mark.parametrize(
    ("legacy_name", "presence_marker"),
    [
        ("QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT"),
        ("QUAZONAI_AUTH_PASSWORD", "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT"),
    ],
)
def test_enabled_auth_rejects_compose_names_only_legacy_markers(
    monkeypatch: pytest.MonkeyPatch,
    legacy_name: str,
    presence_marker: str,
) -> None:
    _configure_enabled_env(monkeypatch)
    monkeypatch.delenv(legacy_name, raising=False)
    monkeypatch.setenv(presence_marker, "true")

    with pytest.raises(SettingsError) as raised:
        Settings.from_env()

    message = str(raised.value)
    assert legacy_name in message
    assert presence_marker not in message


@pytest.mark.parametrize("legacy_name", ["QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_PASSWORD"])
def test_empty_legacy_browser_credentials_do_not_enter_settings(
    monkeypatch: pytest.MonkeyPatch,
    legacy_name: str,
) -> None:
    _configure_enabled_env(monkeypatch)
    monkeypatch.setenv(legacy_name, "")

    configured = Settings.from_env()
    configured.validate_operator_auth()

    assert not hasattr(configured, "operator_username")
    assert not hasattr(configured, "operator_password")
    assert "operator_username" not in repr(configured)
    assert "operator_password" not in repr(configured)


def test_disabled_auth_keeps_legacy_values_dormant(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "false")
    monkeypatch.setenv("QUAZONAI_AUTH_USERNAME", "legacy-user")
    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", "legacy-password")
    monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT", "true")
    monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT", "true")

    configured = Settings.from_env()

    assert configured.auth_enabled is False
    assert not hasattr(configured, "operator_username")
    assert not hasattr(configured, "operator_password")


def test_machine_authentication_uses_fixed_operator_subject(settings: Settings) -> None:
    configured = _enabled_settings(settings)
    assert configured.api_token is not None

    identity = authenticate_machine(configured, f"Bearer {configured.api_token}")

    assert identity is not None
    assert identity.username == OPERATOR_SUBJECT == "local-operator"


def test_cookie_version_three_rejects_a_version_two_session(settings: Settings) -> None:
    configured = _enabled_settings(settings)
    assert COOKIE_VERSION == 3
    issued_at = int(time.time())
    payload = json.dumps(
        {
            "v": 2,
            "kind": "session",
            "sub": OPERATOR_SUBJECT,
            "iat": issued_at,
            "exp": issued_at + 300,
        },
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    nonce = secrets.token_bytes(COOKIE_NONCE_BYTES)
    old_aad = b"quazonai|operator-auth|cookie=session|version=2"
    ciphertext = AESGCM(configured.auth_cookie_key_bytes()).encrypt(nonce, payload, old_aad)
    old_cookie = _urlsafe_encode(nonce + ciphertext)

    assert _read_cookie(configured, old_cookie, kind="session") is None
