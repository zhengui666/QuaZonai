"""Disposable child process for constrained strategy and wheel execution."""

from __future__ import annotations

import base64
import importlib
import json
import os
from pathlib import Path
import socket
import sys
from typing import Any
from uuid import uuid4

from quazonai_nautilus_gateway.engine import NautilusGatewayEngine, _jsonable
from quazonai_nautilus_gateway.models import (
    BacktestExperimentRequest,
    ExperimentMode,
    StrategyArtifact,
)

_TRUSTED_RESULT_NAME = ".trusted-result.json"


def _deny_external_network() -> None:
    original_connect = socket.socket.connect
    original_connect_ex = socket.socket.connect_ex

    def guarded_connect(sock: socket.socket, address: Any) -> Any:
        if sock.family in {socket.AF_INET, socket.AF_INET6}:
            raise OSError("network access is disabled in isolated strategy processes")
        return original_connect(sock, address)

    def guarded_connect_ex(sock: socket.socket, address: Any) -> int:
        if sock.family in {socket.AF_INET, socket.AF_INET6}:
            return 101
        return original_connect_ex(sock, address)

    def denied_create_connection(*_: Any, **__: Any) -> Any:
        raise OSError("network access is disabled in isolated strategy processes")

    socket.socket.connect = guarded_connect  # type: ignore[method-assign]
    socket.socket.connect_ex = guarded_connect_ex  # type: ignore[method-assign]
    socket.create_connection = denied_create_connection


def _run_candidate_conformance(
    engine: NautilusGatewayEngine,
    root: Path,
    payload: dict[str, Any],
) -> dict[str, Any]:
    wheel = base64.b64decode(payload["wheel_b64"], validate=True)
    manifest = payload["manifest"]
    fixture = payload["fixture"]
    engine._verify_wheel_inline(wheel, manifest)

    strategy_manifest = manifest["strategy"]
    run_config = fixture["backtest_run_config"]
    wheel_path = root / "candidate.whl"
    wheel_path.write_bytes(wheel)
    sys.path.insert(0, str(wheel_path))
    importlib.invalidate_caches()
    strategy_module = str(strategy_manifest["strategy_path"]).split(":", 1)[0]
    config_module = str(strategy_manifest["config_path"]).split(":", 1)[0]
    try:
        request = BacktestExperimentRequest(
            experiment_id=uuid4(),
            mode=ExperimentMode.CONFORMANCE,
            dataset_revision_id=fixture["dataset_revision_id"],
            catalog_key=run_config["catalog_key"],
            instrument_ids=fixture["instrument_scope"],
            strategy=StrategyArtifact(
                artifact_id=str(strategy_manifest["artifact_id"]),
                kind="IMPORTABLE",
                strategy_path=str(strategy_manifest["strategy_path"]),
                config_path=str(strategy_manifest["config_path"]),
                config=fixture["strategy_config"],
                requirements=["nautilus_trader==1.231.0"],
            ),
            start_time=run_config.get("start_time"),
            end_time=run_config.get("end_time"),
            venue_config=fixture.get("venue_config", {}),
            risk_config=fixture.get("risk_config", {}),
        )
        return engine.run_backtest(request, _source_isolated=True)
    finally:
        sys.path.remove(str(wheel_path))
        sys.modules.pop(strategy_module, None)
        sys.modules.pop(config_module, None)


def main() -> None:
    if len(sys.argv) != 4:
        raise SystemExit(2)
    operation, root_raw, input_raw = sys.argv[1:]
    root = Path(root_raw).resolve()
    input_path = Path(input_raw).resolve()
    if input_path.parent != root.parent:
        raise SystemExit(3)
    # The strategy never receives the trusted result pathname through argv, input, or env.
    # SOURCE_BUNDLE AST validation also denies filesystem/dynamic-import capabilities.
    output_path = root.parent / _TRUSTED_RESULT_NAME
    if os.getenv("QUAZONAI_NAUTILUS_ISOLATED_CHILD") != "1":
        raise SystemExit(4)
    os.chdir(root)
    _deny_external_network()
    payload = json.loads(input_path.read_text(encoding="utf-8"))
    engine = NautilusGatewayEngine(root)
    if operation == "backtest":
        request = BacktestExperimentRequest.model_validate(payload["request"])
        result = engine.run_backtest(request, _source_isolated=True)
    elif operation == "verify-wheel":
        wheel = base64.b64decode(payload["wheel_b64"], validate=True)
        engine._verify_wheel_inline(wheel, payload["manifest"])
        result = {"verified": True}
    elif operation == "verify-candidate":
        result = _run_candidate_conformance(engine, root, payload)
    else:
        raise SystemExit(5)
    output_path.write_text(json.dumps(_jsonable(result)), encoding="utf-8")


if __name__ == "__main__":
    main()
