"""Stable structural plugin descriptor contract."""

from __future__ import annotations

import re
from enum import StrEnum
from typing import Any, Protocol, runtime_checkable

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

_PLUGIN_ID = re.compile(r"^[a-z][a-z0-9_]{1,63}$")


class Capability(StrEnum):
    HISTORICAL_IMPORT = "HISTORICAL_IMPORT"
    LIVE_DATA = "LIVE_DATA"
    EXECUTION = "EXECUTION"


class DescriptorSnapshot(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)

    plugin_id: str
    version: str
    api_version: str = "1"
    capabilities: frozenset[Capability]
    compatibility_key: str
    requires_python: str
    requires_qf: str
    requires_nautilus: str | None = None
    public_config_schema: dict[str, Any] = Field(default_factory=dict)
    secret_config_schema: dict[str, Any] = Field(default_factory=dict)
    required_secret_names: tuple[str, ...] = ()

    @field_validator("plugin_id")
    @classmethod
    def validate_plugin_id(cls, value: str) -> str:
        if not _PLUGIN_ID.fullmatch(value):
            raise ValueError("plugin_id must match ^[a-z][a-z0-9_]{1,63}$")
        return value

    @field_validator("version", "api_version", "compatibility_key")
    @classmethod
    def non_empty(cls, value: str) -> str:
        value = value.strip()
        if not value:
            raise ValueError("value must not be empty")
        return value

    @model_validator(mode="after")
    def validate_secret_names(self) -> "DescriptorSnapshot":
        properties = self.secret_config_schema.get("properties", {})
        missing = sorted(set(self.required_secret_names) - set(properties))
        if missing:
            raise ValueError(
                "required_secret_names are absent from secret_config_schema: "
                + ", ".join(missing)
            )
        return self


@runtime_checkable
class RuntimePlugin(Protocol):
    """Runtime-only plugin object loaded inside validator/runner child processes."""

    def descriptor(self) -> DescriptorSnapshot: ...

    def build_data_config(self, public_config: dict[str, Any]) -> object: ...

    def build_execution_config(self, public_config: dict[str, Any]) -> object: ...

    def build_catalog_importer(self, public_config: dict[str, Any]) -> object: ...
