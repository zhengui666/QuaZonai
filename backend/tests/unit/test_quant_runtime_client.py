from __future__ import annotations

from datetime import UTC, datetime
from uuid import uuid4

import httpx
import pytest

from errors import QfError
from quant_runtime.client import NautilusQuantRuntime, RemoteNautilusConfig
from quant_runtime.contracts import (
    BacktestExperimentRequest,
    ExperimentMode,
    StrategyArtifact,
)


def _request(mode: ExperimentMode = ExperimentMode.DISCOVERY) -> BacktestExperimentRequest:
    return BacktestExperimentRequest(
        experiment_id=uuid4(),
        mode=mode,
        dataset_revision_id=uuid4(),
        catalog_key="prices-v1",
        instrument_ids=["EUR/USD.SIM"],
        strategy=StrategyArtifact(
            artifact_id="ema-v1",
            kind="IMPORTABLE",
            strategy_path="example:Strategy",
            config_path="example:Config",
        ),
    )


def test_client_sends_service_token_and_parses_evidence() -> None:
    request = _request()

    def handler(incoming: httpx.Request) -> httpx.Response:
        assert incoming.headers["authorization"] == "Bearer service-token"
        assert incoming.headers["idempotency-key"] == str(request.experiment_id)
        assert incoming.url.path == "/v1/backtests"
        return httpx.Response(
            200,
            json={
                "protocol_version": "1",
                "runtime_version": "1.231.0",
                "experiment_id": str(request.experiment_id),
                "remote_run_id": "remote-1",
                "mode": "DISCOVERY",
                "started_at": datetime.now(UTC).isoformat(),
                "finished_at": datetime.now(UTC).isoformat(),
                "orders": [],
                "fills": [],
                "positions": [],
                "balances": [],
                "pnl": {},
                "statistics": {},
                "diagnostics": {},
            },
        )

    config = RemoteNautilusConfig(
        base_url="https://runtime.example.test",
        token="service-token",
    )
    with NautilusQuantRuntime(config, transport=httpx.MockTransport(handler)) as runtime:
        evidence = runtime.run_backtest(request)
    assert evidence.remote_run_id == "remote-1"


def test_client_rejects_runtime_version_drift() -> None:
    def handler(_: httpx.Request) -> httpx.Response:
        return httpx.Response(
            200,
            json={
                "protocol_version": "1",
                "runtime_name": "NAUTILUS_TRADER",
                "runtime_version": "9.9.9",
                "catalog_kind": "PARQUET_DATA_CATALOG",
                "supported_operations": [],
                "live_execution_exposed": False,
            },
        )

    config = RemoteNautilusConfig(
        base_url="https://runtime.example.test",
        token=None,
    )
    with NautilusQuantRuntime(config, transport=httpx.MockTransport(handler)) as runtime:
        with pytest.raises(QfError) as raised:
            runtime.capabilities()
    assert raised.value.code == "NAUTILUS_RUNTIME_VERSION_MISMATCH"


def test_sealed_response_cannot_contain_raw_evidence() -> None:
    request = _request(ExperimentMode.SEALED)

    def handler(_: httpx.Request) -> httpx.Response:
        return httpx.Response(
            200,
            json={
                "protocol_version": "1",
                "runtime_version": "1.231.0",
                "experiment_id": str(request.experiment_id),
                "remote_run_id": "sealed-1",
                "mode": "SEALED",
                "disclosure": {"passed": True},
                "raw_evidence_withheld": True,
                "orders": [{"order_id": "leak"}],
            },
        )

    config = RemoteNautilusConfig(
        base_url="https://sealed.example.test",
        token="sealed-token",
    )
    with NautilusQuantRuntime(config, transport=httpx.MockTransport(handler)) as runtime:
        with pytest.raises(QfError) as raised:
            runtime.run_sealed_backtest(request)
    assert raised.value.code == "NAUTILUS_RUNTIME_PROTOCOL_INVALID"


def test_remote_url_requires_tls() -> None:
    config = RemoteNautilusConfig(base_url="http://runtime.example.test", token=None)
    with pytest.raises(QfError) as raised:
        NautilusQuantRuntime(config)
    assert raised.value.code == "NAUTILUS_RUNTIME_TLS_REQUIRED"
