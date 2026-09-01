"""Add the durable encrypted Operator TOTP binding.

Revision ID: 0011_operator_auth_configuration
Revises: 0010_mobile_operator_sessions
"""

from __future__ import annotations

from alembic import op

from db.models import OperatorAuthConfiguration

revision = "0011_operator_auth_configuration"
down_revision = "0010_mobile_operator_sessions"
branch_labels = None
depends_on = None


def upgrade() -> None:
    OperatorAuthConfiguration.__table__.create(bind=op.get_bind(), checkfirst=True)


def downgrade() -> None:
    OperatorAuthConfiguration.__table__.drop(bind=op.get_bind(), checkfirst=True)
