"""Validate one trusted Strategy module in an isolated short-lived process."""

from __future__ import annotations

import argparse
import importlib.util
import inspect
import json
from pathlib import Path
from types import ModuleType
from typing import Any


def _load_module(path: Path) -> ModuleType:
    specification = importlib.util.spec_from_file_location("qf_uploaded_strategy", path)
    if specification is None or specification.loader is None:
        raise RuntimeError("strategy module could not be loaded")
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def _callable_with_one_argument(value: Any, name: str) -> None:
    if not callable(value):
        raise RuntimeError(f"strategy module must export callable {name}()")
    parameters = [
        parameter
        for parameter in inspect.signature(value).parameters.values()
        if parameter.kind
        in {
            inspect.Parameter.POSITIONAL_ONLY,
            inspect.Parameter.POSITIONAL_OR_KEYWORD,
        }
        and parameter.default is inspect.Parameter.empty
    ]
    if len(parameters) != 1:
        raise RuntimeError(f"{name}() must require exactly one positional argument")


def validate(path: Path, config: dict[str, Any]) -> tuple[str, ...]:
    try:
        from nautilus_trader.config import StrategyConfig
        from nautilus_trader.trading import Strategy as NautilusStrategy
    except ImportError as exc:
        raise RuntimeError("NautilusTrader v2 runtime is unavailable") from exc

    module = _load_module(path)
    config_type = getattr(module, "Config", None)
    strategy_type = getattr(module, "Strategy", None)
    if not isinstance(config_type, type) or not issubclass(config_type, StrategyConfig):
        raise RuntimeError("Config must subclass nautilus_trader.config.StrategyConfig")
    if not isinstance(strategy_type, type) or not issubclass(strategy_type, NautilusStrategy):
        raise RuntimeError("Strategy must subclass nautilus_trader.trading.Strategy")

    config_instance = config_type(**config)
    strategy_instance = strategy_type(config_instance)
    if not isinstance(strategy_instance, NautilusStrategy):
        raise RuntimeError("Strategy(config) must construct a Nautilus Strategy")
    dispose = getattr(strategy_instance, "dispose", None)
    if dispose is not None and callable(dispose):
        dispose()

    _callable_with_one_argument(getattr(module, "suggest", None), "suggest")
    _callable_with_one_argument(getattr(module, "objectives", None), "objectives")

    directions_value = getattr(module, "OBJECTIVE_DIRECTIONS", None)
    if not isinstance(directions_value, tuple) or len(directions_value) not in {2, 3}:
        raise RuntimeError("OBJECTIVE_DIRECTIONS must be a tuple containing 2 or 3 items")
    directions = tuple(str(item).lower() for item in directions_value)
    if any(item not in {"minimize", "maximize"} for item in directions):
        raise RuntimeError("objective directions must be minimize or maximize")
    return directions


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Validate an uploaded QuantFoundry Strategy")
    parser.add_argument("--source", required=True)
    parser.add_argument("--config-json", required=True)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    config = json.loads(args.config_json)
    if not isinstance(config, dict) or any(not isinstance(key, str) for key in config):
        raise RuntimeError("strategy config must be a JSON object with string keys")
    directions = validate(Path(args.source), config)
    print(json.dumps({"objective_directions": directions}, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
