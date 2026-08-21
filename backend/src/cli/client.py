"""Loopback-only HTTP client used by the local CLI."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, BinaryIO
from urllib.parse import urlparse

import httpx


class CliClientError(RuntimeError):
    pass


def validate_loopback_endpoint(endpoint: str) -> str:
    parsed = urlparse(endpoint)
    if parsed.scheme not in {"http", "https"}:
        raise CliClientError("Core API endpoint must use http or https")
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise CliClientError("Core API endpoint cannot contain credentials, query, or fragment")
    if parsed.path not in {"", "/"}:
        raise CliClientError("Core API endpoint cannot contain a path")
    if parsed.hostname not in {"127.0.0.1", "localhost", "::1"}:
        raise CliClientError("Core API endpoint must resolve to the local loopback host")
    return endpoint.rstrip("/")


class ApiClient:
    def __init__(self, endpoint: str, *, timeout: float = 30.0) -> None:
        self.endpoint = validate_loopback_endpoint(endpoint)
        self.timeout = timeout

    def request(
        self,
        method: str,
        path: str,
        *,
        json_body: Any | None = None,
        data: dict[str, str] | None = None,
        files: list[tuple[str, tuple[str, BinaryIO, str]]] | None = None,
        params: dict[str, Any] | None = None,
    ) -> Any:
        if not path.startswith("/api/v1/"):
            raise CliClientError("CLI requests must target a fixed /api/v1 operation")
        try:
            response = httpx.request(
                method,
                f"{self.endpoint}{path}",
                json=json_body,
                data=data,
                files=files,
                params=params,
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
                raise CliClientError(f"{code}: {message}")
            except (ValueError, AttributeError) as exc:
                raise CliClientError(
                    f"HTTP_{response.status_code}: {response.text}",
                ) from exc
        if not response.content:
            return None
        try:
            return response.json()
        except ValueError:
            return response.text

    def upload_plugin(self, primary: Path, dependencies: list[Path]) -> Any:
        opened: list[BinaryIO] = []
        try:
            files: list[tuple[str, tuple[str, BinaryIO, str]]] = []
            primary_handle = primary.open("rb")
            opened.append(primary_handle)
            files.append(("primary", (primary.name, primary_handle, "application/octet-stream")))
            for dependency in dependencies:
                dependency_handle = dependency.open("rb")
                opened.append(dependency_handle)
                files.append(
                    (
                        "dependencies",
                        (dependency.name, dependency_handle, "application/octet-stream"),
                    )
                )
            return self.request("POST", "/api/v1/plugin-releases", files=files)
        finally:
            for opened_handle in opened:
                opened_handle.close()

    def upload_strategy(
        self,
        strategy_id: str,
        source: Path,
        default_config_json: str,
    ) -> Any:
        with source.open("rb") as handle:
            return self.request(
                "POST",
                f"/api/v1/strategies/{strategy_id}/versions",
                data={"default_config_json": default_config_json},
                files=[("file", (source.name, handle, "text/x-python"))],
            )

    def upload_dataset(
        self,
        source_id: str,
        parquet: Path,
        *,
        instrument_id: str,
        source_label: str,
        metadata_json: str,
    ) -> Any:
        with parquet.open("rb") as handle:
            return self.request(
                "POST",
                f"/api/v1/data-sources/{source_id}/imports/parquet-l2",
                data={
                    "instrument_id": instrument_id,
                    "source_label": source_label,
                    "metadata_json": metadata_json,
                },
                files=[("file", (parquet.name, handle, "application/octet-stream"))],
            )
