"""Research/data plugin persistence models."""

from __future__ import annotations

from datetime import datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import (
    Boolean,
    CheckConstraint,
    DateTime,
    ForeignKey,
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

from db.base import Base, JSON_VALUE, TimestampMixin


class PluginRelease(Base):
    __tablename__ = "plugin_releases"
    __table_args__ = (
        UniqueConstraint("plugin_id", "version", name="uq_plugin_release_version"),
        CheckConstraint(
            "state IN ('RECEIVED','INSTALLING','VALIDATING','STAGED','ACTIVE',"
            "'DRAINING','INACTIVE','REMOVING','REMOVED','FAILED')",
            name="ck_plugin_release_state",
        ),
        Index("ix_plugin_release_lookup", "plugin_id", "state", "is_default"),
        Index(
            "uq_plugin_release_default",
            "plugin_id",
            unique=True,
            postgresql_where=text("is_default"),
            sqlite_where=text("is_default"),
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    plugin_id: Mapped[str] = mapped_column(String(200), nullable=False)
    distribution_name: Mapped[str] = mapped_column(String(200), nullable=False)
    version: Mapped[str] = mapped_column(String(100), nullable=False)
    api_version: Mapped[str] = mapped_column(String(50), nullable=False)
    state: Mapped[str] = mapped_column(String(32), nullable=False, default="RECEIVED")
    is_default: Mapped[bool] = mapped_column(Boolean, nullable=False, default=False)
    descriptor_snapshot: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    last_error: Mapped[str | None] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    activated_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    removed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class PluginArtifact(Base):
    __tablename__ = "plugin_artifacts"
    __table_args__ = (
        CheckConstraint("role IN ('PRIMARY','DEPENDENCY')", name="ck_plugin_artifact_role"),
        UniqueConstraint("plugin_release_id", "filename", name="uq_plugin_artifact_filename"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    plugin_release_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("plugin_releases.id", ondelete="RESTRICT"), nullable=False
    )
    role: Mapped[str] = mapped_column(String(20), nullable=False)
    filename: Mapped[str] = mapped_column(String(255), nullable=False)
    relative_path: Mapped[str] = mapped_column(Text, nullable=False)
    package_name: Mapped[str] = mapped_column(String(200), nullable=False)
    package_version: Mapped[str] = mapped_column(String(100), nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class PluginRuntimeBundle(Base):
    __tablename__ = "plugin_runtime_bundles"
    __table_args__ = (
        CheckConstraint(
            "state IN ('BUILDING','READY','FAILED','STALE','REMOVED')",
            name="ck_plugin_runtime_bundle_state",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="BUILDING")
    python_version: Mapped[str] = mapped_column(String(50), nullable=False)
    qf_version: Mapped[str] = mapped_column(String(50), nullable=False)
    environment_path: Mapped[str] = mapped_column(Text, nullable=False)
    last_error: Mapped[str | None] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    ready_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    removed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class PluginRuntimeBundleMember(Base):
    __tablename__ = "plugin_runtime_bundle_members"
    __table_args__ = (
        CheckConstraint(
            "member_role IN ('RESEARCH','DATA','IMPORTER','AUXILIARY')",
            name="ck_plugin_bundle_member_role",
        ),
        Index("ix_plugin_bundle_member_release", "plugin_release_id"),
    )

    runtime_bundle_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("plugin_runtime_bundles.id", ondelete="CASCADE"), primary_key=True
    )
    plugin_release_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("plugin_releases.id", ondelete="RESTRICT"), primary_key=True
    )
    member_role: Mapped[str] = mapped_column(String(20), primary_key=True)


class CredentialSet(Base, TimestampMixin):
    __tablename__ = "credential_sets"
    __table_args__ = (UniqueConstraint("plugin_release_id", "name", name="uq_credential_set_name"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    plugin_release_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("plugin_releases.id", ondelete="RESTRICT"), nullable=False
    )
    name: Mapped[str] = mapped_column(String(200), nullable=False)
    public_config: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)


class CredentialSecret(Base):
    __tablename__ = "credential_secrets"

    credential_set_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("credential_sets.id", ondelete="CASCADE"), primary_key=True
    )
    field_name: Mapped[str] = mapped_column(String(200), primary_key=True)
    ciphertext: Mapped[bytes] = mapped_column(LargeBinary, nullable=False)
    nonce: Mapped[bytes] = mapped_column(LargeBinary, nullable=False)
    key_version: Mapped[int] = mapped_column(Integer, nullable=False, default=1)


__all__ = [
    "PluginRelease",
    "PluginArtifact",
    "PluginRuntimeBundle",
    "PluginRuntimeBundleMember",
    "CredentialSet",
    "CredentialSecret",
]
