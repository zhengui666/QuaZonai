"""Gateway-side validation models for protocol version 1."""

from __future__ import annotations

from datetime import datetime
from enum import StrEnum
from typing import Any, Literal
from uuid import UUID, uuid4

from pydantic import BaseModel, ConfigDict, Field, model_validator


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class ExperimentMode(StrEnum):
    DISCOVERY = "DISCOVERY"
    SEALED = "SEALED"
    PORTFOLIO = "PORTFOLIO"
    CONFORMANCE = "CONFORMANCE"


class QuoteRow(StrictModel):
    timestamp: datetime
    available_at: datetime
    bid_price: str
    ask_price: str
    volume: str | None = None

    @model_validator(mode="after")
    def validate_availability(self) -> QuoteRow:
        if self.available_at < self.timestamp:
            raise ValueError("available_at cannot precede the market event timestamp")
        return self


class CatalogIngestRequest(StrictModel):
    protocol_version: str = "1"
    request_id: UUID = Field(default_factory=uuid4)
    catalog_key: str = Field(min_length=1, max_length=128, pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
    provider: str = Field(min_length=1, max_length=200)
    source: str = Field(min_length=1, max_length=500)
    source_license: str | None = Field(default=None, max_length=500)
    instrument_id: str = Field(min_length=3, max_length=200)
    nautilus_data_type: Literal["QuoteTick"] = "QuoteTick"
    rows: list[QuoteRow] = Field(min_length=2, max_length=1_000_000)


class CatalogValidationRequest(StrictModel):
    protocol_version: str = "1"
    request_id: UUID = Field(default_factory=uuid4)
    catalog_key: str = Field(min_length=1, max_length=128, pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
    instrument_ids: list[str] = Field(default_factory=list)
    nautilus_data_type: str | None = None


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
            parts = path.replace("\\", "/").split("/")
            if path.startswith(("/", "\\")) or ".." in parts:
                raise ValueError("strategy source paths must be relative and traversal-free")
        return self


class BacktestExperimentRequest(StrictModel):
    protocol_version: str = "1"
    experiment_id: UUID = Field(default_factory=uuid4)
    mode: ExperimentMode = ExperimentMode.DISCOVERY
    dataset_revision_id: UUID
    catalog_key: str = Field(min_length=1, max_length=128, pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
    instrument_ids: list[str] = Field(min_length=1)
    strategy: StrategyArtifact
    start_time: datetime | None = None
    end_time: datetime | None = None
    venue_config: dict[str, Any] = Field(default_factory=dict)
    data_config: dict[str, Any] = Field(default_factory=dict)
    risk_config: dict[str, Any] = Field(default_factory=dict)
    tags: dict[str, str] = Field(default_factory=dict)

    @model_validator(mode="after")
    def reject_unapplied_configuration(self) -> BacktestExperimentRequest:
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


class CandidateVerificationRequest(StrictModel):
    protocol_version: str = "1"
    candidate_id: UUID
    manifest: dict[str, Any]
    strategy_wheel_b64: str
    fixture: dict[str, Any]
