"""Persist the frozen 0004 runtime-configuration schema.

Revision ID: 0004_runtime_configuration
Revises: 0003_review_contracts
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op

revision = "0004_runtime_configuration"
down_revision = "0003_review_contracts"
branch_labels = None
depends_on = None


def _runtime_configuration_table() -> sa.Table:
    metadata = sa.MetaData()
    return sa.Table(
        "runtime_configurations",
        metadata,
        sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
        sa.Column("scope", sa.String(length=40), nullable=False),
        sa.Column("revision", sa.Integer(), nullable=False),
        sa.Column("codex_model", sa.String(length=200)),
        sa.Column("codex_base_url", sa.Text()),
        sa.Column("codex_api_key_ciphertext", sa.LargeBinary()),
        sa.Column("codex_api_key_nonce", sa.LargeBinary()),
        sa.Column("codex_api_key_key_version", sa.Integer()),
        sa.Column("max_plugin_wheel_bytes", sa.BigInteger(), nullable=False),
        sa.Column("plugin_validation_timeout_seconds", sa.Integer(), nullable=False),
        sa.Column("bundle_build_timeout_seconds", sa.Integer(), nullable=False),
        sa.Column("plugin_job_timeout_seconds", sa.Integer(), nullable=False),
        sa.Column("mission_job_timeout_seconds", sa.Integer(), nullable=False),
        sa.Column("job_poll_seconds", sa.Float(), nullable=False),
        sa.Column("job_lease_seconds", sa.Integer(), nullable=False),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.Column(
            "updated_at",
            sa.DateTime(timezone=True),
            nullable=False,
            server_default=sa.func.now(),
        ),
        sa.CheckConstraint("scope = 'SYSTEM'", name="ck_runtime_configuration_scope"),
        sa.CheckConstraint("revision > 0", name="ck_runtime_configuration_revision"),
        sa.CheckConstraint(
            "max_plugin_wheel_bytes > 0 AND max_plugin_wheel_bytes <= 1073741824",
            name="ck_runtime_max_plugin_wheel_bytes",
        ),
        sa.CheckConstraint(
            "plugin_validation_timeout_seconds > 0 AND plugin_validation_timeout_seconds <= 86400",
            name="ck_runtime_plugin_validation_timeout",
        ),
        sa.CheckConstraint(
            "bundle_build_timeout_seconds > 0 AND bundle_build_timeout_seconds <= 86400",
            name="ck_runtime_bundle_build_timeout",
        ),
        sa.CheckConstraint(
            "plugin_job_timeout_seconds > 0 AND plugin_job_timeout_seconds <= 86400",
            name="ck_runtime_plugin_job_timeout",
        ),
        sa.CheckConstraint(
            "mission_job_timeout_seconds > 0 AND mission_job_timeout_seconds <= 86400",
            name="ck_runtime_mission_job_timeout",
        ),
        sa.CheckConstraint(
            "job_poll_seconds >= 0.01 AND job_poll_seconds <= 3600",
            name="ck_runtime_job_poll_seconds",
        ),
        sa.CheckConstraint(
            "job_lease_seconds > 0 AND job_lease_seconds <= 86400",
            name="ck_runtime_job_lease_seconds",
        ),
        sa.UniqueConstraint("scope", name="uq_runtime_configuration_scope"),
    )


def upgrade() -> None:
    bind = op.get_bind()
    _runtime_configuration_table().create(bind=bind, checkfirst=True)

    columns = {column["name"] for column in sa.inspect(bind).get_columns("runtime_configurations")}
    if "revision" not in columns:
        op.add_column(
            "runtime_configurations",
            sa.Column("revision", sa.Integer(), nullable=False, server_default="1"),
        )
        op.create_check_constraint(
            "ck_runtime_configuration_revision",
            "runtime_configurations",
            "revision > 0",
        )


def downgrade() -> None:
    _runtime_configuration_table().drop(bind=op.get_bind(), checkfirst=True)
