"""Thin HTTP adapter for a remote NautilusTrader canonical runtime."""

from __future__ import annotations

from pathlib import Path
from typing import Any, Protocol

import httpx

from errors import QfError
from quant_runtime.config import RemoteNautilusConfig
from quant_runtime.contracts import (
    ArchiveManifestDescriptor,
    ArchiveManifestSpec,
    CatalogDescriptor,
    CatalogIngestSpec,
    RuntimeCapabilities,
)


class QuantRuntime(Protocol):
    def capabilities(self) -> RuntimeCapabilities: ...

    def ingest(
        self,
        spec: CatalogIngestSpec,
        *,
        timeout_seconds: float | None = None,
    ) -> CatalogDescriptor: ...

    def inspect_archive_manifest(self, spec: ArchiveManifestSpec) -> ArchiveManifestDescriptor: ...

    def validate_catalog(self, catalog_uri: str) -> CatalogDescriptor: ...

    def verify_candidate(self, bundle_path: Path) -> dict[str, Any]: ...


class NautilusQuantRuntime:
    """Nautilus-first adapter without importing NautilusTrader into QuaZonai Core."""

    def __init__(self, config: RemoteNautilusConfig):
        self.config = config

    def _headers(self) -> dict[str, str]:
        headers = {
            "Accept": "application/json",
            "X-QuaZonai-Quant-Contract": self.config.contract_version,
        }
        if self.config.service_token:
            headers["Authorization"] = f"Bearer {self.config.service_token}"
        return headers

    def _request_json(
        self,
        method: str,
        path: str,
        *,
        json_body: dict[str, Any] | None = None,
        files: dict[str, tuple[str, bytes, str]] | None = None,
        timeout_seconds: float | None = None,
    ) -> dict[str, Any]:
        url = f"{self.config.base_url}{path}"
        try:
            with httpx.Client(
                timeout=timeout_seconds or self.config.timeout_seconds,
                verify=self.config.verify_tls,
                headers=self._headers(),
            ) as client:
                response = client.request(method, url, json=json_body, files=files)
        except httpx.HTTPError as exc:
            raise QfError(
                "NAUTILUS_RUNTIME_UNAVAILABLE",
                "The remote NautilusTrader runtime could not be reached.",
                503,
                {"error_type": type(exc).__name__},
            ) from exc

        if response.status_code >= 400:
            detail: object
            try:
                detail = response.json()
            except ValueError:
                detail = response.text[-2000:]
            raise QfError(
                "NAUTILUS_RUNTIME_REJECTED",
                "The remote NautilusTrader runtime rejected the request.",
                502,
                {"status_code": response.status_code, "detail": detail},
            )
        try:
            payload = response.json()
        except ValueError as exc:
            raise QfError(
                "NAUTILUS_RUNTIME_PROTOCOL_ERROR",
                "The remote NautilusTrader runtime returned invalid JSON.",
                502,
            ) from exc
        if not isinstance(payload, dict):
            raise QfError(
                "NAUTILUS_RUNTIME_PROTOCOL_ERROR",
                "The remote NautilusTrader runtime returned an invalid object.",
                502,
            )
        return payload

    def _assert_capabilities(self, capabilities: RuntimeCapabilities) -> None:
        if capabilities.contract_version != self.config.contract_version:
            raise QfError(
                "NAUTILUS_RUNTIME_CONTRACT_MISMATCH",
                "The remote quant-runtime contract version is incompatible.",
                503,
                {
                    "expected": self.config.contract_version,
                    "actual": capabilities.contract_version,
                },
            )
        if capabilities.nautilus_version != self.config.pinned_version:
            raise QfError(
                "NAUTILUS_RUNTIME_VERSION_MISMATCH",
                "The remote NautilusTrader version does not match the pinned version.",
                503,
                {
                    "expected": self.config.pinned_version,
                    "actual": capabilities.nautilus_version,
                },
            )
        if capabilities.runtime_name != "NautilusTrader":
            raise QfError(
                "NAUTILUS_RUNTIME_IDENTITY_MISMATCH",
                "The configured remote service is not a NautilusTrader runtime.",
                503,
            )
        if capabilities.candidate_contract_version != "1":
            raise QfError(
                "NAUTILUS_RUNTIME_CANDIDATE_CONTRACT_MISMATCH",
                "The remote runtime does not support the Candidate Bundle contract.",
                503,
                {"expected": "1", "actual": capabilities.candidate_contract_version},
            )

    def capabilities(self) -> RuntimeCapabilities:
        capabilities = RuntimeCapabilities.model_validate(
            self._request_json("GET", "/v1/capabilities")
        )
        self._assert_capabilities(capabilities)
        return capabilities

    def ingest(
        self,
        spec: CatalogIngestSpec,
        *,
        timeout_seconds: float | None = None,
    ) -> CatalogDescriptor:
        self.capabilities()
        descriptor = CatalogDescriptor.model_validate(
            self._request_json(
                "POST",
                "/v1/catalogs/ingest",
                json_body=spec.model_dump(mode="json"),
                timeout_seconds=timeout_seconds,
            )
        )
        expected_uri = f"catalog://{spec.catalog_name}"
        mismatches: list[str] = []
        if descriptor.catalog_uri != expected_uri:
            mismatches.append("catalog_uri")
        if descriptor.provider != spec.provider:
            mismatches.append("provider")
        if descriptor.source_license != spec.source_license:
            mismatches.append("source_license")
        if descriptor.source_spec != spec.source_spec:
            mismatches.append("source_spec")
        if descriptor.sealed != spec.sealed:
            mismatches.append("sealed")
        if mismatches:
            raise QfError(
                "NAUTILUS_RUNTIME_DESCRIPTOR_MISMATCH",
                "The remote catalog descriptor does not match the requested dataset.",
                502,
                {"fields": mismatches},
            )
        return descriptor

    def validate_catalog(self, catalog_uri: str) -> CatalogDescriptor:
        self.capabilities()
        return CatalogDescriptor.model_validate(
            self._request_json(
                "POST",
                "/v1/catalogs/validate",
                json_body={"catalog_uri": catalog_uri},
            )
        )

    def inspect_archive_manifest(self, spec: ArchiveManifestSpec) -> ArchiveManifestDescriptor:
        self.capabilities()
        descriptor = ArchiveManifestDescriptor.model_validate(
            self._request_json(
                "POST",
                "/v1/archive-manifests/inspect",
                json_body=spec.model_dump(mode="json"),
                # The runtime bounds a maximum one-year scan at 900 seconds;
                # the client must not abandon a completed scan before that
                # contract deadline.
                timeout_seconds=max(self.config.timeout_seconds, 900.0),
            )
        )
        expected_uri = f"manifest://{spec.manifest_name}"
        mismatches: list[str] = []
        if descriptor.manifest_uri != expected_uri:
            mismatches.append("manifest_uri")
        if descriptor.provider != spec.provider:
            mismatches.append("provider")
        if descriptor.source_license != spec.source_license:
            mismatches.append("source_license")
        if descriptor.source_spec != spec.source_spec:
            mismatches.append("source_spec")
        if mismatches:
            raise QfError(
                "NAUTILUS_RUNTIME_MANIFEST_MISMATCH",
                "The remote archive manifest does not match the requested source.",
                502,
                {"fields": mismatches},
            )
        return descriptor

    def verify_candidate(self, bundle_path: Path) -> dict[str, Any]:
        self.capabilities()
        payload = self._request_json(
            "POST",
            "/v1/candidates/verify",
            files={
                "bundle": (
                    bundle_path.name,
                    bundle_path.read_bytes(),
                    "application/zip",
                )
            },
        )
        if payload.get("valid") is not True:
            raise QfError(
                "CANDIDATE_PACKAGE_CONFORMANCE_FAILED",
                "The Nautilus runtime rejected the Candidate Bundle.",
                422,
                {"detail": payload},
            )
        return payload
