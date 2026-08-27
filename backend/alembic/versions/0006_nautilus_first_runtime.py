"""Mark the clean Nautilus-first schema boundary.

Revision ID: 0006_nautilus_first_runtime
Revises: 0005_performance_indexes

QuaZonai is still in the development phase and Issue #22 explicitly removes
compatibility obligations for the pre-Nautilus-first schema.  Fresh databases
are created by ``0001_initial`` from the current ``Base.metadata`` and already
contain the complete Candidate Bundle, Search Ledger, governed catalog lineage,
and experiment-lineage schema.

Do not translate the historical Candidate Package schema in place.  The
preflight gate rejects databases below this revision and requires a fresh
volume, which keeps the final architecture free of compatibility migrations.
"""

from __future__ import annotations

revision = "0006_nautilus_first_runtime"
down_revision = "0005_performance_indexes"
branch_labels = None
depends_on = None


def upgrade() -> None:
    """Fresh ``0001_initial`` metadata already represents this revision."""


def downgrade() -> None:
    raise RuntimeError(
        "The Nautilus-first schema is a reset boundary; create a fresh volume instead of downgrading."
    )
