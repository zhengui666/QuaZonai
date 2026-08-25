from __future__ import annotations

from typing import Any

import httpx

from cli.client import ApiClient


def _json_response() -> httpx.Response:
    return httpx.Response(
        200,
        json={"ok": True},
        request=httpx.Request("GET", "http://127.0.0.1:8000/api/v1/readiness"),
    )


def test_cli_injects_machine_operator_token(
    monkeypatch: Any,
) -> None:
    observed: dict[str, Any] = {}

    def fake_request(method: str, url: str, **kwargs: Any) -> httpx.Response:
        observed.update(method=method, url=url, **kwargs)
        return _json_response()

    monkeypatch.setenv("QUAZONAI_API_TOKEN", "machine-token-value")
    monkeypatch.setattr(httpx, "request", fake_request)

    with ApiClient("http://127.0.0.1:8000") as client:
        assert client.request("GET", "/api/v1/readiness") == {"ok": True}

    assert observed["headers"]["Authorization"] == "Bearer machine-token-value"


def test_explicit_authorization_header_is_not_overwritten(
    monkeypatch: Any,
) -> None:
    observed: dict[str, Any] = {}

    def fake_request(method: str, url: str, **kwargs: Any) -> httpx.Response:
        observed.update(method=method, url=url, **kwargs)
        return _json_response()

    monkeypatch.setenv("QUAZONAI_API_TOKEN", "machine-token-value")
    monkeypatch.setattr(httpx, "request", fake_request)

    with ApiClient("http://127.0.0.1:8000") as client:
        client.request(
            "GET",
            "/api/v1/handoffs/00000000-0000-0000-0000-000000000001/package",
            headers={"Authorization": "Bearer downstream-service-token"},
        )

    assert observed["headers"]["Authorization"] == "Bearer downstream-service-token"


def test_blank_machine_token_does_not_create_authorization_header(
    monkeypatch: Any,
) -> None:
    observed: dict[str, Any] = {}

    def fake_request(method: str, url: str, **kwargs: Any) -> httpx.Response:
        observed.update(method=method, url=url, **kwargs)
        return _json_response()

    monkeypatch.setenv("QUAZONAI_API_TOKEN", "   ")
    monkeypatch.setattr(httpx, "request", fake_request)

    with ApiClient("http://127.0.0.1:8000") as client:
        client.request("GET", "/api/v1/readiness")

    assert "Authorization" not in observed["headers"]
