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

from sqlalchemy.dialects.postgresql import JSONB

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

_JSON = sa.JSON().with_variant(JSONB(), "postgresql")


def _research_boundary_metadata() -> sa.MetaData:
    metadata = sa.MetaData()

    sa.Table(
        'public_mutation_receipts',
        metadata,
        sa.Column('idempotency_key', sa.String(length=200), primary_key=True, nullable=False),
        sa.Column('operation_name', sa.String(length=200), nullable=False),
        sa.Column('normalized_request', _JSON, nullable=False),
        sa.Column('response_json', _JSON, nullable=False),
        sa.Column('status_code', sa.Integer(), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False),
    )

    sa.Table(
        'research_charters',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('original_idea_text', sa.Text(), nullable=False),
        sa.Column('research_question', sa.Text(), nullable=False),
        sa.Column('market_scope', _JSON, nullable=False),
        sa.Column('universe_version_ids', _JSON, nullable=False),
        sa.Column('prediction_horizon', sa.String(length=100)),
        sa.Column('allowed_data_domains', _JSON, nullable=False),
        sa.Column('explicit_exclusions', _JSON, nullable=False),
        sa.Column('material_assumptions', _JSON, nullable=False),
        sa.Column('system_assumptions', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False),
    )

    sa.Table(
        'research_programs',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('charter_id', sa.Uuid(), sa.ForeignKey('research_charters.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('title', sa.String(length=240), nullable=False),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('cooling_reason', sa.Text()),
        sa.Column('blocked_reason', sa.Text()),
        sa.Column('wake_reason', sa.Text()),
        sa.Column('revision', sa.Integer(), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Index('ix_research_program_state', 'state'),
    )

    sa.Table(
        'research_branches',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('program_id', sa.Uuid(), sa.ForeignKey('research_programs.id', ondelete='CASCADE'), nullable=False),
        sa.Column('parent_branch_id', sa.Uuid(), sa.ForeignKey('research_branches.id', ondelete='RESTRICT')),
        sa.Column('derivation_type', sa.String(length=80), nullable=False),
        sa.Column('hypothesis', sa.Text(), nullable=False),
        sa.Column('changed_assumptions', _JSON, nullable=False),
        sa.Column('preserved_constraints', _JSON, nullable=False),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False),
        sa.Index('ix_research_branch_program', 'program_id'),
    )

    sa.Table(
        'research_missions',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('program_id', sa.Uuid(), sa.ForeignKey('research_programs.id', ondelete='CASCADE'), nullable=False),
        sa.Column('branch_id', sa.Uuid(), sa.ForeignKey('research_branches.id', ondelete='CASCADE'), nullable=False),
        sa.Column('type', sa.String(length=100), nullable=False),
        sa.Column('role', sa.String(length=100)),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('objective', sa.Text()),
        sa.Column('dependencies', _JSON, nullable=False),
        sa.Column('started_at', sa.DateTime(timezone=True)),
        sa.Column('finished_at', sa.DateTime(timezone=True)),
        sa.Column('attempt', sa.Integer(), nullable=False),
        sa.Column('error_code', sa.String(length=100)),
        sa.Column('summary', sa.Text()),
        sa.Index('ix_research_mission_program_state', 'program_id', 'state'),
    )

    sa.Table(
        'market_universe_versions',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('universe_key', sa.String(length=120), nullable=False),
        sa.Column('version_no', sa.Integer(), nullable=False),
        sa.Column('name', sa.String(length=200), nullable=False),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('spec_json', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False),
        sa.UniqueConstraint('universe_key', 'version_no', name='uq_market_universe_version'),
    )

    sa.Table(
        'governed_data_sources',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('name', sa.String(length=200), nullable=False),
        sa.Column('provider', sa.String(length=200)),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('universe_scope', _JSON, nullable=False),
        sa.Column('fields', _JSON, nullable=False),
        sa.Column('update_cadence', sa.String(length=100)),
        sa.Column('preflight_state', sa.String(length=40), nullable=False),
        sa.Column('public_config', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.UniqueConstraint('name', name='uq_governed_data_source_name'),
    )

    sa.Table(
        'dataset_revisions',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('data_source_id', sa.Uuid(), sa.ForeignKey('governed_data_sources.id', ondelete='RESTRICT')),
        sa.Column('universe_version_id', sa.Uuid(), sa.ForeignKey('market_universe_versions.id', ondelete='RESTRICT')),
        sa.Column('universe_name', sa.String(length=200)),
        sa.Column('revision_no', sa.Integer(), nullable=False),
        sa.Column('schema_version', sa.String(length=100)),
        sa.Column('event_start', sa.DateTime(timezone=True)),
        sa.Column('event_end', sa.DateTime(timezone=True)),
        sa.Column('available_start', sa.DateTime(timezone=True)),
        sa.Column('available_end', sa.DateTime(timezone=True)),
        sa.Column('row_count', sa.Integer()),
        sa.Column('quality_state', sa.String(length=40), nullable=False),
        sa.Column('point_in_time_state', sa.String(length=40), nullable=False),
        sa.Column('partition', sa.String(length=40), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False),
        sa.Index('ix_dataset_revision_source', 'data_source_id'),
    )

    sa.Table(
        'alpha_qualifications',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('program_id', sa.Uuid(), sa.ForeignKey('research_programs.id', ondelete='SET NULL')),
        sa.Column('alpha_model_version_id', sa.Uuid()),
        sa.Column('calibration_version_id', sa.Uuid()),
        sa.Column('universe_version_id', sa.Uuid(), sa.ForeignKey('market_universe_versions.id', ondelete='RESTRICT')),
        sa.Column('universe', sa.String(length=200)),
        sa.Column('horizon', sa.String(length=100)),
        sa.Column('role', sa.String(length=100), nullable=False),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('name', sa.String(length=240)),
        sa.Column('scope_json', _JSON, nullable=False),
        sa.Column('evaluation_episode_id', sa.Uuid()),
        sa.Column('degradation_state', sa.String(length=40), nullable=False),
        sa.Column('metrics', _JSON, nullable=False),
        sa.Column('lineage', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False),
        sa.Index('ix_alpha_qualification_state', 'state'),
    )

    sa.Table(
        'portfolio_mandates',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('key', sa.String(length=120), nullable=False),
        sa.Column('name', sa.String(length=200), nullable=False),
        sa.Column('enabled', sa.Boolean(), nullable=False),
        sa.Column('latest_version_id', sa.Uuid(), nullable=False),
        sa.Column('spec_json', _JSON, nullable=False),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.UniqueConstraint('key', name='uq_portfolio_mandate_key'),
    )

    sa.Table(
        'portfolio_programs',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('mandate_version_id', sa.Uuid(), nullable=False),
        sa.Column('mandate_name', sa.String(length=200)),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('current_candidate_id', sa.Uuid()),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
    )

    sa.Table(
        'portfolio_candidates',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('candidate_family_id', sa.Uuid()),
        sa.Column('portfolio_program_id', sa.Uuid(), sa.ForeignKey('portfolio_programs.id', ondelete='CASCADE'), nullable=False),
        sa.Column('mandate_version_id', sa.Uuid()),
        sa.Column('mandate_name', sa.String(length=200)),
        sa.Column('capital_context_version_id', sa.Uuid()),
        sa.Column('universe_set_json', _JSON, nullable=False),
        sa.Column('policy_version', sa.String(length=100)),
        sa.Column('risk_model_version', sa.String(length=100)),
        sa.Column('cost_model_version', sa.String(length=100)),
        sa.Column('capacity_model_version', sa.String(length=100)),
        sa.Column('constraint_set_version', sa.String(length=100)),
        sa.Column('rebalance_policy_version', sa.String(length=100)),
        sa.Column('evaluation_episode_id', sa.Uuid()),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('members', _JSON, nullable=False),
        sa.Column('metrics', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False),
        sa.Index('ix_portfolio_candidate_program', 'portfolio_program_id'),
    )

    sa.Table(
        'downstream_systems',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('name', sa.String(length=200), nullable=False),
        sa.Column('environment_type', sa.String(length=40), nullable=False),
        sa.Column('enabled', sa.Boolean(), nullable=False),
        sa.Column('package_contract_version', sa.String(length=40), nullable=False),
        sa.Column('feedback_contract_version', sa.String(length=40), nullable=False),
        sa.Column('compatibility', _JSON, nullable=False),
        sa.Column('preflight_state', sa.String(length=40), nullable=False),
        sa.Column('public_config', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.UniqueConstraint('name', name='uq_downstream_system_name'),
    )

    sa.Table(
        'approval_snapshots',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('candidate_id', sa.Uuid(), sa.ForeignKey('portfolio_candidates.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('purpose', sa.String(length=40), nullable=False),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('downstream_system_id', sa.Uuid(), sa.ForeignKey('downstream_systems.id', ondelete='RESTRICT')),
        sa.Column('valid_until', sa.DateTime(timezone=True)),
        sa.Column('expires_at', sa.DateTime(timezone=True)),
        sa.Column('stale_reason', sa.Text()),
        sa.Column('recommendation_rationale', sa.Text()),
        sa.Column('human_report', _JSON, nullable=False),
        sa.Column('evidence_summary', _JSON, nullable=False),
        sa.Column('capital_context', _JSON, nullable=False),
        sa.Column('risk_summary', _JSON, nullable=False),
        sa.Column('cost_summary', _JSON, nullable=False),
        sa.Column('capacity_summary', _JSON, nullable=False),
        sa.Column('changes_summary', _JSON, nullable=False),
        sa.Column('revision', sa.Integer(), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Index('ix_approval_snapshot_state', 'state'),
    )

    sa.Table(
        'candidate_packages',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('approval_id', sa.Uuid(), sa.ForeignKey('approval_snapshots.id', ondelete='RESTRICT'), nullable=False, unique=True),
        sa.Column('candidate_id', sa.Uuid(), sa.ForeignKey('portfolio_candidates.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('contract_version', sa.String(length=40), nullable=False),
        sa.Column('payload', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False),
    )

    sa.Table(
        'handoff_offers',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('approval_id', sa.Uuid(), sa.ForeignKey('approval_snapshots.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('candidate_package_id', sa.Uuid(), sa.ForeignKey('candidate_packages.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('candidate_id', sa.Uuid(), sa.ForeignKey('portfolio_candidates.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('purpose', sa.String(length=40), nullable=False),
        sa.Column('downstream_system_id', sa.Uuid(), sa.ForeignKey('downstream_systems.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('claim_deadline', sa.DateTime(timezone=True)),
        sa.Column('stale_reason', sa.Text()),
        sa.Column('feedback_state', sa.String(length=40)),
        sa.Column('claimed_at', sa.DateTime(timezone=True)),
        sa.Column('accepted_at', sa.DateTime(timezone=True)),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Index('ix_handoff_offer_state', 'state'),
    )

    sa.Table(
        'forward_evidence_episodes',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('handoff_id', sa.Uuid(), sa.ForeignKey('handoff_offers.id', ondelete='CASCADE'), nullable=False),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('evidence', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False),
        sa.Index('ix_forward_evidence_handoff', 'handoff_id'),
    )

    return metadata



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

    # Existing installations may already be stamped at 0001 while lacking the
    # 0002 DESIGN domain. Create its frozen tables, then losslessly archive every
    # legacy row before the operational legacy tables are removed.
    _research_boundary_metadata().create_all(bind=bind, checkfirst=True)
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
