from __future__ import annotations

from operator_auth import is_operator_auth_exempt


def test_only_explicit_auth_and_health_routes_are_public() -> None:
    for path in (
        "/api/v1/system/health",
        "/api/v1/auth/login",
        "/api/v1/auth/session",
        "/api/v1/auth/logout",
    ):
        assert is_operator_auth_exempt(path) is True

    for path in (
        "/api/v1/auth/anything-else",
        "/api/v1/auth/login/",
        "/api/v1/system/runtime-configuration",
        "/api/v1/readiness",
        "/api/v1/openapi.json",
    ):
        assert is_operator_auth_exempt(path) is False


def test_only_exact_downstream_owned_handoff_operations_bypass_operator_auth() -> None:
    handoff_id = "00000000-0000-0000-0000-000000000001"
    for operation in ("claim", "accept", "reject", "package", "feedback"):
        assert is_operator_auth_exempt(f"/api/v1/handoffs/{handoff_id}/{operation}") is True

    for path in (
        f"/api/v1/handoffs/{handoff_id}",
        f"/api/v1/handoffs/{handoff_id}/revoke",
        f"/api/v1/handoffs/{handoff_id}/package/extra",
        f"/api/v1/handoffs/{handoff_id}/feedback/",
        "/api/v1/handoffs//claim",
        "/api/v1/handoffs/not-an-id/unknown",
    ):
        assert is_operator_auth_exempt(path) is False
