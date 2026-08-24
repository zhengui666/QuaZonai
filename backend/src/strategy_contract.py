"""Trusted single-file Strategy validation at a short-lived process boundary."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from uuid import uuid4

from errors import QfError

MAX_STRATEGY_SOURCE_BYTES = 1024 * 1024
MAX_STRATEGY_CONFIG_BYTES = 64 * 1024


@dataclass(frozen=True, slots=True)
class StrategyValidationResult:
    objective_directions: tuple[str, ...]


def decode_strategy_source(raw: bytes) -> str:
    if not raw:
        raise QfError("STRATEGY_FILE_INVALID", "Strategy source is empty.", 422)
    if len(raw) > MAX_STRATEGY_SOURCE_BYTES:
        raise QfError(
            "STRATEGY_FILE_INVALID",
            "Strategy source exceeds the 1 MiB limit.",
            413,
            {"max_bytes": MAX_STRATEGY_SOURCE_BYTES},
        )
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise QfError(
            "STRATEGY_FILE_INVALID",
            "Strategy source must be UTF-8 text.",
            422,
        ) from exc


def parse_strategy_config(raw: str) -> dict[str, object]:
    encoded = raw.encode("utf-8")
    if len(encoded) > MAX_STRATEGY_CONFIG_BYTES:
        raise QfError(
            "STRATEGY_CONFIG_INVALID",
            "Strategy configuration exceeds the 64 KiB limit.",
            413,
            {"max_bytes": MAX_STRATEGY_CONFIG_BYTES},
        )
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise QfError(
            "STRATEGY_CONFIG_INVALID",
            "Strategy configuration must be valid JSON.",
            422,
        ) from exc
    if not isinstance(value, dict):
        raise QfError(
            "STRATEGY_CONFIG_INVALID",
            "Strategy configuration must be a JSON object.",
            422,
        )
    return value


def validate_strategy_source(
    source_text: str,
    default_config: dict[str, object],
    *,
    staging_root: Path,
    timeout_seconds: int,
) -> StrategyValidationResult:
    validation_dir = staging_root / f"strategy-{uuid4()}"
    validation_dir.mkdir(parents=True, exist_ok=False)
    source_path = validation_dir / "strategy.py"
    source_path.write_text(source_text, encoding="utf-8")
    environment = os.environ.copy()
    package_root = str(Path(__file__).resolve().parents[1])
    existing = environment.get("PYTHONPATH")
    environment["PYTHONPATH"] = (
        package_root if not existing else f"{existing}{os.pathsep}{package_root}"
    )
    try:
        process = subprocess.run(
            [
                sys.executable,
                "-m",
                "quazonai.runners.strategy_validator",
                "--source",
                str(source_path),
                "--config-json",
                json.dumps(default_config, separators=(",", ":")),
            ],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_seconds,
            env=environment,
        )
    except subprocess.TimeoutExpired as exc:
        raise QfError(
            "STRATEGY_FILE_INVALID",
            "Strategy validation exceeded its time limit.",
            422,
        ) from exc
    except subprocess.CalledProcessError as exc:
        raise QfError(
            "STRATEGY_FILE_INVALID",
            "Strategy validation failed.",
            422,
            {"exit_code": exc.returncode},
        ) from exc
    finally:
        shutil.rmtree(validation_dir, ignore_errors=True)

    try:
        result = json.loads(process.stdout)
        directions = tuple(str(item) for item in result["objective_directions"])
    except (json.JSONDecodeError, KeyError, TypeError) as exc:
        raise QfError(
            "STRATEGY_FILE_INVALID",
            "Strategy validator returned an invalid response.",
            422,
        ) from exc
    return StrategyValidationResult(objective_directions=directions)
