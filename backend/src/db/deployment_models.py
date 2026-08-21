"""Deployment persistence models."""

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


class Deployment(Base, TimestampMixin):
    __tablename__ = "deployments"
    __table_args__ = (
        CheckConstraint(
            "desired_state IN ('CREATED','RUNNING','STOPPED')",
            name="ck_deployment_desired_state",
        ),
        CheckConstraint(
            "observed_state IN ('CREATED','STARTING','RUNNING','STOPPING','STOPPED',"
            "'RECOVERY_BLOCKED','FAILED')",
            name="ck_deployment_observed_state",
        ),
        Index("ix_deployment_state", "desired_state", "observed_state"),
        Index("ix_deployment_funder", "funder_id"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    research_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_cases.id", ondelete="RESTRICT"), nullable=False
    )
    strategy_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("strategy_versions.id", ondelete="RESTRICT"), nullable=False
    )
    data_source_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("data_sources.id", ondelete="RESTRICT"), nullable=False
    )
    execution_connection_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("execution_connections.id", ondelete="RESTRICT"),
        nullable=False,
    )
    runtime_bundle_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("plugin_runtime_bundles.id", ondelete="RESTRICT"),
        nullable=False,
    )
    funder_id: Mapped[str] = mapped_column(String(300), nullable=False)
    desired_state: Mapped[str] = mapped_column(
        String(20), nullable=False, default="CREATED"
    )
    observed_state: Mapped[str] = mapped_column(
        String(30), nullable=False, default="CREATED"
    )
    active_revision_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey(
            "deployment_universe_revisions.id",
            ondelete="SET NULL",
            use_alter=True,
            name="fk_deployment_active_revision",
        ),
    )
    generation: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    last_error: Mapped[str | None] = mapped_column(Text)


class DeploymentGeneration(Base):
    __tablename__ = "deployment_generations"
    __table_args__ = (
        CheckConstraint(
            "state IN ('RECOVERY','RECONCILED','ARMED','STRATEGY_READY','TRADING',"
            "'STOPPING','STOPPED','RECOVERY_BLOCKED','FAILED')",
            name="ck_deployment_generation_state",
        ),
        Index("ix_deployment_generation_state", "state", "last_heartbeat_at"),
    )

    deployment_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("deployments.id", ondelete="CASCADE"), primary_key=True
    )
    generation: Mapped[int] = mapped_column(Integer, primary_key=True)
    state: Mapped[str] = mapped_column(String(30), nullable=False, default="RECOVERY")
    runner_pid: Mapped[int | None] = mapped_column(Integer)
    last_error: Mapped[str | None] = mapped_column(Text)
    started_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    last_heartbeat_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    stopped_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class DeploymentUniverseRevision(Base):
    __tablename__ = "deployment_universe_revisions"
    __table_args__ = (
        UniqueConstraint("deployment_id", "revision_no", name="uq_universe_revision_no"),
        CheckConstraint(
            "state IN ('PENDING','APPROVED','ACTIVE','SUPERSEDED','REJECTED')",
            name="ck_universe_revision_state",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    deployment_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("deployments.id", ondelete="CASCADE"), nullable=False
    )
    revision_no: Mapped[int] = mapped_column(Integer, nullable=False)
    predicate: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False)
    cap: Mapped[int] = mapped_column(Integer, nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="PENDING")
    approval_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("approvals.id", ondelete="RESTRICT")
    )
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class DeploymentInstrument(Base):
    __tablename__ = "deployment_instruments"
    __table_args__ = (
        CheckConstraint(
            "lifecycle_state IN ('PENDING','ACTIVE','EXIT_ONLY','RECOVERY_ONLY','RESOLVED')",
            name="ck_deployment_instrument_state",
        ),
        CheckConstraint("risk_limit_micros >= 0", name="ck_deployment_instrument_risk_limit"),
    )

    deployment_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("deployments.id", ondelete="CASCADE"), primary_key=True
    )
    revision_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("deployment_universe_revisions.id", ondelete="CASCADE"),
        primary_key=True,
    )
    instrument_id: Mapped[str] = mapped_column(String(300), primary_key=True)
    lifecycle_state: Mapped[str] = mapped_column(String(30), nullable=False)
    risk_limit_micros: Mapped[int] = mapped_column(BigInteger, nullable=False)
    last_reconciled_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


__all__ = [
    "Deployment",
    "DeploymentGeneration",
    "DeploymentUniverseRevision",
    "DeploymentInstrument",
]
