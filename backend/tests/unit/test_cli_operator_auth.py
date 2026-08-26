from __future__ import annotations

from typing import Any

import httpx
import pytest

from cli.client import ApiClient, CliClientError


MACHINE_TOKEN = "machine-token-" + "x" * 32


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

    monkeypatch.setenv("QUAZONAI_API_TOKEN", MACHINE_TOKEN)
    monkeypatch.setattr(httpx, "request", fake_request)

    with ApiClient("http://127.0.0.1:8000") as client:
        assert client.request("GET", "/api/v1/readiness") == {"ok": True}

    assert observed["headers"]["Authorization"] == f"Bearer {MACHINE_TOKEN}"


def test_explicit_authorization_header_is_not_overwritten(
    monkeypatch: Any,
) -> None:
    observed: dict[str, Any] = {}

    def fake_request(method: str, url: str, **kwargs: Any) -> httpx.Response:
        observed.update(method=method, url=url, **kwargs)
        return _json_response()

    monkeypatch.setenv("QUAZONAI_API_TOKEN", MACHINE_TOKEN)
    monkeypatch.setattr(httpx, "request", fake_request)

    with ApiClient("http://127.0.0.1:8000") as client:
        client.request(
            "GET",
            "/api/v1/handoffs/00000000-0000-0000-0000-000000000001/package",
            headers={"Authorization": "Bearer downstream-service-token"},
        )

    assert observed["headers"]["Authorization"] == "Bearer downstream-service-token"


def test_lowercase_explicit_authorization_header_is_not_overwritten(
    monkeypatch: Any,
) -> None:
    observed: dict[str, Any] = {}

    def fake_request(method: str, url: str, **kwargs: Any) -> httpx.Response:
        observed.update(method=method, url=url, **kwargs)
        return _json_response()

    monkeypatch.setenv("QUAZONAI_API_TOKEN", MACHINE_TOKEN)
    monkeypatch.setattr(httpx, "request", fake_request)

    with ApiClient("http://127.0.0.1:8000") as client:
        client.request(
            "GET",
            "/api/v1/handoffs/00000000-0000-0000-0000-000000000001/package",
            headers={"authorization": "Bearer downstream-service-token"},
        )

    assert observed["headers"] == {"authorization": "Bearer downstream-service-token"}


def test_whitespace_machine_token_is_rejected_without_sending_a_request(
    monkeypatch: Any,
) -> None:
    monkeypatch.setenv("QUAZONAI_API_TOKEN", "   ")

    with pytest.raises(CliClientError, match="QUAZONAI_API_TOKEN"):
        ApiClient("http://127.0.0.1:8000")
