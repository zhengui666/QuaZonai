from __future__ import annotations

from pathlib import Path

from fastapi.testclient import TestClient
from quazonai_nautilus_gateway.app import create_app


def test_gateway_requires_service_token(monkeypatch, tmp_path: Path) -> None:
    monkeypatch.setenv("NAUTILUS_GATEWAY_TOKEN", "research-service-token")
    monkeypatch.delenv("NAUTILUS_GATEWAY_ALLOW_ANONYMOUS", raising=False)
    client = TestClient(create_app(data_root=tmp_path))

    assert client.get("/v1/capabilities").status_code == 401
    response = client.get(
        "/v1/capabilities",
        headers={"Authorization": "Bearer research-service-token"},
    )
    assert response.status_code == 200
    body = response.json()
    assert body["runtime_name"] == "NAUTILUS_TRADER"
    assert body["runtime_version"] == "1.231.0"
    assert body["live_execution_exposed"] is False
    assert "BACKTEST" in body["supported_operations"]
