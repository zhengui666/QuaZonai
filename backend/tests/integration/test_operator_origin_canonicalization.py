from __future__ import annotations

import base64
import secrets
from dataclasses import replace

import pyotp
import pytest
from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from settings import Settings


def _enabled_settings(settings: Settings, *, origin: str) -> Settings:
    return replace(
        settings,
        operator_auth_enabled=True,
        operator_totp_secret=pyotp.random_base32(),
        auth_cookie_key=base64.b64encode(b"a" * 32).decode("ascii"),
        api_token=secrets.token_urlsafe(32),
        auth_public_origin=origin,
    )


def _login(client: TestClient, settings: Settings, *, origin: str):
    assert settings.operator_totp_secret is not None
    return client.post(
        "/api/v1/auth/login",
        headers={"Origin": origin},
        json={
            "totp_code": pyotp.TOTP(settings.operator_totp_secret).now(),
            "trust_browser": False,
        },
    )


@pytest.mark.parametrize(
    ("configured_origin", "browser_origin"),
    [
        ("https://EXAMPLE.com:443", "https://example.com"),
        ("https://bücher.example:443", "https://xn--bcher-kva.example"),
        ("https://faß.de", "https://xn--fa-hia.de"),
        (
            "https://[2001:0DB8:0:0:0:0:0:1]:443",
            "https://[2001:db8::1]",
        ),
        ("https://EXAMPLE.com:8443", "https://example.com:8443"),
    ],
)
def test_login_accepts_equivalent_browser_origin_serializations(
    settings: Settings,
    engine: Engine,
    configured_origin: str,
    browser_origin: str,
) -> None:
    secured = _enabled_settings(settings, origin=configured_origin)
    # The TestClient transport itself cannot parse bracketed IPv6 netlocs in its
    # synthetic base URL. Origin validation is driven by the explicit Origin
    # header, so keep the transport host neutral while exercising the real app.
    client = TestClient(
        create_app(settings=secured, engine=engine),
        base_url="https://testserver",
    )

    response = _login(client, secured, origin=browser_origin)

    assert response.status_code == 200


def test_cookie_authenticated_mutation_uses_canonical_origin_comparison(
    settings: Settings,
    engine: Engine,
) -> None:
    secured = _enabled_settings(settings, origin="https://EXAMPLE.com:443")
    client = TestClient(
        create_app(settings=secured, engine=engine),
        base_url="https://example.com",
    )
    assert _login(client, secured, origin="https://example.com").status_code == 200

    accepted = client.post(
        "/api/v1/ideas/preview",
        headers={"Origin": "https://example.com:443"},
        json={"idea": "Test a liquid US equity factor after realistic costs."},
    )
    rejected = client.post(
        "/api/v1/ideas/preview",
        headers={"Origin": "https://example.com:8443"},
        json={"idea": "Test a liquid US equity factor after realistic costs."},
    )

    assert accepted.status_code == 200
    assert rejected.status_code == 403
    assert rejected.json()["error"]["code"] == "AUTH_ORIGIN_REJECTED"


@pytest.mark.parametrize(
    "origin",
    [
        "null",
        "https://example.com/path",
        "https://example.com:70000",
        "https://attacker.example",
        "http://example.com",
    ],
)
def test_cookie_authenticated_mutation_rejects_invalid_or_distinct_origins(
    settings: Settings,
    engine: Engine,
    origin: str,
) -> None:
    secured = _enabled_settings(settings, origin="https://example.com")
    client = TestClient(
        create_app(settings=secured, engine=engine),
        base_url="https://example.com",
    )
    assert _login(client, secured, origin="https://example.com").status_code == 200

    response = client.post(
        "/api/v1/ideas/preview",
        headers={"Origin": origin},
        json={"idea": "Test a liquid US equity factor after realistic costs."},
    )

    assert response.status_code == 403
    assert response.json()["error"]["code"] == "AUTH_ORIGIN_REJECTED"
