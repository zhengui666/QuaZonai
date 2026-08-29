"""Runtime persistence models."""

from __future__ import annotations

from datetime import datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import (
    BigInteger,
    CheckConstraint,
    DateTime,
    Float,
    Index,
    Integer,
    LargeBinary,
    String,
    Text,
    UniqueConstraint,
    Uuid,
    func,
    text,
)
from sqlalchemy.orm import Mapped, mapped_column

from db.base import Base, IDENTITY_INT, JSON_VALUE, TimestampMixin


class RuntimeConfiguration(Base, TimestampMixin):
    """Singleton operator-managed Codex and worker runtime configuration."""

    __tablename__ = "runtime_configurations"
    __table_args__ = (
        CheckConstraint("scope = 'SYSTEM'", name="ck_runtime_configuration_scope"),
        CheckConstraint("revision > 0", name="ck_runtime_configuration_revision"),
        CheckConstraint(
            "max_plugin_wheel_bytes > 0 AND max_plugin_wheel_bytes <= 1073741824",
            name="ck_runtime_max_plugin_wheel_bytes",
        ),
        CheckConstraint(
            "plugin_validation_timeout_seconds > 0 AND plugin_validation_timeout_seconds <= 86400",
            name="ck_runtime_plugin_validation_timeout",
        ),
        CheckConstraint(
            "bundle_build_timeout_seconds > 0 AND bundle_build_timeout_seconds <= 86400",
            name="ck_runtime_bundle_build_timeout",
        ),
        CheckConstraint(
            "plugin_job_timeout_seconds > 0 AND plugin_job_timeout_seconds <= 86400",
            name="ck_runtime_plugin_job_timeout",
        ),
        CheckConstraint(
            "mission_job_timeout_seconds > 0 AND mission_job_timeout_seconds <= 86400",
            name="ck_runtime_mission_job_timeout",
        ),
        CheckConstraint(
            "job_poll_seconds >= 0.01 AND job_poll_seconds <= 3600",
            name="ck_runtime_job_poll_seconds",
        ),
        CheckConstraint(
            "job_lease_seconds > 0 AND job_lease_seconds <= 86400",
            name="ck_runtime_job_lease_seconds",
        ),
        UniqueConstraint("scope", name="uq_runtime_configuration_scope"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    scope: Mapped[str] = mapped_column(String(40), nullable=False, default="SYSTEM")
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    codex_model: Mapped[str | None] = mapped_column(String(200))
    codex_base_url: Mapped[str | None] = mapped_column(Text)
    codex_api_key_ciphertext: Mapped[bytes | None] = mapped_column(LargeBinary)
    codex_api_key_nonce: Mapped[bytes | None] = mapped_column(LargeBinary)
    codex_api_key_key_version: Mapped[int | None] = mapped_column(Integer)
    max_plugin_wheel_bytes: Mapped[int] = mapped_column(BigInteger, nullable=False)
    plugin_validation_timeout_seconds: Mapped[int] = mapped_column(Integer, nullable=False)
    bundle_build_timeout_seconds: Mapped[int] = mapped_column(Integer, nullable=False)
    plugin_job_timeout_seconds: Mapped[int] = mapped_column(Integer, nullable=False)
    mission_job_timeout_seconds: Mapped[int] = mapped_column(Integer, nullable=False)
    job_poll_seconds: Mapped[float] = mapped_column(Float, nullable=False)
    job_lease_seconds: Mapped[int] = mapped_column(Integer, nullable=False)


class Job(Base, TimestampMixin):
    __tablename__ = "jobs"
    __table_args__ = (
        CheckConstraint(
            "state IN ('READY','LEASED','SUCCEEDED','FAILED','CANCELLED')",
            name="ck_job_state",
        ),
        Index("ix_jobs_ready", "state", "available_at"),
        Index(
            "ix_jobs_ready_queue",
            "available_at",
            "created_at",
            postgresql_where=text("state = 'READY'"),
        ),
        Index(
            "ix_jobs_leased_expiry",
            "lease_expires_at",
            postgresql_where=text("state = 'LEASED'"),
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    kind: Mapped[str] = mapped_column(String(100), nullable=False)
    resource_type: Mapped[str] = mapped_column(String(100), nullable=False)
    resource_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="READY")
    payload: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    attempt: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    available_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    lease_owner: Mapped[str | None] = mapped_column(String(200))
    lease_expires_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    last_error: Mapped[str | None] = mapped_column(Text)


class Event(Base):
    __tablename__ = "events"
    __table_args__ = (
        Index("ix_events_id", "id"),
        Index("ix_events_aggregate_activity", "aggregate_type", "aggregate_id", "id"),
    )

    id: Mapped[int] = mapped_column(IDENTITY_INT, primary_key=True, autoincrement=True)
    kind: Mapped[str] = mapped_column(String(100), nullable=False)
    aggregate_type: Mapped[str] = mapped_column(String(100), nullable=False)
    aggregate_id: Mapped[UUID | None] = mapped_column(Uuid)
    actor_kind: Mapped[str] = mapped_column(String(40), nullable=False, default="SYSTEM")
    actor_metadata: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    payload: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


__all__ = ["RuntimeConfiguration", "Job", "Event"]
