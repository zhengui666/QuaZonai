from __future__ import annotations

from typing import Any

from main import create_app


def _resolve(schema: dict[str, Any], document: dict[str, Any]) -> dict[str, Any]:
    reference = schema.get("$ref")
    if not isinstance(reference, str):
        return schema
    name = reference.rsplit("/", 1)[-1]
    resolved = document["components"]["schemas"][name]
    assert isinstance(resolved, dict)
    return resolved


def test_native_mobile_login_openapi_is_totp_only(settings, engine) -> None:
    document = create_app(settings=settings, engine=engine).openapi()
    operation = document["paths"]["/api/v1/auth/mobile/login"]["post"]
    body = operation["requestBody"]["content"]["application/json"]["schema"]
    schema = _resolve(body, document)
    properties = schema["properties"]

    assert "totp_code" in properties
    assert "totp_code" in schema["required"]
    assert "username" not in properties
    assert "password" not in properties
    assert schema.get("additionalProperties") is False


def test_native_auth_and_bootstrap_operations_are_exposed(settings, engine) -> None:
    document = create_app(settings=settings, engine=engine).openapi()
    required = {
        ("/api/v1/client/bootstrap", "get"),
        ("/api/v1/auth/mobile/login", "post"),
        ("/api/v1/auth/mobile/refresh", "post"),
        ("/api/v1/auth/mobile/logout", "post"),
        ("/api/v1/auth/mobile/session", "get"),
        ("/api/v1/auth/mobile/devices", "get"),
        ("/api/v1/auth/mobile/devices/{device_id}/revoke", "post"),
    }
    for path, method in required:
        operation = document["paths"][path][method]
        assert operation["operationId"]


def test_all_openapi_operation_ids_are_unique(settings, engine) -> None:
    document = create_app(settings=settings, engine=engine).openapi()
    operation_ids = [
        operation["operationId"]
        for methods in document["paths"].values()
        for method, operation in methods.items()
        if method in {"get", "post", "put", "patch", "delete"}
    ]
    assert len(operation_ids) == len(set(operation_ids))
