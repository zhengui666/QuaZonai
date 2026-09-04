"""Persist complete-only Portfolio assembly inputs and relational Candidates.

Revision ID: 0026_portfolio_assembly_input
Revises: 0025_typed_discovery_calibration
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "0026_portfolio_assembly_input"
down_revision = "0025_typed_discovery_calibration"
branch_labels = None
depends_on = None


_FINITE = "lower(CAST({column} AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')"
_EVENT_ID = sa.BigInteger().with_variant(sa.Integer(), "sqlite")
_INPUT_SCALAR_COLUMNS = (
    "minimum_weight",
    "maximum_weight",
    "gross_exposure_limit",
    "net_exposure_target",
    "cash_reserve",
    "turnover_limit",
    "variance_limit",
    "risk_aversion",
    "cost_aversion",
    "uncertainty_aversion",
    "commission_rate",
    "half_spread_rate",
    "slippage_rate",
    "impact_rate",
    "impact_breakpoint",
)
_ASSIGNMENT_STATE = (
    "(state IN ('FROZEN', 'QUEUED', 'RUNNING') "
    "AND private_result_ref IS NULL AND evaluated_at IS NULL "
    "AND outcome_code IS NULL AND completed_at IS NULL) OR "
    "(state IN ('VALID', 'INCONCLUSIVE', 'INVALID') "
    "AND private_result_ref IS NOT NULL AND evaluated_at IS NOT NULL "
    "AND outcome_code IS NOT NULL AND length(trim(outcome_code)) > 0 "
    "AND completed_at IS NOT NULL) OR "
    "(state = 'FAILED' AND private_result_ref IS NULL AND evaluated_at IS NULL "
    "AND outcome_code IS NOT NULL AND length(trim(outcome_code)) > 0 "
    "AND completed_at IS NOT NULL)"
)
_INPUT_SCALARS = (
    "minimum_alpha_count >= 2 AND minimum_weight >= 0 AND maximum_weight > 0 "
    "AND maximum_weight <= 1 AND minimum_weight <= maximum_weight "
    "AND minimum_weight * minimum_alpha_count <= 1 "
    "AND maximum_weight * minimum_alpha_count >= 1 "
    "AND gross_exposure_limit = 1 AND net_exposure_target = 1 AND cash_reserve = 0 "
    "AND turnover_limit >= 1 AND turnover_limit <= 2 AND variance_limit > 0 "
    "AND risk_aversion >= 0 AND cost_aversion >= 0 AND uncertainty_aversion >= 0 "
    "AND commission_rate >= 0 AND commission_rate <= 1 "
    "AND half_spread_rate >= 0 AND half_spread_rate <= 1 "
    "AND slippage_rate >= 0 AND slippage_rate <= 1 "
    "AND impact_rate >= 0 AND impact_rate <= 1 "
    "AND impact_breakpoint >= 0 AND impact_breakpoint <= 1 AND "
    + " AND ".join(_FINITE.format(column=column) for column in _INPUT_SCALAR_COLUMNS)
)
_INPUT_STATE = (
    "(state = 'PENDING' AND outcome_code IS NULL AND completed_at IS NULL) OR "
    "(state IN ('ASSEMBLED', 'INFEASIBLE', 'STALE', 'INVALID') "
    "AND outcome_code IS NOT NULL AND length(trim(outcome_code)) > 0 "
    "AND completed_at IS NOT NULL)"
)
_TYPED_CANDIDATE = (
    "assembly_input_id IS NULL OR "
    "(candidate_family_id IS NOT NULL AND mandate_version_id IS NOT NULL "
    "AND capital_context_version_id IS NOT NULL AND universe_version_id IS NOT NULL "
    "AND state = 'ASSEMBLED')"
)


def _extend_existing_tables() -> None:
    bind = op.get_bind()
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("evaluation_dataset_selections", recreate="always") as batch:
            batch.create_unique_constraint(
                "uq_evaluation_dataset_selection_sealed_dataset",
                ["id", "sealed_dataset_revision_id"],
            )
        with op.batch_alter_table("alpha_qualifications", recreate="always") as batch:
            batch.create_unique_constraint(
                "uq_alpha_qualification_evaluation_result_pair",
                ["id", "evaluation_result_id"],
            )
        with op.batch_alter_table("portfolio_programs", recreate="always") as batch:
            batch.create_unique_constraint(
                "uq_portfolio_program_mandate_pair", ["id", "mandate_version_id"]
            )
        with op.batch_alter_table("portfolio_candidates", recreate="always") as batch:
            batch.create_unique_constraint(
                "uq_portfolio_candidate_program_pair", ["id", "portfolio_program_id"]
            )
        return

    op.create_unique_constraint(
        "uq_evaluation_dataset_selection_sealed_dataset",
        "evaluation_dataset_selections",
        ["id", "sealed_dataset_revision_id"],
    )
    op.create_unique_constraint(
        "uq_alpha_qualification_evaluation_result_pair",
        "alpha_qualifications",
        ["id", "evaluation_result_id"],
    )
    op.create_unique_constraint(
        "uq_portfolio_program_mandate_pair",
        "portfolio_programs",
        ["id", "mandate_version_id"],
    )
    op.create_unique_constraint(
        "uq_portfolio_candidate_program_pair",
        "portfolio_candidates",
        ["id", "portfolio_program_id"],
    )


def _create_candidate_families() -> None:
    op.create_table(
        "portfolio_candidate_families",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("portfolio_program_id", sa.Uuid(), nullable=False),
        sa.Column("mandate_version_id", sa.Uuid(), nullable=False),
        sa.Column(
            "created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("portfolio_program_id", name="uq_portfolio_candidate_family_program"),
        sa.UniqueConstraint(
            "id",
            "portfolio_program_id",
            "mandate_version_id",
            name="uq_portfolio_candidate_family_lineage_pair",
        ),
        sa.ForeignKeyConstraint(
            ["portfolio_program_id", "mandate_version_id"],
            ["portfolio_programs.id", "portfolio_programs.mandate_version_id"],
            name="fk_portfolio_candidate_family_program_mandate",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["mandate_version_id"],
            ["portfolio_mandate_versions.id"],
            name="fk_portfolio_candidate_family_mandate",
            ondelete="RESTRICT",
        ),
    )


def _create_input_evaluation_tables() -> None:
    numeric = sa.Numeric(20, 8)
    finite = _FINITE.format
    op.create_table(
        "portfolio_input_evaluation_assignments",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("portfolio_program_id", sa.Uuid(), nullable=False),
        sa.Column("mandate_version_id", sa.Uuid(), nullable=False),
        sa.Column("capital_context_version_id", sa.Uuid(), nullable=False),
        sa.Column("evaluation_dataset_selection_id", sa.Uuid(), nullable=False),
        sa.Column("sealed_dataset_revision_id", sa.Uuid(), nullable=False),
        sa.Column("promotion_policy_version_id", sa.Uuid(), nullable=False),
        sa.Column("cause_event_id", _EVENT_ID, nullable=False),
        sa.Column("previous_candidate_id", sa.Uuid()),
        sa.Column("as_of_time", sa.DateTime(timezone=True), nullable=False),
        sa.Column("evaluator_contract_version", sa.String(length=80), nullable=False),
        sa.Column("state", sa.String(length=20), nullable=False),
        sa.Column("private_result_ref", sa.Uuid()),
        sa.Column("evaluated_at", sa.DateTime(timezone=True)),
        sa.Column("outcome_code", sa.String(length=100)),
        sa.Column(
            "created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()
        ),
        sa.Column("completed_at", sa.DateTime(timezone=True)),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "portfolio_program_id",
            "cause_event_id",
            name="uq_portfolio_input_evaluation_assignment_cause",
        ),
        sa.CheckConstraint(
            "evaluator_contract_version = 'PORTFOLIO_INPUT_EVALUATION_V1'",
            name="ck_portfolio_input_evaluation_assignment_contract",
        ),
        sa.CheckConstraint(
            _ASSIGNMENT_STATE, name="ck_portfolio_input_evaluation_assignment_state"
        ),
        sa.ForeignKeyConstraint(
            ["portfolio_program_id", "mandate_version_id"],
            ["portfolio_programs.id", "portfolio_programs.mandate_version_id"],
            name="fk_portfolio_input_evaluation_assignment_program_mandate",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["mandate_version_id"],
            ["portfolio_mandate_versions.id"],
            name="fk_portfolio_input_evaluation_assignment_mandate",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["capital_context_version_id"],
            ["capital_context_versions.id"],
            name="fk_portfolio_input_evaluation_assignment_capital",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["evaluation_dataset_selection_id", "sealed_dataset_revision_id"],
            [
                "evaluation_dataset_selections.id",
                "evaluation_dataset_selections.sealed_dataset_revision_id",
            ],
            name="fk_portfolio_input_evaluation_assignment_selection_sealed",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["promotion_policy_version_id"],
            ["promotion_policy_versions.id"],
            name="fk_portfolio_input_evaluation_assignment_policy",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["cause_event_id"],
            ["events.id"],
            name="fk_portfolio_input_evaluation_assignment_event",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["previous_candidate_id", "portfolio_program_id"],
            ["portfolio_candidates.id", "portfolio_candidates.portfolio_program_id"],
            name="fk_portfolio_input_evaluation_assignment_predecessor",
            ondelete="RESTRICT",
        ),
    )
    op.create_table(
        "portfolio_input_evaluation_assignment_members",
        sa.Column("assignment_id", sa.Uuid(), nullable=False),
        sa.Column("axis_index", sa.Integer(), nullable=False),
        sa.Column("alpha_qualification_id", sa.Uuid(), nullable=False),
        sa.Column("alpha_evaluation_result_id", sa.Uuid(), nullable=False),
        sa.Column("alpha_signal_artifact_id", sa.Uuid(), nullable=False),
        sa.Column("instrument_id", sa.String(length=200), nullable=False),
        sa.PrimaryKeyConstraint("assignment_id", "axis_index"),
        sa.UniqueConstraint(
            "assignment_id",
            "alpha_qualification_id",
            name="uq_portfolio_input_assignment_member_qualification",
        ),
        sa.UniqueConstraint(
            "assignment_id", "instrument_id", name="uq_portfolio_input_assignment_member_instrument"
        ),
        sa.CheckConstraint("axis_index >= 0", name="ck_portfolio_input_assignment_member_axis"),
        sa.ForeignKeyConstraint(
            ["assignment_id"], ["portfolio_input_evaluation_assignments.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["alpha_qualification_id", "alpha_evaluation_result_id"],
            ["alpha_qualifications.id", "alpha_qualifications.evaluation_result_id"],
            name="fk_portfolio_input_assignment_member_qualification_result",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["alpha_signal_artifact_id", "alpha_evaluation_result_id"],
            ["alpha_signal_artifacts.id", "alpha_signal_artifacts.evaluation_result_id"],
            name="fk_portfolio_input_assignment_member_signal_result",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["alpha_evaluation_result_id", "instrument_id"],
            ["alpha_evaluation_forecasts.result_id", "alpha_evaluation_forecasts.instrument_id"],
            name="fk_portfolio_input_assignment_member_forecast",
            ondelete="RESTRICT",
        ),
    )
    op.create_table(
        "portfolio_assembly_inputs",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("portfolio_input_evaluation_assignment_id", sa.Uuid(), nullable=False),
        sa.Column("portfolio_program_id", sa.Uuid(), nullable=False),
        sa.Column("mandate_version_id", sa.Uuid(), nullable=False),
        sa.Column("capital_context_version_id", sa.Uuid(), nullable=False),
        sa.Column("universe_version_id", sa.Uuid(), nullable=False),
        sa.Column("promotion_policy_version_id", sa.Uuid(), nullable=False),
        sa.Column("cause_event_id", _EVENT_ID, nullable=False),
        sa.Column("snapshot_no", sa.BigInteger(), nullable=False),
        sa.Column("input_contract_version", sa.String(length=80), nullable=False),
        sa.Column("as_of_time", sa.DateTime(timezone=True), nullable=False),
        sa.Column("effective_from", sa.DateTime(timezone=True), nullable=False),
        sa.Column("effective_until", sa.DateTime(timezone=True)),
        sa.Column("previous_candidate_id", sa.Uuid()),
        sa.Column("covariance_method", sa.String(length=80), nullable=False),
        sa.Column("covariance_observations", sa.Integer(), nullable=False),
        sa.Column("covariance_decay", numeric, nullable=False),
        sa.Column("covariance_shrinkage", numeric, nullable=False),
        sa.Column("minimum_alpha_count", sa.Integer(), nullable=False),
        sa.Column("minimum_weight", numeric, nullable=False),
        sa.Column("maximum_weight", numeric, nullable=False),
        sa.Column("gross_exposure_limit", numeric, nullable=False),
        sa.Column("net_exposure_target", numeric, nullable=False),
        sa.Column("cash_reserve", numeric, nullable=False),
        sa.Column("turnover_limit", numeric, nullable=False),
        sa.Column("variance_limit", numeric, nullable=False),
        sa.Column("risk_aversion", numeric, nullable=False),
        sa.Column("cost_aversion", numeric, nullable=False),
        sa.Column("uncertainty_aversion", numeric, nullable=False),
        sa.Column("commission_rate", numeric, nullable=False),
        sa.Column("half_spread_rate", numeric, nullable=False),
        sa.Column("slippage_rate", numeric, nullable=False),
        sa.Column("impact_rate", numeric, nullable=False),
        sa.Column("impact_breakpoint", numeric, nullable=False),
        sa.Column("state", sa.String(length=20), nullable=False),
        sa.Column("outcome_code", sa.String(length=100)),
        sa.Column(
            "created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()
        ),
        sa.Column("completed_at", sa.DateTime(timezone=True)),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "portfolio_input_evaluation_assignment_id",
            name="uq_portfolio_assembly_input_assignment",
        ),
        sa.UniqueConstraint(
            "portfolio_program_id", "snapshot_no", name="uq_portfolio_assembly_input_snapshot"
        ),
        sa.UniqueConstraint(
            "portfolio_program_id", "cause_event_id", name="uq_portfolio_assembly_input_cause"
        ),
        sa.UniqueConstraint(
            "id",
            "portfolio_program_id",
            "mandate_version_id",
            "capital_context_version_id",
            "universe_version_id",
            name="uq_portfolio_assembly_input_candidate_source",
        ),
        sa.CheckConstraint("snapshot_no > 0", name="ck_portfolio_assembly_input_snapshot"),
        sa.CheckConstraint(
            "input_contract_version = 'LONG_ONLY_MEAN_VARIANCE_V1'",
            name="ck_portfolio_assembly_input_contract",
        ),
        sa.CheckConstraint(
            "as_of_time <= effective_from AND "
            "(effective_until IS NULL OR effective_until >= effective_from)",
            name="ck_portfolio_assembly_input_time",
        ),
        sa.CheckConstraint(
            "length(trim(covariance_method)) > 0 AND covariance_observations >= 2 "
            "AND covariance_decay > 0 AND covariance_decay < 1 "
            "AND covariance_shrinkage >= 0 AND covariance_shrinkage <= 1 AND "
            f"{finite(column='covariance_decay')} AND {finite(column='covariance_shrinkage')}",
            name="ck_portfolio_assembly_input_covariance_metadata",
        ),
        sa.CheckConstraint(_INPUT_SCALARS, name="ck_portfolio_assembly_input_v1_scalars"),
        sa.CheckConstraint(_INPUT_STATE, name="ck_portfolio_assembly_input_state"),
        sa.ForeignKeyConstraint(
            ["portfolio_input_evaluation_assignment_id"],
            ["portfolio_input_evaluation_assignments.id"],
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["portfolio_program_id", "mandate_version_id"],
            ["portfolio_programs.id", "portfolio_programs.mandate_version_id"],
            name="fk_portfolio_assembly_input_program_mandate",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["mandate_version_id"],
            ["portfolio_mandate_versions.id"],
            name="fk_portfolio_assembly_input_mandate",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["capital_context_version_id"],
            ["capital_context_versions.id"],
            name="fk_portfolio_assembly_input_capital",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["universe_version_id"],
            ["market_universe_versions.id"],
            name="fk_portfolio_assembly_input_universe",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["promotion_policy_version_id"],
            ["promotion_policy_versions.id"],
            name="fk_portfolio_assembly_input_policy",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["cause_event_id"],
            ["events.id"],
            name="fk_portfolio_assembly_input_event",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["previous_candidate_id", "portfolio_program_id"],
            ["portfolio_candidates.id", "portfolio_candidates.portfolio_program_id"],
            name="fk_portfolio_assembly_input_predecessor",
            ondelete="RESTRICT",
        ),
    )
    op.create_index(
        "uq_portfolio_assembly_input_pending_program",
        "portfolio_assembly_inputs",
        ["portfolio_program_id"],
        unique=True,
        sqlite_where=sa.text("state = 'PENDING'"),
        postgresql_where=sa.text("state = 'PENDING'"),
    )
    op.create_table(
        "portfolio_assembly_input_members",
        sa.Column("input_id", sa.Uuid(), nullable=False),
        sa.Column("axis_index", sa.Integer(), nullable=False),
        sa.Column("alpha_qualification_id", sa.Uuid(), nullable=False),
        sa.Column("alpha_evaluation_result_id", sa.Uuid(), nullable=False),
        sa.Column("alpha_signal_artifact_id", sa.Uuid(), nullable=False),
        sa.Column("instrument_id", sa.String(length=200), nullable=False),
        sa.Column("expected_return", numeric, nullable=False),
        sa.Column("uncertainty", numeric, nullable=False),
        sa.Column("confidence", numeric, nullable=False),
        sa.Column("previous_weight", numeric, nullable=False),
        sa.Column("max_trade_notional", numeric, nullable=False),
        sa.Column("max_position_notional", numeric, nullable=False),
        sa.Column("max_participation_rate", numeric, nullable=False),
        sa.Column("days_to_liquidate", numeric, nullable=False),
        sa.Column("stressed_capacity", numeric, nullable=False),
        sa.PrimaryKeyConstraint("input_id", "axis_index"),
        sa.UniqueConstraint(
            "input_id",
            "alpha_qualification_id",
            name="uq_portfolio_assembly_input_member_qualification",
        ),
        sa.UniqueConstraint(
            "input_id", "instrument_id", name="uq_portfolio_assembly_input_member_instrument"
        ),
        sa.CheckConstraint("axis_index >= 0", name="ck_portfolio_assembly_input_member_axis"),
        sa.CheckConstraint(
            finite(column="expected_return"), name="ck_portfolio_assembly_input_member_return"
        ),
        sa.CheckConstraint(
            f"uncertainty >= 0 AND {finite(column='uncertainty')}",
            name="ck_portfolio_assembly_input_member_uncertainty",
        ),
        sa.CheckConstraint(
            "confidence >= 0 AND confidence <= 1 AND previous_weight >= 0 "
            "AND previous_weight <= 1 "
            f"AND {finite(column='confidence')} AND {finite(column='previous_weight')}",
            name="ck_portfolio_assembly_input_member_weight_bounds",
        ),
        sa.CheckConstraint(
            "max_trade_notional > 0 AND max_position_notional > 0 "
            "AND max_participation_rate >= 0 AND max_participation_rate <= 1 "
            "AND days_to_liquidate > 0 AND stressed_capacity > 0 "
            f"AND {finite(column='max_trade_notional')} "
            f"AND {finite(column='max_position_notional')} "
            f"AND {finite(column='max_participation_rate')} "
            f"AND {finite(column='days_to_liquidate')} "
            f"AND {finite(column='stressed_capacity')}",
            name="ck_portfolio_assembly_input_member_capacity",
        ),
        sa.ForeignKeyConstraint(
            ["input_id"], ["portfolio_assembly_inputs.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["alpha_qualification_id", "alpha_evaluation_result_id"],
            ["alpha_qualifications.id", "alpha_qualifications.evaluation_result_id"],
            name="fk_portfolio_assembly_input_member_qualification_result",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["alpha_signal_artifact_id", "alpha_evaluation_result_id"],
            ["alpha_signal_artifacts.id", "alpha_signal_artifacts.evaluation_result_id"],
            name="fk_portfolio_assembly_input_member_signal_result",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["alpha_evaluation_result_id", "instrument_id"],
            ["alpha_evaluation_forecasts.result_id", "alpha_evaluation_forecasts.instrument_id"],
            name="fk_portfolio_assembly_input_member_forecast",
            ondelete="RESTRICT",
        ),
    )
    op.create_table(
        "portfolio_assembly_input_covariances",
        sa.Column("input_id", sa.Uuid(), nullable=False),
        sa.Column("left_axis_index", sa.Integer(), nullable=False),
        sa.Column("right_axis_index", sa.Integer(), nullable=False),
        sa.Column("covariance", numeric, nullable=False),
        sa.PrimaryKeyConstraint("input_id", "left_axis_index", "right_axis_index"),
        sa.CheckConstraint(
            "left_axis_index >= 0 AND right_axis_index >= left_axis_index",
            name="ck_portfolio_assembly_input_covariance_axes",
        ),
        sa.CheckConstraint(
            finite(column="covariance"), name="ck_portfolio_assembly_input_covariance_finite"
        ),
        sa.CheckConstraint(
            "left_axis_index <> right_axis_index OR covariance >= 0",
            name="ck_portfolio_assembly_input_covariance_diagonal",
        ),
        sa.ForeignKeyConstraint(
            ["input_id", "left_axis_index"],
            [
                "portfolio_assembly_input_members.input_id",
                "portfolio_assembly_input_members.axis_index",
            ],
            name="fk_portfolio_assembly_covariance_left_axis",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["input_id", "right_axis_index"],
            [
                "portfolio_assembly_input_members.input_id",
                "portfolio_assembly_input_members.axis_index",
            ],
            name="fk_portfolio_assembly_covariance_right_axis",
            ondelete="RESTRICT",
        ),
    )
    op.create_table(
        "portfolio_search_ledger_entries",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("portfolio_program_id", sa.Uuid(), nullable=False),
        sa.Column("cause_event_id", _EVENT_ID, nullable=False),
        sa.Column("portfolio_assembly_input_id", sa.Uuid()),
        sa.Column("attempt_type", sa.String(length=40), nullable=False),
        sa.Column("outcome_class", sa.String(length=20), nullable=False),
        sa.Column("reason_code", sa.String(length=100), nullable=False),
        sa.Column(
            "created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "portfolio_program_id",
            "cause_event_id",
            "attempt_type",
            name="uq_portfolio_search_ledger_attempt",
        ),
        sa.CheckConstraint(
            "attempt_type IN ('INPUT_STAGING', 'INPUT_EVALUATION', 'ASSEMBLY')",
            name="ck_portfolio_search_ledger_attempt",
        ),
        sa.CheckConstraint(
            "outcome_class IN ('INCONCLUSIVE', 'INVALID', 'INFEASIBLE', 'STALE')",
            name="ck_portfolio_search_ledger_outcome",
        ),
        sa.CheckConstraint(
            "length(trim(reason_code)) > 0", name="ck_portfolio_search_ledger_reason"
        ),
        sa.ForeignKeyConstraint(
            ["portfolio_program_id"], ["portfolio_programs.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(["cause_event_id"], ["events.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(
            ["portfolio_assembly_input_id"],
            ["portfolio_assembly_inputs.id"],
            ondelete="RESTRICT",
        ),
    )


def _extend_candidates() -> None:
    bind = op.get_bind()
    columns = (
        sa.Column("assembly_input_id", sa.Uuid()),
        sa.Column("universe_version_id", sa.Uuid()),
    )
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("portfolio_candidates", recreate="always") as batch:
            for column in columns:
                batch.add_column(column)
            batch.create_unique_constraint(
                "uq_portfolio_candidate_assembly_input", ["assembly_input_id"]
            )
            batch.create_check_constraint(
                "ck_portfolio_candidate_typed_assembled", _TYPED_CANDIDATE
            )
            batch.create_foreign_key(
                "fk_portfolio_candidate_family_program",
                "portfolio_candidate_families",
                ["candidate_family_id", "portfolio_program_id", "mandate_version_id"],
                ["id", "portfolio_program_id", "mandate_version_id"],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_portfolio_candidate_assembly_input",
                "portfolio_assembly_inputs",
                [
                    "assembly_input_id",
                    "portfolio_program_id",
                    "mandate_version_id",
                    "capital_context_version_id",
                    "universe_version_id",
                ],
                [
                    "id",
                    "portfolio_program_id",
                    "mandate_version_id",
                    "capital_context_version_id",
                    "universe_version_id",
                ],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_portfolio_candidate_universe",
                "market_universe_versions",
                ["universe_version_id"],
                ["id"],
                ondelete="RESTRICT",
            )
        return

    for column in columns:
        op.add_column("portfolio_candidates", column)
    op.create_unique_constraint(
        "uq_portfolio_candidate_assembly_input",
        "portfolio_candidates",
        ["assembly_input_id"],
    )
    op.create_check_constraint(
        "ck_portfolio_candidate_typed_assembled", "portfolio_candidates", _TYPED_CANDIDATE
    )
    # Existing legacy candidate_family_id values were not relational facts. NOT VALID
    # preserves their read-only history while enforcing the FK for all new rows.
    op.execute(
        sa.text(
            "ALTER TABLE portfolio_candidates ADD CONSTRAINT "
            "fk_portfolio_candidate_family_program FOREIGN KEY "
            "(candidate_family_id, portfolio_program_id, mandate_version_id) REFERENCES "
            "portfolio_candidate_families (id, portfolio_program_id, mandate_version_id) "
            "ON DELETE RESTRICT NOT VALID"
        )
    )
    op.create_foreign_key(
        "fk_portfolio_candidate_assembly_input",
        "portfolio_candidates",
        "portfolio_assembly_inputs",
        [
            "assembly_input_id",
            "portfolio_program_id",
            "mandate_version_id",
            "capital_context_version_id",
            "universe_version_id",
        ],
        [
            "id",
            "portfolio_program_id",
            "mandate_version_id",
            "capital_context_version_id",
            "universe_version_id",
        ],
        ondelete="RESTRICT",
    )
    op.create_foreign_key(
        "fk_portfolio_candidate_universe",
        "portfolio_candidates",
        "market_universe_versions",
        ["universe_version_id"],
        ["id"],
        ondelete="RESTRICT",
    )


def _create_candidate_member_table() -> None:
    finite = _FINITE.format
    op.create_table(
        "portfolio_candidate_members",
        sa.Column("candidate_id", sa.Uuid(), nullable=False),
        sa.Column("alpha_qualification_id", sa.Uuid(), nullable=False),
        sa.Column("role", sa.String(length=40), nullable=False),
        sa.Column("target_weight", sa.Numeric(20, 8), nullable=False),
        sa.PrimaryKeyConstraint("candidate_id", "alpha_qualification_id"),
        sa.CheckConstraint("role = 'PRIMARY_ALPHA'", name="ck_portfolio_candidate_member_role"),
        sa.CheckConstraint(
            f"target_weight >= 0 AND target_weight <= 1 AND {finite(column='target_weight')}",
            name="ck_portfolio_candidate_member_weight",
        ),
        sa.ForeignKeyConstraint(["candidate_id"], ["portfolio_candidates.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(
            ["alpha_qualification_id"], ["alpha_qualifications.id"], ondelete="RESTRICT"
        ),
    )


def upgrade() -> None:
    _extend_existing_tables()
    _create_candidate_families()
    _create_input_evaluation_tables()
    _extend_candidates()
    _create_candidate_member_table()


def _require_empty_new_facts() -> None:
    bind = op.get_bind()
    tables = (
        "portfolio_candidate_families",
        "portfolio_input_evaluation_assignments",
        "portfolio_input_evaluation_assignment_members",
        "portfolio_assembly_inputs",
        "portfolio_assembly_input_members",
        "portfolio_assembly_input_covariances",
        "portfolio_search_ledger_entries",
        "portfolio_candidate_members",
    )
    if any(bind.execute(sa.text(f"SELECT 1 FROM {table} LIMIT 1")).first() for table in tables):
        raise RuntimeError("PORTFOLIO_ASSEMBLY_INPUT_DOWNGRADE_BLOCKED")
    if (
        bind.execute(
            sa.text(
                "SELECT 1 FROM portfolio_candidates WHERE assembly_input_id IS NOT NULL LIMIT 1"
            )
        ).first()
        is not None
    ):
        raise RuntimeError("PORTFOLIO_ASSEMBLY_INPUT_DOWNGRADE_BLOCKED")


def _shrink_candidates() -> None:
    bind = op.get_bind()
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("portfolio_candidates", recreate="always") as batch:
            batch.drop_constraint("fk_portfolio_candidate_universe", type_="foreignkey")
            batch.drop_constraint("fk_portfolio_candidate_assembly_input", type_="foreignkey")
            batch.drop_constraint("fk_portfolio_candidate_family_program", type_="foreignkey")
            batch.drop_constraint("ck_portfolio_candidate_typed_assembled", type_="check")
            batch.drop_constraint("uq_portfolio_candidate_assembly_input", type_="unique")
            batch.drop_column("universe_version_id")
            batch.drop_column("assembly_input_id")
        return

    op.drop_constraint(
        "fk_portfolio_candidate_universe", "portfolio_candidates", type_="foreignkey"
    )
    op.drop_constraint(
        "fk_portfolio_candidate_assembly_input", "portfolio_candidates", type_="foreignkey"
    )
    op.drop_constraint(
        "fk_portfolio_candidate_family_program", "portfolio_candidates", type_="foreignkey"
    )
    op.drop_constraint(
        "ck_portfolio_candidate_typed_assembled", "portfolio_candidates", type_="check"
    )
    op.drop_constraint(
        "uq_portfolio_candidate_assembly_input", "portfolio_candidates", type_="unique"
    )
    op.drop_column("portfolio_candidates", "universe_version_id")
    op.drop_column("portfolio_candidates", "assembly_input_id")


def _shrink_existing_tables() -> None:
    bind = op.get_bind()
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("portfolio_programs", recreate="always") as batch:
            batch.drop_constraint("uq_portfolio_program_mandate_pair", type_="unique")
        with op.batch_alter_table("portfolio_candidates", recreate="always") as batch:
            batch.drop_constraint("uq_portfolio_candidate_program_pair", type_="unique")
        with op.batch_alter_table("alpha_qualifications", recreate="always") as batch:
            batch.drop_constraint("uq_alpha_qualification_evaluation_result_pair", type_="unique")
        with op.batch_alter_table("evaluation_dataset_selections", recreate="always") as batch:
            batch.drop_constraint("uq_evaluation_dataset_selection_sealed_dataset", type_="unique")
        return

    op.drop_constraint("uq_portfolio_program_mandate_pair", "portfolio_programs", type_="unique")
    op.drop_constraint(
        "uq_portfolio_candidate_program_pair", "portfolio_candidates", type_="unique"
    )
    op.drop_constraint(
        "uq_alpha_qualification_evaluation_result_pair", "alpha_qualifications", type_="unique"
    )
    op.drop_constraint(
        "uq_evaluation_dataset_selection_sealed_dataset",
        "evaluation_dataset_selections",
        type_="unique",
    )


def downgrade() -> None:
    _require_empty_new_facts()
    op.drop_table("portfolio_candidate_members")
    _shrink_candidates()
    op.drop_table("portfolio_search_ledger_entries")
    op.drop_table("portfolio_assembly_input_covariances")
    op.drop_table("portfolio_assembly_input_members")
    op.drop_index(
        "uq_portfolio_assembly_input_pending_program", table_name="portfolio_assembly_inputs"
    )
    op.drop_table("portfolio_assembly_inputs")
    op.drop_table("portfolio_input_evaluation_assignment_members")
    op.drop_table("portfolio_input_evaluation_assignments")
    op.drop_table("portfolio_candidate_families")
    _shrink_existing_tables()
