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

    root_bind_index = command.index("--ro-bind")
    assert command[root_bind_index + 1 : root_bind_index + 3] == ["/", "/"]

    sandbox_dir_index = command.index("/sandbox")
    sandbox_bind_index = command.index("/sandbox", sandbox_dir_index + 1)
    assert root_bind_index < sandbox_dir_index
    assert command[sandbox_dir_index - 1] == "--dir"
    assert command[sandbox_bind_index - 2] == "--bind"
    assert sandbox_dir_index < sandbox_bind_index

    gateway_dir_index = command.index("/gateway-src")
    gateway_mount_index = command.index("/gateway-src", gateway_dir_index + 1)
    assert command[gateway_dir_index - 1] == "--dir"
    assert command[gateway_mount_index - 2] == "--ro-bind"
    assert gateway_dir_index < gateway_mount_index
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
    resolved_targets = [Path(value) for value in tmpfs_targets]
    assert all(
        not child.is_relative_to(parent)
        for index, parent in enumerate(resolved_targets)
        for child in resolved_targets[index + 1 :]
        if child != parent
    )
