"""Thin HTTP adapter for a remote NautilusTrader canonical runtime."""

from __future__ import annotations

import time
from pathlib import Path
from typing import Any, Protocol

import httpx

from errors import QfError
from quant_runtime.config import RemoteNautilusConfig
from quant_runtime.contracts import (
    CatalogDescriptor,
    CatalogIngestSpec,
    ExperimentSpec,
    RunMode,
    RunEvidence,
    RuntimeCapabilities,
)


class QuantRuntime(Protocol):
    def capabilities(self) -> RuntimeCapabilities: ...

    def ingest(self, spec: CatalogIngestSpec) -> CatalogDescriptor: ...

    def validate_catalog(self, catalog_uri: str) -> CatalogDescriptor: ...

    def run_backtest(self, experiment: ExperimentSpec) -> RunEvidence: ...

    def run_sealed_backtest(self, experiment: ExperimentSpec) -> RunEvidence: ...

    def run_portfolio_backtest(self, experiment: ExperimentSpec) -> RunEvidence: ...

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
    ) -> dict[str, Any]:
        url = f"{self.config.base_url}{path}"
        try:
            with httpx.Client(
                timeout=self.config.timeout_seconds,
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

    def ingest(self, spec: CatalogIngestSpec) -> CatalogDescriptor:
        self.capabilities()
        descriptor = CatalogDescriptor.model_validate(
            self._request_json(
                "POST",
                "/v1/catalogs/ingest",
                json_body=spec.model_dump(mode="json"),
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

    def _run(self, experiment: ExperimentSpec, mode: RunMode) -> RunEvidence:
        self.capabilities()
        payload = self._request_json(
            "POST",
            "/v1/runs",
            json_body={
                "mode": mode,
                "experiment": experiment.model_dump(mode="json"),
            },
        )
        deadline = time.monotonic() + self.config.timeout_seconds
        while payload.get("state") in {"PENDING", "RUNNING"}:
            run_id = payload.get("external_run_id")
            if not isinstance(run_id, str) or not run_id:
                raise QfError(
                    "NAUTILUS_RUNTIME_PROTOCOL_ERROR",
                    "An asynchronous run did not return an external_run_id.",
                    502,
                )
            if time.monotonic() >= deadline:
                raise QfError(
                    "NAUTILUS_RUNTIME_TIMEOUT",
                    "The remote NautilusTrader run exceeded its configured time limit.",
                    504,
                    {"external_run_id": run_id},
                )
            time.sleep(self.config.poll_seconds)
            payload = self._request_json("GET", f"/v1/runs/{run_id}")

        evidence = RunEvidence.model_validate(payload)
        expected_artifact = experiment.strategy.model_dump(mode="json")
        if evidence.runtime_name != "NautilusTrader":
            raise QfError(
                "NAUTILUS_RUNTIME_IDENTITY_MISMATCH",
                "Run evidence was produced by an unexpected runtime.",
                502,
            )
        if evidence.mode != mode:
            raise QfError(
                "NAUTILUS_RUNTIME_MODE_MISMATCH",
                "Run evidence mode does not match the requested run.",
                502,
                {"expected": mode, "actual": evidence.mode},
            )
        if evidence.catalog_uri != experiment.catalog_uri:
            raise QfError(
                "NAUTILUS_RUNTIME_CATALOG_MISMATCH",
                "Run evidence references a different catalog than requested.",
                502,
            )
        if evidence.strategy_artifact != expected_artifact:
            raise QfError(
                "NAUTILUS_RUNTIME_STRATEGY_MISMATCH",
                "Run evidence references a different strategy artifact than requested.",
                502,
            )
        if evidence.nautilus_version != self.config.pinned_version:
            raise QfError(
                "NAUTILUS_RUNTIME_VERSION_MISMATCH",
                "Run evidence was produced by an unexpected NautilusTrader version.",
                502,
                {
                    "expected": self.config.pinned_version,
                    "actual": evidence.nautilus_version,
                },
            )
        if evidence.contract_version != self.config.contract_version:
            raise QfError(
                "NAUTILUS_RUNTIME_CONTRACT_MISMATCH",
                "Run evidence uses an incompatible quant-runtime contract.",
                502,
            )
        return evidence

    def run_backtest(self, experiment: ExperimentSpec) -> RunEvidence:
        return self._run(experiment, "DISCOVERY")

    def run_sealed_backtest(self, experiment: ExperimentSpec) -> RunEvidence:
        return self._run(experiment, "SEALED")

    def run_portfolio_backtest(self, experiment: ExperimentSpec) -> RunEvidence:
        return self._run(experiment, "PORTFOLIO")

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
