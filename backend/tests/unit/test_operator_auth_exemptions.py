from __future__ import annotations

from operator_auth import is_operator_auth_exempt


def test_only_explicit_auth_and_health_method_paths_are_public() -> None:
    public_routes = (
        ("GET", "/api/v1/system/health"),
        ("POST", "/api/v1/auth/login"),
        ("GET", "/api/v1/auth/session"),
        ("POST", "/api/v1/auth/logout"),
    )
    for method, path in public_routes:
        assert is_operator_auth_exempt(method, path) is True

    for method, path in (
        ("POST", "/api/v1/system/health"),
        ("GET", "/api/v1/auth/login"),
        ("POST", "/api/v1/auth/session"),
        ("GET", "/api/v1/auth/logout"),
        ("POST", "/api/v1/auth/anything-else"),
        ("POST", "/api/v1/auth/login/"),
        ("GET", "/api/v1/system/runtime-configuration"),
        ("GET", "/api/v1/readiness"),
        ("GET", "/api/v1/openapi.json"),
    ):
        assert is_operator_auth_exempt(method, path) is False


def test_only_exact_downstream_owned_handoff_method_paths_bypass_operator_auth() -> None:
    handoff_id = "00000000-0000-0000-0000-000000000001"
    operations = {
        "claim": "POST",
        "accept": "POST",
        "reject": "POST",
        "package": "GET",
        "feedback": "POST",
    }
    for operation, method in operations.items():
        path = f"/api/v1/handoffs/{handoff_id}/{operation}"
        assert is_operator_auth_exempt(method, path) is True
        wrong_method = "POST" if method == "GET" else "GET"
        assert is_operator_auth_exempt(wrong_method, path) is False

    for path in (
        f"/api/v1/handoffs/{handoff_id}",
        f"/api/v1/handoffs/{handoff_id}/revoke",
        f"/api/v1/handoffs/{handoff_id}/package/extra",
        f"/api/v1/handoffs/{handoff_id}/feedback/",
        "/api/v1/handoffs//claim",
        "/api/v1/handoffs/not-an-id/unknown",
    ):
        assert is_operator_auth_exempt("POST", path) is False
