"""Freeze sealed dataset and capital context promotion inputs."""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op

revision = "0007_promotion_binding_hardening"
down_revision = "0006_nautilus_remote_runtime"
branch_labels = None
depends_on = None


def upgrade() -> None:
    bind = op.get_bind()
    inspector = sa.inspect(bind)
    evaluation_columns = {column["name"] for column in inspector.get_columns("evaluation_episodes")}
    if "sealed_dataset_revision_id" not in evaluation_columns:
        if bind.dialect.name == "sqlite":
            with op.batch_alter_table("evaluation_episodes", recreate="always") as batch:
                batch.add_column(sa.Column("sealed_dataset_revision_id", sa.Uuid(), nullable=True))
                batch.create_foreign_key(
                    "fk_evaluation_episodes_sealed_dataset_revision_id",
                    "dataset_revisions",
                    ["sealed_dataset_revision_id"],
                    ["id"],
                    ondelete="RESTRICT",
                )
        else:
            op.add_column(
                "evaluation_episodes",
                sa.Column(
                    "sealed_dataset_revision_id",
                    sa.Uuid(),
                    sa.ForeignKey(
                        "dataset_revisions.id",
                        name="fk_evaluation_episodes_sealed_dataset_revision_id",
                        ondelete="RESTRICT",
                    ),
                    nullable=True,
                ),
            )
    run_columns = {column["name"] for column in inspector.get_columns("quant_runtime_runs")}
    if "promotion_gate" in run_columns:
        if bind.dialect.name == "sqlite":
            with op.batch_alter_table("quant_runtime_runs", recreate="always") as batch:
                batch.drop_column("promotion_gate")
        else:
            op.drop_column("quant_runtime_runs", "promotion_gate")
    if "capital_context_versions" not in inspector.get_table_names():
        op.create_table(
            "capital_context_versions",
            sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
            sa.Column("source_type", sa.String(length=40), nullable=False),
            sa.Column("source_downstream_system_id", sa.Uuid(), nullable=True),
            sa.Column("base_currency", sa.String(length=20), nullable=False),
            sa.Column("deployable_capital", sa.Numeric(38, 12), nullable=False),
            sa.Column("observed_at", sa.DateTime(timezone=True), nullable=False),
            sa.Column("valid_until", sa.DateTime(timezone=True), nullable=False),
            sa.Column("notes", sa.Text(), nullable=True),
            sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
            sa.Column("updated_at", sa.DateTime(timezone=True), nullable=False),
            sa.ForeignKeyConstraint(
                ["source_downstream_system_id"],
                ["downstream_systems.id"],
                name="fk_capital_context_versions_source_downstream_system_id",
                ondelete="RESTRICT",
            ),
        )
    if "ix_capital_context_validity" not in {
        str(index["name"])
        for index in sa.inspect(bind).get_indexes("capital_context_versions")
        if index.get("name")
    }:
        op.create_index(
            "ix_capital_context_validity",
            "capital_context_versions",
            ["valid_until", "observed_at"],
        )


def downgrade() -> None:
    bind = op.get_bind()
    inspector = sa.inspect(bind)
    if "capital_context_versions" in inspector.get_table_names():
        indexes = {
            str(index["name"])
            for index in inspector.get_indexes("capital_context_versions")
            if index.get("name")
        }
        if "ix_capital_context_validity" in indexes:
            op.drop_index("ix_capital_context_validity", table_name="capital_context_versions")
        op.drop_table("capital_context_versions")
    if "sealed_dataset_revision_id" in {
        column["name"] for column in inspector.get_columns("evaluation_episodes")
    }:
        if bind.dialect.name == "sqlite":
            with op.batch_alter_table("evaluation_episodes", recreate="always") as batch:
                batch.drop_constraint(
                    "fk_evaluation_episodes_sealed_dataset_revision_id",
                    type_="foreignkey",
                )
                batch.drop_column("sealed_dataset_revision_id")
        else:
            op.drop_column("evaluation_episodes", "sealed_dataset_revision_id")
