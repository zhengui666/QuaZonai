"""Add indexes for durable queue and event hot paths.

Revision ID: 0005_performance_indexes
Revises: 0004_runtime_configuration
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op

revision = "0005_performance_indexes"
down_revision = "0004_runtime_configuration"
branch_labels = None
depends_on = None


def _index_names(table_name: str) -> set[str]:
    return {
        str(item["name"])
        for item in sa.inspect(op.get_bind()).get_indexes(table_name)
        if item.get("name")
    }


def upgrade() -> None:
    bind = op.get_bind()
    job_indexes = _index_names("jobs")
    event_indexes = _index_names("events")

    if "ix_jobs_ready_queue" not in job_indexes:
        kwargs: dict[str, object] = {}
        if bind.dialect.name == "postgresql":
            kwargs["postgresql_where"] = sa.text("state = 'READY'")
        op.create_index(
            "ix_jobs_ready_queue",
            "jobs",
            ["available_at", "created_at"],
            unique=False,
            **kwargs,
        )

    if "ix_jobs_leased_expiry" not in job_indexes:
        kwargs = {}
        if bind.dialect.name == "postgresql":
            kwargs["postgresql_where"] = sa.text("state = 'LEASED'")
        op.create_index(
            "ix_jobs_leased_expiry",
            "jobs",
            ["lease_expires_at"],
            unique=False,
            **kwargs,
        )

    if "ix_events_aggregate_activity" not in event_indexes:
        op.create_index(
            "ix_events_aggregate_activity",
            "events",
            ["aggregate_type", "aggregate_id", "id"],
            unique=False,
        )


def downgrade() -> None:
    event_indexes = _index_names("events")
    job_indexes = _index_names("jobs")

    if "ix_events_aggregate_activity" in event_indexes:
        op.drop_index("ix_events_aggregate_activity", table_name="events")
    if "ix_jobs_leased_expiry" in job_indexes:
        op.drop_index("ix_jobs_leased_expiry", table_name="jobs")
    if "ix_jobs_ready_queue" in job_indexes:
        op.drop_index("ix_jobs_ready_queue", table_name="jobs")
