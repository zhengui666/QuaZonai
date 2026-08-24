"""Enforce the Research Intelligence / Portfolio Construction ownership boundary.

Revision ID: 0002_research_intelligence_boundary
Revises: 0001_initial
"""

from __future__ import annotations

from alembic import op

from db.models import Base

revision = "0002_research_intelligence_boundary"
down_revision = "0001_initial"
branch_labels = None
depends_on = None


LEGACY_TABLES_CHILD_FIRST = (
    "risk_events",
    "risk_reservations",
    "risk_open_orders",
    "risk_positions",
    "risk_accounts",
    "deployment_instruments",
    "deployment_generations",
    "deployment_universe_revisions",
    "deployments",
    "reports",
    "catalog_datasets",
    "runs",
    "experiments",
    "research_section_revisions",
    "research_cases",
    "strategy_versions",
    "strategies",
    "approvals",
    "execution_connections",
    "data_sources",
)


def upgrade() -> None:
    bind = op.get_bind()

    # Existing installations may already be stamped at 0001 while lacking the new
    # DESIGN domain. Create only missing current tables before removing legacy ones.
    Base.metadata.create_all(bind=bind, checkfirst=True)

    if bind.dialect.name == "postgresql":
        op.execute(
            "DELETE FROM plugin_runtime_bundle_members WHERE member_role = 'EXECUTION'"
        )
        op.execute(
            "ALTER TABLE plugin_runtime_bundle_members "
            "DROP CONSTRAINT IF EXISTS ck_plugin_bundle_member_role"
        )
        op.execute(
            "ALTER TABLE plugin_runtime_bundle_members ADD CONSTRAINT "
            "ck_plugin_bundle_member_role CHECK "
            "(member_role IN ('RESEARCH','DATA','IMPORTER','AUXILIARY'))"
        )
        op.execute(
            "ALTER TABLE plugin_runtime_bundles DROP COLUMN IF EXISTS nautilus_version"
        )
        for table in LEGACY_TABLES_CHILD_FIRST:
            op.execute(f'DROP TABLE IF EXISTS "{table}" CASCADE')
    else:
        # SQLite is used only by unit/integration tests. Fresh test databases are
        # generated from current metadata; the boundary cleanup is PostgreSQL-owned.
        for table in LEGACY_TABLES_CHILD_FIRST:
            op.execute(f'DROP TABLE IF EXISTS "{table}"')


def downgrade() -> None:
    raise RuntimeError(
        "0002_research_intelligence_boundary is intentionally irreversible: "
        "downgrading would restore QuaZonai-owned execution state outside DESIGN.md."
    )
