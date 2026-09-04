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
    cache_root = staging_root / f".uv-cache-{uuid4()}"
    uv_environment = os.environ.copy()
    uv_environment["UV_CACHE_DIR"] = str(cache_root)
    try:
        _run(
            [uv, "venv", str(root), "--python", sys.executable, "--system-site-packages"],
            timeout_seconds=timeout_seconds,
            env=uv_environment,
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
            env=uv_environment,
        )
        _run(
            [uv, "pip", "check", "--python", str(python)],
            timeout_seconds=timeout_seconds,
            env=uv_environment,
        )
        return ValidationEnvironment(root=root, python=python)
    except Exception:
        shutil.rmtree(root, ignore_errors=True)
        raise
    finally:
        shutil.rmtree(cache_root, ignore_errors=True)


def validate_installed_plugin(
    environment: ValidationEnvironment,
    *,
    plugin_id: str,
    version: str,
    timeout_seconds: int,
) -> DescriptorSnapshot:
    bwrap = shutil.which("bwrap")
    if bwrap is None:
        raise QfError(
            "PLUGIN_VALIDATION_SANDBOX_UNAVAILABLE",
            "The bubblewrap sandbox is unavailable for plugin validation.",
            503,
        )
    package_root = _package_root().resolve()
    environment_root = environment.root.resolve()
    try:
        python_relative = environment.python.relative_to(environment.root)
    except ValueError as exc:
        raise QfError(
            "PLUGIN_VALIDATION_FAILED",
            "Plugin validation environment is outside its trusted root.",
            500,
        ) from exc
    sandbox_python = Path("/qz-plugin") / python_relative
    command = [
        bwrap,
        "--die-with-parent",
        "--new-session",
        "--unshare-net",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-uts",
        "--cap-drop",
        "ALL",
        "--ro-bind",
        str(environment_root),
        "/qz-plugin",
        "--ro-bind",
        str(package_root),
        "/qz-core",
        "--ro-bind",
        "/usr",
        "/usr",
        "--ro-bind",
        "/usr/local",
        "/usr/local",
        "--ro-bind",
        "/bin",
        "/bin",
        "--ro-bind",
        "/lib",
        "/lib",
        "--ro-bind",
        "/lib64",
        "/lib64",
        "--ro-bind",
        "/etc",
        "/etc",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--chdir",
        "/qz-plugin",
        "--clearenv",
        "--setenv",
        "PATH",
        "/usr/local/bin:/usr/bin:/bin",
        "--setenv",
        "LANG",
        "C.UTF-8",
        "--setenv",
        "LC_ALL",
        "C.UTF-8",
        "--setenv",
        "PYTHONNOUSERSITE",
        "1",
        "--setenv",
        "PYTHONPATH",
        "/qz-core",
        "--setenv",
        "HOME",
        "/tmp",
        "--",
        str(sandbox_python),
        "-m",
        "plugins.validator_entry",
        "--plugin-id",
        plugin_id,
        "--version",
        version,
    ]
    result = _run(
        command,
        timeout_seconds=timeout_seconds,
        env={"PATH": "/usr/local/bin:/usr/bin:/bin", "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8"},
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
