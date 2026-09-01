"""Add the durable encrypted Operator TOTP binding.

Revision ID: 0010_operator_auth_configuration
Revises: 0009_archive_manifests
"""

from __future__ import annotations

from alembic import op

from db.models import OperatorAuthConfiguration, OperatorAuthInitialization

revision = "0010_operator_auth_configuration"
down_revision = "0009_archive_manifests"
branch_labels = None
depends_on = None


def upgrade() -> None:
    OperatorAuthInitialization.__table__.create(bind=op.get_bind(), checkfirst=True)
    OperatorAuthConfiguration.__table__.create(bind=op.get_bind(), checkfirst=True)


def downgrade() -> None:
    OperatorAuthConfiguration.__table__.drop(bind=op.get_bind(), checkfirst=True)
    OperatorAuthInitialization.__table__.drop(bind=op.get_bind(), checkfirst=True)
