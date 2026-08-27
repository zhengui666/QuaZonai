"""Typed protocol shared by QuaZonai Core and the remote Nautilus gateway."""

from __future__ import annotations

from datetime import datetime
from enum import StrEnum
from typing import Any, Literal
from uuid import UUID, uuid4

from pydantic import BaseModel, ConfigDict, Field, model_validator

QUANT_RUNTIME_PROTOCOL_VERSION = "1"
PINNED_NAUTILUS_VERSION = "1.231.0"


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


def _require_aware_datetime(value: datetime, *, field_name: str) -> None:
    if value.tzinfo is None or value.utcoffset() is None:
        raise ValueError(f"{field_name} must be timezone-aware")


class ExperimentMode(StrEnum):
    DISCOVERY = "DISCOVERY"
    SEALED = "SEALED"
    PORTFOLIO = "PORTFOLIO"
    CONFORMANCE = "CONFORMANCE"


class RuntimeCapabilities(StrictModel):
    protocol_version: str
    runtime_name: Literal["NAUTILUS_TRADER"] = "NAUTILUS_TRADER"
    runtime_version: str
    catalog_kind: Literal["PARQUET_DATA_CATALOG"] = "PARQUET_DATA_CATALOG"
    supported_operations: list[str]
    live_execution_exposed: bool = False


class QuoteRow(StrictModel):
    timestamp: datetime
    available_at: datetime
    bid_price: str
    ask_price: str
    volume: str | None = None

    @model_validator(mode="after")
    def validate_availability(self) -> QuoteRow:
        _require_aware_datetime(self.timestamp, field_name="timestamp")
        _require_aware_datetime(self.available_at, field_name="available_at")
        if self.available_at < self.timestamp:
            raise ValueError("available_at cannot precede the market event timestamp")
        return self


class InstrumentQuoteBatch(StrictModel):
    instrument_id: str = Field(min_length=3, max_length=200)
    instrument_type: str = Field(
        min_length=1,
        max_length=80,
        pattern=r"^[A-Za-z][A-Za-z0-9_]*$",
    )
    instrument_definition: dict[str, Any]
    rows: list[QuoteRow] = Field(min_length=2, max_length=1_000_000)


class CatalogIngestRequest(StrictModel):
    protocol_version: str = QUANT_RUNTIME_PROTOCOL_VERSION
    request_id: UUID = Field(default_factory=uuid4)
    catalog_key: str = Field(
        min_length=1,
        max_length=128,
        pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$",
    )
    provider: str = Field(min_length=1, max_length=200)
    source: str = Field(min_length=1, max_length=500)
    source_license: str | None = Field(default=None, max_length=500)
    nautilus_data_type: Literal["QuoteTick"] = "QuoteTick"
    instruments: list[InstrumentQuoteBatch] = Field(min_length=1, max_length=10_000)

    @model_validator(mode="after")
    def validate_instrument_batches(self) -> CatalogIngestRequest:
        ids = [item.instrument_id for item in self.instruments]
        if len(ids) != len(set(ids)):
            raise ValueError("catalog ingest instrument_ids must be unique")
        if sum(len(item.rows) for item in self.instruments) > 1_000_000:
            raise ValueError("catalog ingest is limited to 1,000,000 QuoteTicks per revision")
        return self


class CatalogIngestResult(StrictModel):
    protocol_version: str
    runtime_version: str
    catalog_key: str
    catalog_uri: str
    nautilus_data_type: str
    instrument_scope: list[str]
    event_time_start: datetime
    event_time_end: datetime
    available_time_start: datetime
    available_time_end: datetime
    row_count: int = Field(ge=0)
    schema_revision: str
    quality_result: dict[str, Any]
    point_in_time_result: dict[str, Any]
    ingested_at: datetime


class CatalogValidationRequest(StrictModel):
    protocol_version: str = QUANT_RUNTIME_PROTOCOL_VERSION
    request_id: UUID = Field(default_factory=uuid4)
    catalog_key: str = Field(
        min_length=1,
        max_length=128,
        pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$",
    )
    instrument_ids: list[str] = Field(default_factory=list)
    nautilus_data_type: str | None = None


class CatalogValidationResult(StrictModel):
    protocol_version: str
    runtime_version: str
    catalog_key: str
    valid: bool
    instrument_scope: list[str]
    row_count: int = Field(ge=0)
    event_time_start: datetime | None = None
    event_time_end: datetime | None = None
    available_time_start: datetime | None = None
    available_time_end: datetime | None = None
    findings: list[dict[str, Any]] = Field(default_factory=list)


class StrategyArtifact(StrictModel):
    artifact_id: str = Field(min_length=1, max_length=240)
    kind: Literal["IMPORTABLE", "SOURCE_BUNDLE"]
    strategy_path: str = Field(min_length=3, max_length=500)
    config_path: str = Field(min_length=3, max_length=500)
    config: dict[str, Any] = Field(default_factory=dict)
    source_files: dict[str, str] = Field(default_factory=dict)
    requirements: list[str] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_source_bundle(self) -> StrategyArtifact:
        if self.kind == "SOURCE_BUNDLE" and not self.source_files:
            raise ValueError("SOURCE_BUNDLE strategies require source_files")
        if self.kind == "IMPORTABLE" and self.source_files:
            raise ValueError("IMPORTABLE strategies cannot include source_files")
        for path in self.source_files:
            if path.startswith(("/", "\\")) or ".." in path.replace("\\", "/").split("/"):
                raise ValueError("strategy source paths must be relative and traversal-free")
        return self


class BacktestExperimentRequest(StrictModel):
    protocol_version: str = QUANT_RUNTIME_PROTOCOL_VERSION
    experiment_id: UUID = Field(default_factory=uuid4)
    mode: ExperimentMode = ExperimentMode.DISCOVERY
    dataset_revision_id: UUID
    catalog_key: str = Field(
        min_length=1,
        max_length=128,
        pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$",
    )
    instrument_ids: list[str] = Field(min_length=1)
    strategy: StrategyArtifact
    start_time: datetime | None = None
    end_time: datetime | None = None
    venue_config: dict[str, Any] = Field(default_factory=dict)
    data_config: dict[str, Any] = Field(default_factory=dict)
    risk_config: dict[str, Any] = Field(default_factory=dict)
    tags: dict[str, str] = Field(default_factory=dict)

    @model_validator(mode="after")
    def validate_v1_configuration(self) -> BacktestExperimentRequest:
        if self.start_time is not None:
            _require_aware_datetime(self.start_time, field_name="start_time")
        if self.end_time is not None:
            _require_aware_datetime(self.end_time, field_name="end_time")
        if (
            self.start_time is not None
            and self.end_time is not None
            and self.start_time >= self.end_time
        ):
            raise ValueError("start_time must precede end_time")
        if self.data_config:
            raise ValueError(
                "data_config is reserved until protocol v1 explicitly applies its fields; use the "
                "top-level catalog/instrument/time contract instead"
            )
        if self.risk_config:
            raise ValueError(
                "risk_config is reserved until protocol v1 explicitly applies a Nautilus RiskEngine "
                "configuration"
            )
        return self


class OrderEvidence(StrictModel):
    order_id: str
    instrument_id: str | None = None
    side: str | None = None
    order_type: str | None = None
    status: str | None = None
    quantity: str | None = None
    filled_quantity: str | None = None
    ts_init: int | None = None


class FillEvidence(StrictModel):
    trade_id: str | None = None
    order_id: str | None = None
    instrument_id: str | None = None
    side: str | None = None
    quantity: str | None = None
    price: str | None = None
    commission: str | None = None
    ts_event: int | None = None


class PositionEvidence(StrictModel):
    position_id: str
    instrument_id: str | None = None
    side: str | None = None
    quantity: str | None = None
    realized_pnl: str | None = None
    unrealized_pnl: str | None = None
    opened_at: int | None = None
    closed_at: int | None = None


class BacktestEvidence(StrictModel):
    protocol_version: str
    runtime_version: str
    experiment_id: UUID
    remote_run_id: str
    mode: ExperimentMode
    started_at: datetime
    finished_at: datetime
    orders: list[OrderEvidence]
    fills: list[FillEvidence]
    positions: list[PositionEvidence]
    balances: list[dict[str, Any]]
    pnl: dict[str, Any]
    statistics: dict[str, Any]
    diagnostics: dict[str, Any] = Field(default_factory=dict)


class SealedBacktestResult(StrictModel):
    protocol_version: str
    runtime_version: str
    experiment_id: UUID
    remote_run_id: str
    mode: Literal[ExperimentMode.SEALED] = ExperimentMode.SEALED
    disclosure: dict[str, Any]
    raw_evidence_withheld: Literal[True] = True


class CandidateVerificationRequest(StrictModel):
    protocol_version: str = QUANT_RUNTIME_PROTOCOL_VERSION
    candidate_id: UUID
    manifest: dict[str, Any]
    strategy_wheel_b64: str
    fixture: dict[str, Any]


class CandidateVerificationResult(StrictModel):
    protocol_version: str
    runtime_version: str
    candidate_id: UUID
    compatible: bool
    findings: list[dict[str, Any]] = Field(default_factory=list)
