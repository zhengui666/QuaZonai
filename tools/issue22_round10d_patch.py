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
    '''        "--ro-bind", "/", "/",\n        "--bind", str(workspace), "/sandbox",\n        "--ro-bind", str(gateway_source), "/gateway-src",\n''',
    '''        "--ro-bind", "/", "/",\n        "--dir", "/sandbox",\n        "--bind", str(workspace), "/sandbox",\n        "--dir", "/gateway-src",\n        "--ro-bind", str(gateway_source), "/gateway-src",\n''',
)

sandbox_test = "nautilus_runtime/tests/test_sandbox_namespace_profile.py"
replace_once(
    sandbox_test,
    '''    gateway_mount_index = command.index("/gateway-src")\n    gateway_package_root = Path(command[gateway_mount_index - 1])\n''',
    '''    sandbox_dir_index = command.index("/sandbox")\n    sandbox_bind_index = command.index("/sandbox", sandbox_dir_index + 1)\n    assert command[sandbox_dir_index - 1] == "--dir"\n    assert command[sandbox_bind_index - 2] == "--bind"\n    assert sandbox_dir_index < sandbox_bind_index\n\n    gateway_dir_index = command.index("/gateway-src")\n    gateway_mount_index = command.index("/gateway-src", gateway_dir_index + 1)\n    assert command[gateway_dir_index - 1] == "--dir"\n    assert command[gateway_mount_index - 2] == "--ro-bind"\n    assert gateway_dir_index < gateway_mount_index\n    gateway_package_root = Path(command[gateway_mount_index - 1])\n''',
)
