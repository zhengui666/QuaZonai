"""Persist typed Discovery evaluator facts and calibration provenance.

Revision ID: 0025_typed_discovery_calibration
Revises: 0024_typed_portfolio_configuration
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "0025_typed_discovery_calibration"
down_revision = "0024_typed_portfolio_configuration"
branch_labels = None
depends_on = None


_DISCOVERY_PRIVATE_RESULT = (
    "(state IN ('FROZEN', 'QUEUED', 'RUNNING') "
    "AND private_result_ref IS NULL AND evaluated_at IS NULL) OR "
    "(state IN ('VALID', 'INCONCLUSIVE', 'INVALID') "
    "AND private_result_ref IS NOT NULL AND evaluated_at IS NOT NULL) OR "
    "(state = 'FAILED' AND private_result_ref IS NULL AND evaluated_at IS NULL)"
)
_CALIBRATION_TRUSTED_PROVENANCE = (
    "(source_discovery_evaluation_id IS NULL "
    "AND training_dataset_revision_id IS NULL "
    "AND private_artifact_ref IS NULL "
    "AND artifact_uri IS NOT NULL AND length(artifact_uri) > 0) OR "
    "(source_discovery_evaluation_id IS NOT NULL "
    "AND training_dataset_revision_id IS NOT NULL "
    "AND private_artifact_ref IS NOT NULL "
    "AND artifact_uri IS NULL AND state = 'VALIDATED')"
)
_DISCOVERY_METRIC_CODES = (
    "metric_code IN ('OBSERVATION_COUNT', 'COVERAGE', 'IC_MEAN', "
    "'RANK_IC_MEAN', 'HIT_RATE', 'NET_RETURN', 'ANNUALIZED_VOLATILITY', "
    "'SHARPE_RATIO', 'MAX_DRAWDOWN', 'TRIAL_ADJUSTED_SHARPE')"
)
_DISCOVERY_GATE_CODES = (
    "gate_code IN ('EVIDENCE_VALID', 'POINT_IN_TIME_VALID', "
    "'CALIBRATION_VALID', 'STATISTICAL_VALID', 'POLICY_VALID')"
)
_FINITE_NUMERIC = (
    "lower(CAST({column} AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')"
)


def _require_upgrade_compatibility() -> None:
    """Never manufacture evaluator/private provenance for existing immutable facts."""

    bind = op.get_bind()
    if (
        bind.execute(sa.text("SELECT 1 FROM alpha_discovery_evaluations LIMIT 1")).first()
        is not None
    ):
        raise RuntimeError("TRUSTED_DISCOVERY_RESULT_MIGRATION_BLOCKED")
    if (
        bind.execute(
            sa.text(
                "SELECT 1 FROM alpha_calibration_versions "
                "WHERE method IS NULL OR length(trim(method)) = 0 LIMIT 1"
            )
        ).first()
        is not None
    ):
        raise RuntimeError("TRUSTED_CALIBRATION_PROVENANCE_MIGRATION_BLOCKED")
    if (
        bind.execute(
            sa.text(
                "SELECT 1 FROM alpha_evaluation_assignments AS assignment_row "
                "WHERE NOT EXISTS ("
                "SELECT 1 FROM alpha_discovery_evaluations AS discovery "
                "WHERE discovery.id = assignment_row.discovery_evaluation_id "
                "AND discovery.alpha_model_version_id = assignment_row.alpha_model_version_id"
                ") LIMIT 1"
            )
        ).first()
        is not None
    ):
        raise RuntimeError("TRUSTED_ALPHA_ASSIGNMENT_MIGRATION_BLOCKED")
    # 0023 Assignment rows have neither a frozen Selection/Design link on Discovery
    # nor calibration provenance. Never invent either from mutable current configuration.
    if (
        bind.execute(sa.text("SELECT 1 FROM alpha_evaluation_assignments LIMIT 1")).first()
        is not None
    ):
        raise RuntimeError("TRUSTED_ALPHA_ASSIGNMENT_MIGRATION_BLOCKED")


def _extend_dataset_selections() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("evaluation_dataset_selections", recreate="always") as batch:
            batch.create_unique_constraint(
                "uq_evaluation_dataset_selection_discovery_dataset",
                ["id", "discovery_dataset_revision_id"],
            )
        return

    op.create_unique_constraint(
        "uq_evaluation_dataset_selection_discovery_dataset",
        "evaluation_dataset_selections",
        ["id", "discovery_dataset_revision_id"],
    )


def _extend_discovery_evaluations() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_discovery_evaluations", recreate="always") as batch:
            batch.add_column(sa.Column("private_result_ref", sa.Uuid()))
            batch.add_column(sa.Column("evaluated_at", sa.DateTime(timezone=True)))
            batch.add_column(
                sa.Column("evaluation_dataset_selection_id", sa.Uuid(), nullable=False)
            )
            batch.add_column(
                sa.Column("evaluation_design_version_id", sa.Uuid(), nullable=False)
            )
            batch.create_check_constraint(
                "ck_alpha_discovery_evaluation_private_result",
                _DISCOVERY_PRIVATE_RESULT,
            )
            batch.create_unique_constraint(
                "uq_alpha_discovery_evaluation_model",
                ["id", "alpha_model_version_id"],
            )
            batch.create_unique_constraint(
                "uq_alpha_discovery_evaluation_dataset",
                ["id", "discovery_dataset_revision_id"],
            )
            batch.create_unique_constraint(
                "uq_alpha_discovery_evaluation_design",
                ["id", "evaluation_design_version_id"],
            )
            batch.create_foreign_key(
                "fk_alpha_discovery_evaluation_selection_dataset",
                "evaluation_dataset_selections",
                ["evaluation_dataset_selection_id", "discovery_dataset_revision_id"],
                ["id", "discovery_dataset_revision_id"],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_alpha_discovery_evaluation_design",
                "evaluation_design_versions",
                ["evaluation_design_version_id"],
                ["id"],
                ondelete="RESTRICT",
            )
        return

    op.add_column("alpha_discovery_evaluations", sa.Column("private_result_ref", sa.Uuid()))
    op.add_column(
        "alpha_discovery_evaluations",
        sa.Column("evaluated_at", sa.DateTime(timezone=True)),
    )
    op.add_column(
        "alpha_discovery_evaluations",
        sa.Column("evaluation_dataset_selection_id", sa.Uuid(), nullable=False),
    )
    op.add_column(
        "alpha_discovery_evaluations",
        sa.Column("evaluation_design_version_id", sa.Uuid(), nullable=False),
    )
    op.create_check_constraint(
        "ck_alpha_discovery_evaluation_private_result",
        "alpha_discovery_evaluations",
        _DISCOVERY_PRIVATE_RESULT,
    )
    op.create_unique_constraint(
        "uq_alpha_discovery_evaluation_model",
        "alpha_discovery_evaluations",
        ["id", "alpha_model_version_id"],
    )
    op.create_unique_constraint(
        "uq_alpha_discovery_evaluation_dataset",
        "alpha_discovery_evaluations",
        ["id", "discovery_dataset_revision_id"],
    )
    op.create_unique_constraint(
        "uq_alpha_discovery_evaluation_design",
        "alpha_discovery_evaluations",
        ["id", "evaluation_design_version_id"],
    )
    op.create_foreign_key(
        "fk_alpha_discovery_evaluation_selection_dataset",
        "alpha_discovery_evaluations",
        "evaluation_dataset_selections",
        ["evaluation_dataset_selection_id", "discovery_dataset_revision_id"],
        ["id", "discovery_dataset_revision_id"],
        ondelete="RESTRICT",
    )
    op.create_foreign_key(
        "fk_alpha_discovery_evaluation_design",
        "alpha_discovery_evaluations",
        "evaluation_design_versions",
        ["evaluation_design_version_id"],
        ["id"],
        ondelete="RESTRICT",
    )


def _create_discovery_result_details() -> None:
    op.create_table(
        "alpha_discovery_evaluation_metrics",
        sa.Column("discovery_evaluation_id", sa.Uuid(), nullable=False),
        sa.Column("metric_code", sa.String(length=100), nullable=False),
        sa.Column("value", sa.Numeric(20, 8)),
        sa.Column("status", sa.String(length=20), nullable=False),
        sa.CheckConstraint(
            _DISCOVERY_METRIC_CODES,
            name="ck_alpha_discovery_evaluation_metric_code",
        ),
        sa.CheckConstraint(
            "status IN ('AVAILABLE', 'NOT_AVAILABLE')",
            name="ck_alpha_discovery_evaluation_metric_status",
        ),
        sa.CheckConstraint(
            "(status = 'AVAILABLE' AND value IS NOT NULL AND "
            f"{_FINITE_NUMERIC.format(column='value')}) OR "
            "(status = 'NOT_AVAILABLE' AND value IS NULL)",
            name="ck_alpha_discovery_evaluation_metric_value",
        ),
        sa.ForeignKeyConstraint(
            ["discovery_evaluation_id"],
            ["alpha_discovery_evaluations.id"],
            ondelete="RESTRICT",
        ),
        sa.PrimaryKeyConstraint("discovery_evaluation_id", "metric_code"),
    )
    op.create_table(
        "alpha_discovery_evaluation_gates",
        sa.Column("discovery_evaluation_id", sa.Uuid(), nullable=False),
        sa.Column("gate_code", sa.String(length=100), nullable=False),
        sa.Column("status", sa.String(length=20), nullable=False),
        sa.Column("reason_code", sa.String(length=100)),
        sa.CheckConstraint(
            _DISCOVERY_GATE_CODES,
            name="ck_alpha_discovery_evaluation_gate_code",
        ),
        sa.CheckConstraint(
            "status IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_discovery_evaluation_gate_status",
        ),
        sa.CheckConstraint(
            "(status = 'PASS' AND reason_code IS NULL) OR "
            "(status <> 'PASS' AND reason_code IS NOT NULL AND length(reason_code) > 0)",
            name="ck_alpha_discovery_evaluation_gate_reason",
        ),
        sa.ForeignKeyConstraint(
            ["discovery_evaluation_id"],
            ["alpha_discovery_evaluations.id"],
            ondelete="RESTRICT",
        ),
        sa.PrimaryKeyConstraint("discovery_evaluation_id", "gate_code"),
    )


def _extend_calibration_versions() -> None:
    columns = (
        sa.Column("source_discovery_evaluation_id", sa.Uuid()),
        sa.Column("training_dataset_revision_id", sa.Uuid()),
        sa.Column("private_artifact_ref", sa.Uuid()),
    )
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_calibration_versions", recreate="always") as batch:
            for column in columns:
                batch.add_column(column)
            batch.alter_column("artifact_uri", existing_type=sa.Text(), nullable=True)
            batch.drop_constraint("ck_alpha_calibration_artifact_uri", type_="check")
            batch.create_check_constraint(
                "ck_alpha_calibration_version_trusted_provenance",
                _CALIBRATION_TRUSTED_PROVENANCE,
            )
            batch.create_check_constraint(
                "ck_alpha_calibration_version_method",
                "length(trim(method)) > 0",
            )
            batch.create_unique_constraint(
                "uq_alpha_calibration_version_discovery_chain",
                ["id", "alpha_model_version_id", "source_discovery_evaluation_id"],
            )
            batch.create_foreign_key(
                "fk_alpha_calibration_version_training_dataset",
                "dataset_revisions",
                ["training_dataset_revision_id"],
                ["id"],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_alpha_calibration_version_discovery_dataset",
                "alpha_discovery_evaluations",
                ["source_discovery_evaluation_id", "training_dataset_revision_id"],
                ["id", "discovery_dataset_revision_id"],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_alpha_calibration_version_discovery_model",
                "alpha_discovery_evaluations",
                ["source_discovery_evaluation_id", "alpha_model_version_id"],
                ["id", "alpha_model_version_id"],
                ondelete="RESTRICT",
            )
        return

    for column in columns:
        op.add_column("alpha_calibration_versions", column)
    op.alter_column(
        "alpha_calibration_versions",
        "artifact_uri",
        existing_type=sa.Text(),
        nullable=True,
    )
    op.drop_constraint(
        "ck_alpha_calibration_artifact_uri",
        "alpha_calibration_versions",
        type_="check",
    )
    op.create_check_constraint(
        "ck_alpha_calibration_version_trusted_provenance",
        "alpha_calibration_versions",
        _CALIBRATION_TRUSTED_PROVENANCE,
    )
    op.create_check_constraint(
        "ck_alpha_calibration_version_method",
        "alpha_calibration_versions",
        "length(trim(method)) > 0",
    )
    op.create_unique_constraint(
        "uq_alpha_calibration_version_discovery_chain",
        "alpha_calibration_versions",
        ["id", "alpha_model_version_id", "source_discovery_evaluation_id"],
    )
    op.create_foreign_key(
        "fk_alpha_calibration_version_training_dataset",
        "alpha_calibration_versions",
        "dataset_revisions",
        ["training_dataset_revision_id"],
        ["id"],
        ondelete="RESTRICT",
    )
    op.create_foreign_key(
        "fk_alpha_calibration_version_discovery_dataset",
        "alpha_calibration_versions",
        "alpha_discovery_evaluations",
        ["source_discovery_evaluation_id", "training_dataset_revision_id"],
        ["id", "discovery_dataset_revision_id"],
        ondelete="RESTRICT",
    )
    op.create_foreign_key(
        "fk_alpha_calibration_version_discovery_model",
        "alpha_calibration_versions",
        "alpha_discovery_evaluations",
        ["source_discovery_evaluation_id", "alpha_model_version_id"],
        ["id", "alpha_model_version_id"],
        ondelete="RESTRICT",
    )


def _bind_assignment_discovery_chain() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_evaluation_assignments", recreate="always") as batch:
            batch.create_foreign_key(
                "fk_alpha_evaluation_assignment_discovery_model",
                "alpha_discovery_evaluations",
                ["discovery_evaluation_id", "alpha_model_version_id"],
                ["id", "alpha_model_version_id"],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_alpha_evaluation_assignment_discovery_design",
                "alpha_discovery_evaluations",
                ["discovery_evaluation_id", "evaluation_design_version_id"],
                ["id", "evaluation_design_version_id"],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_alpha_evaluation_assignment_calibration_chain",
                "alpha_calibration_versions",
                [
                    "alpha_calibration_version_id",
                    "alpha_model_version_id",
                    "discovery_evaluation_id",
                ],
                [
                    "id",
                    "alpha_model_version_id",
                    "source_discovery_evaluation_id",
                ],
                ondelete="RESTRICT",
            )
        return

    op.create_foreign_key(
        "fk_alpha_evaluation_assignment_discovery_model",
        "alpha_evaluation_assignments",
        "alpha_discovery_evaluations",
        ["discovery_evaluation_id", "alpha_model_version_id"],
        ["id", "alpha_model_version_id"],
        ondelete="RESTRICT",
    )
    op.create_foreign_key(
        "fk_alpha_evaluation_assignment_discovery_design",
        "alpha_evaluation_assignments",
        "alpha_discovery_evaluations",
        ["discovery_evaluation_id", "evaluation_design_version_id"],
        ["id", "evaluation_design_version_id"],
        ondelete="RESTRICT",
    )
    op.create_foreign_key(
        "fk_alpha_evaluation_assignment_calibration_chain",
        "alpha_evaluation_assignments",
        "alpha_calibration_versions",
        [
            "alpha_calibration_version_id",
            "alpha_model_version_id",
            "discovery_evaluation_id",
        ],
        ["id", "alpha_model_version_id", "source_discovery_evaluation_id"],
        ondelete="RESTRICT",
    )


def _add_signal_result_identity() -> None:
    """Make the trusted signal/result pair a target for the forecast FK."""

    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_signal_artifacts", recreate="always") as batch:
            batch.create_unique_constraint(
                "uq_alpha_signal_artifact_result",
                ["id", "evaluation_result_id"],
            )
        return

    op.create_unique_constraint(
        "uq_alpha_signal_artifact_result",
        "alpha_signal_artifacts",
        ["id", "evaluation_result_id"],
    )


def _create_alpha_evaluation_forecasts() -> None:
    finite = _FINITE_NUMERIC.format
    op.create_table(
        "alpha_evaluation_forecasts",
        sa.Column("result_id", sa.Uuid(), nullable=False),
        sa.Column("signal_artifact_id", sa.Uuid(), nullable=False),
        sa.Column("instrument_id", sa.String(length=200), nullable=False),
        sa.Column("as_of_time", sa.DateTime(timezone=True), nullable=False),
        sa.Column("effective_from", sa.DateTime(timezone=True), nullable=False),
        sa.Column("effective_until", sa.DateTime(timezone=True)),
        sa.Column("expected_return", sa.Numeric(20, 8), nullable=False),
        sa.Column("uncertainty", sa.Numeric(20, 8), nullable=False),
        sa.Column("confidence", sa.Numeric(20, 8), nullable=False),
        sa.Column("max_trade_notional", sa.Numeric(20, 8), nullable=False),
        sa.Column("max_position_notional", sa.Numeric(20, 8), nullable=False),
        sa.Column("max_participation_rate", sa.Numeric(20, 8), nullable=False),
        sa.Column("days_to_liquidate", sa.Numeric(20, 8), nullable=False),
        sa.Column("stressed_capacity_notional", sa.Numeric(20, 8), nullable=False),
        sa.CheckConstraint("length(trim(instrument_id)) > 0", name="ck_alpha_forecast_instrument"),
        sa.CheckConstraint(
            "as_of_time <= effective_from AND "
            "(effective_until IS NULL OR effective_until >= effective_from)",
            name="ck_alpha_forecast_time_order",
        ),
        sa.CheckConstraint(
            finite(column="expected_return"),
            name="ck_alpha_forecast_expected_return_finite",
        ),
        sa.CheckConstraint(
            f"uncertainty >= 0 AND {finite(column='uncertainty')}",
            name="ck_alpha_forecast_uncertainty",
        ),
        sa.CheckConstraint(
            f"confidence >= 0 AND confidence <= 1 AND {finite(column='confidence')}",
            name="ck_alpha_forecast_confidence",
        ),
        sa.CheckConstraint(
            f"max_trade_notional > 0 AND {finite(column='max_trade_notional')}",
            name="ck_alpha_forecast_max_trade_notional",
        ),
        sa.CheckConstraint(
            f"max_position_notional > 0 AND {finite(column='max_position_notional')}",
            name="ck_alpha_forecast_max_position_notional",
        ),
        sa.CheckConstraint(
            "max_participation_rate >= 0 AND max_participation_rate <= 1 "
            f"AND {finite(column='max_participation_rate')}",
            name="ck_alpha_forecast_max_participation_rate",
        ),
        sa.CheckConstraint(
            f"days_to_liquidate > 0 AND {finite(column='days_to_liquidate')}",
            name="ck_alpha_forecast_days_to_liquidate",
        ),
        sa.CheckConstraint(
            f"stressed_capacity_notional > 0 AND {finite(column='stressed_capacity_notional')}",
            name="ck_alpha_forecast_stressed_capacity_notional",
        ),
        sa.ForeignKeyConstraint(
            ["result_id"],
            ["alpha_evaluation_results.id"],
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["signal_artifact_id", "result_id"],
            [
                "alpha_signal_artifacts.id",
                "alpha_signal_artifacts.evaluation_result_id",
            ],
            name="fk_alpha_evaluation_forecast_signal_result",
            ondelete="RESTRICT",
        ),
        sa.PrimaryKeyConstraint("result_id", "instrument_id"),
    )


def upgrade() -> None:
    _require_upgrade_compatibility()
    _extend_dataset_selections()
    _extend_discovery_evaluations()
    _create_discovery_result_details()
    _extend_calibration_versions()
    _bind_assignment_discovery_chain()
    _add_signal_result_identity()
    _create_alpha_evaluation_forecasts()


def _require_legacy_only_for_downgrade() -> None:
    bind = op.get_bind()
    queries = (
        "SELECT 1 FROM alpha_discovery_evaluations LIMIT 1",
        "SELECT 1 FROM alpha_discovery_evaluation_metrics LIMIT 1",
        "SELECT 1 FROM alpha_discovery_evaluation_gates LIMIT 1",
        "SELECT 1 FROM alpha_calibration_versions "
        "WHERE source_discovery_evaluation_id IS NOT NULL "
        "OR training_dataset_revision_id IS NOT NULL "
        "OR private_artifact_ref IS NOT NULL LIMIT 1",
        "SELECT 1 FROM alpha_evaluation_forecasts LIMIT 1",
    )
    if any(bind.execute(sa.text(query)).first() is not None for query in queries):
        raise RuntimeError("TYPED_DISCOVERY_CALIBRATION_DOWNGRADE_BLOCKED")


def _unbind_assignment_discovery_chain() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_evaluation_assignments", recreate="always") as batch:
            batch.drop_constraint(
                "fk_alpha_evaluation_assignment_calibration_chain",
                type_="foreignkey",
            )
            batch.drop_constraint(
                "fk_alpha_evaluation_assignment_discovery_design",
                type_="foreignkey",
            )
            batch.drop_constraint(
                "fk_alpha_evaluation_assignment_discovery_model",
                type_="foreignkey",
            )
        return

    op.drop_constraint(
        "fk_alpha_evaluation_assignment_calibration_chain",
        "alpha_evaluation_assignments",
        type_="foreignkey",
    )
    op.drop_constraint(
        "fk_alpha_evaluation_assignment_discovery_design",
        "alpha_evaluation_assignments",
        type_="foreignkey",
    )
    op.drop_constraint(
        "fk_alpha_evaluation_assignment_discovery_model",
        "alpha_evaluation_assignments",
        type_="foreignkey",
    )


def _drop_signal_result_identity() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_signal_artifacts", recreate="always") as batch:
            batch.drop_constraint("uq_alpha_signal_artifact_result", type_="unique")
        return

    op.drop_constraint(
        "uq_alpha_signal_artifact_result",
        "alpha_signal_artifacts",
        type_="unique",
    )


def _shrink_calibration_versions() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_calibration_versions", recreate="always") as batch:
            batch.drop_constraint(
                "fk_alpha_calibration_version_discovery_model",
                type_="foreignkey",
            )
            batch.drop_constraint(
                "fk_alpha_calibration_version_discovery_dataset",
                type_="foreignkey",
            )
            batch.drop_constraint(
                "fk_alpha_calibration_version_training_dataset",
                type_="foreignkey",
            )
            batch.drop_constraint(
                "uq_alpha_calibration_version_discovery_chain",
                type_="unique",
            )
            batch.drop_constraint(
                "ck_alpha_calibration_version_trusted_provenance",
                type_="check",
            )
            batch.drop_constraint("ck_alpha_calibration_version_method", type_="check")
            batch.create_check_constraint(
                "ck_alpha_calibration_artifact_uri",
                "length(artifact_uri) > 0",
            )
            batch.alter_column("artifact_uri", existing_type=sa.Text(), nullable=False)
            batch.drop_column("private_artifact_ref")
            batch.drop_column("training_dataset_revision_id")
            batch.drop_column("source_discovery_evaluation_id")
        return

    op.drop_constraint(
        "fk_alpha_calibration_version_discovery_model",
        "alpha_calibration_versions",
        type_="foreignkey",
    )
    op.drop_constraint(
        "fk_alpha_calibration_version_discovery_dataset",
        "alpha_calibration_versions",
        type_="foreignkey",
    )
    op.drop_constraint(
        "fk_alpha_calibration_version_training_dataset",
        "alpha_calibration_versions",
        type_="foreignkey",
    )
    op.drop_constraint(
        "uq_alpha_calibration_version_discovery_chain",
        "alpha_calibration_versions",
        type_="unique",
    )
    op.drop_constraint(
        "ck_alpha_calibration_version_trusted_provenance",
        "alpha_calibration_versions",
        type_="check",
    )
    op.drop_constraint(
        "ck_alpha_calibration_version_method",
        "alpha_calibration_versions",
        type_="check",
    )
    op.create_check_constraint(
        "ck_alpha_calibration_artifact_uri",
        "alpha_calibration_versions",
        "length(artifact_uri) > 0",
    )
    op.alter_column(
        "alpha_calibration_versions",
        "artifact_uri",
        existing_type=sa.Text(),
        nullable=False,
    )
    op.drop_column("alpha_calibration_versions", "private_artifact_ref")
    op.drop_column("alpha_calibration_versions", "training_dataset_revision_id")
    op.drop_column("alpha_calibration_versions", "source_discovery_evaluation_id")


def _shrink_discovery_evaluations() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_discovery_evaluations", recreate="always") as batch:
            batch.drop_constraint(
                "fk_alpha_discovery_evaluation_selection_dataset",
                type_="foreignkey",
            )
            batch.drop_constraint("fk_alpha_discovery_evaluation_design", type_="foreignkey")
            batch.drop_constraint(
                "uq_alpha_discovery_evaluation_design",
                type_="unique",
            )
            batch.drop_constraint(
                "uq_alpha_discovery_evaluation_dataset",
                type_="unique",
            )
            batch.drop_constraint(
                "uq_alpha_discovery_evaluation_model",
                type_="unique",
            )
            batch.drop_constraint(
                "ck_alpha_discovery_evaluation_private_result",
                type_="check",
            )
            batch.drop_column("evaluation_design_version_id")
            batch.drop_column("evaluation_dataset_selection_id")
            batch.drop_column("evaluated_at")
            batch.drop_column("private_result_ref")
        return

    op.drop_constraint(
        "fk_alpha_discovery_evaluation_selection_dataset",
        "alpha_discovery_evaluations",
        type_="foreignkey",
    )
    op.drop_constraint(
        "fk_alpha_discovery_evaluation_design",
        "alpha_discovery_evaluations",
        type_="foreignkey",
    )
    op.drop_constraint(
        "uq_alpha_discovery_evaluation_design",
        "alpha_discovery_evaluations",
        type_="unique",
    )
    op.drop_constraint(
        "uq_alpha_discovery_evaluation_dataset",
        "alpha_discovery_evaluations",
        type_="unique",
    )
    op.drop_constraint(
        "uq_alpha_discovery_evaluation_model",
        "alpha_discovery_evaluations",
        type_="unique",
    )
    op.drop_constraint(
        "ck_alpha_discovery_evaluation_private_result",
        "alpha_discovery_evaluations",
        type_="check",
    )
    op.drop_column("alpha_discovery_evaluations", "evaluation_design_version_id")
    op.drop_column("alpha_discovery_evaluations", "evaluation_dataset_selection_id")
    op.drop_column("alpha_discovery_evaluations", "evaluated_at")
    op.drop_column("alpha_discovery_evaluations", "private_result_ref")


def _shrink_dataset_selections() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("evaluation_dataset_selections", recreate="always") as batch:
            batch.drop_constraint(
                "uq_evaluation_dataset_selection_discovery_dataset",
                type_="unique",
            )
        return

    op.drop_constraint(
        "uq_evaluation_dataset_selection_discovery_dataset",
        "evaluation_dataset_selections",
        type_="unique",
    )


def downgrade() -> None:
    _require_legacy_only_for_downgrade()
    op.drop_table("alpha_evaluation_forecasts")
    _drop_signal_result_identity()
    _unbind_assignment_discovery_chain()
    op.drop_table("alpha_discovery_evaluation_gates")
    op.drop_table("alpha_discovery_evaluation_metrics")
    _shrink_calibration_versions()
    _shrink_discovery_evaluations()
    _shrink_dataset_selections()
