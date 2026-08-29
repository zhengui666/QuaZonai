from __future__ import annotations

from pathlib import Path

import pytest

from quazonai_nautilus_gateway import engine as gateway_engine


def test_source_bundle_sandbox_uses_minimal_supported_namespaces(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setattr(gateway_engine.shutil, "which", lambda _: "/usr/bin/bwrap")
    command = gateway_engine._source_bundle_sandbox_command(
        operation="backtest",
        workspace=tmp_path,
    )
    assert "--unshare-all" not in command
    for flag in (
        "--unshare-user",
        "--unshare-ipc",
        "--unshare-pid",
        "--unshare-net",
        "--unshare-uts",
    ):
        assert flag in command
