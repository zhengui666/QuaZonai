"""Execute one Nautilus BacktestNode run for an Experiment and parameter set."""

from __future__ import annotations

import argparse
import importlib.util
import json
import math
import shutil
import sys
from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from enum import Enum
from pathlib import Path
from types import ModuleType
from typing import Any, Literal
from uuid import UUID, uuid4

from quantfoundry.db.models import CatalogDataset, Experiment, StrategyVersion
from quantfoundry.db.session import create_database_engine, create_session_factory
from quantfoundry.errors import QfError
from quantfoundry.settings import Settings

Phase = Literal["train", "holdout"]


@dataclass(frozen=True, slots=True)
class BacktestContext:
    experiment: Experiment
    strategy: StrategyVersion
    dataset: CatalogDataset


def _load_context(settings: Settings, experiment_id: UUID) -> BacktestContext:
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    with factory() as session:
        experiment = session.get(Experiment, experiment_id)
        if experiment is None:
            raise QfError("EXPERIMENT_UNKNOWN", "Experiment does not exist.", 404)
        strategy = session.get(StrategyVersion, experiment.strategy_version_id)
        dataset = session.get(CatalogDataset, experiment.dataset_id)
        if strategy is None or dataset is None or dataset.state != "READY":
            raise QfError(
                "BACKTEST_INPUT_UNAVAILABLE",
                "Backtest Strategy or Dataset is unavailable.",
                503,
            )
        session.expunge(experiment)
        session.expunge(strategy)
        session.expunge(dataset)
        return BacktestContext(experiment=experiment, strategy=strategy, dataset=dataset)


def _load_strategy_module(path: Path, module_name: str) -> ModuleType:
    specification = importlib.util.spec_from_file_location(module_name, path)
    if specification is None or specification.loader is None:
        raise QfError("STRATEGY_FILE_INVALID", "Strategy module could not be loaded.", 422)
    module = importlib.util.module_from_spec(specification)
    sys.modules[module_name] = module
    specification.loader.exec_module(module)
    return module


def _json_safe(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, bool)):
        return value
    if isinstance(value, float):
        return value if math.isfinite(value) else str(value)
    if isinstance(value, (Decimal, UUID, Path, datetime, Enum)):
        return str(value)
    if isinstance(value, dict):
        return {str(key): _json_safe(item) for key, item in value.items()}
    if isinstance(value, (list, tuple, set)):
        return [_json_safe(item) for item in value]
    if hasattr(value, "to_dict") and callable(value.to_dict):
        return _json_safe(value.to_dict())
    if hasattr(value, "model_dump") and callable(value.model_dump):
        return _json_safe(value.model_dump())
    return str(value)


def _nanos(value: datetime) -> int:
    return int(value.timestamp() * 1_000_000_000)


def execute_backtest(
    settings: Settings,
    *,
    experiment_id: UUID,
    parameters: dict[str, Any],
    phase: Phase,
) -> dict[str, Any]:
    try:
        from nautilus_trader.backtest import (
            BacktestDataConfig,
            BacktestEngineConfig,
            BacktestNode,
            BacktestRunConfig,
            BacktestVenueConfig,
        )
        from nautilus_trader.model.enums import AccountType, BookType, OmsType
        from nautilus_trader.model.identifiers import InstrumentId
        from nautilus_trader.trading import ImportableStrategyConfig
    except ImportError as exc:
        raise QfError(
            "RESEARCH_RUNTIME_UNAVAILABLE",
            "NautilusTrader v2 is not installed in the backtest runtime.",
            503,
        ) from exc

    context = _load_context(settings, experiment_id)
    experiment = context.experiment
    start = experiment.train_start if phase == "train" else experiment.holdout_start
    end = experiment.train_end if phase == "train" else experiment.holdout_end
    catalog_path = settings.catalog_root / context.dataset.catalog_path
    if not catalog_path.is_dir():
        raise QfError(
            "BACKTEST_INPUT_UNAVAILABLE",
            "Nautilus catalog directory is unavailable.",
            503,
            {"dataset_id": str(context.dataset.id)},
        )

    staging = settings.import_root / "backtests" / f"{experiment_id}-{phase}-{uuid4()}"
    staging.mkdir(parents=True, exist_ok=False)
    module_name = f"qf_strategy_{context.strategy.id.hex}"
    source_path = staging / f"{module_name}.py"
    source_path.write_text(context.strategy.source_text, encoding="utf-8")
    sys.path.insert(0, str(staging))
    node: Any | None = None
    try:
        strategy_module = _load_strategy_module(source_path, module_name)
        config_payload = {**context.strategy.default_config, **parameters}
        strategy_config = ImportableStrategyConfig(
            strategy_path=f"{module_name}:Strategy",
            config_path=f"{module_name}:Config",
            config=config_payload,
        )
        instrument_id = InstrumentId.from_str(context.dataset.instrument_id)
        data_type = str(context.dataset.dataset_metadata.get("data_type", "OrderBookDeltas"))
        if not data_type.strip():
            raise QfError(
                "BACKTEST_INPUT_INVALID",
                "Dataset data_type must not be empty.",
                422,
            )
        venue = BacktestVenueConfig(
            name="POLYMARKET",
            oms_type=OmsType.NETTING,
            account_type=AccountType.CASH,
            starting_balances=["1000 pUSD"],
            book_type=BookType.L2_MBP,
        )
        data = BacktestDataConfig(
            data_type=data_type,
            catalog_path=str(catalog_path),
            instrument_id=instrument_id,
            start_time=_nanos(start),
            end_time=_nanos(end),
        )
        config = BacktestRunConfig(
            engine=BacktestEngineConfig(),
            data=[data],
            venues=[venue],
        )
        node = BacktestNode(configs=[config])
        node.build()
        node.add_strategy_from_config(config.id, strategy_config)
        results = node.run()
        if len(results) != 1:
            raise QfError(
                "BACKTEST_FAILED",
                "BacktestNode did not return exactly one result.",
                500,
                {"result_count": len(results)},
            )
        result = results[0]
        objective_function = getattr(strategy_module, "objectives", None)
        if objective_function is None or not callable(objective_function):
            raise QfError(
                "STRATEGY_FILE_INVALID",
                "Strategy module no longer exports objectives().",
                422,
            )
        objective_values = tuple(float(item) for item in objective_function(result))
        if len(objective_values) != len(context.experiment.objective_directions):
            raise QfError(
                "BACKTEST_FAILED",
                "Strategy objective count changed after version validation.",
                422,
            )
        return {
            "phase": phase,
            "parameters": _json_safe(parameters),
            "objectives": list(objective_values),
            "summary": _json_safe(result.summary),
            "stats_pnls": _json_safe(result.stats_pnls),
            "stats_returns": _json_safe(result.stats_returns),
            "stats_general": _json_safe(result.stats_general),
            "returns_series": _json_safe(result.returns_series),
            "iterations": result.iterations,
            "total_events": result.total_events,
            "total_orders": result.total_orders,
            "total_positions": result.total_positions,
            "run_started": result.run_started,
            "run_finished": result.run_finished,
            "backtest_start": result.backtest_start,
            "backtest_end": result.backtest_end,
            "elapsed_time_secs": result.elapsed_time_secs,
        }
    finally:
        if node is not None:
            dispose = getattr(node, "dispose", None)
            if dispose is not None and callable(dispose):
                dispose()
        try:
            sys.path.remove(str(staging))
        except ValueError:
            pass
        sys.modules.pop(module_name, None)
        shutil.rmtree(staging, ignore_errors=True)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one QuantFoundry Nautilus backtest")
    parser.add_argument("--experiment-id", required=True)
    parser.add_argument("--parameters-json", default="{}")
    parser.add_argument("--phase", choices=["train", "holdout"], required=True)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    parameters = json.loads(args.parameters_json)
    if not isinstance(parameters, dict):
        raise RuntimeError("parameters must be a JSON object")
    result = execute_backtest(
        Settings.from_env(),
        experiment_id=UUID(args.experiment_id),
        parameters=parameters,
        phase=args.phase,
    )
    print(json.dumps(result, separators=(",", ":"), default=str))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
