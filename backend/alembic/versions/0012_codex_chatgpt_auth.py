"""Add database-owned Codex ChatGPT authentication.

Revision ID: 0012_codex_chatgpt_auth
Revises: 0011_codex_runtime_controls
"""

from __future__ import annotations

from alembic import op
import sqlalchemy as sa

revision = "0012_codex_chatgpt_auth"
down_revision = "0011_codex_runtime_controls"
branch_labels = None
depends_on = None


def upgrade() -> None:
    inspector = sa.inspect(op.get_bind())
    if not inspector.has_table("codex_chatgpt_auth_configurations"):
        op.create_table(
            "codex_chatgpt_auth_configurations",
            sa.Column("id", sa.Uuid(), nullable=False),
            sa.Column("scope", sa.String(length=40), nullable=False),
            sa.Column("state", sa.String(length=32), nullable=False),
            sa.Column("token_generation", sa.BigInteger(), nullable=False),
            sa.Column("chatgpt_account_id", sa.String(length=255), nullable=True),
            sa.Column("email", sa.String(length=320), nullable=True),
            sa.Column("plan_type", sa.String(length=64), nullable=True),
            sa.Column("access_token_ciphertext", sa.LargeBinary(), nullable=True),
            sa.Column("access_token_nonce", sa.LargeBinary(), nullable=True),
            sa.Column("access_token_key_version", sa.Integer(), nullable=True),
            sa.Column("access_token_expires_at", sa.DateTime(timezone=True), nullable=True),
            sa.Column("refresh_token_ciphertext", sa.LargeBinary(), nullable=True),
            sa.Column("refresh_token_nonce", sa.LargeBinary(), nullable=True),
            sa.Column("refresh_token_key_version", sa.Integer(), nullable=True),
            sa.Column("authenticated_at", sa.DateTime(timezone=True), nullable=True),
            sa.Column("last_refresh_at", sa.DateTime(timezone=True), nullable=True),
            sa.Column("reauth_required_at", sa.DateTime(timezone=True), nullable=True),
            sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.func.now(), nullable=False),
            sa.Column("updated_at", sa.DateTime(timezone=True), server_default=sa.func.now(), nullable=False),
            sa.CheckConstraint("scope = 'SYSTEM'", name="ck_codex_chatgpt_auth_scope"),
            sa.CheckConstraint(
                "state IN ('CONNECTED', 'REAUTH_REQUIRED')",
                name="ck_codex_chatgpt_auth_state",
            ),
            sa.CheckConstraint("token_generation >= 1", name="ck_codex_chatgpt_auth_generation"),
            sa.CheckConstraint(
                "state = 'REAUTH_REQUIRED' OR "
                "(chatgpt_account_id IS NOT NULL AND access_token_ciphertext IS NOT NULL AND "
                "access_token_nonce IS NOT NULL AND access_token_key_version IS NOT NULL AND "
                "access_token_expires_at IS NOT NULL AND refresh_token_ciphertext IS NOT NULL AND "
                "refresh_token_nonce IS NOT NULL AND refresh_token_key_version IS NOT NULL AND "
                "authenticated_at IS NOT NULL)",
                name="ck_codex_chatgpt_auth_connected_bundle",
            ),
            sa.PrimaryKeyConstraint("id"),
            sa.UniqueConstraint("scope", name="uq_codex_chatgpt_auth_scope"),
        )
    if not inspector.has_table("codex_chatgpt_login_attempts"):
        op.create_table(
            "codex_chatgpt_login_attempts",
            sa.Column("id", sa.Uuid(), nullable=False),
            sa.Column("scope", sa.String(length=40), nullable=False),
            sa.Column("state", sa.String(length=24), nullable=False),
            sa.Column("device_auth_id_ciphertext", sa.LargeBinary(), nullable=True),
            sa.Column("device_auth_id_nonce", sa.LargeBinary(), nullable=True),
            sa.Column("device_auth_id_key_version", sa.Integer(), nullable=True),
            sa.Column("user_code", sa.String(length=64), nullable=True),
            sa.Column("verification_url", sa.Text(), nullable=False),
            sa.Column("poll_interval_seconds", sa.Integer(), nullable=False),
            sa.Column("expires_at", sa.DateTime(timezone=True), nullable=False),
            sa.Column("next_poll_at", sa.DateTime(timezone=True), nullable=False),
            sa.Column("error_code", sa.String(length=64), nullable=True),
            sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.func.now(), nullable=False),
            sa.Column("updated_at", sa.DateTime(timezone=True), server_default=sa.func.now(), nullable=False),
            sa.CheckConstraint("scope = 'SYSTEM'", name="ck_codex_chatgpt_login_scope"),
            sa.CheckConstraint(
                "state IN ('PENDING', 'SUCCEEDED', 'CANCELLED', 'EXPIRED', 'FAILED')",
                name="ck_codex_chatgpt_login_state",
            ),
            sa.CheckConstraint(
                "poll_interval_seconds > 0 AND poll_interval_seconds <= 3600",
                name="ck_codex_chatgpt_login_interval",
            ),
            sa.PrimaryKeyConstraint("id"),
        )
    existing_indexes = {
        index["name"]
        for index in sa.inspect(op.get_bind()).get_indexes("codex_chatgpt_login_attempts")
    }
    if "uq_codex_chatgpt_login_pending_scope" not in existing_indexes:
        op.create_index(
            "uq_codex_chatgpt_login_pending_scope",
            "codex_chatgpt_login_attempts",
            ["scope"],
            unique=True,
            postgresql_where=sa.text("state = 'PENDING'"),
            sqlite_where=sa.text("state = 'PENDING'"),
        )
    if "ix_codex_chatgpt_login_expiry" not in existing_indexes:
        op.create_index(
            "ix_codex_chatgpt_login_expiry",
            "codex_chatgpt_login_attempts",
            ["state", "expires_at"],
        )


def downgrade() -> None:
    op.drop_index("ix_codex_chatgpt_login_expiry", table_name="codex_chatgpt_login_attempts")
    op.drop_index("uq_codex_chatgpt_login_pending_scope", table_name="codex_chatgpt_login_attempts")
    op.drop_table("codex_chatgpt_login_attempts")
    op.drop_table("codex_chatgpt_auth_configurations")
