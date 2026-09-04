"""Add frozen 0011 Codex model runtime controls and default-selection mode.

Revision ID: 0011_codex_runtime_controls
Revises: 0010_operator_auth_configuration
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op

revision = "0011_codex_runtime_controls"
down_revision = "0010_operator_auth_configuration"
branch_labels = None
depends_on = None

_REASONING_CONSTRAINT = "ck_runtime_configuration_codex_reasoning_effort"


def _runtime_configuration_table() -> sa.Table:
    metadata = sa.MetaData()
    return sa.Table(
        "runtime_configurations",
        metadata,
        sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
        sa.Column("scope", sa.String(length=40), nullable=False),
        sa.Column("revision", sa.Integer(), nullable=False),
        sa.Column("codex_model", sa.String(length=200)),
        sa.Column("codex_reasoning_effort", sa.String(length=16)),
        sa.Column(
            "codex_fast_mode",
            sa.Boolean(),
            nullable=False,
            server_default=sa.text("false"),
        ),
        sa.Column(
            "codex_use_default_model_settings",
            sa.Boolean(),
            nullable=False,
            server_default=sa.text("false"),
        ),
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
            "codex_reasoning_effort IS NULL OR codex_reasoning_effort IN "
            "('minimal', 'low', 'medium', 'high', 'xhigh')",
            name=_REASONING_CONSTRAINT,
        ),
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


def _check_constraint_exists(bind: sa.Connection, name: str) -> bool:
    return any(
        constraint.get("name") == name
        for constraint in sa.inspect(bind).get_check_constraints("runtime_configurations")
    )


def upgrade() -> None:
    bind = op.get_bind()
    inspector = sa.inspect(bind)
    if not inspector.has_table("runtime_configurations"):
        _runtime_configuration_table().create(bind=bind, checkfirst=True)
        return

    columns = {column["name"] for column in inspector.get_columns("runtime_configurations")}
    if "codex_reasoning_effort" not in columns:
        op.add_column(
            "runtime_configurations",
            sa.Column("codex_reasoning_effort", sa.String(length=16), nullable=True),
        )
    if "codex_fast_mode" not in columns:
        op.add_column(
            "runtime_configurations",
            sa.Column("codex_fast_mode", sa.Boolean(), nullable=False, server_default=sa.false()),
        )
    if "codex_use_default_model_settings" not in columns:
        op.add_column(
            "runtime_configurations",
            sa.Column(
                "codex_use_default_model_settings",
                sa.Boolean(),
                nullable=False,
                server_default=sa.false(),
            ),
        )

    if _check_constraint_exists(bind, _REASONING_CONSTRAINT):
        return
    expression = (
        "codex_reasoning_effort IS NULL OR codex_reasoning_effort IN "
        "('minimal', 'low', 'medium', 'high', 'xhigh')"
    )
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("runtime_configurations", recreate="always") as batch:
            batch.create_check_constraint(_REASONING_CONSTRAINT, expression)
    else:
        op.create_check_constraint(
            _REASONING_CONSTRAINT,
            "runtime_configurations",
            expression,
        )


def downgrade() -> None:
    bind = op.get_bind()
    if not sa.inspect(bind).has_table("runtime_configurations"):
        return
    columns = {column["name"] for column in sa.inspect(bind).get_columns("runtime_configurations")}
    has_constraint = _check_constraint_exists(bind, _REASONING_CONSTRAINT)

    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("runtime_configurations", recreate="always") as batch:
            if has_constraint:
                batch.drop_constraint(_REASONING_CONSTRAINT, type_="check")
            if "codex_use_default_model_settings" in columns:
                batch.drop_column("codex_use_default_model_settings")
            if "codex_fast_mode" in columns:
                batch.drop_column("codex_fast_mode")
            if "codex_reasoning_effort" in columns:
                batch.drop_column("codex_reasoning_effort")
        return

    if has_constraint:
        op.drop_constraint(_REASONING_CONSTRAINT, "runtime_configurations", type_="check")
    if "codex_use_default_model_settings" in columns:
        op.drop_column("runtime_configurations", "codex_use_default_model_settings")
    if "codex_fast_mode" in columns:
        op.drop_column("runtime_configurations", "codex_fast_mode")
    if "codex_reasoning_effort" in columns:
        op.drop_column("runtime_configurations", "codex_reasoning_effort")
