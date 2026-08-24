"""Runtime environment construction for validated research/data plugin releases."""

from __future__ import annotations

import os
import shutil
import stat
import sys
from dataclasses import dataclass
from pathlib import Path
from uuid import UUID, uuid4

from quazonai import __version__
from errors import QfError
from plugins.contract import DescriptorSnapshot
from plugins.validation import (
    ValidationEnvironment,
    create_validation_environment,
    remove_environment,
    validate_installed_plugin,
)


@dataclass(frozen=True, slots=True)
class BundleBuildResult:
    environment_path: Path
    python_version: str
    qf_version: str


def resolve_plugin_path(plugin_root: Path, relative_path: str) -> Path:
    candidate = (plugin_root / relative_path).resolve()
    root = plugin_root.resolve()
    if candidate == root or root not in candidate.parents:
        raise QfError(
            "PLUGIN_ARTIFACT_INVALID",
            "Plugin artifact path escapes the plugin root.",
            500,
        )
    return candidate


def _remove_write_permissions(root: Path) -> None:
    for path in sorted(root.rglob("*"), key=lambda item: len(item.parts), reverse=True):
        try:
            current = stat.S_IMODE(path.stat().st_mode)
            path.chmod(current & ~0o222)
        except FileNotFoundError:
            continue
    current = stat.S_IMODE(root.stat().st_mode)
    root.chmod(current & ~0o222)


def validate_release_environment(
    *,
    staging_root: Path,
    release_id: UUID,
    wheel_paths: tuple[Path, ...],
    plugin_id: str,
    version: str,
    timeout_seconds: int,
) -> DescriptorSnapshot:
    environment: ValidationEnvironment | None = None
    try:
        environment = create_validation_environment(
            staging_root=staging_root,
            release_id=release_id,
            wheel_paths=wheel_paths,
            timeout_seconds=timeout_seconds,
        )
        return validate_installed_plugin(
            environment,
            plugin_id=plugin_id,
            version=version,
            timeout_seconds=timeout_seconds,
        )
    finally:
        remove_environment(environment)


def build_bundle_environment(
    *,
    plugin_root: Path,
    bundle_id: UUID,
    wheel_paths: tuple[Path, ...],
    expected_snapshots: tuple[DescriptorSnapshot, ...],
    timeout_seconds: int,
) -> BundleBuildResult:
    staging_parent = plugin_root / "bundle-staging"
    final_parent = plugin_root / "bundles"
    staging_parent.mkdir(parents=True, exist_ok=True)
    final_parent.mkdir(parents=True, exist_ok=True)

    temporary = staging_parent / f"{bundle_id}-{uuid4()}"
    final = final_parent / str(bundle_id)
    if final.exists():
        raise QfError(
            "PLUGIN_BUNDLE_BUILD_FAILED",
            "Runtime bundle destination already exists.",
            409,
            {"bundle_id": str(bundle_id)},
        )

    environment: ValidationEnvironment | None = None
    try:
        environment = create_validation_environment(
            staging_root=staging_parent,
            release_id=bundle_id,
            wheel_paths=wheel_paths,
            timeout_seconds=timeout_seconds,
        )
        temporary = environment.root
        for snapshot in expected_snapshots:
            actual = validate_installed_plugin(
                environment,
                plugin_id=snapshot.plugin_id,
                version=snapshot.version,
                timeout_seconds=timeout_seconds,
            )
            if actual != snapshot:
                raise QfError(
                    "PLUGIN_BUNDLE_BUILD_FAILED",
                    "Runtime descriptor differs from the validated release snapshot.",
                    422,
                    {"plugin_id": snapshot.plugin_id, "version": snapshot.version},
                )

        os.replace(temporary, final)
        environment = None
        _remove_write_permissions(final)
        return BundleBuildResult(
            environment_path=final,
            python_version=(
                f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}"
            ),
            qf_version=__version__,
        )
    except OSError as exc:
        raise QfError(
            "PLUGIN_BUNDLE_BUILD_FAILED",
            "Runtime bundle could not be published atomically.",
            500,
            {"bundle_id": str(bundle_id)},
        ) from exc
    finally:
        remove_environment(environment)
        if temporary.exists() and temporary != final:
            shutil.rmtree(temporary, ignore_errors=True)
