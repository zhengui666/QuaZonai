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

_ONE_SHOT_SOURCE = '''
from decimal import Decimal

from nautilus_trader.config import StrategyConfig
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.trading.strategy import Strategy


class OneShotConfig(StrategyConfig, frozen=True):
    instrument_id: InstrumentId
    trade_size: Decimal


class OneShotStrategy(Strategy):
    def __init__(self, config: OneShotConfig):
        super().__init__(config)
        self._submitted = False

    def on_start(self) -> None:
        self.subscribe_quote_ticks(self.config.instrument_id)

    def on_quote_tick(self, tick: QuoteTick) -> None:
        if self._submitted:
            return
        instrument = self.cache.instrument(self.config.instrument_id)
        if instrument is None:
            raise RuntimeError(f"instrument unavailable: {self.config.instrument_id}")
        order = self.order_factory.market(
            instrument_id=self.config.instrument_id,
            order_side=OrderSide.BUY,
            quantity=instrument.make_qty(self.config.trade_size),
        )
        self.submit_order(order)
        self._submitted = True
'''.lstrip()


def _rows(*, base: float = 1.09) -> list[QuoteRow]:
    started = datetime(2024, 1, 2, tzinfo=UTC)
    result: list[QuoteRow] = []
    for index in range(360):
        mid = base + ((index % 20) - 10) * 0.00001
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
            artifact_id="one-shot-source-bundle-v1",
            kind="SOURCE_BUNDLE",
            strategy_path="one_shot:OneShotStrategy",
            config_path="one_shot:OneShotConfig",
            config={
                "instrument_id": "EUR/USD.SIM",
                "trade_size": "100000",
            },
            source_files={"one_shot.py": _ONE_SHOT_SOURCE},
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
