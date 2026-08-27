"""Gateway-side validation models for protocol version 1."""

from __future__ import annotations

import ast
from datetime import datetime
from enum import StrEnum
from typing import Any, Literal
from uuid import UUID, uuid4

from pydantic import BaseModel, ConfigDict, Field, model_validator

_SAFE_SOURCE_IMPORTS = {
    "collections",
    "dataclasses",
    "decimal",
    "enum",
    "math",
    "statistics",
    "typing",
    "nautilus_trader.config",
    "nautilus_trader.examples.strategies.ema_cross",
    "nautilus_trader.indicators",
    "nautilus_trader.model.data",
    "nautilus_trader.model.enums",
    "nautilus_trader.model.identifiers",
    "nautilus_trader.model.objects",
    "nautilus_trader.trading.strategy",
}
_FORBIDDEN_SOURCE_NAMES = {
    "__import__",
    "breakpoint",
    "compile",
    "delattr",
    "dir",
    "eval",
    "exec",
    "exit",
    "getattr",
    "globals",
    "help",
    "input",
    "locals",
    "open",
    "quit",
    "setattr",
    "vars",
}
_FORBIDDEN_SOURCE_ATTRIBUTES = {
    "__bases__",
    "__builtins__",
    "__class__",
    "__closure__",
    "__code__",
    "__dict__",
    "__getattribute__",
    "__globals__",
    "__loader__",
    "__mro__",
    "__spec__",
    "__subclasses__",
}


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


def _require_aware_datetime(value: datetime, *, field_name: str) -> None:
    if value.tzinfo is None or value.utcoffset() is None:
        raise ValueError(f"{field_name} must be timezone-aware")


def _validate_restricted_strategy_source(path: str, source: str) -> None:
    """Keep Mission-authored Python inside the remote runtime capability boundary.

    SOURCE_BUNDLE code executes in a disposable process with a sanitized environment and no
    network. This AST gate additionally prevents it from obtaining filesystem/process/reflection
    capabilities which could inspect a held-out catalog or forge the trusted runner result.
    """
    try:
        tree = ast.parse(source, filename=path)
    except SyntaxError as exc:
        raise ValueError(f"strategy source {path!r} is not valid Python") from exc
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                if alias.name not in _SAFE_SOURCE_IMPORTS:
                    raise ValueError(
                        f"strategy source import {alias.name!r} is outside the runtime allowlist"
                    )
        elif isinstance(node, ast.ImportFrom):
            if node.level:
                continue
            module = node.module or ""
            if module not in _SAFE_SOURCE_IMPORTS:
                raise ValueError(
                    f"strategy source import {module!r} is outside the runtime allowlist"
                )
        elif isinstance(node, ast.Name):
            if node.id in _FORBIDDEN_SOURCE_NAMES or node.id.startswith("__"):
                raise ValueError(f"strategy source name {node.id!r} is not permitted")
        elif isinstance(node, ast.Attribute) and node.attr in _FORBIDDEN_SOURCE_ATTRIBUTES:
            raise ValueError(f"strategy source attribute {node.attr!r} is not permitted")


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
    protocol_version: str = "1"
    request_id: UUID = Field(default_factory=uuid4)
    catalog_key: str = Field(min_length=1, max_length=128, pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
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
        for path, source in self.source_files.items():
            parts = path.replace("\\", "/").split("/")
            if path.startswith(("/", "\\")) or ".." in parts:
                raise ValueError("strategy source paths must be relative and traversal-free")
            _validate_restricted_strategy_source(path, source)
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


class CandidateVerificationRequest(StrictModel):
    protocol_version: str = "1"
    candidate_id: UUID
    manifest: dict[str, Any]
    strategy_wheel_b64: str
    fixture: dict[str, Any]
