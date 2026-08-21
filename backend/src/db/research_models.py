"""Research persistence models."""

from __future__ import annotations

from datetime import datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import (
    BigInteger,
    CheckConstraint,
    DateTime,
    ForeignKey,
    Index,
    Integer,
    String,
    Text,
    UniqueConstraint,
    Uuid,
    func,
)
from sqlalchemy.orm import Mapped, mapped_column

from db.base import Base, JSON_VALUE, TimestampMixin


class Strategy(Base):
    __tablename__ = "strategies"

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    name: Mapped[str] = mapped_column(String(200), nullable=False, unique=True)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class StrategyVersion(Base):
    __tablename__ = "strategy_versions"
    __table_args__ = (
        UniqueConstraint("strategy_id", "version_no", name="uq_strategy_version_no"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    strategy_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("strategies.id", ondelete="CASCADE"), nullable=False
    )
    version_no: Mapped[int] = mapped_column(Integer, nullable=False)
    source_text: Mapped[str] = mapped_column(Text, nullable=False)
    default_config: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    objective_directions: Mapped[list[str]] = mapped_column(
        JSON_VALUE, nullable=False, default=list
    )
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class ResearchCase(Base, TimestampMixin):
    __tablename__ = "research_cases"
    __table_args__ = (
        CheckConstraint(
            "state IN ('DRAFT','ACTIVE','REVIEW','CLOSED')",
            name="ck_research_case_state",
        ),
        Index("ix_research_case_state", "state", "updated_at"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    title: Mapped[str] = mapped_column(String(300), nullable=False)
    strategy_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("strategy_versions.id", ondelete="RESTRICT")
    )
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="DRAFT")
    content_revision: Mapped[int] = mapped_column(Integer, nullable=False, default=0)


class ResearchSectionRevision(Base):
    __tablename__ = "research_section_revisions"
    __table_args__ = (
        UniqueConstraint(
            "research_id", "section", "revision_no", name="uq_research_section_revision"
        ),
        CheckConstraint(
            "section IN ('HYPOTHESIS','MARKET_CONTEXT','DATA','METHOD','RESULTS',"
            "'RISKS','CONCLUSION')",
            name="ck_research_section",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    research_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_cases.id", ondelete="CASCADE"), nullable=False
    )
    section: Mapped[str] = mapped_column(String(40), nullable=False)
    revision_no: Mapped[int] = mapped_column(Integer, nullable=False)
    markdown: Mapped[str] = mapped_column(Text, nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class Experiment(Base):
    __tablename__ = "experiments"
    __table_args__ = (
        CheckConstraint("train_start < train_end", name="ck_experiment_train_range"),
        CheckConstraint("holdout_start < holdout_end", name="ck_experiment_holdout_range"),
        CheckConstraint(
            "train_end <= holdout_start OR holdout_end <= train_start",
            name="ck_experiment_non_overlapping_ranges",
        ),
        Index("ix_experiment_research", "research_id", "id"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    research_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_cases.id", ondelete="RESTRICT"), nullable=False
    )
    strategy_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("strategy_versions.id", ondelete="RESTRICT"), nullable=False
    )
    dataset_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("catalog_datasets.id", ondelete="RESTRICT"), nullable=False
    )
    runtime_bundle_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("plugin_runtime_bundles.id", ondelete="RESTRICT"),
        nullable=False,
    )
    train_start: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    train_end: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    holdout_start: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    holdout_end: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    seed: Mapped[int] = mapped_column(Integer, nullable=False)
    objective_directions: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False)
    optuna_study_name: Mapped[str | None] = mapped_column(String(300), unique=True)
    selected_trial_no: Mapped[int | None] = mapped_column(Integer)


class Run(Base):
    __tablename__ = "runs"
    __table_args__ = (
        CheckConstraint(
            "type IN ('PARQUET_IMPORT','BACKTEST','OPTIMIZATION','HOLDOUT')",
            name="ck_run_type",
        ),
        CheckConstraint(
            "state IN ('QUEUED','RUNNING','SUCCEEDED','FAILED','CANCELLED')",
            name="ck_run_state",
        ),
        Index("ix_runs_experiment_state", "experiment_id", "state"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    experiment_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("experiments.id", ondelete="RESTRICT")
    )
    runtime_bundle_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("plugin_runtime_bundles.id", ondelete="RESTRICT")
    )
    type: Mapped[str] = mapped_column(String(30), nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="QUEUED")
    attempt: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    summary: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    error_code: Mapped[str | None] = mapped_column(String(100))
    error_message: Mapped[str | None] = mapped_column(Text)
    started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    finished_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class Report(Base):
    __tablename__ = "reports"
    __table_args__ = (UniqueConstraint("run_id", "kind", name="uq_report_run_kind"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    run_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("runs.id", ondelete="CASCADE"), nullable=False
    )
    kind: Mapped[str] = mapped_column(String(100), nullable=False)
    relative_path: Mapped[str] = mapped_column(Text, nullable=False)
    media_type: Mapped[str] = mapped_column(String(200), nullable=False)
    row_count: Mapped[int | None] = mapped_column(BigInteger)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class Approval(Base):
    __tablename__ = "approvals"
    __table_args__ = (
        CheckConstraint(
            "type IN ('DEPLOYMENT_START','UNIVERSE_EXPANSION')",
            name="ck_approval_type",
        ),
        CheckConstraint(
            "state IN ('PENDING','APPROVED','REJECTED')",
            name="ck_approval_state",
        ),
        Index("ix_approval_resource_state", "resource_type", "resource_id", "state"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    type: Mapped[str] = mapped_column(String(40), nullable=False)
    resource_type: Mapped[str] = mapped_column(String(100), nullable=False)
    resource_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    scope: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="PENDING")
    reason: Mapped[str | None] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    decided_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


__all__ = [
    "Strategy",
    "StrategyVersion",
    "ResearchCase",
    "ResearchSectionRevision",
    "Experiment",
    "Run",
    "Report",
    "Approval",
]
