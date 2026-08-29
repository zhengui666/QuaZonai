"""Add Nautilus-first remote runtime governance tables.

Revision ID: 0006_nautilus_remote_runtime
Revises: 0005_performance_indexes
"""

from __future__ import annotations

import sqlalchemy as sa
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


def _create_legacy_evaluation_episodes(bind: sa.Connection) -> None:
    inspector = sa.inspect(bind)
    if "evaluation_episodes" in inspector.get_table_names():
        return
    source = Base.metadata.tables["evaluation_episodes"]
    metadata = sa.MetaData()
    table = sa.Table(
        "evaluation_episodes",
        metadata,
        *(
            column.copy()
            for column in source.columns
            if column.name != "sealed_dataset_revision_id"
        ),
    )
    table.create(bind=bind, checkfirst=True)
    if "ix_evaluation_episode_program" not in {
        str(index["name"])
        for index in inspector.get_indexes("evaluation_episodes")
        if index.get("name")
    }:
        op.create_index(
            "ix_evaluation_episode_program",
            "evaluation_episodes",
            ["program_id", "state"],
        )


def upgrade() -> None:
    bind = op.get_bind()
    for name in _TABLES:
        if name == "evaluation_episodes":
            _create_legacy_evaluation_episodes(bind)
        else:
            Base.metadata.tables[name].create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    for name in reversed(_TABLES):
        Base.metadata.tables[name].drop(bind=bind, checkfirst=True)
