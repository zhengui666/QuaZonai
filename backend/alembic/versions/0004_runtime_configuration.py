"""Persist operator-managed Codex and worker runtime configuration.

Revision ID: 0004_runtime_configuration
Revises: 0003_review_contracts
"""

from __future__ import annotations

from alembic import op

from db.models import RuntimeConfiguration

revision = "0004_runtime_configuration"
down_revision = "0003_review_contracts"
branch_labels = None
depends_on = None


def upgrade() -> None:
    # The project's 0001/0002 development baseline creates current Base.metadata,
    # so a fresh database may already contain this table before reaching 0004.
    # Existing databases stamped at 0003 do not. checkfirst supports both paths.
    RuntimeConfiguration.__table__.create(bind=op.get_bind(), checkfirst=True)


def downgrade() -> None:
    RuntimeConfiguration.__table__.drop(bind=op.get_bind(), checkfirst=True)
