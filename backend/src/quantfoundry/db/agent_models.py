"""Agent persistence models."""

from __future__ import annotations

from datetime import datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import (
    BigInteger,
    CheckConstraint,
    DateTime,
    Index,
    String,
    Text,
    UniqueConstraint,
    Uuid,
    func,
)
from sqlalchemy.orm import Mapped, mapped_column

from quantfoundry.db.base import Base, JSON_VALUE, TimestampMixin


class OperationReceipt(Base):
    __tablename__ = "operation_receipts"
    __table_args__ = (
        UniqueConstraint(
            "actor_kind", "actor_id", "idempotency_key", name="uq_operation_receipt_key"
        ),
        CheckConstraint(
            "state IN ('IN_PROGRESS','SUCCEEDED','FAILED')", name="ck_operation_receipt_state"
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    actor_kind: Mapped[str] = mapped_column(String(40), nullable=False)
    actor_id: Mapped[str] = mapped_column(String(500), nullable=False)
    idempotency_key: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    operation_name: Mapped[str] = mapped_column(String(200), nullable=False)
    target_type: Mapped[str | None] = mapped_column(String(100))
    target_id: Mapped[UUID | None] = mapped_column(Uuid)
    normalized_arguments: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="IN_PROGRESS")
    result: Mapped[dict[str, Any] | None] = mapped_column(JSON_VALUE)
    error_code: Mapped[str | None] = mapped_column(String(100))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    completed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class AgentArtifact(Base, TimestampMixin):
    __tablename__ = "agent_artifacts"
    __table_args__ = (
        CheckConstraint(
            "kind IN ('STRATEGY_SOURCE','PLUGIN_WHEEL','PARQUET_L2')",
            name="ck_agent_artifact_kind",
        ),
        CheckConstraint(
            "state IN ('STAGING','READY','CONSUMED','FAILED','EXPIRED')",
            name="ck_agent_artifact_state",
        ),
        Index(
            "ix_agent_artifact_owner_state",
            "owner_issuer",
            "owner_subject",
            "owner_client_id",
            "state",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    owner_issuer: Mapped[str] = mapped_column(String(500), nullable=False)
    owner_subject: Mapped[str] = mapped_column(String(500), nullable=False)
    owner_client_id: Mapped[str] = mapped_column(String(500), nullable=False)
    kind: Mapped[str] = mapped_column(String(40), nullable=False)
    filename: Mapped[str] = mapped_column(String(255), nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="STAGING")
    size_declared: Mapped[int] = mapped_column(BigInteger, nullable=False)
    size_received: Mapped[int] = mapped_column(BigInteger, nullable=False, default=0)
    relative_path: Mapped[str] = mapped_column(Text, nullable=False)
    expires_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    consumed_by_type: Mapped[str | None] = mapped_column(String(100))
    consumed_by_id: Mapped[UUID | None] = mapped_column(Uuid)


class AgentImpactToken(Base):
    __tablename__ = "agent_impact_tokens"

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    issuer: Mapped[str] = mapped_column(String(500), nullable=False)
    subject: Mapped[str] = mapped_column(String(500), nullable=False)
    client_id: Mapped[str] = mapped_column(String(500), nullable=False)
    operation_name: Mapped[str] = mapped_column(String(200), nullable=False)
    target_type: Mapped[str] = mapped_column(String(100), nullable=False)
    target_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    expected_state: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False)
    impact_summary: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False)
    expires_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    consumed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class McpTaskBinding(Base):
    __tablename__ = "mcp_task_bindings"

    task_id: Mapped[str] = mapped_column(String(300), primary_key=True)
    issuer: Mapped[str] = mapped_column(String(500), nullable=False)
    subject: Mapped[str] = mapped_column(String(500), nullable=False)
    client_id: Mapped[str] = mapped_column(String(500), nullable=False)
    extension_version: Mapped[str] = mapped_column(String(100), nullable=False)
    operation_type: Mapped[str] = mapped_column(String(100), nullable=False)
    operation_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    expires_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


__all__ = [
    "OperationReceipt",
    "AgentArtifact",
    "AgentImpactToken",
    "McpTaskBinding",
]
