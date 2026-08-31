"""Invoke one validated plugin inside its immutable runtime environment."""

from __future__ import annotations

import inspect
import json
import sys
from importlib import metadata
from typing import Any

from plugins.contract import Capability, DescriptorSnapshot


def _materialize(value: Any) -> Any:
    if isinstance(value, type):
        return value()
    if callable(value) and not hasattr(value, "descriptor"):
        return value()
    return value


def _load_plugin(plugin_id: str) -> tuple[Any, DescriptorSnapshot]:
    candidates = [
        item
        for item in metadata.entry_points(group="quazonai.plugins")
        if item.name == plugin_id
    ]
    if len(candidates) != 1:
        raise RuntimeError(f"expected one plugin entry point named {plugin_id!r}")
    plugin = _materialize(candidates[0].load())
    descriptor_method = getattr(plugin, "descriptor", None)
    if descriptor_method is None or not callable(descriptor_method):
        raise RuntimeError("plugin entry point must expose descriptor()")
    return plugin, DescriptorSnapshot.model_validate(descriptor_method())


def _invoke_builder(builder: Any, public: dict[str, Any], secret: dict[str, str]) -> Any:
    signature = inspect.signature(builder)
    positional = [
        item
        for item in signature.parameters.values()
        if item.kind
        in {
            inspect.Parameter.POSITIONAL_ONLY,
            inspect.Parameter.POSITIONAL_OR_KEYWORD,
        }
    ]
    if len(positional) >= 2:
        return builder(public, secret)
    return builder(public)


def _preflight(
    plugin: Any,
    descriptor: DescriptorSnapshot,
    request: dict[str, Any],
) -> dict[str, Any]:
    capability = Capability(str(request["capability"]))
    public_config = dict(request.get("public_config") or {})
    secret_config = {
        str(name): str(value) for name, value in dict(request.get("secret_config") or {}).items()
    }
    if capability not in descriptor.capabilities:
        raise RuntimeError(
            f"plugin {descriptor.plugin_id!r} does not provide capability {capability.value!r}"
        )

    if capability is Capability.HISTORICAL_IMPORT:
        method_name = "build_catalog_importer"
        preflight_name = "preflight_importer"
    elif capability is Capability.LIVE_DATA:
        method_name = "build_data_config"
        preflight_name = "preflight_data"
    else:
        method_name = "build_execution_config"
        preflight_name = "preflight_execution"

    builder = getattr(plugin, method_name, None)
    if builder is None or not callable(builder):
        raise RuntimeError(f"plugin does not implement {method_name}()")
    built = _invoke_builder(builder, public_config, secret_config)

    preflight = getattr(plugin, preflight_name, None)
    if preflight is None:
        preflight = getattr(built, "preflight", None)
    if preflight is not None and callable(preflight):
        parameters = inspect.signature(preflight).parameters
        if parameters:
            preflight(built)
        else:
            preflight()

    return {
        "ok": True,
        "plugin_id": descriptor.plugin_id,
        "version": descriptor.version,
        "capability": capability.value,
        "constructed_type": f"{type(built).__module__}.{type(built).__qualname__}",
        "preflight_performed": preflight is not None and callable(preflight),
    }


def _import_catalog(
    plugin: Any,
    descriptor: DescriptorSnapshot,
    request: dict[str, Any],
) -> dict[str, Any]:
    if Capability.HISTORICAL_IMPORT not in descriptor.capabilities:
        raise RuntimeError("plugin does not provide historical import")
    public_config = dict(request.get("public_config") or {})
    secret_config = {
        str(name): str(value) for name, value in dict(request.get("secret_config") or {}).items()
    }
    builder = getattr(plugin, "build_catalog_importer", None)
    if builder is None or not callable(builder):
        raise RuntimeError("plugin does not implement build_catalog_importer()")
    importer = _invoke_builder(builder, public_config, secret_config)
    source_shards = request.get("source_shards")
    if source_shards is not None:
        operation = getattr(importer, "import_sources", None)
        if operation is None or not callable(operation):
            raise RuntimeError("catalog importer does not implement import_sources()")
        result = operation(
            source_shards=list(source_shards),
            catalog_path=str(request["catalog_path"]),
            instrument_id=str(request["instrument_id"]),
            metadata=dict(request.get("metadata") or {}),
        )
    else:
        operation = getattr(importer, "import_source", None)
        operation_kind = "source"
        if operation is None:
            operation = getattr(importer, "import_parquet", None)
            operation_kind = "parquet"
        if operation is None and callable(importer):
            operation = importer
            operation_kind = "parquet"
        if operation is None or not callable(operation):
            raise RuntimeError(
                "catalog importer must be callable or expose import_source()/import_parquet()"
            )
        if operation_kind == "source":
            result = operation(
                source_url=str(request["source_url"]),
                catalog_path=str(request["catalog_path"]),
                instrument_id=str(request["instrument_id"]),
                metadata=dict(request.get("metadata") or {}),
            )
        else:
            result = operation(
                source_path=str(request["source_path"]),
                catalog_path=str(request["catalog_path"]),
                instrument_id=str(request["instrument_id"]),
                metadata=dict(request.get("metadata") or {}),
            )
    if result is None:
        result = {}
    if not isinstance(result, dict):
        raise RuntimeError("catalog importer must return a dictionary summary")
    return {
        "ok": True,
        "plugin_id": descriptor.plugin_id,
        "version": descriptor.version,
        "summary": result,
    }


def _scan_manifest(
    plugin: Any,
    descriptor: DescriptorSnapshot,
    request: dict[str, Any],
) -> dict[str, Any]:
    if Capability.HISTORICAL_IMPORT not in descriptor.capabilities:
        raise RuntimeError("plugin does not provide historical import")
    public_config = dict(request.get("public_config") or {})
    secret_config = {
        str(name): str(value) for name, value in dict(request.get("secret_config") or {}).items()
    }
    builder = getattr(plugin, "build_catalog_importer", None)
    if builder is None or not callable(builder):
        raise RuntimeError("plugin does not implement build_catalog_importer()")
    importer = _invoke_builder(builder, public_config, secret_config)
    operation = getattr(importer, "scan_manifest", None)
    if operation is None or not callable(operation):
        raise RuntimeError("catalog importer does not implement scan_manifest()")
    result = operation(metadata=dict(request.get("metadata") or {}))
    if not isinstance(result, dict):
        raise RuntimeError("manifest scanner must return a dictionary summary")
    return {
        "ok": True,
        "plugin_id": descriptor.plugin_id,
        "version": descriptor.version,
        "summary": result,
    }


def main() -> int:
    request = json.load(sys.stdin)
    plugin_id = str(request["plugin_id"])
    plugin, descriptor = _load_plugin(plugin_id)
    expected_version = request.get("plugin_version")
    if expected_version is not None and descriptor.version != str(expected_version):
        raise RuntimeError(
            f"plugin version mismatch: expected {expected_version!r}, got {descriptor.version!r}"
        )
    action = str(request.get("action", "preflight"))
    if action == "preflight":
        response = _preflight(plugin, descriptor, request)
    elif action == "import_catalog":
        response = _import_catalog(plugin, descriptor, request)
    elif action == "scan_manifest":
        response = _scan_manifest(plugin, descriptor, request)
    else:
        raise RuntimeError(f"unsupported runtime plugin action: {action}")
    print(json.dumps(response, separators=(",", ":"), default=str))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
