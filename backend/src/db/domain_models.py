"""QuaZonai research-intelligence and portfolio-construction domain models."""

from __future__ import annotations

from datetime import datetime
from decimal import Decimal
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import (
    BigInteger,
    Boolean,
    CheckConstraint,
    DateTime,
    ForeignKey,
    ForeignKeyConstraint,
    Index,
    Integer,
    LargeBinary,
    Numeric,
    String,
    Text,
    UniqueConstraint,
    Uuid,
    func,
    text,
)
from sqlalchemy.orm import Mapped, mapped_column, synonym

from db.base import IDENTITY_INT, Base, JSON_VALUE, TimestampMixin


class PublicMutationReceipt(Base):
    __tablename__ = "public_mutation_receipts"

    idempotency_key: Mapped[str] = mapped_column(String(200), primary_key=True)
    operation_name: Mapped[str] = mapped_column(String(240), nullable=False)
    normalized_request: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    response_json: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    status_code: Mapped[int] = mapped_column(Integer, nullable=False, default=200)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class ResearchCharter(Base):
    __tablename__ = "research_charters"
    __table_args__ = (UniqueConstraint("idea_draft_id", name="uq_research_charter_idea_draft"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    # Nullable only while legacy Charters are retained ahead of their Draft backfill.
    idea_draft_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("idea_drafts.id", ondelete="RESTRICT")
    )
    original_idea_text: Mapped[str] = mapped_column(Text, nullable=False)
    research_question: Mapped[str] = mapped_column(Text, nullable=False)
    market_scope: Mapped[Any] = mapped_column(JSON_VALUE, nullable=False, default=list)
    universe_version_ids: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    prediction_horizon: Mapped[str | None] = mapped_column(String(100))
    allowed_data_domains: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    explicit_exclusions: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    material_assumptions: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    system_assumptions: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    clarification_transcript: Mapped[list[dict[str, Any]]] = mapped_column(
        JSON_VALUE, nullable=False, default=list
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class ResearchProgram(Base, TimestampMixin):
    __tablename__ = "research_programs"
    __table_args__ = (
        Index("ix_research_program_state", "state"),
        CheckConstraint(
            "state IN ('ACTIVE', 'COOLING', 'APPROVAL_PENDING', 'WAITING_FOR_FEEDBACK', "
            "'BLOCKED', 'PAUSED', 'ARCHIVED')",
            name="ck_research_program_state",
        ),
        CheckConstraint("revision > 0", name="ck_research_program_revision"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    charter_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_charters.id", ondelete="RESTRICT"), nullable=False
    )
    title: Mapped[str] = mapped_column(String(240), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")
    cooling_reason: Mapped[str | None] = mapped_column(Text)
    cooling_until: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    blocked_reason: Mapped[str | None] = mapped_column(Text)
    blocked_reason_code: Mapped[str | None] = mapped_column(String(100))
    pause_reason: Mapped[str | None] = mapped_column(Text)
    wake_reason: Mapped[str | None] = mapped_column(Text)
    wake_policy_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    current_cycle_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey(
            "research_cycles.id",
            name="fk_research_program_current_cycle",
            ondelete="SET NULL",
            use_alter=True,
        ),
    )
    source_program_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="RESTRICT")
    )
    relationship_type: Mapped[str | None] = mapped_column(String(80))
    evidence_inherited_from_program_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="RESTRICT")
    )
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)


class IdeaContribution(Base):
    __tablename__ = "idea_contributions"
    __table_args__ = (Index("ix_idea_contribution_program", "program_id", "created_at"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="CASCADE"), nullable=False
    )
    idea_text: Mapped[str] = mapped_column(Text, nullable=False)
    action: Mapped[str] = mapped_column(String(80), nullable=False)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class IdeaDraft(Base, TimestampMixin):
    __tablename__ = "idea_drafts"
    __table_args__ = (
        CheckConstraint(
            "state IN ('DRAFT', 'CLARIFYING', 'READY', 'STARTED', 'DISCARDED')",
            name="ck_idea_draft_state",
        ),
        CheckConstraint("revision > 0", name="ck_idea_draft_revision"),
        CheckConstraint(
            "clarification_round BETWEEN 0 AND 1",
            name="ck_idea_draft_clarification_round",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    original_idea_text: Mapped[str] = mapped_column(Text, nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="DRAFT")
    clarification_round: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)


class ClarificationQuestion(Base):
    __tablename__ = "clarification_questions"
    __table_args__ = (
        UniqueConstraint(
            "idea_draft_id",
            "round_no",
            "ordinal",
            name="uq_clarification_question_ordinal",
        ),
        CheckConstraint("round_no = 1", name="ck_clarification_question_single_round"),
        CheckConstraint(
            "ordinal BETWEEN 1 AND 3",
            name="ck_clarification_question_max_three",
        ),
        Index("ix_clarification_question_draft", "idea_draft_id", "round_no", "ordinal"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    idea_draft_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("idea_drafts.id", ondelete="CASCADE"), nullable=False
    )
    round_no: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    ordinal: Mapped[int] = mapped_column(Integer, nullable=False)
    question_text: Mapped[str] = mapped_column(Text, nullable=False)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class ClarificationAnswer(Base):
    __tablename__ = "clarification_answers"

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    question_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("clarification_questions.id", ondelete="CASCADE"),
        nullable=False,
        unique=True,
    )
    answer_text: Mapped[str] = mapped_column(Text, nullable=False)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class ProgramRelationship(Base):
    __tablename__ = "program_relationships"
    __table_args__ = (
        UniqueConstraint(
            "from_program_id",
            "to_program_id",
            "relationship_type",
            name="uq_program_relationship",
        ),
        CheckConstraint(
            "from_program_id <> to_program_id",
            name="ck_program_relationship_not_self",
        ),
        Index("ix_program_relationship_to", "to_program_id", "relationship_type"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    from_program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="RESTRICT"), nullable=False
    )
    to_program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="RESTRICT"), nullable=False
    )
    relationship_type: Mapped[str] = mapped_column(String(80), nullable=False)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class ResearchBranch(Base):
    __tablename__ = "research_branches"
    __table_args__ = (
        Index("ix_research_branch_program", "program_id"),
        CheckConstraint("revision_no > 0", name="ck_research_branch_revision"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="CASCADE"), nullable=False
    )
    # Nullable only until the legacy branch rows are backfilled into their first Cycle.
    cycle_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("research_cycles.id", ondelete="RESTRICT")
    )
    parent_branch_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("research_branches.id", ondelete="RESTRICT")
    )
    derivation_type: Mapped[str] = mapped_column(String(80), nullable=False, default="ROOT")
    hypothesis: Mapped[str] = mapped_column(Text, nullable=False)
    changed_assumptions: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    preserved_constraints: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")
    revision_no: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class ResearchMission(Base):
    __tablename__ = "research_missions"
    __table_args__ = (
        Index("ix_research_mission_program_state", "program_id", "state"),
        Index("ix_research_mission_cycle_state", "cycle_id", "state"),
        CheckConstraint(
            "state IN ('PLANNED', 'READY', 'RUNNING', 'SUCCEEDED', 'FAILED', "
            "'INTERRUPTED', 'CANCELLED', 'AWAITING_VALIDATION')",
            name="ck_research_mission_state",
        ),
        CheckConstraint("attempt > 0", name="ck_research_mission_attempt"),
        CheckConstraint("revision > 0", name="ck_research_mission_revision"),
        CheckConstraint("max_turns > 0", name="ck_research_mission_max_turns"),
        CheckConstraint("max_tool_calls >= 0", name="ck_research_mission_max_tool_calls"),
        CheckConstraint(
            "finished_at IS NULL OR started_at IS NULL OR started_at <= finished_at",
            name="ck_research_mission_time_order",
        ),
        CheckConstraint(
            "state NOT IN ('SUCCEEDED', 'FAILED', 'INTERRUPTED', 'CANCELLED') "
            "OR finished_at IS NOT NULL",
            name="ck_research_mission_terminal_finished",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="CASCADE"), nullable=False
    )
    # Nullable only until legacy Mission rows are assigned to a persisted Cycle.
    cycle_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("research_cycles.id", ondelete="RESTRICT")
    )
    branch_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_branches.id", ondelete="CASCADE"), nullable=False
    )
    type: Mapped[str] = mapped_column(String(100), nullable=False)
    # ``mission_type`` is the stricter domain spelling; the physical legacy column
    # remains canonical until its later, explicitly backfilled rename.
    mission_type = synonym("type")
    role: Mapped[str | None] = mapped_column(String(100))
    # ``role_profile`` is the domain spelling without creating a second fact column.
    role_profile = synonym("role")
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="PLANNED")
    outcome: Mapped[str | None] = mapped_column(String(80))
    objective: Mapped[str | None] = mapped_column(Text)
    contract_version: Mapped[str] = mapped_column(String(40), nullable=False, default="1")
    input_snapshot: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    capability_snapshot: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    runtime_snapshot: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    prompt_version: Mapped[str] = mapped_column(String(80), nullable=False, default="1")
    max_turns: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    max_tool_calls: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    dependencies: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    codex_thread_id: Mapped[str | None] = mapped_column(String(200))
    workspace_path: Mapped[str | None] = mapped_column(Text)
    started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    finished_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    attempt: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    error_code: Mapped[str | None] = mapped_column(String(100))
    summary: Mapped[str | None] = mapped_column(Text)


class ResearchCycle(Base):
    __tablename__ = "research_cycles"
    __table_args__ = (
        UniqueConstraint("program_id", "cycle_no", name="uq_research_cycle_number"),
        CheckConstraint("cycle_no > 0", name="ck_research_cycle_number"),
        CheckConstraint("mission_budget >= 0", name="ck_research_cycle_mission_budget"),
        CheckConstraint("replan_budget >= 0", name="ck_research_cycle_replan_budget"),
        CheckConstraint(
            "runtime_configuration_revision > 0",
            name="ck_research_cycle_runtime_revision",
        ),
        CheckConstraint(
            "state IN ('PLANNED', 'RUNNING', 'COOLING', 'SUCCEEDED', 'FAILED', 'CANCELLED')",
            name="ck_research_cycle_state",
        ),
        CheckConstraint(
            "finished_at IS NULL OR started_at IS NULL OR started_at <= finished_at",
            name="ck_research_cycle_time_order",
        ),
        CheckConstraint(
            "state NOT IN ('SUCCEEDED', 'FAILED', 'CANCELLED') OR finished_at IS NOT NULL",
            name="ck_research_cycle_terminal_finished",
        ),
        Index("ix_research_cycle_program_state", "program_id", "state"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="CASCADE"), nullable=False
    )
    cycle_no: Mapped[int] = mapped_column(Integer, nullable=False)
    trigger: Mapped[str] = mapped_column(String(40), nullable=False)
    trigger_ref_id: Mapped[UUID | None] = mapped_column(Uuid)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="PLANNED")
    mission_budget: Mapped[int] = mapped_column(Integer, nullable=False)
    replan_budget: Mapped[int] = mapped_column(Integer, nullable=False)
    runtime_configuration_revision: Mapped[int] = mapped_column(Integer, nullable=False)
    started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    finished_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    summary: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class MissionDependency(Base):
    __tablename__ = "mission_dependencies"
    __table_args__ = (
        CheckConstraint(
            "mission_id <> depends_on_mission_id",
            name="ck_mission_dependency_not_self",
        ),
        Index("ix_mission_dependency_depends_on", "depends_on_mission_id"),
    )

    mission_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_missions.id", ondelete="RESTRICT"),
        primary_key=True,
    )
    depends_on_mission_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("research_missions.id", ondelete="RESTRICT"),
        primary_key=True,
    )
    required_outcome: Mapped[str | None] = mapped_column(String(80))


class AgentSession(Base):
    __tablename__ = "agent_sessions"
    __table_args__ = (
        UniqueConstraint("mission_id", name="uq_agent_session_mission"),
        UniqueConstraint("codex_thread_id", name="uq_agent_session_thread"),
        CheckConstraint(
            "state IN ('PLANNED', 'RUNNING', 'SUCCEEDED', 'FAILED', 'INTERRUPTED', 'CANCELLED')",
            name="ck_agent_session_state",
        ),
        CheckConstraint(
            "reasoning_effort IS NULL OR reasoning_effort IN "
            "('minimal', 'low', 'medium', 'high', 'xhigh')",
            name="ck_agent_session_reasoning_effort",
        ),
        CheckConstraint(
            "service_tier IS NULL OR service_tier IN ('standard', 'fast')",
            name="ck_agent_session_service_tier",
        ),
        CheckConstraint(
            "finished_at IS NULL OR started_at IS NULL OR started_at <= finished_at",
            name="ck_agent_session_time_order",
        ),
        CheckConstraint(
            "state NOT IN ('SUCCEEDED', 'FAILED', 'INTERRUPTED', 'CANCELLED') "
            "OR finished_at IS NOT NULL",
            name="ck_agent_session_terminal_finished",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    mission_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_missions.id", ondelete="RESTRICT"), nullable=False
    )
    role_profile: Mapped[str] = mapped_column(String(80), nullable=False)
    codex_thread_id: Mapped[str] = mapped_column(String(200), nullable=False)
    codex_version: Mapped[str] = mapped_column(String(40), nullable=False)
    model: Mapped[str | None] = mapped_column(String(120))
    reasoning_effort: Mapped[str | None] = mapped_column(String(20))
    service_tier: Mapped[str | None] = mapped_column(String(20))
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="PLANNED")
    started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    finished_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    last_event_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class AgentTurn(Base):
    __tablename__ = "agent_turns"
    __table_args__ = (
        UniqueConstraint("agent_session_id", "ordinal", name="uq_agent_turn_ordinal"),
        UniqueConstraint("agent_session_id", "codex_turn_id", name="uq_agent_turn_codex_turn"),
        CheckConstraint("ordinal > 0", name="ck_agent_turn_ordinal"),
        CheckConstraint(
            "kind IN ('PLAN', 'IMPLEMENT', 'VALIDATE', 'EXECUTE', 'REPAIR', 'REVIEW', "
            "'REPLAN', 'DIAGNOSE')",
            name="ck_agent_turn_kind",
        ),
        CheckConstraint(
            "state IN ('PLANNED', 'RUNNING', 'SUCCEEDED', 'FAILED', 'INTERRUPTED', 'CANCELLED')",
            name="ck_agent_turn_state",
        ),
        CheckConstraint("tool_call_count >= 0", name="ck_agent_turn_tool_call_count"),
        CheckConstraint(
            "finished_at IS NULL OR started_at IS NULL OR started_at <= finished_at",
            name="ck_agent_turn_time_order",
        ),
        CheckConstraint(
            "state NOT IN ('SUCCEEDED', 'FAILED', 'INTERRUPTED', 'CANCELLED') "
            "OR finished_at IS NOT NULL",
            name="ck_agent_turn_terminal_finished",
        ),
        Index("ix_agent_turn_session_ordinal", "agent_session_id", "ordinal"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    agent_session_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("agent_sessions.id", ondelete="RESTRICT"), nullable=False
    )
    ordinal: Mapped[int] = mapped_column(Integer, nullable=False)
    kind: Mapped[str] = mapped_column(String(40), nullable=False)
    codex_turn_id: Mapped[str] = mapped_column(String(200), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="PLANNED")
    input_artifact_ids: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    output_artifact_ids: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    observable_summary: Mapped[str | None] = mapped_column(Text)
    tool_call_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    finished_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    error_code: Mapped[str | None] = mapped_column(String(100))


class MissionArtifact(Base):
    __tablename__ = "mission_artifacts"
    __table_args__ = (
        UniqueConstraint("mission_id", "kind", "revision", name="uq_mission_artifact_revision"),
        CheckConstraint("revision > 0", name="ck_mission_artifact_revision"),
        CheckConstraint(
            "state IN ('DRAFT', 'VALIDATED', 'REJECTED')",
            name="ck_mission_artifact_state",
        ),
        CheckConstraint("length(storage_uri) > 0", name="ck_mission_artifact_storage_uri"),
        Index("ix_mission_artifact_mission", "mission_id", "kind"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    mission_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_missions.id", ondelete="RESTRICT"), nullable=False
    )
    turn_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("agent_turns.id", ondelete="SET NULL")
    )
    kind: Mapped[str] = mapped_column(String(80), nullable=False)
    schema_version: Mapped[str] = mapped_column(String(40), nullable=False)
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="DRAFT")
    storage_uri: Mapped[str] = mapped_column(Text, nullable=False)
    metadata_json: Mapped[dict[str, Any]] = mapped_column(
        "metadata", JSON_VALUE, nullable=False, default=dict
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class PreflightReceipt(Base):
    __tablename__ = "preflight_receipts"
    __table_args__ = (
        UniqueConstraint(
            "resource_type",
            "resource_id",
            "resource_revision",
            "revision",
            name="uq_preflight_receipt_revision",
        ),
        CheckConstraint("resource_revision > 0", name="ck_preflight_receipt_resource_revision"),
        CheckConstraint("revision > 0", name="ck_preflight_receipt_revision"),
        CheckConstraint(
            "status IN ('READY', 'DEGRADED', 'FAILED', 'EXPIRED')",
            name="ck_preflight_receipt_status",
        ),
        CheckConstraint("checked_at <= valid_until", name="ck_preflight_receipt_validity"),
        Index(
            "ix_preflight_receipt_resource_validity",
            "resource_type",
            "resource_id",
            "valid_until",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    resource_type: Mapped[str] = mapped_column(String(80), nullable=False)
    resource_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    resource_revision: Mapped[int] = mapped_column(Integer, nullable=False)
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    status: Mapped[str] = mapped_column(String(40), nullable=False)
    reason_codes: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    capabilities: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    contract_version: Mapped[str] = mapped_column(String(40), nullable=False)
    remote_identity: Mapped[str | None] = mapped_column(String(200))
    checked_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    valid_until: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    checker_version: Mapped[str] = mapped_column(String(80), nullable=False)


class MarketUniverseVersion(Base):
    __tablename__ = "market_universe_versions"
    __table_args__ = (
        UniqueConstraint("universe_key", "version_no", name="uq_market_universe_version"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    universe_key: Mapped[str] = mapped_column(String(120), nullable=False)
    version_no: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    name: Mapped[str] = mapped_column(String(200), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")
    spec_json: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class GovernedDataSource(Base, TimestampMixin):
    __tablename__ = "governed_data_sources"
    __table_args__ = (UniqueConstraint("name", name="uq_governed_data_source_name"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    name: Mapped[str] = mapped_column(String(200), nullable=False)
    # These fields describe an approved connector contract.  Credentials stay
    # in CredentialSet, never in this public configuration record.
    connector_key: Mapped[str] = mapped_column(
        String(80), nullable=False, default="UNSPECIFIED", server_default="UNSPECIFIED"
    )
    provider: Mapped[str | None] = mapped_column(String(200))
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")
    universe_scope: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    fields: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    field_schema: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    license_classification: Mapped[str] = mapped_column(
        String(80), nullable=False, default="UNCLASSIFIED", server_default="UNCLASSIFIED"
    )
    availability_semantics: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    update_cadence: Mapped[str | None] = mapped_column(String(100))
    preflight_state: Mapped[str] = mapped_column(String(40), nullable=False, default="READY")
    public_config: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)


class DatasetRevision(Base):
    __tablename__ = "dataset_revisions"
    __table_args__ = (
        Index("ix_dataset_revision_source", "data_source_id"),
        Index(
            "uq_dataset_revision_canonical",
            "data_source_id",
            "universe_version_id",
            "revision_no",
            "partition",
            unique=True,
            sqlite_where=text("data_class IS NOT NULL"),
            postgresql_where=text("data_class IS NOT NULL"),
        ),
        CheckConstraint(
            "data_class IS NULL OR data_class IN "
            "('SYNTHETIC', 'FIXTURE', 'VENDOR', 'PRODUCTION')",
            name="ck_dataset_revision_data_class",
        ),
        CheckConstraint(
            "promotability IS NULL OR promotability IN ('PROMOTABLE', 'NON_PROMOTABLE')",
            name="ck_dataset_revision_promotability",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    data_source_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("governed_data_sources.id", ondelete="RESTRICT")
    )
    universe_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("market_universe_versions.id", ondelete="RESTRICT")
    )
    universe_name: Mapped[str | None] = mapped_column(String(200))
    revision_no: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    # Nullable while pre-0017 Dataset rows await their explicit data classification.
    data_class: Mapped[str | None] = mapped_column(String(20))
    origin: Mapped[str | None] = mapped_column(String(200))
    ingested_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    promotability: Mapped[str | None] = mapped_column(String(20))
    schema_version: Mapped[str | None] = mapped_column(String(100))
    event_start: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    event_end: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    available_start: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    available_end: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    row_count: Mapped[int | None] = mapped_column(Integer)
    quality_state: Mapped[str] = mapped_column(String(40), nullable=False, default="VALID")
    quality_result_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey(
            "data_quality_results.id",
            name="fk_dataset_revision_quality_result",
            ondelete="SET NULL",
            use_alter=True,
        ),
    )
    point_in_time_state: Mapped[str] = mapped_column(String(40), nullable=False, default="VALID")
    point_in_time_result_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey(
            "data_quality_results.id",
            name="fk_dataset_revision_point_in_time_result",
            ondelete="SET NULL",
            use_alter=True,
        ),
    )
    partition: Mapped[str] = mapped_column(String(40), nullable=False, default="DISCOVERY")
    # The immutable request is retained while a materialization job is pending;
    # it is not a catalog or a claim that quality validation has passed.
    materialization_request: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class DataQualityResult(Base):
    """An immutable quality or point-in-time result for one Dataset Revision."""

    __tablename__ = "data_quality_results"
    __table_args__ = (
        UniqueConstraint(
            "dataset_revision_id",
            "check_kind",
            "revision_no",
            name="uq_data_quality_result_revision",
        ),
        CheckConstraint("revision_no > 0", name="ck_data_quality_result_revision"),
        CheckConstraint(
            "check_kind IN ('QUALITY', 'POINT_IN_TIME')",
            name="ck_data_quality_result_kind",
        ),
        CheckConstraint(
            "state IN ('VALID', 'INVALID')",
            name="ck_data_quality_result_state",
        ),
        Index("ix_data_quality_result_dataset", "dataset_revision_id", "check_kind"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("dataset_revisions.id", ondelete="CASCADE"), nullable=False
    )
    check_kind: Mapped[str] = mapped_column(String(40), nullable=False)
    revision_no: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    state: Mapped[str] = mapped_column(String(40), nullable=False)
    summary: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    checker_version: Mapped[str] = mapped_column(String(80), nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class EvaluationDatasetSelection(Base):
    """Immutable, explicitly phased Dataset Revision bindings for one Universe."""

    __tablename__ = "evaluation_dataset_selections"
    __table_args__ = (
        UniqueConstraint(
            "universe_version_id",
            "version_no",
            name="uq_evaluation_dataset_selection_version",
        ),
        UniqueConstraint(
            "id",
            "discovery_dataset_revision_id",
            name="uq_evaluation_dataset_selection_discovery_dataset",
        ),
        UniqueConstraint(
            "id",
            "sealed_dataset_revision_id",
            name="uq_evaluation_dataset_selection_sealed_dataset",
        ),
        CheckConstraint("version_no > 0", name="ck_evaluation_dataset_selection_version"),
        CheckConstraint(
            "state IN ('ENABLED', 'RETIRED')",
            name="ck_evaluation_dataset_selection_state",
        ),
        CheckConstraint(
            "discovery_dataset_revision_id <> validation_dataset_revision_id AND "
            "discovery_dataset_revision_id <> sealed_dataset_revision_id AND "
            "validation_dataset_revision_id <> sealed_dataset_revision_id",
            name="ck_evaluation_dataset_selection_distinct_revisions",
        ),
        Index(
            "uq_evaluation_dataset_selection_enabled",
            "universe_version_id",
            unique=True,
            sqlite_where=text("state = 'ENABLED'"),
            postgresql_where=text("state = 'ENABLED'"),
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    universe_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("market_universe_versions.id", ondelete="RESTRICT"), nullable=False
    )
    version_no: Mapped[int] = mapped_column(Integer, nullable=False)
    discovery_dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("dataset_revisions.id", ondelete="RESTRICT"), nullable=False
    )
    validation_dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("dataset_revisions.id", ondelete="RESTRICT"), nullable=False
    )
    sealed_dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("dataset_revisions.id", ondelete="RESTRICT"), nullable=False
    )
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="ENABLED")
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class EvaluationDesignVersion(Base):
    """Server-owned typed statistical and disclosure rules for Alpha evaluation."""

    __tablename__ = "evaluation_design_versions"
    __table_args__ = (
        UniqueConstraint(
            "universe_version_id",
            "version_no",
            name="uq_evaluation_design_version",
        ),
        CheckConstraint("version_no > 0", name="ck_evaluation_design_version_number"),
        CheckConstraint("length(contract_version) > 0", name="ck_evaluation_design_contract"),
        CheckConstraint(
            "allowed_model_mode IN ('RELATIVE_SCORE', 'CALIBRATED_RETURN')",
            name="ck_evaluation_design_model_mode",
        ),
        CheckConstraint(
            "qualification_role IN "
            "('PRIMARY_ALPHA', 'DIVERSIFIER_ALPHA', 'HEDGE_ALPHA', "
            "'REGIME_SIGNAL', 'RISK_MODULATOR', 'SHADOW_ALPHA')",
            name="ck_evaluation_design_role",
        ),
        CheckConstraint("walk_forward_folds > 0", name="ck_evaluation_design_walk_forward"),
        CheckConstraint("annualization_factor > 0", name="ck_evaluation_design_annualization"),
        CheckConstraint(
            "multiple_testing_method IN ('BONFERRONI', 'BENJAMINI_HOCHBERG')",
            name="ck_evaluation_design_multiple_testing_method",
        ),
        CheckConstraint(
            "multiple_testing_max_trials > 0",
            name="ck_evaluation_design_multiple_testing_trials",
        ),
        CheckConstraint(
            "qualification_comparator IN ('MINIMUM', 'MAXIMUM')",
            name="ck_evaluation_design_qualification_comparator",
        ),
        CheckConstraint(
            "length(qualification_metric_code) > 0",
            name="ck_evaluation_design_qualification_metric",
        ),
        CheckConstraint(
            "length(pass_disclosure_code) > 0 AND length(failure_disclosure_code) > 0 "
            "AND length(inconclusive_disclosure_code) > 0 "
            "AND length(invalid_disclosure_code) > 0",
            name="ck_evaluation_design_disclosure_codes",
        ),
        CheckConstraint(
            "state IN ('ACTIVE', 'RETIRED')",
            name="ck_evaluation_design_state",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    version_no: Mapped[int] = mapped_column(Integer, nullable=False)
    universe_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("market_universe_versions.id", ondelete="RESTRICT"), nullable=False
    )
    contract_version: Mapped[str] = mapped_column(String(40), nullable=False)
    allowed_model_mode: Mapped[str] = mapped_column(String(20), nullable=False)
    qualification_role: Mapped[str] = mapped_column(String(40), nullable=False)
    walk_forward_folds: Mapped[int] = mapped_column(Integer, nullable=False)
    annualization_factor: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    multiple_testing_method: Mapped[str] = mapped_column(String(40), nullable=False)
    multiple_testing_max_trials: Mapped[int] = mapped_column(Integer, nullable=False)
    qualification_metric_code: Mapped[str] = mapped_column(String(100), nullable=False)
    qualification_comparator: Mapped[str] = mapped_column(String(20), nullable=False)
    qualification_threshold: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    pass_disclosure_code: Mapped[str] = mapped_column(String(100), nullable=False)
    failure_disclosure_code: Mapped[str] = mapped_column(String(100), nullable=False)
    inconclusive_disclosure_code: Mapped[str] = mapped_column(String(100), nullable=False)
    invalid_disclosure_code: Mapped[str] = mapped_column(String(100), nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="ACTIVE")
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class FeaturePipelineVersion(Base):
    """One immutable, point-in-time-aware feature-pipeline version."""

    __tablename__ = "feature_pipeline_versions"
    __table_args__ = (
        UniqueConstraint("pipeline_key", "version_no", name="uq_feature_pipeline_version"),
        CheckConstraint("version_no > 0", name="ck_feature_pipeline_version_number"),
        CheckConstraint("length(artifact_uri) > 0", name="ck_feature_pipeline_artifact_uri"),
        Index("ix_feature_pipeline_universe", "universe_version_id", "created_at"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    pipeline_key: Mapped[str] = mapped_column(String(160), nullable=False)
    version_no: Mapped[int] = mapped_column(Integer, nullable=False)
    universe_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("market_universe_versions.id", ondelete="RESTRICT"), nullable=False
    )
    artifact_uri: Mapped[str] = mapped_column(Text, nullable=False)
    input_schema: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    output_schema: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    point_in_time_policy_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class AlphaModel(Base, TimestampMixin):
    """Stable Alpha identity; its versions and qualifications hold the evidence."""

    __tablename__ = "alpha_models"
    __table_args__ = (
        UniqueConstraint("alpha_key", name="uq_alpha_model_key"),
        CheckConstraint(
            "state IN ('RESEARCHING', 'QUALIFIED', 'PAPER_ACTIVE', 'LIVE_ACTIVE', "
            "'DEGRADING', 'SUSPENDED', 'RETIRED')",
            name="ck_alpha_model_state",
        ),
        Index("ix_alpha_model_owner_state", "owner_program_id", "state"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    alpha_key: Mapped[str] = mapped_column(String(160), nullable=False)
    name: Mapped[str] = mapped_column(String(240), nullable=False)
    family: Mapped[str] = mapped_column(String(120), nullable=False)
    description: Mapped[str] = mapped_column(Text, nullable=False)
    owner_program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="RESTRICT"), nullable=False
    )
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="RESEARCHING")
    # This pointer gains its FK when legacy Qualifications are backfilled.
    current_qualified_version_id: Mapped[UUID | None] = mapped_column(Uuid)


class AlphaModelVersion(Base):
    """Immutable Alpha implementation and its output contract."""

    __tablename__ = "alpha_model_versions"
    __table_args__ = (
        UniqueConstraint("alpha_model_id", "version_no", name="uq_alpha_model_version"),
        CheckConstraint("version_no > 0", name="ck_alpha_model_version_number"),
        CheckConstraint(
            "mode IN ('RELATIVE_SCORE', 'CALIBRATED_RETURN')",
            name="ck_alpha_model_version_mode",
        ),
        CheckConstraint(
            "state IN ('DRAFT', 'VALIDATED', 'REJECTED', 'RETIRED')",
            name="ck_alpha_model_version_state",
        ),
        CheckConstraint(
            "(source_mission_artifact_id IS NULL AND source_mission_artifact_revision IS NULL) "
            "OR (source_mission_artifact_id IS NOT NULL "
            "AND source_mission_artifact_revision IS NOT NULL "
            "AND source_mission_artifact_revision > 0)",
            name="ck_alpha_model_version_source_artifact",
        ),
        CheckConstraint("length(artifact_uri) > 0", name="ck_alpha_model_version_artifact_uri"),
        CheckConstraint("length(entrypoint) > 0", name="ck_alpha_model_version_entrypoint"),
        Index("ix_alpha_model_version_universe", "universe_version_id", "created_at"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    alpha_model_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("alpha_models.id", ondelete="CASCADE"), nullable=False
    )
    version_no: Mapped[int] = mapped_column(Integer, nullable=False)
    source_mission_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_missions.id", ondelete="RESTRICT"), nullable=False
    )
    # Nullable only for pre-0023 versions. New trusted versions freeze the exact
    # validated Mission Artifact as a normal foreign-key fact.
    source_mission_artifact_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("mission_artifacts.id", ondelete="RESTRICT")
    )
    source_mission_artifact_revision: Mapped[int | None] = mapped_column(Integer)
    universe_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("market_universe_versions.id", ondelete="RESTRICT"), nullable=False
    )
    feature_pipeline_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("feature_pipeline_versions.id", ondelete="RESTRICT")
    )
    horizon: Mapped[str] = mapped_column(String(100), nullable=False)
    mode: Mapped[str] = mapped_column(String(20), nullable=False)
    artifact_uri: Mapped[str] = mapped_column(Text, nullable=False)
    entrypoint: Mapped[str] = mapped_column(String(500), nullable=False)
    parameters: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    input_contract: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    output_contract: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="DRAFT")
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class AlphaSignalArtifact(Base):
    """A materialized AlphaSignalFrame tied to its model, Dataset, and runtime run."""

    __tablename__ = "alpha_signal_artifacts"
    __table_args__ = (
        UniqueConstraint(
            "alpha_model_version_id",
            "dataset_revision_id",
            "mode",
            name="uq_alpha_signal_artifact",
        ),
        CheckConstraint(
            "mode IN ('RELATIVE_SCORE', 'CALIBRATED_RETURN')",
            name="ck_alpha_signal_artifact_mode",
        ),
        CheckConstraint("row_count >= 0", name="ck_alpha_signal_artifact_row_count"),
        CheckConstraint("event_start <= event_end", name="ck_alpha_signal_artifact_event_range"),
        CheckConstraint(
            "available_start <= available_end",
            name="ck_alpha_signal_artifact_available_range",
        ),
        CheckConstraint(
            "event_start <= available_start AND event_end <= available_end",
            name="ck_alpha_signal_artifact_point_in_time",
        ),
        CheckConstraint(
            "run_id IS NOT NULL OR evaluation_result_id IS NOT NULL",
            name="ck_alpha_signal_artifact_provenance",
        ),
        CheckConstraint("length(artifact_uri) > 0", name="ck_alpha_signal_artifact_uri"),
        Index("ix_alpha_signal_artifact_run", "run_id"),
        Index(
            "uq_alpha_signal_artifact_evaluation_result",
            "evaluation_result_id",
            unique=True,
            sqlite_where=text("evaluation_result_id IS NOT NULL"),
            postgresql_where=text("evaluation_result_id IS NOT NULL"),
        ),
        UniqueConstraint(
            "id",
            "evaluation_result_id",
            name="uq_alpha_signal_artifact_result",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    alpha_model_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("alpha_model_versions.id", ondelete="RESTRICT"), nullable=False
    )
    dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("dataset_revisions.id", ondelete="RESTRICT"), nullable=False
    )
    run_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("quant_runtime_runs.id", ondelete="RESTRICT")
    )
    # Legacy runtime-backed artifacts remain readable. New sealed-evaluator
    # artifacts must point at the accepted typed result.
    evaluation_result_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("alpha_evaluation_results.id", ondelete="RESTRICT")
    )
    mode: Mapped[str] = mapped_column(String(20), nullable=False)
    artifact_uri: Mapped[str] = mapped_column(Text, nullable=False)
    row_count: Mapped[int] = mapped_column(BigInteger, nullable=False)
    event_start: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    event_end: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    available_start: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    available_end: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    schema_version: Mapped[str] = mapped_column(String(40), nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class AlphaCalibrationVersion(Base):
    """Immutable mapping from raw scores to calibrated economic quantities."""

    __tablename__ = "alpha_calibration_versions"
    __table_args__ = (
        UniqueConstraint(
            "alpha_model_version_id",
            "version_no",
            name="uq_alpha_calibration_version",
        ),
        UniqueConstraint(
            "id",
            "alpha_model_version_id",
            "source_discovery_evaluation_id",
            name="uq_alpha_calibration_version_discovery_chain",
        ),
        CheckConstraint("version_no > 0", name="ck_alpha_calibration_version_number"),
        CheckConstraint("length(trim(method)) > 0", name="ck_alpha_calibration_version_method"),
        CheckConstraint(
            "state IN ('DRAFT', 'VALIDATED', 'REJECTED', 'RETIRED')",
            name="ck_alpha_calibration_version_state",
        ),
        CheckConstraint(
            "(source_discovery_evaluation_id IS NULL "
            "AND training_dataset_revision_id IS NULL "
            "AND private_artifact_ref IS NULL "
            "AND artifact_uri IS NOT NULL AND length(artifact_uri) > 0) OR "
            "(source_discovery_evaluation_id IS NOT NULL "
            "AND training_dataset_revision_id IS NOT NULL "
            "AND private_artifact_ref IS NOT NULL "
            "AND artifact_uri IS NULL AND state = 'VALIDATED')",
            name="ck_alpha_calibration_version_trusted_provenance",
        ),
        ForeignKeyConstraint(
            ["source_discovery_evaluation_id", "training_dataset_revision_id"],
            [
                "alpha_discovery_evaluations.id",
                "alpha_discovery_evaluations.discovery_dataset_revision_id",
            ],
            name="fk_alpha_calibration_version_discovery_dataset",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["source_discovery_evaluation_id", "alpha_model_version_id"],
            [
                "alpha_discovery_evaluations.id",
                "alpha_discovery_evaluations.alpha_model_version_id",
            ],
            name="fk_alpha_calibration_version_discovery_model",
            ondelete="RESTRICT",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    alpha_model_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("alpha_model_versions.id", ondelete="CASCADE"), nullable=False
    )
    version_no: Mapped[int] = mapped_column(Integer, nullable=False)
    method: Mapped[str] = mapped_column(String(80), nullable=False)
    # Legacy JSON remains readable but is not accepted as trusted provenance.
    training_dataset_revision_ids: Mapped[list[str]] = mapped_column(
        JSON_VALUE, nullable=False, default=list
    )
    # Nullable only for new evaluator-private artifact references.
    artifact_uri: Mapped[str | None] = mapped_column(Text)
    source_discovery_evaluation_id: Mapped[UUID | None] = mapped_column(Uuid)
    training_dataset_revision_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("dataset_revisions.id", ondelete="RESTRICT")
    )
    private_artifact_ref: Mapped[UUID | None] = mapped_column(Uuid)
    parameters: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    metrics: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="DRAFT")
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class PromotionPolicyVersion(Base):
    """Immutable typed gate policy shared by sealed and handoff promotion."""

    __tablename__ = "promotion_policy_versions"
    __table_args__ = (
        UniqueConstraint("purpose", "version_no", name="uq_promotion_policy_version"),
        CheckConstraint("version_no > 0", name="ck_promotion_policy_version_number"),
        CheckConstraint(
            "purpose IN ('ALPHA_DISCOVERY_TO_SEALED', 'SEALED_TO_QUALIFIED', "
            "'PORTFOLIO_TO_PAPER', 'PAPER_TO_LIVE')",
            name="ck_promotion_policy_version_purpose",
        ),
        CheckConstraint(
            "mode IN ('MANUAL_APPROVAL', 'AUTO_HANDOFF')",
            name="ck_promotion_policy_version_mode",
        ),
        CheckConstraint(
            "state IN ('ACTIVE', 'RETIRED')",
            name="ck_promotion_policy_version_state",
        ),
        CheckConstraint(
            "policy_contract_version IS NULL OR policy_contract_version = 'PROMOTION_POLICY_V1'",
            name="ck_promotion_policy_version_contract",
        ),
        CheckConstraint(
            "(policy_contract_version IS NULL AND paper_connection_version_id IS NULL "
            "AND paper_feedback_contract_version_id IS NULL AND paper_preflight_receipt_id IS NULL "
            "AND live_connection_version_id IS NULL "
            "AND live_feedback_contract_version_id IS NULL AND live_preflight_receipt_id IS NULL "
            "AND paper_to_live_policy_version_id IS NULL AND "
            "((purpose IN ('ALPHA_DISCOVERY_TO_SEALED', 'SEALED_TO_QUALIFIED') "
            "AND paper_downstream_system_id IS NULL AND live_downstream_system_id IS NULL) OR "
            "(purpose = 'PORTFOLIO_TO_PAPER' "
            "AND paper_downstream_system_id IS NOT NULL AND live_downstream_system_id IS NULL) OR "
            "(purpose = 'PAPER_TO_LIVE' "
            "AND paper_downstream_system_id IS NOT NULL AND live_downstream_system_id IS NOT NULL))) OR "
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
            "AND paper_to_live_policy_version_id IS NULL)",
            name="ck_promotion_policy_version_tuples",
        ),
        ForeignKeyConstraint(
            [
                "paper_connection_version_id",
                "paper_downstream_system_id",
                "paper_feedback_contract_version_id",
            ],
            [
                "downstream_connection_versions.id",
                "downstream_connection_versions.downstream_system_id",
                "downstream_connection_versions.feedback_contract_version_id",
            ],
            name="fk_promotion_policy_paper_connection_tuple",
            ondelete="RESTRICT",
            use_alter=True,
        ),
        ForeignKeyConstraint(
            [
                "live_connection_version_id",
                "live_downstream_system_id",
                "live_feedback_contract_version_id",
            ],
            [
                "downstream_connection_versions.id",
                "downstream_connection_versions.downstream_system_id",
                "downstream_connection_versions.feedback_contract_version_id",
            ],
            name="fk_promotion_policy_live_connection_tuple",
            ondelete="RESTRICT",
            use_alter=True,
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    version_no: Mapped[int] = mapped_column(Integer, nullable=False)
    purpose: Mapped[str] = mapped_column(String(40), nullable=False)
    mode: Mapped[str] = mapped_column(String(40), nullable=False)
    policy_contract_version: Mapped[str | None] = mapped_column(String(80))
    paper_downstream_system_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("downstream_systems.id", ondelete="RESTRICT")
    )
    paper_connection_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    paper_feedback_contract_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    paper_preflight_receipt_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("preflight_receipts.id", ondelete="RESTRICT")
    )
    live_downstream_system_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("downstream_systems.id", ondelete="RESTRICT")
    )
    live_connection_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    live_feedback_contract_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    live_preflight_receipt_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("preflight_receipts.id", ondelete="RESTRICT")
    )
    paper_to_live_policy_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("promotion_policy_versions.id", ondelete="RESTRICT")
    )
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="ACTIVE")
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class PromotionPolicyGate(Base):
    """One required typed metric threshold within a Promotion Policy Version."""

    __tablename__ = "promotion_policy_gates"
    __table_args__ = (
        UniqueConstraint("policy_version_id", "ordinal", name="uq_promotion_policy_gate_ordinal"),
        CheckConstraint("ordinal > 0", name="ck_promotion_policy_gate_ordinal"),
        CheckConstraint("length(metric_code) > 0", name="ck_promotion_policy_gate_metric"),
        CheckConstraint(
            "comparator IN ('MINIMUM', 'MAXIMUM')",
            name="ck_promotion_policy_gate_comparator",
        ),
    )

    policy_version_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("promotion_policy_versions.id", ondelete="RESTRICT"),
        primary_key=True,
    )
    metric_code: Mapped[str] = mapped_column(String(100), primary_key=True)
    comparator: Mapped[str] = mapped_column(String(20), nullable=False)
    threshold: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    ordinal: Mapped[int] = mapped_column(Integer, nullable=False)


class AlphaDiscoveryEvaluation(Base):
    """Core-owned immutable discovery evidence for one validated Alpha proposal."""

    __tablename__ = "alpha_discovery_evaluations"
    __table_args__ = (
        UniqueConstraint(
            "source_mission_artifact_id",
            "cause_event_id",
            name="uq_alpha_discovery_evaluation_source_cause",
        ),
        UniqueConstraint(
            "id",
            "alpha_model_version_id",
            name="uq_alpha_discovery_evaluation_model",
        ),
        UniqueConstraint(
            "id",
            "discovery_dataset_revision_id",
            name="uq_alpha_discovery_evaluation_dataset",
        ),
        UniqueConstraint(
            "id",
            "evaluation_design_version_id",
            name="uq_alpha_discovery_evaluation_design",
        ),
        CheckConstraint(
            "source_mission_artifact_revision > 0",
            name="ck_alpha_discovery_evaluation_artifact_revision",
        ),
        CheckConstraint(
            "length(evaluator_contract_version) > 0",
            name="ck_alpha_discovery_evaluation_contract",
        ),
        CheckConstraint(
            "state IN ('FROZEN', 'QUEUED', 'RUNNING', 'VALID', "
            "'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_discovery_evaluation_state",
        ),
        CheckConstraint(
            "(state IN ('FROZEN', 'QUEUED', 'RUNNING') "
            "AND outcome_code IS NULL AND completed_at IS NULL) OR "
            "(state IN ('VALID', 'INCONCLUSIVE', 'INVALID') "
            "AND outcome_code IS NOT NULL AND length(outcome_code) > 0 "
            "AND completed_at IS NOT NULL)",
            name="ck_alpha_discovery_evaluation_completion",
        ),
        CheckConstraint(
            "(state IN ('FROZEN', 'QUEUED', 'RUNNING') "
            "AND private_result_ref IS NULL AND evaluated_at IS NULL) OR "
            "(state IN ('VALID', 'INCONCLUSIVE', 'INVALID') "
            "AND private_result_ref IS NOT NULL AND evaluated_at IS NOT NULL)",
            name="ck_alpha_discovery_evaluation_private_result",
        ),
        ForeignKeyConstraint(
            ["evaluation_dataset_selection_id", "discovery_dataset_revision_id"],
            [
                "evaluation_dataset_selections.id",
                "evaluation_dataset_selections.discovery_dataset_revision_id",
            ],
            name="fk_alpha_discovery_evaluation_selection_dataset",
            ondelete="RESTRICT",
        ),
        Index("ix_alpha_discovery_evaluation_program_state", "program_id", "state"),
        Index("ix_alpha_discovery_evaluation_mission", "mission_id", "created_at"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    source_mission_artifact_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("mission_artifacts.id", ondelete="RESTRICT"), nullable=False
    )
    source_mission_artifact_revision: Mapped[int] = mapped_column(Integer, nullable=False)
    alpha_model_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("alpha_model_versions.id", ondelete="RESTRICT"), nullable=False
    )
    program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="RESTRICT"), nullable=False
    )
    cycle_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_cycles.id", ondelete="RESTRICT"), nullable=False
    )
    branch_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_branches.id", ondelete="RESTRICT"), nullable=False
    )
    mission_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_missions.id", ondelete="RESTRICT"), nullable=False
    )
    discovery_dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("dataset_revisions.id", ondelete="RESTRICT"), nullable=False
    )
    evaluation_dataset_selection_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    evaluation_design_version_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("evaluation_design_versions.id", ondelete="RESTRICT"),
        nullable=False,
    )
    cause_event_id: Mapped[int] = mapped_column(
        IDENTITY_INT, ForeignKey("events.id", ondelete="RESTRICT"), nullable=False
    )
    evaluator_contract_version: Mapped[str] = mapped_column(String(40), nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="FROZEN")
    outcome_code: Mapped[str | None] = mapped_column(String(100))
    # UUID only: evaluator-private result material never enters Core as a URI or payload.
    private_result_ref: Mapped[UUID | None] = mapped_column(Uuid)
    evaluated_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    completed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class AlphaDiscoveryEvaluationMetric(Base):
    """One fixed aggregate emitted by the isolated Discovery evaluator."""

    __tablename__ = "alpha_discovery_evaluation_metrics"
    __table_args__ = (
        CheckConstraint(
            "metric_code IN ('OBSERVATION_COUNT', 'COVERAGE', 'IC_MEAN', "
            "'RANK_IC_MEAN', 'HIT_RATE', 'NET_RETURN', 'ANNUALIZED_VOLATILITY', "
            "'SHARPE_RATIO', 'MAX_DRAWDOWN', 'TRIAL_ADJUSTED_SHARPE')",
            name="ck_alpha_discovery_evaluation_metric_code",
        ),
        CheckConstraint(
            "status IN ('AVAILABLE', 'NOT_AVAILABLE')",
            name="ck_alpha_discovery_evaluation_metric_status",
        ),
        CheckConstraint(
            "(status = 'AVAILABLE' AND value IS NOT NULL "
            "AND lower(CAST(value AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')) OR "
            "(status = 'NOT_AVAILABLE' AND value IS NULL)",
            name="ck_alpha_discovery_evaluation_metric_value",
        ),
    )

    discovery_evaluation_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("alpha_discovery_evaluations.id", ondelete="RESTRICT"),
        primary_key=True,
    )
    metric_code: Mapped[str] = mapped_column(String(100), primary_key=True)
    value: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    status: Mapped[str] = mapped_column(String(20), nullable=False)


class AlphaDiscoveryEvaluationGate(Base):
    """One fixed categorical gate emitted by the isolated Discovery evaluator."""

    __tablename__ = "alpha_discovery_evaluation_gates"
    __table_args__ = (
        CheckConstraint(
            "gate_code IN ('EVIDENCE_VALID', 'POINT_IN_TIME_VALID', "
            "'CALIBRATION_VALID', 'STATISTICAL_VALID', 'POLICY_VALID')",
            name="ck_alpha_discovery_evaluation_gate_code",
        ),
        CheckConstraint(
            "status IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_discovery_evaluation_gate_status",
        ),
        CheckConstraint(
            "(status = 'PASS' AND reason_code IS NULL) OR "
            "(status <> 'PASS' AND reason_code IS NOT NULL AND length(reason_code) > 0)",
            name="ck_alpha_discovery_evaluation_gate_reason",
        ),
    )

    discovery_evaluation_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("alpha_discovery_evaluations.id", ondelete="RESTRICT"),
        primary_key=True,
    )
    gate_code: Mapped[str] = mapped_column(String(100), primary_key=True)
    status: Mapped[str] = mapped_column(String(20), nullable=False)
    reason_code: Mapped[str | None] = mapped_column(String(100))


class AlphaEvaluationAssignment(Base):
    """Core-owned immutable evaluator input; never a Mission or Job payload."""

    __tablename__ = "alpha_evaluation_assignments"
    __table_args__ = (
        UniqueConstraint(
            "source_mission_artifact_id",
            "cause_event_id",
            name="uq_alpha_evaluation_assignment_source_cause",
        ),
        UniqueConstraint(
            "alpha_model_version_id",
            "cycle_id",
            "assignment_no",
            name="uq_alpha_evaluation_assignment_number",
        ),
        UniqueConstraint(
            "discovery_evaluation_id",
            name="uq_alpha_evaluation_assignment_discovery_evaluation",
        ),
        CheckConstraint(
            "source_mission_artifact_revision > 0",
            name="ck_alpha_evaluation_assignment_artifact_revision",
        ),
        CheckConstraint(
            "assignment_no > 0",
            name="ck_alpha_evaluation_assignment_number",
        ),
        CheckConstraint(
            "length(evaluator_contract_version) > 0",
            name="ck_alpha_evaluation_assignment_contract",
        ),
        CheckConstraint(
            "state IN ('FROZEN', 'QUEUED', 'RUNNING', 'FINALIZED', 'INVALIDATED')",
            name="ck_alpha_evaluation_assignment_state",
        ),
        ForeignKeyConstraint(
            ["discovery_evaluation_id", "alpha_model_version_id"],
            [
                "alpha_discovery_evaluations.id",
                "alpha_discovery_evaluations.alpha_model_version_id",
            ],
            name="fk_alpha_evaluation_assignment_discovery_model",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["discovery_evaluation_id", "evaluation_design_version_id"],
            [
                "alpha_discovery_evaluations.id",
                "alpha_discovery_evaluations.evaluation_design_version_id",
            ],
            name="fk_alpha_evaluation_assignment_discovery_design",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            [
                "alpha_calibration_version_id",
                "alpha_model_version_id",
                "discovery_evaluation_id",
            ],
            [
                "alpha_calibration_versions.id",
                "alpha_calibration_versions.alpha_model_version_id",
                "alpha_calibration_versions.source_discovery_evaluation_id",
            ],
            name="fk_alpha_evaluation_assignment_calibration_chain",
            ondelete="RESTRICT",
        ),
        Index("ix_alpha_evaluation_assignment_program_state", "program_id", "state"),
        Index("ix_alpha_evaluation_assignment_mission", "mission_id", "created_at"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    source_mission_artifact_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("mission_artifacts.id", ondelete="RESTRICT"), nullable=False
    )
    source_mission_artifact_revision: Mapped[int] = mapped_column(Integer, nullable=False)
    discovery_evaluation_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("alpha_discovery_evaluations.id", ondelete="RESTRICT"),
        nullable=False,
    )
    program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="RESTRICT"), nullable=False
    )
    cycle_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_cycles.id", ondelete="RESTRICT"), nullable=False
    )
    branch_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_branches.id", ondelete="RESTRICT"), nullable=False
    )
    mission_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_missions.id", ondelete="RESTRICT"), nullable=False
    )
    alpha_model_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("alpha_model_versions.id", ondelete="RESTRICT"), nullable=False
    )
    alpha_calibration_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("alpha_calibration_versions.id", ondelete="RESTRICT")
    )
    universe_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("market_universe_versions.id", ondelete="RESTRICT"), nullable=False
    )
    sealed_dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("dataset_revisions.id", ondelete="RESTRICT"), nullable=False
    )
    evaluation_design_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("evaluation_design_versions.id", ondelete="RESTRICT"), nullable=False
    )
    promotion_policy_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("promotion_policy_versions.id", ondelete="RESTRICT"), nullable=False
    )
    cause_event_id: Mapped[int] = mapped_column(
        IDENTITY_INT, ForeignKey("events.id", ondelete="RESTRICT"), nullable=False
    )
    assignment_no: Mapped[int] = mapped_column(Integer, nullable=False)
    evaluator_contract_version: Mapped[str] = mapped_column(String(40), nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="FROZEN")
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class AlphaEvaluationAssignmentDatasetRevision(Base):
    """A frozen ordered Dataset Revision reference for an Alpha Assignment."""

    __tablename__ = "alpha_evaluation_assignment_dataset_revisions"
    __table_args__ = (
        UniqueConstraint(
            "assignment_id",
            "dataset_revision_id",
            name="uq_alpha_evaluation_assignment_dataset_revision",
        ),
        CheckConstraint(
            "phase IN ('DISCOVERY', 'VALIDATION', 'SEALED')",
            name="ck_alpha_evaluation_assignment_dataset_phase",
        ),
        CheckConstraint(
            "ordinal > 0",
            name="ck_alpha_evaluation_assignment_dataset_ordinal",
        ),
    )

    assignment_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("alpha_evaluation_assignments.id", ondelete="RESTRICT"),
        primary_key=True,
    )
    phase: Mapped[str] = mapped_column(String(20), primary_key=True)
    ordinal: Mapped[int] = mapped_column(Integer, primary_key=True)
    dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("dataset_revisions.id", ondelete="RESTRICT"), nullable=False
    )


class AlphaEvaluationEpisode(Base, TimestampMixin):
    """Alpha-specific evidence and gate result; the legacy episode table remains readable."""

    __tablename__ = "alpha_evaluation_episodes"
    __table_args__ = (
        CheckConstraint(
            "state IN ('PENDING', 'RUNNING', 'COMPLETED', 'FAILED', 'CANCELLED', "
            "'PLANNED', 'SEALED', 'ASSIGNED', 'EVALUATING', 'EVALUATED', "
            "'DISCLOSED', 'CONSUMED', 'INVALIDATED')",
            name="ck_alpha_evaluation_episode_state",
        ),
        CheckConstraint(
            "result IS NULL OR result IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_evaluation_episode_result",
        ),
        Index("ix_alpha_evaluation_episode_program", "program_id", "state"),
        Index("ix_alpha_evaluation_episode_model", "alpha_model_version_id", "created_at"),
        Index(
            "uq_alpha_evaluation_episode_assignment",
            "assignment_id",
            unique=True,
            sqlite_where=text("assignment_id IS NOT NULL"),
            postgresql_where=text("assignment_id IS NOT NULL"),
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="CASCADE"), nullable=False
    )
    branch_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_branches.id", ondelete="CASCADE"), nullable=False
    )
    alpha_model_version_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("alpha_model_versions.id", ondelete="RESTRICT"), nullable=False
    )
    # Nullable only for pre-0023 rows. New trusted episodes are one-to-one with
    # a Core-owned Assignment rather than using the legacy free-form run fields.
    assignment_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("alpha_evaluation_assignments.id", ondelete="RESTRICT")
    )
    discovery_run_ids: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    validation_run_ids: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    sealed_run_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("quant_runtime_runs.id", ondelete="SET NULL")
    )
    sealed_dataset_revision_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("dataset_revisions.id", ondelete="RESTRICT"), nullable=False
    )
    # Promotion Policy is introduced by the later promotion slice.
    promotion_policy_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="PENDING")
    result: Mapped[str | None] = mapped_column(String(40))
    gate_results: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    multiple_testing_summary: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    disclosure: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    sealed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    evaluated_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    disclosed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    consumed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    invalid_reason: Mapped[str | None] = mapped_column(String(100))


class AlphaEvaluationResult(Base):
    """Typed aggregate result accepted from the isolated sealed evaluator."""

    __tablename__ = "alpha_evaluation_results"
    __table_args__ = (
        UniqueConstraint("episode_id", name="uq_alpha_evaluation_result_episode"),
        CheckConstraint(
            "evidence_validity IN ('VALID', 'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_evaluation_result_validity",
        ),
        CheckConstraint(
            "result IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_evaluation_result_result",
        ),
        CheckConstraint(
            "(evidence_validity = 'VALID' AND result IN ('PASS', 'FAIL')) OR "
            "(evidence_validity = 'INCONCLUSIVE' AND result = 'INCONCLUSIVE') OR "
            "(evidence_validity = 'INVALID' AND result = 'INVALID')",
            name="ck_alpha_evaluation_result_consistency",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    episode_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("alpha_evaluation_episodes.id", ondelete="RESTRICT"), nullable=False
    )
    evidence_validity: Mapped[str] = mapped_column(String(20), nullable=False)
    result: Mapped[str] = mapped_column(String(20), nullable=False)
    private_result_ref: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    evaluated_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class AlphaEvaluationForecast(Base):
    """One bounded expected-return forecast that a trusted Alpha result may expose to Portfolio."""

    __tablename__ = "alpha_evaluation_forecasts"
    __table_args__ = (
        CheckConstraint("length(trim(instrument_id)) > 0", name="ck_alpha_forecast_instrument"),
        CheckConstraint(
            "as_of_time <= effective_from AND "
            "(effective_until IS NULL OR effective_until >= effective_from)",
            name="ck_alpha_forecast_time_order",
        ),
        CheckConstraint(
            "lower(CAST(expected_return AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_alpha_forecast_expected_return_finite",
        ),
        CheckConstraint(
            "uncertainty >= 0 AND lower(CAST(uncertainty AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_alpha_forecast_uncertainty",
        ),
        CheckConstraint(
            "confidence >= 0 AND confidence <= 1 AND lower(CAST(confidence AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_alpha_forecast_confidence",
        ),
        CheckConstraint(
            "max_trade_notional > 0 AND lower(CAST(max_trade_notional AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_alpha_forecast_max_trade_notional",
        ),
        CheckConstraint(
            "max_position_notional > 0 AND lower(CAST(max_position_notional AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_alpha_forecast_max_position_notional",
        ),
        CheckConstraint(
            "max_participation_rate >= 0 AND max_participation_rate <= 1 "
            "AND lower(CAST(max_participation_rate AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_alpha_forecast_max_participation_rate",
        ),
        CheckConstraint(
            "days_to_liquidate > 0 AND lower(CAST(days_to_liquidate AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_alpha_forecast_days_to_liquidate",
        ),
        CheckConstraint(
            "stressed_capacity_notional > 0 AND "
            "lower(CAST(stressed_capacity_notional AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_alpha_forecast_stressed_capacity_notional",
        ),
        ForeignKeyConstraint(
            ["signal_artifact_id", "result_id"],
            [
                "alpha_signal_artifacts.id",
                "alpha_signal_artifacts.evaluation_result_id",
            ],
            name="fk_alpha_evaluation_forecast_signal_result",
            ondelete="RESTRICT",
        ),
    )

    result_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("alpha_evaluation_results.id", ondelete="RESTRICT"),
        primary_key=True,
    )
    signal_artifact_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    instrument_id: Mapped[str] = mapped_column(String(200), primary_key=True)
    as_of_time: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    effective_from: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    effective_until: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    expected_return: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    uncertainty: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    confidence: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    max_trade_notional: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    max_position_notional: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    max_participation_rate: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    days_to_liquidate: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    stressed_capacity_notional: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)


class AlphaEvaluationMetric(Base):
    """One bounded aggregate metric from a typed Alpha evaluator result."""

    __tablename__ = "alpha_evaluation_metrics"
    __table_args__ = (
        CheckConstraint("length(metric_code) > 0", name="ck_alpha_evaluation_metric_code"),
        CheckConstraint(
            "phase IN ('DISCOVERY', 'VALIDATION', 'SEALED')",
            name="ck_alpha_evaluation_metric_phase",
        ),
        CheckConstraint(
            "status IN ('AVAILABLE', 'NOT_AVAILABLE')",
            name="ck_alpha_evaluation_metric_status",
        ),
        CheckConstraint(
            "(status = 'AVAILABLE' AND value IS NOT NULL) OR "
            "(status = 'NOT_AVAILABLE' AND value IS NULL)",
            name="ck_alpha_evaluation_metric_value",
        ),
    )

    result_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("alpha_evaluation_results.id", ondelete="RESTRICT"),
        primary_key=True,
    )
    metric_code: Mapped[str] = mapped_column(String(100), primary_key=True)
    phase: Mapped[str] = mapped_column(String(20), primary_key=True)
    value: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    status: Mapped[str] = mapped_column(String(20), nullable=False)


class AlphaEvaluationGate(Base):
    """One deterministic policy gate evaluation for an Alpha evaluator result."""

    __tablename__ = "alpha_evaluation_gates"
    __table_args__ = (
        CheckConstraint("length(gate_code) > 0", name="ck_alpha_evaluation_gate_code"),
        CheckConstraint(
            "status IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_alpha_evaluation_gate_status",
        ),
        CheckConstraint(
            "(status = 'PASS' AND reason_code IS NULL) OR "
            "(status <> 'PASS' AND reason_code IS NOT NULL AND length(reason_code) > 0)",
            name="ck_alpha_evaluation_gate_reason",
        ),
    )

    result_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("alpha_evaluation_results.id", ondelete="RESTRICT"),
        primary_key=True,
    )
    gate_code: Mapped[str] = mapped_column(String(100), primary_key=True)
    status: Mapped[str] = mapped_column(String(20), nullable=False)
    reason_code: Mapped[str | None] = mapped_column(String(100))


class EvidenceExposure(Base):
    """An immutable record of the maximum evaluation evidence exposed by lineage."""

    __tablename__ = "evidence_exposures"
    __table_args__ = (
        UniqueConstraint(
            "episode_id",
            "subject_type",
            "subject_id",
            "level",
            name="uq_evidence_exposure_subject_level",
        ),
        CheckConstraint(
            "subject_type IN ('PROGRAM', 'BRANCH', 'MISSION', 'ALPHA_MODEL', "
            "'ALPHA_QUALIFICATION')",
            name="ck_evidence_exposure_subject_type",
        ),
        CheckConstraint("level BETWEEN 1 AND 3", name="ck_evidence_exposure_level"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    episode_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("alpha_evaluation_episodes.id", ondelete="RESTRICT"), nullable=False
    )
    subject_type: Mapped[str] = mapped_column(String(40), nullable=False)
    subject_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    level: Mapped[int] = mapped_column(Integer, nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class Disclosure(Base):
    """Deterministic audience-specific classification, never evaluator raw output."""

    __tablename__ = "disclosures"
    __table_args__ = (
        UniqueConstraint("episode_id", "audience", "level", name="uq_disclosure_audience_level"),
        CheckConstraint(
            "(audience = 'CODEX' AND level = 1) OR "
            "(audience = 'OPERATOR' AND level = 2) OR "
            "(audience = 'POSTMORTEM' AND level = 3)",
            name="ck_disclosure_audience_level",
        ),
        CheckConstraint("length(classification_code) > 0", name="ck_disclosure_classification"),
        CheckConstraint(
            "(classification_code = 'QUALIFIED' AND reason_code IS NULL) OR "
            "(classification_code <> 'QUALIFIED' AND reason_code IS NOT NULL "
            "AND length(reason_code) > 0)",
            name="ck_disclosure_reason",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    episode_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("alpha_evaluation_episodes.id", ondelete="RESTRICT"), nullable=False
    )
    audience: Mapped[str] = mapped_column(String(20), nullable=False)
    level: Mapped[int] = mapped_column(Integer, nullable=False)
    classification_code: Mapped[str] = mapped_column(String(100), nullable=False)
    reason_code: Mapped[str | None] = mapped_column(String(100))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class AlphaQualification(Base, TimestampMixin):
    __tablename__ = "alpha_qualifications"
    __table_args__ = (
        Index("ix_alpha_qualification_state", "state"),
        Index(
            "uq_alpha_qualification_scope",
            "alpha_model_version_id",
            "universe_version_id",
            "horizon",
            "role",
            unique=True,
            sqlite_where=text("alpha_model_id IS NOT NULL"),
            postgresql_where=text("alpha_model_id IS NOT NULL"),
        ),
        Index(
            "uq_alpha_qualification_evaluation_result",
            "evaluation_result_id",
            unique=True,
            sqlite_where=text("evaluation_result_id IS NOT NULL"),
            postgresql_where=text("evaluation_result_id IS NOT NULL"),
        ),
        UniqueConstraint(
            "id",
            "evaluation_result_id",
            name="uq_alpha_qualification_evaluation_result_pair",
        ),
        CheckConstraint(
            "alpha_model_id IS NULL OR role IN "
            "('PRIMARY_ALPHA', 'DIVERSIFIER_ALPHA', 'HEDGE_ALPHA', "
            "'REGIME_SIGNAL', 'RISK_MODULATOR', 'SHADOW_ALPHA') OR "
            "(role = 'RISK_SIGNAL' AND evaluation_result_id IS NULL)",
            name="ck_alpha_qualification_canonical_role",
        ),
        CheckConstraint(
            "alpha_model_id IS NULL OR state IN "
            "('ACTIVE', 'WATCH', 'QUARANTINED', 'RETIRED', 'SHADOW')",
            name="ck_alpha_qualification_canonical_state",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="SET NULL")
    )
    # Nullable while existing qualifications are backfilled to AlphaModel identities.
    alpha_model_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("alpha_models.id", ondelete="RESTRICT")
    )
    # Existing version/evaluation columns remain readable until their later FK backfill.
    alpha_model_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    calibration_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    universe_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("market_universe_versions.id", ondelete="RESTRICT")
    )
    universe: Mapped[str | None] = mapped_column(String(200))
    horizon: Mapped[str | None] = mapped_column(String(100))
    role: Mapped[str] = mapped_column(String(100), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")
    name: Mapped[str | None] = mapped_column(String(240))
    scope_json: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    evaluation_episode_id: Mapped[UUID | None] = mapped_column(Uuid)
    # Legacy episode IDs remain readable; production qualifications link to the
    # typed sealed-evaluator result that justified them.
    evaluation_result_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("alpha_evaluation_results.id", ondelete="RESTRICT")
    )
    degradation_state: Mapped[str] = mapped_column(String(40), nullable=False, default="HEALTHY")
    metrics: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    qualification_metrics = synonym("metrics")
    lineage: Mapped[list[dict[str, Any]]] = mapped_column(JSON_VALUE, nullable=False, default=list)


class PortfolioMandate(Base, TimestampMixin):
    __tablename__ = "portfolio_mandates"
    __table_args__ = (UniqueConstraint("key", name="uq_portfolio_mandate_key"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    key: Mapped[str] = mapped_column(String(120), nullable=False)
    name: Mapped[str] = mapped_column(String(200), nullable=False)
    enabled: Mapped[bool] = mapped_column(Boolean, nullable=False, default=True)
    latest_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False, default=uuid4)
    spec_json: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")


class PortfolioMandateVersion(Base):
    """Immutable investment-policy inputs for one Portfolio Mandate revision."""

    __tablename__ = "portfolio_mandate_versions"
    __table_args__ = (
        UniqueConstraint(
            "portfolio_mandate_id", "version_no", name="uq_portfolio_mandate_version"
        ),
        CheckConstraint("version_no > 0", name="ck_portfolio_mandate_version_number"),
        CheckConstraint(
            "minimum_alpha_count >= 2", name="ck_portfolio_mandate_minimum_alphas"
        ),
        CheckConstraint(
            "(policy_family IS NULL AND universe_version_id IS NULL "
            "AND eligible_alpha_role IS NULL AND minimum_weight IS NULL "
            "AND maximum_weight IS NULL AND gross_exposure_limit IS NULL "
            "AND net_exposure_target IS NULL AND cash_reserve IS NULL "
            "AND turnover_limit IS NULL AND variance_limit IS NULL "
            "AND risk_aversion IS NULL AND cost_aversion IS NULL "
            "AND uncertainty_aversion IS NULL AND commission_rate IS NULL "
            "AND half_spread_rate IS NULL AND slippage_rate IS NULL "
            "AND impact_rate IS NULL AND impact_breakpoint IS NULL AND state IS NULL) "
            "OR (policy_family = 'LONG_ONLY_MEAN_VARIANCE_V1' "
            "AND objective = 'MAXIMIZE_NET_RETURN' "
            "AND universe_version_id IS NOT NULL "
            "AND eligible_alpha_role = 'PRIMARY_ALPHA' "
            "AND minimum_weight IS NOT NULL AND maximum_weight IS NOT NULL "
            "AND gross_exposure_limit IS NOT NULL AND net_exposure_target IS NOT NULL "
            "AND cash_reserve IS NOT NULL AND turnover_limit IS NOT NULL "
            "AND variance_limit IS NOT NULL AND risk_aversion IS NOT NULL "
            "AND cost_aversion IS NOT NULL AND uncertainty_aversion IS NOT NULL "
            "AND commission_rate IS NOT NULL AND half_spread_rate IS NOT NULL "
            "AND slippage_rate IS NOT NULL AND impact_rate IS NOT NULL "
            "AND impact_breakpoint IS NOT NULL AND state IN ('ACTIVE', 'RETIRED') "
            "AND minimum_weight >= 0 AND maximum_weight > 0 AND maximum_weight <= 1 "
            "AND minimum_weight <= maximum_weight "
            "AND minimum_weight * minimum_alpha_count <= 1 "
            "AND maximum_weight * minimum_alpha_count >= 1 "
            "AND gross_exposure_limit = 1 AND net_exposure_target = 1 "
            "AND cash_reserve = 0 AND turnover_limit >= 1 AND turnover_limit <= 2 "
            "AND variance_limit > 0 AND risk_aversion >= 0 AND cost_aversion >= 0 "
            "AND uncertainty_aversion >= 0 AND commission_rate >= 0 AND commission_rate <= 1 "
            "AND half_spread_rate >= 0 AND half_spread_rate <= 1 "
            "AND slippage_rate >= 0 AND slippage_rate <= 1 "
            "AND impact_rate >= 0 AND impact_rate <= 1 "
            "AND impact_breakpoint >= 0 AND impact_breakpoint <= 1)",
            name="ck_portfolio_mandate_v1_complete",
        ),
        Index("ix_portfolio_mandate_version_mandate", "portfolio_mandate_id", "version_no"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    portfolio_mandate_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("portfolio_mandates.id", ondelete="RESTRICT"), nullable=False
    )
    version_no: Mapped[int] = mapped_column(Integer, nullable=False)
    base_currency: Mapped[str] = mapped_column(String(20), nullable=False)
    objective: Mapped[str] = mapped_column(String(80), nullable=False)
    # Legacy JSON rows have no policy family and stay unavailable to production.
    policy_family: Mapped[str | None] = mapped_column(String(40))
    universe_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("market_universe_versions.id", ondelete="RESTRICT")
    )
    eligible_alpha_role: Mapped[str | None] = mapped_column(String(40))
    eligible_alpha_roles: Mapped[list[str]] = mapped_column(
        JSON_VALUE, nullable=False, default=list
    )
    eligible_universe_version_ids: Mapped[list[str]] = mapped_column(
        JSON_VALUE, nullable=False, default=list
    )
    minimum_alpha_count: Mapped[int] = mapped_column(Integer, nullable=False, default=2)
    minimum_weight: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    maximum_weight: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    gross_exposure_limit: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    net_exposure_target: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    cash_reserve: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    turnover_limit: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    variance_limit: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    risk_aversion: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    cost_aversion: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    uncertainty_aversion: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    commission_rate: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    half_spread_rate: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    slippage_rate: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    impact_rate: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    impact_breakpoint: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    state: Mapped[str | None] = mapped_column(String(20))
    capital_config: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    risk_config: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    cost_config: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    capacity_config: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    promotion_policy: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    constraint_config: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class PortfolioProgram(Base, TimestampMixin):
    __tablename__ = "portfolio_programs"
    __table_args__ = (
        UniqueConstraint("mandate_version_id", name="uq_portfolio_program_mandate_version"),
        # This pair is intentionally explicit: Portfolio input assignments bind a
        # Program to the exact Mandate version without consulting a mutable pointer.
        UniqueConstraint(
            "id", "mandate_version_id", name="uq_portfolio_program_mandate_pair"
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    mandate_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    mandate_name: Mapped[str | None] = mapped_column(String(200))
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")
    current_candidate_id: Mapped[UUID | None] = mapped_column(Uuid)


class PortfolioCandidateFamily(Base):
    """One stable comparison lineage for each V1 Portfolio Program."""

    __tablename__ = "portfolio_candidate_families"
    __table_args__ = (
        UniqueConstraint("portfolio_program_id", name="uq_portfolio_candidate_family_program"),
        UniqueConstraint(
            "id",
            "portfolio_program_id",
            "mandate_version_id",
            name="uq_portfolio_candidate_family_lineage_pair",
        ),
        ForeignKeyConstraint(
            ["portfolio_program_id", "mandate_version_id"],
            ["portfolio_programs.id", "portfolio_programs.mandate_version_id"],
            name="fk_portfolio_candidate_family_program_mandate",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["mandate_version_id"],
            ["portfolio_mandate_versions.id"],
            name="fk_portfolio_candidate_family_mandate",
            ondelete="RESTRICT",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    portfolio_program_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    mandate_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class PortfolioInputEvaluationAssignment(Base):
    """The sole durable, typed covariance-evaluator descriptor."""

    __tablename__ = "portfolio_input_evaluation_assignments"
    __table_args__ = (
        UniqueConstraint(
            "portfolio_program_id",
            "cause_event_id",
            name="uq_portfolio_input_evaluation_assignment_cause",
        ),
        CheckConstraint(
            "evaluator_contract_version = 'PORTFOLIO_INPUT_EVALUATION_V1'",
            name="ck_portfolio_input_evaluation_assignment_contract",
        ),
        CheckConstraint(
            "(state IN ('FROZEN', 'QUEUED', 'RUNNING') "
            "AND private_result_ref IS NULL AND evaluated_at IS NULL "
            "AND outcome_code IS NULL AND completed_at IS NULL) OR "
            "(state IN ('VALID', 'INCONCLUSIVE', 'INVALID') "
            "AND private_result_ref IS NOT NULL AND evaluated_at IS NOT NULL "
            "AND outcome_code IS NOT NULL AND length(trim(outcome_code)) > 0 "
            "AND completed_at IS NOT NULL)",
            name="ck_portfolio_input_evaluation_assignment_state",
        ),
        Index(
            "uq_portfolio_initial_input_assignment_active",
            "portfolio_program_id",
            unique=True,
            sqlite_where=text(
                "previous_candidate_id IS NULL AND state IN ('FROZEN', 'QUEUED', 'RUNNING')"
            ),
            postgresql_where=text(
                "previous_candidate_id IS NULL AND state IN ('FROZEN', 'QUEUED', 'RUNNING')"
            ),
        ),
        ForeignKeyConstraint(
            ["portfolio_program_id", "mandate_version_id"],
            ["portfolio_programs.id", "portfolio_programs.mandate_version_id"],
            name="fk_portfolio_input_evaluation_assignment_program_mandate",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["mandate_version_id"],
            ["portfolio_mandate_versions.id"],
            name="fk_portfolio_input_evaluation_assignment_mandate",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["capital_context_version_id"],
            ["capital_context_versions.id"],
            name="fk_portfolio_input_evaluation_assignment_capital",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["evaluation_dataset_selection_id", "sealed_dataset_revision_id"],
            [
                "evaluation_dataset_selections.id",
                "evaluation_dataset_selections.sealed_dataset_revision_id",
            ],
            name="fk_portfolio_input_evaluation_assignment_selection_sealed",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["promotion_policy_version_id"],
            ["promotion_policy_versions.id"],
            name="fk_portfolio_input_evaluation_assignment_policy",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["cause_event_id"], ["events.id"], name="fk_portfolio_input_evaluation_assignment_event", ondelete="RESTRICT"
        ),
        ForeignKeyConstraint(
            ["previous_candidate_id", "portfolio_program_id"],
            ["portfolio_candidates.id", "portfolio_candidates.portfolio_program_id"],
            name="fk_portfolio_input_evaluation_assignment_predecessor",
            ondelete="RESTRICT",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    portfolio_program_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    mandate_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    capital_context_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    evaluation_dataset_selection_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    sealed_dataset_revision_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    promotion_policy_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    cause_event_id: Mapped[int] = mapped_column(IDENTITY_INT, nullable=False)
    previous_candidate_id: Mapped[UUID | None] = mapped_column(Uuid)
    as_of_time: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    evaluator_contract_version: Mapped[str] = mapped_column(String(80), nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="FROZEN")
    private_result_ref: Mapped[UUID | None] = mapped_column(Uuid)
    evaluated_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    outcome_code: Mapped[str | None] = mapped_column(String(100))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    completed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class PortfolioInputEvaluationAssignmentMember(Base):
    __tablename__ = "portfolio_input_evaluation_assignment_members"
    __table_args__ = (
        UniqueConstraint(
            "assignment_id", "alpha_qualification_id", name="uq_portfolio_input_assignment_member_qualification"
        ),
        UniqueConstraint(
            "assignment_id", "instrument_id", name="uq_portfolio_input_assignment_member_instrument"
        ),
        CheckConstraint("axis_index >= 0", name="ck_portfolio_input_assignment_member_axis"),
        ForeignKeyConstraint(
            ["assignment_id"],
            ["portfolio_input_evaluation_assignments.id"],
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["alpha_qualification_id", "alpha_evaluation_result_id"],
            ["alpha_qualifications.id", "alpha_qualifications.evaluation_result_id"],
            name="fk_portfolio_input_assignment_member_qualification_result",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["alpha_signal_artifact_id", "alpha_evaluation_result_id"],
            ["alpha_signal_artifacts.id", "alpha_signal_artifacts.evaluation_result_id"],
            name="fk_portfolio_input_assignment_member_signal_result",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["alpha_evaluation_result_id", "instrument_id"],
            ["alpha_evaluation_forecasts.result_id", "alpha_evaluation_forecasts.instrument_id"],
            name="fk_portfolio_input_assignment_member_forecast",
            ondelete="RESTRICT",
        ),
    )

    assignment_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    axis_index: Mapped[int] = mapped_column(Integer, primary_key=True)
    alpha_qualification_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    alpha_evaluation_result_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    alpha_signal_artifact_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    instrument_id: Mapped[str] = mapped_column(String(200), nullable=False)


class PortfolioAssemblyInput(Base):
    """Complete-only relational optimizer input; never a partial work record."""

    __tablename__ = "portfolio_assembly_inputs"
    __table_args__ = (
        UniqueConstraint(
            "portfolio_input_evaluation_assignment_id",
            name="uq_portfolio_assembly_input_assignment",
        ),
        UniqueConstraint(
            "portfolio_program_id", "snapshot_no", name="uq_portfolio_assembly_input_snapshot"
        ),
        UniqueConstraint(
            "portfolio_program_id", "cause_event_id", name="uq_portfolio_assembly_input_cause"
        ),
        UniqueConstraint(
            "id",
            "portfolio_program_id",
            "mandate_version_id",
            "capital_context_version_id",
            "universe_version_id",
            name="uq_portfolio_assembly_input_candidate_source",
        ),
        Index(
            "uq_portfolio_assembly_input_pending_program",
            "portfolio_program_id",
            unique=True,
            sqlite_where=text("state = 'PENDING'"),
            postgresql_where=text("state = 'PENDING'"),
        ),
        CheckConstraint("snapshot_no > 0", name="ck_portfolio_assembly_input_snapshot"),
        CheckConstraint(
            "input_contract_version = 'LONG_ONLY_MEAN_VARIANCE_V1'",
            name="ck_portfolio_assembly_input_contract",
        ),
        CheckConstraint(
            "as_of_time <= effective_from AND "
            "(effective_until IS NULL OR effective_until >= effective_from)",
            name="ck_portfolio_assembly_input_time",
        ),
        CheckConstraint(
            "length(trim(covariance_method)) > 0 AND covariance_observations >= 2 "
            "AND covariance_decay > 0 AND covariance_decay < 1 "
            "AND covariance_shrinkage >= 0 AND covariance_shrinkage <= 1 "
            "AND lower(CAST(covariance_decay AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(covariance_shrinkage AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_portfolio_assembly_input_covariance_metadata",
        ),
        CheckConstraint(
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
            "AND impact_breakpoint >= 0 AND impact_breakpoint <= 1 "
            "AND lower(CAST(minimum_weight AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(maximum_weight AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(gross_exposure_limit AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(net_exposure_target AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(cash_reserve AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(turnover_limit AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(variance_limit AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(risk_aversion AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(cost_aversion AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(uncertainty_aversion AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(commission_rate AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(half_spread_rate AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(slippage_rate AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(impact_rate AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(impact_breakpoint AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_portfolio_assembly_input_v1_scalars",
        ),
        CheckConstraint(
            "(state = 'PENDING' AND outcome_code IS NULL AND completed_at IS NULL) OR "
            "(state IN ('ASSEMBLED', 'INFEASIBLE', 'STALE', 'INVALID') "
            "AND outcome_code IS NOT NULL AND length(trim(outcome_code)) > 0 "
            "AND completed_at IS NOT NULL)",
            name="ck_portfolio_assembly_input_state",
        ),
        ForeignKeyConstraint(
            ["portfolio_input_evaluation_assignment_id"],
            ["portfolio_input_evaluation_assignments.id"],
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["portfolio_program_id", "mandate_version_id"],
            ["portfolio_programs.id", "portfolio_programs.mandate_version_id"],
            name="fk_portfolio_assembly_input_program_mandate",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["mandate_version_id"],
            ["portfolio_mandate_versions.id"],
            name="fk_portfolio_assembly_input_mandate",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["capital_context_version_id"],
            ["capital_context_versions.id"],
            name="fk_portfolio_assembly_input_capital",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["universe_version_id"],
            ["market_universe_versions.id"],
            name="fk_portfolio_assembly_input_universe",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["promotion_policy_version_id"],
            ["promotion_policy_versions.id"],
            name="fk_portfolio_assembly_input_policy",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["cause_event_id"], ["events.id"], name="fk_portfolio_assembly_input_event", ondelete="RESTRICT"
        ),
        ForeignKeyConstraint(
            ["previous_candidate_id", "portfolio_program_id"],
            ["portfolio_candidates.id", "portfolio_candidates.portfolio_program_id"],
            name="fk_portfolio_assembly_input_predecessor",
            ondelete="RESTRICT",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    portfolio_input_evaluation_assignment_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    portfolio_program_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    mandate_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    capital_context_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    universe_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    promotion_policy_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    cause_event_id: Mapped[int] = mapped_column(IDENTITY_INT, nullable=False)
    # The causal Event ID is the immutable V1 snapshot number; no latest-row lookup.
    snapshot_no: Mapped[int] = mapped_column(BigInteger, nullable=False)
    input_contract_version: Mapped[str] = mapped_column(String(80), nullable=False)
    as_of_time: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    effective_from: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    effective_until: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    previous_candidate_id: Mapped[UUID | None] = mapped_column(Uuid)
    covariance_method: Mapped[str] = mapped_column(String(80), nullable=False)
    covariance_observations: Mapped[int] = mapped_column(Integer, nullable=False)
    covariance_decay: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    covariance_shrinkage: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    minimum_alpha_count: Mapped[int] = mapped_column(Integer, nullable=False)
    minimum_weight: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    maximum_weight: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    gross_exposure_limit: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    net_exposure_target: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    cash_reserve: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    turnover_limit: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    variance_limit: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    risk_aversion: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    cost_aversion: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    uncertainty_aversion: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    commission_rate: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    half_spread_rate: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    slippage_rate: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    impact_rate: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    impact_breakpoint: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="PENDING")
    outcome_code: Mapped[str | None] = mapped_column(String(100))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    completed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class PortfolioAssemblyInputMember(Base):
    __tablename__ = "portfolio_assembly_input_members"
    __table_args__ = (
        UniqueConstraint(
            "input_id", "alpha_qualification_id", name="uq_portfolio_assembly_input_member_qualification"
        ),
        UniqueConstraint(
            "input_id", "instrument_id", name="uq_portfolio_assembly_input_member_instrument"
        ),
        CheckConstraint("axis_index >= 0", name="ck_portfolio_assembly_input_member_axis"),
        CheckConstraint(
            "lower(CAST(expected_return AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_portfolio_assembly_input_member_return",
        ),
        CheckConstraint(
            "uncertainty >= 0 AND lower(CAST(uncertainty AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_portfolio_assembly_input_member_uncertainty",
        ),
        CheckConstraint(
            "confidence >= 0 AND confidence <= 1 AND previous_weight >= 0 "
            "AND previous_weight <= 1 AND lower(CAST(confidence AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(previous_weight AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_portfolio_assembly_input_member_weight_bounds",
        ),
        CheckConstraint(
            "max_trade_notional > 0 AND max_position_notional > 0 "
            "AND max_participation_rate >= 0 AND max_participation_rate <= 1 "
            "AND days_to_liquidate > 0 AND stressed_capacity > 0 "
            "AND lower(CAST(max_trade_notional AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(max_position_notional AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(max_participation_rate AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(days_to_liquidate AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity') "
            "AND lower(CAST(stressed_capacity AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_portfolio_assembly_input_member_capacity",
        ),
        ForeignKeyConstraint(
            ["input_id"], ["portfolio_assembly_inputs.id"], ondelete="RESTRICT"
        ),
        ForeignKeyConstraint(
            ["alpha_qualification_id", "alpha_evaluation_result_id"],
            ["alpha_qualifications.id", "alpha_qualifications.evaluation_result_id"],
            name="fk_portfolio_assembly_input_member_qualification_result",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["alpha_signal_artifact_id", "alpha_evaluation_result_id"],
            ["alpha_signal_artifacts.id", "alpha_signal_artifacts.evaluation_result_id"],
            name="fk_portfolio_assembly_input_member_signal_result",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["alpha_evaluation_result_id", "instrument_id"],
            ["alpha_evaluation_forecasts.result_id", "alpha_evaluation_forecasts.instrument_id"],
            name="fk_portfolio_assembly_input_member_forecast",
            ondelete="RESTRICT",
        ),
    )

    input_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    axis_index: Mapped[int] = mapped_column(Integer, primary_key=True)
    alpha_qualification_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    alpha_evaluation_result_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    alpha_signal_artifact_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    instrument_id: Mapped[str] = mapped_column(String(200), nullable=False)
    expected_return: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    uncertainty: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    confidence: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    previous_weight: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    max_trade_notional: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    max_position_notional: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    max_participation_rate: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    days_to_liquidate: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)
    stressed_capacity: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)


class PortfolioAssemblyInputCovariance(Base):
    __tablename__ = "portfolio_assembly_input_covariances"
    __table_args__ = (
        CheckConstraint(
            "left_axis_index >= 0 AND right_axis_index >= left_axis_index",
            name="ck_portfolio_assembly_input_covariance_axes",
        ),
        CheckConstraint(
            "lower(CAST(covariance AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_portfolio_assembly_input_covariance_finite",
        ),
        CheckConstraint(
            "left_axis_index <> right_axis_index OR covariance >= 0",
            name="ck_portfolio_assembly_input_covariance_diagonal",
        ),
        ForeignKeyConstraint(
            ["input_id", "left_axis_index"],
            ["portfolio_assembly_input_members.input_id", "portfolio_assembly_input_members.axis_index"],
            name="fk_portfolio_assembly_covariance_left_axis",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["input_id", "right_axis_index"],
            ["portfolio_assembly_input_members.input_id", "portfolio_assembly_input_members.axis_index"],
            name="fk_portfolio_assembly_covariance_right_axis",
            ondelete="RESTRICT",
        ),
    )

    input_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    left_axis_index: Mapped[int] = mapped_column(Integer, primary_key=True)
    right_axis_index: Mapped[int] = mapped_column(Integer, primary_key=True)
    covariance: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)


class PortfolioSearchLedgerEntry(Base):
    __tablename__ = "portfolio_search_ledger_entries"
    __table_args__ = (
        UniqueConstraint(
            "portfolio_program_id",
            "cause_event_id",
            "attempt_type",
            name="uq_portfolio_search_ledger_attempt",
        ),
        CheckConstraint(
            "attempt_type IN ('INPUT_STAGING', 'INPUT_EVALUATION', 'ASSEMBLY')",
            name="ck_portfolio_search_ledger_attempt",
        ),
        CheckConstraint(
            "outcome_class IN ('INCONCLUSIVE', 'INVALID', 'INFEASIBLE', 'STALE')",
            name="ck_portfolio_search_ledger_outcome",
        ),
        CheckConstraint("length(trim(reason_code)) > 0", name="ck_portfolio_search_ledger_reason"),
        ForeignKeyConstraint(
            ["portfolio_program_id"], ["portfolio_programs.id"], ondelete="RESTRICT"
        ),
        ForeignKeyConstraint(
            ["cause_event_id"], ["events.id"], ondelete="RESTRICT"
        ),
        ForeignKeyConstraint(
            ["portfolio_assembly_input_id"],
            ["portfolio_assembly_inputs.id"],
            ondelete="RESTRICT",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    portfolio_program_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    cause_event_id: Mapped[int] = mapped_column(IDENTITY_INT, nullable=False)
    portfolio_assembly_input_id: Mapped[UUID | None] = mapped_column(Uuid)
    attempt_type: Mapped[str] = mapped_column(String(40), nullable=False)
    outcome_class: Mapped[str] = mapped_column(String(20), nullable=False)
    reason_code: Mapped[str] = mapped_column(String(100), nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class PortfolioCandidate(Base):
    __tablename__ = "portfolio_candidates"
    __table_args__ = (
        Index("ix_portfolio_candidate_program", "portfolio_program_id"),
        UniqueConstraint("id", "portfolio_program_id", name="uq_portfolio_candidate_program_pair"),
        UniqueConstraint(
            "id",
            "candidate_family_id",
            "portfolio_program_id",
            "mandate_version_id",
            "assembly_input_id",
            name="uq_portfolio_candidate_evaluation_lineage",
        ),
        UniqueConstraint("assembly_input_id", name="uq_portfolio_candidate_assembly_input"),
        CheckConstraint(
            "assembly_input_id IS NULL OR "
            "(candidate_family_id IS NOT NULL AND mandate_version_id IS NOT NULL "
            "AND capital_context_version_id IS NOT NULL AND universe_version_id IS NOT NULL "
            "AND state = 'ASSEMBLED')",
            name="ck_portfolio_candidate_typed_assembled",
        ),
        ForeignKeyConstraint(
            ["candidate_family_id", "portfolio_program_id", "mandate_version_id"],
            [
                "portfolio_candidate_families.id",
                "portfolio_candidate_families.portfolio_program_id",
                "portfolio_candidate_families.mandate_version_id",
            ],
            name="fk_portfolio_candidate_family_program",
            ondelete="RESTRICT",
            use_alter=True,
        ),
        ForeignKeyConstraint(
            [
                "assembly_input_id",
                "portfolio_program_id",
                "mandate_version_id",
                "capital_context_version_id",
                "universe_version_id",
            ],
            [
                "portfolio_assembly_inputs.id",
                "portfolio_assembly_inputs.portfolio_program_id",
                "portfolio_assembly_inputs.mandate_version_id",
                "portfolio_assembly_inputs.capital_context_version_id",
                "portfolio_assembly_inputs.universe_version_id",
            ],
            name="fk_portfolio_candidate_assembly_input",
            ondelete="RESTRICT",
            use_alter=True,
        ),
        ForeignKeyConstraint(
            ["universe_version_id"],
            ["market_universe_versions.id"],
            name="fk_portfolio_candidate_universe",
            ondelete="RESTRICT",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    candidate_family_id: Mapped[UUID | None] = mapped_column(Uuid)
    portfolio_program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("portfolio_programs.id", ondelete="CASCADE"), nullable=False
    )
    mandate_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    mandate_name: Mapped[str | None] = mapped_column(String(200))
    capital_context_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    assembly_input_id: Mapped[UUID | None] = mapped_column(Uuid)
    universe_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    universe_set_json: Mapped[Any] = mapped_column(JSON_VALUE, nullable=False, default=list)
    policy_version: Mapped[str | None] = mapped_column(String(100))
    risk_model_version: Mapped[str | None] = mapped_column(String(100))
    cost_model_version: Mapped[str | None] = mapped_column(String(100))
    capacity_model_version: Mapped[str | None] = mapped_column(String(100))
    constraint_set_version: Mapped[str | None] = mapped_column(String(100))
    rebalance_policy_version: Mapped[str | None] = mapped_column(String(100))
    evaluation_episode_id: Mapped[UUID | None] = mapped_column(Uuid)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="READY")
    members: Mapped[list[dict[str, Any]]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    metrics: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class PortfolioCandidateMember(Base):
    __tablename__ = "portfolio_candidate_members"
    __table_args__ = (
        CheckConstraint("role = 'PRIMARY_ALPHA'", name="ck_portfolio_candidate_member_role"),
        CheckConstraint(
            "target_weight >= 0 AND target_weight <= 1 AND "
            "lower(CAST(target_weight AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_portfolio_candidate_member_weight",
        ),
        ForeignKeyConstraint(
            ["candidate_id"], ["portfolio_candidates.id"], ondelete="RESTRICT"
        ),
        ForeignKeyConstraint(
            ["alpha_qualification_id"], ["alpha_qualifications.id"], ondelete="RESTRICT"
        ),
    )

    candidate_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    alpha_qualification_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    role: Mapped[str] = mapped_column(String(40), nullable=False)
    target_weight: Mapped[Decimal] = mapped_column(Numeric(20, 8), nullable=False)


class PortfolioEvaluationAssignment(Base):
    """Frozen trusted-evaluator request for one assembled Portfolio Candidate."""

    __tablename__ = "portfolio_evaluation_assignments"
    __table_args__ = (
        UniqueConstraint("candidate_id", name="uq_portfolio_evaluation_assignment_candidate"),
        UniqueConstraint("id", "candidate_id", name="uq_portfolio_evaluation_assignment_candidate_pair"),
        CheckConstraint(
            "evaluator_contract_version = 'PORTFOLIO_EVALUATION_V1'",
            name="ck_portfolio_evaluation_assignment_contract",
        ),
        CheckConstraint(
            "(state IN ('FROZEN', 'QUEUED', 'RUNNING') "
            "AND private_result_ref IS NULL AND evaluated_at IS NULL "
            "AND outcome IS NULL AND completed_at IS NULL) OR "
            "(state = 'FINALIZED' AND private_result_ref IS NOT NULL "
            "AND evaluated_at IS NOT NULL AND outcome IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID') "
            "AND completed_at IS NOT NULL)",
            name="ck_portfolio_evaluation_assignment_state",
        ),
        ForeignKeyConstraint(
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
        ForeignKeyConstraint(
            ["evaluation_dataset_selection_id", "sealed_dataset_revision_id"],
            [
                "evaluation_dataset_selections.id",
                "evaluation_dataset_selections.sealed_dataset_revision_id",
            ],
            name="fk_portfolio_evaluation_assignment_selection_sealed",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["promotion_policy_version_id"],
            ["promotion_policy_versions.id"],
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(["cause_event_id"], ["events.id"], ondelete="RESTRICT"),
        ForeignKeyConstraint(
            ["previous_candidate_id", "portfolio_program_id"],
            ["portfolio_candidates.id", "portfolio_candidates.portfolio_program_id"],
            name="fk_portfolio_evaluation_assignment_predecessor",
            ondelete="RESTRICT",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    portfolio_program_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    candidate_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    candidate_family_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    mandate_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    assembly_input_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    evaluation_dataset_selection_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    sealed_dataset_revision_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    promotion_policy_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    cause_event_id: Mapped[int] = mapped_column(IDENTITY_INT, nullable=False)
    previous_candidate_id: Mapped[UUID | None] = mapped_column(Uuid)
    evaluator_contract_version: Mapped[str] = mapped_column(String(80), nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="FROZEN")
    private_result_ref: Mapped[UUID | None] = mapped_column(Uuid)
    evaluated_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    outcome: Mapped[str | None] = mapped_column(String(20))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    completed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class PortfolioEvaluationEpisode(Base):
    """One disclosure boundary for the frozen Portfolio evaluation Assignment."""

    __tablename__ = "portfolio_evaluation_episodes"
    __table_args__ = (
        UniqueConstraint("assignment_id", name="uq_portfolio_evaluation_episode_assignment"),
        UniqueConstraint("id", "candidate_id", name="uq_portfolio_evaluation_episode_candidate_pair"),
        CheckConstraint(
            "(state IN ('ASSIGNED', 'EVALUATING') AND result IS NULL "
            "AND evaluated_at IS NULL AND disclosed_at IS NULL) OR "
            "(state = 'DISCLOSED' AND result IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID') "
            "AND evaluated_at IS NOT NULL AND disclosed_at IS NOT NULL)",
            name="ck_portfolio_evaluation_episode_state",
        ),
        ForeignKeyConstraint(
            ["assignment_id", "candidate_id"],
            [
                "portfolio_evaluation_assignments.id",
                "portfolio_evaluation_assignments.candidate_id",
            ],
            name="fk_portfolio_evaluation_episode_assignment_candidate",
            ondelete="RESTRICT",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    assignment_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    candidate_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="ASSIGNED")
    result: Mapped[str | None] = mapped_column(String(20))
    evaluated_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    disclosed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class PortfolioEvaluationMetric(Base):
    __tablename__ = "portfolio_evaluation_metrics"
    __table_args__ = (
        CheckConstraint(
            "status IN ('AVAILABLE', 'NOT_AVAILABLE')", name="ck_portfolio_evaluation_metric_status"
        ),
        CheckConstraint(
            "(status = 'AVAILABLE' AND value IS NOT NULL "
            "AND lower(CAST(value AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')) "
            "OR (status = 'NOT_AVAILABLE' AND value IS NULL)",
            name="ck_portfolio_evaluation_metric_value",
        ),
        ForeignKeyConstraint(
            ["episode_id"], ["portfolio_evaluation_episodes.id"], ondelete="RESTRICT"
        ),
    )

    episode_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    metric_code: Mapped[str] = mapped_column(String(100), primary_key=True)
    status: Mapped[str] = mapped_column(String(20), nullable=False)
    value: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))


class PortfolioEvaluationGate(Base):
    __tablename__ = "portfolio_evaluation_gates"
    __table_args__ = (
        CheckConstraint(
            "status IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_portfolio_evaluation_gate_status",
        ),
        CheckConstraint(
            "(status = 'PASS' AND reason_code IS NULL) OR "
            "(status <> 'PASS' AND reason_code IS NOT NULL AND length(trim(reason_code)) > 0)",
            name="ck_portfolio_evaluation_gate_reason",
        ),
        ForeignKeyConstraint(
            ["episode_id"], ["portfolio_evaluation_episodes.id"], ondelete="RESTRICT"
        ),
    )

    episode_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    gate_code: Mapped[str] = mapped_column(String(100), primary_key=True)
    status: Mapped[str] = mapped_column(String(20), nullable=False)
    reason_code: Mapped[str | None] = mapped_column(String(100))


class PortfolioEvaluationDisclosure(Base):
    __tablename__ = "portfolio_evaluation_disclosures"
    __table_args__ = (
        CheckConstraint(
            "classification IN ('QUALIFIED', 'REJECTED', 'INCONCLUSIVE', 'INVALID')",
            name="ck_portfolio_evaluation_disclosure_classification",
        ),
        CheckConstraint(
            "(classification = 'QUALIFIED' AND reason_code IS NULL) OR "
            "(classification <> 'QUALIFIED' AND reason_code IS NOT NULL "
            "AND length(trim(reason_code)) > 0)",
            name="ck_portfolio_evaluation_disclosure_reason",
        ),
        ForeignKeyConstraint(
            ["episode_id", "candidate_id"],
            [
                "portfolio_evaluation_episodes.id",
                "portfolio_evaluation_episodes.candidate_id",
            ],
            name="fk_portfolio_evaluation_disclosure_episode_candidate",
            ondelete="RESTRICT",
        ),
    )

    episode_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    candidate_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    classification: Mapped[str] = mapped_column(String(20), nullable=False)
    reason_code: Mapped[str | None] = mapped_column(String(100))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class DownstreamSystem(Base, TimestampMixin):
    __tablename__ = "downstream_systems"
    __table_args__ = (
        UniqueConstraint("name", name="uq_downstream_system_name"),
        CheckConstraint("revision > 0", name="ck_downstream_system_revision"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    name: Mapped[str] = mapped_column(String(200), nullable=False)
    environment_type: Mapped[str] = mapped_column(String(40), nullable=False)
    enabled: Mapped[bool] = mapped_column(Boolean, nullable=False, default=True)
    package_contract_version: Mapped[str] = mapped_column(String(40), nullable=False, default="1")
    feedback_contract_version: Mapped[str] = mapped_column(String(40), nullable=False, default="1")
    compatibility: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    preflight_state: Mapped[str] = mapped_column(String(40), nullable=False, default="PENDING")
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    public_config: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    service_token_ciphertext: Mapped[bytes | None] = mapped_column(LargeBinary)
    service_token_nonce: Mapped[bytes | None] = mapped_column(LargeBinary)
    service_token_key_version: Mapped[int | None] = mapped_column(Integer)


class FeedbackContractVersion(Base):
    """Immutable typed completeness contract for one logical downstream purpose."""

    __tablename__ = "feedback_contract_versions"
    __table_args__ = (
        UniqueConstraint(
            "downstream_system_id", "version_no", name="uq_feedback_contract_version"
        ),
        UniqueConstraint(
            "id", "downstream_system_id", name="uq_feedback_contract_system_pair"
        ),
        CheckConstraint("version_no > 0", name="ck_feedback_contract_version_number"),
        CheckConstraint("purpose IN ('PAPER', 'LIVE')", name="ck_feedback_contract_version_purpose"),
        CheckConstraint("state IN ('ACTIVE', 'RETIRED')", name="ck_feedback_contract_version_state"),
        CheckConstraint(
            "minimum_observation_seconds > 0 AND minimum_valid_sample_size > 0 "
            "AND first_status_deadline_seconds > 0 AND complete_feedback_deadline_seconds > 0 "
            "AND grace_period_seconds >= 0",
            name="ck_feedback_contract_version_timing",
        ),
        CheckConstraint(
            "length(trim(disclosure_policy)) > 0",
            name="ck_feedback_contract_version_contracts",
        ),
        ForeignKeyConstraint(
            ["downstream_system_id"], ["downstream_systems.id"], ondelete="RESTRICT"
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    downstream_system_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    version_no: Mapped[int] = mapped_column(Integer, nullable=False)
    purpose: Mapped[str] = mapped_column(String(20), nullable=False)
    minimum_observation_seconds: Mapped[int] = mapped_column(Integer, nullable=False)
    minimum_valid_sample_size: Mapped[int] = mapped_column(Integer, nullable=False)
    first_status_deadline_seconds: Mapped[int] = mapped_column(Integer, nullable=False)
    complete_feedback_deadline_seconds: Mapped[int] = mapped_column(Integer, nullable=False)
    grace_period_seconds: Mapped[int] = mapped_column(Integer, nullable=False)
    disclosure_policy: Mapped[str] = mapped_column(String(80), nullable=False)
    spec_json: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="ACTIVE")
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class FeedbackContractMetricRequirement(Base):
    __tablename__ = "feedback_contract_metric_requirements"
    __table_args__ = (
        UniqueConstraint(
            "feedback_contract_version_id", "ordinal", name="uq_feedback_contract_metric_ordinal"
        ),
        CheckConstraint("ordinal > 0", name="ck_feedback_contract_metric_ordinal"),
        CheckConstraint(
            "length(trim(metric_code)) > 0", name="ck_feedback_contract_metric_code"
        ),
        ForeignKeyConstraint(
            ["feedback_contract_version_id"],
            ["feedback_contract_versions.id"],
            ondelete="RESTRICT",
        ),
    )

    feedback_contract_version_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    metric_code: Mapped[str] = mapped_column(String(100), primary_key=True)
    ordinal: Mapped[int] = mapped_column(Integer, nullable=False)


class FeedbackContractAcceptedPackageContract(Base):
    __tablename__ = "feedback_contract_accepted_package_contracts"
    __table_args__ = (
        UniqueConstraint(
            "feedback_contract_version_id", "ordinal", name="uq_feedback_package_contract_ordinal"
        ),
        CheckConstraint("ordinal > 0", name="ck_feedback_package_contract_ordinal"),
        CheckConstraint(
            "length(trim(contract_version)) > 0", name="ck_feedback_package_contract_value"
        ),
        ForeignKeyConstraint(
            ["feedback_contract_version_id"],
            ["feedback_contract_versions.id"],
            ondelete="RESTRICT",
        ),
    )

    feedback_contract_version_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    contract_version: Mapped[str] = mapped_column(String(40), primary_key=True)
    ordinal: Mapped[int] = mapped_column(Integer, nullable=False)


class FeedbackContractAcceptedArrowContract(Base):
    __tablename__ = "feedback_contract_accepted_arrow_contracts"
    __table_args__ = (
        UniqueConstraint(
            "feedback_contract_version_id", "ordinal", name="uq_feedback_arrow_contract_ordinal"
        ),
        CheckConstraint("ordinal > 0", name="ck_feedback_arrow_contract_ordinal"),
        CheckConstraint(
            "length(trim(contract_version)) > 0", name="ck_feedback_arrow_contract_value"
        ),
        ForeignKeyConstraint(
            ["feedback_contract_version_id"],
            ["feedback_contract_versions.id"],
            ondelete="RESTRICT",
        ),
    )

    feedback_contract_version_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    contract_version: Mapped[str] = mapped_column(String(40), primary_key=True)
    ordinal: Mapped[int] = mapped_column(Integer, nullable=False)


class DownstreamConnectionVersion(Base):
    """An immutable logical downstream binding, never a mutable current connection."""

    __tablename__ = "downstream_connection_versions"
    __table_args__ = (
        UniqueConstraint(
            "downstream_system_id", "version_no", name="uq_downstream_connection_version"
        ),
        UniqueConstraint(
            "id",
            "downstream_system_id",
            "feedback_contract_version_id",
            name="uq_downstream_connection_policy_tuple",
        ),
        CheckConstraint("version_no > 0", name="ck_downstream_connection_version_number"),
        CheckConstraint("state IN ('ACTIVE', 'RETIRED')", name="ck_downstream_connection_version_state"),
        CheckConstraint(
            "length(trim(package_contract_version)) > 0",
            name="ck_downstream_connection_version_package_contract",
        ),
        ForeignKeyConstraint(
            ["downstream_system_id"], ["downstream_systems.id"], ondelete="RESTRICT"
        ),
        ForeignKeyConstraint(
            ["feedback_contract_version_id", "downstream_system_id"],
            ["feedback_contract_versions.id", "feedback_contract_versions.downstream_system_id"],
            name="fk_downstream_connection_feedback_contract",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["plugin_release_id"], ["plugin_releases.id"], ondelete="RESTRICT"
        ),
        ForeignKeyConstraint(
            ["credential_set_id"], ["credential_sets.id"], ondelete="RESTRICT"
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    downstream_system_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    version_no: Mapped[int] = mapped_column(Integer, nullable=False)
    plugin_release_id: Mapped[UUID | None] = mapped_column(Uuid)
    credential_set_id: Mapped[UUID | None] = mapped_column(Uuid)
    package_contract_version: Mapped[str] = mapped_column(String(40), nullable=False)
    feedback_contract_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    public_config: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="ACTIVE")
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class PromotionEvaluation(Base):
    """One deterministic P2P or P2L decision built from immutable evidence."""

    __tablename__ = "promotion_evaluations"
    __table_args__ = (
        UniqueConstraint(
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
        CheckConstraint(
            "purpose IN ('PORTFOLIO_TO_PAPER', 'PAPER_TO_LIVE')",
            name="ck_promotion_evaluation_purpose",
        ),
        CheckConstraint(
            "outcome IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_promotion_evaluation_outcome",
        ),
        CheckConstraint(
            "action IN ('MANUAL_APPROVAL', 'AUTO_HANDOFF', 'NO_ACTION')",
            name="ck_promotion_evaluation_action",
        ),
        CheckConstraint(
            "(purpose = 'PORTFOLIO_TO_PAPER' AND portfolio_evaluation_episode_id IS NOT NULL "
            "AND forward_evidence_episode_id IS NULL AND paper_to_live_policy_version_id IS NOT NULL) OR "
            "(purpose = 'PAPER_TO_LIVE' AND portfolio_evaluation_episode_id IS NULL "
            "AND forward_evidence_episode_id IS NOT NULL AND paper_to_live_policy_version_id IS NULL)",
            name="ck_promotion_evaluation_source_xor",
        ),
        CheckConstraint(
            "purpose <> 'PORTFOLIO_TO_PAPER' OR action <> 'AUTO_HANDOFF'",
            name="ck_promotion_evaluation_p2p_action",
        ),
        ForeignKeyConstraint(
            ["portfolio_evaluation_episode_id", "candidate_id"],
            [
                "portfolio_evaluation_episodes.id",
                "portfolio_evaluation_episodes.candidate_id",
            ],
            name="fk_promotion_evaluation_portfolio_episode_candidate",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["forward_evidence_episode_id"],
            ["forward_evidence_episodes.id"],
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["candidate_package_id", "candidate_id", "package_revision"],
            [
                "candidate_packages.id",
                "candidate_packages.candidate_id",
                "candidate_packages.revision",
            ],
            name="fk_promotion_evaluation_candidate_package",
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
            ["policy_version_id"], ["promotion_policy_versions.id"], ondelete="RESTRICT"
        ),
        ForeignKeyConstraint(
            ["paper_to_live_policy_version_id"],
            ["promotion_policy_versions.id"],
            ondelete="RESTRICT",
        ),
        ForeignKeyConstraint(
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
        ForeignKeyConstraint(
            ["preflight_receipt_id"], ["preflight_receipts.id"], ondelete="RESTRICT"
        ),
        Index(
            "uq_promotion_evaluation_p2p_episode",
            "portfolio_evaluation_episode_id",
            unique=True,
            sqlite_where=text("purpose = 'PORTFOLIO_TO_PAPER'"),
            postgresql_where=text("purpose = 'PORTFOLIO_TO_PAPER'"),
        ),
        Index(
            "uq_promotion_evaluation_p2l_episode",
            "forward_evidence_episode_id",
            unique=True,
            sqlite_where=text("purpose = 'PAPER_TO_LIVE'"),
            postgresql_where=text("purpose = 'PAPER_TO_LIVE'"),
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    purpose: Mapped[str] = mapped_column(String(40), nullable=False)
    portfolio_evaluation_episode_id: Mapped[UUID | None] = mapped_column(Uuid)
    forward_evidence_episode_id: Mapped[UUID | None] = mapped_column(Uuid)
    candidate_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    candidate_package_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    package_revision: Mapped[int] = mapped_column(Integer, nullable=False)
    policy_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    paper_to_live_policy_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    downstream_system_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    downstream_connection_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    feedback_contract_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    preflight_receipt_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    outcome: Mapped[str] = mapped_column(String(20), nullable=False)
    action: Mapped[str] = mapped_column(String(20), nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class PromotionGateResult(Base):
    __tablename__ = "promotion_gate_results"
    __table_args__ = (
        CheckConstraint(
            "status IN ('PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID')",
            name="ck_promotion_gate_result_status",
        ),
        CheckConstraint(
            "(status = 'PASS' AND reason_code IS NULL) OR "
            "(status <> 'PASS' AND reason_code IS NOT NULL AND length(trim(reason_code)) > 0)",
            name="ck_promotion_gate_result_reason",
        ),
        CheckConstraint(
            "actual IS NULL OR lower(CAST(actual AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_promotion_gate_result_actual",
        ),
        CheckConstraint(
            "expected IS NULL OR lower(CAST(expected AS TEXT)) "
            "NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')",
            name="ck_promotion_gate_result_expected",
        ),
        ForeignKeyConstraint(
            ["evaluation_id"], ["promotion_evaluations.id"], ondelete="RESTRICT"
        ),
    )

    evaluation_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    gate_code: Mapped[str] = mapped_column(String(100), primary_key=True)
    status: Mapped[str] = mapped_column(String(20), nullable=False)
    actual: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    expected: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    reason_code: Mapped[str | None] = mapped_column(String(100))


class ApprovalSnapshot(Base, TimestampMixin):
    __tablename__ = "approval_snapshots"
    __table_args__ = (
        UniqueConstraint(
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
            name="uq_approval_snapshot_handoff_lineage",
        ),
        CheckConstraint(
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
            "AND preflight_receipt_id IS NOT NULL AND paper_to_live_policy_version_id IS NULL)",
            name="ck_approval_snapshot_typed_lineage",
        ),
        Index("ix_approval_snapshot_state", "state"),
        Index(
            "uq_approval_snapshot_promotion_evaluation",
            "promotion_evaluation_id",
            unique=True,
            sqlite_where=text("promotion_evaluation_id IS NOT NULL"),
            postgresql_where=text("promotion_evaluation_id IS NOT NULL"),
        ),
        ForeignKeyConstraint(
            [
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
            ],
            [
                "promotion_evaluations.id",
                "promotion_evaluations.purpose",
                "promotion_evaluations.candidate_id",
                "promotion_evaluations.candidate_package_id",
                "promotion_evaluations.package_revision",
                "promotion_evaluations.downstream_system_id",
                "promotion_evaluations.downstream_connection_version_id",
                "promotion_evaluations.feedback_contract_version_id",
                "promotion_evaluations.preflight_receipt_id",
                "promotion_evaluations.paper_to_live_policy_version_id",
            ],
            name="fk_approval_snapshot_promotion_lineage",
            ondelete="RESTRICT",
            use_alter=True,
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    candidate_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("portfolio_candidates.id", ondelete="RESTRICT"), nullable=False
    )
    candidate_package_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("candidate_packages.id", ondelete="RESTRICT")
    )
    candidate_package_revision: Mapped[int | None] = mapped_column(Integer)
    promotion_evaluation_id: Mapped[UUID | None] = mapped_column(
        Uuid,
        ForeignKey(
            "promotion_evaluations.id",
            ondelete="RESTRICT",
            use_alter=True,
            name="fk_approval_snapshot_promotion_evaluation",
        ),
    )
    promotion_purpose: Mapped[str | None] = mapped_column(String(40))
    purpose: Mapped[str] = mapped_column(String(40), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="PENDING")
    downstream_system_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("downstream_systems.id", ondelete="RESTRICT")
    )
    downstream_connection_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("downstream_connection_versions.id", ondelete="RESTRICT")
    )
    feedback_contract_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("feedback_contract_versions.id", ondelete="RESTRICT")
    )
    preflight_receipt_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("preflight_receipts.id", ondelete="RESTRICT")
    )
    paper_to_live_policy_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("promotion_policy_versions.id", ondelete="RESTRICT")
    )
    valid_until: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    expires_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    stale_reason: Mapped[str | None] = mapped_column(Text)
    recommendation_rationale: Mapped[str | None] = mapped_column(Text)
    human_report: Mapped[Any] = mapped_column(JSON_VALUE)
    evidence_summary: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    capital_context: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    risk_summary: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    cost_summary: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    capacity_summary: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    changes_summary: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)


class CandidatePackage(Base):
    __tablename__ = "candidate_packages"
    __table_args__ = (
        UniqueConstraint(
            "candidate_id", "revision", name="uq_candidate_package_candidate_revision"
        ),
        UniqueConstraint(
            "id", "candidate_id", "revision", name="uq_candidate_package_promotion_lineage"
        ),
        CheckConstraint("revision > 0", name="ck_candidate_package_revision"),
        CheckConstraint(
            "state IN ('LEGACY_NON_EXECUTABLE', 'STALE', 'BUILDING', 'AVAILABLE')",
            name="ck_candidate_package_state",
        ),
        Index(
            "uq_candidate_package_building_candidate",
            "candidate_id",
            unique=True,
            sqlite_where=text("state = 'BUILDING'"),
            postgresql_where=text("state = 'BUILDING'"),
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    candidate_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("portfolio_candidates.id", ondelete="RESTRICT"), nullable=False
    )
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    contract_version: Mapped[str] = mapped_column(String(40), nullable=False, default="1")
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="BUILDING")
    manifest_json: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    relative_path: Mapped[str] = mapped_column(Text, nullable=False)
    payload: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class HandoffOffer(Base, TimestampMixin):
    __tablename__ = "handoff_offers"
    __table_args__ = (
        UniqueConstraint(
            "id", "feedback_contract_version_id", name="uq_handoff_offer_feedback_contract_pair"
        ),
        CheckConstraint(
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
            "AND paper_to_live_policy_version_id IS NULL)",
            name="ck_handoff_offer_typed_lineage",
        ),
        Index("ix_handoff_offer_state", "state"),
        Index(
            "uq_handoff_offer_typed_approval",
            "approval_id",
            unique=True,
            sqlite_where=text("downstream_connection_version_id IS NOT NULL"),
            postgresql_where=text("downstream_connection_version_id IS NOT NULL"),
        ),
        ForeignKeyConstraint(
            [
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
            ],
            [
                "approval_snapshots.id",
                "approval_snapshots.promotion_purpose",
                "approval_snapshots.candidate_id",
                "approval_snapshots.candidate_package_id",
                "approval_snapshots.candidate_package_revision",
                "approval_snapshots.downstream_system_id",
                "approval_snapshots.downstream_connection_version_id",
                "approval_snapshots.feedback_contract_version_id",
                "approval_snapshots.preflight_receipt_id",
                "approval_snapshots.paper_to_live_policy_version_id",
            ],
            name="fk_handoff_offer_approval_lineage",
            ondelete="RESTRICT",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    approval_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("approval_snapshots.id", ondelete="RESTRICT"), nullable=False
    )
    candidate_package_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("candidate_packages.id", ondelete="RESTRICT"), nullable=False
    )
    candidate_package_revision: Mapped[int | None] = mapped_column(Integer)
    candidate_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("portfolio_candidates.id", ondelete="RESTRICT"), nullable=False
    )
    promotion_purpose: Mapped[str | None] = mapped_column(String(40))
    purpose: Mapped[str] = mapped_column(String(40), nullable=False)
    downstream_system_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("downstream_systems.id", ondelete="RESTRICT"), nullable=False
    )
    downstream_connection_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("downstream_connection_versions.id", ondelete="RESTRICT")
    )
    feedback_contract_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("feedback_contract_versions.id", ondelete="RESTRICT")
    )
    preflight_receipt_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("preflight_receipts.id", ondelete="RESTRICT")
    )
    paper_to_live_policy_version_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("promotion_policy_versions.id", ondelete="RESTRICT")
    )
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="AVAILABLE")
    claim_deadline: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    stale_reason: Mapped[str | None] = mapped_column(Text)
    feedback_state: Mapped[str | None] = mapped_column(String(40), default="PENDING")
    feedback_contract_snapshot: Mapped[dict[str, Any]] = mapped_column(
        JSON_VALUE, nullable=False, default=dict
    )
    claimed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    accepted_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class FeedbackPackage(Base):
    """One immutable typed Paper feedback submission for a frozen Handoff."""

    __tablename__ = "feedback_packages"
    __table_args__ = (
        UniqueConstraint("handoff_offer_id", name="uq_feedback_package_handoff"),
        UniqueConstraint("id", "handoff_offer_id", name="uq_feedback_package_handoff_pair"),
        CheckConstraint(
            "state IN ('RECEIVED', 'COMPLETE', 'INVALID')", name="ck_feedback_package_state"
        ),
        CheckConstraint(
            "observation_end >= observation_start AND sample_size > 0",
            name="ck_feedback_package_observation",
        ),
        ForeignKeyConstraint(
            ["handoff_offer_id", "feedback_contract_version_id"],
            ["handoff_offers.id", "handoff_offers.feedback_contract_version_id"],
            name="fk_feedback_package_handoff_contract",
            ondelete="RESTRICT",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    handoff_offer_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    feedback_contract_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False)
    observation_start: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    observation_end: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    sample_size: Mapped[int] = mapped_column(Integer, nullable=False)
    summary_json: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    relative_path: Mapped[str | None] = mapped_column(Text)
    received_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )


class ForwardEvidenceEpisode(Base):
    __tablename__ = "forward_evidence_episodes"
    __table_args__ = (
        Index("ix_forward_evidence_handoff", "handoff_id"),
        Index(
            "uq_forward_evidence_feedback_package",
            "feedback_package_id",
            unique=True,
            sqlite_where=text("feedback_package_id IS NOT NULL"),
            postgresql_where=text("feedback_package_id IS NOT NULL"),
        ),
        ForeignKeyConstraint(
            ["feedback_package_id", "handoff_id"],
            ["feedback_packages.id", "feedback_packages.handoff_offer_id"],
            name="fk_forward_evidence_feedback_handoff",
            ondelete="RESTRICT",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    handoff_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("handoff_offers.id", ondelete="CASCADE"), nullable=False
    )
    feedback_package_id: Mapped[UUID | None] = mapped_column(Uuid)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="FEEDBACK_COMPLETE")
    evidence: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    observation_start: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    observation_end: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    sample_size: Mapped[int] = mapped_column(Integer, nullable=False)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class ForwardEvidenceMetric(Base):
    __tablename__ = "forward_evidence_metrics"
    __table_args__ = (
        CheckConstraint(
            "length(trim(metric_code)) > 0", name="ck_forward_evidence_metric_code"
        ),
        CheckConstraint(
            "status IN ('AVAILABLE', 'NOT_AVAILABLE')", name="ck_forward_evidence_metric_status"
        ),
        CheckConstraint(
            "(status = 'AVAILABLE' AND value IS NOT NULL "
            "AND lower(CAST(value AS TEXT)) NOT IN ('nan', 'inf', '-inf', 'infinity', '-infinity')) "
            "OR (status = 'NOT_AVAILABLE' AND value IS NULL)",
            name="ck_forward_evidence_metric_value",
        ),
        ForeignKeyConstraint(
            ["episode_id"], ["forward_evidence_episodes.id"], ondelete="RESTRICT"
        ),
    )

    episode_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)
    metric_code: Mapped[str] = mapped_column(String(100), primary_key=True)
    value: Mapped[Decimal | None] = mapped_column(Numeric(20, 8))
    status: Mapped[str] = mapped_column(String(20), nullable=False)


class DegradationObservation(Base):
    """One immutable, typed evaluation of completed Forward Evidence."""

    __tablename__ = "degradation_observations"
    __table_args__ = (
        UniqueConstraint(
            "forward_evidence_episode_id",
            "subject_type",
            "subject_id",
            "metric_name",
            "policy_revision",
            name="uq_degradation_observation_causal",
        ),
        CheckConstraint(
            "subject_type IN ('ALPHA', 'PORTFOLIO')",
            name="ck_degradation_observation_subject_type",
        ),
        CheckConstraint(
            "severity >= 0 AND severity <= 1",
            name="ck_degradation_observation_severity",
        ),
        CheckConstraint(
            "confidence >= 0 AND confidence <= 1",
            name="ck_degradation_observation_confidence",
        ),
        CheckConstraint(
            "state IN ('HEALTHY', 'WATCH', 'DEGRADING', 'FAILED', 'RECOVERED')",
            name="ck_degradation_observation_state",
        ),
        CheckConstraint(
            "consecutive_breaches >= 0",
            name="ck_degradation_observation_breaches",
        ),
        Index(
            "ix_degradation_observation_program_subject",
            "program_id",
            "subject_type",
            "subject_id",
            "created_at",
        ),
        Index(
            "ix_degradation_observation_forward_evidence",
            "forward_evidence_episode_id",
        ),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="RESTRICT"), nullable=False
    )
    forward_evidence_episode_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("forward_evidence_episodes.id", ondelete="RESTRICT"),
        nullable=False,
    )
    subject_type: Mapped[str] = mapped_column(String(20), nullable=False)
    subject_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    metric_name: Mapped[str] = mapped_column(String(100), nullable=False)
    severity: Mapped[Decimal] = mapped_column(Numeric(10, 8), nullable=False)
    confidence: Mapped[Decimal] = mapped_column(Numeric(10, 8), nullable=False)
    policy_revision: Mapped[str] = mapped_column(String(100), nullable=False)
    policy_snapshot: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    reason_code: Mapped[str] = mapped_column(String(100), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False)
    consecutive_breaches: Mapped[int] = mapped_column(Integer, nullable=False)
    evaluated: Mapped[bool] = mapped_column(Boolean, nullable=False, default=False)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class ResearchWakeEvent(Base):
    """A deduplicated research-only wake caused by one degradation observation."""

    __tablename__ = "research_wake_events"
    __table_args__ = (
        UniqueConstraint(
            "degradation_observation_id",
            name="uq_research_wake_event_observation",
        ),
        UniqueConstraint(
            "program_id",
            "subject_type",
            "subject_id",
            "forward_evidence_episode_id",
            "policy_revision",
            "reason_code",
            name="uq_research_wake_event_causal",
        ),
        CheckConstraint(
            "subject_type IN ('ALPHA', 'PORTFOLIO')",
            name="ck_research_wake_event_subject_type",
        ),
        CheckConstraint(
            "state IN ('PENDING', 'CONSUMED')",
            name="ck_research_wake_event_state",
        ),
        CheckConstraint(
            "(state = 'PENDING' AND cycle_id IS NULL AND consumed_at IS NULL) OR "
            "(state = 'CONSUMED' AND cycle_id IS NOT NULL AND consumed_at IS NOT NULL)",
            name="ck_research_wake_event_consumption",
        ),
        Index("ix_research_wake_event_program_state", "program_id", "state", "created_at"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        Uuid, ForeignKey("research_programs.id", ondelete="RESTRICT"), nullable=False
    )
    degradation_observation_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("degradation_observations.id", ondelete="RESTRICT"),
        nullable=False,
    )
    forward_evidence_episode_id: Mapped[UUID] = mapped_column(
        Uuid,
        ForeignKey("forward_evidence_episodes.id", ondelete="RESTRICT"),
        nullable=False,
    )
    subject_type: Mapped[str] = mapped_column(String(20), nullable=False)
    subject_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    policy_revision: Mapped[str] = mapped_column(String(100), nullable=False)
    reason_code: Mapped[str] = mapped_column(String(100), nullable=False)
    state: Mapped[str] = mapped_column(String(20), nullable=False, default="PENDING")
    cycle_id: Mapped[UUID | None] = mapped_column(
        Uuid, ForeignKey("research_cycles.id", ondelete="RESTRICT")
    )
    consumed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


__all__ = [
    "PublicMutationReceipt",
    "ResearchCharter",
    "ResearchProgram",
    "IdeaContribution",
    "IdeaDraft",
    "ClarificationQuestion",
    "ClarificationAnswer",
    "ProgramRelationship",
    "ResearchBranch",
    "ResearchMission",
    "ResearchCycle",
    "MissionDependency",
    "AgentSession",
    "AgentTurn",
    "MissionArtifact",
    "PreflightReceipt",
    "MarketUniverseVersion",
    "GovernedDataSource",
    "DatasetRevision",
    "DataQualityResult",
    "EvaluationDatasetSelection",
    "EvaluationDesignVersion",
    "FeaturePipelineVersion",
    "AlphaModel",
    "AlphaModelVersion",
    "AlphaSignalArtifact",
    "AlphaCalibrationVersion",
    "PromotionPolicyVersion",
    "PromotionPolicyGate",
    "AlphaDiscoveryEvaluation",
    "AlphaDiscoveryEvaluationMetric",
    "AlphaDiscoveryEvaluationGate",
    "AlphaEvaluationAssignment",
    "AlphaEvaluationAssignmentDatasetRevision",
    "AlphaEvaluationEpisode",
    "AlphaEvaluationResult",
    "AlphaEvaluationForecast",
    "AlphaEvaluationMetric",
    "AlphaEvaluationGate",
    "EvidenceExposure",
    "Disclosure",
    "AlphaQualification",
    "PortfolioMandate",
    "PortfolioMandateVersion",
    "PortfolioProgram",
    "PortfolioCandidate",
    "PortfolioCandidateFamily",
    "PortfolioCandidateMember",
    "PortfolioInputEvaluationAssignment",
    "PortfolioInputEvaluationAssignmentMember",
    "PortfolioAssemblyInput",
    "PortfolioAssemblyInputMember",
    "PortfolioAssemblyInputCovariance",
    "PortfolioSearchLedgerEntry",
    "PortfolioEvaluationAssignment",
    "PortfolioEvaluationEpisode",
    "PortfolioEvaluationMetric",
    "PortfolioEvaluationGate",
    "PortfolioEvaluationDisclosure",
    "DownstreamSystem",
    "DownstreamConnectionVersion",
    "FeedbackContractVersion",
    "FeedbackContractMetricRequirement",
    "FeedbackContractAcceptedPackageContract",
    "FeedbackContractAcceptedArrowContract",
    "ApprovalSnapshot",
    "CandidatePackage",
    "HandoffOffer",
    "FeedbackPackage",
    "ForwardEvidenceEpisode",
    "ForwardEvidenceMetric",
    "PromotionEvaluation",
    "PromotionGateResult",
    "DegradationObservation",
    "ResearchWakeEvent",
]
