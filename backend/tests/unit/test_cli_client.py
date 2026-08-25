from __future__ import annotations

from typing import Any

import httpx
import pytest

from cli.client import ApiClient, CliClientError, validate_loopback_endpoint


def test_loopback_endpoint_is_accepted() -> None:
    assert validate_loopback_endpoint("http://127.0.0.1:8000/") == "http://127.0.0.1:8000"


def test_remote_endpoint_is_rejected() -> None:
    with pytest.raises(CliClientError, match="REMOTE_API_ENDPOINT_FORBIDDEN"):
        validate_loopback_endpoint("https://quazonai.example.com")


def test_api_token_from_environment_is_sent_as_bearer(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    captured: dict[str, Any] = {}
    token = "machine-token-value-" + "x" * 32

    def fake_request(method: str, url: str, **kwargs: Any) -> httpx.Response:
        captured.update(method=method, url=url, **kwargs)
        return httpx.Response(200, json={"ready": True})

    monkeypatch.setenv("QUAZONAI_API_TOKEN", token)
    monkeypatch.setattr(httpx, "request", fake_request)

    result = ApiClient("http://127.0.0.1:8000").request(
        "GET",
        "/api/v1/readiness",
    )

    assert result == {"ready": True}
    assert captured["headers"] == {"Authorization": f"Bearer {token}"}


@pytest.mark.parametrize(
    "token",
    [
        " " + "x" * 32,
        "x" * 32 + " ",
        "x" * 32 + "\n",
        "x" * 32 + "\r",
        "x" * 31 + "é",
    ],
)
def test_malformed_machine_token_is_rejected_without_normalization(
    monkeypatch: pytest.MonkeyPatch,
    token: str,
) -> None:
    monkeypatch.setenv("QUAZONAI_API_TOKEN", token)

    with pytest.raises(CliClientError, match="QUAZONAI_API_TOKEN"):
        ApiClient("http://127.0.0.1:8000")


def test_empty_machine_token_is_treated_as_unconfigured(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    captured: dict[str, Any] = {}

    def fake_request(method: str, url: str, **kwargs: Any) -> httpx.Response:
        captured.update(method=method, url=url, **kwargs)
        return httpx.Response(200, json={"ok": True})

    monkeypatch.setenv("QUAZONAI_API_TOKEN", "")
    monkeypatch.setattr(httpx, "request", fake_request)

    ApiClient("http://127.0.0.1:8000").request(
        "GET",
        "/api/v1/system/health",
    )

    assert captured["headers"] == {}


def test_explicit_authorization_header_is_not_overwritten(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    captured: dict[str, Any] = {}

    def fake_request(method: str, url: str, **kwargs: Any) -> httpx.Response:
        captured.update(method=method, url=url, **kwargs)
        return httpx.Response(200, json={"ok": True})

    monkeypatch.setattr(httpx, "request", fake_request)

    ApiClient(
        "http://127.0.0.1:8000",
        api_token="operator-machine-token-" + "x" * 32,
    ).request(
        "GET",
        "/api/v1/handoffs/offer-1/package",
        headers={"Authorization": "Bearer downstream-service-token"},
    )

    assert captured["headers"] == {
        "Authorization": "Bearer downstream-service-token",
    }
