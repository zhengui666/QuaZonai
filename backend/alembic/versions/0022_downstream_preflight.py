"""Make downstream readiness depend on immutable preflight receipts.

Revision ID: 0022_downstream_preflight
Revises: 0021_degradation_wake_events
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "0022_downstream_preflight"
down_revision = "0021_degradation_wake_events"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.add_column(
        "downstream_systems",
        sa.Column("revision", sa.Integer(), nullable=False, server_default="1"),
    )
    # No legacy system has a receipt binding its current contracts, so none can
    # truthfully retain READY after this fail-closed migration.
    op.execute(sa.text("UPDATE downstream_systems SET preflight_state = 'PENDING'"))

    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("downstream_systems", recreate="always") as batch:
            batch.create_check_constraint("ck_downstream_system_revision", "revision > 0")
        return
    op.create_check_constraint(
        "ck_downstream_system_revision", "downstream_systems", "revision > 0"
    )


def downgrade() -> None:
    # Keep the fail-closed PENDING state; only remove this revision's schema.
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("downstream_systems", recreate="always") as batch:
            batch.drop_constraint("ck_downstream_system_revision", type_="check")
            batch.drop_column("revision")
        return
    op.drop_constraint("ck_downstream_system_revision", "downstream_systems", type_="check")
    op.drop_column("downstream_systems", "revision")
