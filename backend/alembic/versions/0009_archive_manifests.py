"""Register remote archive manifests and their deterministic shards.

Revision ID: 0009_archive_manifests
Revises: 0008_stale_legacy_approvals
"""

from __future__ import annotations

from alembic import op

from db.models import ArchiveManifest, ArchiveManifestShard

revision = "0009_archive_manifests"
down_revision = "0008_stale_legacy_approvals"
branch_labels = None
depends_on = None


def upgrade() -> None:
    bind = op.get_bind()
    ArchiveManifest.__table__.create(bind=bind, checkfirst=True)
    ArchiveManifestShard.__table__.create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    ArchiveManifestShard.__table__.drop(bind=bind, checkfirst=True)
    ArchiveManifest.__table__.drop(bind=bind, checkfirst=True)
