"""Persistence for remote Nautilus catalogs, runs, sealed episodes, and Search Ledger."""

from __future__ import annotations

from datetime import datetime
from decimal import Decimal
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import DateTime, ForeignKey, Index, String, Text, UniqueConstraint, Uuid
from sqlalchemy.orm import Mapped, mapped_column

from db.base import Base, JSON_VALUE, MONEY, TimestampMixin


class NautilusCatalogBinding(Base, TimestampMixin):
    __tablename__ = "nautilus_catalog_bindings"
    __table_args__ = (
        UniqueConstraint("catalog_uri", name="uq_nautilus_catalog_uri"),
        Index("ix_nautilus_catalog_sealed", "sealed", "quality_state"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("dataset_revisions.id", ondelete="CASCADE"),
        nullable=False,
        unique=True,
    )
    catalog_uri: Mapped[str] = mapped_column(Text, nullable=False)
    provider: Mapped[str] = mapped_column(String(200), nullable=False)
    source_license: Mapped[str] = mapped_column(Text, nullable=False)
    nautilus_data_type: Mapped[str] = mapped_column(String(200), nullable=False)
    instrument_scope: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    event_time_range: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    available_time_range: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    schema_revision: Mapped[str] = mapped_column(String(100), nullable=False)
    quality_state: Mapped[str] = mapped_column(String(40), nullable=False, default="VALID")
    quality_result: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    point_in_time_state: Mapped[str] = mapped_column(
        String(40), nullable=False, default="VALID"
    )
    point_in_time_result: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    sealed: Mapped[bool] = mapped_column(nullable=False, default=False)


class QuantRuntimeRun(Base, TimestampMixin):
    __tablename__ = "quant_runtime_runs"
    __table_args__ = (
        Index("ix_quant_runtime_run_program", "program_id", "created_at"),
        Index("ix_quant_runtime_run_mission", "mission_id", "created_at"),
        UniqueConstraint(
            "runtime_name",
            "external_run_id",
            name="uq_quant_runtime_external_run",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_programs.id", ondelete="CASCADE"),
        nullable=False,
    )
    branch_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_branches.id", ondelete="CASCADE"),
        nullable=False,
    )
    mission_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey("research_missions.id", ondelete="SET NULL"),
    )
    evaluation_episode_id: Mapped[UUID | None] = mapped_column(Uuid)
    parent_run_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey("quant_runtime_runs.id", ondelete="SET NULL"),
    )
    mode: Mapped[str] = mapped_column(String(40), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="PENDING")
    external_run_id: Mapped[str | None] = mapped_column(String(200))
    experiment_key: Mapped[str] = mapped_column(String(200), nullable=False)
    family: Mapped[str] = mapped_column(String(200), nullable=False)
    catalog_uri: Mapped[str] = mapped_column(Text, nullable=False)
    runtime_name: Mapped[str] = mapped_column(String(100), nullable=False, default="NautilusTrader")
    runtime_version: Mapped[str | None] = mapped_column(String(100))
    contract_version: Mapped[str | None] = mapped_column(String(40))
    strategy_artifact: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    parameters: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    evidence: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    error_code: Mapped[str | None] = mapped_column(String(100))
    error_message: Mapped[str | None] = mapped_column(Text)
    started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    finished_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class EvaluationEpisode(Base, TimestampMixin):
    __tablename__ = "evaluation_episodes"
    __table_args__ = (Index("ix_evaluation_episode_program", "program_id", "state"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_programs.id", ondelete="CASCADE"),
        nullable=False,
    )
    branch_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_branches.id", ondelete="CASCADE"),
        nullable=False,
    )
    discovery_run_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("quant_runtime_runs.id", ondelete="RESTRICT"),
        nullable=False,
    )
    sealed_run_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey("quant_runtime_runs.id", ondelete="SET NULL"),
    )
    sealed_dataset_revision_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey("dataset_revisions.id", ondelete="RESTRICT"),
    )
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="SEALED_PENDING")
    disclosure: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    failure_code: Mapped[str | None] = mapped_column(String(100))


class CapitalContextVersion(Base, TimestampMixin):
    """Versioned research capital scale; never an account or position ledger."""

    __tablename__ = "capital_context_versions"
    __table_args__ = (Index("ix_capital_context_validity", "valid_until", "observed_at"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    source_type: Mapped[str] = mapped_column(String(40), nullable=False)
    source_downstream_system_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey("downstream_systems.id", ondelete="RESTRICT"),
    )
    base_currency: Mapped[str] = mapped_column(String(20), nullable=False)
    deployable_capital: Mapped[Decimal] = mapped_column(MONEY, nullable=False)
    observed_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    valid_until: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    notes: Mapped[str | None] = mapped_column(Text)


class SearchLedgerEntry(Base):
    __tablename__ = "search_ledger_entries"
    __table_args__ = (
        Index("ix_search_ledger_program", "program_id", "created_at"),
        UniqueConstraint("run_id", name="uq_search_ledger_run"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_programs.id", ondelete="CASCADE"),
        nullable=False,
    )
    branch_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_branches.id", ondelete="CASCADE"),
        nullable=False,
    )
    mission_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey("research_missions.id", ondelete="SET NULL"),
    )
    run_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("quant_runtime_runs.id", ondelete="CASCADE"),
        nullable=False,
    )
    family: Mapped[str] = mapped_column(String(200), nullable=False)
    parameters: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    outcome: Mapped[str] = mapped_column(String(40), nullable=False)
    failure_code: Mapped[str | None] = mapped_column(String(100))
    disclosure_level: Mapped[str] = mapped_column(
        String(40), nullable=False, default="DISCOVERY_FULL"
    )
    evidence_summary: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


__all__ = [
    "NautilusCatalogBinding",
    "QuantRuntimeRun",
    "EvaluationEpisode",
    "CapitalContextVersion",
    "SearchLedgerEntry",
]
