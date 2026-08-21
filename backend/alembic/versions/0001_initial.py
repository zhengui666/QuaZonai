"""Fresh QuantFoundry control-plane baseline.

Revision ID: 0001_initial
Revises: None
"""

from __future__ import annotations

from alembic import op

from quantfoundry.db.models import Base

revision = "0001_initial"
down_revision = None
branch_labels = None
depends_on = None


def upgrade() -> None:
    Base.metadata.create_all(bind=op.get_bind(), checkfirst=False)


def downgrade() -> None:
    Base.metadata.drop_all(bind=op.get_bind(), checkfirst=False)
