"""Persist operator-managed Codex and worker runtime configuration.

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


def upgrade() -> None:
    op.create_table(
        "runtime_configurations",
        sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
        sa.Column("scope", sa.String(length=40), nullable=False),
        sa.Column("codex_model", sa.String(length=200), nullable=True),
        sa.Column("codex_base_url", sa.Text(), nullable=True),
        sa.Column("codex_api_key_ciphertext", sa.LargeBinary(), nullable=True),
        sa.Column("codex_api_key_nonce", sa.LargeBinary(), nullable=True),
        sa.Column("codex_api_key_key_version", sa.Integer(), nullable=True),
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
        sa.CheckConstraint(
            "max_plugin_wheel_bytes > 0",
            name="ck_runtime_max_plugin_wheel_bytes",
        ),
        sa.CheckConstraint(
            "plugin_validation_timeout_seconds > 0",
            name="ck_runtime_plugin_validation_timeout",
        ),
        sa.CheckConstraint(
            "bundle_build_timeout_seconds > 0",
            name="ck_runtime_bundle_build_timeout",
        ),
        sa.CheckConstraint(
            "plugin_job_timeout_seconds > 0",
            name="ck_runtime_plugin_job_timeout",
        ),
        sa.CheckConstraint(
            "mission_job_timeout_seconds > 0",
            name="ck_runtime_mission_job_timeout",
        ),
        sa.CheckConstraint("job_poll_seconds > 0", name="ck_runtime_job_poll_seconds"),
        sa.CheckConstraint("job_lease_seconds > 0", name="ck_runtime_job_lease_seconds"),
        sa.UniqueConstraint("scope", name="uq_runtime_configuration_scope"),
    )


def downgrade() -> None:
    op.drop_table("runtime_configurations")
