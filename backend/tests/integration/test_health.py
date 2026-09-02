from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import Engine

from api import system as system_api
from main import create_app
from settings import Settings


def test_health_reports_ready_with_database_and_master_key(
    engine: Engine,
    settings: Settings,
) -> None:
    app = create_app(settings=settings, engine=engine)
    response = TestClient(app).get("/api/v1/system/health")
    assert response.status_code == 200
    payload = response.json()
    assert payload["live"] is True
    assert payload["ready"] is True
    assert payload["database"] == "ready"
    assert payload["master_key"] == "configured"


def test_health_reports_custom_provider_reauth_as_degraded(
    engine: Engine,
    settings: Settings,
    monkeypatch,
) -> None:
    monkeypatch.setattr(
        system_api,
        "codex_auth_readiness",
        lambda session, configured_settings: (False, "CUSTOM_PROVIDER_REAUTH_REQUIRED"),
    )
    response = TestClient(create_app(settings=settings, engine=engine)).get("/api/v1/system/health")
    payload = response.json()

    assert response.status_code == 200
    assert payload["ready"] is False
    assert payload["codex"] == "degraded"


def test_openapi_is_available_only_at_explicit_path(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    assert client.get("/docs").status_code == 404
    response = client.get("/api/v1/openapi.json")
    assert response.status_code == 200
    assert response.json()["info"]["title"] == "QuaZonai API"


def test_served_workbench_denies_framing_without_affecting_api(
    engine: Engine,
    settings: Settings,
) -> None:
    settings.frontend_dist.mkdir()
    (settings.frontend_dist / "index.html").write_text("<main>QuaZonai</main>")
    client = TestClient(create_app(settings=settings, engine=engine))

    for path in ("/", "/administration"):
        response = client.get(path)
        assert response.status_code == 200
        assert response.headers["content-security-policy"] == "frame-ancestors 'none'"
        assert response.headers["x-frame-options"] == "DENY"

    openapi = client.get("/api/v1/openapi.json")
    assert openapi.status_code == 200
    assert openapi.json()["info"]["title"] == "QuaZonai API"
