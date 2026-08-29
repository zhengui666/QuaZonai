from __future__ import annotations

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected one match, found {count}: {old[:120]!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


engine = "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py"
replace_once(
    engine,
    '''    masked: set[str] = set()\n    for candidate in (Path("/tmp"), Path("/run"), Path("/root"), data_root.resolve()):\n        resolved = str(candidate.resolve())\n        if candidate.exists() and resolved not in {"/", str(workspace.resolve())} and resolved not in masked:\n            command.extend(["--tmpfs", resolved])\n            masked.add(resolved)\n    home = Path.home().resolve()\n    executable = Path(sys.executable).resolve()\n    if home.exists() and home != Path("/") and not executable.is_relative_to(home):\n        resolved_home = str(home)\n        if resolved_home not in masked:\n            command.extend(["--tmpfs", resolved_home])\n''',
    '''    home = Path.home().resolve()\n    executable = Path(sys.executable).resolve()\n    mask_candidates = [Path("/tmp"), Path("/run"), Path("/root"), data_root.resolve()]\n    if home.exists() and home != Path("/") and not executable.is_relative_to(home):\n        mask_candidates.append(home)\n    masked_paths: list[Path] = []\n    for candidate in sorted(\n        {item.resolve() for item in mask_candidates if item.exists()},\n        key=lambda item: (len(item.parts), str(item)),\n    ):\n        if candidate == Path("/") or candidate == workspace.resolve():\n            continue\n        if any(candidate == parent or candidate.is_relative_to(parent) for parent in masked_paths):\n            continue\n        command.extend(["--tmpfs", str(candidate)])\n        masked_paths.append(candidate)\n''',
)

sandbox_test = "nautilus_runtime/tests/test_sandbox_namespace_profile.py"
replace_once(
    sandbox_test,
    '''    monkeypatch.setattr(gateway_engine.shutil, "which", lambda _: "/usr/bin/bwrap")\n    command = gateway_engine._source_bundle_sandbox_command(\n        operation="backtest",\n        workspace=tmp_path,\n        data_root=tmp_path / "gateway-data",\n    )\n''',
    '''    monkeypatch.setattr(gateway_engine.shutil, "which", lambda _: "/usr/bin/bwrap")\n    data_root = tmp_path / "gateway-data"\n    data_root.mkdir()\n    command = gateway_engine._source_bundle_sandbox_command(\n        operation="backtest",\n        workspace=tmp_path,\n        data_root=data_root,\n    )\n''',
)
replace_once(
    sandbox_test,
    '''    assert (gateway_package_root / "quazonai_nautilus_gateway").is_dir()\n''',
    '''    assert (gateway_package_root / "quazonai_nautilus_gateway").is_dir()\n\n    tmpfs_targets = [\n        command[index + 1]\n        for index, value in enumerate(command[:-1])\n        if value == "--tmpfs"\n    ]\n    assert "/tmp" in tmpfs_targets\n    assert str(data_root.resolve()) not in tmpfs_targets\n''',
)
