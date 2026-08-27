from __future__ import annotations

import json
from uuid import uuid4

import httpx
import pytest
from pydantic import ValidationError

from quant_runtime import (
    BacktestEvidence,
    CatalogReference,
    ExperimentContract,
    RemoteNautilusQuantRuntime,
    StrategyArtifact,
)


def _contract() -> ExperimentContract:
    return ExperimentContract(
        catalog=CatalogReference(
            dataset_revision_id=uuid4(),
            catalog_uri="catalog://discovery-eurusd",
            nautilus_data_type="QuoteTick",
            instrument_ids=["EUR/USD.SIM"],
            partition="DISCOVERY",
        ),
        strategy=StrategyArtifact(
            strategy_path="nautilus_trader.examples.strategies.ema_cross:EMACross",
            config_path="nautilus_trader.examples.strategies.ema_cross:EMACrossConfig",
            config={"fast_ema_period": 3, "slow_ema_period": 8, "trade_size": "10000"},
        ),
    )


def test_remote_runtime_sends_bearer_auth_and_validates_evidence() -> None:
    token = "runtime-service-token"

    def handler(request: httpx.Request) -> httpx.Response:
        assert request.headers["authorization"] == f"Bearer {token}"
        assert request.headers["idempotency-key"]
        body = json.loads(request.content)
        return httpx.Response(
            200,
            json={
                "run_id": body["run_id"],
                "run_config_id": body["run_id"],
                "runtime_name": "NAUTILUS_TRADER",
                "runtime_version": "1.231.0",
                "catalog_uri": body["catalog"]["catalog_uri"],
                "partition": "DISCOVERY",
                "elapsed_time_seconds": 0.1,
                "total_events": 10,
                "total_orders": 2,
                "total_positions": 1,
                "statistics": {"returns": {"Sharpe Ratio (252 days)": 1.0}},
                "reports": {"orders": [{"client_order_id": "O-1"}]},
                "disclosure": {},
            },
        )

    runtime = RemoteNautilusQuantRuntime(
        base_url="https://runtime.example.test",
        token=token,
        transport=httpx.MockTransport(handler),
    )
    try:
        evidence = runtime.run_backtest(_contract())
    finally:
        runtime.close()
    assert isinstance(evidence, BacktestEvidence)
    assert evidence.total_orders == 2
    assert token not in repr(runtime)


def test_remote_runtime_rejects_urls_with_embedded_credentials() -> None:
    with pytest.raises(ValueError, match="credentials"):
        RemoteNautilusQuantRuntime(
            base_url="https://user:password@runtime.example.test",
            token="token",
        )


def test_experiment_contract_forbids_broker_fields() -> None:
    payload = _contract().model_dump(mode="json")
    payload["broker_credentials"] = {"api_key": "forbidden"}
    with pytest.raises(ValidationError):
        ExperimentContract.model_validate(payload)
