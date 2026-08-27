from __future__ import annotations

from datetime import UTC, datetime, timedelta
from pathlib import Path
from uuid import uuid4

from quazonai_nautilus_gateway.engine import NautilusGatewayEngine
from quazonai_nautilus_gateway.models import (
    BacktestExperimentRequest,
    CatalogIngestRequest,
    CatalogValidationRequest,
    ExperimentMode,
    QuoteRow,
    StrategyArtifact,
)


def _rows() -> list[QuoteRow]:
    started = datetime(2024, 1, 2, tzinfo=UTC)
    result: list[QuoteRow] = []
    # Repeated trends force more than one fast/slow EMA crossing.
    for index in range(360):
        phase = index % 90
        offset = phase if phase < 45 else 90 - phase
        mid = 1.0900 + (offset - 22) * 0.0001
        result.append(
            QuoteRow(
                timestamp=started + timedelta(minutes=index),
                bid_price=f"{mid - 0.00005:.5f}",
                ask_price=f"{mid + 0.00005:.5f}",
                volume="1000000",
            )
        )
    return result


def _request() -> BacktestExperimentRequest:
    return BacktestExperimentRequest(
        experiment_id=uuid4(),
        mode=ExperimentMode.DISCOVERY,
        dataset_revision_id=uuid4(),
        catalog_key="integration-fx-quotes",
        instrument_ids=["EUR/USD.SIM"],
        strategy=StrategyArtifact(
            artifact_id="builtin-ema-cross-v1",
            kind="IMPORTABLE",
            strategy_path="nautilus_trader.examples.strategies.ema_cross:EMACross",
            config_path="nautilus_trader.examples.strategies.ema_cross:EMACrossConfig",
            config={
                "instrument_id": "EUR/USD.SIM",
                "bar_type": "EUR/USD.SIM-1-MINUTE-BID-INTERNAL",
                "trade_size": "100000",
                "fast_ema_period": 3,
                "slow_ema_period": 8,
            },
            requirements=["nautilus_trader==1.231.0"],
        ),
    )


def test_real_catalog_backtest_and_sealed_disclosure(tmp_path: Path) -> None:
    engine = NautilusGatewayEngine(tmp_path)
    ingested = engine.ingest(
        CatalogIngestRequest(
            catalog_key="integration-fx-quotes",
            provider="CI_GENERATED_CSV_EQUIVALENT",
            source="fixture://deterministic-eur-usd-quotes.csv",
            source_license="CC0-1.0",
            instrument_id="EUR/USD.SIM",
            rows=_rows(),
        )
    )
    assert ingested["catalog_uri"] == "nautilus-catalog://integration-fx-quotes"
    assert ingested["row_count"] == 360
    assert ingested["quality_result"]["state"] == "VALID"

    validated = engine.validate_catalog(
        CatalogValidationRequest(
            catalog_key="integration-fx-quotes",
            instrument_ids=["EUR/USD.SIM"],
            nautilus_data_type="QuoteTick",
        )
    )
    assert validated["valid"] is True

    request = _request()
    evidence = engine.run_backtest(request)
    assert evidence["runtime_version"] == "1.231.0"
    assert evidence["statistics"]["total_events"] > 0
    assert evidence["statistics"]["total_orders"] > 0
    assert evidence["orders"]
    assert evidence["fills"]
    assert evidence["positions"]
    assert isinstance(evidence["pnl"], dict)

    sealed = engine.run_sealed_backtest(
        request.model_copy(update={"mode": ExperimentMode.SEALED})
    )
    assert sealed["raw_evidence_withheld"] is True
    assert "orders" not in sealed
    assert "fills" not in sealed
    assert "positions" not in sealed
    assert sealed["disclosure"]["fill_count"] > 0
