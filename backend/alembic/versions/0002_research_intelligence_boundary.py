"""Enforce the Research Intelligence / Portfolio Construction ownership boundary.

Revision ID: 0002_research_boundary
Revises: 0001_initial
"""

from __future__ import annotations

import base64
from datetime import date, datetime
from decimal import Decimal
from typing import Any
from uuid import UUID

import sqlalchemy as sa
from alembic import op

from db.models import Base

revision = "0002_research_boundary"
down_revision = "0001_initial"
branch_labels = None
depends_on = None


LEGACY_TABLES_CHILD_FIRST = (
    "agent_impact_tokens",
    "agent_artifacts",
    "mcp_task_bindings",
    "operation_receipts",
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

ARCHIVE_TABLE = "legacy_boundary_archive"


def _json_value(value: Any) -> Any:
    if value is None or isinstance(value, (bool, int, float, str)):
        return value
    if isinstance(value, (UUID, datetime, date, Decimal)):
        return str(value)
    if isinstance(value, bytes):
        return {"encoding": "base64", "value": base64.b64encode(value).decode("ascii")}
    if isinstance(value, dict):
        return {str(key): _json_value(item) for key, item in value.items()}
    if isinstance(value, (list, tuple, set)):
        return [_json_value(item) for item in value]
    return str(value)


def _archive_legacy_rows(bind: sa.Connection) -> None:
    inspector = sa.inspect(bind)
    existing = set(inspector.get_table_names())
    if ARCHIVE_TABLE not in existing:
        op.create_table(
            ARCHIVE_TABLE,
            sa.Column("id", sa.BigInteger(), primary_key=True, autoincrement=True),
            sa.Column("source_table", sa.String(length=128), nullable=False),
            sa.Column("source_id", sa.String(length=256), nullable=True),
            sa.Column("payload", sa.JSON(), nullable=False),
            sa.Column(
                "archived_at",
                sa.DateTime(timezone=True),
                nullable=False,
                server_default=sa.func.now(),
            ),
        )
        existing.add(ARCHIVE_TABLE)

    archive = sa.Table(ARCHIVE_TABLE, sa.MetaData(), autoload_with=bind)
    metadata = sa.MetaData()
    for table_name in reversed(LEGACY_TABLES_CHILD_FIRST):
        if table_name not in existing:
            continue
        table = sa.Table(table_name, metadata, autoload_with=bind)
        primary_keys = [column.name for column in table.primary_key.columns]
        batch: list[dict[str, Any]] = []
        for row in bind.execute(sa.select(table)).mappings():
            source_id = None
            if primary_keys:
                source_id = ":".join(str(row.get(key, "")) for key in primary_keys)
            batch.append(
                {
                    "source_table": table_name,
                    "source_id": source_id,
                    "payload": {key: _json_value(value) for key, value in row.items()},
                }
            )
            if len(batch) >= 500:
                bind.execute(archive.insert(), batch)
                batch.clear()
        if batch:
            bind.execute(archive.insert(), batch)


def upgrade() -> None:
    bind = op.get_bind()

    # Existing installations may already be stamped at 0001 while lacking the new
    # DESIGN domain. Create missing current tables, then losslessly archive every
    # legacy row before the operational legacy tables are removed.
    Base.metadata.create_all(bind=bind, checkfirst=True)
    _archive_legacy_rows(bind)

    if bind.dialect.name == "postgresql":
        op.execute(
            "UPDATE plugin_runtime_bundles SET state = 'STALE' "
            "WHERE id IN (SELECT runtime_bundle_id FROM plugin_runtime_bundle_members "
            "WHERE member_role = 'EXECUTION')"
        )
        op.execute(
            "DELETE FROM plugin_runtime_bundle_members WHERE member_role = 'EXECUTION'"
        )
        op.execute(
            "UPDATE plugin_releases SET state = 'INACTIVE', is_default = FALSE, "
            "last_error = 'Legacy execution capability disabled by ownership migration' "
            "WHERE descriptor_snapshot::text LIKE '%\"EXECUTION\"%'"
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
        op.execute("ALTER TABLE plugin_runtime_bundles DROP COLUMN IF EXISTS nautilus_version")
        for table in LEGACY_TABLES_CHILD_FIRST:
            op.execute(f'DROP TABLE IF EXISTS "{table}" CASCADE')
    else:
        # SQLite remains a local fallback for isolated unit tests. Existing SQLite
        # fixtures are archived with the same lossless row representation first.
        for table in LEGACY_TABLES_CHILD_FIRST:
            op.execute(f'DROP TABLE IF EXISTS "{table}"')


def downgrade() -> None:
    raise RuntimeError(
        "0002_research_boundary is intentionally irreversible: downgrading would restore "
        "QuaZonai-owned execution state outside DESIGN.md. Legacy facts remain preserved "
        "in legacy_boundary_archive."
    )
