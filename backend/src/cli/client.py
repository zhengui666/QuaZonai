"""Loopback-only HTTP client used by the local CLI."""

from __future__ import annotations

import os
from types import TracebackType
from typing import Any
from urllib.parse import urlparse

import httpx

from settings import SettingsError, validate_machine_api_token


class CliClientError(RuntimeError):
    def __init__(self, message: str, *, status_code: int | None = None) -> None:
        super().__init__(message)
        self.status_code = status_code


def validate_loopback_endpoint(endpoint: str) -> str:
    parsed = urlparse(endpoint)
    if parsed.scheme not in {"http", "https"}:
        raise CliClientError("Core API endpoint must use http or https")
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise CliClientError("Core API endpoint cannot contain credentials, query, or fragment")
    if parsed.path not in {"", "/"}:
        raise CliClientError("Core API endpoint cannot contain a path")
    if parsed.hostname not in {"127.0.0.1", "localhost", "::1"}:
        raise CliClientError(
            "REMOTE_API_ENDPOINT_FORBIDDEN: Core API endpoint must resolve to the local loopback host"
        )
    return endpoint.rstrip("/")


def _machine_token(explicit: str | None) -> str | None:
    configured = explicit if explicit is not None else os.environ.get("QUAZONAI_API_TOKEN")
    if configured is None or configured == "":
        return None
    try:
        validate_machine_api_token(configured)
    except SettingsError as exc:
        raise CliClientError(str(exc)) from exc
    return configured


class ApiClient:
    def __init__(
        self,
        endpoint: str,
        *,
        timeout: float = 30.0,
        api_token: str | None = None,
    ) -> None:
        self.endpoint = validate_loopback_endpoint(endpoint)
        self.timeout = timeout
        self.api_token = _machine_token(api_token)

    def __enter__(self) -> "ApiClient":
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_value: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        return None

    def request(
        self,
        method: str,
        path: str,
        *,
        json_body: Any | None = None,
        params: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
    ) -> Any:
        if not path.startswith("/api/v1/"):
            raise CliClientError("CLI requests must target a fixed /api/v1 operation")
        request_headers = dict(headers or {})
        if self.api_token is not None:
            request_headers.setdefault("Authorization", f"Bearer {self.api_token}")
        try:
            response = httpx.request(
                method,
                f"{self.endpoint}{path}",
                json=json_body,
                params=params,
                headers=request_headers,
                timeout=self.timeout,
            )
        except httpx.HTTPError as exc:
            raise CliClientError(str(exc)) from exc
        if response.status_code >= 400:
            try:
                payload = response.json()
                error = payload.get("error") or {}
                code = error.get("code") or f"HTTP_{response.status_code}"
                message = error.get("message") or response.text
                raise CliClientError(
                    f"{code}: {message}",
                    status_code=response.status_code,
                )
            except (ValueError, AttributeError) as exc:
                raise CliClientError(
                    f"HTTP_{response.status_code}: {response.text}",
                    status_code=response.status_code,
                ) from exc
        if not response.content:
            return None
        try:
            return response.json()
        except ValueError:
            return response.text
