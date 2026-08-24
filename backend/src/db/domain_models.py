"""QuaZonai research-intelligence and portfolio-construction domain models."""

from __future__ import annotations

from datetime import datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import Boolean, DateTime, ForeignKey, Index, Integer, String, Text, UniqueConstraint, Uuid
from sqlalchemy.orm import Mapped, mapped_column

from db.base import Base, JSON_VALUE, TimestampMixin


class PublicMutationReceipt(Base):
    __tablename__ = "public_mutation_receipts"

    idempotency_key: Mapped[str] = mapped_column(String(200), primary_key=True)
    operation_name: Mapped[str] = mapped_column(String(200), nullable=False)
    normalized_request: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    response_json: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    status_code: Mapped[int] = mapped_column(Integer, nullable=False, default=200)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class ResearchCharter(Base):
    __tablename__ = "research_charters"

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    original_idea_text: Mapped[str] = mapped_column(Text, nullable=False)
    research_question: Mapped[str] = mapped_column(Text, nullable=False)
    market_scope: Mapped[Any] = mapped_column(JSON_VALUE, nullable=False, default=list)
    universe_version_ids: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    prediction_horizon: Mapped[str | None] = mapped_column(String(100))
    allowed_data_domains: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    explicit_exclusions: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    material_assumptions: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    system_assumptions: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class ResearchProgram(Base, TimestampMixin):
    __tablename__ = "research_programs"
    __table_args__ = (Index("ix_research_program_state", "state"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    charter_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("research_charters.id", ondelete="RESTRICT"), nullable=False)
    title: Mapped[str] = mapped_column(String(240), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")
    cooling_reason: Mapped[str | None] = mapped_column(Text)
    blocked_reason: Mapped[str | None] = mapped_column(Text)
    wake_reason: Mapped[str | None] = mapped_column(Text)
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)


class ResearchBranch(Base):
    __tablename__ = "research_branches"
    __table_args__ = (Index("ix_research_branch_program", "program_id"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("research_programs.id", ondelete="CASCADE"), nullable=False)
    parent_branch_id: Mapped[UUID | None] = mapped_column(Uuid, ForeignKey("research_branches.id", ondelete="RESTRICT"))
    derivation_type: Mapped[str] = mapped_column(String(80), nullable=False, default="ROOT")
    hypothesis: Mapped[str] = mapped_column(Text, nullable=False)
    changed_assumptions: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    preserved_constraints: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class ResearchMission(Base):
    __tablename__ = "research_missions"
    __table_args__ = (Index("ix_research_mission_program_state", "program_id", "state"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("research_programs.id", ondelete="CASCADE"), nullable=False)
    branch_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("research_branches.id", ondelete="CASCADE"), nullable=False)
    type: Mapped[str] = mapped_column(String(100), nullable=False)
    role: Mapped[str | None] = mapped_column(String(100))
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="PLANNED")
    objective: Mapped[str | None] = mapped_column(Text)
    dependencies: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    finished_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    attempt: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    error_code: Mapped[str | None] = mapped_column(String(100))
    summary: Mapped[str | None] = mapped_column(Text)


class MarketUniverseVersion(Base):
    __tablename__ = "market_universe_versions"
    __table_args__ = (UniqueConstraint("universe_key", "version_no", name="uq_market_universe_version"),)

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
    provider: Mapped[str | None] = mapped_column(String(200))
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")
    universe_scope: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    fields: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    update_cadence: Mapped[str | None] = mapped_column(String(100))
    preflight_state: Mapped[str] = mapped_column(String(40), nullable=False, default="READY")
    public_config: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)


class DatasetRevision(Base):
    __tablename__ = "dataset_revisions"
    __table_args__ = (Index("ix_dataset_revision_source", "data_source_id"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    data_source_id: Mapped[UUID | None] = mapped_column(Uuid, ForeignKey("governed_data_sources.id", ondelete="RESTRICT"))
    universe_version_id: Mapped[UUID | None] = mapped_column(Uuid, ForeignKey("market_universe_versions.id", ondelete="RESTRICT"))
    universe_name: Mapped[str | None] = mapped_column(String(200))
    revision_no: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    schema_version: Mapped[str | None] = mapped_column(String(100))
    event_start: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    event_end: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    available_start: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    available_end: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    row_count: Mapped[int | None] = mapped_column(Integer)
    quality_state: Mapped[str] = mapped_column(String(40), nullable=False, default="VALID")
    point_in_time_state: Mapped[str] = mapped_column(String(40), nullable=False, default="VALID")
    partition: Mapped[str] = mapped_column(String(40), nullable=False, default="DISCOVERY")
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class AlphaQualification(Base):
    __tablename__ = "alpha_qualifications"
    __table_args__ = (Index("ix_alpha_qualification_state", "state"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    program_id: Mapped[UUID | None] = mapped_column(Uuid, ForeignKey("research_programs.id", ondelete="SET NULL"))
    alpha_model_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    calibration_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    universe_version_id: Mapped[UUID | None] = mapped_column(Uuid, ForeignKey("market_universe_versions.id", ondelete="RESTRICT"))
    universe: Mapped[str | None] = mapped_column(String(200))
    horizon: Mapped[str | None] = mapped_column(String(100))
    role: Mapped[str] = mapped_column(String(100), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")
    name: Mapped[str | None] = mapped_column(String(240))
    scope_json: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    evaluation_episode_id: Mapped[UUID | None] = mapped_column(Uuid)
    degradation_state: Mapped[str] = mapped_column(String(40), nullable=False, default="HEALTHY")
    metrics: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    lineage: Mapped[list[dict[str, Any]]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


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


class PortfolioProgram(Base, TimestampMixin):
    __tablename__ = "portfolio_programs"

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    mandate_version_id: Mapped[UUID] = mapped_column(Uuid, nullable=False)
    mandate_name: Mapped[str | None] = mapped_column(String(200))
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="ACTIVE")
    current_candidate_id: Mapped[UUID | None] = mapped_column(Uuid)


class PortfolioCandidate(Base):
    __tablename__ = "portfolio_candidates"
    __table_args__ = (Index("ix_portfolio_candidate_program", "portfolio_program_id"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    candidate_family_id: Mapped[UUID | None] = mapped_column(Uuid)
    portfolio_program_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("portfolio_programs.id", ondelete="CASCADE"), nullable=False)
    mandate_version_id: Mapped[UUID | None] = mapped_column(Uuid)
    mandate_name: Mapped[str | None] = mapped_column(String(200))
    capital_context_version_id: Mapped[UUID | None] = mapped_column(Uuid)
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


class DownstreamSystem(Base, TimestampMixin):
    __tablename__ = "downstream_systems"
    __table_args__ = (UniqueConstraint("name", name="uq_downstream_system_name"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    name: Mapped[str] = mapped_column(String(200), nullable=False)
    environment_type: Mapped[str] = mapped_column(String(40), nullable=False)
    enabled: Mapped[bool] = mapped_column(Boolean, nullable=False, default=True)
    package_contract_version: Mapped[str] = mapped_column(String(40), nullable=False, default="1")
    feedback_contract_version: Mapped[str] = mapped_column(String(40), nullable=False, default="1")
    compatibility: Mapped[list[str]] = mapped_column(JSON_VALUE, nullable=False, default=list)
    preflight_state: Mapped[str] = mapped_column(String(40), nullable=False, default="READY")
    public_config: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)


class ApprovalSnapshot(Base, TimestampMixin):
    __tablename__ = "approval_snapshots"
    __table_args__ = (Index("ix_approval_snapshot_state", "state"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    candidate_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("portfolio_candidates.id", ondelete="RESTRICT"), nullable=False)
    purpose: Mapped[str] = mapped_column(String(40), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="PENDING")
    downstream_system_id: Mapped[UUID | None] = mapped_column(Uuid, ForeignKey("downstream_systems.id", ondelete="RESTRICT"))
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

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    approval_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("approval_snapshots.id", ondelete="RESTRICT"), nullable=False, unique=True)
    candidate_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("portfolio_candidates.id", ondelete="RESTRICT"), nullable=False)
    contract_version: Mapped[str] = mapped_column(String(40), nullable=False, default="1")
    payload: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class HandoffOffer(Base, TimestampMixin):
    __tablename__ = "handoff_offers"
    __table_args__ = (Index("ix_handoff_offer_state", "state"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    approval_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("approval_snapshots.id", ondelete="RESTRICT"), nullable=False)
    candidate_package_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("candidate_packages.id", ondelete="RESTRICT"), nullable=False)
    candidate_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("portfolio_candidates.id", ondelete="RESTRICT"), nullable=False)
    purpose: Mapped[str] = mapped_column(String(40), nullable=False)
    downstream_system_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("downstream_systems.id", ondelete="RESTRICT"), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="AVAILABLE")
    claim_deadline: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    stale_reason: Mapped[str | None] = mapped_column(Text)
    feedback_state: Mapped[str | None] = mapped_column(String(40), default="PENDING")
    claimed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    accepted_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class ForwardEvidenceEpisode(Base):
    __tablename__ = "forward_evidence_episodes"
    __table_args__ = (Index("ix_forward_evidence_handoff", "handoff_id"),)

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    handoff_id: Mapped[UUID] = mapped_column(Uuid, ForeignKey("handoff_offers.id", ondelete="CASCADE"), nullable=False)
    state: Mapped[str] = mapped_column(String(40), nullable=False, default="FEEDBACK_COMPLETE")
    evidence: Mapped[dict[str, Any]] = mapped_column(JSON_VALUE, nullable=False, default=dict)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


__all__ = [
    "PublicMutationReceipt",
    "ResearchCharter",
    "ResearchProgram",
    "ResearchBranch",
    "ResearchMission",
    "MarketUniverseVersion",
    "GovernedDataSource",
    "DatasetRevision",
    "AlphaQualification",
    "PortfolioMandate",
    "PortfolioProgram",
    "PortfolioCandidate",
    "DownstreamSystem",
    "ApprovalSnapshot",
    "CandidatePackage",
    "HandoffOffer",
    "ForwardEvidenceEpisode",
]
