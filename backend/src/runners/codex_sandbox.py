"""Preflight for the Codex command sandbox used by Mission workers."""

from __future__ import annotations

import importlib.util
import os
import shutil
import subprocess


def codex_bwrap_path() -> str:
    """Resolve the pinned Codex bundled bubblewrap executable."""
    spec = importlib.util.find_spec("codex_cli_bin")
    if spec is not None and spec.submodule_search_locations:
        bundled = os.path.join(
            next(iter(spec.submodule_search_locations)),
            "codex-resources",
            "bwrap",
        )
        if os.path.isfile(bundled) and os.access(bundled, os.X_OK):
            return bundled
    fallback = shutil.which("bwrap")
    if fallback:
        return fallback
    raise RuntimeError("Codex bundled bubblewrap executable is unavailable")


def codex_sandbox_preflight() -> None:
    """Fail closed unless the worker can create the Codex command sandbox."""
    command = [
        codex_bwrap_path(),
        "--die-with-parent",
        "--unshare-user",
        "--uid",
        "0",
        "--gid",
        "0",
        "--unshare-ipc",
        "--unshare-uts",
        "--unshare-cgroup",
        "--unshare-net",
        "--ro-bind",
        "/",
        "/",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--",
        "/bin/true",
    ]
    try:
        result = subprocess.run(
            command,
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
            timeout=10,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise RuntimeError("Codex workspace sandbox preflight could not start") from exc
    if result.returncode != 0:
        detail = (result.stderr or "").strip()[-500:]
        suffix = f": {detail}" if detail else ""
        raise RuntimeError(f"Codex workspace sandbox preflight failed{suffix}")
