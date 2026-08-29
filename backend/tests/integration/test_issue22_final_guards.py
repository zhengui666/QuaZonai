from __future__ import annotations

from types import SimpleNamespace
from uuid import uuid4

from fastapi.testclient import TestClient
import pytest
from sqlalchemy import Engine

from api import research_runtime
from errors import QfError
from main import create_app
from quant_runtime.promotion import _mandate_constraints
from settings import Settings


def test_candidate_simulation_endpoint_replays_idempotently(
    engine: Engine,
    settings: Settings,
    monkeypatch,
) -> None:
    calls: list[object] = []
    selected_alpha_id = uuid4()
    portfolio_program_id = uuid4()

    def fake_simulation(
        _factory,
        *,
        portfolio_program_id,
        alpha_ids,
        simulation_experiment_id=None,
        portfolio_sealed_experiment_id=None,
    ):
        calls.append((portfolio_program_id, tuple(alpha_ids), simulation_experiment_id, portfolio_sealed_experiment_id))
        return SimpleNamespace(
            simulation_experiment_id=simulation_experiment_id,
            selected_alpha_id=selected_alpha_id,
        )

    monkeypatch.setattr(research_runtime, "prepare_portfolio_simulation", fake_simulation)
    client = TestClient(create_app(settings=settings, engine=engine))
    path = f"/api/v1/portfolio-programs/{portfolio_program_id}/simulate-candidate"
    headers = {"Idempotency-Key": "issue22-candidate-simulation"}
    payload = {"alpha_ids": [str(selected_alpha_id)]}

    first = client.post(path, headers=headers, json=payload)
    assert first.status_code == 202, first.text
    replay = client.post(path, headers=headers, json=payload)
    assert replay.status_code == 202, replay.text
    assert replay.json() == first.json()
    assert first.json()["state"] == "READY"
    assert first.json()["job_id"]
    assert len(calls) == 1
    assert calls[0][2] is not None
    assert calls[0][3] is None

    collision = client.post(
        path,
        headers=headers,
        json={"alpha_ids": [str(uuid4())]},
    )
    assert collision.status_code == 409, collision.text
    assert collision.json()["error"]["code"] == "IDEMPOTENCY_KEY_REUSED"
    assert len(calls) == 1


def test_capacity_constraint_is_rejected_before_remote_simulation() -> None:
    mandate = SimpleNamespace(spec_json={"constraints": {"min_capacity_ratio": 0.5}})
    with pytest.raises(QfError) as raised:
        _mandate_constraints(mandate)
    assert raised.value.code == "PORTFOLIO_MANDATE_CONSTRAINT_UNSUPPORTED"


def test_top_level_capacity_constraint_is_also_rejected_before_remote_simulation() -> None:
    mandate = SimpleNamespace(spec_json={"min_capacity_ratio": 0.5})
    with pytest.raises(QfError) as raised:
        _mandate_constraints(mandate)
    assert raised.value.code == "PORTFOLIO_MANDATE_CONSTRAINT_UNSUPPORTED"


def test_duplicate_instrument_ids_are_rejected_by_core_contract() -> None:
    from pydantic import ValidationError

    from quant_runtime.contracts import BacktestExperimentRequest, StrategyArtifact

    strategy = StrategyArtifact(
        artifact_id="duplicate-instrument-test",
        kind="IMPORTABLE",
        strategy_path="module:Strategy",
        config_path="module:Config",
    )
    with pytest.raises(ValidationError):
        BacktestExperimentRequest(
            dataset_revision_id=uuid4(),
            catalog_key="duplicate-instruments",
            instrument_ids=["EUR/USD.SIM", "EUR/USD.SIM"],
            strategy=strategy,
        )


def test_discovery_quality_model_is_discriminating_and_search_adjusted() -> None:
    from quant_runtime.promotion import _discovery_quality_score

    weak, weak_model = _discovery_quality_score(
        {
            "fills": [{}],
            "pnl": {"USD": {"PnL (total)": 10.0}},
            "statistics": {"returns": {"Sharpe Ratio (252 days)": 0.1}},
        },
        search_attempt_count=8,
    )
    strong, strong_model = _discovery_quality_score(
        {
            "fills": [{}, {}, {}, {}],
            "pnl": {"USD": {"PnL (total)": 1000.0}},
            "statistics": {
                "returns": {
                    "Sharpe Ratio (252 days)": 2.0,
                    "Total Return": 0.20,
                }
            },
        },
        search_attempt_count=1,
    )
    assert strong > weak
    assert strong_model["sealed_evidence_used_for_scoring"] is False
    assert weak_model["search_exposure_penalty"] > 0
    assert weak_model["model"] == "DISCOVERY_PUBLIC_PERFORMANCE_V2"


def test_mandate_filter_runs_before_quality_ranking() -> None:
    from types import SimpleNamespace

    from quant_runtime.promotion import _select_portfolio_alpha

    allowed_universe_id = uuid4()
    mandate = SimpleNamespace(
        spec_json={
            "constraints": {
                "allowed_universe_version_ids": [str(allowed_universe_id)]
            }
        }
    )
    outside = SimpleNamespace(
        id=uuid4(),
        universe_version_id=uuid4(),
        universe="OUTSIDE",
        metrics={"search_adjusted_quality": 0.99},
    )
    eligible = SimpleNamespace(
        id=uuid4(),
        universe_version_id=allowed_universe_id,
        universe="ELIGIBLE",
        metrics={"search_adjusted_quality": 0.60},
    )
    assert _select_portfolio_alpha(mandate, [outside, eligible]) is eligible
