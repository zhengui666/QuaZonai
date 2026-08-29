"""Mark pre-Nautilus approvals that cannot produce a valid Candidate Bundle."""

from __future__ import annotations

import json

import sqlalchemy as sa
from alembic import op

revision = "0008_stale_legacy_approvals"
down_revision = "0007_promotion_binding_hardening"
branch_labels = None
depends_on = None


def _mapping(value: object) -> dict[str, object]:
    if isinstance(value, dict):
        return value
    if isinstance(value, str):
        try:
            decoded = json.loads(value)
        except (TypeError, ValueError):
            return {}
        return decoded if isinstance(decoded, dict) else {}
    return {}


def upgrade() -> None:
    bind = op.get_bind()
    approvals = sa.table(
        "approval_snapshots",
        sa.column("id", sa.Uuid()),
        sa.column("candidate_id", sa.Uuid()),
        sa.column("state", sa.String()),
        sa.column("stale_reason", sa.Text()),
        sa.column("revision", sa.Integer()),
    )
    candidates = sa.table(
        "portfolio_candidates",
        sa.column("id", sa.Uuid()),
        sa.column("metrics", sa.JSON()),
    )
    legacy_ids = [
        row.id
        for row in bind.execute(
            sa.select(approvals.c.id, candidates.c.metrics)
            .select_from(approvals.join(candidates, approvals.c.candidate_id == candidates.c.id))
            .where(approvals.c.state == "PENDING")
        ).all()
        if not isinstance(_mapping(row.metrics).get("nautilus"), dict)
    ]
    for approval_id in legacy_ids:
        bind.execute(
            sa.update(approvals)
            .where(approvals.c.id == approval_id)
            .values(
                state="STALE",
                stale_reason="NAUTILUS_RUNTIME_EVIDENCE_REQUIRED",
                revision=approvals.c.revision + 1,
            )
        )


def downgrade() -> None:
    # Reopening an approval that was intentionally invalidated would violate its history.
    pass
