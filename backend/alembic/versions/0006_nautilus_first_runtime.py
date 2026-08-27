"""Adopt the remote Nautilus-first research runtime.

Revision ID: 0006_nautilus_first_runtime
Revises: 0005_performance_indexes
"""

from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql

revision = "0006_nautilus_first_runtime"
down_revision = "0005_performance_indexes"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.rename_table("candidate_packages", "candidate_bundles")
    op.alter_column(
        "handoff_offers",
        "candidate_package_id",
        new_column_name="candidate_bundle_id",
    )
    op.create_table(
        "search_ledger_entries",
        sa.Column("id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("program_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("branch_id", postgresql.UUID(as_uuid=True), nullable=True),
        sa.Column("mission_id", postgresql.UUID(as_uuid=True), nullable=True),
        sa.Column("dataset_revision_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("parent_entry_id", postgresql.UUID(as_uuid=True), nullable=True),
        sa.Column("mode", sa.String(length=30), nullable=False),
        sa.Column("state", sa.String(length=30), nullable=False),
        sa.Column("runtime_name", sa.String(length=80), nullable=False),
        sa.Column("runtime_version", sa.String(length=80), nullable=True),
        sa.Column("remote_run_id", sa.String(length=240), nullable=True),
        sa.Column("request_json", postgresql.JSONB(astext_type=sa.Text()), nullable=False),
        sa.Column("evidence_json", postgresql.JSONB(astext_type=sa.Text()), nullable=False),
        sa.Column("disclosure_json", postgresql.JSONB(astext_type=sa.Text()), nullable=False),
        sa.Column("failure_code", sa.String(length=100), nullable=True),
        sa.Column("failure_message", sa.Text(), nullable=True),
        sa.Column("started_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("finished_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            server_default=sa.text("now()"),
            nullable=False,
        ),
        sa.ForeignKeyConstraint(["branch_id"], ["research_branches.id"], ondelete="SET NULL"),
        sa.ForeignKeyConstraint(
            ["dataset_revision_id"], ["dataset_revisions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(["mission_id"], ["research_missions.id"], ondelete="SET NULL"),
        sa.ForeignKeyConstraint(
            ["parent_entry_id"], ["search_ledger_entries.id"], ondelete="SET NULL"
        ),
        sa.ForeignKeyConstraint(["program_id"], ["research_programs.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index(
        "ix_search_ledger_program_created", "search_ledger_entries", ["program_id", "created_at"]
    )
    op.create_index(
        "ix_search_ledger_mission_state", "search_ledger_entries", ["mission_id", "state"]
    )
    for name, type_ in (
        ("provider_name", sa.String(length=200)),
        ("source_license", sa.Text()),
        ("catalog_uri", sa.Text()),
        ("nautilus_data_type", sa.String(length=100)),
        ("schema_revision", sa.String(length=128)),
    ):
        op.add_column("dataset_revisions", sa.Column(name, type_, nullable=True))
    op.add_column(
        "dataset_revisions",
        sa.Column(
            "instrument_scope",
            postgresql.JSONB(astext_type=sa.Text()),
            server_default=sa.text("'[]'::jsonb"),
            nullable=False,
        ),
    )
    op.add_column(
        "dataset_revisions",
        sa.Column(
            "quality_result",
            postgresql.JSONB(astext_type=sa.Text()),
            server_default=sa.text("'{}'::jsonb"),
            nullable=False,
        ),
    )
    op.add_column(
        "dataset_revisions",
        sa.Column(
            "point_in_time_result",
            postgresql.JSONB(astext_type=sa.Text()),
            server_default=sa.text("'{}'::jsonb"),
            nullable=False,
        ),
    )
    op.add_column(
        "dataset_revisions", sa.Column("ingested_at", sa.DateTime(timezone=True), nullable=True)
    )
    op.add_column(
        "alpha_qualifications",
        sa.Column("source_experiment_id", postgresql.UUID(as_uuid=True), nullable=True),
    )
    op.create_foreign_key(
        "fk_alpha_qualification_source_experiment",
        "alpha_qualifications",
        "search_ledger_entries",
        ["source_experiment_id"],
        ["id"],
        ondelete="RESTRICT",
    )
    op.add_column(
        "portfolio_candidates",
        sa.Column("simulation_experiment_id", postgresql.UUID(as_uuid=True), nullable=True),
    )
    op.create_foreign_key(
        "fk_portfolio_candidate_simulation_experiment",
        "portfolio_candidates",
        "search_ledger_entries",
        ["simulation_experiment_id"],
        ["id"],
        ondelete="RESTRICT",
    )


def downgrade() -> None:
    op.alter_column(
        "handoff_offers",
        "candidate_bundle_id",
        new_column_name="candidate_package_id",
    )
    op.rename_table("candidate_bundles", "candidate_packages")
    op.drop_constraint(
        "fk_portfolio_candidate_simulation_experiment", "portfolio_candidates", type_="foreignkey"
    )
    op.drop_column("portfolio_candidates", "simulation_experiment_id")
    op.drop_constraint(
        "fk_alpha_qualification_source_experiment", "alpha_qualifications", type_="foreignkey"
    )
    op.drop_column("alpha_qualifications", "source_experiment_id")
    for column in (
        "ingested_at",
        "point_in_time_result",
        "quality_result",
        "instrument_scope",
        "schema_revision",
        "nautilus_data_type",
        "catalog_uri",
        "source_license",
        "provider_name",
    ):
        op.drop_column("dataset_revisions", column)
    op.drop_index("ix_search_ledger_mission_state", table_name="search_ledger_entries")
    op.drop_index("ix_search_ledger_program_created", table_name="search_ledger_entries")
    op.drop_table("search_ledger_entries")
