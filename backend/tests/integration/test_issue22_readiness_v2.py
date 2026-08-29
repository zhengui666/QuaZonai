from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from settings import Settings


def test_paper_readiness_requires_candidate_bundle_v2(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    legacy = client.post(
        "/api/v1/downstream-systems",
        headers={"Idempotency-Key": "paper-v1"},
        json={
            "name": "Legacy Paper",
            "environment_type": "PAPER",
            "package_contract_version": "1",
            "feedback_contract_version": "1",
            "enabled": True,
        },
    )
    assert legacy.status_code == 201, legacy.text
    assert client.get("/api/v1/readiness").json()["PAPER_HANDOFF_READY"] is False

    current = client.post(
        "/api/v1/downstream-systems",
        headers={"Idempotency-Key": "paper-v2"},
        json={
            "name": "Bundle v2 Paper",
            "environment_type": "PAPER",
            "package_contract_version": "2",
            "feedback_contract_version": "1",
            "enabled": True,
        },
    )
    assert current.status_code == 201, current.text
    assert client.get("/api/v1/readiness").json()["PAPER_HANDOFF_READY"] is True
