from __future__ import annotations

import base64
import io
import zipfile
from datetime import UTC, datetime, timedelta
from pathlib import Path
from uuid import UUID, uuid4

from quazonai_nautilus_gateway.engine import NautilusGatewayEngine
from quazonai_nautilus_gateway.models import (
    BacktestExperimentRequest,
    CandidateVerificationRequest,
    CatalogIngestRequest,
    CatalogValidationRequest,
    ExperimentMode,
    QuoteRow,
    StrategyArtifact,
)


def _rows(*, base: float = 1.09) -> list[QuoteRow]:
    started = datetime(2024, 1, 2, tzinfo=UTC)
    result: list[QuoteRow] = []
    # Hold stable regimes and then step sharply between them. Once both EMAs are
    # initialized, each regime change deterministically flips fast-vs-slow and
    # therefore submits a real market order through Nautilus's matching engine.
    for index in range(360):
        if index < 60:
            mid = base - 0.01
        elif index < 180:
            mid = base + 0.01
        elif index < 300:
            mid = base - 0.01
        else:
            mid = base + 0.01
        timestamp = started + timedelta(minutes=index)
        result.append(
            QuoteRow(
                timestamp=timestamp,
                available_at=timestamp + timedelta(seconds=2),
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
        instrument_ids=["EUR/USD.SIM", "GBP/USD.SIM"],
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


def _candidate_wheel() -> bytes:
    stream = io.BytesIO()
    metadata = (
        "Metadata-Version: 2.3\n"
        "Name: quazonai-candidate-strategy\n"
        "Version: 0.0.1\n"
        "Requires-Dist: nautilus_trader (==1.231.0)\n\n"
    )
    source = (
        "from nautilus_trader.examples.strategies.ema_cross import "
        "EMACross as CandidateStrategy, EMACrossConfig as CandidateConfig\n"
    )
    with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("candidate_strategy.py", source)
        archive.writestr("quazonai_candidate_strategy-0.0.1.dist-info/METADATA", metadata)
    return stream.getvalue()


def test_real_catalog_backtest_and_sealed_disclosure(tmp_path: Path) -> None:
    engine = NautilusGatewayEngine(tmp_path)
    first = engine.ingest(
        CatalogIngestRequest(
            catalog_key="integration-fx-quotes",
            provider="CI_GENERATED_CSV_EQUIVALENT",
            source="fixture://deterministic-eur-usd-quotes.csv",
            source_license="CC0-1.0",
            instrument_id="EUR/USD.SIM",
            rows=_rows(),
        )
    )
    assert first["catalog_uri"] == "nautilus-catalog://integration-fx-quotes"
    assert first["row_count"] == 360
    assert first["quality_result"]["state"] == "VALID"
    assert first["point_in_time_result"]["replay_order"] == "TS_INIT"
    assert first["available_time_start"] > first["event_time_start"]

    second = engine.ingest(
        CatalogIngestRequest(
            catalog_key="integration-fx-quotes",
            provider="CI_GENERATED_CSV_EQUIVALENT",
            source="fixture://deterministic-gbp-usd-quotes.csv",
            source_license="CC0-1.0",
            instrument_id="GBP/USD.SIM",
            rows=_rows(base=1.27),
        )
    )
    assert second["row_count"] == 720
    assert second["instrument_scope"] == ["EUR/USD.SIM", "GBP/USD.SIM"]

    validated = engine.validate_catalog(
        CatalogValidationRequest(
            catalog_key="integration-fx-quotes",
            instrument_ids=["EUR/USD.SIM", "GBP/USD.SIM"],
            nautilus_data_type="QuoteTick",
        )
    )
    assert validated["valid"] is True
    assert validated["row_count"] == 720

    request = _request()
    evidence = engine.run_backtest(request)
    assert evidence["runtime_version"] == "1.231.0"
    # BacktestResult.total_events counts domain events, not loaded market data;
    # iterations is the direct proof that both 360-row QuoteTick streams ran.
    assert evidence["statistics"]["iterations"] >= 720
    assert evidence["statistics"]["total_orders"] > 0
    assert evidence["orders"]
    assert evidence["fills"]
    assert evidence["positions"]
    assert isinstance(evidence["pnl"], dict)
    assert evidence["diagnostics"]["loaded_instrument_count"] == 2

    sealed = engine.run_sealed_backtest(
        request.model_copy(update={"mode": ExperimentMode.SEALED})
    )
    assert sealed["raw_evidence_withheld"] is True
    assert "orders" not in sealed
    assert "fills" not in sealed
    assert "positions" not in sealed
    assert sealed["disclosure"]["fill_count"] > 0


def test_candidate_bundle_v2_conformance_uses_same_nautilus_strategy(tmp_path: Path) -> None:
    engine = NautilusGatewayEngine(tmp_path)
    candidate_id = uuid4()
    manifest = {
        "contract": "QUAZONAI_NAUTILUS_CANDIDATE_BUNDLE",
        "contract_version": "2",
        "bundle_id": str(uuid4()),
        "candidate_id": str(candidate_id),
        "runtime": {
            "name": "NAUTILUS_TRADER",
            "version": "1.231.0",
            "deployment": "REMOTE_INDEPENDENT_RUNTIME",
            "paper_live_reuse": "SAME_STRATEGY_WHEEL_AND_CONFIG",
        },
        "strategy": {
            "artifact_id": "source-bundle-1",
            "wheel": "strategy/strategy.whl",
            "strategy_path": "candidate_strategy:CandidateStrategy",
            "config_path": "candidate_strategy:CandidateConfig",
        },
        "data": {},
        "runtime_config": {},
        "validation": {},
        "evidence": {},
        "lineage": {},
        "target_weights": [],
    }
    verified = engine.verify_candidate(
        CandidateVerificationRequest(
            candidate_id=UUID(str(candidate_id)),
            manifest=manifest,
            strategy_wheel_b64=base64.b64encode(_candidate_wheel()).decode(),
            fixture={"orders": [], "positions": [], "statistics": {}},
        )
    )
    assert verified["compatible"] is True, verified["findings"]
    assert verified["runtime_version"] == "1.231.0"
