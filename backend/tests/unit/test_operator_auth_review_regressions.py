from __future__ import annotations

import base64
import ipaddress
from dataclasses import replace
from pathlib import Path

import pytest
from fastapi import Request

from errors import QfError
from operator_auth import login_source_key, require_same_origin
from settings import Settings, SettingsError


REPO_ROOT = Path(__file__).resolve().parents[3]


def _enabled_settings(settings: Settings, **overrides: object) -> Settings:
    values: dict[str, object] = {
        "operator_auth_enabled": True,
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


def _request_from_peer(
    peer: str,
    headers: list[tuple[bytes, bytes]],
) -> Request:
    return Request(
        {
            "type": "http",
            "http_version": "1.1",
            "method": "POST",
            "scheme": "https",
            "path": "/api/v1/auth/login",
            "raw_path": b"/api/v1/auth/login",
            "query_string": b"",
            "headers": headers,
            "client": (peer, 43210),
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


@pytest.mark.parametrize("origin", ["https://[::1", "https://[abc]"])
def test_malformed_bracketed_origin_returns_auth_rejection(
    settings: Settings,
    origin: str,
) -> None:
    configured = _enabled_settings(settings)

    with pytest.raises(QfError, match="origin is not allowed") as raised:
        require_same_origin(_mutation_request(origin), configured)

    assert raised.value.code == "AUTH_ORIGIN_REJECTED"


def test_login_source_uses_rightmost_untrusted_address_from_trusted_proxy_chain(
    settings: Settings,
) -> None:
    configured = _enabled_settings(
        settings,
        auth_trusted_proxy_cidrs=(ipaddress.ip_network("10.20.0.0/16"),),
    )
    configured.validate_operator_auth()
    request = _request_from_peer(
        "10.20.0.5",
        [(b"x-forwarded-for", b"198.51.100.11, 10.20.0.4")],
    )

    assert login_source_key(request, configured) == "198.51.100.11"


def test_login_source_ignores_forwarded_header_from_untrusted_peer(
    settings: Settings,
) -> None:
    configured = _enabled_settings(
        settings,
        auth_trusted_proxy_cidrs=(ipaddress.ip_network("10.20.0.0/16"),),
    )
    request = _request_from_peer(
        "203.0.113.45",
        [(b"x-forwarded-for", b"198.51.100.11")],
    )

    assert login_source_key(request, configured) == "203.0.113.45"


@pytest.mark.parametrize(
    "headers",
    [
        [(b"x-forwarded-for", b"not-an-ip")],
        [(b"x-forwarded-for", b"10.20.0.4")],
        [
            (b"x-forwarded-for", b"198.51.100.11"),
            (b"x-forwarded-for", b"203.0.113.12"),
        ],
    ],
)
def test_trusted_proxy_falls_back_to_direct_peer_for_ambiguous_forwarded_headers(
    settings: Settings,
    headers: list[tuple[bytes, bytes]],
) -> None:
    configured = _enabled_settings(
        settings,
        auth_trusted_proxy_cidrs=(ipaddress.ip_network("10.20.0.0/16"),),
    )
    request = _request_from_peer("10.20.0.5", headers)

    assert login_source_key(request, configured) == "10.20.0.5"


def test_compose_disables_uvicorn_proxy_header_rewriting() -> None:
    compose = (REPO_ROOT / "compose.yml").read_text(encoding="utf-8")
    api_service = compose.split("\n  finite-worker:", maxsplit=1)[0]

    assert '"--no-proxy-headers"' in api_service
    for workflow in (
        REPO_ROOT / ".github/workflows/frontend.yml",
        REPO_ROOT / ".github/workflows/operator-auth-e2e.yml",
    ):
        assert "--no-proxy-headers" in workflow.read_text(encoding="utf-8")
