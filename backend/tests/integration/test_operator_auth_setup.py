from __future__ import annotations

import base64
from dataclasses import replace

import pyotp
import pytest
from fastapi.testclient import TestClient
from sqlalchemy import delete, select

from db.auth_models import OperatorAuthConfiguration
from main import create_app
from settings import Settings, SettingsError


def _fresh_auth_settings(settings: Settings, *, secret: str | None = None) -> Settings:
    return replace(
        settings,
        operator_auth_enabled=True,
        operator_totp_secret=secret,
        auth_cookie_key=base64.b64encode(b"c" * 32).decode("ascii"),
        api_token="machine-token-" + "x" * 32,
        auth_public_origin="http://testserver",
    )


def _origin() -> dict[str, str]:
    return {"Origin": "http://testserver"}


def test_fresh_enabled_instance_bootstraps_and_binds_first_totp_candidate(settings, engine) -> None:
    configured = _fresh_auth_settings(settings)
    app = create_app(settings=configured, engine=engine)
    client = TestClient(app)

    bootstrap = client.get("/api/v1/auth/bootstrap")
    assert bootstrap.status_code == 200
    assert bootstrap.json() == {"auth_enabled": True, "setup_required": True}
    assert bootstrap.headers["cache-control"] == "no-store"

    start = client.post("/api/v1/auth/setup/start", headers=_origin())
    assert start.status_code == 200
    candidate = start.json()
    assert candidate["issuer"] == "QuaZonai"
    assert candidate["account_name"].startswith("local-operator@")
    assert candidate["otpauth_uri"].startswith("otpauth://totp/")
    assert len(candidate["manual_key"]) >= 32
    assert start.headers["cache-control"] == "no-store"
    assert "HttpOnly" in start.headers["set-cookie"]
    assert "SameSite=strict" in start.headers["set-cookie"]

    confirm = client.post(
        "/api/v1/auth/setup/confirm",
        headers=_origin(),
        json={
            "totp_code": pyotp.TOTP(candidate["manual_key"]).now(),
            "trust_browser": True,
        },
    )
    assert confirm.status_code == 200
    assert confirm.json()["authenticated"] is True
    assert confirm.json()["trusted_browser"] is True
    assert "quazonai_totp_setup" in confirm.headers["set-cookie"]
    assert "Max-Age=0" in confirm.headers["set-cookie"]

    with app.state.session_factory() as session:
        binding = session.scalar(select(OperatorAuthConfiguration))
        assert binding is not None
        assert candidate["manual_key"].encode("ascii") not in binding.totp_secret_ciphertext

    assert client.get("/api/v1/auth/bootstrap").json() == {
        "auth_enabled": True,
        "setup_required": False,
    }


def test_setup_is_first_claim_wins_and_loser_is_rebootstrapped(settings, engine) -> None:
    configured = _fresh_auth_settings(settings)
    app = create_app(settings=configured, engine=engine)
    first = TestClient(app)
    second = TestClient(app)

    first_candidate = first.post("/api/v1/auth/setup/start", headers=_origin()).json()
    second_candidate = second.post("/api/v1/auth/setup/start", headers=_origin()).json()

    winner = first.post(
        "/api/v1/auth/setup/confirm",
        headers=_origin(),
        json={"totp_code": pyotp.TOTP(first_candidate["manual_key"]).now(), "trust_browser": False},
    )
    assert winner.status_code == 200

    loser = second.post(
        "/api/v1/auth/setup/confirm",
        headers=_origin(),
        json={"totp_code": pyotp.TOTP(second_candidate["manual_key"]).now(), "trust_browser": False},
    )
    assert loser.status_code == 409
    assert loser.json()["error"]["code"] == "AUTH_SETUP_ALREADY_COMPLETED"
    assert loser.headers["cache-control"] == "no-store"
    assert "Max-Age=0" in loser.headers["set-cookie"]


def test_setup_confirmation_consumes_code_for_canonical_login(settings, engine) -> None:
    configured = _fresh_auth_settings(settings)
    client = TestClient(create_app(settings=configured, engine=engine))

    candidate = client.post("/api/v1/auth/setup/start", headers=_origin()).json()
    code = pyotp.TOTP(candidate["manual_key"]).now()
    confirm = client.post(
        "/api/v1/auth/setup/confirm",
        headers=_origin(),
        json={"totp_code": code, "trust_browser": False},
    )
    assert confirm.status_code == 200

    replay = client.post(
        "/api/v1/auth/login",
        headers=_origin(),
        json={"totp_code": code},
    )
    assert replay.status_code == 401
    assert replay.json()["error"]["code"] == "AUTH_INVALID"


def test_canonical_binding_survives_restart_and_legacy_mismatch_fails_closed(settings, engine) -> None:
    secret = pyotp.random_base32()
    configured = _fresh_auth_settings(settings, secret=secret)
    first_app = create_app(settings=configured, engine=engine)
    first_client = TestClient(first_app)
    first_client.get("/api/v1/auth/bootstrap")

    restarted_app = create_app(
        settings=_fresh_auth_settings(settings),
        engine=engine,
    )
    assert TestClient(restarted_app).get("/api/v1/auth/bootstrap").json() == {
        "auth_enabled": True,
        "setup_required": False,
    }

    with pytest.raises(SettingsError, match="conflicts"):
        create_app(settings=_fresh_auth_settings(settings, secret=pyotp.random_base32()), engine=engine)


def test_missing_binding_after_initialization_fails_closed(settings, engine) -> None:
    configured = _fresh_auth_settings(settings)
    app = create_app(settings=configured, engine=engine)
    client = TestClient(app)
    candidate = client.post("/api/v1/auth/setup/start", headers=_origin()).json()
    assert client.post(
        "/api/v1/auth/setup/confirm",
        headers=_origin(),
        json={"totp_code": pyotp.TOTP(candidate["manual_key"]).now()},
    ).status_code == 200

    with app.state.session_factory.begin() as session:
        session.execute(delete(OperatorAuthConfiguration))

    with pytest.raises(SettingsError, match="missing after initialization"):
        create_app(settings=configured, engine=engine)


def test_no_binding_without_legacy_secret_does_not_allow_normal_login(settings, engine) -> None:
    configured = _fresh_auth_settings(settings)
    client = TestClient(create_app(settings=configured, engine=engine))

    response = client.post(
        "/api/v1/auth/login",
        headers=_origin(),
        json={"totp_code": "123456"},
    )
    assert response.status_code == 409
    assert response.json()["error"]["code"] == "AUTH_SETUP_REQUIRED"
    assert response.headers["cache-control"] == "no-store"
