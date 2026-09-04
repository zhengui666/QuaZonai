"""Typed protocol shared by QuaZonai and a remote NautilusTrader runtime."""

from __future__ import annotations

from datetime import datetime
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

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


class CatalogIngestSpec(StrictModel):
    catalog_name: str = Field(pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]{0,119}$")
    provider: str = Field(min_length=1, max_length=200)
    source_license: str = Field(min_length=1, max_length=500)
    sealed: bool = False
    source_spec: dict[str, Any]
    source_shards: list[dict[str, Any]] | None = Field(default=None, max_length=168)
    plugin_id: str | None = Field(default=None, pattern=r"^[a-z][a-z0-9_]{1,63}$")
    plugin_version: str | None = Field(default=None, max_length=100)
    plugin_bundle_path: str | None = Field(default=None, max_length=500)

    @model_validator(mode="after")
    def validate_plugin_binding(self) -> CatalogIngestSpec:
        values = (self.plugin_id, self.plugin_version, self.plugin_bundle_path)
        if any(value is not None for value in values) and not all(value is not None for value in values):
            raise ValueError("plugin_id, plugin_version and plugin_bundle_path must be provided together")
        return self

    @model_validator(mode="after")
    def reject_credentials(self) -> CatalogIngestSpec:
        found = _secret_path(
            {
                "source_spec": self.source_spec,
                "source_shards": self.source_shards,
            }
        )
        if found is not None:
            raise ValueError(f"Catalog source spec contains a forbidden credential field at {found}")
        if self.source_shards is not None:
            for shard in self.source_shards:
                ArchiveShardDescriptor.model_validate(shard)
                if shard.get("state") != "AVAILABLE":
                    raise ValueError("Catalog materialization may only use AVAILABLE archive shards")
        return self


class ArchiveManifestSpec(StrictModel):
    """Request for a plugin-owned remote archive shard inventory."""

    manifest_name: str = Field(pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]{0,119}$")
    provider: str = Field(min_length=1, max_length=200)
    source_license: str = Field(min_length=1, max_length=500)
    source_spec: dict[str, Any]
    plugin_id: str | None = Field(default=None, pattern=r"^[a-z][a-z0-9_]{1,63}$")
    plugin_version: str | None = Field(default=None, max_length=100)
    plugin_bundle_path: str | None = Field(default=None, max_length=500)

    @model_validator(mode="after")
    def validate_plugin_binding(self) -> ArchiveManifestSpec:
        values = (self.plugin_id, self.plugin_version, self.plugin_bundle_path)
        if any(value is not None for value in values) and not all(value is not None for value in values):
            raise ValueError("plugin_id, plugin_version and plugin_bundle_path must be provided together")
        if self.source_spec.get("kind") != "plugin":
            raise ValueError("archive manifest inspection requires source_spec.kind=plugin")
        found = _secret_path(self.source_spec)
        if found is not None:
            raise ValueError(f"Archive manifest source spec contains a forbidden credential field at {found}")
        return self


class ArchiveShardDescriptor(StrictModel):
    shard_key: str = Field(min_length=1, max_length=40)
    source_url: str = Field(min_length=1, max_length=500)
    coverage_start: datetime
    coverage_end: datetime
    size_bytes: int | None = Field(default=None, ge=0)
    state: Literal["AVAILABLE", "MISSING", "PROBE_ERROR"]
    observed_at: datetime


class ArchiveManifestDescriptor(StrictModel):
    manifest_uri: str
    provider: str
    source_license: str
    source_spec: dict[str, Any]
    coverage_start: datetime
    coverage_end: datetime
    scanned_until: datetime
    shard_count: int = Field(ge=0)
    total_bytes: int = Field(ge=0)
    missing_shard_count: int = Field(ge=0)
    probe_error_count: int = Field(ge=0)
    schema_revision: str
    point_in_time_result: dict[str, Any]
    shards: list[ArchiveShardDescriptor] = Field(max_length=200_000)

    @field_validator("manifest_uri")
    @classmethod
    def require_opaque_manifest_uri(cls, value: str) -> str:
        if not value.startswith("manifest://"):
            raise ValueError("Archive manifest references must use manifest://")
        return value

    @model_validator(mode="after")
    def validate_counts(self) -> ArchiveManifestDescriptor:
        if self.shard_count != len(self.shards):
            raise ValueError("Archive manifest shard_count does not match shards")
        if self.missing_shard_count != sum(item.state == "MISSING" for item in self.shards):
            raise ValueError("Archive manifest missing_shard_count does not match shards")
        if self.probe_error_count != sum(item.state == "PROBE_ERROR" for item in self.shards):
            raise ValueError("Archive manifest probe_error_count does not match shards")
        available_sizes = [
            item.size_bytes for item in self.shards if item.state == "AVAILABLE"
        ]
        if any(size is None for size in available_sizes):
            raise ValueError("Archive manifest AVAILABLE shard sizes must be known")
        if self.total_bytes != sum(size for size in available_sizes if size is not None):
            raise ValueError("Archive manifest total_bytes does not match available shard sizes")
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


class RuntimeCapabilities(StrictModel):
    runtime_name: str
    nautilus_version: str
    contract_version: str
    catalog_type: str
    supported_modes: list[str]
    candidate_contract_version: str
