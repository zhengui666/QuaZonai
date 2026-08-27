"""Typed QuaZonai boundary for a remote NautilusTrader quant runtime.

The QuaZonai control plane never imports NautilusTrader and never receives broker
credentials. Research and sealed-evaluation workers call an independently deployed
runtime over this narrow HTTP contract.
"""

from __future__ import annotations

import os
import secrets
from collections.abc import Mapping
from typing import Any, Literal, Protocol, Self
from urllib.parse import urlparse
from uuid import UUID, uuid4

import httpx
from pydantic import BaseModel, ConfigDict, Field, field_validator

from errors import QfError

NAUTILUS_RUNTIME_NAME = "NAUTILUS_TRADER"
PINNED_NAUTILUS_VERSION = "1.231.0"


class RuntimeContract(BaseModel):
    model_config = ConfigDict(extra="forbid")


class CatalogReference(RuntimeContract):
    dataset_revision_id: UUID
    catalog_uri: str = Field(min_length=1, max_length=4096)
    nautilus_data_type: str = Field(min_length=1, max_length=240)
    instrument_ids: list[str] = Field(min_length=1)
    partition: Literal["DISCOVERY", "SEALED"]
    start_time: str | None = None
    end_time: str | None = None


class StrategyArtifact(RuntimeContract):
    artifact_id: UUID = Field(default_factory=uuid4)
    strategy_path: str = Field(min_length=3, max_length=500)
    config_path: str = Field(min_length=3, max_length=500)
    config: dict[str, Any] = Field(default_factory=dict)
    wheel_base64: str | None = None
    requirements: list[str] = Field(default_factory=list)


class VenueConfiguration(RuntimeContract):
    name: str = "SIM"
    oms_type: Literal["HEDGING", "NETTING"] = "HEDGING"
    account_type: Literal["CASH", "MARGIN", "BETTING"] = "MARGIN"
    base_currency: str = "USD"
    starting_balances: list[str] = Field(default_factory=lambda: ["1_000_000 USD"])
    book_type: str = "L1_MBP"


class ExperimentContract(RuntimeContract):
    run_id: UUID = Field(default_factory=uuid4)
    catalog: CatalogReference
    strategy: StrategyArtifact
    venue: VenueConfiguration = Field(default_factory=VenueConfiguration)
    parameters: dict[str, Any] = Field(default_factory=dict)
    portfolio_targets: list[dict[str, Any]] = Field(default_factory=list)
    requested_reports: list[str] = Field(
        default_factory=lambda: ["orders", "fills", "positions", "account"]
    )

    @field_validator("requested_reports")
    @classmethod
    def reports_are_supported(cls, value: list[str]) -> list[str]:
        allowed = {"orders", "fills", "positions", "account"}
        unsupported = sorted(set(value) - allowed)
        if unsupported:
            raise ValueError(f"Unsupported reports: {', '.join(unsupported)}")
        return value


class BacktestEvidence(RuntimeContract):
    run_id: UUID
    run_config_id: str | None = None
    runtime_name: str = NAUTILUS_RUNTIME_NAME
    runtime_version: str
    catalog_uri: str
    partition: Literal["DISCOVERY", "SEALED"]
    started_at: str | None = None
    finished_at: str | None = None
    elapsed_time_seconds: float = 0.0
    total_events: int = 0
    total_orders: int = 0
    total_positions: int = 0
    statistics: dict[str, Any] = Field(default_factory=dict)
    reports: dict[str, list[dict[str, Any]]] = Field(default_factory=dict)
    disclosure: dict[str, Any] = Field(default_factory=dict)


class CatalogValidation(RuntimeContract):
    valid: bool
    runtime_version: str
    catalog_uri: str
    instruments: list[str] = Field(default_factory=list)
    data_types: list[str] = Field(default_factory=list)
    details: dict[str, Any] = Field(default_factory=dict)


class CandidateVerification(RuntimeContract):
    valid: bool
    runtime_version: str
    errors: list[str] = Field(default_factory=list)
    details: dict[str, Any] = Field(default_factory=dict)


class QuantRuntime(Protocol):
    def ingest(self, payload: Mapping[str, Any], *, idempotency_key: str) -> dict[str, Any]: ...

    def validate_catalog(self, catalog: CatalogReference) -> CatalogValidation: ...

    def run_backtest(self, contract: ExperimentContract) -> BacktestEvidence: ...

    def run_sealed_backtest(self, contract: ExperimentContract) -> BacktestEvidence: ...

    def build_candidate(self, payload: Mapping[str, Any]) -> dict[str, Any]: ...

    def verify_candidate(self, manifest: Mapping[str, Any]) -> CandidateVerification: ...


class RemoteNautilusQuantRuntime:
    """HTTP client for an independently deployed NautilusTrader runtime."""

    def __init__(
        self,
        *,
        base_url: str,
        token: str,
        timeout_seconds: float = 120.0,
        transport: httpx.BaseTransport | None = None,
    ) -> None:
        clean_url = self._validate_base_url(base_url)
        clean_token = token.strip()
        if not clean_token:
            raise ValueError("Remote Nautilus runtime token must be configured")
        if timeout_seconds <= 0:
            raise ValueError("Remote Nautilus runtime timeout must be positive")
        self._base_url = clean_url
        self._token_marker = secrets.token_hex(4)
        self._client = httpx.Client(
            base_url=clean_url,
            timeout=timeout_seconds,
            transport=transport,
            headers={
                "Authorization": f"Bearer {clean_token}",
                "Accept": "application/json",
                "User-Agent": "QuaZonai/remote-nautilus-runtime",
            },
        )

    @staticmethod
    def _validate_base_url(value: str) -> str:
        clean = value.strip().rstrip("/")
        parsed = urlparse(clean)
        if parsed.scheme not in {"http", "https"} or not parsed.netloc:
            raise ValueError("Remote Nautilus runtime URL must be absolute HTTP(S)")
        if parsed.username or parsed.password or parsed.query or parsed.fragment:
            raise ValueError("Remote Nautilus runtime URL must not contain credentials or tokens")
        return clean

    @classmethod
    def from_env(cls, zone: Literal["DISCOVERY", "SEALED"]) -> Self:
        prefix = "QUAZONAI_NAUTILUS_RESEARCH" if zone == "DISCOVERY" else "QUAZONAI_NAUTILUS_SEALED"
        base_url = os.environ.get(f"{prefix}_URL", "")
        token = os.environ.get(f"{prefix}_TOKEN", "")
        raw_timeout = os.environ.get("QUAZONAI_NAUTILUS_TIMEOUT_SECONDS", "120")
        try:
            timeout = float(raw_timeout)
        except ValueError as exc:
            raise ValueError("QUAZONAI_NAUTILUS_TIMEOUT_SECONDS must be numeric") from exc
        return cls(base_url=base_url, token=token, timeout_seconds=timeout)

    @property
    def endpoint(self) -> str:
        return self._base_url

    def __repr__(self) -> str:
        return (
            f"RemoteNautilusQuantRuntime(base_url={self._base_url!r}, "
            f"credential='configured:{self._token_marker}')"
        )

    def __enter__(self) -> Self:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    def close(self) -> None:
        self._client.close()

    def _request(
        self,
        method: str,
        path: str,
        *,
        payload: Mapping[str, Any] | BaseModel | None = None,
        idempotency_key: str | None = None,
    ) -> dict[str, Any]:
        headers = {"Idempotency-Key": idempotency_key} if idempotency_key else None
        json_payload: Any
        if isinstance(payload, BaseModel):
            json_payload = payload.model_dump(mode="json")
        else:
            json_payload = dict(payload) if payload is not None else None
        try:
            response = self._client.request(method, path, json=json_payload, headers=headers)
            response.raise_for_status()
            result = response.json()
        except httpx.TimeoutException as exc:
            raise QfError(
                "NAUTILUS_RUNTIME_TIMEOUT",
                "The remote NautilusTrader runtime exceeded its request timeout.",
                504,
                {"endpoint": self._base_url, "path": path},
            ) from exc
        except httpx.HTTPStatusError as exc:
            detail: Any = None
            try:
                detail = exc.response.json()
            except ValueError:
                detail = exc.response.text[-2000:]
            raise QfError(
                "NAUTILUS_RUNTIME_REJECTED",
                "The remote NautilusTrader runtime rejected the request.",
                502,
                {
                    "endpoint": self._base_url,
                    "path": path,
                    "status_code": exc.response.status_code,
                    "runtime_detail": detail,
                },
            ) from exc
        except (httpx.HTTPError, ValueError) as exc:
            raise QfError(
                "NAUTILUS_RUNTIME_UNAVAILABLE",
                "The remote NautilusTrader runtime is unavailable or returned invalid JSON.",
                503,
                {"endpoint": self._base_url, "path": path},
            ) from exc
        if not isinstance(result, dict):
            raise QfError(
                "NAUTILUS_RUNTIME_CONTRACT_INVALID",
                "The remote NautilusTrader runtime returned a non-object response.",
                502,
                {"endpoint": self._base_url, "path": path},
            )
        return result

    def health(self) -> dict[str, Any]:
        return self._request("GET", "/v1/health")

    def ingest(self, payload: Mapping[str, Any], *, idempotency_key: str) -> dict[str, Any]:
        return self._request(
            "POST",
            "/v1/catalogs/ingest",
            payload=payload,
            idempotency_key=idempotency_key,
        )

    def validate_catalog(self, catalog: CatalogReference) -> CatalogValidation:
        return CatalogValidation.model_validate(
            self._request("POST", "/v1/catalogs/validate", payload=catalog)
        )

    def run_backtest(self, contract: ExperimentContract) -> BacktestEvidence:
        discovery = contract.model_copy(
            update={"catalog": contract.catalog.model_copy(update={"partition": "DISCOVERY"})}
        )
        return BacktestEvidence.model_validate(
            self._request(
                "POST",
                "/v1/backtests",
                payload=discovery,
                idempotency_key=str(contract.run_id),
            )
        )

    def run_sealed_backtest(self, contract: ExperimentContract) -> BacktestEvidence:
        sealed = contract.model_copy(
            update={"catalog": contract.catalog.model_copy(update={"partition": "SEALED"})}
        )
        return BacktestEvidence.model_validate(
            self._request(
                "POST",
                "/v1/sealed-backtests",
                payload=sealed,
                idempotency_key=str(contract.run_id),
            )
        )

    def build_candidate(self, payload: Mapping[str, Any]) -> dict[str, Any]:
        return self._request("POST", "/v1/candidates/build", payload=payload)

    def verify_candidate(self, manifest: Mapping[str, Any]) -> CandidateVerification:
        return CandidateVerification.model_validate(
            self._request("POST", "/v1/candidates/verify", payload={"manifest": dict(manifest)})
        )
