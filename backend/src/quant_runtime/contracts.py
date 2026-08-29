"""Typed protocol shared by QuaZonai and a remote NautilusTrader runtime."""

from __future__ import annotations

from datetime import datetime
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

RunMode = Literal["DISCOVERY", "SEALED", "PORTFOLIO"]
TerminalState = Literal["SUCCEEDED", "FAILED", "CANCELLED"]
_FORBIDDEN_SECRET_PARTS = (
    "api_key",
    "apikey",
    "private_key",
    "broker_secret",
    "exchange_secret",
    "wallet_seed",
    "password",
)


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


def _secret_path(value: object, path: str = "$") -> str | None:
    if isinstance(value, dict):
        for key, item in value.items():
            key_text = str(key).casefold()
            if any(part in key_text for part in _FORBIDDEN_SECRET_PARTS):
                return f"{path}.{key}"
            found = _secret_path(item, f"{path}.{key}")
            if found is not None:
                return found
    elif isinstance(value, list):
        for index, item in enumerate(value):
            found = _secret_path(item, f"{path}[{index}]")
            if found is not None:
                return found
    return None


class StrategyArtifact(StrictModel):
    """A frozen Nautilus strategy artifact usable by Backtest, Paper, and Live runtimes."""

    strategy_path: str = Field(min_length=3, max_length=500)
    config_path: str = Field(min_length=3, max_length=500)
    config: dict[str, Any]
    source_files: dict[str, str] = Field(min_length=1, max_length=50)
    requirements: list[str] = Field(default_factory=list, max_length=100)

    @field_validator("strategy_path", "config_path")
    @classmethod
    def require_import_path(cls, value: str) -> str:
        if ":" not in value:
            raise ValueError("Strategy and config paths must use module:attribute syntax")
        return value

    @field_validator("source_files")
    @classmethod
    def validate_source_files(cls, value: dict[str, str]) -> dict[str, str]:
        for path, source in value.items():
            if path.startswith("/") or ".." in path.split("/"):
                raise ValueError("Strategy source paths must remain relative to the artifact root")
            if not path.endswith(".py"):
                raise ValueError("Strategy artifacts may contain Python source files only")
            if len(source.encode("utf-8")) > 1_000_000:
                raise ValueError("A strategy source file exceeds the 1 MiB contract limit")
        return value

    @model_validator(mode="after")
    def reject_credentials(self) -> StrategyArtifact:
        found = _secret_path(self.model_dump(mode="python"))
        if found is not None:
            raise ValueError(f"Strategy artifact contains a forbidden credential field at {found}")
        return self


class CatalogIngestSpec(StrictModel):
    catalog_name: str = Field(pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]{0,119}$")
    provider: str = Field(min_length=1, max_length=200)
    source_license: str = Field(min_length=1, max_length=500)
    sealed: bool = False
    source_spec: dict[str, Any]

    @model_validator(mode="after")
    def reject_credentials(self) -> CatalogIngestSpec:
        found = _secret_path(self.source_spec)
        if found is not None:
            raise ValueError(f"Catalog source spec contains a forbidden credential field at {found}")
        return self


class CatalogDescriptor(StrictModel):
    catalog_uri: str
    provider: str
    source_license: str
    source_spec: dict[str, Any] = Field(default_factory=dict)
    nautilus_data_type: str
    instrument_scope: list[str]
    event_start: datetime | None = None
    event_end: datetime | None = None
    available_start: datetime | None = None
    available_end: datetime | None = None
    row_count: int = Field(ge=0)
    schema_revision: str
    quality_result: dict[str, Any]
    point_in_time_result: dict[str, Any]
    sealed: bool = False

    @field_validator("catalog_uri")
    @classmethod
    def require_opaque_catalog_uri(cls, value: str) -> str:
        if not value.startswith("catalog://"):
            raise ValueError("Remote catalog references must use an opaque catalog:// URI")
        return value


class ExperimentSpec(StrictModel):
    experiment_key: str = Field(min_length=1, max_length=200)
    family: str = Field(min_length=1, max_length=200)
    catalog_uri: str
    strategy: StrategyArtifact
    parameters: dict[str, Any] = Field(default_factory=dict)
    promotion_gate: dict[str, float | int] = Field(default_factory=dict)

    @field_validator("catalog_uri")
    @classmethod
    def require_catalog_uri(cls, value: str) -> str:
        if not value.startswith("catalog://"):
            raise ValueError("Experiment catalog_uri must use the catalog:// scheme")
        return value

    @model_validator(mode="after")
    def reject_credentials(self) -> ExperimentSpec:
        found = _secret_path(self.parameters)
        if found is not None:
            raise ValueError(f"Experiment parameters contain a forbidden credential field at {found}")
        return self


class MissionExperimentEnvelope(StrictModel):
    experiments: list[ExperimentSpec] = Field(min_length=1, max_length=20)


class RunEvidence(StrictModel):
    external_run_id: str
    state: TerminalState
    mode: RunMode
    runtime_name: str
    nautilus_version: str
    contract_version: str
    catalog_uri: str
    strategy_artifact: dict[str, Any]
    orders: list[dict[str, Any]] = Field(default_factory=list)
    fills: list[dict[str, Any]] = Field(default_factory=list)
    positions: list[dict[str, Any]] = Field(default_factory=list)
    account: list[dict[str, Any]] = Field(default_factory=list)
    statistics: dict[str, Any] = Field(default_factory=dict)
    started_at: datetime | None = None
    finished_at: datetime | None = None
    error_code: str | None = None
    error_message: str | None = None


class RuntimeCapabilities(StrictModel):
    runtime_name: str
    nautilus_version: str
    contract_version: str
    catalog_type: str
    supported_modes: list[RunMode]
    candidate_contract_version: str
