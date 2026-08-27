"""Add Nautilus-first remote runtime governance tables.

Revision ID: 0006_nautilus_remote_runtime
Revises: 0005_performance_indexes
"""

from __future__ import annotations

from alembic import op

from db.models import Base

revision = "0006_nautilus_remote_runtime"
down_revision = "0005_performance_indexes"
branch_labels = None
depends_on = None

_TABLES = (
    "nautilus_catalog_bindings",
    "quant_runtime_runs",
    "evaluation_episodes",
    "search_ledger_entries",
)


def upgrade() -> None:
    bind = op.get_bind()
    for name in _TABLES:
        Base.metadata.tables[name].create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    for name in reversed(_TABLES):
        Base.metadata.tables[name].drop(bind=bind, checkfirst=True)
