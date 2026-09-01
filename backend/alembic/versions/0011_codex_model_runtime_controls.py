"""Add Codex reasoning effort and Fast service-tier controls.

Revision ID: 0011_codex_model_runtime_controls
Revises: 0010_operator_auth_configuration
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op

from db.models import RuntimeConfiguration

revision = "0011_codex_model_runtime_controls"
down_revision = "0010_operator_auth_configuration"
branch_labels = None
depends_on = None

_REASONING_CONSTRAINT = "ck_runtime_configuration_codex_reasoning_effort"


def _check_constraint_exists(bind: sa.Connection, name: str) -> bool:
    return any(
        constraint.get("name") == name
        for constraint in sa.inspect(bind).get_check_constraints("runtime_configurations")
    )


def upgrade() -> None:
    bind = op.get_bind()
    inspector = sa.inspect(bind)
    if not inspector.has_table("runtime_configurations"):
        RuntimeConfiguration.__table__.create(bind=bind, checkfirst=True)
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
            if "codex_fast_mode" in columns:
                batch.drop_column("codex_fast_mode")
            if "codex_reasoning_effort" in columns:
                batch.drop_column("codex_reasoning_effort")
        return

    if has_constraint:
        op.drop_constraint(_REASONING_CONSTRAINT, "runtime_configurations", type_="check")
    if "codex_fast_mode" in columns:
        op.drop_column("runtime_configurations", "codex_fast_mode")
    if "codex_reasoning_effort" in columns:
        op.drop_column("runtime_configurations", "codex_reasoning_effort")
