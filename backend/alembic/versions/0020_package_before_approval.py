"""Bind Approval Snapshots to prebuilt immutable Candidate Packages.

Revision ID: 0020_package_before_approval
Revises: 0019_visible_mission_turns
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "0020_package_before_approval"
down_revision = "0019_visible_mission_turns"
branch_labels = None
depends_on = None


def _drop_postgresql_constraints(table: str, column: str) -> None:
    inspector = sa.inspect(op.get_bind())
    for foreign_key in inspector.get_foreign_keys(table):
        if foreign_key.get("constrained_columns") == [column] and foreign_key.get("name"):
            op.drop_constraint(foreign_key["name"], table, type_="foreignkey")
    for unique in inspector.get_unique_constraints(table):
        if unique.get("column_names") == [column] and unique.get("name"):
            op.drop_constraint(unique["name"], table, type_="unique")
    for index in inspector.get_indexes(table):
        if index.get("column_names") == [column] and index.get("unique") and index.get("name"):
            op.drop_index(index["name"], table_name=table)


def _backfill_legacy_bindings() -> None:
    bind = op.get_bind()
    approvals = sa.table(
        "approval_snapshots",
        sa.column("id", sa.Uuid()),
        sa.column("candidate_id", sa.Uuid()),
        sa.column("candidate_package_id", sa.Uuid()),
        sa.column("candidate_package_revision", sa.Integer()),
        sa.column("state", sa.String()),
        sa.column("stale_reason", sa.Text()),
        sa.column("revision", sa.Integer()),
    )
    packages = sa.table(
        "candidate_packages",
        sa.column("id", sa.Uuid()),
        sa.column("approval_id", sa.Uuid()),
        sa.column("candidate_id", sa.Uuid()),
        sa.column("revision", sa.Integer()),
        sa.column("state", sa.String()),
    )

    for package in bind.execute(
        sa.select(
            packages.c.id,
            packages.c.approval_id,
            packages.c.candidate_id,
            packages.c.revision,
        )
    ):
        bind.execute(
            sa.update(approvals)
            .where(
                approvals.c.id == package.approval_id,
                approvals.c.candidate_id == package.candidate_id,
            )
            .values(
                candidate_package_id=package.id,
                candidate_package_revision=package.revision,
            )
        )

    bind.execute(sa.update(packages).values(state="STALE"))
    bind.execute(
        sa.update(approvals)
        .where(approvals.c.state == "PENDING")
        .values(
            state="STALE",
            stale_reason="PACKAGE_BEFORE_APPROVAL_REQUIRED",
            revision=approvals.c.revision + 1,
        )
    )


def upgrade() -> None:
    op.add_column(
        "candidate_packages",
        sa.Column("revision", sa.Integer(), nullable=False, server_default="1"),
    )
    op.add_column(
        "approval_snapshots",
        sa.Column("candidate_package_id", sa.Uuid(), nullable=True),
    )
    op.add_column(
        "approval_snapshots",
        sa.Column("candidate_package_revision", sa.Integer(), nullable=True),
    )
    _backfill_legacy_bindings()

    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("candidate_packages", recreate="always") as batch:
            batch.drop_column("approval_id")
        with op.batch_alter_table("approval_snapshots", recreate="always") as batch:
            batch.create_foreign_key(
                "fk_approval_snapshots_candidate_package_id",
                "candidate_packages",
                ["candidate_package_id"],
                ["id"],
                ondelete="RESTRICT",
            )
        return

    _drop_postgresql_constraints("candidate_packages", "approval_id")
    op.drop_column("candidate_packages", "approval_id")
    op.create_foreign_key(
        "fk_approval_snapshots_candidate_package_id",
        "approval_snapshots",
        "candidate_packages",
        ["candidate_package_id"],
        ["id"],
        ondelete="RESTRICT",
    )


def downgrade() -> None:
    # Reopening legacy pending approvals or recreating their reverse package link is unsafe.
    pass
