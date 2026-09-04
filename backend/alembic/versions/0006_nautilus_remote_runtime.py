"""Add frozen Nautilus-first remote runtime governance tables.

Revision ID: 0006_nautilus_remote_runtime
Revises: 0005_performance_indexes
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import JSONB

revision = "0006_nautilus_remote_runtime"
down_revision = "0005_performance_indexes"
branch_labels = None
depends_on = None

_JSON = sa.JSON().with_variant(JSONB(), "postgresql")


def _tables() -> tuple[sa.Table, ...]:
    metadata = sa.MetaData()
    for name in ("dataset_revisions", "research_programs", "research_branches", "research_missions"):
        sa.Table(name, metadata, sa.Column("id", sa.Uuid(), primary_key=True))

    nautilus_catalog_bindings = sa.Table(
        "nautilus_catalog_bindings",
        metadata,
        sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
        sa.Column(
            "dataset_revision_id",
            sa.Uuid(),
            sa.ForeignKey("dataset_revisions.id", ondelete="CASCADE"),
            nullable=False,
            unique=True,
        ),
        sa.Column("catalog_uri", sa.Text(), nullable=False),
        sa.Column("provider", sa.String(length=200), nullable=False),
        sa.Column("source_license", sa.Text(), nullable=False),
        sa.Column("nautilus_data_type", sa.String(length=200), nullable=False),
        sa.Column("instrument_scope", _JSON, nullable=False),
        sa.Column("event_time_range", _JSON, nullable=False),
        sa.Column("available_time_range", _JSON, nullable=False),
        sa.Column("schema_revision", sa.String(length=100), nullable=False),
        sa.Column("quality_state", sa.String(length=40), nullable=False),
        sa.Column("quality_result", _JSON, nullable=False),
        sa.Column("point_in_time_state", sa.String(length=40), nullable=False),
        sa.Column("point_in_time_result", _JSON, nullable=False),
        sa.Column("sealed", sa.Boolean(), nullable=False),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.Column(
            "updated_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.UniqueConstraint("catalog_uri", name="uq_nautilus_catalog_uri"),
        sa.Index("ix_nautilus_catalog_sealed", "sealed", "quality_state"),
    )
    quant_runtime_runs = sa.Table(
        "quant_runtime_runs",
        metadata,
        sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
        sa.Column(
            "program_id",
            sa.Uuid(),
            sa.ForeignKey("research_programs.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column(
            "branch_id",
            sa.Uuid(),
            sa.ForeignKey("research_branches.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column("mission_id", sa.Uuid(), sa.ForeignKey("research_missions.id", ondelete="SET NULL")),
        sa.Column("evaluation_episode_id", sa.Uuid()),
        sa.Column("parent_run_id", sa.Uuid(), sa.ForeignKey("quant_runtime_runs.id", ondelete="SET NULL")),
        sa.Column("mode", sa.String(length=40), nullable=False),
        sa.Column("state", sa.String(length=40), nullable=False),
        sa.Column("external_run_id", sa.String(length=200)),
        sa.Column("experiment_key", sa.String(length=200), nullable=False),
        sa.Column("family", sa.String(length=200), nullable=False),
        sa.Column("catalog_uri", sa.Text(), nullable=False),
        sa.Column("runtime_name", sa.String(length=100), nullable=False),
        sa.Column("runtime_version", sa.String(length=100)),
        sa.Column("contract_version", sa.String(length=40)),
        sa.Column("strategy_artifact", _JSON, nullable=False),
        sa.Column("parameters", _JSON, nullable=False),
        sa.Column("promotion_gate", _JSON, nullable=False),
        sa.Column("evidence", _JSON, nullable=False),
        sa.Column("error_code", sa.String(length=100)),
        sa.Column("error_message", sa.Text()),
        sa.Column("started_at", sa.DateTime(timezone=True)),
        sa.Column("finished_at", sa.DateTime(timezone=True)),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.Column(
            "updated_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.UniqueConstraint("runtime_name", "external_run_id", name="uq_quant_runtime_external_run"),
        sa.Index("ix_quant_runtime_run_program", "program_id", "created_at"),
        sa.Index("ix_quant_runtime_run_mission", "mission_id", "created_at"),
    )
    evaluation_episodes = sa.Table(
        "evaluation_episodes",
        metadata,
        sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
        sa.Column(
            "program_id",
            sa.Uuid(),
            sa.ForeignKey("research_programs.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column(
            "branch_id",
            sa.Uuid(),
            sa.ForeignKey("research_branches.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column(
            "discovery_run_id",
            sa.Uuid(),
            sa.ForeignKey("quant_runtime_runs.id", ondelete="RESTRICT"),
            nullable=False,
        ),
        sa.Column("sealed_run_id", sa.Uuid(), sa.ForeignKey("quant_runtime_runs.id", ondelete="SET NULL")),
        sa.Column("state", sa.String(length=40), nullable=False),
        sa.Column("disclosure", _JSON, nullable=False),
        sa.Column("failure_code", sa.String(length=100)),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.Column(
            "updated_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.Index("ix_evaluation_episode_program", "program_id", "state"),
    )
    search_ledger_entries = sa.Table(
        "search_ledger_entries",
        metadata,
        sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
        sa.Column(
            "program_id",
            sa.Uuid(),
            sa.ForeignKey("research_programs.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column(
            "branch_id",
            sa.Uuid(),
            sa.ForeignKey("research_branches.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column("mission_id", sa.Uuid(), sa.ForeignKey("research_missions.id", ondelete="SET NULL")),
        sa.Column(
            "run_id",
            sa.Uuid(),
            sa.ForeignKey("quant_runtime_runs.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column("family", sa.String(length=200), nullable=False),
        sa.Column("parameters", _JSON, nullable=False),
        sa.Column("outcome", sa.String(length=40), nullable=False),
        sa.Column("failure_code", sa.String(length=100)),
        sa.Column("disclosure_level", sa.String(length=40), nullable=False),
        sa.Column("evidence_summary", _JSON, nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.UniqueConstraint("run_id", name="uq_search_ledger_run"),
        sa.Index("ix_search_ledger_program", "program_id", "created_at"),
    )
    return nautilus_catalog_bindings, quant_runtime_runs, evaluation_episodes, search_ledger_entries


def _create_legacy_evaluation_episodes(bind: sa.Connection, table: sa.Table) -> None:
    inspector = sa.inspect(bind)
    if not inspector.has_table(table.name):
        table.create(bind=bind, checkfirst=True)
    if "ix_evaluation_episode_program" not in {
        str(index["name"])
        for index in inspector.get_indexes(table.name)
        if index.get("name")
    }:
        op.create_index("ix_evaluation_episode_program", table.name, ["program_id", "state"])


def upgrade() -> None:
    bind = op.get_bind()
    for table in _tables():
        if table.name == "evaluation_episodes":
            _create_legacy_evaluation_episodes(bind, table)
        else:
            table.create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    for table in reversed(_tables()):
        table.drop(bind=bind, checkfirst=True)
