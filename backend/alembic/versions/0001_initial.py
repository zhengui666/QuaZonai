"""Frozen original QuantFoundry schema baseline.

Revision ID: 0001_initial
Revises: None
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import JSONB

revision = "0001_initial"
down_revision = None
branch_labels = None
depends_on = None

_JSON = sa.JSON().with_variant(JSONB(), "postgresql")
_IDENTITY_INT = sa.BigInteger().with_variant(sa.Integer(), "sqlite")


def _legacy_metadata() -> sa.MetaData:
    metadata = sa.MetaData()

    sa.Table(
        'operation_receipts',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('actor_kind', sa.String(length=40), nullable=False),
        sa.Column('actor_id', sa.String(length=500), nullable=False),
        sa.Column('idempotency_key', sa.Uuid(), nullable=False),
        sa.Column('operation_name', sa.String(length=200), nullable=False),
        sa.Column('target_type', sa.String(length=100)),
        sa.Column('target_id', sa.Uuid()),
        sa.Column('normalized_arguments', _JSON, nullable=False),
        sa.Column('state', sa.String(length=20), nullable=False),
        sa.Column('result', _JSON),
        sa.Column('error_code', sa.String(length=100)),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('completed_at', sa.DateTime(timezone=True)),
        sa.CheckConstraint("state IN ('IN_PROGRESS','SUCCEEDED','FAILED')", name='ck_operation_receipt_state'),
        sa.UniqueConstraint('actor_kind', 'actor_id', 'idempotency_key', name='uq_operation_receipt_key'),
    )

    sa.Table(
        'agent_artifacts',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('owner_issuer', sa.String(length=500), nullable=False),
        sa.Column('owner_subject', sa.String(length=500), nullable=False),
        sa.Column('owner_client_id', sa.String(length=500), nullable=False),
        sa.Column('kind', sa.String(length=40), nullable=False),
        sa.Column('filename', sa.String(length=255), nullable=False),
        sa.Column('state', sa.String(length=20), nullable=False),
        sa.Column('size_declared', sa.BigInteger(), nullable=False),
        sa.Column('size_received', sa.BigInteger(), nullable=False),
        sa.Column('relative_path', sa.Text(), nullable=False),
        sa.Column('expires_at', sa.DateTime(timezone=True), nullable=False),
        sa.Column('consumed_by_type', sa.String(length=100)),
        sa.Column('consumed_by_id', sa.Uuid()),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.CheckConstraint("state IN ('STAGING','READY','CONSUMED','FAILED','EXPIRED')", name='ck_agent_artifact_state'),
        sa.CheckConstraint("kind IN ('STRATEGY_SOURCE','PLUGIN_WHEEL','PARQUET_L2')", name='ck_agent_artifact_kind'),
        sa.Index('ix_agent_artifact_owner_state', 'owner_issuer', 'owner_subject', 'owner_client_id', 'state'),
    )

    sa.Table(
        'agent_impact_tokens',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('issuer', sa.String(length=500), nullable=False),
        sa.Column('subject', sa.String(length=500), nullable=False),
        sa.Column('client_id', sa.String(length=500), nullable=False),
        sa.Column('operation_name', sa.String(length=200), nullable=False),
        sa.Column('target_type', sa.String(length=100), nullable=False),
        sa.Column('target_id', sa.Uuid(), nullable=False),
        sa.Column('expected_state', _JSON, nullable=False),
        sa.Column('impact_summary', _JSON, nullable=False),
        sa.Column('expires_at', sa.DateTime(timezone=True), nullable=False),
        sa.Column('consumed_at', sa.DateTime(timezone=True)),
    )

    sa.Table(
        'mcp_task_bindings',
        metadata,
        sa.Column('task_id', sa.String(length=300), primary_key=True, nullable=False),
        sa.Column('issuer', sa.String(length=500), nullable=False),
        sa.Column('subject', sa.String(length=500), nullable=False),
        sa.Column('client_id', sa.String(length=500), nullable=False),
        sa.Column('extension_version', sa.String(length=100), nullable=False),
        sa.Column('operation_type', sa.String(length=100), nullable=False),
        sa.Column('operation_id', sa.Uuid(), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('expires_at', sa.DateTime(timezone=True), nullable=False),
    )

    sa.Table(
        'deployments',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('research_id', sa.Uuid(), sa.ForeignKey('research_cases.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('strategy_version_id', sa.Uuid(), sa.ForeignKey('strategy_versions.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('data_source_id', sa.Uuid(), sa.ForeignKey('data_sources.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('execution_connection_id', sa.Uuid(), sa.ForeignKey('execution_connections.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('runtime_bundle_id', sa.Uuid(), sa.ForeignKey('plugin_runtime_bundles.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('funder_id', sa.String(length=300), nullable=False),
        sa.Column('desired_state', sa.String(length=20), nullable=False),
        sa.Column('observed_state', sa.String(length=30), nullable=False),
        sa.Column('active_revision_id', sa.Uuid(), sa.ForeignKey('deployment_universe_revisions.id', ondelete='SET NULL', name='fk_deployment_active_revision', use_alter=True)),
        sa.Column('generation', sa.Integer(), nullable=False),
        sa.Column('last_error', sa.Text()),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.CheckConstraint("observed_state IN ('CREATED','STARTING','RUNNING','STOPPING','STOPPED','RECOVERY_BLOCKED','FAILED')", name='ck_deployment_observed_state'),
        sa.CheckConstraint("desired_state IN ('CREATED','RUNNING','STOPPED')", name='ck_deployment_desired_state'),
        sa.Index('ix_deployment_state', 'desired_state', 'observed_state'),
        sa.Index('ix_deployment_funder', 'funder_id'),
    )

    sa.Table(
        'deployment_generations',
        metadata,
        sa.Column('deployment_id', sa.Uuid(), sa.ForeignKey('deployments.id', ondelete='CASCADE'), primary_key=True, nullable=False),
        sa.Column('generation', sa.Integer(), primary_key=True, nullable=False),
        sa.Column('state', sa.String(length=30), nullable=False),
        sa.Column('runner_pid', sa.Integer()),
        sa.Column('last_error', sa.Text()),
        sa.Column('started_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('last_heartbeat_at', sa.DateTime(timezone=True)),
        sa.Column('stopped_at', sa.DateTime(timezone=True)),
        sa.CheckConstraint("state IN ('RECOVERY','RECONCILED','ARMED','STRATEGY_READY','TRADING','STOPPING','STOPPED','RECOVERY_BLOCKED','FAILED')", name='ck_deployment_generation_state'),
        sa.Index('ix_deployment_generation_state', 'state', 'last_heartbeat_at'),
    )

    sa.Table(
        'deployment_universe_revisions',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('deployment_id', sa.Uuid(), sa.ForeignKey('deployments.id', ondelete='CASCADE'), nullable=False),
        sa.Column('revision_no', sa.Integer(), nullable=False),
        sa.Column('predicate', _JSON, nullable=False),
        sa.Column('cap', sa.Integer(), nullable=False),
        sa.Column('state', sa.String(length=20), nullable=False),
        sa.Column('approval_id', sa.Uuid(), sa.ForeignKey('approvals.id', ondelete='RESTRICT')),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.CheckConstraint("state IN ('PENDING','APPROVED','ACTIVE','SUPERSEDED','REJECTED')", name='ck_universe_revision_state'),
        sa.UniqueConstraint('deployment_id', 'revision_no', name='uq_universe_revision_no'),
    )

    sa.Table(
        'deployment_instruments',
        metadata,
        sa.Column('deployment_id', sa.Uuid(), sa.ForeignKey('deployments.id', ondelete='CASCADE'), primary_key=True, nullable=False),
        sa.Column('revision_id', sa.Uuid(), sa.ForeignKey('deployment_universe_revisions.id', ondelete='CASCADE'), primary_key=True, nullable=False),
        sa.Column('instrument_id', sa.String(length=300), primary_key=True, nullable=False),
        sa.Column('lifecycle_state', sa.String(length=30), nullable=False),
        sa.Column('risk_limit_micros', sa.BigInteger(), nullable=False),
        sa.Column('last_reconciled_at', sa.DateTime(timezone=True)),
        sa.CheckConstraint("lifecycle_state IN ('PENDING','ACTIVE','EXIT_ONLY','RECOVERY_ONLY','RESOLVED')", name='ck_deployment_instrument_state'),
        sa.CheckConstraint('risk_limit_micros >= 0', name='ck_deployment_instrument_risk_limit'),
    )

    sa.Table(
        'plugin_releases',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('plugin_id', sa.String(length=200), nullable=False),
        sa.Column('distribution_name', sa.String(length=200), nullable=False),
        sa.Column('version', sa.String(length=100), nullable=False),
        sa.Column('api_version', sa.String(length=50), nullable=False),
        sa.Column('state', sa.String(length=32), nullable=False),
        sa.Column('is_default', sa.Boolean(), nullable=False),
        sa.Column('descriptor_snapshot', _JSON, nullable=False),
        sa.Column('last_error', sa.Text()),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('activated_at', sa.DateTime(timezone=True)),
        sa.Column('removed_at', sa.DateTime(timezone=True)),
        sa.UniqueConstraint('plugin_id', 'version', name='uq_plugin_release_version'),
        sa.CheckConstraint("state IN ('RECEIVED','INSTALLING','VALIDATING','STAGED','ACTIVE','DRAINING','INACTIVE','REMOVING','REMOVED','FAILED')", name='ck_plugin_release_state'),
        sa.Index('uq_plugin_release_default', 'plugin_id', unique=True, postgresql_where=sa.text('is_default'), sqlite_where=sa.text('is_default')),
        sa.Index('ix_plugin_release_lookup', 'plugin_id', 'state', 'is_default'),
    )

    sa.Table(
        'plugin_artifacts',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('plugin_release_id', sa.Uuid(), sa.ForeignKey('plugin_releases.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('role', sa.String(length=20), nullable=False),
        sa.Column('filename', sa.String(length=255), nullable=False),
        sa.Column('relative_path', sa.Text(), nullable=False),
        sa.Column('package_name', sa.String(length=200), nullable=False),
        sa.Column('package_version', sa.String(length=100), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.UniqueConstraint('plugin_release_id', 'filename', name='uq_plugin_artifact_filename'),
        sa.CheckConstraint("role IN ('PRIMARY','DEPENDENCY')", name='ck_plugin_artifact_role'),
    )

    sa.Table(
        'plugin_runtime_bundles',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('state', sa.String(length=20), nullable=False),
        sa.Column('python_version', sa.String(length=50), nullable=False),
        sa.Column('qf_version', sa.String(length=50), nullable=False),
        sa.Column('nautilus_version', sa.String(length=100)),
        sa.Column('environment_path', sa.Text(), nullable=False),
        sa.Column('last_error', sa.Text()),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('ready_at', sa.DateTime(timezone=True)),
        sa.Column('removed_at', sa.DateTime(timezone=True)),
        sa.CheckConstraint("state IN ('BUILDING','READY','FAILED','STALE','REMOVED')", name='ck_plugin_runtime_bundle_state'),
    )

    sa.Table(
        'plugin_runtime_bundle_members',
        metadata,
        sa.Column('runtime_bundle_id', sa.Uuid(), sa.ForeignKey('plugin_runtime_bundles.id', ondelete='CASCADE'), primary_key=True, nullable=False),
        sa.Column('plugin_release_id', sa.Uuid(), sa.ForeignKey('plugin_releases.id', ondelete='RESTRICT'), primary_key=True, nullable=False),
        sa.Column('member_role', sa.String(length=20), primary_key=True, nullable=False),
        sa.CheckConstraint("member_role IN ('DATA','EXECUTION','IMPORTER','AUXILIARY')", name='ck_plugin_bundle_member_role'),
        sa.Index('ix_plugin_bundle_member_release', 'plugin_release_id'),
    )

    sa.Table(
        'credential_sets',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('plugin_release_id', sa.Uuid(), sa.ForeignKey('plugin_releases.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('name', sa.String(length=200), nullable=False),
        sa.Column('public_config', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.UniqueConstraint('plugin_release_id', 'name', name='uq_credential_set_name'),
    )

    sa.Table(
        'credential_secrets',
        metadata,
        sa.Column('credential_set_id', sa.Uuid(), sa.ForeignKey('credential_sets.id', ondelete='CASCADE'), primary_key=True, nullable=False),
        sa.Column('field_name', sa.String(length=200), primary_key=True, nullable=False),
        sa.Column('ciphertext', sa.LargeBinary(), nullable=False),
        sa.Column('nonce', sa.LargeBinary(), nullable=False),
        sa.Column('key_version', sa.Integer(), nullable=False),
    )

    sa.Table(
        'data_sources',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('plugin_release_id', sa.Uuid(), sa.ForeignKey('plugin_releases.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('credential_set_id', sa.Uuid(), sa.ForeignKey('credential_sets.id', ondelete='RESTRICT')),
        sa.Column('name', sa.String(length=200), nullable=False),
        sa.Column('config', _JSON, nullable=False),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.CheckConstraint("state IN ('ACTIVE','INACTIVE','BLOCKED_PLUGIN_REMOVED')", name='ck_data_source_state'),
        sa.UniqueConstraint('name', name='uq_data_source_name'),
    )

    sa.Table(
        'execution_connections',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('plugin_release_id', sa.Uuid(), sa.ForeignKey('plugin_releases.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('credential_set_id', sa.Uuid(), sa.ForeignKey('credential_sets.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('name', sa.String(length=200), nullable=False),
        sa.Column('config', _JSON, nullable=False),
        sa.Column('state', sa.String(length=40), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.CheckConstraint("state IN ('ACTIVE','INACTIVE','BLOCKED_PLUGIN_REMOVED')", name='ck_execution_connection_state'),
        sa.UniqueConstraint('name', name='uq_execution_connection_name'),
    )

    sa.Table(
        'catalog_datasets',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('data_source_id', sa.Uuid(), sa.ForeignKey('data_sources.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('instrument_id', sa.String(length=300), nullable=False),
        sa.Column('catalog_path', sa.Text(), nullable=False),
        sa.Column('metadata', _JSON, nullable=False),
        sa.Column('started_at', sa.DateTime(timezone=True)),
        sa.Column('ended_at', sa.DateTime(timezone=True)),
        sa.Column('row_count', sa.BigInteger()),
        sa.Column('state', sa.String(length=20), nullable=False),
        sa.Column('run_id', sa.Uuid(), sa.ForeignKey('runs.id', ondelete='SET NULL', name='fk_catalog_dataset_run', use_alter=True)),
        sa.CheckConstraint("state IN ('IMPORTING','READY','FAILED')", name='ck_catalog_dataset_state'),
        sa.Index('ix_catalog_dataset_source_state', 'data_source_id', 'state'),
    )

    sa.Table(
        'strategies',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('name', sa.String(length=200), nullable=False, unique=True),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
    )

    sa.Table(
        'strategy_versions',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('strategy_id', sa.Uuid(), sa.ForeignKey('strategies.id', ondelete='CASCADE'), nullable=False),
        sa.Column('version_no', sa.Integer(), nullable=False),
        sa.Column('source_text', sa.Text(), nullable=False),
        sa.Column('default_config', _JSON, nullable=False),
        sa.Column('objective_directions', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.UniqueConstraint('strategy_id', 'version_no', name='uq_strategy_version_no'),
    )

    sa.Table(
        'research_cases',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('title', sa.String(length=300), nullable=False),
        sa.Column('strategy_version_id', sa.Uuid(), sa.ForeignKey('strategy_versions.id', ondelete='RESTRICT')),
        sa.Column('state', sa.String(length=20), nullable=False),
        sa.Column('content_revision', sa.Integer(), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.CheckConstraint("state IN ('DRAFT','ACTIVE','REVIEW','CLOSED')", name='ck_research_case_state'),
        sa.Index('ix_research_case_state', 'state', 'updated_at'),
    )

    sa.Table(
        'research_section_revisions',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('research_id', sa.Uuid(), sa.ForeignKey('research_cases.id', ondelete='CASCADE'), nullable=False),
        sa.Column('section', sa.String(length=40), nullable=False),
        sa.Column('revision_no', sa.Integer(), nullable=False),
        sa.Column('markdown', sa.Text(), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.CheckConstraint("section IN ('HYPOTHESIS','MARKET_CONTEXT','DATA','METHOD','RESULTS','RISKS','CONCLUSION')", name='ck_research_section'),
        sa.UniqueConstraint('research_id', 'section', 'revision_no', name='uq_research_section_revision'),
    )

    sa.Table(
        'experiments',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('research_id', sa.Uuid(), sa.ForeignKey('research_cases.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('strategy_version_id', sa.Uuid(), sa.ForeignKey('strategy_versions.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('dataset_id', sa.Uuid(), sa.ForeignKey('catalog_datasets.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('runtime_bundle_id', sa.Uuid(), sa.ForeignKey('plugin_runtime_bundles.id', ondelete='RESTRICT'), nullable=False),
        sa.Column('train_start', sa.DateTime(timezone=True), nullable=False),
        sa.Column('train_end', sa.DateTime(timezone=True), nullable=False),
        sa.Column('holdout_start', sa.DateTime(timezone=True), nullable=False),
        sa.Column('holdout_end', sa.DateTime(timezone=True), nullable=False),
        sa.Column('seed', sa.Integer(), nullable=False),
        sa.Column('objective_directions', _JSON, nullable=False),
        sa.Column('optuna_study_name', sa.String(length=300), unique=True),
        sa.Column('selected_trial_no', sa.Integer()),
        sa.CheckConstraint('train_start < train_end', name='ck_experiment_train_range'),
        sa.CheckConstraint('train_end <= holdout_start OR holdout_end <= train_start', name='ck_experiment_non_overlapping_ranges'),
        sa.CheckConstraint('holdout_start < holdout_end', name='ck_experiment_holdout_range'),
        sa.Index('ix_experiment_research', 'research_id', 'id'),
    )

    sa.Table(
        'runs',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('experiment_id', sa.Uuid(), sa.ForeignKey('experiments.id', ondelete='RESTRICT')),
        sa.Column('runtime_bundle_id', sa.Uuid(), sa.ForeignKey('plugin_runtime_bundles.id', ondelete='RESTRICT')),
        sa.Column('type', sa.String(length=30), nullable=False),
        sa.Column('state', sa.String(length=20), nullable=False),
        sa.Column('attempt', sa.Integer(), nullable=False),
        sa.Column('summary', _JSON, nullable=False),
        sa.Column('error_code', sa.String(length=100)),
        sa.Column('error_message', sa.Text()),
        sa.Column('started_at', sa.DateTime(timezone=True)),
        sa.Column('finished_at', sa.DateTime(timezone=True)),
        sa.CheckConstraint("type IN ('PARQUET_IMPORT','BACKTEST','OPTIMIZATION','HOLDOUT')", name='ck_run_type'),
        sa.CheckConstraint("state IN ('QUEUED','RUNNING','SUCCEEDED','FAILED','CANCELLED')", name='ck_run_state'),
        sa.Index('ix_runs_experiment_state', 'experiment_id', 'state'),
    )

    sa.Table(
        'reports',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('run_id', sa.Uuid(), sa.ForeignKey('runs.id', ondelete='CASCADE'), nullable=False),
        sa.Column('kind', sa.String(length=100), nullable=False),
        sa.Column('relative_path', sa.Text(), nullable=False),
        sa.Column('media_type', sa.String(length=200), nullable=False),
        sa.Column('row_count', sa.BigInteger()),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.UniqueConstraint('run_id', 'kind', name='uq_report_run_kind'),
    )

    sa.Table(
        'approvals',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('type', sa.String(length=40), nullable=False),
        sa.Column('resource_type', sa.String(length=100), nullable=False),
        sa.Column('resource_id', sa.Uuid(), nullable=False),
        sa.Column('scope', _JSON, nullable=False),
        sa.Column('state', sa.String(length=20), nullable=False),
        sa.Column('reason', sa.Text()),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('decided_at', sa.DateTime(timezone=True)),
        sa.CheckConstraint("type IN ('DEPLOYMENT_START','UNIVERSE_EXPANSION')", name='ck_approval_type'),
        sa.CheckConstraint("state IN ('PENDING','APPROVED','REJECTED')", name='ck_approval_state'),
        sa.Index('ix_approval_resource_state', 'resource_type', 'resource_id', 'state'),
    )

    sa.Table(
        'risk_accounts',
        metadata,
        sa.Column('funder_id', sa.String(length=300), primary_key=True, nullable=False),
        sa.Column('status', sa.String(length=20), nullable=False),
        sa.Column('gross_limit_micros', sa.BigInteger(), nullable=False),
        sa.Column('owner_deployment_id', sa.Uuid(), sa.ForeignKey('deployments.id', ondelete='SET NULL')),
        sa.Column('owner_generation', sa.Integer()),
        sa.Column('last_reconciled_at', sa.DateTime(timezone=True)),
        sa.Column('last_heartbeat_at', sa.DateTime(timezone=True)),
        sa.CheckConstraint('gross_limit_micros >= 0', name='ck_risk_account_limit'),
        sa.CheckConstraint("status IN ('BLOCKED','RECOVERING','RECONCILING','READY','STOPPED')", name='ck_risk_account_status'),
    )

    sa.Table(
        'risk_positions',
        metadata,
        sa.Column('funder_id', sa.String(length=300), sa.ForeignKey('risk_accounts.funder_id', ondelete='CASCADE'), primary_key=True, nullable=False),
        sa.Column('instrument_id', sa.String(length=300), primary_key=True, nullable=False),
        sa.Column('entry_cost_micros', sa.BigInteger(), nullable=False),
        sa.Column('observed_at', sa.DateTime(timezone=True), nullable=False),
        sa.CheckConstraint('entry_cost_micros >= 0', name='ck_risk_position_cost'),
    )

    sa.Table(
        'risk_open_orders',
        metadata,
        sa.Column('funder_id', sa.String(length=300), sa.ForeignKey('risk_accounts.funder_id', ondelete='CASCADE'), primary_key=True, nullable=False),
        sa.Column('client_order_id', sa.String(length=300), primary_key=True, nullable=False),
        sa.Column('instrument_id', sa.String(length=300), nullable=False),
        sa.Column('increase_debit_micros', sa.BigInteger(), nullable=False),
        sa.Column('state', sa.String(length=30), nullable=False),
        sa.Column('observed_at', sa.DateTime(timezone=True), nullable=False),
        sa.CheckConstraint('increase_debit_micros >= 0', name='ck_risk_open_order_debit'),
        sa.CheckConstraint("state IN ('OPEN','PENDING_CANCEL','UNKNOWN')", name='ck_risk_open_order_state'),
    )

    sa.Table(
        'risk_reservations',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('funder_id', sa.String(length=300), sa.ForeignKey('risk_accounts.funder_id', ondelete='CASCADE'), nullable=False),
        sa.Column('runner_generation', sa.Integer(), nullable=False),
        sa.Column('client_order_id', sa.String(length=300), nullable=False),
        sa.Column('instrument_id', sa.String(length=300), nullable=False),
        sa.Column('reserved_micros', sa.BigInteger(), nullable=False),
        sa.Column('state', sa.String(length=20), nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('resolved_at', sa.DateTime(timezone=True)),
        sa.CheckConstraint("state IN ('PENDING','SUBMITTED','REJECTED','RELEASED','UNKNOWN')", name='ck_risk_reservation_state'),
        sa.UniqueConstraint('funder_id', 'runner_generation', 'client_order_id', name='uq_risk_reservation_order'),
        sa.CheckConstraint('reserved_micros >= 0', name='ck_risk_reservation_amount'),
        sa.Index('ix_risk_reservation_state', 'funder_id', 'state'),
    )

    sa.Table(
        'risk_events',
        metadata,
        sa.Column('id', _IDENTITY_INT, primary_key=True, nullable=False, autoincrement=True),
        sa.Column('funder_id', sa.String(length=300), sa.ForeignKey('risk_accounts.funder_id', ondelete='CASCADE'), nullable=False),
        sa.Column('kind', sa.String(length=100), nullable=False),
        sa.Column('payload', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
    )

    sa.Table(
        'jobs',
        metadata,
        sa.Column('id', sa.Uuid(), primary_key=True, nullable=False),
        sa.Column('kind', sa.String(length=100), nullable=False),
        sa.Column('resource_type', sa.String(length=100), nullable=False),
        sa.Column('resource_id', sa.Uuid(), nullable=False),
        sa.Column('state', sa.String(length=20), nullable=False),
        sa.Column('payload', _JSON, nullable=False),
        sa.Column('attempt', sa.Integer(), nullable=False),
        sa.Column('available_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('lease_owner', sa.String(length=200)),
        sa.Column('lease_expires_at', sa.DateTime(timezone=True)),
        sa.Column('last_error', sa.Text()),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column('updated_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.CheckConstraint("state IN ('READY','LEASED','SUCCEEDED','FAILED','CANCELLED')", name='ck_job_state'),
        sa.Index('ix_jobs_ready', 'state', 'available_at'),
    )

    sa.Table(
        'events',
        metadata,
        sa.Column('id', _IDENTITY_INT, primary_key=True, nullable=False, autoincrement=True),
        sa.Column('kind', sa.String(length=100), nullable=False),
        sa.Column('aggregate_type', sa.String(length=100), nullable=False),
        sa.Column('aggregate_id', sa.Uuid()),
        sa.Column('actor_kind', sa.String(length=40), nullable=False),
        sa.Column('actor_metadata', _JSON, nullable=False),
        sa.Column('payload', _JSON, nullable=False),
        sa.Column('created_at', sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Index('ix_events_id', 'id'),
    )

    return metadata



def upgrade() -> None:
    _legacy_metadata().create_all(bind=op.get_bind(), checkfirst=True)


def downgrade() -> None:
    _legacy_metadata().drop_all(bind=op.get_bind(), checkfirst=True)
