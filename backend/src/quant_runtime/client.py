"""Thin HTTP adapter for a separately deployed NautilusTrader gateway."""

from __future__ import annotations

import hmac
import os
from dataclasses import dataclass
from typing import Any, Protocol, TypeVar
from urllib.parse import urlparse

import httpx
from pydantic import BaseModel, ValidationError

from errors import QfError
from quant_runtime.contracts import (
    PINNED_NAUTILUS_VERSION,
    QUANT_RUNTIME_PROTOCOL_VERSION,
    BacktestEvidence,
    BacktestExperimentRequest,
    CandidateVerificationRequest,
    CandidateVerificationResult,
    CatalogIngestRequest,
    CatalogIngestResult,
    CatalogValidationRequest,
    CatalogValidationResult,
    ExperimentMode,
    RuntimeCapabilities,
    SealedBacktestResult,
)

ResponseModel = TypeVar("ResponseModel", bound=BaseModel)


class QuantRuntime(Protocol):
    def capabilities(self) -> RuntimeCapabilities: ...

    def ingest(self, request: CatalogIngestRequest) -> CatalogIngestResult: ...

    def validate_catalog(self, request: CatalogValidationRequest) -> CatalogValidationResult: ...

    def run_backtest(self, request: BacktestExperimentRequest) -> BacktestEvidence: ...

    def run_sealed_backtest(self, request: BacktestExperimentRequest) -> SealedBacktestResult: ...

    def verify_candidate(
        self, request: CandidateVerificationRequest
    ) -> CandidateVerificationResult: ...


@dataclass(frozen=True, slots=True)
class RemoteNautilusConfig:
    base_url: str
    token: str | None
    timeout_seconds: float = 120.0
    allow_insecure_http: bool = False
    expected_runtime_version: str = PINNED_NAUTILUS_VERSION

    @classmethod
    def from_env(cls, *, sealed: bool = False) -> RemoteNautilusConfig:
        prefix = "QUAZONAI_NAUTILUS_SEALED" if sealed else "QUAZONAI_NAUTILUS_RESEARCH"
        base_url = os.getenv(f"{prefix}_URL", "").strip()
        if not base_url:
            raise QfError(
                "NAUTILUS_RUNTIME_NOT_CONFIGURED",
                f"{prefix}_URL is required for remote Nautilus experiments.",
                503,
            )
        timeout_raw = os.getenv(f"{prefix}_TIMEOUT_SECONDS", "120")
        try:
            timeout = float(timeout_raw)
        except ValueError as exc:
            raise QfError(
                "NAUTILUS_RUNTIME_CONFIGURATION_INVALID",
                f"{prefix}_TIMEOUT_SECONDS must be numeric.",
                500,
            ) from exc
        return cls(
            base_url=base_url,
            token=os.getenv(f"{prefix}_TOKEN") or None,
            timeout_seconds=timeout,
            allow_insecure_http=os.getenv(f"{prefix}_ALLOW_INSECURE_HTTP", "false").lower()
            in {"1", "true", "yes"},
            expected_runtime_version=os.getenv(
                f"{prefix}_EXPECTED_VERSION", PINNED_NAUTILUS_VERSION
            ),
        )

    def validate(self) -> None:
        parsed = urlparse(self.base_url)
        if parsed.scheme not in {"http", "https"} or not parsed.netloc:
            raise QfError(
                "NAUTILUS_RUNTIME_CONFIGURATION_INVALID",
                "Remote Nautilus URL must be an absolute HTTP(S) URL.",
                500,
            )
        if parsed.username or parsed.password:
            raise QfError(
                "NAUTILUS_RUNTIME_CONFIGURATION_INVALID",
                "Credentials must not be embedded in the remote Nautilus URL.",
                500,
            )
        host = (parsed.hostname or "").lower()
        local_host = host in {"127.0.0.1", "localhost", "::1"}
        if parsed.scheme != "https" and not (self.allow_insecure_http or local_host):
            raise QfError(
                "NAUTILUS_RUNTIME_TLS_REQUIRED",
                "Remote Nautilus traffic must use HTTPS outside local tests.",
                500,
            )
        if self.timeout_seconds <= 0 or self.timeout_seconds > 1800:
            raise QfError(
                "NAUTILUS_RUNTIME_CONFIGURATION_INVALID",
                "Remote Nautilus timeout must be in (0, 1800] seconds.",
                500,
            )


class NautilusQuantRuntime:
    """Version-pinned adapter; it never imports or starts NautilusTrader locally."""

    def __init__(
        self,
        config: RemoteNautilusConfig,
        *,
        transport: httpx.BaseTransport | None = None,
    ) -> None:
        config.validate()
        self._config = config
        headers = {
            "Accept": "application/json",
            "User-Agent": "QuaZonai-QuantRuntime/1",
            "X-QuaZonai-Protocol": QUANT_RUNTIME_PROTOCOL_VERSION,
        }
        if config.token:
            headers["Authorization"] = f"Bearer {config.token}"
        self._client = httpx.Client(
            base_url=config.base_url.rstrip("/") + "/",
            headers=headers,
            timeout=httpx.Timeout(
                config.timeout_seconds, connect=min(config.timeout_seconds, 15.0)
            ),
            transport=transport,
            follow_redirects=False,
        )

    def __enter__(self) -> NautilusQuantRuntime:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    def close(self) -> None:
        self._client.close()

    def _parse(self, response: httpx.Response, model: type[ResponseModel]) -> ResponseModel:
        if response.status_code >= 400:
            request_id = response.headers.get("x-request-id")
            try:
                detail: Any = response.json()
            except ValueError:
                detail = response.text[-2000:]
            raise QfError(
                "NAUTILUS_RUNTIME_REQUEST_FAILED",
                "Remote Nautilus runtime rejected the request.",
                502 if response.status_code >= 500 else 422,
                {
                    "remote_status": response.status_code,
                    "remote_request_id": request_id,
                    "detail": detail,
                },
            )
        try:
            parsed = model.model_validate(response.json())
        except (ValueError, ValidationError) as exc:
            raise QfError(
                "NAUTILUS_RUNTIME_PROTOCOL_INVALID",
                "Remote Nautilus runtime returned an invalid protocol payload.",
                502,
            ) from exc
        self._assert_versions(parsed)
        return parsed

    def _assert_versions(self, payload: BaseModel) -> None:
        protocol = getattr(payload, "protocol_version", None)
        if not hmac.compare_digest(str(protocol), QUANT_RUNTIME_PROTOCOL_VERSION):
            raise QfError(
                "NAUTILUS_RUNTIME_PROTOCOL_MISMATCH",
                "Remote Nautilus protocol version is incompatible.",
                502,
                {"expected": QUANT_RUNTIME_PROTOCOL_VERSION, "received": protocol},
            )
        runtime_version = getattr(payload, "runtime_version", None)
        if runtime_version and not hmac.compare_digest(
            str(runtime_version), self._config.expected_runtime_version
        ):
            raise QfError(
                "NAUTILUS_RUNTIME_VERSION_MISMATCH",
                "Remote Nautilus version does not match the validated pin.",
                502,
                {
                    "expected": self._config.expected_runtime_version,
                    "received": runtime_version,
                },
            )

    def capabilities(self) -> RuntimeCapabilities:
        return self._parse(self._client.get("v1/capabilities"), RuntimeCapabilities)

    def ingest(self, request: CatalogIngestRequest) -> CatalogIngestResult:
        response = self._client.post(
            "v1/catalogs/ingest",
            json=request.model_dump(mode="json"),
            headers={"Idempotency-Key": str(request.request_id)},
        )
        return self._parse(response, CatalogIngestResult)

    def validate_catalog(self, request: CatalogValidationRequest) -> CatalogValidationResult:
        response = self._client.post(
            "v1/catalogs/validate",
            json=request.model_dump(mode="json"),
            headers={"Idempotency-Key": str(request.request_id)},
        )
        return self._parse(response, CatalogValidationResult)

    def run_backtest(self, request: BacktestExperimentRequest) -> BacktestEvidence:
        if request.mode == ExperimentMode.SEALED:
            raise QfError(
                "NAUTILUS_RUNTIME_MODE_INVALID",
                "Sealed requests must use run_sealed_backtest().",
                422,
            )
        response = self._client.post(
            "v1/backtests",
            json=request.model_dump(mode="json"),
            headers={"Idempotency-Key": str(request.experiment_id)},
        )
        return self._parse(response, BacktestEvidence)

    def run_sealed_backtest(self, request: BacktestExperimentRequest) -> SealedBacktestResult:
        if request.mode != ExperimentMode.SEALED:
            request = request.model_copy(update={"mode": ExperimentMode.SEALED})
        response = self._client.post(
            "v1/sealed-backtests",
            json=request.model_dump(mode="json"),
            headers={"Idempotency-Key": str(request.experiment_id)},
        )
        return self._parse(response, SealedBacktestResult)

    def verify_candidate(
        self, request: CandidateVerificationRequest
    ) -> CandidateVerificationResult:
        response = self._client.post(
            "v1/candidates/verify",
            json=request.model_dump(mode="json"),
            headers={"Idempotency-Key": str(request.candidate_id)},
        )
        return self._parse(response, CandidateVerificationResult)
