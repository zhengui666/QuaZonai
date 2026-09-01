"""Add explicit Codex default-model selection mode.

Revision ID: 0011_codex_default_model_mode
Revises: 0010_operator_auth_configuration
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op

from db.models import RuntimeConfiguration

revision = "0011_codex_default_model_mode"
down_revision = "0010_operator_auth_configuration"
branch_labels = None
depends_on = None


def upgrade() -> None:
    bind = op.get_bind()
    inspector = sa.inspect(bind)
    if not inspector.has_table("runtime_configurations"):
        RuntimeConfiguration.__table__.create(bind=bind, checkfirst=True)
        return

    columns = {column["name"] for column in inspector.get_columns("runtime_configurations")}
    if "codex_use_default_model_settings" in columns:
        return

    op.add_column(
        "runtime_configurations",
        sa.Column(
            "codex_use_default_model_settings",
            sa.Boolean(),
            nullable=False,
            server_default=sa.true(),
        ),
    )
    op.execute(
        sa.text(
            "UPDATE runtime_configurations "
            "SET codex_use_default_model_settings = "
            "CASE WHEN codex_model IS NULL THEN true ELSE false END"
        )
    )


def downgrade() -> None:
    bind = op.get_bind()
    inspector = sa.inspect(bind)
    if not inspector.has_table("runtime_configurations"):
        return
    columns = {column["name"] for column in inspector.get_columns("runtime_configurations")}
    if "codex_use_default_model_settings" not in columns:
        return

    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("runtime_configurations", recreate="always") as batch:
            batch.drop_column("codex_use_default_model_settings")
        return
    op.drop_column("runtime_configurations", "codex_use_default_model_settings")
