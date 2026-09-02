"""Add exact-UUID row locks for device-code poll execution."""

from __future__ import annotations

from alembic import op
import sqlalchemy as sa

revision = "0015_codex_poll_lock_rows"
down_revision = "0014_codex_poll_lease"
branch_labels = None
depends_on = None


def upgrade() -> None:
    inspector = sa.inspect(op.get_bind())
    if not inspector.has_table("codex_chatgpt_poll_locks"):
        op.create_table(
            "codex_chatgpt_poll_locks",
            sa.Column("login_id", sa.Uuid(), nullable=False),
            sa.PrimaryKeyConstraint("login_id"),
        )


def downgrade() -> None:
    inspector = sa.inspect(op.get_bind())
    if inspector.has_table("codex_chatgpt_poll_locks"):
        op.drop_table("codex_chatgpt_poll_locks")
