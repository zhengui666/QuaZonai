"""Add immutable administration facts for fresh-install configuration.

Revision ID: 0018_configuration_facts
Revises: 0017_data_pit_and_true_alpha
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import JSONB


revision = "0018_configuration_facts"
down_revision = "0017_data_pit_and_true_alpha"
branch_labels = None
depends_on = None

_JSON = sa.JSON().with_variant(JSONB(), "postgresql")


def upgrade() -> None:
    op.add_column(
        "governed_data_sources",
        sa.Column(
            "connector_key",
            sa.String(length=80),
            nullable=False,
            server_default="UNSPECIFIED",
        ),
    )
    op.add_column(
        "governed_data_sources",
        sa.Column(
            "field_schema",
            _JSON,
            nullable=False,
            server_default=sa.text("'{}'"),
        ),
    )
    op.add_column(
        "governed_data_sources",
        sa.Column(
            "license_classification",
            sa.String(length=80),
            nullable=False,
            server_default="UNCLASSIFIED",
        ),
    )
    op.add_column(
        "governed_data_sources",
        sa.Column(
            "availability_semantics",
            _JSON,
            nullable=False,
            server_default=sa.text("'{}'"),
        ),
    )
    op.add_column(
        "dataset_revisions",
        sa.Column(
            "materialization_request",
            _JSON,
            nullable=False,
            server_default=sa.text("'{}'"),
        ),
    )
    op.create_table(
        "portfolio_mandate_versions",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("portfolio_mandate_id", sa.Uuid(), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("base_currency", sa.String(length=20), nullable=False),
        sa.Column("objective", sa.String(length=80), nullable=False),
        sa.Column(
            "eligible_alpha_roles", _JSON, nullable=False, server_default=sa.text("'[]'")
        ),
        sa.Column(
            "eligible_universe_version_ids",
            _JSON,
            nullable=False,
            server_default=sa.text("'[]'"),
        ),
        sa.Column("minimum_alpha_count", sa.Integer(), nullable=False, server_default="2"),
        sa.Column("capital_config", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("risk_config", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("cost_config", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("capacity_config", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("promotion_policy", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("constraint_config", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint("version_no > 0", name="ck_portfolio_mandate_version_number"),
        sa.CheckConstraint(
            "minimum_alpha_count >= 2", name="ck_portfolio_mandate_minimum_alphas"
        ),
        sa.ForeignKeyConstraint(
            ["portfolio_mandate_id"], ["portfolio_mandates.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "portfolio_mandate_id", "version_no", name="uq_portfolio_mandate_version"
        ),
    )
    op.create_index(
        "ix_portfolio_mandate_version_mandate",
        "portfolio_mandate_versions",
        ["portfolio_mandate_id", "version_no"],
    )


def downgrade() -> None:
    op.drop_index(
        "ix_portfolio_mandate_version_mandate", table_name="portfolio_mandate_versions"
    )
    op.drop_table("portfolio_mandate_versions")
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("dataset_revisions", recreate="always") as batch:
            batch.drop_column("materialization_request")
        with op.batch_alter_table("governed_data_sources", recreate="always") as batch:
            batch.drop_column("availability_semantics")
            batch.drop_column("license_classification")
            batch.drop_column("field_schema")
            batch.drop_column("connector_key")
        return
    op.drop_column("dataset_revisions", "materialization_request")
    for column in (
        "availability_semantics",
        "license_classification",
        "field_schema",
        "connector_key",
    ):
        op.drop_column("governed_data_sources", column)
