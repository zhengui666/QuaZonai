from __future__ import annotations

import zipfile
from pathlib import Path

import pytest

from quantfoundry.errors import QfError
from quantfoundry.plugins.wheel_metadata import inspect_wheel, validate_wheel_set


def write_wheel(
    path: Path,
    *,
    name: str,
    version: str,
    plugin_id: str | None,
) -> None:
    dist_info = f"{name.replace('-', '_')}-{version}.dist-info"
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr(
            f"{dist_info}/METADATA",
            f"Metadata-Version: 2.4\nName: {name}\nVersion: {version}\n",
        )
        if plugin_id is not None:
            archive.writestr(
                f"{dist_info}/entry_points.txt",
                f"[quantfoundry.plugins]\n{plugin_id} = sample_plugin:plugin\n",
            )


def test_inspect_and_validate_primary_wheel(tmp_path: Path) -> None:
    primary_path = tmp_path / "sample_plugin-1.0.0-py3-none-any.whl"
    dependency_path = tmp_path / "sample_dep-2.0.0-py3-none-any.whl"
    write_wheel(primary_path, name="sample-plugin", version="1.0.0", plugin_id="sample")
    write_wheel(dependency_path, name="sample-dep", version="2.0.0", plugin_id=None)

    primary = inspect_wheel(primary_path)
    dependency = inspect_wheel(dependency_path)
    entry_point = validate_wheel_set(primary, (dependency,))

    assert primary.distribution_name == "sample-plugin"
    assert primary.version == "1.0.0"
    assert entry_point.name == "sample"
    assert entry_point.value == "sample_plugin:plugin"


def test_dependency_cannot_export_plugin_entry_point(tmp_path: Path) -> None:
    primary_path = tmp_path / "sample_plugin-1.0.0-py3-none-any.whl"
    dependency_path = tmp_path / "sample_dep-2.0.0-py3-none-any.whl"
    write_wheel(primary_path, name="sample-plugin", version="1.0.0", plugin_id="sample")
    write_wheel(dependency_path, name="sample-dep", version="2.0.0", plugin_id="other")

    with pytest.raises(QfError) as error:
        validate_wheel_set(inspect_wheel(primary_path), (inspect_wheel(dependency_path),))

    assert error.value.code == "PLUGIN_ARTIFACT_INVALID"
