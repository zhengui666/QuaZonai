"""Add Nautilus-first catalog, experiment, ledger and sealed-evaluation facts.

Revision ID: 0006_nautilus_first_runtime
Revises: 0005_performance_indexes
"""

from __future__ import annotations

from alembic import op

from db.models import Base

revision = "0006_nautilus_first_runtime"
down_revision = "0005_performance_indexes"
branch_labels = None
depends_on = None

_TABLES = (
    "nautilus_catalog_bindings",
    "quant_experiments",
    "search_ledger_entries",
    "sealed_evaluations",
)


def upgrade() -> None:
    bind = op.get_bind()
    for table_name in _TABLES:
        Base.metadata.tables[table_name].create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    for table_name in reversed(_TABLES):
        Base.metadata.tables[table_name].drop(bind=bind, checkfirst=True)
