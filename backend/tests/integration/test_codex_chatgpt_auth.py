from __future__ import annotations

import json
from datetime import UTC, datetime, timedelta
from uuid import uuid4

from fastapi.testclient import TestClient
from sqlalchemy import Engine, select

from api import codex_auth as codex_auth_api
from codex_chatgpt_auth import DeviceLoginPollResult, DeviceLoginView
from errors import QfError
from db.models import Event
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


def test_device_start_requires_json_and_deduplicates_reused_attempt_event(
    engine: Engine,
    settings: Settings,
    monkeypatch,
) -> None:
    login_id = uuid4()
    expires_at = datetime.now(UTC) + timedelta(minutes=10)
    views = iter(
        [
            DeviceLoginView(
                login_id=login_id,
                status="PENDING",
                verification_url="https://auth.openai.com/codex/device",
                user_code="ABCD-EFGH",
                expires_at=expires_at,
                poll_after_seconds=5,
                created=True,
            ),
            DeviceLoginView(
                login_id=login_id,
                status="PENDING",
                verification_url="https://auth.openai.com/codex/device",
                user_code="ABCD-EFGH",
                expires_at=expires_at,
                poll_after_seconds=5,
                created=False,
            ),
        ]
    )
    calls = 0

    def fake_start(session, configured_settings):
        nonlocal calls
        calls += 1
        return next(views)

    monkeypatch.setattr(codex_auth_api, "start_device_login", fake_start)
    client = TestClient(create_app(settings=settings, engine=engine))
    path = "/api/v1/system/codex-auth/chatgpt/device/start"

    rejected = client.post(path)
    assert rejected.status_code == 415
    assert rejected.json()["error"]["code"] == "CODEX_CHATGPT_JSON_REQUIRED"
    assert calls == 0

    first = client.post(path, json={})
    second = client.post(path, json={})
    assert first.status_code == 200
    assert second.status_code == 200
    assert calls == 2

    with engine.connect() as connection:
        events = connection.execute(
            select(Event).where(Event.kind == "CODEX_CHATGPT_AUTH_LOGIN_STARTED")
        ).all()
    assert len(events) == 1


def test_terminal_success_poll_does_not_repeat_connected_event(engine: Engine, settings: Settings, monkeypatch) -> None:
    login_id = uuid4()
    result = DeviceLoginPollResult(status="SUCCEEDED", transitioned=False)
    monkeypatch.setattr(codex_auth_api, "poll_device_login", lambda session, configured_settings, configured_login_id: result)
    client = TestClient(create_app(settings=settings, engine=engine))

    response = client.post(f"/api/v1/system/codex-auth/chatgpt/device/{login_id}/poll")

    assert response.status_code == 200
    assert response.json()["status"] == "SUCCEEDED"
    with engine.connect() as connection:
        events = connection.execute(
            select(Event).where(Event.kind == "CODEX_CHATGPT_AUTH_CONNECTED")
        ).all()
    assert events == []


def test_disconnect_keeps_database_state_when_legacy_cleanup_fails(
    engine: Engine,
    settings: Settings,
    monkeypatch,
) -> None:
    settings.codex_home.mkdir(parents=True, exist_ok=True)
    legacy = settings.codex_home / "auth.json"
    legacy.write_text("legacy", encoding="utf-8")
    disconnect_calls = 0

    def fail_cleanup(configured_settings):
        raise QfError("CODEX_LEGACY_AUTH_CLEANUP_FAILED", "cleanup failed", 503)

    def record_disconnect(session):
        nonlocal disconnect_calls
        disconnect_calls += 1

    monkeypatch.setattr(codex_auth_api, "remove_legacy_auth_file", fail_cleanup)
    monkeypatch.setattr(codex_auth_api, "disconnect_chatgpt", record_disconnect)
    client = TestClient(create_app(settings=settings, engine=engine))

    response = client.delete("/api/v1/system/codex-auth/chatgpt")

    assert response.status_code == 503
    assert response.json()["error"]["code"] == "CODEX_LEGACY_AUTH_CLEANUP_FAILED"
    assert disconnect_calls == 0
    assert legacy.is_file()
