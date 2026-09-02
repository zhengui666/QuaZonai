"""Add a durable singleton lock for ChatGPT auth operations.

Revision ID: 0013_codex_auth_operation_lock
Revises: 0012_codex_chatgpt_auth
"""

from __future__ import annotations

from uuid import UUID

import sqlalchemy as sa
from alembic import op

revision = "0013_codex_auth_operation_lock"
down_revision = "0012_codex_chatgpt_auth"
branch_labels = None
depends_on = None


_LOCK_ID = "00000000-0000-0000-0000-000000001013"


def upgrade() -> None:
    inspector = sa.inspect(op.get_bind())
    if not inspector.has_table("codex_chatgpt_auth_operation_locks"):
        op.create_table(
            "codex_chatgpt_auth_operation_locks",
            sa.Column("id", sa.Uuid(), nullable=False),
            sa.Column("scope", sa.String(length=40), nullable=False),
            sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.func.now(), nullable=False),
            sa.Column("updated_at", sa.DateTime(timezone=True), server_default=sa.func.now(), nullable=False),
            sa.CheckConstraint("scope = 'SYSTEM'", name="ck_codex_chatgpt_auth_lock_scope"),
            sa.PrimaryKeyConstraint("id"),
            sa.UniqueConstraint("scope", name="uq_codex_chatgpt_auth_lock_scope"),
        )
    with op.get_bind().begin_nested():
        op.execute(
            sa.text(
                "INSERT INTO codex_chatgpt_auth_operation_locks (id, scope) "
                "SELECT :id, 'SYSTEM' WHERE NOT EXISTS "
                "(SELECT 1 FROM codex_chatgpt_auth_operation_locks WHERE scope = 'SYSTEM')"
            ).bindparams(sa.bindparam("id", value=UUID(_LOCK_ID), type_=sa.Uuid()))
        )


def downgrade() -> None:
    op.drop_table("codex_chatgpt_auth_operation_locks")
