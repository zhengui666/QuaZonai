"""Add durable Phase-A research and agent facts.

Revision ID: 0016_research_agent_domain
Revises: 0015_codex_poll_lock_rows
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "0016_research_agent_domain"
down_revision = "0015_codex_poll_lock_rows"
branch_labels = None
depends_on = None


def _json_type(bind: sa.Connection) -> sa.types.TypeEngine[object]:
    if bind.dialect.name == "postgresql":
        from sqlalchemy.dialects.postgresql import JSONB

        return JSONB()
    return sa.JSON()


def _table_names(bind: sa.Connection) -> set[str]:
    return set(sa.inspect(bind).get_table_names())


def _column_names(bind: sa.Connection, table_name: str) -> set[str]:
    return {column["name"] for column in sa.inspect(bind).get_columns(table_name)}


def _check_names(bind: sa.Connection, table_name: str) -> set[str]:
    return {
        str(constraint["name"])
        for constraint in sa.inspect(bind).get_check_constraints(table_name)
        if constraint.get("name")
    }


def _foreign_key_names(bind: sa.Connection, table_name: str) -> set[str]:
    return {
        str(constraint["name"])
        for constraint in sa.inspect(bind).get_foreign_keys(table_name)
        if constraint.get("name")
    }


def _index_names(bind: sa.Connection, table_name: str) -> set[str]:
    return {
        str(index["name"])
        for index in sa.inspect(bind).get_indexes(table_name)
        if index.get("name")
    }


def _unique_constraint_names(bind: sa.Connection, table_name: str) -> set[str]:
    return {
        str(constraint["name"])
        for constraint in sa.inspect(bind).get_unique_constraints(table_name)
        if constraint.get("name")
    }


def _add_column_if_missing(table_name: str, column: sa.Column[object]) -> None:
    if column.name not in _column_names(op.get_bind(), table_name):
        op.add_column(table_name, column)


def _ensure_research_charter_constraints(bind: sa.Connection) -> None:
    foreign_keys = _foreign_key_names(bind, "research_charters")
    unique_constraints = _unique_constraint_names(bind, "research_charters")
    needs_draft_fk = "fk_research_charter_idea_draft" not in foreign_keys
    needs_draft_unique = "uq_research_charter_idea_draft" not in unique_constraints
    if not needs_draft_fk and not needs_draft_unique:
        return
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("research_charters", recreate="always") as batch:
            if needs_draft_fk:
                batch.create_foreign_key(
                    "fk_research_charter_idea_draft",
                    "idea_drafts",
                    ["idea_draft_id"],
                    ["id"],
                    ondelete="RESTRICT",
                )
            if needs_draft_unique:
                batch.create_unique_constraint("uq_research_charter_idea_draft", ["idea_draft_id"])
        return
    if needs_draft_fk:
        op.create_foreign_key(
            "fk_research_charter_idea_draft",
            "research_charters",
            "idea_drafts",
            ["idea_draft_id"],
            ["id"],
            ondelete="RESTRICT",
        )
    if needs_draft_unique:
        op.create_unique_constraint(
            "uq_research_charter_idea_draft", "research_charters", ["idea_draft_id"]
        )


def _ensure_research_program_constraints(bind: sa.Connection) -> None:
    checks = _check_names(bind, "research_programs")
    foreign_keys = _foreign_key_names(bind, "research_programs")
    wanted_checks = (
        (
            "ck_research_program_state",
            "state IN ('ACTIVE', 'COOLING', 'APPROVAL_PENDING', 'WAITING_FOR_FEEDBACK', "
            "'BLOCKED', 'PAUSED', 'ARCHIVED')",
        ),
        ("ck_research_program_revision", "revision > 0"),
    )
    missing_checks = [(name, expression) for name, expression in wanted_checks if name not in checks]
    needs_current_cycle_fk = "fk_research_program_current_cycle" not in foreign_keys
    if not missing_checks and not needs_current_cycle_fk:
        return
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("research_programs", recreate="always") as batch:
            for name, expression in missing_checks:
                batch.create_check_constraint(name, expression)
            if needs_current_cycle_fk:
                batch.create_foreign_key(
                    "fk_research_program_current_cycle",
                    "research_cycles",
                    ["current_cycle_id"],
                    ["id"],
                    ondelete="SET NULL",
                )
        return
    for name, expression in missing_checks:
        op.create_check_constraint(name, "research_programs", expression)
    if needs_current_cycle_fk:
        op.create_foreign_key(
            "fk_research_program_current_cycle",
            "research_programs",
            "research_cycles",
            ["current_cycle_id"],
            ["id"],
            ondelete="SET NULL",
        )


def _ensure_research_branch_constraints(bind: sa.Connection) -> None:
    checks = _check_names(bind, "research_branches")
    foreign_keys = _foreign_key_names(bind, "research_branches")
    needs_check = "ck_research_branch_revision" not in checks
    needs_cycle_fk = "fk_research_branch_cycle" not in foreign_keys
    if not needs_check and not needs_cycle_fk:
        return
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("research_branches", recreate="always") as batch:
            if needs_check:
                batch.create_check_constraint("ck_research_branch_revision", "revision_no > 0")
            if needs_cycle_fk:
                batch.create_foreign_key(
                    "fk_research_branch_cycle",
                    "research_cycles",
                    ["cycle_id"],
                    ["id"],
                    ondelete="RESTRICT",
                )
        return
    if needs_check:
        op.create_check_constraint(
            "ck_research_branch_revision", "research_branches", "revision_no > 0"
        )
    if needs_cycle_fk:
        op.create_foreign_key(
            "fk_research_branch_cycle",
            "research_branches",
            "research_cycles",
            ["cycle_id"],
            ["id"],
            ondelete="RESTRICT",
        )


def _ensure_research_mission_constraints(bind: sa.Connection) -> None:
    checks = _check_names(bind, "research_missions")
    foreign_keys = _foreign_key_names(bind, "research_missions")
    wanted_checks = (
        (
            "ck_research_mission_state",
            "state IN ('PLANNED', 'READY', 'RUNNING', 'SUCCEEDED', 'FAILED', "
            "'INTERRUPTED', 'CANCELLED')",
        ),
        ("ck_research_mission_attempt", "attempt > 0"),
        ("ck_research_mission_revision", "revision > 0"),
        ("ck_research_mission_max_turns", "max_turns > 0"),
        ("ck_research_mission_max_tool_calls", "max_tool_calls >= 0"),
        (
            "ck_research_mission_time_order",
            "finished_at IS NULL OR started_at IS NULL OR started_at <= finished_at",
        ),
        (
            "ck_research_mission_terminal_finished",
            "state NOT IN ('SUCCEEDED', 'FAILED', 'INTERRUPTED', 'CANCELLED') "
            "OR finished_at IS NOT NULL",
        ),
    )
    missing_checks = [(name, expression) for name, expression in wanted_checks if name not in checks]
    needs_cycle_fk = "fk_research_mission_cycle" not in foreign_keys
    if not missing_checks and not needs_cycle_fk:
        return
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("research_missions", recreate="always") as batch:
            for name, expression in missing_checks:
                batch.create_check_constraint(name, expression)
            if needs_cycle_fk:
                batch.create_foreign_key(
                    "fk_research_mission_cycle",
                    "research_cycles",
                    ["cycle_id"],
                    ["id"],
                    ondelete="RESTRICT",
                )
        return
    for name, expression in missing_checks:
        op.create_check_constraint(name, "research_missions", expression)
    if needs_cycle_fk:
        op.create_foreign_key(
            "fk_research_mission_cycle",
            "research_missions",
            "research_cycles",
            ["cycle_id"],
            ["id"],
            ondelete="RESTRICT",
        )


def _create_idea_tables(bind: sa.Connection) -> None:
    tables = _table_names(bind)
    if "idea_drafts" not in tables:
        op.create_table(
            "idea_drafts",
            sa.Column("id", sa.Uuid(), nullable=False),
            sa.Column("original_idea_text", sa.Text(), nullable=False),
            sa.Column("state", sa.String(length=40), nullable=False, server_default="DRAFT"),
            sa.Column("clarification_round", sa.Integer(), nullable=False, server_default="0"),
            sa.Column("revision", sa.Integer(), nullable=False, server_default="1"),
            sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
            sa.Column("updated_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
            sa.CheckConstraint(
                "state IN ('DRAFT', 'CLARIFYING', 'READY', 'STARTED', 'DISCARDED')",
                name="ck_idea_draft_state",
            ),
            sa.CheckConstraint("revision > 0", name="ck_idea_draft_revision"),
            sa.CheckConstraint(
                "clarification_round BETWEEN 0 AND 1",
                name="ck_idea_draft_clarification_round",
            ),
            sa.PrimaryKeyConstraint("id"),
        )
    if "clarification_questions" not in tables:
        op.create_table(
            "clarification_questions",
            sa.Column("id", sa.Uuid(), nullable=False),
            sa.Column("idea_draft_id", sa.Uuid(), nullable=False),
            sa.Column("round_no", sa.Integer(), nullable=False, server_default="1"),
            sa.Column("ordinal", sa.Integer(), nullable=False),
            sa.Column("question_text", sa.Text(), nullable=False),
            sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
            sa.CheckConstraint("round_no = 1", name="ck_clarification_question_single_round"),
            sa.CheckConstraint(
                "ordinal BETWEEN 1 AND 3",
                name="ck_clarification_question_max_three",
            ),
            sa.ForeignKeyConstraint(
                ["idea_draft_id"], ["idea_drafts.id"], ondelete="CASCADE"
            ),
            sa.PrimaryKeyConstraint("id"),
            sa.UniqueConstraint(
                "idea_draft_id",
                "round_no",
                "ordinal",
                name="uq_clarification_question_ordinal",
            ),
        )
    if "ix_clarification_question_draft" not in _index_names(bind, "clarification_questions"):
        op.create_index(
            "ix_clarification_question_draft",
            "clarification_questions",
            ["idea_draft_id", "round_no", "ordinal"],
        )
    if "clarification_answers" not in tables:
        op.create_table(
            "clarification_answers",
            sa.Column("id", sa.Uuid(), nullable=False),
            sa.Column("question_id", sa.Uuid(), nullable=False),
            sa.Column("answer_text", sa.Text(), nullable=False),
            sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
            sa.ForeignKeyConstraint(
                ["question_id"], ["clarification_questions.id"], ondelete="CASCADE"
            ),
            sa.PrimaryKeyConstraint("id"),
            sa.UniqueConstraint("question_id", name="uq_clarification_answer_question"),
        )


def _create_research_cycle_table(bind: sa.Connection) -> None:
    if "research_cycles" in _table_names(bind):
        return
    op.create_table(
        "research_cycles",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("program_id", sa.Uuid(), nullable=False),
        sa.Column("cycle_no", sa.Integer(), nullable=False),
        sa.Column("trigger", sa.String(length=40), nullable=False),
        sa.Column("trigger_ref_id", sa.Uuid(), nullable=True),
        sa.Column("state", sa.String(length=40), nullable=False, server_default="PLANNED"),
        sa.Column("mission_budget", sa.Integer(), nullable=False),
        sa.Column("replan_budget", sa.Integer(), nullable=False),
        sa.Column("runtime_configuration_revision", sa.Integer(), nullable=False),
        sa.Column("started_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("finished_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("summary", _json_type(bind), nullable=False, server_default=sa.text("'{}'")),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.CheckConstraint("cycle_no > 0", name="ck_research_cycle_number"),
        sa.CheckConstraint("mission_budget >= 0", name="ck_research_cycle_mission_budget"),
        sa.CheckConstraint("replan_budget >= 0", name="ck_research_cycle_replan_budget"),
        sa.CheckConstraint(
            "runtime_configuration_revision > 0",
            name="ck_research_cycle_runtime_revision",
        ),
        sa.CheckConstraint(
            "state IN ('PLANNED', 'RUNNING', 'COOLING', 'SUCCEEDED', 'FAILED', 'CANCELLED')",
            name="ck_research_cycle_state",
        ),
        sa.CheckConstraint(
            "finished_at IS NULL OR started_at IS NULL OR started_at <= finished_at",
            name="ck_research_cycle_time_order",
        ),
        sa.CheckConstraint(
            "state NOT IN ('SUCCEEDED', 'FAILED', 'CANCELLED') OR finished_at IS NOT NULL",
            name="ck_research_cycle_terminal_finished",
        ),
        sa.ForeignKeyConstraint(["program_id"], ["research_programs.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("program_id", "cycle_no", name="uq_research_cycle_number"),
    )
    op.create_index(
        "ix_research_cycle_program_state", "research_cycles", ["program_id", "state"]
    )


def _extend_existing_research_tables(bind: sa.Connection) -> None:
    json_type = _json_type(bind)
    _add_column_if_missing(
        "research_charters", sa.Column("idea_draft_id", sa.Uuid(), nullable=True)
    )
    _add_column_if_missing(
        "research_charters",
        sa.Column(
            "clarification_transcript",
            json_type,
            nullable=False,
            server_default=sa.text("'[]'"),
        ),
    )
    _ensure_research_charter_constraints(bind)

    _add_column_if_missing(
        "research_programs", sa.Column("cooling_until", sa.DateTime(timezone=True), nullable=True)
    )
    _add_column_if_missing(
        "research_programs", sa.Column("blocked_reason_code", sa.String(length=100), nullable=True)
    )
    _add_column_if_missing(
        "research_programs", sa.Column("pause_reason", sa.Text(), nullable=True)
    )
    _add_column_if_missing(
        "research_programs", sa.Column("wake_policy_version_id", sa.Uuid(), nullable=True)
    )
    _add_column_if_missing(
        "research_programs", sa.Column("current_cycle_id", sa.Uuid(), nullable=True)
    )
    _ensure_research_program_constraints(bind)

    _add_column_if_missing(
        "research_branches", sa.Column("cycle_id", sa.Uuid(), nullable=True)
    )
    _add_column_if_missing(
        "research_branches",
        sa.Column("revision_no", sa.Integer(), nullable=False, server_default="1"),
    )
    _ensure_research_branch_constraints(bind)

    _add_column_if_missing(
        "research_missions", sa.Column("cycle_id", sa.Uuid(), nullable=True)
    )
    _add_column_if_missing(
        "research_missions", sa.Column("outcome", sa.String(length=80), nullable=True)
    )
    _add_column_if_missing(
        "research_missions",
        sa.Column("contract_version", sa.String(length=40), nullable=False, server_default="1"),
    )
    _add_column_if_missing(
        "research_missions",
        sa.Column("input_snapshot", json_type, nullable=False, server_default=sa.text("'{}'")),
    )
    _add_column_if_missing(
        "research_missions",
        sa.Column(
            "capability_snapshot", json_type, nullable=False, server_default=sa.text("'{}'")
        ),
    )
    _add_column_if_missing(
        "research_missions",
        sa.Column("runtime_snapshot", json_type, nullable=False, server_default=sa.text("'{}'")),
    )
    _add_column_if_missing(
        "research_missions",
        sa.Column("prompt_version", sa.String(length=80), nullable=False, server_default="1"),
    )
    _add_column_if_missing(
        "research_missions",
        sa.Column("max_turns", sa.Integer(), nullable=False, server_default="1"),
    )
    _add_column_if_missing(
        "research_missions",
        sa.Column("max_tool_calls", sa.Integer(), nullable=False, server_default="0"),
    )
    _add_column_if_missing(
        "research_missions",
        sa.Column("revision", sa.Integer(), nullable=False, server_default="1"),
    )
    _ensure_research_mission_constraints(bind)
    if "ix_research_mission_cycle_state" not in _index_names(bind, "research_missions"):
        op.create_index(
            "ix_research_mission_cycle_state",
            "research_missions",
            ["cycle_id", "state"],
        )


def _create_program_relationship_table(bind: sa.Connection) -> None:
    if "program_relationships" in _table_names(bind):
        return
    op.create_table(
        "program_relationships",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("from_program_id", sa.Uuid(), nullable=False),
        sa.Column("to_program_id", sa.Uuid(), nullable=False),
        sa.Column("relationship_type", sa.String(length=80), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.CheckConstraint(
            "from_program_id <> to_program_id", name="ck_program_relationship_not_self"
        ),
        sa.ForeignKeyConstraint(
            ["from_program_id"], ["research_programs.id"], ondelete="RESTRICT"
        ),
        sa.ForeignKeyConstraint(
            ["to_program_id"], ["research_programs.id"], ondelete="RESTRICT"
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "from_program_id",
            "to_program_id",
            "relationship_type",
            name="uq_program_relationship",
        ),
    )
    op.create_index(
        "ix_program_relationship_to",
        "program_relationships",
        ["to_program_id", "relationship_type"],
    )


def _create_agent_tables(bind: sa.Connection) -> None:
    tables = _table_names(bind)
    json_type = _json_type(bind)
    if "mission_dependencies" not in tables:
        op.create_table(
            "mission_dependencies",
            sa.Column("mission_id", sa.Uuid(), nullable=False),
            sa.Column("depends_on_mission_id", sa.Uuid(), nullable=False),
            sa.Column("required_outcome", sa.String(length=80), nullable=True),
            sa.CheckConstraint(
                "mission_id <> depends_on_mission_id", name="ck_mission_dependency_not_self"
            ),
            sa.ForeignKeyConstraint(
                ["mission_id"], ["research_missions.id"], ondelete="RESTRICT"
            ),
            sa.ForeignKeyConstraint(
                ["depends_on_mission_id"], ["research_missions.id"], ondelete="RESTRICT"
            ),
            sa.PrimaryKeyConstraint("mission_id", "depends_on_mission_id"),
        )
    if "ix_mission_dependency_depends_on" not in _index_names(bind, "mission_dependencies"):
        op.create_index(
            "ix_mission_dependency_depends_on",
            "mission_dependencies",
            ["depends_on_mission_id"],
        )
    if "agent_sessions" not in tables:
        op.create_table(
            "agent_sessions",
            sa.Column("id", sa.Uuid(), nullable=False),
            sa.Column("mission_id", sa.Uuid(), nullable=False),
            sa.Column("role_profile", sa.String(length=80), nullable=False),
            sa.Column("codex_thread_id", sa.String(length=200), nullable=False),
            sa.Column("codex_version", sa.String(length=40), nullable=False),
            sa.Column("model", sa.String(length=120), nullable=True),
            sa.Column("reasoning_effort", sa.String(length=20), nullable=True),
            sa.Column("service_tier", sa.String(length=20), nullable=True),
            sa.Column("state", sa.String(length=40), nullable=False, server_default="PLANNED"),
            sa.Column("started_at", sa.DateTime(timezone=True), nullable=True),
            sa.Column("finished_at", sa.DateTime(timezone=True), nullable=True),
            sa.Column("last_event_at", sa.DateTime(timezone=True), nullable=True),
            sa.CheckConstraint(
                "state IN ('PLANNED', 'RUNNING', 'SUCCEEDED', 'FAILED', 'INTERRUPTED', 'CANCELLED')",
                name="ck_agent_session_state",
            ),
            sa.CheckConstraint(
                "reasoning_effort IS NULL OR reasoning_effort IN "
                "('minimal', 'low', 'medium', 'high', 'xhigh')",
                name="ck_agent_session_reasoning_effort",
            ),
            sa.CheckConstraint(
                "service_tier IS NULL OR service_tier IN ('standard', 'fast')",
                name="ck_agent_session_service_tier",
            ),
            sa.CheckConstraint(
                "finished_at IS NULL OR started_at IS NULL OR started_at <= finished_at",
                name="ck_agent_session_time_order",
            ),
            sa.CheckConstraint(
                "state NOT IN ('SUCCEEDED', 'FAILED', 'INTERRUPTED', 'CANCELLED') "
                "OR finished_at IS NOT NULL",
                name="ck_agent_session_terminal_finished",
            ),
            sa.ForeignKeyConstraint(["mission_id"], ["research_missions.id"], ondelete="RESTRICT"),
            sa.PrimaryKeyConstraint("id"),
            sa.UniqueConstraint("mission_id", name="uq_agent_session_mission"),
            sa.UniqueConstraint("codex_thread_id", name="uq_agent_session_thread"),
        )
    if "agent_turns" not in tables:
        op.create_table(
            "agent_turns",
            sa.Column("id", sa.Uuid(), nullable=False),
            sa.Column("agent_session_id", sa.Uuid(), nullable=False),
            sa.Column("ordinal", sa.Integer(), nullable=False),
            sa.Column("kind", sa.String(length=40), nullable=False),
            sa.Column("codex_turn_id", sa.String(length=200), nullable=False),
            sa.Column("state", sa.String(length=40), nullable=False, server_default="PLANNED"),
            sa.Column("input_artifact_ids", json_type, nullable=False, server_default=sa.text("'[]'")),
            sa.Column("output_artifact_ids", json_type, nullable=False, server_default=sa.text("'[]'")),
            sa.Column("observable_summary", sa.Text(), nullable=True),
            sa.Column("tool_call_count", sa.Integer(), nullable=False, server_default="0"),
            sa.Column("started_at", sa.DateTime(timezone=True), nullable=True),
            sa.Column("finished_at", sa.DateTime(timezone=True), nullable=True),
            sa.Column("error_code", sa.String(length=100), nullable=True),
            sa.CheckConstraint("ordinal > 0", name="ck_agent_turn_ordinal"),
            sa.CheckConstraint(
                "kind IN ('PLAN', 'IMPLEMENT', 'REPAIR', 'REVIEW', 'REPLAN', 'DIAGNOSE')",
                name="ck_agent_turn_kind",
            ),
            sa.CheckConstraint(
                "state IN ('PLANNED', 'RUNNING', 'SUCCEEDED', 'FAILED', 'INTERRUPTED', 'CANCELLED')",
                name="ck_agent_turn_state",
            ),
            sa.CheckConstraint("tool_call_count >= 0", name="ck_agent_turn_tool_call_count"),
            sa.CheckConstraint(
                "finished_at IS NULL OR started_at IS NULL OR started_at <= finished_at",
                name="ck_agent_turn_time_order",
            ),
            sa.CheckConstraint(
                "state NOT IN ('SUCCEEDED', 'FAILED', 'INTERRUPTED', 'CANCELLED') "
                "OR finished_at IS NOT NULL",
                name="ck_agent_turn_terminal_finished",
            ),
            sa.ForeignKeyConstraint(
                ["agent_session_id"], ["agent_sessions.id"], ondelete="RESTRICT"
            ),
            sa.PrimaryKeyConstraint("id"),
            sa.UniqueConstraint("agent_session_id", "ordinal", name="uq_agent_turn_ordinal"),
            sa.UniqueConstraint(
                "agent_session_id", "codex_turn_id", name="uq_agent_turn_codex_turn"
            ),
        )
    if "ix_agent_turn_session_ordinal" not in _index_names(bind, "agent_turns"):
        op.create_index(
            "ix_agent_turn_session_ordinal", "agent_turns", ["agent_session_id", "ordinal"]
        )
    if "mission_artifacts" not in tables:
        op.create_table(
            "mission_artifacts",
            sa.Column("id", sa.Uuid(), nullable=False),
            sa.Column("mission_id", sa.Uuid(), nullable=False),
            sa.Column("turn_id", sa.Uuid(), nullable=True),
            sa.Column("kind", sa.String(length=80), nullable=False),
            sa.Column("schema_version", sa.String(length=40), nullable=False),
            sa.Column("revision", sa.Integer(), nullable=False, server_default="1"),
            sa.Column("state", sa.String(length=40), nullable=False, server_default="DRAFT"),
            sa.Column("storage_uri", sa.Text(), nullable=False),
            sa.Column("metadata", json_type, nullable=False, server_default=sa.text("'{}'")),
            sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
            sa.CheckConstraint("revision > 0", name="ck_mission_artifact_revision"),
            sa.CheckConstraint(
                "state IN ('DRAFT', 'VALIDATED', 'REJECTED')",
                name="ck_mission_artifact_state",
            ),
            sa.CheckConstraint(
                "length(storage_uri) > 0", name="ck_mission_artifact_storage_uri"
            ),
            sa.ForeignKeyConstraint(["mission_id"], ["research_missions.id"], ondelete="RESTRICT"),
            sa.ForeignKeyConstraint(["turn_id"], ["agent_turns.id"], ondelete="SET NULL"),
            sa.PrimaryKeyConstraint("id"),
            sa.UniqueConstraint("mission_id", "kind", "revision", name="uq_mission_artifact_revision"),
        )
    if "ix_mission_artifact_mission" not in _index_names(bind, "mission_artifacts"):
        op.create_index(
            "ix_mission_artifact_mission", "mission_artifacts", ["mission_id", "kind"]
        )


def _create_preflight_receipt_table(bind: sa.Connection) -> None:
    if "preflight_receipts" in _table_names(bind):
        return
    op.create_table(
        "preflight_receipts",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("resource_type", sa.String(length=80), nullable=False),
        sa.Column("resource_id", sa.Uuid(), nullable=False),
        sa.Column("resource_revision", sa.Integer(), nullable=False),
        sa.Column("revision", sa.Integer(), nullable=False, server_default="1"),
        sa.Column("status", sa.String(length=40), nullable=False),
        sa.Column("reason_codes", _json_type(bind), nullable=False, server_default=sa.text("'[]'")),
        sa.Column("capabilities", _json_type(bind), nullable=False, server_default=sa.text("'[]'")),
        sa.Column("contract_version", sa.String(length=40), nullable=False),
        sa.Column("remote_identity", sa.String(length=200), nullable=True),
        sa.Column("checked_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("valid_until", sa.DateTime(timezone=True), nullable=False),
        sa.Column("checker_version", sa.String(length=80), nullable=False),
        sa.CheckConstraint("resource_revision > 0", name="ck_preflight_receipt_resource_revision"),
        sa.CheckConstraint("revision > 0", name="ck_preflight_receipt_revision"),
        sa.CheckConstraint(
            "status IN ('READY', 'DEGRADED', 'FAILED', 'EXPIRED')",
            name="ck_preflight_receipt_status",
        ),
        sa.CheckConstraint("checked_at <= valid_until", name="ck_preflight_receipt_validity"),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "resource_type",
            "resource_id",
            "resource_revision",
            "revision",
            name="uq_preflight_receipt_revision",
        ),
    )
    op.create_index(
        "ix_preflight_receipt_resource_validity",
        "preflight_receipts",
        ["resource_type", "resource_id", "valid_until"],
    )


def upgrade() -> None:
    bind = op.get_bind()
    _create_idea_tables(bind)
    _create_research_cycle_table(bind)
    _extend_existing_research_tables(bind)
    _create_program_relationship_table(bind)
    _create_agent_tables(bind)
    _create_preflight_receipt_table(bind)


def downgrade() -> None:
    raise RuntimeError(
        "0016_research_agent_domain is intentionally irreversible: it establishes "
        "durable research, agent, and preflight facts."
    )
