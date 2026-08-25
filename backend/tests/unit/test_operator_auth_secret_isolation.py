from __future__ import annotations

import base64
from dataclasses import replace
from pathlib import Path

from runners.research_missions import _codex_launch_configuration
from settings import Settings


def test_codex_child_environment_scrubs_operator_auth_configuration(
    settings: Settings,
    tmp_path: Path,
) -> None:
    configured = replace(
        settings,
        operator_auth_enabled=True,
        operator_username="operator-secret-name",
        operator_password="operator-secret-password",
        operator_totp_secret="JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",
        auth_cookie_key=base64.b64encode(b"c" * 32).decode("ascii"),
        api_token="operator-machine-token-" + "x" * 32,
        auth_public_origin="https://quazonai.example.test",
    )

    config, provider_id = _codex_launch_configuration(configured, tmp_path)

    assert provider_id is None
    assert config.env is not None
    scrubbed_names = (
        "QUAZONAI_AUTH_ENABLED",
        "QUAZONAI_AUTH_USERNAME",
        "QUAZONAI_AUTH_PASSWORD",
        "QUAZONAI_AUTH_TOTP_SECRET",
        "QUAZONAI_AUTH_COOKIE_KEY",
        "QUAZONAI_API_TOKEN",
        "QUAZONAI_AUTH_PUBLIC_ORIGIN",
        "QUAZONAI_AUTH_SESSION_TTL_SECONDS",
        "QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS",
    )
    for name in scrubbed_names:
        assert config.env[name] == ""

    serialized = repr(config.env) + repr(config.config_overrides)
    assert configured.operator_username not in serialized
    assert configured.operator_password not in serialized
    assert configured.operator_totp_secret not in serialized
    assert configured.auth_cookie_key not in serialized
    assert configured.api_token not in serialized
    assert configured.auth_public_origin not in serialized
