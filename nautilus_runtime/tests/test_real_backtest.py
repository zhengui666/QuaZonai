from __future__ import annotations

import base64
import io
import json
import zipfile
from datetime import UTC, datetime, timedelta
from pathlib import Path
from uuid import UUID, uuid4

import pytest
from nautilus_trader.test_kit.providers import TestInstrumentProvider

from quazonai_nautilus_gateway.engine import (
    GatewayContractError,
    NautilusGatewayEngine,
    _sealed_performance_disclosure,
)
from quazonai_nautilus_gateway.models import (
    BacktestExperimentRequest,
    CandidateVerificationRequest,
    CatalogIngestRequest,
    CatalogValidationRequest,
    ExperimentMode,
    InstrumentQuoteBatch,
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
        self._entered = False
        self._closed = False
        self._ticks = 0

    def on_start(self) -> None:
        self.subscribe_quote_ticks(self.config.instrument_id)

    def on_quote_tick(self, tick: QuoteTick) -> None:
        self._ticks += 1
        instrument = self.cache.instrument(self.config.instrument_id)
        if instrument is None:
            raise RuntimeError(f"instrument unavailable: {self.config.instrument_id}")
        if not self._entered:
            order = self.order_factory.market(
                instrument_id=self.config.instrument_id,
                order_side=OrderSide.BUY,
                quantity=instrument.make_qty(self.config.trade_size),
            )
            self.submit_order(order)
            self._entered = True
            return
        if not self._closed and self._ticks >= 20:
            order = self.order_factory.market(
                instrument_id=self.config.instrument_id,
                order_side=OrderSide.SELL,
                quantity=instrument.make_qty(self.config.trade_size),
            )
            self.submit_order(order)
            self._closed = True
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


def _batch(instrument_id: str, *, base: float) -> InstrumentQuoteBatch:
    instrument = TestInstrumentProvider.default_fx_ccy(instrument_id.split(".", 1)[0])
    assert str(instrument.id) == instrument_id
    return InstrumentQuoteBatch(
        instrument_id=instrument_id,
        instrument_type=type(instrument).__name__,
        instrument_definition=type(instrument).to_dict(instrument),
        rows=_rows(base=base),
    )


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
        venue_config={"oms_type": "NETTING"},
    )


def _candidate_wheel() -> bytes:
    stream = io.BytesIO()
    metadata = (
        "Metadata-Version: 2.3\n"
        "Name: quazonai-candidate-strategy\n"
        "Version: 0.0.1\n"
        "Requires-Dist: nautilus_trader (==1.231.0)\n\n"
    )
    with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("one_shot.py", _ONE_SHOT_SOURCE)
        archive.writestr("quazonai_candidate_strategy-0.0.1.dist-info/METADATA", metadata)
    return stream.getvalue()


def _ingest_fixture(engine: NautilusGatewayEngine) -> dict:
    return engine.ingest(
        CatalogIngestRequest(
            catalog_key="integration-fx-quotes",
            provider="CI_GENERATED_GOVERNED_QUOTES",
            source="fixture://deterministic-multi-fx-quotes",
            source_license="CC0-1.0",
            instruments=[
                _batch("EUR/USD.SIM", base=1.09),
                _batch("GBP/USD.SIM", base=1.27),
            ],
        )
    )


def test_real_catalog_backtest_and_sealed_disclosure(tmp_path: Path) -> None:
    engine = NautilusGatewayEngine(tmp_path)
    first = _ingest_fixture(engine)
    assert first["catalog_uri"] == "nautilus-catalog://integration-fx-quotes"
    assert first["row_count"] == 720
    assert first["instrument_scope"] == ["EUR/USD.SIM", "GBP/USD.SIM"]
    assert first["schema_revision"] == "nautilus.quote_tick.v2"
    assert first["quality_result"]["state"] == "VALID"
    assert first["point_in_time_result"]["replay_order"] == "TS_INIT"
    assert first["available_time_start"] > first["event_time_start"]

    replay = _ingest_fixture(engine)
    assert replay == first
    with pytest.raises(GatewayContractError, match="immutable"):
        engine.ingest(
            CatalogIngestRequest(
                catalog_key="integration-fx-quotes",
                provider="CI_GENERATED_GOVERNED_QUOTES",
                source="fixture://different-contract",
                source_license="CC0-1.0",
                instruments=[
                    _batch("EUR/USD.SIM", base=1.09),
                    _batch("GBP/USD.SIM", base=1.28),
                ],
            )
        )

    validated = engine.validate_catalog(
        CatalogValidationRequest(
            catalog_key="integration-fx-quotes",
            instrument_ids=["EUR/USD.SIM", "GBP/USD.SIM"],
            nautilus_data_type="QuoteTick",
        )
    )
    assert validated["valid"] is True, validated["findings"]
    assert validated["row_count"] == 720

    request = _request()
    evidence = engine.run_backtest(request)
    assert evidence["runtime_version"] == "1.231.0"
    assert evidence["statistics"]["iterations"] >= 720
    assert evidence["statistics"]["total_orders"] >= 2
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
    disclosure = sealed["disclosure"]
    assert disclosure["policy"] == "SEALED_LEVEL1_POLICY_V1"
    assert disclosure["passed"] is True, disclosure
    assert disclosure["quality_tier"] == "QUALIFIED"
    assert disclosure["reason_codes"] == ["SEALED_POLICY_PASSED"]
    for forbidden in (
        "quality_score",
        "performance",
        "order_count",
        "fill_count",
        "position_count",
        "statistics",
        "pnl_summary",
    ):
        assert forbidden not in disclosure


def test_catalog_validation_reads_parquet_instead_of_trusting_manifest(tmp_path: Path) -> None:
    engine = NautilusGatewayEngine(tmp_path)
    _ingest_fixture(engine)
    manifest_path = (
        tmp_path / "catalogs" / "integration-fx-quotes" / "quazonai-catalog-manifest.json"
    )
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    manifest["row_count"] = 999_999
    manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

    validation = engine.validate_catalog(
        CatalogValidationRequest(catalog_key="integration-fx-quotes")
    )
    assert validation["valid"] is False
    assert validation["row_count"] == 720
    assert any(
        item["code"] == "CATALOG_MANIFEST_ROW_COUNT_MISMATCH"
        for item in validation["findings"]
    )


def test_sealed_policy_rejects_trading_activity_with_negative_performance() -> None:
    disclosure = _sealed_performance_disclosure(
        {
            "orders": [{"order_id": "1"}],
            "fills": [{"trade_id": "1"}],
            "positions": [{"position_id": "1"}],
            "pnl": {"USD": {"PnL (total)": -10.0, "Profit Factor": 0.5}},
            "statistics": {"returns": {"Sharpe Ratio (252 days)": -0.2}, "general": {}},
        }
    )
    assert disclosure["passed"] is False
    assert disclosure["quality_tier"] == "REJECTED"
    assert "TOTAL_PNL_POLICY_FAILED" in disclosure["reason_codes"]
    assert disclosure["policy_checks"]["positive_total_pnl"] is False
    assert "quality_score" not in disclosure


def test_candidate_bundle_v2_replays_exact_wheel_against_reference_fixture(tmp_path: Path) -> None:
    engine = NautilusGatewayEngine(tmp_path)
    _ingest_fixture(engine)
    reference_request = _request()
    reference = engine.run_backtest(
        reference_request.model_copy(update={"mode": ExperimentMode.PORTFOLIO})
    )
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
            "artifact_id": reference_request.strategy.artifact_id,
            "wheel": (
                "strategy/quazonai_candidate_strategy-"
                f"0.0.{candidate_id.int % 1_000_000}-py3-none-any.whl"
            ),
            "strategy_path": "one_shot:OneShotStrategy",
            "config_path": "one_shot:OneShotConfig",
        },
        "data": {},
        "runtime_config": {},
        "validation": {},
        "evidence": {},
        "lineage": {},
        "target_weights": [
            {"instrument_id": "EUR/USD.SIM", "target_weight": "0.5"},
            {"instrument_id": "GBP/USD.SIM", "target_weight": "0.5"},
        ],
    }
    fixture = {
        "dataset_revision_id": str(reference_request.dataset_revision_id),
        "strategy_config": reference_request.strategy.config,
        "instrument_scope": reference_request.instrument_ids,
        "backtest_run_config": {
            "catalog_key": reference_request.catalog_key,
            "mode": "PORTFOLIO",
            "start_time": None,
            "end_time": None,
        },
        "venue_config": reference_request.venue_config,
        "risk_config": reference_request.risk_config,
        "orders": reference["orders"],
        "fills": reference["fills"],
        "positions": reference["positions"],
        "statistics": reference["statistics"],
    }
    verified = engine.verify_candidate(
        CandidateVerificationRequest(
            candidate_id=UUID(str(candidate_id)),
            manifest=manifest,
            strategy_wheel_b64=base64.b64encode(_candidate_wheel()).decode(),
            fixture=fixture,
        )
    )
    assert verified["compatible"] is True, verified["findings"]
    assert verified["runtime_version"] == "1.231.0"

    tampered = {**fixture, "orders": []}
    rejected = engine.verify_candidate(
        CandidateVerificationRequest(
            candidate_id=UUID(str(candidate_id)),
            manifest=manifest,
            strategy_wheel_b64=base64.b64encode(_candidate_wheel()).decode(),
            fixture=tampered,
        )
    )
    assert rejected["compatible"] is False
    assert any(item["code"] == "CONFORMANCE_REFERENCE_MISMATCH" for item in rejected["findings"])


def test_protocol_rejects_naive_timestamps() -> None:
    naive = datetime(2024, 1, 2)
    with pytest.raises(ValueError, match="timezone-aware"):
        QuoteRow(
            timestamp=naive,
            available_at=naive + timedelta(seconds=2),
            bid_price="1.0000",
            ask_price="1.0001",
        )
