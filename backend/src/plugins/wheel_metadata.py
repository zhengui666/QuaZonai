"""Read plugin wheel metadata without importing or extracting plugin code."""

from __future__ import annotations

import configparser
import re
import zipfile
from dataclasses import dataclass
from email.parser import BytesParser
from email.policy import default
from pathlib import Path, PurePosixPath

from errors import QfError

_DISTRIBUTION_NORMALIZER = re.compile(r"[-_.]+")
_ENTRY_POINT_GROUP = "quazonai.plugins"


@dataclass(frozen=True, slots=True)
class PluginEntryPoint:
    name: str
    value: str


@dataclass(frozen=True, slots=True)
class WheelMetadata:
    distribution_name: str
    normalized_distribution_name: str
    version: str
    requires_python: str | None
    plugin_entry_points: tuple[PluginEntryPoint, ...]
    dist_info_dir: str


def normalize_distribution_name(value: str) -> str:
    return _DISTRIBUTION_NORMALIZER.sub("-", value).lower()


def _safe_archive_member(name: str) -> bool:
    path = PurePosixPath(name)
    return bool(name) and not path.is_absolute() and ".." not in path.parts


def inspect_wheel(path: Path) -> WheelMetadata:
    if path.name != Path(path.name).name or path.suffix != ".whl":
        raise QfError(
            "PLUGIN_ARTIFACT_INVALID",
            "Plugin artifacts must be wheel files with basename filenames.",
            422,
            {"filename": path.name},
        )
    if not path.is_file():
        raise QfError(
            "PLUGIN_ARTIFACT_INVALID",
            "Plugin wheel does not exist.",
            422,
            {"filename": path.name},
        )

    try:
        with zipfile.ZipFile(path) as archive:
            members = archive.infolist()
            unsafe = [item.filename for item in members if not _safe_archive_member(item.filename)]
            encrypted = [item.filename for item in members if item.flag_bits & 0x1]
            if unsafe or encrypted:
                raise QfError(
                    "PLUGIN_ARTIFACT_INVALID",
                    "Wheel contains unsafe or encrypted archive members.",
                    422,
                    {"unsafe_members": unsafe[:10], "encrypted_members": encrypted[:10]},
                )

            metadata_names = [
                item.filename
                for item in members
                if item.filename.endswith(".dist-info/METADATA")
            ]
            if len(metadata_names) != 1:
                raise QfError(
                    "PLUGIN_ARTIFACT_INVALID",
                    "Wheel must contain exactly one dist-info METADATA file.",
                    422,
                    {"metadata_files": metadata_names},
                )

            metadata_name = metadata_names[0]
            dist_info_dir = metadata_name.removesuffix("/METADATA")
            message = BytesParser(policy=default).parsebytes(archive.read(metadata_name))
            distribution_name = str(message.get("Name") or "").strip()
            version = str(message.get("Version") or "").strip()
            requires_python = str(message.get("Requires-Python") or "").strip() or None
            if not distribution_name or not version:
                raise QfError(
                    "PLUGIN_ARTIFACT_INVALID",
                    "Wheel METADATA must contain Name and Version.",
                    422,
                )

            entry_points_name = f"{dist_info_dir}/entry_points.txt"
            plugin_entry_points: list[PluginEntryPoint] = []
            if entry_points_name in archive.namelist():
                parser = configparser.ConfigParser(interpolation=None, strict=True)
                try:
                    parser.read_string(archive.read(entry_points_name).decode("utf-8"))
                except (UnicodeDecodeError, configparser.Error) as exc:
                    raise QfError(
                        "PLUGIN_ARTIFACT_INVALID",
                        "Wheel entry_points.txt is invalid.",
                        422,
                    ) from exc
                if parser.has_section(_ENTRY_POINT_GROUP):
                    for name, value in parser.items(_ENTRY_POINT_GROUP):
                        normalized_name = name.strip()
                        normalized_value = value.strip()
                        if not normalized_name or not normalized_value:
                            raise QfError(
                                "PLUGIN_ARTIFACT_INVALID",
                                "Plugin entry point name and value must be non-empty.",
                                422,
                            )
                        plugin_entry_points.append(
                            PluginEntryPoint(normalized_name, normalized_value)
                        )
    except zipfile.BadZipFile as exc:
        raise QfError(
            "PLUGIN_ARTIFACT_INVALID",
            "Plugin artifact is not a valid wheel archive.",
            422,
            {"filename": path.name},
        ) from exc

    return WheelMetadata(
        distribution_name=distribution_name,
        normalized_distribution_name=normalize_distribution_name(distribution_name),
        version=version,
        requires_python=requires_python,
        plugin_entry_points=tuple(plugin_entry_points),
        dist_info_dir=dist_info_dir,
    )


def validate_wheel_set(
    primary: WheelMetadata,
    dependencies: tuple[WheelMetadata, ...],
) -> PluginEntryPoint:
    if len(primary.plugin_entry_points) != 1:
        raise QfError(
            "PLUGIN_ARTIFACT_INVALID",
            "The primary wheel must declare exactly one quazonai.plugins entry point.",
            422,
            {"entry_point_count": len(primary.plugin_entry_points)},
        )

    dependency_entry_points = [
        {"distribution": item.distribution_name, "entry_points": len(item.plugin_entry_points)}
        for item in dependencies
        if item.plugin_entry_points
    ]
    if dependency_entry_points:
        raise QfError(
            "PLUGIN_ARTIFACT_INVALID",
            "Dependency wheels must not declare quazonai.plugins entry points.",
            422,
            {"dependencies": dependency_entry_points},
        )

    seen: dict[str, str] = {}
    for metadata in (primary, *dependencies):
        previous = seen.get(metadata.normalized_distribution_name)
        if previous is not None:
            raise QfError(
                "PLUGIN_DEPENDENCY_CONFLICT",
                "A wheel set cannot contain multiple versions of one distribution.",
                422,
                {
                    "distribution": metadata.distribution_name,
                    "versions": [previous, metadata.version],
                },
            )
        seen[metadata.normalized_distribution_name] = metadata.version

    return primary.plugin_entry_points[0]
