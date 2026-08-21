"""Run one live plugin runtime inside an immutable bundle.

The process accepts one JSON configuration on stdin, performs Recovery without a
Strategy heartbeat, emits a reconciled projection, accepts a structured ARM command
containing the exact instrument limits, reports STRATEGY_READY, and starts trading
only after a separate START command. The plugin remains responsible for constructing
the official Nautilus adapter and TradingNode; QF does not duplicate venue protocol
logic.
"""

from __future__ import annotations

import json
import queue
import sys
import threading
from importlib import metadata
from typing import Any

from plugins.contract import DescriptorSnapshot


def _materialize(value: Any) -> Any:
    if isinstance(value, type) or callable(value) and not hasattr(value, "descriptor"):
        return value()
    return value


def _load_plugin(plugin_id: str) -> tuple[Any, DescriptorSnapshot]:
    candidates = [
        item
        for item in metadata.entry_points(group="quazonai.plugins")
        if item.name == plugin_id
    ]
    if len(candidates) != 1:
        raise RuntimeError(f"expected one plugin entry point named {plugin_id!r}")
    plugin = _materialize(candidates[0].load())
    descriptor_method = getattr(plugin, "descriptor", None)
    if descriptor_method is None or not callable(descriptor_method):
        raise RuntimeError("plugin entry point must expose descriptor()")
    return plugin, DescriptorSnapshot.model_validate(descriptor_method())


def _emit(kind: str, **payload: Any) -> None:
    print(json.dumps({"kind": kind, **payload}, separators=(",", ":"), default=str), flush=True)


def _parse_command(line: str) -> dict[str, Any]:
    stripped = line.strip()
    if not stripped:
        return {"command": "INVALID", "message": "empty command"}
    try:
        value = json.loads(stripped)
    except json.JSONDecodeError:
        return {"command": stripped.upper()}
    if not isinstance(value, dict):
        return {"command": "INVALID", "message": "command must be a JSON object"}
    command = str(value.get("command") or "").upper()
    return {**value, "command": command}


def _commands(target: queue.Queue[dict[str, Any]]) -> None:
    for line in sys.stdin:
        target.put(_parse_command(line))


def _stop_runtime(runtime: Any) -> None:
    stop = getattr(runtime, "stop", None)
    if stop is not None and callable(stop):
        stop()


def _wait_for(
    command_queue: queue.Queue[dict[str, Any]],
    runtime: Any,
    expected: str,
) -> dict[str, Any] | None:
    while True:
        message = command_queue.get()
        command = str(message.get("command") or "")
        if command == expected:
            return message
        if command == "STOP":
            _stop_runtime(runtime)
            _emit("STOPPED")
            return None
        _emit("ERROR", message=str(message.get("message") or f"unexpected command: {command}"))


def main() -> int:
    first_line = sys.stdin.readline()
    if not first_line:
        raise RuntimeError("live node did not receive its configuration")
    request = json.loads(first_line)
    if not isinstance(request, dict):
        raise RuntimeError("live node configuration must be a JSON object")

    data_plugin, data_descriptor = _load_plugin(str(request["data_plugin_id"]))
    execution_plugin, execution_descriptor = _load_plugin(
        str(request["execution_plugin_id"])
    )
    if data_descriptor.compatibility_key != execution_descriptor.compatibility_key:
        raise RuntimeError("data and execution plugin compatibility keys differ")

    factory = getattr(execution_plugin, "build_live_runtime", None)
    if factory is None or not callable(factory):
        raise RuntimeError("execution plugin must implement build_live_runtime()")
    runtime = factory(
        data_plugin=data_plugin,
        data_config=dict(request["data_config"]),
        data_secrets=dict(request.get("data_secrets") or {}),
        execution_config=dict(request["execution_config"]),
        execution_secrets=dict(request.get("execution_secrets") or {}),
        strategy_source=str(request["strategy_source"]),
        strategy_config=dict(request.get("strategy_config") or {}),
        universe_predicate=dict(request.get("universe_predicate") or {}),
        universe_cap=int(request["universe_cap"]),
        funder_id=str(request["funder_id"]),
        deployment_id=str(request["deployment_id"]),
        generation=int(request["generation"]),
    )
    recover = getattr(runtime, "recover", None)
    if recover is None or not callable(recover):
        raise RuntimeError("live runtime must implement recover()")
    recovery = recover()
    if not isinstance(recovery, dict):
        raise RuntimeError("recover() must return a dictionary")
    _emit(
        "RECOVERED",
        positions=list(recovery.get("positions") or []),
        open_orders=list(recovery.get("open_orders") or []),
        instruments=list(recovery.get("instruments") or []),
        details=dict(recovery.get("details") or {}),
    )

    command_queue: queue.Queue[dict[str, Any]] = queue.Queue()
    reader = threading.Thread(target=_commands, args=(command_queue,), daemon=True)
    reader.start()

    arm_message = _wait_for(command_queue, runtime, "ARM")
    if arm_message is None:
        return 0
    raw_limits = arm_message.get("instrument_limits_micros") or {}
    if not isinstance(raw_limits, dict) or not raw_limits:
        raise RuntimeError("ARM requires a non-empty instrument limit map")
    limits = {str(key): int(value) for key, value in raw_limits.items()}
    if any(value <= 0 for value in limits.values()):
        raise RuntimeError("instrument limits must be positive integers")
    arm = getattr(runtime, "arm", None)
    if arm is None or not callable(arm):
        raise RuntimeError("live runtime must implement arm(instrument_limits_micros)")
    arm(limits)
    _emit("STRATEGY_READY")

    start_message = _wait_for(command_queue, runtime, "START")
    if start_message is None:
        return 0
    start = getattr(runtime, "start", None)
    if start is None or not callable(start):
        raise RuntimeError("live runtime must implement start()")
    start()
    _emit("TRADING")

    heartbeat_seconds = float(request.get("heartbeat_seconds") or 5.0)
    if heartbeat_seconds <= 0:
        raise RuntimeError("heartbeat_seconds must be positive")
    while True:
        try:
            message = command_queue.get(timeout=heartbeat_seconds)
        except queue.Empty:
            alive = getattr(runtime, "is_alive", None)
            if alive is not None and callable(alive) and not bool(alive()):
                raise RuntimeError("live runtime stopped unexpectedly") from None
            _emit("HEARTBEAT")
            continue
        command = str(message.get("command") or "")
        if command == "STOP":
            _stop_runtime(runtime)
            _emit("STOPPED")
            return 0
        _emit("ERROR", message=str(message.get("message") or f"unexpected command: {command}"))


if __name__ == "__main__":
    raise SystemExit(main())
