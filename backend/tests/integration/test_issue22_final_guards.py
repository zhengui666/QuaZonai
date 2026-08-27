from __future__ import annotations

from types import SimpleNamespace
from uuid import uuid4

from fastapi.testclient import TestClient
from sqlalchemy import Engine

from api import research_runtime
from main import create_app
from settings import Settings


def test_candidate_simulation_endpoint_replays_idempotently(
    engine: Engine,
    settings: Settings,
    monkeypatch,
) -> None:
    calls: list[object] = []
    candidate_id = uuid4()
    approval_id = uuid4()
    selected_alpha_id = uuid4()
    portfolio_program_id = uuid4()

    def fake_simulation(
        _factory,
        *,
        portfolio_program_id,
        alpha_ids,
        simulation_experiment_id=None,
    ):
        calls.append((portfolio_program_id, tuple(alpha_ids), simulation_experiment_id))
        return SimpleNamespace(
            candidate_id=candidate_id,
            approval_id=approval_id,
            simulation_experiment_id=simulation_experiment_id,
            selected_alpha_id=selected_alpha_id,
        )

    monkeypatch.setattr(research_runtime, "simulate_portfolio_candidate", fake_simulation)
    client = TestClient(create_app(settings=settings, engine=engine))
    path = f"/api/v1/portfolio-programs/{portfolio_program_id}/simulate-candidate"
    headers = {"Idempotency-Key": "issue22-candidate-simulation"}
    payload = {"alpha_ids": [str(selected_alpha_id)]}

    first = client.post(path, headers=headers, json=payload)
    assert first.status_code == 200, first.text
    replay = client.post(path, headers=headers, json=payload)
    assert replay.status_code == 200, replay.text
    assert replay.json() == first.json()
    assert len(calls) == 1
    assert calls[0][2] is not None

    collision = client.post(
        path,
        headers=headers,
        json={"alpha_ids": [str(uuid4())]},
    )
    assert collision.status_code == 409, collision.text
    assert collision.json()["error"]["code"] == "IDEMPOTENCY_KEY_REUSED"
    assert len(calls) == 1
