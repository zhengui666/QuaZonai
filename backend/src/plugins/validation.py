"""Build short-lived environments and validate plugin descriptors in child processes."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from uuid import UUID, uuid4

from errors import QfError
from plugins.contract import DescriptorSnapshot


@dataclass(frozen=True, slots=True)
class ValidationEnvironment:
    root: Path
    python: Path


def _venv_python(root: Path) -> Path:
    return root / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def _package_root() -> Path:
    return Path(__file__).resolve().parents[1]


def _run(
    command: list[str],
    *,
    timeout_seconds: int,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            command,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_seconds,
            env=env,
        )
    except subprocess.TimeoutExpired as exc:
        raise QfError(
            "PLUGIN_VALIDATION_FAILED",
            "Plugin validation process exceeded its time limit.",
            422,
            {"command": command[0]},
        ) from exc
    except subprocess.CalledProcessError as exc:
        raise QfError(
            "PLUGIN_VALIDATION_FAILED",
            "Plugin validation process failed.",
            422,
            {"stderr": exc.stderr[-4000:] if exc.stderr else ""},
        ) from exc


def create_validation_environment(
    *,
    staging_root: Path,
    release_id: UUID,
    wheel_paths: tuple[Path, ...],
    timeout_seconds: int,
) -> ValidationEnvironment:
    uv = shutil.which("uv")
    if uv is None:
        raise QfError(
            "PLUGIN_VALIDATION_FAILED",
            "The pinned uv executable is unavailable in the worker runtime.",
            503,
        )

    root = staging_root / f"validation-{release_id}-{uuid4()}"
    root.parent.mkdir(parents=True, exist_ok=True)
    _run(
        [uv, "venv", str(root), "--python", sys.executable, "--system-site-packages"],
        timeout_seconds=timeout_seconds,
    )
    python = _venv_python(root)
    _run(
        [
            uv,
            "pip",
            "install",
            "--python",
            str(python),
            "--offline",
            "--no-index",
            "--no-deps",
            *[str(path) for path in wheel_paths],
        ],
        timeout_seconds=timeout_seconds,
    )
    _run([uv, "pip", "check", "--python", str(python)], timeout_seconds=timeout_seconds)
    return ValidationEnvironment(root=root, python=python)


def validate_installed_plugin(
    environment: ValidationEnvironment,
    *,
    plugin_id: str,
    version: str,
    timeout_seconds: int,
) -> DescriptorSnapshot:
    process_env = os.environ.copy()
    package_root = str(_package_root())
    existing = process_env.get("PYTHONPATH")
    process_env["PYTHONPATH"] = (
        package_root if not existing else f"{package_root}{os.pathsep}{existing}"
    )
    result = _run(
        [
            str(environment.python),
            "-m",
            "plugins.validator_entry",
            "--plugin-id",
            plugin_id,
            "--version",
            version,
        ],
        timeout_seconds=timeout_seconds,
        env=process_env,
    )
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise QfError(
            "PLUGIN_VALIDATION_FAILED",
            "Plugin validator did not return a valid descriptor payload.",
            422,
        ) from exc
    return DescriptorSnapshot.model_validate(payload)


def remove_environment(environment: ValidationEnvironment | None) -> None:
    if environment is not None:
        shutil.rmtree(environment.root, ignore_errors=True)
