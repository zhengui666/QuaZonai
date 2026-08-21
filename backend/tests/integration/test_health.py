from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import Engine

from quantfoundry.main import create_app
from quantfoundry.settings import Settings


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


def test_openapi_is_available_only_at_explicit_path(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    assert client.get("/docs").status_code == 404
    response = client.get("/api/v1/openapi.json")
    assert response.status_code == 200
    assert response.json()["info"]["title"] == "QuantFoundry API"
