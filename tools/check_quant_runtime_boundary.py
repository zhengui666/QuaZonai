#!/usr/bin/env python3
"""Fail CI when QuaZonai Core takes ownership of the Nautilus runtime.

This replaces the former broad text ban.  Documentation, contracts and
Candidate Bundle contents may name NautilusTrader; executable Core modules may
not import it or expose live brokerage/order-control surfaces.
"""

from __future__ import annotations

import ast
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "backend" / "src"
GATEWAY_PYPROJECT = ROOT / "nautilus_runtime" / "pyproject.toml"
CORE_PYPROJECT = ROOT / "backend" / "pyproject.toml"
PIN = "nautilus_trader==1.231.0"

errors: list[str] = []

for path in sorted(CORE.rglob("*.py")):
    try:
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    except SyntaxError as exc:
        errors.append(f"{path.relative_to(ROOT)} cannot be parsed: {exc}")
        continue
    for node in ast.walk(tree):
        module = ""
        if isinstance(node, ast.Import):
            names = [item.name for item in node.names]
        elif isinstance(node, ast.ImportFrom):
            names = [node.module or ""]
        else:
            continue
        if any(name == "nautilus_trader" or name.startswith("nautilus_trader.") for name in names):
            errors.append(
                f"{path.relative_to(ROOT)}:{node.lineno} imports nautilus_trader in Core"
            )

core_pyproject = CORE_PYPROJECT.read_text(encoding="utf-8")
if re.search(r"(?im)^\s*[\"']?nautilus[-_]trader", core_pyproject):
    errors.append("backend/pyproject.toml must not install NautilusTrader in Core")

if not GATEWAY_PYPROJECT.exists():
    errors.append("nautilus_runtime/pyproject.toml is missing")
else:
    gateway = tomllib.loads(GATEWAY_PYPROJECT.read_text(encoding="utf-8"))
    dependencies = gateway.get("project", {}).get("dependencies", [])
    normalized = {str(value).replace(" ", "").lower() for value in dependencies}
    if PIN not in normalized:
        errors.append(f"remote gateway must pin the validated runtime exactly: {PIN}")

for path in sorted((CORE / "api").rglob("*.py")):
    text = path.read_text(encoding="utf-8").lower()
    forbidden = {
        "submit_order(": "order submission",
        "cancel_order(": "order cancellation",
        "modify_order(": "order modification",
        "tradingnode": "live TradingNode ownership",
        "interactivebrokerslive": "broker adapter ownership",
    }
    for token, label in forbidden.items():
        if token in text:
            errors.append(f"{path.relative_to(ROOT)} exposes forbidden {label}")

if errors:
    print("Quant-runtime ownership boundary failed:", file=sys.stderr)
    for error in errors:
        print(f" - {error}", file=sys.stderr)
    raise SystemExit(1)

print("Quant-runtime ownership boundary passed.")
