from __future__ import annotations

from main import create_app


def test_native_device_revoke_uses_explicit_action_route(settings, engine) -> None:
    document = create_app(settings=settings, engine=engine).openapi()
    operation = document["paths"]["/api/v1/auth/mobile/devices/{device_id}/revoke"]["post"]
    assert operation["operationId"]


def test_native_device_revoke_does_not_overload_resource_collection(settings, engine) -> None:
    document = create_app(settings=settings, engine=engine).openapi()
    assert "post" not in document["paths"].get("/api/v1/auth/mobile/devices/{device_id}", {})
