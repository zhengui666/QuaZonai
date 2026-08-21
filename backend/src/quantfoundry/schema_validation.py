"""Minimal validation for the JSON Schemas persisted in plugin descriptor snapshots."""

from __future__ import annotations

from typing import Any

from quantfoundry.errors import QfError

_TYPE_CHECKS: dict[str, type[Any] | tuple[type[Any], ...]] = {
    "object": dict,
    "array": list,
    "string": str,
    "integer": int,
    "number": (int, float),
    "boolean": bool,
    "null": type(None),
}


def _fail(path: str, message: str) -> None:
    raise QfError(
        "PLUGIN_CONFIG_INVALID",
        "Plugin configuration does not match its validated schema.",
        422,
        {"path": path, "reason": message},
    )


def validate_schema_value(value: Any, schema: dict[str, Any], *, path: str = "$") -> None:
    expected = schema.get("type")
    if isinstance(expected, list):
        allowed = tuple(_TYPE_CHECKS[item] for item in expected if item in _TYPE_CHECKS)
        if allowed and not isinstance(value, allowed):
            _fail(path, f"expected one of {expected}")
    elif isinstance(expected, str) and expected in _TYPE_CHECKS:
        type_check = _TYPE_CHECKS[expected]
        if not isinstance(value, type_check) or (
            expected in {"integer", "number"} and isinstance(value, bool)
        ):
            _fail(path, f"expected {expected}")

    enum = schema.get("enum")
    if isinstance(enum, list) and value not in enum:
        _fail(path, "value is not in the allowed enum")

    if isinstance(value, dict):
        properties = schema.get("properties", {})
        required = schema.get("required", [])
        if isinstance(required, list):
            for name in required:
                if name not in value:
                    _fail(f"{path}.{name}", "required field is missing")
        if schema.get("additionalProperties") is False and isinstance(properties, dict):
            extras = sorted(set(value) - set(properties))
            if extras:
                _fail(path, f"unexpected fields: {', '.join(extras)}")
        if isinstance(properties, dict):
            for name, child_schema in properties.items():
                if name in value and isinstance(child_schema, dict):
                    validate_schema_value(value[name], child_schema, path=f"{path}.{name}")

    if isinstance(value, list):
        minimum = schema.get("minItems")
        maximum = schema.get("maxItems")
        if isinstance(minimum, int) and len(value) < minimum:
            _fail(path, f"requires at least {minimum} items")
        if isinstance(maximum, int) and len(value) > maximum:
            _fail(path, f"allows at most {maximum} items")
        item_schema = schema.get("items")
        if isinstance(item_schema, dict):
            for index, item in enumerate(value):
                validate_schema_value(item, item_schema, path=f"{path}[{index}]")

    if isinstance(value, str):
        minimum = schema.get("minLength")
        maximum = schema.get("maxLength")
        if isinstance(minimum, int) and len(value) < minimum:
            _fail(path, f"requires at least {minimum} characters")
        if isinstance(maximum, int) and len(value) > maximum:
            _fail(path, f"allows at most {maximum} characters")
