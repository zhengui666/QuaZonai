from __future__ import annotations

from pathlib import Path

import pytest

from quazonai_nautilus_gateway import engine as gateway_engine


def test_source_bundle_sandbox_uses_minimal_supported_namespaces(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setattr(gateway_engine.shutil, "which", lambda _: "/usr/bin/bwrap")
    data_root = tmp_path / "gateway-data"
    data_root.mkdir()
    command = gateway_engine._source_bundle_sandbox_command(
        operation="backtest",
        workspace=tmp_path,
        data_root=data_root,
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

    gateway_mount_index = command.index("/gateway-src")
    gateway_package_root = Path(command[gateway_mount_index - 1])
    assert gateway_package_root.name == "src"
    assert (gateway_package_root / "quazonai_nautilus_gateway").is_dir()

    tmpfs_targets = [
        command[index + 1]
        for index, value in enumerate(command[:-1])
        if value == "--tmpfs"
    ]
    assert "/tmp" in tmpfs_targets
    assert str(data_root.resolve()) not in tmpfs_targets
