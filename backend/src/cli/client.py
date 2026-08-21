"""Loopback-only HTTP client for the local human CLI."""

from __future__ import annotations

import ipaddress
import socket
from pathlib import Path
from typing import Any, BinaryIO
from urllib.parse import urlparse

import httpx


class CliClientError(RuntimeError):
    def __init__(self, message: str, *, status_code: int | None = None) -> None:
        super().__init__(message)
        self.status_code = status_code


def validate_loopback_endpoint(endpoint: str) -> str:
    parsed = urlparse(endpoint)
    if parsed.scheme != "http" or not parsed.hostname:
        raise CliClientError("Core endpoint must be an absolute http:// loopback URL")
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise CliClientError("Core endpoint must not contain credentials, query, or fragment")
    if parsed.path not in {"", "/"}:
        raise CliClientError("Core endpoint must not contain a path")
    try:
        addresses = {
            ipaddress.ip_address(item[4][0])
            for item in socket.getaddrinfo(parsed.hostname, parsed.port or 80)
        }
    except socket.gaierror as exc:
        raise CliClientError("Core endpoint hostname cannot be resolved") from exc
    if not addresses or any(not address.is_loopback for address in addresses):
        raise CliClientError("REMOTE_API_ENDPOINT_FORBIDDEN")
    return endpoint.rstrip("/")


class ApiClient:
    def __init__(self, endpoint: str, *, timeout: float = 30.0) -> None:
        self.endpoint = validate_loopback_endpoint(endpoint)
        self.client = httpx.Client(base_url=self.endpoint, timeout=timeout)

    def close(self) -> None:
        self.client.close()

    def __enter__(self) -> ApiClient:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    def request(
        self,
        method: str,
        path: str,
        *,
        json_body: dict[str, Any] | None = None,
        params: dict[str, Any] | None = None,
        data: dict[str, str] | None = None,
        files: list[tuple[str, tuple[str, BinaryIO, str]]] | None = None,
    ) -> Any:
        try:
            response = self.client.request(
                method,
                path,
                json=json_body,
                params=params,
                data=data,
                files=files,
            )
        except httpx.HTTPError as exc:
            raise CliClientError(f"Core request failed: {exc}") from exc
        if response.status_code >= 400:
            try:
                payload = response.json()
                error = payload.get("error", {}) if isinstance(payload, dict) else {}
                code = error.get("code", "REQUEST_FAILED")
                message = error.get("message", response.text)
                details = error.get("details") or {}
                suffix = f" details={details}" if details else ""
                raise CliClientError(
                    f"{code}: {message}{suffix}",
                    status_code=response.status_code,
                )
            except ValueError as exc:
                raise CliClientError(
                    f"HTTP {response.status_code}: {response.text}",
                    status_code=response.status_code,
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
            files.append(
                ("primary", (primary.name, primary_handle, "application/octet-stream"))
            )
            for dependency in dependencies:
                handle = dependency.open("rb")
                opened.append(handle)
                files.append(
                    ("dependencies", (dependency.name, handle, "application/octet-stream"))
                )
            return self.request("POST", "/api/v1/plugin-releases", files=files)
        finally:
            for handle in opened:
                handle.close()

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
