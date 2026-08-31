"""Add revocable native operator devices.

Revision ID: 0010_mobile_operator_sessions
Revises: 0009_archive_manifests
"""

from __future__ import annotations

from alembic import op

from db.models import MobileOperatorDevice

revision = "0010_mobile_operator_sessions"
down_revision = "0009_archive_manifests"
branch_labels = None
depends_on = None


def upgrade() -> None:
    MobileOperatorDevice.__table__.create(bind=op.get_bind(), checkfirst=True)


def downgrade() -> None:
    MobileOperatorDevice.__table__.drop(bind=op.get_bind(), checkfirst=True)
