"""Register frozen 0009 remote archive manifests and shards.

Revision ID: 0009_archive_manifests
Revises: 0008_stale_legacy_approvals
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import JSONB

revision = "0009_archive_manifests"
down_revision = "0008_stale_legacy_approvals"
branch_labels = None
depends_on = None

_JSON = sa.JSON().with_variant(JSONB(), "postgresql")


def _tables() -> tuple[sa.Table, sa.Table]:
    metadata = sa.MetaData()
    for name in ("governed_data_sources", "market_universe_versions"):
        sa.Table(name, metadata, sa.Column("id", sa.Uuid(), primary_key=True))

    manifests = sa.Table(
        "archive_manifests",
        metadata,
        sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
        sa.Column("manifest_uri", sa.Text(), nullable=False),
        sa.Column(
            "data_source_id",
            sa.Uuid(),
            sa.ForeignKey("governed_data_sources.id", ondelete="RESTRICT"),
            nullable=False,
        ),
        sa.Column(
            "universe_version_id",
            sa.Uuid(),
            sa.ForeignKey("market_universe_versions.id", ondelete="RESTRICT"),
            nullable=False,
        ),
        sa.Column("provider", sa.String(length=200), nullable=False),
        sa.Column("source_license", sa.Text(), nullable=False),
        sa.Column("source_spec", _JSON, nullable=False),
        sa.Column("coverage_start", sa.DateTime(timezone=True), nullable=False),
        sa.Column("coverage_end", sa.DateTime(timezone=True), nullable=False),
        sa.Column("scanned_until", sa.DateTime(timezone=True), nullable=False),
        sa.Column("shard_count", sa.Integer(), nullable=False),
        sa.Column("total_bytes", sa.BigInteger(), nullable=False),
        sa.Column("missing_shard_count", sa.Integer(), nullable=False),
        sa.Column("probe_error_count", sa.Integer(), nullable=False),
        sa.Column("schema_revision", sa.String(length=100), nullable=False),
        sa.Column("state", sa.String(length=40), nullable=False),
        sa.Column("point_in_time_result", _JSON, nullable=False),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.Column(
            "updated_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.UniqueConstraint("manifest_uri", name="uq_archive_manifest_uri"),
        sa.Index("ix_archive_manifest_universe", "universe_version_id", "created_at"),
    )
    shards = sa.Table(
        "archive_manifest_shards",
        metadata,
        sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
        sa.Column(
            "manifest_id",
            sa.Uuid(),
            sa.ForeignKey("archive_manifests.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column("shard_key", sa.String(length=40), nullable=False),
        sa.Column("source_url", sa.Text(), nullable=False),
        sa.Column("coverage_start", sa.DateTime(timezone=True), nullable=False),
        sa.Column("coverage_end", sa.DateTime(timezone=True), nullable=False),
        sa.Column("size_bytes", sa.BigInteger()),
        sa.Column("state", sa.String(length=40), nullable=False),
        sa.Column("observed_at", sa.DateTime(timezone=True), nullable=False),
        sa.UniqueConstraint("manifest_id", "shard_key", name="uq_archive_manifest_shard_key"),
        sa.Index("ix_archive_manifest_shard_range", "manifest_id", "coverage_start"),
    )
    return manifests, shards


def upgrade() -> None:
    bind = op.get_bind()
    for table in _tables():
        table.create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    for table in reversed(_tables()):
        table.drop(bind=bind, checkfirst=True)
