#!/usr/bin/env python3
"""Build this checkout's portable CLI and print its Cargo-reported executable path.

Pass the Cargo launcher, for example: rustup run 1.98.1 cargo.
The environment, including CARGO_TARGET_DIR and CARGO_BUILD_TARGET, is inherited.
"""
import json
import os
from pathlib import Path
import subprocess
import sys


if len(sys.argv) < 2:
    raise SystemExit("usage: build_portable_cli.py <cargo launcher...>")
result = subprocess.run(
    [
        *sys.argv[1:],
        "build",
        "--locked",
        "-p",
        "quazonai-cli",
        "--bin",
        "quazonai",
        "--message-format=json",
    ],
    cwd=Path(__file__).resolve().parents[2],
    stdout=subprocess.PIPE,
    text=True,
)
executables = set()
for line in result.stdout.splitlines():
    message = json.loads(line)
    if message.get("reason") == "compiler-message":
        sys.stderr.write(message["message"].get("rendered") or "")
    if (
        message.get("reason") == "compiler-artifact"
        and message["target"]["name"] == "quazonai"
        and "bin" in message["target"]["kind"]
        and message.get("executable")
    ):
        executables.add(message["executable"])
if result.returncode:
    raise SystemExit(result.returncode)
if len(executables) != 1:
    raise SystemExit("Cargo did not report exactly one portable quazonai executable")
executable = Path(executables.pop()).resolve(strict=True)
if not executable.is_file() or not os.access(executable, os.X_OK):
    raise SystemExit("Cargo-reported portable quazonai executable is not executable")
print(executable)
