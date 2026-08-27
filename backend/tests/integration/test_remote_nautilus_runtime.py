from __future__ import annotations

import math
import os
from datetime import UTC, datetime, timedelta
from uuid import uuid4

import httpx
import pytest

from quant_runtime import (
    CatalogReference,
    ExperimentContract,
    RemoteNautilusQuantRuntime,
    StrategyArtifact,
)

pytestmark = pytest.mark.nautilus


def test_remote_runtime_ingests_catalog_and_runs_discovery_and_sealed_backtests() -> None:
    base_url = os.environ.get("QUAZONAI_NAUTILUS_TEST_URL")
    token = os.environ.get("QUAZONAI_NAUTILUS_TEST_TOKEN")
    if not base_url or not token:
        pytest.skip("remote Nautilus integration runtime is not configured")

    dataset_id = uuid4()
    start = datetime(2026, 1, 5, 0, 0, tzinfo=UTC)
    quotes = []
    for index in range(720):
        timestamp = start + timedelta(seconds=index * 10)
        mid = 1.10 + 0.006 * math.sin(index / 22.0) + 0.00001 * index
        quotes.append(
            {
                "timestamp": timestamp.isoformat(),
                "bid_price": round(mid - 0.00005, 5),
                "ask_price": round(mid + 0.00005, 5),
            }
        )
    key = f"issue22-{dataset_id.hex}"
    with httpx.Client(
        base_url=base_url,
        headers={"Authorization": f"Bearer {token}"},
        timeout=120,
    ) as client:
        ingested = client.post(
            "/v1/catalogs/ingest",
            json={
                "catalog_key": key,
                "dataset_revision_id": str(dataset_id),
                "provider": "CI normalized quote fixture",
                "source_license": "CI_TEST",
                "instrument": "EUR/USD",
                "quotes": quotes,
                "schema_revision": "quote-v1",
                "partition": "DISCOVERY",
            },
            headers={"Idempotency-Key": key},
        )
        ingested.raise_for_status()
        metadata = ingested.json()

    catalog = CatalogReference(
        dataset_revision_id=dataset_id,
        catalog_uri=metadata["catalog_uri"],
        nautilus_data_type=metadata["nautilus_data_type"],
        instrument_ids=metadata["instrument_scope"],
        partition="DISCOVERY",
        start_time=quotes[0]["timestamp"],
        end_time=quotes[-1]["timestamp"],
    )
    contract = ExperimentContract(
        catalog=catalog,
        strategy=StrategyArtifact(
            strategy_path="nautilus_trader.examples.strategies.ema_cross:EMACross",
            config_path="nautilus_trader.examples.strategies.ema_cross:EMACrossConfig",
            config={
                "fast_ema_period": 2,
                "slow_ema_period": 5,
                "trade_size": "10000",
            },
        ),
    )
    runtime = RemoteNautilusQuantRuntime(base_url=base_url, token=token, timeout_seconds=120)
    try:
        health = runtime.health()
        assert health["runtime_version"] == "1.231.0"
        validation = runtime.validate_catalog(catalog)
        assert validation.valid is True
        discovery = runtime.run_backtest(contract)
        assert discovery.runtime_version == "1.231.0"
        assert discovery.total_events > 0
        assert discovery.total_orders > 0
        assert discovery.reports["orders"]

        sealed = runtime.run_sealed_backtest(contract)
        assert sealed.partition == "SEALED"
        assert sealed.reports == {}
        assert sealed.disclosure["decision"] == "PASS"
        verification = runtime.verify_candidate(
            {
                "runtime": {"name": "NAUTILUS_TRADER", "version": "1.231.0"},
                "required_files": ["manifest.json", "requirements.lock", "lineage.json"],
            }
        )
        assert verification.valid is True
    finally:
        runtime.close()
