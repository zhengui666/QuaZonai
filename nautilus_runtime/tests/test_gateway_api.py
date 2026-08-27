from __future__ import annotations

from pathlib import Path

from fastapi.testclient import TestClient
from pydantic import ValidationError
import pytest

from quazonai_nautilus_gateway.app import create_app
from quazonai_nautilus_gateway.models import StrategyArtifact


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
