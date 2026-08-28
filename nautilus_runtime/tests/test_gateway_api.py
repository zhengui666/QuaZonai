from __future__ import annotations

from pathlib import Path
from uuid import UUID, uuid4

from fastapi.testclient import TestClient
from pydantic import ValidationError
import pytest

from quazonai_nautilus_gateway.app import create_app
from quazonai_nautilus_gateway.engine import (
    GatewayContractError,
    NautilusGatewayEngine,
    _candidate_strategy_wheel_path,
)
from quazonai_nautilus_gateway.models import (
    BacktestExperimentRequest,
    StrategyArtifact,
)


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


def _source_artifact(source: str) -> StrategyArtifact:
    return StrategyArtifact(
        artifact_id="capability-test",
        kind="SOURCE_BUNDLE",
        strategy_path="strategy:CandidateStrategy",
        config_path="strategy:CandidateConfig",
        source_files={"strategy.py": source},
        requirements=["nautilus_trader==1.231.0"],
    )


def test_source_bundle_allows_normal_strategy_initialization() -> None:
    artifact = _source_artifact(
        "from nautilus_trader.trading.strategy import Strategy\n"
        "class CandidateConfig: pass\n"
        "class CandidateStrategy(Strategy):\n"
        "    def __init__(self, config):\n"
        "        super().__init__(config)\n"
    )
    assert artifact.kind == "SOURCE_BUNDLE"


@pytest.mark.parametrize(
    "source",
    [
        "import os\nclass CandidateConfig: pass\nclass CandidateStrategy: pass\n",
        "class CandidateConfig: pass\nclass CandidateStrategy:\n    def x(self): open('/tmp/x')\n",
        (
            "class CandidateConfig: pass\nclass CandidateStrategy:\n"
            "    def x(self): return self.__class__.__mro__\n"
        ),
    ],
)
def test_source_bundle_rejects_filesystem_process_and_reflection_capabilities(source: str) -> None:
    with pytest.raises(ValidationError):
        _source_artifact(source)



def test_gateway_rejects_duplicate_instrument_ids() -> None:
    strategy = StrategyArtifact(
        artifact_id="duplicate-gateway-instruments",
        kind="IMPORTABLE",
        strategy_path="module:Strategy",
        config_path="module:Config",
    )
    with pytest.raises(ValidationError):
        BacktestExperimentRequest(
            dataset_revision_id=uuid4(),
            catalog_key="duplicate-gateway-instruments",
            instrument_ids=["EUR/USD.SIM", "EUR/USD.SIM"],
            strategy=strategy,
        )


def test_gateway_backtest_idempotency_replays_terminal_result(
    monkeypatch, tmp_path: Path
) -> None:
    monkeypatch.setenv("NAUTILUS_GATEWAY_ALLOW_ANONYMOUS", "true")
    monkeypatch.delenv("NAUTILUS_GATEWAY_TOKEN", raising=False)
    calls: list[object] = []

    def fake_backtest(self, request, *, _source_isolated=False):
        del self, _source_isolated
        calls.append(request.experiment_id)
        return {
            "experiment_id": str(request.experiment_id),
            "mode": request.mode.value,
            "terminal": "SUCCEEDED",
        }

    monkeypatch.setattr(NautilusGatewayEngine, "run_backtest", fake_backtest)
    experiment_id = uuid4()
    request = BacktestExperimentRequest(
        experiment_id=experiment_id,
        dataset_revision_id=uuid4(),
        catalog_key="idempotent-backtest",
        instrument_ids=["EUR/USD.SIM"],
        strategy=StrategyArtifact(
            artifact_id="idempotent-backtest",
            kind="IMPORTABLE",
            strategy_path="module:Strategy",
            config_path="module:Config",
        ),
    )
    client = TestClient(create_app(data_root=tmp_path))
    missing = client.post("/v1/backtests", json=request.model_dump(mode="json"))
    assert missing.status_code == 422
    headers = {"Idempotency-Key": str(experiment_id)}
    first = client.post(
        "/v1/backtests", headers=headers, json=request.model_dump(mode="json")
    )
    replay = client.post(
        "/v1/backtests", headers=headers, json=request.model_dump(mode="json")
    )
    assert first.status_code == 200, first.text
    assert replay.json() == first.json()
    assert calls == [experiment_id]
    collision_request = request.model_copy(update={"tags": {"changed": "true"}})
    collision = client.post(
        "/v1/backtests",
        headers=headers,
        json=collision_request.model_dump(mode="json"),
    )
    assert collision.status_code == 422
    assert calls == [experiment_id]



def test_gateway_backtest_idempotency_replays_terminal_contract_failure(
    monkeypatch, tmp_path: Path
) -> None:
    monkeypatch.setenv("NAUTILUS_GATEWAY_ALLOW_ANONYMOUS", "true")
    monkeypatch.delenv("NAUTILUS_GATEWAY_TOKEN", raising=False)
    calls: list[object] = []

    def fail_backtest(self, request, *, _source_isolated=False):
        del self, _source_isolated
        calls.append(request.experiment_id)
        raise GatewayContractError("deterministic contract failure")

    monkeypatch.setattr(NautilusGatewayEngine, "run_backtest", fail_backtest)
    experiment_id = uuid4()
    request = BacktestExperimentRequest(
        experiment_id=experiment_id,
        dataset_revision_id=uuid4(),
        catalog_key="failed-idempotent-backtest",
        instrument_ids=["EUR/USD.SIM"],
        strategy=StrategyArtifact(
            artifact_id="failed-idempotent-backtest",
            kind="IMPORTABLE",
            strategy_path="module:Strategy",
            config_path="module:Config",
        ),
    )
    client = TestClient(create_app(data_root=tmp_path))
    headers = {"Idempotency-Key": str(experiment_id)}
    first = client.post(
        "/v1/backtests", headers=headers, json=request.model_dump(mode="json")
    )
    replay = client.post(
        "/v1/backtests", headers=headers, json=request.model_dump(mode="json")
    )
    assert first.status_code == 422
    assert replay.status_code == 422
    assert calls == [experiment_id]

def test_sealed_gateway_hides_catalog_validation(monkeypatch, tmp_path: Path) -> None:
    monkeypatch.setenv("NAUTILUS_GATEWAY_ALLOW_ANONYMOUS", "true")
    monkeypatch.delenv("NAUTILUS_GATEWAY_TOKEN", raising=False)
    client = TestClient(create_app(data_root=tmp_path, role="SEALED"))
    response = client.post(
        "/v1/catalogs/validate",
        json={
            "request_id": str(uuid4()),
            "catalog_key": "sealed-private-catalog",
            "instrument_ids": [],
        },
    )
    assert response.status_code == 404
    assert response.json()["detail"] == "operation unavailable on this gateway role"


def test_candidate_wheel_identity_uses_full_uuid_integer() -> None:
    candidate_id = UUID(int=1_000_001)
    assert _candidate_strategy_wheel_path(candidate_id) == (
        "strategy/quazonai_candidate_strategy-0.0.1000001-py3-none-any.whl"
    )


def test_source_bundle_rejects_module_object_escape() -> None:
    with pytest.raises(ValueError, match="attribute 'sys'"):
        StrategyArtifact(
            artifact_id="escape",
            kind="SOURCE_BUNDLE",
            strategy_path="evil:S",
            config_path="evil:C",
            source_files={"evil.py": "import dataclasses\ndataclasses.sys.modules['os'].system('id')\n"},
        )
