"""Persist typed trusted Alpha evaluation inputs and aggregate evidence.

Revision ID: 0023_trusted_alpha_evaluation
Revises: 0022_downstream_preflight
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "0023_trusted_alpha_evaluation"
down_revision = "0022_downstream_preflight"
branch_labels = None
depends_on = None


_EVENT_ID = sa.BigInteger().with_variant(sa.Integer(), "sqlite")
_LEGACY_MISSION_STATE = (
    "state IN ('PLANNED', 'READY', 'RUNNING', 'SUCCEEDED', 'FAILED', "
    "'INTERRUPTED', 'CANCELLED')"
)
_TRUSTED_MISSION_STATE = (
    "state IN ('PLANNED', 'READY', 'RUNNING', 'SUCCEEDED', 'FAILED', "
    "'INTERRUPTED', 'CANCELLED', 'AWAITING_VALIDATION')"
)
_LEGACY_EPISODE_STATE = "state IN ('PENDING', 'RUNNING', 'COMPLETED', 'FAILED', 'CANCELLED')"
_TRUSTED_EPISODE_STATE = (
    "state IN ('PENDING', 'RUNNING', 'COMPLETED', 'FAILED', 'CANCELLED', "
    "'PLANNED', 'SEALED', 'ASSIGNED', 'EVALUATING', 'EVALUATED', "
    "'DISCLOSED', 'CONSUMED', 'INVALIDATED')"
)
_LEGACY_QUALIFICATION_ROLE = (
    "alpha_model_id IS NULL OR role IN "
    "('PRIMARY_ALPHA', 'SHADOW_ALPHA', 'HEDGE_ALPHA', 'RISK_SIGNAL')"
)
_TRUSTED_QUALIFICATION_ROLE = (
    "alpha_model_id IS NULL OR role IN "
    "('PRIMARY_ALPHA', 'DIVERSIFIER_ALPHA', 'HEDGE_ALPHA', "
    "'REGIME_SIGNAL', 'RISK_MODULATOR', 'SHADOW_ALPHA') OR "
    "(role = 'RISK_SIGNAL' AND evaluation_result_id IS NULL)"
)


def _create_dataset_selections() -> None:
    op.create_table(
        "evaluation_dataset_selections",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("universe_version_id", sa.Uuid(), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("discovery_dataset_revision_id", sa.Uuid(), nullable=False),
        sa.Column("validation_dataset_revision_id", sa.Uuid(), nullable=False),
        sa.Column("sealed_dataset_revision_id", sa.Uuid(), nullable=False),
        sa.Column("state", sa.String(length=20), nullable=False, server_default="ENABLED"),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint("version_no > 0", name="ck_evaluation_dataset_selection_version"),
        sa.CheckConstraint(
            "state IN ('ENABLED', 'RETIRED')",
            name="ck_evaluation_dataset_selection_state",
        ),
        sa.CheckConstraint(
            "discovery_dataset_revision_id <> validation_dataset_revision_id AND "
            "discovery_dataset_revision_id <> sealed_dataset_revision_id AND "
            "validation_dataset_revision_id <> sealed_dataset_revision_id",
            name="ck_evaluation_dataset_selection_distinct_revisions",
        ),
        sa.ForeignKeyConstraint(
            ["universe_version_id"], ["market_universe_versions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["discovery_dataset_revision_id"], ["dataset_revisions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["validation_dataset_revision_id"], ["dataset_revisions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["sealed_dataset_revision_id"], ["dataset_revisions.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "universe_version_id",
            "version_no",
            name="uq_evaluation_dataset_selection_version",
        ),
    )
    op.create_index(
        "uq_evaluation_dataset_selection_enabled",
        "evaluation_dataset_selections",
        ["universe_version_id"],
        unique=True,
        sqlite_where=sa.text("state = 'ENABLED'"),
        postgresql_where=sa.text("state = 'ENABLED'"),
    )


def _create_evaluation_designs() -> None:
    op.create_table(
        "evaluation_design_versions",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("universe_version_id", sa.Uuid(), nullable=False),
        sa.Column("contract_version", sa.String(length=40), nullable=False),
        sa.Column("allowed_model_mode", sa.String(length=20), nullable=False),
        sa.Column("qualification_role", sa.String(length=40), nullable=False),
        sa.Column("walk_forward_folds", sa.Integer(), nullable=False),
        sa.Column("annualization_factor", sa.Numeric(20, 8), nullable=False),
        sa.Column("multiple_testing_method", sa.String(length=40), nullable=False),
        sa.Column("multiple_testing_max_trials", sa.Integer(), nullable=False),
        sa.Column("qualification_metric_code", sa.String(length=100), nullable=False),
        sa.Column("qualification_comparator", sa.String(length=20), nullable=False),
        sa.Column("qualification_threshold", sa.Numeric(20, 8), nullable=False),
        sa.Column("pass_disclosure_code", sa.String(length=100), nullable=False),
        sa.Column("failure_disclosure_code", sa.String(length=100), nullable=False),
        sa.Column("inconclusive_disclosure_code", sa.String(length=100), nullable=False),
        sa.Column("invalid_disclosure_code", sa.String(length=100), nullable=False),
        sa.Column("state", sa.String(length=20), nullable=False, server_default="ACTIVE"),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint("version_no > 0", name="ck_evaluation_design_version_number"),
        sa.CheckConstraint("length(contract_version) > 0", name="ck_evaluation_design_contract"),
        sa.CheckConstraint(
            "allowed_model_mode IN ('RELATIVE_SCORE', 'CALIBRATED_RETURN')",
            name="ck_evaluation_design_model_mode",
        ),
        sa.CheckConstraint(
            "qualification_role IN "
            "('PRIMARY_ALPHA', 'DIVERSIFIER_ALPHA', 'HEDGE_ALPHA', "
            "'REGIME_SIGNAL', 'RISK_MODULATOR', 'SHADOW_ALPHA')",
            name="ck_evaluation_design_role",
        ),
        sa.CheckConstraint("walk_forward_folds > 0", name="ck_evaluation_design_walk_forward"),
        sa.CheckConstraint("annualization_factor > 0", name="ck_evaluation_design_annualization"),
        sa.CheckConstraint(
            "multiple_testing_method IN ('BONFERRONI', 'BENJAMINI_HOCHBERG')",
            name="ck_evaluation_design_multiple_testing_method",
        ),
        sa.CheckConstraint(
            "multiple_testing_max_trials > 0",
            name="ck_evaluation_design_multiple_testing_trials",
        ),
        sa.CheckConstraint(
            "qualification_comparator IN ('MINIMUM', 'MAXIMUM')",
            name="ck_evaluation_design_qualification_comparator",
        ),
        sa.CheckConstraint(
            "length(qualification_metric_code) > 0",
            name="ck_evaluation_design_qualification_metric",
        ),
        sa.CheckConstraint(
            "length(pass_disclosure_code) > 0 AND length(failure_disclosure_code) > 0 "
            "AND length(inconclusive_disclosure_code) > 0 "
            "AND length(invalid_disclosure_code) > 0",
            name="ck_evaluation_design_disclosure_codes",
        ),
        sa.CheckConstraint("state IN ('ACTIVE', 'RETIRED')", name="ck_evaluation_design_state"),
        sa.ForeignKeyConstraint(
            ["universe_version_id"], ["market_universe_versions.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "universe_version_id", "version_no", name="uq_evaluation_design_version"
        ),
    )


def _create_promotion_policies() -> None:
    op.create_table(
        "promotion_policy_versions",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("purpose", sa.String(length=40), nullable=False),
        sa.Column("mode", sa.String(length=40), nullable=False),
        sa.Column("paper_downstream_system_id", sa.Uuid()),
        sa.Column("live_downstream_system_id", sa.Uuid()),
        sa.Column("state", sa.String(length=20), nullable=False, server_default="ACTIVE"),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint("version_no > 0", name="ck_promotion_policy_version_number"),
        sa.CheckConstraint(
            "purpose IN ('ALPHA_DISCOVERY_TO_SEALED', 'SEALED_TO_QUALIFIED', "
            "'PORTFOLIO_TO_PAPER', 'PAPER_TO_LIVE')",
            name="ck_promotion_policy_version_purpose",
        ),
        sa.CheckConstraint(
            "mode IN ('MANUAL_APPROVAL', 'AUTO_HANDOFF')",
            name="ck_promotion_policy_version_mode",
        ),
        sa.CheckConstraint(
            "state IN ('ACTIVE', 'RETIRED')",
            name="ck_promotion_policy_version_state",
        ),
        sa.CheckConstraint(
            "(purpose IN ('ALPHA_DISCOVERY_TO_SEALED', 'SEALED_TO_QUALIFIED') "
            "AND paper_downstream_system_id IS NULL AND live_downstream_system_id IS NULL) OR "
            "(purpose = 'PORTFOLIO_TO_PAPER' "
            "AND paper_downstream_system_id IS NOT NULL AND live_downstream_system_id IS NULL) OR "
            "(purpose = 'PAPER_TO_LIVE' "
            "AND paper_downstream_system_id IS NOT NULL AND live_downstream_system_id IS NOT NULL)",
            name="ck_promotion_policy_version_downstreams",
        ),
        sa.ForeignKeyConstraint(
            ["paper_downstream_system_id"], ["downstream_systems.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["live_downstream_system_id"], ["downstream_systems.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("purpose", "version_no", name="uq_promotion_policy_version"),
    )
    op.create_table(
        "promotion_policy_gates",
        sa.Column("policy_version_id", sa.Uuid(), nullable=False),
        sa.Column("metric_code", sa.String(length=100), nullable=False),
        sa.Column("comparator", sa.String(length=20), nullable=False),
        sa.Column("threshold", sa.Numeric(20, 8), nullable=False),
        sa.Column("ordinal", sa.Integer(), nullable=False),
        sa.CheckConstraint("ordinal > 0", name="ck_promotion_policy_gate_ordinal"),
        sa.CheckConstraint("length(metric_code) > 0", name="ck_promotion_policy_gate_metric"),
        sa.CheckConstraint(
            "comparator IN ('MINIMUM', 'MAXIMUM')",
            name="ck_promotion_policy_gate_comparator",
        ),
        sa.ForeignKeyConstraint(
            ["policy_version_id"], ["promotion_policy_versions.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("policy_version_id", "metric_code"),
        sa.UniqueConstraint("policy_version_id", "ordinal", name="uq_promotion_policy_gate_ordinal"),
    )


def _extend_mission_state() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("research_missions", recreate="always") as batch:
            batch.drop_constraint("ck_research_mission_state", type_="check")
            batch.create_check_constraint("ck_research_mission_state", _TRUSTED_MISSION_STATE)
        return
    op.drop_constraint("ck_research_mission_state", "research_missions", type_="check")
    op.create_check_constraint(
        "ck_research_mission_state", "research_missions", _TRUSTED_MISSION_STATE
    )


def _restore_legacy_mission_state() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("research_missions", recreate="always") as batch:
            batch.drop_constraint("ck_research_mission_state", type_="check")
            batch.create_check_constraint("ck_research_mission_state", _LEGACY_MISSION_STATE)
        return
    op.drop_constraint("ck_research_mission_state", "research_missions", type_="check")
    op.create_check_constraint(
        "ck_research_mission_state", "research_missions", _LEGACY_MISSION_STATE
    )


def _add_alpha_model_artifact_reference() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_model_versions", recreate="always") as batch:
            batch.add_column(sa.Column("source_mission_artifact_id", sa.Uuid()))
            batch.add_column(sa.Column("source_mission_artifact_revision", sa.Integer()))
            batch.create_check_constraint(
                "ck_alpha_model_version_source_artifact",
                "(source_mission_artifact_id IS NULL AND source_mission_artifact_revision IS NULL) "
                "OR (source_mission_artifact_id IS NOT NULL "
                "AND source_mission_artifact_revision IS NOT NULL "
                "AND source_mission_artifact_revision > 0)",
            )
            batch.create_foreign_key(
                "fk_alpha_model_version_source_artifact",
                "mission_artifacts",
                ["source_mission_artifact_id"],
                ["id"],
                ondelete="RESTRICT",
            )
        return
    op.add_column("alpha_model_versions", sa.Column("source_mission_artifact_id", sa.Uuid()))
    op.add_column("alpha_model_versions", sa.Column("source_mission_artifact_revision", sa.Integer()))
    op.create_check_constraint(
        "ck_alpha_model_version_source_artifact",
        "alpha_model_versions",
        "(source_mission_artifact_id IS NULL AND source_mission_artifact_revision IS NULL) "
        "OR (source_mission_artifact_id IS NOT NULL "
        "AND source_mission_artifact_revision IS NOT NULL "
        "AND source_mission_artifact_revision > 0)",
    )
    op.create_foreign_key(
        "fk_alpha_model_version_source_artifact",
        "alpha_model_versions",
        "mission_artifacts",
        ["source_mission_artifact_id"],
        ["id"],
        ondelete="RESTRICT",
    )


def _create_discovery_evaluations() -> None:
    op.create_table(
        "alpha_discovery_evaluations",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("source_mission_artifact_id", sa.Uuid(), nullable=False),
        sa.Column("source_mission_artifact_revision", sa.Integer(), nullable=False),
        sa.Column("alpha_model_version_id", sa.Uuid(), nullable=False),
        sa.Column("program_id", sa.Uuid(), nullable=False),
        sa.Column("cycle_id", sa.Uuid(), nullable=False),
        sa.Column("branch_id", sa.Uuid(), nullable=False),
        sa.Column("mission_id", sa.Uuid(), nullable=False),
        sa.Column("discovery_dataset_revision_id", sa.Uuid(), nullable=False),
        sa.Column("cause_event_id", _EVENT_ID, nullable=False),
        sa.Column("evaluator_contract_version", sa.String(length=40), nullable=False),
        sa.Column("state", sa.String(length=20), nullable=False, server_default="FROZEN"),
        sa.Column("outcome_code", sa.String(length=100)),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.Column("completed_at", sa.DateTime(timezone=True)),
        sa.CheckConstraint(
            "source_mission_artifact_revision > 0",
            name="ck_alpha_discovery_evaluation_artifact_revision",
        ),
        sa.CheckConstraint(
            "length(evaluator_contract_version) > 0",
            name="ck_alpha_discovery_evaluation_contract",
        ),
        sa.CheckConstraint(
            "state IN ('FROZEN', 'QUEUED', 'RUNNING', 'VALID', "
            "'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_discovery_evaluation_state",
        ),
        sa.CheckConstraint(
            "(state IN ('FROZEN', 'QUEUED', 'RUNNING') "
            "AND outcome_code IS NULL AND completed_at IS NULL) OR "
            "(state IN ('VALID', 'INCONCLUSIVE', 'INVALID') "
            "AND outcome_code IS NOT NULL AND length(outcome_code) > 0 "
            "AND completed_at IS NOT NULL)",
            name="ck_alpha_discovery_evaluation_completion",
        ),
        sa.ForeignKeyConstraint(
            ["source_mission_artifact_id"], ["mission_artifacts.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["alpha_model_version_id"], ["alpha_model_versions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(["program_id"], ["research_programs.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(["cycle_id"], ["research_cycles.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(["branch_id"], ["research_branches.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(["mission_id"], ["research_missions.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(
            ["discovery_dataset_revision_id"], ["dataset_revisions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(["cause_event_id"], ["events.id"], ondelete="RESTRICT"),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "source_mission_artifact_id",
            "cause_event_id",
            name="uq_alpha_discovery_evaluation_source_cause",
        ),
    )
    op.create_index(
        "ix_alpha_discovery_evaluation_program_state",
        "alpha_discovery_evaluations",
        ["program_id", "state"],
    )
    op.create_index(
        "ix_alpha_discovery_evaluation_mission",
        "alpha_discovery_evaluations",
        ["mission_id", "created_at"],
    )


def _create_assignments() -> None:
    op.create_table(
        "alpha_evaluation_assignments",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("source_mission_artifact_id", sa.Uuid(), nullable=False),
        sa.Column("source_mission_artifact_revision", sa.Integer(), nullable=False),
        sa.Column("discovery_evaluation_id", sa.Uuid(), nullable=False),
        sa.Column("program_id", sa.Uuid(), nullable=False),
        sa.Column("cycle_id", sa.Uuid(), nullable=False),
        sa.Column("branch_id", sa.Uuid(), nullable=False),
        sa.Column("mission_id", sa.Uuid(), nullable=False),
        sa.Column("alpha_model_version_id", sa.Uuid(), nullable=False),
        sa.Column("alpha_calibration_version_id", sa.Uuid()),
        sa.Column("universe_version_id", sa.Uuid(), nullable=False),
        sa.Column("sealed_dataset_revision_id", sa.Uuid(), nullable=False),
        sa.Column("evaluation_design_version_id", sa.Uuid(), nullable=False),
        sa.Column("promotion_policy_version_id", sa.Uuid(), nullable=False),
        sa.Column("cause_event_id", _EVENT_ID, nullable=False),
        sa.Column("assignment_no", sa.Integer(), nullable=False),
        sa.Column("evaluator_contract_version", sa.String(length=40), nullable=False),
        sa.Column("state", sa.String(length=20), nullable=False, server_default="FROZEN"),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint(
            "source_mission_artifact_revision > 0",
            name="ck_alpha_evaluation_assignment_artifact_revision",
        ),
        sa.CheckConstraint(
            "assignment_no > 0",
            name="ck_alpha_evaluation_assignment_number",
        ),
        sa.CheckConstraint(
            "length(evaluator_contract_version) > 0",
            name="ck_alpha_evaluation_assignment_contract",
        ),
        sa.CheckConstraint(
            "state IN ('FROZEN', 'QUEUED', 'RUNNING', 'FINALIZED', 'INVALIDATED')",
            name="ck_alpha_evaluation_assignment_state",
        ),
        sa.ForeignKeyConstraint(
            ["source_mission_artifact_id"], ["mission_artifacts.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["discovery_evaluation_id"], ["alpha_discovery_evaluations.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(["program_id"], ["research_programs.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(["cycle_id"], ["research_cycles.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(["branch_id"], ["research_branches.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(["mission_id"], ["research_missions.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(
            ["alpha_model_version_id"], ["alpha_model_versions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["alpha_calibration_version_id"],
            ["alpha_calibration_versions.id"],
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["universe_version_id"], ["market_universe_versions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["sealed_dataset_revision_id"], ["dataset_revisions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["evaluation_design_version_id"], ["evaluation_design_versions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["promotion_policy_version_id"], ["promotion_policy_versions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(["cause_event_id"], ["events.id"], ondelete="RESTRICT"),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "source_mission_artifact_id",
            "cause_event_id",
            name="uq_alpha_evaluation_assignment_source_cause",
        ),
        sa.UniqueConstraint(
            "alpha_model_version_id",
            "cycle_id",
            "assignment_no",
            name="uq_alpha_evaluation_assignment_number",
        ),
        sa.UniqueConstraint(
            "discovery_evaluation_id",
            name="uq_alpha_evaluation_assignment_discovery_evaluation",
        ),
    )
    op.create_index(
        "ix_alpha_evaluation_assignment_program_state",
        "alpha_evaluation_assignments",
        ["program_id", "state"],
    )
    op.create_index(
        "ix_alpha_evaluation_assignment_mission",
        "alpha_evaluation_assignments",
        ["mission_id", "created_at"],
    )
    op.create_table(
        "alpha_evaluation_assignment_dataset_revisions",
        sa.Column("assignment_id", sa.Uuid(), nullable=False),
        sa.Column("phase", sa.String(length=20), nullable=False),
        sa.Column("ordinal", sa.Integer(), nullable=False),
        sa.Column("dataset_revision_id", sa.Uuid(), nullable=False),
        sa.CheckConstraint(
            "phase IN ('DISCOVERY', 'VALIDATION', 'SEALED')",
            name="ck_alpha_evaluation_assignment_dataset_phase",
        ),
        sa.CheckConstraint(
            "ordinal > 0",
            name="ck_alpha_evaluation_assignment_dataset_ordinal",
        ),
        sa.ForeignKeyConstraint(
            ["assignment_id"], ["alpha_evaluation_assignments.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["dataset_revision_id"], ["dataset_revisions.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("assignment_id", "phase", "ordinal"),
        sa.UniqueConstraint(
            "assignment_id",
            "dataset_revision_id",
            name="uq_alpha_evaluation_assignment_dataset_revision",
        ),
    )


def _extend_episodes() -> None:
    columns = (
        sa.Column("assignment_id", sa.Uuid()),
        sa.Column("sealed_at", sa.DateTime(timezone=True)),
        sa.Column("evaluated_at", sa.DateTime(timezone=True)),
        sa.Column("disclosed_at", sa.DateTime(timezone=True)),
        sa.Column("consumed_at", sa.DateTime(timezone=True)),
        sa.Column("invalid_reason", sa.String(length=100)),
    )
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_evaluation_episodes", recreate="always") as batch:
            for column in columns:
                batch.add_column(column)
            batch.drop_constraint("ck_alpha_evaluation_episode_state", type_="check")
            batch.create_check_constraint("ck_alpha_evaluation_episode_state", _TRUSTED_EPISODE_STATE)
            batch.create_foreign_key(
                "fk_alpha_evaluation_episode_assignment",
                "alpha_evaluation_assignments",
                ["assignment_id"],
                ["id"],
                ondelete="RESTRICT",
            )
    else:
        for column in columns:
            op.add_column("alpha_evaluation_episodes", column)
        op.drop_constraint(
            "ck_alpha_evaluation_episode_state",
            "alpha_evaluation_episodes",
            type_="check",
        )
        op.create_check_constraint(
            "ck_alpha_evaluation_episode_state",
            "alpha_evaluation_episodes",
            _TRUSTED_EPISODE_STATE,
        )
        op.create_foreign_key(
            "fk_alpha_evaluation_episode_assignment",
            "alpha_evaluation_episodes",
            "alpha_evaluation_assignments",
            ["assignment_id"],
            ["id"],
            ondelete="RESTRICT",
        )
    op.create_index(
        "uq_alpha_evaluation_episode_assignment",
        "alpha_evaluation_episodes",
        ["assignment_id"],
        unique=True,
        sqlite_where=sa.text("assignment_id IS NOT NULL"),
        postgresql_where=sa.text("assignment_id IS NOT NULL"),
    )


def _create_results_and_exposure() -> None:
    op.create_table(
        "alpha_evaluation_results",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("episode_id", sa.Uuid(), nullable=False),
        sa.Column("evidence_validity", sa.String(length=20), nullable=False),
        sa.Column("result", sa.String(length=20), nullable=False),
        sa.Column("private_result_ref", sa.Uuid(), nullable=False),
        sa.Column("evaluated_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint(
            "evidence_validity IN ('VALID', 'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_evaluation_result_validity",
        ),
        sa.CheckConstraint(
            "result IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_evaluation_result_result",
        ),
        sa.CheckConstraint(
            "(evidence_validity = 'VALID' AND result IN ('PASS', 'FAIL')) OR "
            "(evidence_validity = 'INCONCLUSIVE' AND result = 'INCONCLUSIVE') OR "
            "(evidence_validity = 'INVALID' AND result = 'INVALID')",
            name="ck_alpha_evaluation_result_consistency",
        ),
        sa.ForeignKeyConstraint(
            ["episode_id"], ["alpha_evaluation_episodes.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("episode_id", name="uq_alpha_evaluation_result_episode"),
    )
    op.create_table(
        "alpha_evaluation_metrics",
        sa.Column("result_id", sa.Uuid(), nullable=False),
        sa.Column("metric_code", sa.String(length=100), nullable=False),
        sa.Column("phase", sa.String(length=20), nullable=False),
        sa.Column("value", sa.Numeric(20, 8)),
        sa.Column("status", sa.String(length=20), nullable=False),
        sa.CheckConstraint("length(metric_code) > 0", name="ck_alpha_evaluation_metric_code"),
        sa.CheckConstraint(
            "phase IN ('DISCOVERY', 'VALIDATION', 'SEALED')",
            name="ck_alpha_evaluation_metric_phase",
        ),
        sa.CheckConstraint(
            "status IN ('AVAILABLE', 'NOT_AVAILABLE')",
            name="ck_alpha_evaluation_metric_status",
        ),
        sa.CheckConstraint(
            "(status = 'AVAILABLE' AND value IS NOT NULL) OR "
            "(status = 'NOT_AVAILABLE' AND value IS NULL)",
            name="ck_alpha_evaluation_metric_value",
        ),
        sa.ForeignKeyConstraint(
            ["result_id"], ["alpha_evaluation_results.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("result_id", "metric_code", "phase"),
    )
    op.create_table(
        "alpha_evaluation_gates",
        sa.Column("result_id", sa.Uuid(), nullable=False),
        sa.Column("gate_code", sa.String(length=100), nullable=False),
        sa.Column("status", sa.String(length=20), nullable=False),
        sa.Column("reason_code", sa.String(length=100)),
        sa.CheckConstraint("length(gate_code) > 0", name="ck_alpha_evaluation_gate_code"),
        sa.CheckConstraint(
            "status IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_evaluation_gate_status",
        ),
        sa.CheckConstraint(
            "(status = 'PASS' AND reason_code IS NULL) OR "
            "(status <> 'PASS' AND reason_code IS NOT NULL AND length(reason_code) > 0)",
            name="ck_alpha_evaluation_gate_reason",
        ),
        sa.ForeignKeyConstraint(
            ["result_id"], ["alpha_evaluation_results.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("result_id", "gate_code"),
    )
    op.create_table(
        "evidence_exposures",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("episode_id", sa.Uuid(), nullable=False),
        sa.Column("subject_type", sa.String(length=40), nullable=False),
        sa.Column("subject_id", sa.Uuid(), nullable=False),
        sa.Column("level", sa.Integer(), nullable=False),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint(
            "subject_type IN ('PROGRAM', 'BRANCH', 'MISSION', 'ALPHA_MODEL', "
            "'ALPHA_QUALIFICATION')",
            name="ck_evidence_exposure_subject_type",
        ),
        sa.CheckConstraint("level BETWEEN 1 AND 3", name="ck_evidence_exposure_level"),
        sa.ForeignKeyConstraint(
            ["episode_id"], ["alpha_evaluation_episodes.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "episode_id",
            "subject_type",
            "subject_id",
            "level",
            name="uq_evidence_exposure_subject_level",
        ),
    )
    op.create_table(
        "disclosures",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("episode_id", sa.Uuid(), nullable=False),
        sa.Column("audience", sa.String(length=20), nullable=False),
        sa.Column("level", sa.Integer(), nullable=False),
        sa.Column("classification_code", sa.String(length=100), nullable=False),
        sa.Column("reason_code", sa.String(length=100)),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint(
            "(audience = 'CODEX' AND level = 1) OR "
            "(audience = 'OPERATOR' AND level = 2) OR "
            "(audience = 'POSTMORTEM' AND level = 3)",
            name="ck_disclosure_audience_level",
        ),
        sa.CheckConstraint("length(classification_code) > 0", name="ck_disclosure_classification"),
        sa.CheckConstraint(
            "(classification_code = 'QUALIFIED' AND reason_code IS NULL) OR "
            "(classification_code <> 'QUALIFIED' AND reason_code IS NOT NULL "
            "AND length(reason_code) > 0)",
            name="ck_disclosure_reason",
        ),
        sa.ForeignKeyConstraint(
            ["episode_id"], ["alpha_evaluation_episodes.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("episode_id", "audience", "level", name="uq_disclosure_audience_level"),
    )


def _add_trusted_result_references() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_signal_artifacts", recreate="always") as batch:
            batch.add_column(sa.Column("evaluation_result_id", sa.Uuid()))
            batch.alter_column("run_id", existing_type=sa.Uuid(), nullable=True)
            batch.create_check_constraint(
                "ck_alpha_signal_artifact_provenance",
                "run_id IS NOT NULL OR evaluation_result_id IS NOT NULL",
            )
            batch.create_foreign_key(
                "fk_alpha_signal_artifact_evaluation_result",
                "alpha_evaluation_results",
                ["evaluation_result_id"],
                ["id"],
                ondelete="RESTRICT",
            )
        with op.batch_alter_table("alpha_qualifications", recreate="always") as batch:
            batch.add_column(sa.Column("evaluation_result_id", sa.Uuid()))
            batch.drop_constraint("ck_alpha_qualification_canonical_role", type_="check")
            batch.create_check_constraint(
                "ck_alpha_qualification_canonical_role", _TRUSTED_QUALIFICATION_ROLE
            )
            batch.create_foreign_key(
                "fk_alpha_qualification_evaluation_result",
                "alpha_evaluation_results",
                ["evaluation_result_id"],
                ["id"],
                ondelete="RESTRICT",
            )
    else:
        op.add_column("alpha_signal_artifacts", sa.Column("evaluation_result_id", sa.Uuid()))
        op.alter_column(
            "alpha_signal_artifacts",
            "run_id",
            existing_type=sa.Uuid(),
            nullable=True,
        )
        op.create_check_constraint(
            "ck_alpha_signal_artifact_provenance",
            "alpha_signal_artifacts",
            "run_id IS NOT NULL OR evaluation_result_id IS NOT NULL",
        )
        op.create_foreign_key(
            "fk_alpha_signal_artifact_evaluation_result",
            "alpha_signal_artifacts",
            "alpha_evaluation_results",
            ["evaluation_result_id"],
            ["id"],
            ondelete="RESTRICT",
        )
        op.add_column("alpha_qualifications", sa.Column("evaluation_result_id", sa.Uuid()))
        op.drop_constraint(
            "ck_alpha_qualification_canonical_role",
            "alpha_qualifications",
            type_="check",
        )
        op.create_check_constraint(
            "ck_alpha_qualification_canonical_role",
            "alpha_qualifications",
            _TRUSTED_QUALIFICATION_ROLE,
        )
        op.create_foreign_key(
            "fk_alpha_qualification_evaluation_result",
            "alpha_qualifications",
            "alpha_evaluation_results",
            ["evaluation_result_id"],
            ["id"],
            ondelete="RESTRICT",
        )
    for table, index in (
        ("alpha_signal_artifacts", "uq_alpha_signal_artifact_evaluation_result"),
        ("alpha_qualifications", "uq_alpha_qualification_evaluation_result"),
    ):
        op.create_index(
            index,
            table,
            ["evaluation_result_id"],
            unique=True,
            sqlite_where=sa.text("evaluation_result_id IS NOT NULL"),
            postgresql_where=sa.text("evaluation_result_id IS NOT NULL"),
        )


def upgrade() -> None:
    _create_dataset_selections()
    _create_evaluation_designs()
    _create_promotion_policies()
    _extend_mission_state()
    _add_alpha_model_artifact_reference()
    _create_discovery_evaluations()
    _create_assignments()
    _extend_episodes()
    _create_results_and_exposure()
    _add_trusted_result_references()


def _drop_trusted_result_references() -> None:
    for table, index in (
        ("alpha_qualifications", "uq_alpha_qualification_evaluation_result"),
        ("alpha_signal_artifacts", "uq_alpha_signal_artifact_evaluation_result"),
    ):
        op.drop_index(index, table_name=table)
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_qualifications", recreate="always") as batch:
            batch.drop_constraint(
                "fk_alpha_qualification_evaluation_result", type_="foreignkey"
            )
            batch.drop_constraint("ck_alpha_qualification_canonical_role", type_="check")
            batch.create_check_constraint(
                "ck_alpha_qualification_canonical_role", _LEGACY_QUALIFICATION_ROLE
            )
            batch.drop_column("evaluation_result_id")
        with op.batch_alter_table("alpha_signal_artifacts", recreate="always") as batch:
            batch.drop_constraint(
                "fk_alpha_signal_artifact_evaluation_result", type_="foreignkey"
            )
            batch.drop_constraint("ck_alpha_signal_artifact_provenance", type_="check")
            batch.alter_column("run_id", existing_type=sa.Uuid(), nullable=False)
            batch.drop_column("evaluation_result_id")
        return
    op.drop_constraint(
        "fk_alpha_qualification_evaluation_result",
        "alpha_qualifications",
        type_="foreignkey",
    )
    op.drop_constraint(
        "ck_alpha_qualification_canonical_role",
        "alpha_qualifications",
        type_="check",
    )
    op.create_check_constraint(
        "ck_alpha_qualification_canonical_role",
        "alpha_qualifications",
        _LEGACY_QUALIFICATION_ROLE,
    )
    op.drop_column("alpha_qualifications", "evaluation_result_id")
    op.drop_constraint(
        "fk_alpha_signal_artifact_evaluation_result",
        "alpha_signal_artifacts",
        type_="foreignkey",
    )
    op.drop_constraint(
        "ck_alpha_signal_artifact_provenance",
        "alpha_signal_artifacts",
        type_="check",
    )
    op.alter_column(
        "alpha_signal_artifacts",
        "run_id",
        existing_type=sa.Uuid(),
        nullable=False,
    )
    op.drop_column("alpha_signal_artifacts", "evaluation_result_id")


def _shrink_episodes() -> None:
    op.drop_index("uq_alpha_evaluation_episode_assignment", table_name="alpha_evaluation_episodes")
    columns = (
        "invalid_reason",
        "consumed_at",
        "disclosed_at",
        "evaluated_at",
        "sealed_at",
        "assignment_id",
    )
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_evaluation_episodes", recreate="always") as batch:
            batch.drop_constraint("fk_alpha_evaluation_episode_assignment", type_="foreignkey")
            batch.drop_constraint("ck_alpha_evaluation_episode_state", type_="check")
            batch.create_check_constraint("ck_alpha_evaluation_episode_state", _LEGACY_EPISODE_STATE)
            for column in columns:
                batch.drop_column(column)
        return
    op.drop_constraint(
        "fk_alpha_evaluation_episode_assignment",
        "alpha_evaluation_episodes",
        type_="foreignkey",
    )
    op.drop_constraint(
        "ck_alpha_evaluation_episode_state",
        "alpha_evaluation_episodes",
        type_="check",
    )
    op.create_check_constraint(
        "ck_alpha_evaluation_episode_state",
        "alpha_evaluation_episodes",
        _LEGACY_EPISODE_STATE,
    )
    for column in columns:
        op.drop_column("alpha_evaluation_episodes", column)


def _drop_assignments() -> None:
    op.drop_table("alpha_evaluation_assignment_dataset_revisions")
    op.drop_index("ix_alpha_evaluation_assignment_mission", table_name="alpha_evaluation_assignments")
    op.drop_index(
        "ix_alpha_evaluation_assignment_program_state",
        table_name="alpha_evaluation_assignments",
    )
    op.drop_table("alpha_evaluation_assignments")


def _drop_discovery_evaluations() -> None:
    op.drop_index(
        "ix_alpha_discovery_evaluation_mission",
        table_name="alpha_discovery_evaluations",
    )
    op.drop_index(
        "ix_alpha_discovery_evaluation_program_state",
        table_name="alpha_discovery_evaluations",
    )
    op.drop_table("alpha_discovery_evaluations")


def _drop_alpha_model_artifact_reference() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("alpha_model_versions", recreate="always") as batch:
            batch.drop_constraint("fk_alpha_model_version_source_artifact", type_="foreignkey")
            batch.drop_constraint("ck_alpha_model_version_source_artifact", type_="check")
            batch.drop_column("source_mission_artifact_revision")
            batch.drop_column("source_mission_artifact_id")
        return
    op.drop_constraint(
        "fk_alpha_model_version_source_artifact",
        "alpha_model_versions",
        type_="foreignkey",
    )
    op.drop_constraint(
        "ck_alpha_model_version_source_artifact",
        "alpha_model_versions",
        type_="check",
    )
    op.drop_column("alpha_model_versions", "source_mission_artifact_revision")
    op.drop_column("alpha_model_versions", "source_mission_artifact_id")


def _require_empty_trusted_facts_for_downgrade() -> None:
    """Do not erase immutable 0023 facts or force trusted artifacts into legacy shape."""

    bind = op.get_bind()
    queries = (
        "SELECT 1 FROM disclosures LIMIT 1",
        "SELECT 1 FROM evidence_exposures LIMIT 1",
        "SELECT 1 FROM alpha_evaluation_gates LIMIT 1",
        "SELECT 1 FROM alpha_evaluation_metrics LIMIT 1",
        "SELECT 1 FROM alpha_evaluation_results LIMIT 1",
        "SELECT 1 FROM alpha_evaluation_assignment_dataset_revisions LIMIT 1",
        "SELECT 1 FROM alpha_evaluation_assignments LIMIT 1",
        "SELECT 1 FROM alpha_discovery_evaluations LIMIT 1",
        "SELECT 1 FROM promotion_policy_gates LIMIT 1",
        "SELECT 1 FROM promotion_policy_versions LIMIT 1",
        "SELECT 1 FROM evaluation_design_versions LIMIT 1",
        "SELECT 1 FROM evaluation_dataset_selections LIMIT 1",
        "SELECT 1 FROM alpha_model_versions "
        "WHERE source_mission_artifact_id IS NOT NULL LIMIT 1",
        "SELECT 1 FROM alpha_evaluation_episodes WHERE assignment_id IS NOT NULL LIMIT 1",
        "SELECT 1 FROM alpha_signal_artifacts WHERE evaluation_result_id IS NOT NULL LIMIT 1",
        "SELECT 1 FROM alpha_qualifications WHERE evaluation_result_id IS NOT NULL LIMIT 1",
        "SELECT 1 FROM research_missions WHERE state = 'AWAITING_VALIDATION' LIMIT 1",
    )
    if any(bind.execute(sa.text(query)).first() is not None for query in queries):
        raise RuntimeError("TRUSTED_ALPHA_EVALUATION_DOWNGRADE_BLOCKED")


def downgrade() -> None:
    _require_empty_trusted_facts_for_downgrade()
    _drop_trusted_result_references()
    for table in (
        "disclosures",
        "evidence_exposures",
        "alpha_evaluation_gates",
        "alpha_evaluation_metrics",
        "alpha_evaluation_results",
    ):
        op.drop_table(table)
    _shrink_episodes()
    _drop_assignments()
    _drop_discovery_evaluations()
    _drop_alpha_model_artifact_reference()
    _restore_legacy_mission_state()
    op.drop_table("promotion_policy_gates")
    op.drop_table("promotion_policy_versions")
    op.drop_table("evaluation_design_versions")
    op.drop_index(
        "uq_evaluation_dataset_selection_enabled",
        table_name="evaluation_dataset_selections",
    )
    op.drop_table("evaluation_dataset_selections")
