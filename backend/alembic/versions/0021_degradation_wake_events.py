"""Persist bounded degradation observations and research-only wake events.

Revision ID: 0021_degradation_wake_events
Revises: 0020_package_before_approval
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import JSONB


revision = "0021_degradation_wake_events"
down_revision = "0020_package_before_approval"
branch_labels = None
depends_on = None

_JSON = sa.JSON().with_variant(JSONB(), "postgresql")


def upgrade() -> None:
    op.create_table(
        "degradation_observations",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("program_id", sa.Uuid(), nullable=False),
        sa.Column("forward_evidence_episode_id", sa.Uuid(), nullable=False),
        sa.Column("subject_type", sa.String(length=20), nullable=False),
        sa.Column("subject_id", sa.Uuid(), nullable=False),
        sa.Column("metric_name", sa.String(length=100), nullable=False),
        sa.Column("severity", sa.Numeric(10, 8), nullable=False),
        sa.Column("confidence", sa.Numeric(10, 8), nullable=False),
        sa.Column("policy_revision", sa.String(length=100), nullable=False),
        sa.Column("policy_snapshot", _JSON, nullable=False),
        sa.Column("reason_code", sa.String(length=100), nullable=False),
        sa.Column("state", sa.String(length=40), nullable=False),
        sa.Column("consecutive_breaches", sa.Integer(), nullable=False),
        sa.Column("evaluated", sa.Boolean(), nullable=False, server_default=sa.false()),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint(
            "subject_type IN ('ALPHA', 'PORTFOLIO')",
            name="ck_degradation_observation_subject_type",
        ),
        sa.CheckConstraint(
            "severity >= 0 AND severity <= 1",
            name="ck_degradation_observation_severity",
        ),
        sa.CheckConstraint(
            "confidence >= 0 AND confidence <= 1",
            name="ck_degradation_observation_confidence",
        ),
        sa.CheckConstraint(
            "state IN ('HEALTHY', 'WATCH', 'DEGRADING', 'FAILED', 'RECOVERED')",
            name="ck_degradation_observation_state",
        ),
        sa.CheckConstraint(
            "consecutive_breaches >= 0",
            name="ck_degradation_observation_breaches",
        ),
        sa.ForeignKeyConstraint(["program_id"], ["research_programs.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(
            ["forward_evidence_episode_id"],
            ["forward_evidence_episodes.id"],
            ondelete="RESTRICT",
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "forward_evidence_episode_id",
            "subject_type",
            "subject_id",
            "metric_name",
            "policy_revision",
            name="uq_degradation_observation_causal",
        ),
    )
    op.create_index(
        "ix_degradation_observation_program_subject",
        "degradation_observations",
        ["program_id", "subject_type", "subject_id", "created_at"],
    )
    op.create_index(
        "ix_degradation_observation_forward_evidence",
        "degradation_observations",
        ["forward_evidence_episode_id"],
    )
    op.create_table(
        "research_wake_events",
        sa.Column("id", sa.Uuid(), nullable=False),
        sa.Column("program_id", sa.Uuid(), nullable=False),
        sa.Column("degradation_observation_id", sa.Uuid(), nullable=False),
        sa.Column("forward_evidence_episode_id", sa.Uuid(), nullable=False),
        sa.Column("subject_type", sa.String(length=20), nullable=False),
        sa.Column("subject_id", sa.Uuid(), nullable=False),
        sa.Column("policy_revision", sa.String(length=100), nullable=False),
        sa.Column("reason_code", sa.String(length=100), nullable=False),
        sa.Column("state", sa.String(length=20), nullable=False, server_default="PENDING"),
        sa.Column("cycle_id", sa.Uuid(), nullable=True),
        sa.Column("consumed_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint(
            "subject_type IN ('ALPHA', 'PORTFOLIO')",
            name="ck_research_wake_event_subject_type",
        ),
        sa.CheckConstraint(
            "state IN ('PENDING', 'CONSUMED')",
            name="ck_research_wake_event_state",
        ),
        sa.CheckConstraint(
            "(state = 'PENDING' AND cycle_id IS NULL AND consumed_at IS NULL) OR "
            "(state = 'CONSUMED' AND cycle_id IS NOT NULL AND consumed_at IS NOT NULL)",
            name="ck_research_wake_event_consumption",
        ),
        sa.ForeignKeyConstraint(["program_id"], ["research_programs.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(
            ["degradation_observation_id"],
            ["degradation_observations.id"],
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(
            ["forward_evidence_episode_id"],
            ["forward_evidence_episodes.id"],
            ondelete="RESTRICT",
        ),
        sa.ForeignKeyConstraint(["cycle_id"], ["research_cycles.id"], ondelete="RESTRICT"),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "degradation_observation_id",
            name="uq_research_wake_event_observation",
        ),
        sa.UniqueConstraint(
            "program_id",
            "subject_type",
            "subject_id",
            "forward_evidence_episode_id",
            "policy_revision",
            "reason_code",
            name="uq_research_wake_event_causal",
        ),
    )
    op.create_index(
        "ix_research_wake_event_program_state",
        "research_wake_events",
        ["program_id", "state", "created_at"],
    )


def downgrade() -> None:
    # These are immutable research facts; rollback must not silently delete them.
    pass
