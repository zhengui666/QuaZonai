"""Persistence for the Nautilus-first research runtime boundary."""

from __future__ import annotations

from datetime import datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import DateTime, ForeignKey, Index, String, Text, Uuid
from sqlalchemy.orm import Mapped, mapped_column

from db.base import Base, JSON_VALUE, TimestampMixin


class NautilusCatalogBinding(Base, TimestampMixin):
    """Governance metadata binding a QZ Dataset Revision to a remote catalog."""

    __tablename__ = "nautilus_catalog_bindings"
    __table_args__ = (
        Index("ix_nautilus_catalog_binding_dataset", "dataset_revision_id", unique=True),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("dataset_revisions.id", ondelete="RESTRICT"),
        nullable=False,
    )
    provider: Mapped[str | None] = mapped_column(String(200))
    source_license: Mapped[str | None] = mapped_column(String(200))
    catalog_uri: Mapped[str] = mapped_column(Text, nullable=False)
    nautilus_data_type: Mapped[str] = mapped_column(String(240), nullable=False)
    instrument_scope: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    event_time_range: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE,
        nullable=False,
        default=dict,
    )
    available_time_range: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE,
        nullable=False,
        default=dict,
    )
    schema_revision: Mapped[str | None] = mapped_column(String(100))
    quality_result: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE,
        nullable=False,
        default=dict,
    )
    point_in_time_result: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE,
        nullable=False,
        default=dict,
    )
    runtime_name: Mapped[str] = mapped_column(
        String(80),
        nullable=False,
        default="NAUTILUS_TRADER",
    )
    runtime_version: Mapped[str] = mapped_column(String(40), nullable=False)
    ingested_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class QuantExperiment(Base, TimestampMixin):
    """One immutable experiment contract plus its runtime evidence."""

    __tablename__ = "quant_experiments"
    __table_args__ = (
        Index("ix_quant_experiment_program_zone", "program_id", "zone", "created_at"),
        Index("ix_quant_experiment_mission", "mission_id", "created_at"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    parent_experiment_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey("quant_experiments.id", ondelete="RESTRICT"),
    )
    mission_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_missions.id", ondelete="CASCADE"),
        nullable=False,
    )
    program_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_programs.id", ondelete="CASCADE"),
        nullable=False,
    )
    dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("dataset_revisions.id", ondelete="RESTRICT"),
        nullable=False,
    )
    zone: Mapped[str] = mapped_column(String(40), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="READY")
    runtime_name: Mapped[str] = mapped_column(
        String(80),
        nullable=False,
        default="NAUTILUS_TRADER",
    )
    runtime_version: Mapped[str | None] = mapped_column(String(40))
    strategy_artifact: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE,
        nullable=False,
        default=dict,
    )
    contract_json: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE,
        nullable=False,
        default=dict,
    )
    evidence_json: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE,
        nullable=False,
        default=dict,
    )
    disclosure_json: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE,
        nullable=False,
        default=dict,
    )
    error_code: Mapped[str | None] = mapped_column(String(100))
    error_detail: Mapped[str | None] = mapped_column(Text)
    started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    finished_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class SearchLedgerEntry(Base):
    """Append-only record of successful and failed research attempts."""

    __tablename__ = "search_ledger_entries"
    __table_args__ = (
        Index("ix_search_ledger_program", "program_id", "created_at"),
        Index("ix_search_ledger_experiment", "experiment_id"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_programs.id", ondelete="CASCADE"),
        nullable=False,
    )
    mission_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_missions.id", ondelete="CASCADE"),
        nullable=False,
    )
    experiment_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("quant_experiments.id", ondelete="CASCADE"),
        nullable=False,
    )
    attempt_kind: Mapped[str] = mapped_column(String(100), nullable=False)
    outcome: Mapped[str] = mapped_column(String(40), nullable=False)
    hypothesis_key: Mapped[str | None] = mapped_column(String(240))
    parameters: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE,
        nullable=False,
        default=dict,
    )
    evidence_summary: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE,
        nullable=False,
        default=dict,
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class SealedEvaluation(Base):
    """Deterministic disclosure from an isolated Nautilus sealed run."""

    __tablename__ = "sealed_evaluations"
    __table_args__ = (Index("ix_sealed_evaluation_experiment", "experiment_id", unique=True),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    experiment_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("quant_experiments.id", ondelete="CASCADE"),
        nullable=False,
    )
    dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("dataset_revisions.id", ondelete="RESTRICT"),
        nullable=False,
    )
    state: Mapped[str] = mapped_column(String(40), nullable=False)
    decision: Mapped[str] = mapped_column(String(40), nullable=False)
    runtime_version: Mapped[str] = mapped_column(String(40), nullable=False)
    disclosure: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE,
        nullable=False,
        default=dict,
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


__all__ = [
    "NautilusCatalogBinding",
    "QuantExperiment",
    "SearchLedgerEntry",
    "SealedEvaluation",
]
