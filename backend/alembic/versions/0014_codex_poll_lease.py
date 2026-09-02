"""Add an in-flight lease for device-code polling."""

from __future__ import annotations

from alembic import op
import sqlalchemy as sa

revision = "0014_codex_poll_lease"
down_revision = "0013_codex_auth_operation_lock"
branch_labels = None
depends_on = None


def upgrade() -> None:
    inspector = sa.inspect(op.get_bind())
    columns = {column["name"] for column in inspector.get_columns("codex_chatgpt_login_attempts")}
    if "poll_lease_until" not in columns:
        op.add_column(
            "codex_chatgpt_login_attempts",
            sa.Column("poll_lease_until", sa.DateTime(timezone=True), nullable=True),
        )


def downgrade() -> None:
    inspector = sa.inspect(op.get_bind())
    columns = {column["name"] for column in inspector.get_columns("codex_chatgpt_login_attempts")}
    if "poll_lease_until" in columns:
        op.drop_column("codex_chatgpt_login_attempts", "poll_lease_until")
