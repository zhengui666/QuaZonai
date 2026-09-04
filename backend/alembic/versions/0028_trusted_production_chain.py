"""Persist the trusted Alpha-to-Portfolio-to-Promotion production lineage.

Revision ID: 0028_trusted_production_chain
Revises: 0027_candidate_package_build
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import JSONB


revision = "0028_trusted_production_chain"
down_revision = "0027_candidate_package_build"
branch_labels = None
depends_on = None


_JSON = sa.JSON().with_variant(JSONB(), "postgresql")
_EVENT_ID = sa.BigInteger().with_variant(sa.Integer(), "sqlite")
_FINITE = "lower(CAST({column} AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')"
_OLD_POLICY_DOWNSTREAMS = (
    "(purpose IN ('ALPHA_DISCOVERY_TO_SEALED', 'SEALED_TO_QUALIFIED') "
    "AND paper_downstream_system_id IS NULL AND live_downstream_system_id IS NULL) OR "
    "(purpose = 'PORTFOLIO_TO_PAPER' "
    "AND paper_downstream_system_id IS NOT NULL AND live_downstream_system_id IS NULL) OR "
    "(purpose = 'PAPER_TO_LIVE' "
    "AND paper_downstream_system_id IS NOT NULL AND live_downstream_system_id IS NOT NULL)"
)
_POLICY_LEGACY_TUPLE = (
    "policy_contract_version IS NULL AND paper_connection_version_id IS NULL "
    "AND paper_feedback_contract_version_id IS NULL AND paper_preflight_receipt_id IS NULL "
    "AND live_connection_version_id IS NULL "
    "AND live_feedback_contract_version_id IS NULL AND live_preflight_receipt_id IS NULL "
    "AND paper_to_live_policy_version_id IS NULL AND "
    "((purpose IN ('ALPHA_DISCOVERY_TO_SEALED', 'SEALED_TO_QUALIFIED') "
    "AND paper_downstream_system_id IS NULL AND live_downstream_system_id IS NULL) OR "
    "(purpose = 'PORTFOLIO_TO_PAPER' "
    "AND paper_downstream_system_id IS NOT NULL AND live_downstream_system_id IS NULL) OR "
    "(purpose = 'PAPER_TO_LIVE' "
    "AND paper_downstream_system_id IS NOT NULL AND live_downstream_system_id IS NOT NULL))"
)
_POLICY_TUPLES = (
    f"({_POLICY_LEGACY_TUPLE}) OR "
    "(policy_contract_version = 'PROMOTION_POLICY_V1' "
    "AND purpose IN ('ALPHA_DISCOVERY_TO_SEALED', 'SEALED_TO_QUALIFIED') "
    "AND paper_downstream_system_id IS NULL AND paper_connection_version_id IS NULL "
    "AND paper_feedback_contract_version_id IS NULL AND paper_preflight_receipt_id IS NULL "
    "AND live_downstream_system_id IS NULL AND live_connection_version_id IS NULL "
    "AND live_feedback_contract_version_id IS NULL AND live_preflight_receipt_id IS NULL "
    "AND paper_to_live_policy_version_id IS NULL) OR "
    "(policy_contract_version = 'PROMOTION_POLICY_V1' "
    "AND purpose = 'PORTFOLIO_TO_PAPER' AND mode = 'MANUAL_APPROVAL' "
    "AND paper_downstream_system_id IS NOT NULL AND paper_connection_version_id IS NOT NULL "
    "AND paper_feedback_contract_version_id IS NOT NULL AND paper_preflight_receipt_id IS NOT NULL "
    "AND live_downstream_system_id IS NULL AND live_connection_version_id IS NULL "
    "AND live_feedback_contract_version_id IS NULL AND live_preflight_receipt_id IS NULL "
    "AND paper_to_live_policy_version_id IS NOT NULL) OR "
    "(policy_contract_version = 'PROMOTION_POLICY_V1' AND purpose = 'PAPER_TO_LIVE' "
    "AND paper_downstream_system_id IS NOT NULL AND paper_connection_version_id IS NOT NULL "
    "AND paper_feedback_contract_version_id IS NOT NULL AND paper_preflight_receipt_id IS NOT NULL "
    "AND live_downstream_system_id IS NOT NULL AND live_connection_version_id IS NOT NULL "
    "AND live_feedback_contract_version_id IS NOT NULL AND live_preflight_receipt_id IS NOT NULL "
    "AND paper_to_live_policy_version_id IS NULL)"
)
_JOB_INDEXES = (
    (
        "uq_portfolio_input_evaluation_job_active",
        "kind = 'PORTFOLIO_INPUT_EVALUATION' "
        "AND resource_type = 'portfolio_input_evaluation_assignment' "
        "AND state IN ('READY', 'LEASED')",
    ),
    (
        "uq_portfolio_assembly_job_active",
        "kind = 'PORTFOLIO_ASSEMBLY' AND resource_type = 'portfolio_assembly_input' "
        "AND state IN ('READY', 'LEASED')",
    ),
    (
        "uq_portfolio_evaluation_job_active",
        "kind = 'PORTFOLIO_EVALUATION' "
        "AND resource_type = 'portfolio_evaluation_assignment' "
        "AND state IN ('READY', 'LEASED')",
    ),
    (
        "uq_portfolio_to_paper_promotion_job_active",
        "kind = 'PORTFOLIO_TO_PAPER_PROMOTION' "
        "AND resource_type = 'portfolio_evaluation_episode' "
        "AND state IN ('READY', 'LEASED')",
    ),
    (
        "uq_paper_to_live_promotion_job_active",
        "kind = 'PAPER_TO_LIVE_PROMOTION' "
        "AND resource_type = 'forward_evidence_episode' "
        "AND state IN ('READY', 'LEASED')",
    ),
)


def _require_compatible_history() -> None:
    """Refuse ambiguous active lineage; preserve old nullable facts read-only."""
    bind = op.get_bind()
    checks = (
        (
            "SELECT 1 FROM portfolio_programs GROUP BY mandate_version_id "
            "HAVING count(*) > 1 LIMIT 1",
            "PORTFOLIO_PROGRAM_MANDATE_IDENTITY_CONFLICT",
        ),
        (
            "SELECT 1 FROM portfolio_input_evaluation_assignments "
            "WHERE previous_candidate_id IS NULL AND state IN ('FROZEN', 'QUEUED', 'RUNNING') "
            "GROUP BY portfolio_program_id HAVING count(*) > 1 LIMIT 1",
            "PORTFOLIO_INITIAL_INPUT_INFLIGHT_CONFLICT",
        ),
    )
    for query, code in checks:
        if bind.execute(sa.text(query)).first() is not None:
            raise RuntimeError(code)
    for index_name, predicate in _JOB_INDEXES:
        del index_name
        if bind.execute(
            sa.text(
                "SELECT 1 FROM jobs WHERE "
                f"{predicate} GROUP BY resource_id HAVING count(*) > 1 LIMIT 1"
            )
        ).first() is not None:
            raise RuntimeError("TRUSTED_PRODUCTION_JOB_IDENTITY_CONFLICT")


def _create_feedback_and_connection_tables() -> None:
    op.create_table(
        "feedback_contract_versions",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("downstream_system_id", sa.Uuid(), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("purpose", sa.String(length=20), nullable=False),
        sa.Column("minimum_observation_seconds", sa.Integer(), nullable=False),
        sa.Column("minimum_valid_sample_size", sa.Integer(), nullable=False),
        sa.Column("first_status_deadline_seconds", sa.Integer(), nullable=False),
        sa.Column("complete_feedback_deadline_seconds", sa.Integer(), nullable=False),
        sa.Column("grace_period_seconds", sa.Integer(), nullable=False),
        sa.Column("disclosure_policy", sa.String(length=80), nullable=False),
        sa.Column("spec_json", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("state", sa.String(length=20), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("downstream_system_id", "version_no", name="uq_feedback_contract_version"),
        sa.UniqueConstraint("id", "downstream_system_id", name="uq_feedback_contract_system_pair"),
        sa.CheckConstraint("version_no > 0", name="ck_feedback_contract_version_number"),
        sa.CheckConstraint("purpose IN ('PAPER', 'LIVE')", name="ck_feedback_contract_version_purpose"),
        sa.CheckConstraint("state IN ('ACTIVE', 'RETIRED')", name="ck_feedback_contract_version_state"),
        sa.CheckConstraint(
            "minimum_observation_seconds > 0 AND minimum_valid_sample_size > 0 "
            "AND first_status_deadline_seconds > 0 AND complete_feedback_deadline_seconds > 0 "
            "AND grace_period_seconds >= 0",
            name="ck_feedback_contract_version_timing",
        ),
        sa.CheckConstraint(
            "length(trim(disclosure_policy)) > 0",
            name="ck_feedback_contract_version_contracts",
        ),
        sa.ForeignKeyConstraint(
            ["downstream_system_id"], ["downstream_systems.id"], ondelete="RESTRICT"
        ),
    )
    op.create_table(
        "feedback_contract_metric_requirements",
        sa.Column("feedback_contract_version_id", sa.Uuid(), nullable=False),
        sa.Column("metric_code", sa.String(length=100), nullable=False),
        sa.Column("ordinal", sa.Integer(), nullable=False),
        sa.PrimaryKeyConstraint("feedback_contract_version_id", "metric_code"),
        sa.UniqueConstraint(
            "feedback_contract_version_id", "ordinal", name="uq_feedback_contract_metric_ordinal"
        ),
        sa.CheckConstraint("ordinal > 0", name="ck_feedback_contract_metric_ordinal"),
        sa.CheckConstraint("length(trim(metric_code)) > 0", name="ck_feedback_contract_metric_code"),
        sa.ForeignKeyConstraint(
            ["feedback_contract_version_id"],
            ["feedback_contract_versions.id"],
            ondelete="RESTRICT",
        ),
    )
    for table_name, unique_name, ordinal_name, value_name in (
        (
            "feedback_contract_accepted_package_contracts",
            "uq_feedback_package_contract_ordinal",
            "ck_feedback_package_contract_ordinal",
            "ck_feedback_package_contract_value",
        ),
        (
            "feedback_contract_accepted_arrow_contracts",
            "uq_feedback_arrow_contract_ordinal",
            "ck_feedback_arrow_contract_ordinal",
            "ck_feedback_arrow_contract_value",
        ),
    ):
        op.create_table(
            table_name,
            sa.Column("feedback_contract_version_id", sa.Uuid(), nullable=False),
            sa.Column("contract_version", sa.String(length=40), nullable=False),
            sa.Column("ordinal", sa.Integer(), nullable=False),
            sa.PrimaryKeyConstraint("feedback_contract_version_id", "contract_version"),
            sa.UniqueConstraint("feedback_contract_version_id", "ordinal", name=unique_name),
            sa.CheckConstraint("ordinal > 0", name=ordinal_name),
            sa.CheckConstraint("length(trim(contract_version)) > 0", name=value_name),
            sa.ForeignKeyConstraint(
                ["feedback_contract_version_id"],
                ["feedback_contract_versions.id"],
                ondelete="RESTRICT",
            ),
        )
    op.create_table(
        "downstream_connection_versions",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("downstream_system_id", sa.Uuid(), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("plugin_release_id", sa.Uuid()),
        sa.Column("credential_set_id", sa.Uuid()),
        sa.Column("package_contract_version", sa.String(length=40), nullable=False),
        sa.Column("feedback_contract_version_id", sa.Uuid(), nullable=False),
        sa.Column("public_config", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("state", sa.String(length=20), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "downstream_system_id", "version_no", name="uq_downstream_connection_version"
        ),
        sa.UniqueConstraint(
            "id",
            "downstream_system_id",
            "feedback_contract_version_id",
            name="uq_downstream_connection_policy_tuple",
        ),
        sa.CheckConstraint("version_no > 0", name="ck_downstream_connection_version_number"),
        sa.CheckConstraint(
            "state IN ('ACTIVE', 'RETIRED')", name="ck_downstream_connection_version_state"
        ),
        sa.CheckConstraint(
            "length(trim(package_contract_version)) > 0",
            name="ck_downstream_connection_version_package_contract",
        ),
        sa.ForeignKeyConstraint(
            ["downstream_system_id"], ["downstream_systems.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["feedback_contract_version_id", "downstream_system_id"],
            ["feedback_contract_versions.id", "feedback_contract_versions.downstream_system_id"],
            name="fk_downstream_connection_feedback_contract",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(["plugin_release_id"], ["plugin_releases.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(["credential_set_id"], ["credential_sets.id"], ondelete="RESTRICT"),
    )


def _extend_program_candidate_and_input_identity() -> None:
    bind = op.get_bind()
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("portfolio_programs", recreate="always") as batch:
            batch.create_unique_constraint(
                "uq_portfolio_program_mandate_version", ["mandate_version_id"]
            )
        with op.batch_alter_table("portfolio_candidates", recreate="always") as batch:
            batch.create_unique_constraint(
                "uq_portfolio_candidate_evaluation_lineage",
                [
                    "id",
                    "candidate_family_id",
                    "portfolio_program_id",
                    "mandate_version_id",
                    "assembly_input_id",
                ],
            )
        with op.batch_alter_table("candidate_packages", recreate="always") as batch:
            batch.create_unique_constraint(
                "uq_candidate_package_promotion_lineage", ["id", "candidate_id", "revision"]
            )
    else:
        op.create_unique_constraint(
            "uq_portfolio_program_mandate_version", "portfolio_programs", ["mandate_version_id"]
        )
        op.create_unique_constraint(
            "uq_portfolio_candidate_evaluation_lineage",
            "portfolio_candidates",
            [
                "id",
                "candidate_family_id",
                "portfolio_program_id",
                "mandate_version_id",
                "assembly_input_id",
            ],
        )
        op.create_unique_constraint(
            "uq_candidate_package_promotion_lineage",
            "candidate_packages",
            ["id", "candidate_id", "revision"],
        )
    op.create_index(
        "uq_portfolio_initial_input_assignment_active",
        "portfolio_input_evaluation_assignments",
        ["portfolio_program_id"],
        unique=True,
        sqlite_where=sa.text(
            "previous_candidate_id IS NULL AND state IN ('FROZEN', 'QUEUED', 'RUNNING')"
        ),
        postgresql_where=sa.text(
            "previous_candidate_id IS NULL AND state IN ('FROZEN', 'QUEUED', 'RUNNING')"
        ),
    )


def _extend_promotion_policy_versions() -> None:
    columns = (
        sa.Column("policy_contract_version", sa.String(length=80)),
        sa.Column("paper_connection_version_id", sa.Uuid()),
        sa.Column("paper_feedback_contract_version_id", sa.Uuid()),
        sa.Column("paper_preflight_receipt_id", sa.Uuid()),
        sa.Column("live_connection_version_id", sa.Uuid()),
        sa.Column("live_feedback_contract_version_id", sa.Uuid()),
        sa.Column("live_preflight_receipt_id", sa.Uuid()),
        sa.Column("paper_to_live_policy_version_id", sa.Uuid()),
    )
    bind = op.get_bind()
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("promotion_policy_versions", recreate="always") as batch:
            batch.drop_constraint("ck_promotion_policy_version_downstreams", type_="check")
            for column in columns:
                batch.add_column(column)
            batch.create_check_constraint(
                "ck_promotion_policy_version_contract",
                "policy_contract_version IS NULL OR policy_contract_version = 'PROMOTION_POLICY_V1'",
            )
            batch.create_check_constraint("ck_promotion_policy_version_tuples", _POLICY_TUPLES)
            batch.create_foreign_key(
                "fk_promotion_policy_paper_connection_tuple",
                "downstream_connection_versions",
                [
                    "paper_connection_version_id",
                    "paper_downstream_system_id",
                    "paper_feedback_contract_version_id",
                ],
                ["id", "downstream_system_id", "feedback_contract_version_id"],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_promotion_policy_live_connection_tuple",
                "downstream_connection_versions",
                [
                    "live_connection_version_id",
                    "live_downstream_system_id",
                    "live_feedback_contract_version_id",
                ],
                ["id", "downstream_system_id", "feedback_contract_version_id"],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_promotion_policy_paper_receipt",
                "preflight_receipts",
                ["paper_preflight_receipt_id"],
                ["id"],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_promotion_policy_live_receipt",
                "preflight_receipts",
                ["live_preflight_receipt_id"],
                ["id"],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_promotion_policy_p2l_policy",
                "promotion_policy_versions",
                ["paper_to_live_policy_version_id"],
                ["id"],
                ondelete="RESTRICT",
            )
        return
    for column in columns:
        op.add_column("promotion_policy_versions", column)
    op.drop_constraint(
        "ck_promotion_policy_version_downstreams", "promotion_policy_versions", type_="check"
    )
    op.create_check_constraint(
        "ck_promotion_policy_version_tuples", "promotion_policy_versions", _POLICY_TUPLES
    )
    op.create_check_constraint(
        "ck_promotion_policy_version_contract",
        "promotion_policy_versions",
        "policy_contract_version IS NULL OR policy_contract_version = 'PROMOTION_POLICY_V1'",
    )
    op.create_foreign_key(
        "fk_promotion_policy_paper_connection_tuple",
        "promotion_policy_versions",
        "downstream_connection_versions",
        [
            "paper_connection_version_id",
            "paper_downstream_system_id",
            "paper_feedback_contract_version_id",
        ],
        ["id", "downstream_system_id", "feedback_contract_version_id"],
        ondelete="RESTRICT",
    )
    op.create_foreign_key(
        "fk_promotion_policy_live_connection_tuple",
        "promotion_policy_versions",
        "downstream_connection_versions",
        [
            "live_connection_version_id",
            "live_downstream_system_id",
            "live_feedback_contract_version_id",
        ],
        ["id", "downstream_system_id", "feedback_contract_version_id"],
        ondelete="RESTRICT",
    )
    op.create_foreign_key(
        "fk_promotion_policy_paper_receipt",
        "promotion_policy_versions",
        "preflight_receipts",
        ["paper_preflight_receipt_id"],
        ["id"],
        ondelete="RESTRICT",
    )
    op.create_foreign_key(
        "fk_promotion_policy_live_receipt",
        "promotion_policy_versions",
        "preflight_receipts",
        ["live_preflight_receipt_id"],
        ["id"],
        ondelete="RESTRICT",
    )
    op.create_foreign_key(
        "fk_promotion_policy_p2l_policy",
        "promotion_policy_versions",
        "promotion_policy_versions",
        ["paper_to_live_policy_version_id"],
        ["id"],
        ondelete="RESTRICT",
    )


def _create_portfolio_evaluation_tables() -> None:
    op.create_table(
        "portfolio_evaluation_assignments",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("portfolio_program_id", sa.Uuid(), nullable=False),
        sa.Column("candidate_id", sa.Uuid(), nullable=False),
        sa.Column("candidate_family_id", sa.Uuid(), nullable=False),
        sa.Column("mandate_version_id", sa.Uuid(), nullable=False),
        sa.Column("assembly_input_id", sa.Uuid(), nullable=False),
        sa.Column("evaluation_dataset_selection_id", sa.Uuid(), nullable=False),
        sa.Column("sealed_dataset_revision_id", sa.Uuid(), nullable=False),
        sa.Column("promotion_policy_version_id", sa.Uuid(), nullable=False),
        sa.Column("cause_event_id", _EVENT_ID, nullable=False),
        sa.Column("previous_candidate_id", sa.Uuid()),
        sa.Column("evaluator_contract_version", sa.String(length=80), nullable=False),
        sa.Column("state", sa.String(length=20), nullable=False),
        sa.Column("private_result_ref", sa.Uuid()),
        sa.Column("evaluated_at", sa.DateTime(timezone=True)),
        sa.Column("outcome", sa.String(length=20)),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column("completed_at", sa.DateTime(timezone=True)),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("candidate_id", name="uq_portfolio_evaluation_assignment_candidate"),
        sa.UniqueConstraint("id", "candidate_id", name="uq_portfolio_evaluation_assignment_candidate_pair"),
        sa.CheckConstraint(
            "evaluator_contract_version = 'PORTFOLIO_EVALUATION_V1'",
            name="ck_portfolio_evaluation_assignment_contract",
        ),
        sa.CheckConstraint(
            "(state IN ('FROZEN', 'QUEUED', 'RUNNING') "
            "AND private_result_ref IS NULL AND evaluated_at IS NULL "
            "AND outcome IS NULL AND completed_at IS NULL) OR "
            "(state = 'FINALIZED' AND private_result_ref IS NOT NULL "
            "AND evaluated_at IS NOT NULL AND outcome IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID') "
            "AND completed_at IS NOT NULL)",
            name="ck_portfolio_evaluation_assignment_state",
        ),
        sa.ForeignKeyConstraint(
            [
                "candidate_id",
                "candidate_family_id",
                "portfolio_program_id",
                "mandate_version_id",
                "assembly_input_id",
            ],
            [
                "portfolio_candidates.id",
                "portfolio_candidates.candidate_family_id",
                "portfolio_candidates.portfolio_program_id",
                "portfolio_candidates.mandate_version_id",
                "portfolio_candidates.assembly_input_id",
            ],
            name="fk_portfolio_evaluation_assignment_candidate_lineage",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["evaluation_dataset_selection_id", "sealed_dataset_revision_id"],
            [
                "evaluation_dataset_selections.id",
                "evaluation_dataset_selections.sealed_dataset_revision_id",
            ],
            name="fk_portfolio_evaluation_assignment_selection_sealed",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["promotion_policy_version_id"],
            ["promotion_policy_versions.id"],
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(["cause_event_id"], ["events.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(
            ["previous_candidate_id", "portfolio_program_id"],
            ["portfolio_candidates.id", "portfolio_candidates.portfolio_program_id"],
            name="fk_portfolio_evaluation_assignment_predecessor",
            ondelete="RESTRICT",
        ),
    )
    op.create_table(
        "portfolio_evaluation_episodes",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("assignment_id", sa.Uuid(), nullable=False),
        sa.Column("candidate_id", sa.Uuid(), nullable=False),
        sa.Column("state", sa.String(length=20), nullable=False),
        sa.Column("result", sa.String(length=20)),
        sa.Column("evaluated_at", sa.DateTime(timezone=True)),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column("disclosed_at", sa.DateTime(timezone=True)),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("assignment_id", name="uq_portfolio_evaluation_episode_assignment"),
        sa.UniqueConstraint("id", "candidate_id", name="uq_portfolio_evaluation_episode_candidate_pair"),
        sa.CheckConstraint(
            "(state IN ('ASSIGNED', 'EVALUATING') AND result IS NULL "
            "AND evaluated_at IS NULL AND disclosed_at IS NULL) OR "
            "(state = 'DISCLOSED' AND result IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID') "
            "AND evaluated_at IS NOT NULL AND disclosed_at IS NOT NULL)",
            name="ck_portfolio_evaluation_episode_state",
        ),
        sa.ForeignKeyConstraint(
            ["assignment_id", "candidate_id"],
            ["portfolio_evaluation_assignments.id", "portfolio_evaluation_assignments.candidate_id"],
            name="fk_portfolio_evaluation_episode_assignment_candidate",
            ondelete="RESTRICT",
        ),
    )
    op.create_table(
        "portfolio_evaluation_metrics",
        sa.Column("episode_id", sa.Uuid(), nullable=False),
        sa.Column("metric_code", sa.String(length=100), nullable=False),
        sa.Column("status", sa.String(length=20), nullable=False),
        sa.Column("value", sa.Numeric(20, 8)),
        sa.PrimaryKeyConstraint("episode_id", "metric_code"),
        sa.CheckConstraint("status IN ('AVAILABLE', 'NOT_AVAILABLE')", name="ck_portfolio_evaluation_metric_status"),
        sa.CheckConstraint(
            "(status = 'AVAILABLE' AND value IS NOT NULL AND "
            f"{_FINITE.format(column='value')}) OR (status = 'NOT_AVAILABLE' AND value IS NULL)",
            name="ck_portfolio_evaluation_metric_value",
        ),
        sa.ForeignKeyConstraint(
            ["episode_id"], ["portfolio_evaluation_episodes.id"], ondelete="RESTRICT"
        ),
    )
    op.create_table(
        "portfolio_evaluation_gates",
        sa.Column("episode_id", sa.Uuid(), nullable=False),
        sa.Column("gate_code", sa.String(length=100), nullable=False),
        sa.Column("status", sa.String(length=20), nullable=False),
        sa.Column("reason_code", sa.String(length=100)),
        sa.PrimaryKeyConstraint("episode_id", "gate_code"),
        sa.CheckConstraint(
            "status IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_portfolio_evaluation_gate_status",
        ),
        sa.CheckConstraint(
            "(status = 'PASS' AND reason_code IS NULL) OR "
            "(status <> 'PASS' AND reason_code IS NOT NULL AND length(trim(reason_code)) > 0)",
            name="ck_portfolio_evaluation_gate_reason",
        ),
        sa.ForeignKeyConstraint(
            ["episode_id"], ["portfolio_evaluation_episodes.id"], ondelete="RESTRICT"
        ),
    )
    op.create_table(
        "portfolio_evaluation_disclosures",
        sa.Column("episode_id", sa.Uuid(), nullable=False),
        sa.Column("candidate_id", sa.Uuid(), nullable=False),
        sa.Column("classification", sa.String(length=20), nullable=False),
        sa.Column("reason_code", sa.String(length=100)),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.PrimaryKeyConstraint("episode_id"),
        sa.CheckConstraint(
            "classification IN ('QUALIFIED', 'REJECTED', 'INCONCLUSIVE', 'INVALID')",
            name="ck_portfolio_evaluation_disclosure_classification",
        ),
        sa.CheckConstraint(
            "(classification = 'QUALIFIED' AND reason_code IS NULL) OR "
            "(classification <> 'QUALIFIED' AND reason_code IS NOT NULL "
            "AND length(trim(reason_code)) > 0)",
            name="ck_portfolio_evaluation_disclosure_reason",
        ),
        sa.ForeignKeyConstraint(
            ["episode_id", "candidate_id"],
            ["portfolio_evaluation_episodes.id", "portfolio_evaluation_episodes.candidate_id"],
            name="fk_portfolio_evaluation_disclosure_episode_candidate",
            ondelete="RESTRICT",
        ),
    )


def _create_feedback_and_forward_evidence_tables() -> None:
    op.create_table(
        "feedback_packages",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("handoff_offer_id", sa.Uuid(), nullable=False),
        sa.Column("feedback_contract_version_id", sa.Uuid(), nullable=False),
        sa.Column("state", sa.String(length=20), nullable=False),
        sa.Column("observation_start", sa.DateTime(timezone=True), nullable=False),
        sa.Column("observation_end", sa.DateTime(timezone=True), nullable=False),
        sa.Column("sample_size", sa.Integer(), nullable=False),
        sa.Column("summary_json", _JSON, nullable=False, server_default=sa.text("'{}'")),
        sa.Column("relative_path", sa.Text()),
        sa.Column("received_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("handoff_offer_id", name="uq_feedback_package_handoff"),
        sa.UniqueConstraint("id", "handoff_offer_id", name="uq_feedback_package_handoff_pair"),
        sa.CheckConstraint(
            "state IN ('RECEIVED', 'COMPLETE', 'INVALID')", name="ck_feedback_package_state"
        ),
        sa.CheckConstraint(
            "observation_end >= observation_start AND sample_size > 0",
            name="ck_feedback_package_observation",
        ),
        sa.ForeignKeyConstraint(
            ["handoff_offer_id", "feedback_contract_version_id"],
            ["handoff_offers.id", "handoff_offers.feedback_contract_version_id"],
            name="fk_feedback_package_handoff_contract",
            ondelete="RESTRICT",
        ),
    )
    bind = op.get_bind()
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("forward_evidence_episodes", recreate="always") as batch:
            batch.add_column(sa.Column("feedback_package_id", sa.Uuid()))
            batch.create_foreign_key(
                "fk_forward_evidence_feedback_handoff",
                "feedback_packages",
                ["feedback_package_id", "handoff_id"],
                ["id", "handoff_offer_id"],
                ondelete="RESTRICT",
            )
    else:
        op.add_column("forward_evidence_episodes", sa.Column("feedback_package_id", sa.Uuid()))
        op.create_foreign_key(
            "fk_forward_evidence_feedback_handoff",
            "forward_evidence_episodes",
            "feedback_packages",
            ["feedback_package_id", "handoff_id"],
            ["id", "handoff_offer_id"],
            ondelete="RESTRICT",
        )
    op.create_index(
        "uq_forward_evidence_feedback_package",
        "forward_evidence_episodes",
        ["feedback_package_id"],
        unique=True,
        sqlite_where=sa.text("feedback_package_id IS NOT NULL"),
        postgresql_where=sa.text("feedback_package_id IS NOT NULL"),
    )
    op.create_table(
        "forward_evidence_metrics",
        sa.Column("episode_id", sa.Uuid(), nullable=False),
        sa.Column("metric_code", sa.String(length=100), nullable=False),
        sa.Column("value", sa.Numeric(20, 8)),
        sa.Column("status", sa.String(length=20), nullable=False),
        sa.PrimaryKeyConstraint("episode_id", "metric_code"),
        sa.CheckConstraint("length(trim(metric_code)) > 0", name="ck_forward_evidence_metric_code"),
        sa.CheckConstraint(
            "status IN ('AVAILABLE', 'NOT_AVAILABLE')", name="ck_forward_evidence_metric_status"
        ),
        sa.CheckConstraint(
            "(status = 'AVAILABLE' AND value IS NOT NULL AND "
            f"{_FINITE.format(column='value')}) OR (status = 'NOT_AVAILABLE' AND value IS NULL)",
            name="ck_forward_evidence_metric_value",
        ),
        sa.ForeignKeyConstraint(
            ["episode_id"], ["forward_evidence_episodes.id"], ondelete="RESTRICT"
        ),
    )


def _create_promotion_tables() -> None:
    op.create_table(
        "promotion_evaluations",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("purpose", sa.String(length=40), nullable=False),
        sa.Column("portfolio_evaluation_episode_id", sa.Uuid()),
        sa.Column("forward_evidence_episode_id", sa.Uuid()),
        sa.Column("candidate_id", sa.Uuid(), nullable=False),
        sa.Column("candidate_package_id", sa.Uuid(), nullable=False),
        sa.Column("package_revision", sa.Integer(), nullable=False),
        sa.Column("policy_version_id", sa.Uuid(), nullable=False),
        sa.Column("paper_to_live_policy_version_id", sa.Uuid()),
        sa.Column("downstream_system_id", sa.Uuid(), nullable=False),
        sa.Column("downstream_connection_version_id", sa.Uuid(), nullable=False),
        sa.Column("feedback_contract_version_id", sa.Uuid(), nullable=False),
        sa.Column("preflight_receipt_id", sa.Uuid(), nullable=False),
        sa.Column("outcome", sa.String(length=20), nullable=False),
        sa.Column("action", sa.String(length=20), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "id",
            "purpose",
            "candidate_id",
            "candidate_package_id",
            "package_revision",
            "paper_to_live_policy_version_id",
            "downstream_system_id",
            "downstream_connection_version_id",
            "feedback_contract_version_id",
            "preflight_receipt_id",
            name="uq_promotion_evaluation_approval_lineage",
        ),
        sa.CheckConstraint(
            "purpose IN ('PORTFOLIO_TO_PAPER', 'PAPER_TO_LIVE')",
            name="ck_promotion_evaluation_purpose",
        ),
        sa.CheckConstraint(
            "outcome IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_promotion_evaluation_outcome",
        ),
        sa.CheckConstraint(
            "action IN ('MANUAL_APPROVAL', 'AUTO_HANDOFF', 'NO_ACTION')",
            name="ck_promotion_evaluation_action",
        ),
        sa.CheckConstraint(
            "(purpose = 'PORTFOLIO_TO_PAPER' AND portfolio_evaluation_episode_id IS NOT NULL "
            "AND forward_evidence_episode_id IS NULL AND paper_to_live_policy_version_id IS NOT NULL) OR "
            "(purpose = 'PAPER_TO_LIVE' AND portfolio_evaluation_episode_id IS NULL "
            "AND forward_evidence_episode_id IS NOT NULL AND paper_to_live_policy_version_id IS NULL)",
            name="ck_promotion_evaluation_source_xor",
        ),
        sa.CheckConstraint(
            "purpose <> 'PORTFOLIO_TO_PAPER' OR action <> 'AUTO_HANDOFF'",
            name="ck_promotion_evaluation_p2p_action",
        ),
        sa.ForeignKeyConstraint(
            ["portfolio_evaluation_episode_id", "candidate_id"],
            ["portfolio_evaluation_episodes.id", "portfolio_evaluation_episodes.candidate_id"],
            name="fk_promotion_evaluation_portfolio_episode_candidate",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["forward_evidence_episode_id"],
            ["forward_evidence_episodes.id"],
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["candidate_package_id", "candidate_id", "package_revision"],
            ["candidate_packages.id", "candidate_packages.candidate_id", "candidate_packages.revision"],
            name="fk_promotion_evaluation_candidate_package",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["policy_version_id"], ["promotion_policy_versions.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["paper_to_live_policy_version_id"],
            ["promotion_policy_versions.id"],
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            [
                "downstream_connection_version_id",
                "downstream_system_id",
                "feedback_contract_version_id",
            ],
            [
                "downstream_connection_versions.id",
                "downstream_connection_versions.downstream_system_id",
                "downstream_connection_versions.feedback_contract_version_id",
            ],
            name="fk_promotion_evaluation_connection_tuple",
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["preflight_receipt_id"], ["preflight_receipts.id"], ondelete="RESTRICT"
        ),
    )
    op.create_index(
        "uq_promotion_evaluation_p2p_episode",
        "promotion_evaluations",
        ["portfolio_evaluation_episode_id"],
        unique=True,
        sqlite_where=sa.text("purpose = 'PORTFOLIO_TO_PAPER'"),
        postgresql_where=sa.text("purpose = 'PORTFOLIO_TO_PAPER'"),
    )
    op.create_index(
        "uq_promotion_evaluation_p2l_episode",
        "promotion_evaluations",
        ["forward_evidence_episode_id"],
        unique=True,
        sqlite_where=sa.text("purpose = 'PAPER_TO_LIVE'"),
        postgresql_where=sa.text("purpose = 'PAPER_TO_LIVE'"),
    )
    op.create_table(
        "promotion_gate_results",
        sa.Column("evaluation_id", sa.Uuid(), nullable=False),
        sa.Column("gate_code", sa.String(length=100), nullable=False),
        sa.Column("status", sa.String(length=20), nullable=False),
        sa.Column("actual", sa.Numeric(20, 8)),
        sa.Column("expected", sa.Numeric(20, 8)),
        sa.Column("reason_code", sa.String(length=100)),
        sa.PrimaryKeyConstraint("evaluation_id", "gate_code"),
        sa.CheckConstraint(
            "status IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_promotion_gate_result_status",
        ),
        sa.CheckConstraint(
            "(status = 'PASS' AND reason_code IS NULL) OR "
            "(status <> 'PASS' AND reason_code IS NOT NULL AND length(trim(reason_code)) > 0)",
            name="ck_promotion_gate_result_reason",
        ),
        sa.CheckConstraint(
            f"actual IS NULL OR {_FINITE.format(column='actual')}",
            name="ck_promotion_gate_result_actual",
        ),
        sa.CheckConstraint(
            f"expected IS NULL OR {_FINITE.format(column='expected')}",
            name="ck_promotion_gate_result_expected",
        ),
        sa.ForeignKeyConstraint(
            ["evaluation_id"], ["promotion_evaluations.id"], ondelete="RESTRICT"
        ),
    )


def _extend_approval_and_handoff_lineage() -> None:
    approval_typed_lineage = (
        "(promotion_evaluation_id IS NULL AND promotion_purpose IS NULL "
        "AND downstream_connection_version_id IS NULL AND feedback_contract_version_id IS NULL "
        "AND preflight_receipt_id IS NULL AND paper_to_live_policy_version_id IS NULL) OR "
        "(promotion_evaluation_id IS NOT NULL AND promotion_purpose = 'PORTFOLIO_TO_PAPER' "
        "AND purpose = 'PAPER' AND candidate_package_id IS NOT NULL "
        "AND candidate_package_revision IS NOT NULL AND downstream_system_id IS NOT NULL "
        "AND downstream_connection_version_id IS NOT NULL AND feedback_contract_version_id IS NOT NULL "
        "AND preflight_receipt_id IS NOT NULL AND paper_to_live_policy_version_id IS NOT NULL) OR "
        "(promotion_evaluation_id IS NOT NULL AND promotion_purpose = 'PAPER_TO_LIVE' "
        "AND purpose = 'LIVE' AND candidate_package_id IS NOT NULL "
        "AND candidate_package_revision IS NOT NULL AND downstream_system_id IS NOT NULL "
        "AND downstream_connection_version_id IS NOT NULL AND feedback_contract_version_id IS NOT NULL "
        "AND preflight_receipt_id IS NOT NULL AND paper_to_live_policy_version_id IS NULL)"
    )
    handoff_typed_lineage = (
        "(promotion_purpose IS NULL AND candidate_package_revision IS NULL "
        "AND downstream_connection_version_id IS NULL AND feedback_contract_version_id IS NULL "
        "AND preflight_receipt_id IS NULL AND paper_to_live_policy_version_id IS NULL) OR "
        "(promotion_purpose = 'PORTFOLIO_TO_PAPER' AND purpose = 'PAPER' "
        "AND candidate_package_revision IS NOT NULL AND downstream_connection_version_id IS NOT NULL "
        "AND feedback_contract_version_id IS NOT NULL AND preflight_receipt_id IS NOT NULL "
        "AND paper_to_live_policy_version_id IS NOT NULL) OR "
        "(promotion_purpose = 'PAPER_TO_LIVE' AND purpose = 'LIVE' "
        "AND candidate_package_revision IS NOT NULL AND downstream_connection_version_id IS NOT NULL "
        "AND feedback_contract_version_id IS NOT NULL AND preflight_receipt_id IS NOT NULL "
        "AND paper_to_live_policy_version_id IS NULL)"
    )
    approval_columns = (
        sa.Column("promotion_evaluation_id", sa.Uuid()),
        sa.Column("promotion_purpose", sa.String(length=40)),
        sa.Column("downstream_connection_version_id", sa.Uuid()),
        sa.Column("feedback_contract_version_id", sa.Uuid()),
        sa.Column("preflight_receipt_id", sa.Uuid()),
        sa.Column("paper_to_live_policy_version_id", sa.Uuid()),
    )
    handoff_columns = (
        sa.Column("candidate_package_revision", sa.Integer()),
        sa.Column("promotion_purpose", sa.String(length=40)),
        sa.Column("downstream_connection_version_id", sa.Uuid()),
        sa.Column("feedback_contract_version_id", sa.Uuid()),
        sa.Column("preflight_receipt_id", sa.Uuid()),
        sa.Column("paper_to_live_policy_version_id", sa.Uuid()),
    )
    approval_lineage_columns = [
        "id",
        "promotion_purpose",
        "candidate_id",
        "candidate_package_id",
        "candidate_package_revision",
        "downstream_system_id",
        "downstream_connection_version_id",
        "feedback_contract_version_id",
        "preflight_receipt_id",
        "paper_to_live_policy_version_id",
    ]
    promotion_lineage_columns = [
        "promotion_evaluation_id",
        "promotion_purpose",
        "candidate_id",
        "candidate_package_id",
        "candidate_package_revision",
        "downstream_system_id",
        "downstream_connection_version_id",
        "feedback_contract_version_id",
        "preflight_receipt_id",
        "paper_to_live_policy_version_id",
    ]
    promotion_target_columns = [
        "id",
        "purpose",
        "candidate_id",
        "candidate_package_id",
        "package_revision",
        "downstream_system_id",
        "downstream_connection_version_id",
        "feedback_contract_version_id",
        "preflight_receipt_id",
        "paper_to_live_policy_version_id",
    ]
    handoff_lineage_columns = [
        "approval_id",
        "promotion_purpose",
        "candidate_id",
        "candidate_package_id",
        "candidate_package_revision",
        "downstream_system_id",
        "downstream_connection_version_id",
        "feedback_contract_version_id",
        "preflight_receipt_id",
        "paper_to_live_policy_version_id",
    ]
    bind = op.get_bind()
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("approval_snapshots", recreate="always") as batch:
            for column in approval_columns:
                batch.add_column(column)
            batch.create_check_constraint(
                "ck_approval_snapshot_typed_lineage", approval_typed_lineage
            )
            batch.create_unique_constraint(
                "uq_approval_snapshot_handoff_lineage", approval_lineage_columns
            )
            batch.create_foreign_key(
                "fk_approval_snapshot_promotion_evaluation",
                "promotion_evaluations",
                ["promotion_evaluation_id"],
                ["id"],
                ondelete="RESTRICT",
            )
            batch.create_foreign_key(
                "fk_approval_snapshot_promotion_lineage",
                "promotion_evaluations",
                promotion_lineage_columns,
                promotion_target_columns,
                ondelete="RESTRICT",
            )
            for name, target, column in (
                ("fk_approval_snapshot_connection", "downstream_connection_versions", "downstream_connection_version_id"),
                ("fk_approval_snapshot_contract", "feedback_contract_versions", "feedback_contract_version_id"),
                ("fk_approval_snapshot_receipt", "preflight_receipts", "preflight_receipt_id"),
                ("fk_approval_snapshot_p2l_policy", "promotion_policy_versions", "paper_to_live_policy_version_id"),
            ):
                batch.create_foreign_key(name, target, [column], ["id"], ondelete="RESTRICT")
        with op.batch_alter_table("handoff_offers", recreate="always") as batch:
            for column in handoff_columns:
                batch.add_column(column)
            batch.create_check_constraint(
                "ck_handoff_offer_typed_lineage", handoff_typed_lineage
            )
            batch.create_unique_constraint(
                "uq_handoff_offer_feedback_contract_pair",
                ["id", "feedback_contract_version_id"],
            )
            batch.create_foreign_key(
                "fk_handoff_offer_approval_lineage",
                "approval_snapshots",
                handoff_lineage_columns,
                approval_lineage_columns,
                ondelete="RESTRICT",
            )
            for name, target, column in (
                ("fk_handoff_offer_connection", "downstream_connection_versions", "downstream_connection_version_id"),
                ("fk_handoff_offer_contract", "feedback_contract_versions", "feedback_contract_version_id"),
                ("fk_handoff_offer_receipt", "preflight_receipts", "preflight_receipt_id"),
                ("fk_handoff_offer_p2l_policy", "promotion_policy_versions", "paper_to_live_policy_version_id"),
            ):
                batch.create_foreign_key(name, target, [column], ["id"], ondelete="RESTRICT")
    else:
        for column in approval_columns:
            op.add_column("approval_snapshots", column)
        op.create_check_constraint(
            "ck_approval_snapshot_typed_lineage", "approval_snapshots", approval_typed_lineage
        )
        op.create_unique_constraint(
            "uq_approval_snapshot_handoff_lineage", "approval_snapshots", approval_lineage_columns
        )
        op.create_foreign_key(
            "fk_approval_snapshot_promotion_evaluation",
            "approval_snapshots",
            "promotion_evaluations",
            ["promotion_evaluation_id"],
            ["id"],
            ondelete="RESTRICT",
        )
        op.create_foreign_key(
            "fk_approval_snapshot_promotion_lineage",
            "approval_snapshots",
            "promotion_evaluations",
            promotion_lineage_columns,
            promotion_target_columns,
            ondelete="RESTRICT",
        )
        for name, target, column in (
            ("fk_approval_snapshot_connection", "downstream_connection_versions", "downstream_connection_version_id"),
            ("fk_approval_snapshot_contract", "feedback_contract_versions", "feedback_contract_version_id"),
            ("fk_approval_snapshot_receipt", "preflight_receipts", "preflight_receipt_id"),
            ("fk_approval_snapshot_p2l_policy", "promotion_policy_versions", "paper_to_live_policy_version_id"),
        ):
            op.create_foreign_key(name, "approval_snapshots", target, [column], ["id"], ondelete="RESTRICT")
        for column in handoff_columns:
            op.add_column("handoff_offers", column)
        op.create_check_constraint(
            "ck_handoff_offer_typed_lineage", "handoff_offers", handoff_typed_lineage
        )
        op.create_unique_constraint(
            "uq_handoff_offer_feedback_contract_pair",
            "handoff_offers",
            ["id", "feedback_contract_version_id"],
        )
        op.create_foreign_key(
            "fk_handoff_offer_approval_lineage",
            "handoff_offers",
            "approval_snapshots",
            handoff_lineage_columns,
            approval_lineage_columns,
            ondelete="RESTRICT",
        )
        for name, target, column in (
            ("fk_handoff_offer_connection", "downstream_connection_versions", "downstream_connection_version_id"),
            ("fk_handoff_offer_contract", "feedback_contract_versions", "feedback_contract_version_id"),
            ("fk_handoff_offer_receipt", "preflight_receipts", "preflight_receipt_id"),
            ("fk_handoff_offer_p2l_policy", "promotion_policy_versions", "paper_to_live_policy_version_id"),
        ):
            op.create_foreign_key(name, "handoff_offers", target, [column], ["id"], ondelete="RESTRICT")
    op.create_index(
        "uq_approval_snapshot_promotion_evaluation",
        "approval_snapshots",
        ["promotion_evaluation_id"],
        unique=True,
        sqlite_where=sa.text("promotion_evaluation_id IS NOT NULL"),
        postgresql_where=sa.text("promotion_evaluation_id IS NOT NULL"),
    )
    op.create_index(
        "uq_handoff_offer_typed_approval",
        "handoff_offers",
        ["approval_id"],
        unique=True,
        sqlite_where=sa.text("downstream_connection_version_id IS NOT NULL"),
        postgresql_where=sa.text("downstream_connection_version_id IS NOT NULL"),
    )


def _create_job_indexes() -> None:
    for index_name, predicate in _JOB_INDEXES:
        op.create_index(
            index_name,
            "jobs",
            ["resource_id"],
            unique=True,
            sqlite_where=sa.text(predicate),
            postgresql_where=sa.text(predicate),
        )


def upgrade() -> None:
    _require_compatible_history()
    _create_feedback_and_connection_tables()
    _extend_program_candidate_and_input_identity()
    _extend_promotion_policy_versions()
    _create_portfolio_evaluation_tables()
    _create_promotion_tables()
    _extend_approval_and_handoff_lineage()
    _create_feedback_and_forward_evidence_tables()
    _create_job_indexes()


def _require_no_trusted_production_facts() -> None:
    bind = op.get_bind()
    tables = (
        "feedback_contract_versions",
        "feedback_contract_metric_requirements",
        "feedback_contract_accepted_package_contracts",
        "feedback_contract_accepted_arrow_contracts",
        "downstream_connection_versions",
        "portfolio_evaluation_assignments",
        "portfolio_evaluation_episodes",
        "portfolio_evaluation_metrics",
        "portfolio_evaluation_gates",
        "portfolio_evaluation_disclosures",
        "promotion_evaluations",
        "promotion_gate_results",
        "feedback_packages",
        "forward_evidence_metrics",
    )
    if any(bind.execute(sa.text(f"SELECT 1 FROM {table} LIMIT 1")).first() for table in tables):
        raise RuntimeError("TRUSTED_PRODUCTION_CHAIN_DOWNGRADE_BLOCKED")
    checks = (
        "SELECT 1 FROM promotion_policy_versions WHERE policy_contract_version IS NOT NULL LIMIT 1",
        "SELECT 1 FROM approval_snapshots WHERE promotion_evaluation_id IS NOT NULL OR promotion_purpose IS NOT NULL LIMIT 1",
        "SELECT 1 FROM handoff_offers WHERE promotion_purpose IS NOT NULL LIMIT 1",
        "SELECT 1 FROM forward_evidence_episodes WHERE feedback_package_id IS NOT NULL LIMIT 1",
        "SELECT 1 FROM jobs WHERE kind IN ('PORTFOLIO_INPUT_EVALUATION', 'PORTFOLIO_ASSEMBLY', "
        "'PORTFOLIO_EVALUATION', 'PORTFOLIO_TO_PAPER_PROMOTION', 'PAPER_TO_LIVE_PROMOTION') LIMIT 1",
    )
    if any(bind.execute(sa.text(query)).first() for query in checks):
        raise RuntimeError("TRUSTED_PRODUCTION_CHAIN_DOWNGRADE_BLOCKED")


def _shrink_feedback_and_forward_evidence() -> None:
    bind = op.get_bind()
    op.drop_table("forward_evidence_metrics")
    op.drop_index("uq_forward_evidence_feedback_package", table_name="forward_evidence_episodes")
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("forward_evidence_episodes", recreate="always") as batch:
            batch.drop_constraint("fk_forward_evidence_feedback_handoff", type_="foreignkey")
            batch.drop_column("feedback_package_id")
    else:
        op.drop_constraint(
            "fk_forward_evidence_feedback_handoff",
            "forward_evidence_episodes",
            type_="foreignkey",
        )
        op.drop_column("forward_evidence_episodes", "feedback_package_id")
    op.drop_table("feedback_packages")


def _shrink_approval_and_handoff_lineage() -> None:
    bind = op.get_bind()
    op.drop_index("uq_handoff_offer_typed_approval", table_name="handoff_offers")
    op.drop_index("uq_approval_snapshot_promotion_evaluation", table_name="approval_snapshots")
    handoff_fks = (
        "fk_handoff_offer_approval_lineage",
        "fk_handoff_offer_connection",
        "fk_handoff_offer_contract",
        "fk_handoff_offer_receipt",
        "fk_handoff_offer_p2l_policy",
    )
    approval_fks = (
        "fk_approval_snapshot_promotion_evaluation",
        "fk_approval_snapshot_promotion_lineage",
        "fk_approval_snapshot_connection",
        "fk_approval_snapshot_contract",
        "fk_approval_snapshot_receipt",
        "fk_approval_snapshot_p2l_policy",
    )
    handoff_columns = (
        "candidate_package_revision",
        "promotion_purpose",
        "downstream_connection_version_id",
        "feedback_contract_version_id",
        "preflight_receipt_id",
        "paper_to_live_policy_version_id",
    )
    approval_columns = (
        "promotion_evaluation_id",
        "promotion_purpose",
        "downstream_connection_version_id",
        "feedback_contract_version_id",
        "preflight_receipt_id",
        "paper_to_live_policy_version_id",
    )
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("handoff_offers", recreate="always") as batch:
            for name in handoff_fks:
                batch.drop_constraint(name, type_="foreignkey")
            batch.drop_constraint("uq_handoff_offer_feedback_contract_pair", type_="unique")
            batch.drop_constraint("ck_handoff_offer_typed_lineage", type_="check")
            for column in handoff_columns:
                batch.drop_column(column)
        with op.batch_alter_table("approval_snapshots", recreate="always") as batch:
            for name in approval_fks:
                batch.drop_constraint(name, type_="foreignkey")
            batch.drop_constraint("uq_approval_snapshot_handoff_lineage", type_="unique")
            batch.drop_constraint("ck_approval_snapshot_typed_lineage", type_="check")
            for column in approval_columns:
                batch.drop_column(column)
        return
    for name in handoff_fks:
        op.drop_constraint(name, "handoff_offers", type_="foreignkey")
    op.drop_constraint("uq_handoff_offer_feedback_contract_pair", "handoff_offers", type_="unique")
    op.drop_constraint("ck_handoff_offer_typed_lineage", "handoff_offers", type_="check")
    for column in handoff_columns:
        op.drop_column("handoff_offers", column)
    for name in approval_fks:
        op.drop_constraint(name, "approval_snapshots", type_="foreignkey")
    op.drop_constraint("uq_approval_snapshot_handoff_lineage", "approval_snapshots", type_="unique")
    op.drop_constraint("ck_approval_snapshot_typed_lineage", "approval_snapshots", type_="check")
    for column in approval_columns:
        op.drop_column("approval_snapshots", column)


def _drop_promotion_and_portfolio_evaluation_tables() -> None:
    op.drop_table("promotion_gate_results")
    op.drop_index("uq_promotion_evaluation_p2l_episode", table_name="promotion_evaluations")
    op.drop_index("uq_promotion_evaluation_p2p_episode", table_name="promotion_evaluations")
    op.drop_table("promotion_evaluations")
    op.drop_table("portfolio_evaluation_disclosures")
    op.drop_table("portfolio_evaluation_gates")
    op.drop_table("portfolio_evaluation_metrics")
    op.drop_table("portfolio_evaluation_episodes")
    op.drop_table("portfolio_evaluation_assignments")


def _shrink_promotion_policy_versions() -> None:
    bind = op.get_bind()
    constraints = (
        "fk_promotion_policy_paper_connection_tuple",
        "fk_promotion_policy_live_connection_tuple",
        "fk_promotion_policy_paper_receipt",
        "fk_promotion_policy_live_receipt",
        "fk_promotion_policy_p2l_policy",
    )
    columns = (
        "policy_contract_version",
        "paper_connection_version_id",
        "paper_feedback_contract_version_id",
        "paper_preflight_receipt_id",
        "live_connection_version_id",
        "live_feedback_contract_version_id",
        "live_preflight_receipt_id",
        "paper_to_live_policy_version_id",
    )
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("promotion_policy_versions", recreate="always") as batch:
            for name in constraints:
                batch.drop_constraint(name, type_="foreignkey")
            batch.drop_constraint("ck_promotion_policy_version_tuples", type_="check")
            batch.drop_constraint("ck_promotion_policy_version_contract", type_="check")
            for column in columns:
                batch.drop_column(column)
            batch.create_check_constraint(
                "ck_promotion_policy_version_downstreams", _OLD_POLICY_DOWNSTREAMS
            )
        return
    for name in constraints:
        op.drop_constraint(name, "promotion_policy_versions", type_="foreignkey")
    op.drop_constraint("ck_promotion_policy_version_tuples", "promotion_policy_versions", type_="check")
    op.drop_constraint("ck_promotion_policy_version_contract", "promotion_policy_versions", type_="check")
    for column in columns:
        op.drop_column("promotion_policy_versions", column)
    op.create_check_constraint(
        "ck_promotion_policy_version_downstreams",
        "promotion_policy_versions",
        _OLD_POLICY_DOWNSTREAMS,
    )


def _shrink_identity_constraints() -> None:
    bind = op.get_bind()
    op.drop_index(
        "uq_portfolio_initial_input_assignment_active",
        table_name="portfolio_input_evaluation_assignments",
    )
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("candidate_packages", recreate="always") as batch:
            batch.drop_constraint("uq_candidate_package_promotion_lineage", type_="unique")
        with op.batch_alter_table("portfolio_candidates", recreate="always") as batch:
            batch.drop_constraint("uq_portfolio_candidate_evaluation_lineage", type_="unique")
        with op.batch_alter_table("portfolio_programs", recreate="always") as batch:
            batch.drop_constraint("uq_portfolio_program_mandate_version", type_="unique")
        return
    op.drop_constraint(
        "uq_candidate_package_promotion_lineage", "candidate_packages", type_="unique"
    )
    op.drop_constraint(
        "uq_portfolio_candidate_evaluation_lineage", "portfolio_candidates", type_="unique"
    )
    op.drop_constraint(
        "uq_portfolio_program_mandate_version", "portfolio_programs", type_="unique"
    )


def _drop_feedback_and_connection_tables() -> None:
    op.drop_table("downstream_connection_versions")
    op.drop_table("feedback_contract_accepted_arrow_contracts")
    op.drop_table("feedback_contract_accepted_package_contracts")
    op.drop_table("feedback_contract_metric_requirements")
    op.drop_table("feedback_contract_versions")


def downgrade() -> None:
    _require_no_trusted_production_facts()
    for index_name, _ in _JOB_INDEXES:
        op.drop_index(index_name, table_name="jobs")
    _shrink_feedback_and_forward_evidence()
    _shrink_approval_and_handoff_lineage()
    _drop_promotion_and_portfolio_evaluation_tables()
    _shrink_promotion_policy_versions()
    _shrink_identity_constraints()
    _drop_feedback_and_connection_tables()
