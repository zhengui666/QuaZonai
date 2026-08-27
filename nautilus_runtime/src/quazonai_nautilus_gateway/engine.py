"""Canonical NautilusTrader catalog, backtest and bundle-conformance engine."""

from __future__ import annotations

import base64
from contextlib import contextmanager
from datetime import UTC, datetime
from decimal import Decimal
import hashlib
import importlib
import io
import json
from pathlib import Path
import sys
import tempfile
from typing import Any, Iterator
from uuid import uuid4
import zipfile

import pandas as pd

from nautilus_trader import __version__ as nautilus_version
from nautilus_trader.backtest.config import (
    BacktestDataConfig,
    BacktestEngineConfig,
    BacktestRunConfig,
    BacktestVenueConfig,
)
from nautilus_trader.backtest.node import BacktestNode
from nautilus_trader.config import ImportableStrategyConfig, StrategyConfig
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.persistence.catalog.parquet import ParquetDataCatalog
from nautilus_trader.persistence.wranglers import QuoteTickDataWrangler
from nautilus_trader.test_kit.providers import TestInstrumentProvider
from nautilus_trader.trading.strategy import Strategy

from quazonai_nautilus_gateway import PROTOCOL_VERSION, VALIDATED_NAUTILUS_VERSION
from quazonai_nautilus_gateway.models import (
    BacktestExperimentRequest,
    CandidateVerificationRequest,
    CatalogIngestRequest,
    CatalogValidationRequest,
    ExperimentMode,
)


class GatewayContractError(ValueError):
    pass


def _utc_now() -> datetime:
    return datetime.now(UTC)


def _jsonable(value: Any) -> Any:
    if value is None or isinstance(value, (bool, int, float, str)):
        return value
    if isinstance(value, Decimal):
        return str(value)
    if isinstance(value, datetime):
        return value.isoformat()
    if isinstance(value, dict):
        return {str(key): _jsonable(item) for key, item in value.items()}
    if isinstance(value, (list, tuple, set)):
        return [_jsonable(item) for item in value]
    if hasattr(value, "to_dict"):
        try:
            return _jsonable(value.to_dict())
        except TypeError:
            try:
                return _jsonable(type(value).to_dict(value))
            except Exception:
                pass
    if hasattr(value, "as_dict"):
        try:
            return _jsonable(value.as_dict())
        except Exception:
            pass
    return str(value)


def _attr(value: Any, *names: str) -> Any:
    for name in names:
        if hasattr(value, name):
            candidate = getattr(value, name)
            if callable(candidate):
                try:
                    candidate = candidate()
                except TypeError:
                    continue
            return candidate
    return None


def _safe_rel_path(path: str) -> Path:
    candidate = Path(path)
    if candidate.is_absolute() or ".." in candidate.parts:
        raise GatewayContractError("artifact path traversal is forbidden")
    return candidate


class NautilusGatewayEngine:
    def __init__(self, data_root: Path) -> None:
        if nautilus_version != VALIDATED_NAUTILUS_VERSION:
            raise RuntimeError(
                f"validated Nautilus version is {VALIDATED_NAUTILUS_VERSION}, got {nautilus_version}"
            )
        self._data_root = data_root.resolve()
        self._catalog_root = self._data_root / "catalogs"
        self._artifact_root = self._data_root / "artifacts"
        self._catalog_root.mkdir(parents=True, exist_ok=True)
        self._artifact_root.mkdir(parents=True, exist_ok=True)

    def capabilities(self) -> dict[str, Any]:
        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_name": "NAUTILUS_TRADER",
            "runtime_version": nautilus_version,
            "catalog_kind": "PARQUET_DATA_CATALOG",
            "supported_operations": [
                "CATALOG_INGEST",
                "CATALOG_VALIDATE",
                "BACKTEST",
                "SEALED_BACKTEST",
                "CANDIDATE_VERIFY",
            ],
            "live_execution_exposed": False,
        }

    def _catalog_path(self, key: str) -> Path:
        path = (self._catalog_root / key).resolve()
        if path.parent != self._catalog_root:
            raise GatewayContractError("catalog key escaped the configured root")
        return path

    @staticmethod
    def _instrument(requested_id: str) -> Any:
        symbol = requested_id.split(".", 1)[0]
        instrument = TestInstrumentProvider.default_fx_ccy(symbol)
        if str(instrument.id) != requested_id:
            raise GatewayContractError(
                f"this reference gateway resolves {requested_id!r} as {instrument.id!s}; "
                "production providers must install the matching instrument definition"
            )
        return instrument

    def ingest(self, request: CatalogIngestRequest) -> dict[str, Any]:
        instrument = self._instrument(request.instrument_id)
        frame = pd.DataFrame(
            {
                "bid_price": [row.bid_price for row in request.rows],
                "ask_price": [row.ask_price for row in request.rows],
                "bid_size": [row.volume or "1000000" for row in request.rows],
                "ask_size": [row.volume or "1000000" for row in request.rows],
            },
            index=pd.DatetimeIndex([row.timestamp for row in request.rows], name="timestamp"),
        ).sort_index()
        if not frame.index.is_monotonic_increasing or frame.index.has_duplicates:
            raise GatewayContractError("event timestamps must be unique and monotonically increasing")
        if (frame["bid_price"].map(Decimal) > frame["ask_price"].map(Decimal)).any():
            raise GatewayContractError("bid price cannot exceed ask price")

        ticks = QuoteTickDataWrangler(instrument).process(frame)
        catalog_path = self._catalog_path(request.catalog_key)
        catalog_path.mkdir(parents=True, exist_ok=True)
        catalog = ParquetDataCatalog(path=str(catalog_path))
        catalog.write_data([instrument])
        catalog.write_data(ticks)
        event_start = request.rows[0].timestamp.astimezone(UTC)
        event_end = request.rows[-1].timestamp.astimezone(UTC)
        schema_revision = hashlib.sha256(
            b"QuoteTick:timestamp,bid_price,ask_price,bid_size,ask_size:v1"
        ).hexdigest()
        manifest = {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_version": nautilus_version,
            "catalog_key": request.catalog_key,
            "provider": request.provider,
            "source": request.source,
            "source_license": request.source_license,
            "instrument_id": str(instrument.id),
            "nautilus_data_type": request.nautilus_data_type,
            "event_time_start": event_start.isoformat(),
            "event_time_end": event_end.isoformat(),
            "row_count": len(ticks),
            "schema_revision": schema_revision,
            "quality_result": {
                "state": "VALID",
                "duplicate_timestamps": 0,
                "crossed_quotes": 0,
                "sorted": True,
            },
            "point_in_time_result": {
                "state": "VALID",
                "event_time_ordered": True,
                "ingestion_time_recorded": True,
            },
            "ingested_at": _utc_now().isoformat(),
        }
        (catalog_path / "quazonai-catalog-manifest.json").write_text(
            json.dumps(manifest, indent=2), encoding="utf-8"
        )
        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_version": nautilus_version,
            "catalog_key": request.catalog_key,
            "catalog_uri": f"nautilus-catalog://{request.catalog_key}",
            "nautilus_data_type": request.nautilus_data_type,
            "instrument_scope": [str(instrument.id)],
            "event_time_start": event_start,
            "event_time_end": event_end,
            "row_count": len(ticks),
            "schema_revision": schema_revision,
            "quality_result": manifest["quality_result"],
            "point_in_time_result": manifest["point_in_time_result"],
            "ingested_at": datetime.fromisoformat(manifest["ingested_at"]),
        }

    def validate_catalog(self, request: CatalogValidationRequest) -> dict[str, Any]:
        catalog_path = self._catalog_path(request.catalog_key)
        manifest_path = catalog_path / "quazonai-catalog-manifest.json"
        findings: list[dict[str, Any]] = []
        if not manifest_path.exists():
            return {
                "protocol_version": PROTOCOL_VERSION,
                "runtime_version": nautilus_version,
                "catalog_key": request.catalog_key,
                "valid": False,
                "instrument_scope": [],
                "row_count": 0,
                "findings": [{"code": "CATALOG_MANIFEST_MISSING"}],
            }
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        catalog = ParquetDataCatalog(path=str(catalog_path))
        instruments = catalog.instruments()
        scope = [str(instrument.id) for instrument in instruments]
        requested = set(request.instrument_ids)
        if requested and not requested.issubset(scope):
            findings.append(
                {
                    "code": "INSTRUMENT_SCOPE_MISMATCH",
                    "missing": sorted(requested.difference(scope)),
                }
            )
        if request.nautilus_data_type and request.nautilus_data_type != manifest["nautilus_data_type"]:
            findings.append(
                {
                    "code": "DATA_TYPE_MISMATCH",
                    "expected": request.nautilus_data_type,
                    "actual": manifest["nautilus_data_type"],
                }
            )
        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_version": nautilus_version,
            "catalog_key": request.catalog_key,
            "valid": not findings and bool(instruments),
            "instrument_scope": scope,
            "row_count": manifest["row_count"],
            "event_time_start": manifest["event_time_start"],
            "event_time_end": manifest["event_time_end"],
            "findings": findings,
        }

    @contextmanager
    def _strategy(self, request: BacktestExperimentRequest) -> Iterator[ImportableStrategyConfig]:
        strategy = request.strategy
        if strategy.requirements:
            normalized = {item.replace(" ", "").lower() for item in strategy.requirements}
            allowed = {f"nautilus_trader=={VALIDATED_NAUTILUS_VERSION}"}
            if not normalized.issubset(allowed):
                raise GatewayContractError(
                    "remote artifact dependencies must be pre-approved in the runtime image"
                )
        if strategy.kind == "IMPORTABLE":
            yield ImportableStrategyConfig(
                strategy_path=strategy.strategy_path,
                config_path=strategy.config_path,
                config=self._materialize_config(strategy.config, request),
            )
            return

        with tempfile.TemporaryDirectory(prefix="qz-strategy-", dir=self._artifact_root) as directory:
            root = Path(directory)
            for relative, content in strategy.source_files.items():
                destination = root / _safe_rel_path(relative)
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(content, encoding="utf-8")
            sys.path.insert(0, str(root))
            importlib.invalidate_caches()
            try:
                yield ImportableStrategyConfig(
                    strategy_path=strategy.strategy_path,
                    config_path=strategy.config_path,
                    config=self._materialize_config(strategy.config, request),
                )
            finally:
                sys.path.remove(str(root))
                module_names = {
                    strategy.strategy_path.split(":", 1)[0],
                    strategy.config_path.split(":", 1)[0],
                }
                for module_name in module_names:
                    sys.modules.pop(module_name, None)

    @staticmethod
    def _materialize_config(
        config: dict[str, Any], request: BacktestExperimentRequest
    ) -> dict[str, Any]:
        result = dict(config)
        instrument = NautilusGatewayEngine._instrument(request.instrument_ids[0])
        if "instrument_id" in result:
            result["instrument_id"] = instrument.id
        if "bar_type" in result:
            from nautilus_trader.model.data import BarType

            result["bar_type"] = BarType.from_str(str(result["bar_type"]))
        for key in ("trade_size", "order_qty", "quantity"):
            if key in result:
                result[key] = Decimal(str(result[key]))
        return result

    def run_backtest(self, request: BacktestExperimentRequest) -> dict[str, Any]:
        if request.protocol_version != PROTOCOL_VERSION:
            raise GatewayContractError("unsupported protocol version")
        validation = self.validate_catalog(
            CatalogValidationRequest(
                catalog_key=request.catalog_key,
                instrument_ids=request.instrument_ids,
                nautilus_data_type="QuoteTick",
            )
        )
        if not validation["valid"]:
            raise GatewayContractError(f"catalog validation failed: {validation['findings']!r}")
        catalog_path = self._catalog_path(request.catalog_key)
        instrument = self._instrument(request.instrument_ids[0])
        venue = str(instrument.id).rsplit(".", 1)[-1]
        started_at = _utc_now()

        with self._strategy(request) as strategy_config:
            engine_config = BacktestEngineConfig(strategies=[strategy_config])
            venue_defaults = {
                "name": venue,
                "oms_type": "HEDGING",
                "account_type": "MARGIN",
                "base_currency": "USD",
                "starting_balances": ["1_000_000 USD"],
                "book_type": "L1_MBP",
            }
            venue_defaults.update(request.venue_config)
            data_config = BacktestDataConfig(
                catalog_path=str(catalog_path),
                data_cls=QuoteTick,
                instrument_id=instrument.id,
                start_time=request.start_time,
                end_time=request.end_time,
            )
            run_config = BacktestRunConfig(
                engine=engine_config,
                venues=[BacktestVenueConfig(**venue_defaults)],
                data=[data_config],
            )
            node = BacktestNode(configs=[run_config])
            try:
                results = node.run()
                if not results:
                    raise RuntimeError("Nautilus BacktestNode returned no result")
                result = results[0]
                try:
                    engine = node.get_engine(run_config.id)
                except (AttributeError, KeyError):
                    engine = getattr(node, "_engines", {}).get(run_config.id)
                if engine is None:
                    raise RuntimeError("Nautilus BacktestNode did not retain its engine")
                evidence = self._evidence(request, result, engine, started_at)
            finally:
                node.dispose()
        return evidence

    def _evidence(
        self,
        request: BacktestExperimentRequest,
        result: Any,
        engine: Any,
        started_at: datetime,
    ) -> dict[str, Any]:
        orders = list(engine.cache.orders())
        positions = list(engine.cache.positions())
        try:
            accounts = list(engine.cache.accounts())
        except (AttributeError, TypeError):
            accounts = []
        fills: list[Any] = []
        for order in orders:
            for event in list(_attr(order, "events") or []):
                if type(event).__name__ == "OrderFilled":
                    fills.append(event)

        order_payload = [
            {
                "order_id": str(_attr(order, "client_order_id", "id") or "UNKNOWN"),
                "instrument_id": _jsonable(_attr(order, "instrument_id")),
                "side": _jsonable(_attr(order, "side")),
                "order_type": _jsonable(_attr(order, "order_type")),
                "status": _jsonable(_attr(order, "status")),
                "quantity": _jsonable(_attr(order, "quantity")),
                "filled_quantity": _jsonable(_attr(order, "filled_qty", "filled_quantity")),
                "ts_init": _jsonable(_attr(order, "ts_init")),
            }
            for order in orders
        ]
        fill_payload = [
            {
                "trade_id": _jsonable(_attr(fill, "trade_id")),
                "order_id": _jsonable(_attr(fill, "client_order_id", "order_id")),
                "instrument_id": _jsonable(_attr(fill, "instrument_id")),
                "side": _jsonable(_attr(fill, "order_side", "side")),
                "quantity": _jsonable(_attr(fill, "last_qty", "quantity")),
                "price": _jsonable(_attr(fill, "last_px", "price")),
                "commission": _jsonable(_attr(fill, "commission")),
                "ts_event": _jsonable(_attr(fill, "ts_event")),
            }
            for fill in fills
        ]
        position_payload = [
            {
                "position_id": str(_attr(position, "id", "position_id") or "UNKNOWN"),
                "instrument_id": _jsonable(_attr(position, "instrument_id")),
                "side": _jsonable(_attr(position, "side")),
                "quantity": _jsonable(_attr(position, "quantity")),
                "realized_pnl": _jsonable(_attr(position, "realized_pnl")),
                "unrealized_pnl": _jsonable(_attr(position, "unrealized_pnl")),
                "opened_at": _jsonable(_attr(position, "ts_opened")),
                "closed_at": _jsonable(_attr(position, "ts_closed")),
            }
            for position in positions
        ]
        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_version": nautilus_version,
            "experiment_id": request.experiment_id,
            "remote_run_id": str(_attr(result, "run_id") or uuid4()),
            "mode": request.mode,
            "started_at": started_at,
            "finished_at": _utc_now(),
            "orders": order_payload,
            "fills": fill_payload,
            "positions": position_payload,
            "balances": [_jsonable(account) for account in accounts],
            "pnl": _jsonable(_attr(result, "stats_pnls") or {}),
            "statistics": {
                "returns": _jsonable(_attr(result, "stats_returns") or {}),
                "general": _jsonable(_attr(result, "stats_general") or {}),
                "total_events": _jsonable(_attr(result, "total_events") or 0),
                "total_orders": _jsonable(_attr(result, "total_orders") or len(orders)),
                "total_positions": _jsonable(_attr(result, "total_positions") or len(positions)),
                "iterations": _jsonable(_attr(result, "iterations") or 0),
                "elapsed_time": _jsonable(_attr(result, "elapsed_time") or 0),
            },
            "diagnostics": {
                "catalog_key": request.catalog_key,
                "instrument_ids": request.instrument_ids,
                "strategy_artifact_id": request.strategy.artifact_id,
            },
        }

    def run_sealed_backtest(self, request: BacktestExperimentRequest) -> dict[str, Any]:
        sealed = request.model_copy(update={"mode": ExperimentMode.SEALED})
        raw = self.run_backtest(sealed)
        statistics = raw["statistics"]
        disclosure = {
            "passed": bool(raw["fills"] and raw["positions"]),
            "statistics": statistics,
            "pnl_summary": raw["pnl"],
            "order_count": len(raw["orders"]),
            "fill_count": len(raw["fills"]),
            "position_count": len(raw["positions"]),
            "policy": "AGGREGATES_ONLY_V1",
        }
        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_version": nautilus_version,
            "experiment_id": request.experiment_id,
            "remote_run_id": raw["remote_run_id"],
            "mode": ExperimentMode.SEALED,
            "disclosure": disclosure,
            "raw_evidence_withheld": True,
        }

    def verify_candidate(self, request: CandidateVerificationRequest) -> dict[str, Any]:
        findings: list[dict[str, Any]] = []
        try:
            wheel = base64.b64decode(request.strategy_wheel_b64, validate=True)
        except ValueError:
            wheel = b""
            findings.append({"code": "STRATEGY_WHEEL_BASE64_INVALID"})
        if wheel:
            try:
                self._verify_wheel(wheel, request.manifest)
            except Exception as exc:
                findings.append({"code": "STRATEGY_WHEEL_INVALID", "detail": str(exc)})
        forbidden_keys = {"password", "secret", "api_key", "private_key", "broker_token"}

        def walk(value: Any, path: str = "manifest") -> None:
            if isinstance(value, dict):
                for key, item in value.items():
                    if str(key).lower() in forbidden_keys:
                        findings.append({"code": "LIVE_SECRET_IN_BUNDLE", "path": f"{path}.{key}"})
                    walk(item, f"{path}.{key}")
            elif isinstance(value, list):
                for index, item in enumerate(value):
                    walk(item, f"{path}[{index}]")

        walk(request.manifest)
        required = {
            "runtime",
            "strategy",
            "data_requirements",
            "backtest_run_config",
            "venue_config",
            "risk_config",
            "lineage",
            "evidence",
        }
        missing = sorted(required.difference(request.manifest))
        if missing:
            findings.append({"code": "MANIFEST_FIELDS_MISSING", "fields": missing})
        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_version": nautilus_version,
            "candidate_id": request.candidate_id,
            "compatible": not findings,
            "findings": findings,
        }

    @staticmethod
    def _verify_wheel(wheel: bytes, manifest: dict[str, Any]) -> None:
        with zipfile.ZipFile(io.BytesIO(wheel)) as archive:
            names = archive.namelist()
            if not names or any(Path(name).is_absolute() or ".." in Path(name).parts for name in names):
                raise GatewayContractError("wheel contains unsafe paths")
            metadata_names = [name for name in names if name.endswith(".dist-info/METADATA")]
            if len(metadata_names) != 1:
                raise GatewayContractError("wheel must contain exactly one METADATA file")
            metadata = archive.read(metadata_names[0]).decode("utf-8")
            pin = f"Requires-Dist: nautilus_trader (=={VALIDATED_NAUTILUS_VERSION})"
            alternate_pin = f"Requires-Dist: nautilus-trader (=={VALIDATED_NAUTILUS_VERSION})"
            if pin not in metadata and alternate_pin not in metadata:
                raise GatewayContractError("wheel does not pin the validated Nautilus version")
            strategy_path = str(manifest.get("strategy", {}).get("strategy_path", ""))
            config_path = str(manifest.get("strategy", {}).get("config_path", ""))
            if ":" not in strategy_path or ":" not in config_path:
                raise GatewayContractError("manifest strategy import paths are invalid")
            with tempfile.TemporaryDirectory(prefix="qz-wheel-verify-") as directory:
                wheel_path = Path(directory) / "candidate.whl"
                wheel_path.write_bytes(wheel)
                sys.path.insert(0, str(wheel_path))
                importlib.invalidate_caches()
                try:
                    strategy_module, strategy_name = strategy_path.split(":", 1)
                    config_module, config_name = config_path.split(":", 1)
                    strategy_class = getattr(importlib.import_module(strategy_module), strategy_name)
                    config_class = getattr(importlib.import_module(config_module), config_name)
                    if not issubclass(strategy_class, Strategy):
                        raise GatewayContractError("strategy class is not a Nautilus Strategy")
                    if not issubclass(config_class, StrategyConfig):
                        raise GatewayContractError("config class is not a Nautilus StrategyConfig")
                finally:
                    sys.path.remove(str(wheel_path))
                    sys.modules.pop(strategy_module, None)
                    sys.modules.pop(config_module, None)
