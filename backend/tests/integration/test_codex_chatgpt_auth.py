from __future__ import annotations

import json

from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from settings import Settings


def test_codex_auth_status_is_database_owned_and_not_cacheable(engine: Engine, settings: Settings) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    response = client.get("/api/v1/system/codex-auth")

    assert response.status_code == 200
    assert response.headers["cache-control"] == "no-store"
    assert response.headers["pragma"] == "no-cache"
    assert response.json() == {
        "state": "DISCONNECTED",
        "active": False,
        "email": None,
        "plan_type": None,
        "authenticated_at": None,
        "last_refresh_at": None,
        "reauth_required_at": None,
        "pending_login": None,
        "legacy_auth_file_present": False,
    }


def test_legacy_auth_file_does_not_configure_runtime_without_valid_db_import(
    engine: Engine,
    settings: Settings,
) -> None:
    settings.codex_home.mkdir(parents=True, exist_ok=True)
    legacy = settings.codex_home / "auth.json"
    legacy.write_text(json.dumps({"auth_mode": "api_key", "OPENAI_API_KEY": "not-a-chatgpt-token"}), encoding="utf-8")
    client = TestClient(create_app(settings=settings, engine=engine))

    status = client.get("/api/v1/system/codex-auth")
    runtime = client.get("/api/v1/system/runtime-configuration")

    assert status.json()["state"] == "DISCONNECTED"
    assert runtime.json()["codex_login_configured"] is False
    assert legacy.is_file()
