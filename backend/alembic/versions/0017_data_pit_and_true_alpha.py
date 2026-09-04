"""Add point-in-time Dataset facts and durable true-Alpha facts.

Revision ID: 0017_data_pit_and_true_alpha
Revises: 0016_research_agent_domain
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import JSONB


revision = "0017_data_pit_and_true_alpha"
down_revision = "0016_research_agent_domain"
branch_labels = None
depends_on = None

_JSON = sa.JSON().with_variant(JSONB(), "postgresql")


def _create_data_quality_results() -> None:
    op.create_table(
        "data_quality_results",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("dataset_revision_id", sa.Uuid(), nullable=False),
        sa.Column("check_kind", sa.String(length=40), nullable=False),
        sa.Column("revision_no", sa.Integer(), nullable=False, server_default="1"),
        sa.Column("state", sa.String(length=40), nullable=False),
        sa.Column("summary", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("checker_version", sa.String(length=80), nullable=False),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint("revision_no > 0", name="ck_data_quality_result_revision"),
        sa.CheckConstraint(
            "check_kind IN ('QUALITY', 'POINT_IN_TIME')",
            name="ck_data_quality_result_kind",
        ),
        sa.CheckConstraint(
            "state IN ('VALID', 'INVALID')",
            name="ck_data_quality_result_state",
        ),
        sa.ForeignKeyConstraint(
            ["dataset_revision_id"], ["dataset_revisions.id"], ondelete="CASCADE"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "dataset_revision_id",
            "check_kind",
            "revision_no",
            name="uq_data_quality_result_revision",
        ),
    )
    op.create_index(
        "ix_data_quality_result_dataset",
        "data_quality_results",
        ["dataset_revision_id", "check_kind"],
    )


def _extend_dataset_revisions() -> None:
    op.add_column("dataset_revisions", sa.Column("data_class", sa.String(length=20)))
    op.add_column("dataset_revisions", sa.Column("origin", sa.String(length=200)))
    op.add_column(
        "dataset_revisions", sa.Column("ingested_at", sa.DateTime(timezone=True))
    )
    op.add_column("dataset_revisions", sa.Column("promotability", sa.String(length=20)))
    op.add_column("dataset_revisions", sa.Column("quality_result_id", sa.Uuid()))
    op.add_column("dataset_revisions", sa.Column("point_in_time_result_id", sa.Uuid()))

    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("dataset_revisions", recreate="always") as batch:
            batch.create_check_constraint(
                "ck_dataset_revision_data_class",
                "data_class IS NULL OR data_class IN "
                "('SYNTHETIC', 'FIXTURE', 'VENDOR', 'PRODUCTION')",
            )
            batch.create_check_constraint(
                "ck_dataset_revision_promotability",
                "promotability IS NULL OR promotability IN ('PROMOTABLE', 'NON_PROMOTABLE')",
            )
            batch.create_foreign_key(
                "fk_dataset_revision_quality_result",
                "data_quality_results",
                ["quality_result_id"],
                ["id"],
                ondelete="SET NULL",
            )
            batch.create_foreign_key(
                "fk_dataset_revision_point_in_time_result",
                "data_quality_results",
                ["point_in_time_result_id"],
                ["id"],
                ondelete="SET NULL",
            )
    else:
        op.create_check_constraint(
            "ck_dataset_revision_data_class",
            "dataset_revisions",
            "data_class IS NULL OR data_class IN "
            "('SYNTHETIC', 'FIXTURE', 'VENDOR', 'PRODUCTION')",
        )
        op.create_check_constraint(
            "ck_dataset_revision_promotability",
            "dataset_revisions",
            "promotability IS NULL OR promotability IN ('PROMOTABLE', 'NON_PROMOTABLE')",
        )
        op.create_foreign_key(
            "fk_dataset_revision_quality_result",
            "dataset_revisions",
            "data_quality_results",
            ["quality_result_id"],
            ["id"],
            ondelete="SET NULL",
        )
        op.create_foreign_key(
            "fk_dataset_revision_point_in_time_result",
            "dataset_revisions",
            "data_quality_results",
            ["point_in_time_result_id"],
            ["id"],
            ondelete="SET NULL",
        )

    op.create_index(
        "uq_dataset_revision_canonical",
        "dataset_revisions",
        ["data_source_id", "universe_version_id", "revision_no", "partition"],
        unique=True,
        sqlite_where=sa.text("data_class IS NOT NULL"),
        postgresql_where=sa.text("data_class IS NOT NULL"),
    )


def _create_alpha_tables() -> None:
    op.create_table(
        "feature_pipeline_versions",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("pipeline_key", sa.String(length=160), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("universe_version_id", sa.Uuid(), nullable=False),
        sa.Column("artifact_uri", sa.Text(), nullable=False),
        sa.Column("input_schema", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("output_schema", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("point_in_time_policy_version_id", sa.Uuid(), nullable=False),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint("version_no > 0", name="ck_feature_pipeline_version_number"),
        sa.CheckConstraint("length(artifact_uri) > 0", name="ck_feature_pipeline_artifact_uri"),
        sa.ForeignKeyConstraint(
            ["universe_version_id"], ["market_universe_versions.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("pipeline_key", "version_no", name="uq_feature_pipeline_version"),
    )
    op.create_index(
        "ix_feature_pipeline_universe",
        "feature_pipeline_versions",
        ["universe_version_id", "created_at"],
    )

    op.create_table(
        "alpha_models",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("alpha_key", sa.String(length=160), nullable=False),
        sa.Column("name", sa.String(length=240), nullable=False),
        sa.Column("family", sa.String(length=120), nullable=False),
        sa.Column("description", sa.Text(), nullable=False),
        sa.Column("owner_program_id", sa.Uuid(), nullable=False),
        sa.Column("state", sa.String(length=40), nullable=False, server_default="RESEARCHING"),
        sa.Column("current_qualified_version_id", sa.Uuid()),
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
        sa.CheckConstraint(
            "state IN ('RESEARCHING', 'QUALIFIED', 'PAPER_ACTIVE', 'LIVE_ACTIVE', "
            "'DEGRADING', 'SUSPENDED', 'RETIRED')",
            name="ck_alpha_model_state",
        ),
        sa.ForeignKeyConstraint(
            ["owner_program_id"], ["research_programs.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("alpha_key", name="uq_alpha_model_key"),
    )
    op.create_index(
        "ix_alpha_model_owner_state", "alpha_models", ["owner_program_id", "state"]
    )

    op.create_table(
        "alpha_model_versions",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("alpha_model_id", sa.Uuid(), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("source_mission_id", sa.Uuid(), nullable=False),
        sa.Column("universe_version_id", sa.Uuid(), nullable=False),
        sa.Column("feature_pipeline_version_id", sa.Uuid()),
        sa.Column("horizon", sa.String(length=100), nullable=False),
        sa.Column("mode", sa.String(length=20), nullable=False),
        sa.Column("artifact_uri", sa.Text(), nullable=False),
        sa.Column("entrypoint", sa.String(length=500), nullable=False),
        sa.Column("parameters", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("input_contract", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("output_contract", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("state", sa.String(length=40), nullable=False, server_default="DRAFT"),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint("version_no > 0", name="ck_alpha_model_version_number"),
        sa.CheckConstraint(
            "mode IN ('RELATIVE_SCORE', 'CALIBRATED_RETURN')",
            name="ck_alpha_model_version_mode",
        ),
        sa.CheckConstraint(
            "state IN ('DRAFT', 'VALIDATED', 'REJECTED', 'RETIRED')",
            name="ck_alpha_model_version_state",
        ),
        sa.CheckConstraint(
            "length(artifact_uri) > 0", name="ck_alpha_model_version_artifact_uri"
        ),
        sa.CheckConstraint(
            "length(entrypoint) > 0", name="ck_alpha_model_version_entrypoint"
        ),
        sa.ForeignKeyConstraint(["alpha_model_id"], ["alpha_models.id"], ondelete="CASCADE"),
        sa.ForeignKeyConstraint(
            ["source_mission_id"], ["research_missions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["universe_version_id"], ["market_universe_versions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["feature_pipeline_version_id"],
            ["feature_pipeline_versions.id"],
            ondelete="RESTRICT",
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("alpha_model_id", "version_no", name="uq_alpha_model_version"),
    )
    op.create_index(
        "ix_alpha_model_version_universe",
        "alpha_model_versions",
        ["universe_version_id", "created_at"],
    )

    op.create_table(
        "alpha_signal_artifacts",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("alpha_model_version_id", sa.Uuid(), nullable=False),
        sa.Column("dataset_revision_id", sa.Uuid(), nullable=False),
        sa.Column("run_id", sa.Uuid(), nullable=False),
        sa.Column("mode", sa.String(length=20), nullable=False),
        sa.Column("artifact_uri", sa.Text(), nullable=False),
        sa.Column("row_count", sa.BigInteger(), nullable=False),
        sa.Column("event_start", sa.DateTime(timezone=True), nullable=False),
        sa.Column("event_end", sa.DateTime(timezone=True), nullable=False),
        sa.Column("available_start", sa.DateTime(timezone=True), nullable=False),
        sa.Column("available_end", sa.DateTime(timezone=True), nullable=False),
        sa.Column("schema_version", sa.String(length=40), nullable=False),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint(
            "mode IN ('RELATIVE_SCORE', 'CALIBRATED_RETURN')",
            name="ck_alpha_signal_artifact_mode",
        ),
        sa.CheckConstraint("row_count >= 0", name="ck_alpha_signal_artifact_row_count"),
        sa.CheckConstraint(
            "event_start <= event_end", name="ck_alpha_signal_artifact_event_range"
        ),
        sa.CheckConstraint(
            "available_start <= available_end",
            name="ck_alpha_signal_artifact_available_range",
        ),
        sa.CheckConstraint(
            "event_start <= available_start AND event_end <= available_end",
            name="ck_alpha_signal_artifact_point_in_time",
        ),
        sa.CheckConstraint("length(artifact_uri) > 0", name="ck_alpha_signal_artifact_uri"),
        sa.ForeignKeyConstraint(
            ["alpha_model_version_id"], ["alpha_model_versions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["dataset_revision_id"], ["dataset_revisions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["run_id"], ["quant_runtime_runs.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "alpha_model_version_id",
            "dataset_revision_id",
            "mode",
            name="uq_alpha_signal_artifact",
        ),
    )
    op.create_index("ix_alpha_signal_artifact_run", "alpha_signal_artifacts", ["run_id"])

    op.create_table(
        "alpha_calibration_versions",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("alpha_model_version_id", sa.Uuid(), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("method", sa.String(length=80), nullable=False),
        sa.Column(
            "training_dataset_revision_ids",
            _JSON,
            nullable=False,
            server_default=sa.text("'[]'"),
        ),
        sa.Column("artifact_uri", sa.Text(), nullable=False),
        sa.Column("parameters", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("metrics", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("state", sa.String(length=40), nullable=False, server_default="DRAFT"),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint("version_no > 0", name="ck_alpha_calibration_version_number"),
        sa.CheckConstraint(
            "state IN ('DRAFT', 'VALIDATED', 'REJECTED', 'RETIRED')",
            name="ck_alpha_calibration_version_state",
        ),
        sa.CheckConstraint(
            "length(artifact_uri) > 0", name="ck_alpha_calibration_artifact_uri"
        ),
        sa.ForeignKeyConstraint(
            ["alpha_model_version_id"], ["alpha_model_versions.id"], ondelete="CASCADE"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "alpha_model_version_id",
            "version_no",
            name="uq_alpha_calibration_version",
        ),
    )

    op.create_table(
        "alpha_evaluation_episodes",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("program_id", sa.Uuid(), nullable=False),
        sa.Column("branch_id", sa.Uuid(), nullable=False),
        sa.Column("alpha_model_version_id", sa.Uuid(), nullable=False),
        sa.Column("discovery_run_ids", _JSON, nullable=False, server_default=sa.text("'[]'")),
        sa.Column("validation_run_ids", _JSON, nullable=False, server_default=sa.text("'[]'")),
        sa.Column("sealed_run_id", sa.Uuid()),
        sa.Column("sealed_dataset_revision_id", sa.Uuid(), nullable=False),
        sa.Column("promotion_policy_version_id", sa.Uuid(), nullable=False),
        sa.Column("state", sa.String(length=40), nullable=False, server_default="PENDING"),
        sa.Column("result", sa.String(length=40)),
        sa.Column("gate_results", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column(
            "multiple_testing_summary", _JSON, nullable=False, server_default=sa.text("'{}'")
        ),
        sa.Column("disclosure", _JSON, nullable=False, server_default=sa.text("'{}'")),
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
        sa.CheckConstraint(
            "state IN ('PENDING', 'RUNNING', 'COMPLETED', 'FAILED', 'CANCELLED')",
            name="ck_alpha_evaluation_episode_state",
        ),
        sa.CheckConstraint(
            "result IS NULL OR result IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_evaluation_episode_result",
        ),
        sa.ForeignKeyConstraint(
            ["program_id"], ["research_programs.id"], ondelete="CASCADE"
        ),
        sa.ForeignKeyConstraint(
            ["branch_id"], ["research_branches.id"], ondelete="CASCADE"
        ),
        sa.ForeignKeyConstraint(
            ["alpha_model_version_id"], ["alpha_model_versions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(["sealed_run_id"], ["quant_runtime_runs.id"], ondelete="SET NULL"),
        sa.ForeignKeyConstraint(
            ["sealed_dataset_revision_id"], ["dataset_revisions.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index(
        "ix_alpha_evaluation_episode_program",
        "alpha_evaluation_episodes",
        ["program_id", "state"],
    )
    op.create_index(
        "ix_alpha_evaluation_episode_model",
        "alpha_evaluation_episodes",
        ["alpha_model_version_id", "created_at"],
    )


def _extend_alpha_qualifications() -> None:
    op.add_column("alpha_qualifications", sa.Column("alpha_model_id", sa.Uuid()))
    if op.get_bind().dialect.name == "sqlite":
        op.add_column(
            "alpha_qualifications",
            sa.Column("updated_at", sa.DateTime(timezone=True)),
        )
        op.execute("UPDATE alpha_qualifications SET updated_at = CURRENT_TIMESTAMP")
        with op.batch_alter_table("alpha_qualifications", recreate="always") as batch:
            batch.alter_column(
                "updated_at",
                existing_type=sa.DateTime(timezone=True),
                nullable=False,
                server_default=sa.func.now(),
            )
            batch.create_foreign_key(
                "fk_alpha_qualification_model",
                "alpha_models",
                ["alpha_model_id"],
                ["id"],
                ondelete="RESTRICT",
            )
            batch.create_check_constraint(
                "ck_alpha_qualification_canonical_role",
                "alpha_model_id IS NULL OR role IN "
                "('PRIMARY_ALPHA', 'SHADOW_ALPHA', 'HEDGE_ALPHA', 'RISK_SIGNAL')",
            )
            batch.create_check_constraint(
                "ck_alpha_qualification_canonical_state",
                "alpha_model_id IS NULL OR state IN "
                "('ACTIVE', 'WATCH', 'QUARANTINED', 'RETIRED', 'SHADOW')",
            )
    else:
        op.add_column(
            "alpha_qualifications",
            sa.Column(
                "updated_at",
                sa.DateTime(timezone=True),
                nullable=False,
                server_default=sa.func.now(),
            ),
        )
        op.create_foreign_key(
            "fk_alpha_qualification_model",
            "alpha_qualifications",
            "alpha_models",
            ["alpha_model_id"],
            ["id"],
            ondelete="RESTRICT",
        )
        op.create_check_constraint(
            "ck_alpha_qualification_canonical_role",
            "alpha_qualifications",
            "alpha_model_id IS NULL OR role IN "
            "('PRIMARY_ALPHA', 'SHADOW_ALPHA', 'HEDGE_ALPHA', 'RISK_SIGNAL')",
        )
        op.create_check_constraint(
            "ck_alpha_qualification_canonical_state",
            "alpha_qualifications",
            "alpha_model_id IS NULL OR state IN "
            "('ACTIVE', 'WATCH', 'QUARANTINED', 'RETIRED', 'SHADOW')",
        )

    op.create_index(
        "uq_alpha_qualification_scope",
        "alpha_qualifications",
        ["alpha_model_version_id", "universe_version_id", "horizon", "role"],
        unique=True,
        sqlite_where=sa.text("alpha_model_id IS NOT NULL"),
        postgresql_where=sa.text("alpha_model_id IS NOT NULL"),
    )


def upgrade() -> None:
    if op.get_bind().dialect.name == "sqlite":
        _extend_dataset_revisions()
        _create_data_quality_results()
    else:
        _create_data_quality_results()
        _extend_dataset_revisions()
    _create_alpha_tables()
    _extend_alpha_qualifications()


def _shrink_alpha_qualifications() -> None:
    op.drop_index("uq_alpha_qualification_scope", table_name="alpha_qualifications")
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_qualifications", recreate="always") as batch:
            batch.drop_constraint("fk_alpha_qualification_model", type_="foreignkey")
            batch.drop_constraint("ck_alpha_qualification_canonical_role", type_="check")
            batch.drop_constraint("ck_alpha_qualification_canonical_state", type_="check")
            batch.drop_column("updated_at")
            batch.drop_column("alpha_model_id")
    else:
        op.drop_constraint(
            "fk_alpha_qualification_model", "alpha_qualifications", type_="foreignkey"
        )
        op.drop_constraint(
            "ck_alpha_qualification_canonical_role", "alpha_qualifications", type_="check"
        )
        op.drop_constraint(
            "ck_alpha_qualification_canonical_state", "alpha_qualifications", type_="check"
        )
        op.drop_column("alpha_qualifications", "updated_at")
        op.drop_column("alpha_qualifications", "alpha_model_id")


def _shrink_dataset_revisions() -> None:
    op.drop_index("uq_dataset_revision_canonical", table_name="dataset_revisions")
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("dataset_revisions", recreate="always") as batch:
            batch.drop_constraint("fk_dataset_revision_quality_result", type_="foreignkey")
            batch.drop_constraint(
                "fk_dataset_revision_point_in_time_result", type_="foreignkey"
            )
            batch.drop_constraint("ck_dataset_revision_data_class", type_="check")
            batch.drop_constraint("ck_dataset_revision_promotability", type_="check")
            batch.drop_column("point_in_time_result_id")
            batch.drop_column("quality_result_id")
            batch.drop_column("promotability")
            batch.drop_column("ingested_at")
            batch.drop_column("origin")
            batch.drop_column("data_class")
    else:
        op.drop_constraint(
            "fk_dataset_revision_quality_result", "dataset_revisions", type_="foreignkey"
        )
        op.drop_constraint(
            "fk_dataset_revision_point_in_time_result",
            "dataset_revisions",
            type_="foreignkey",
        )
        op.drop_constraint(
            "ck_dataset_revision_data_class", "dataset_revisions", type_="check"
        )
        op.drop_constraint(
            "ck_dataset_revision_promotability", "dataset_revisions", type_="check"
        )
        for column in (
            "point_in_time_result_id",
            "quality_result_id",
            "promotability",
            "ingested_at",
            "origin",
            "data_class",
        ):
            op.drop_column("dataset_revisions", column)


def downgrade() -> None:
    _shrink_alpha_qualifications()
    _shrink_dataset_revisions()
    for table_name in (
        "alpha_evaluation_episodes",
        "alpha_calibration_versions",
        "alpha_signal_artifacts",
        "alpha_model_versions",
        "alpha_models",
        "feature_pipeline_versions",
        "data_quality_results",
    ):
        op.drop_table(table_name)
