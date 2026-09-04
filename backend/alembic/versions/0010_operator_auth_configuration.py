"""Add the frozen 0010 durable encrypted Operator TOTP binding.

Revision ID: 0010_operator_auth_configuration
Revises: 0009_archive_manifests
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op

revision = "0010_operator_auth_configuration"
down_revision = "0009_archive_manifests"
branch_labels = None
depends_on = None


def _tables() -> tuple[sa.Table, sa.Table]:
    metadata = sa.MetaData()
    configurations = sa.Table(
        "operator_auth_configurations",
        metadata,
        sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
        sa.Column("scope", sa.String(length=40), nullable=False),
        sa.Column("totp_secret_ciphertext", sa.LargeBinary(), nullable=False),
        sa.Column("totp_secret_nonce", sa.LargeBinary(), nullable=False),
        sa.Column("totp_secret_key_version", sa.Integer(), nullable=False),
        sa.Column("bound_at", sa.DateTime(timezone=True), nullable=False),
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
        sa.CheckConstraint("scope = 'SYSTEM'", name="ck_operator_auth_configuration_scope"),
        sa.CheckConstraint(
            "totp_secret_key_version > 0",
            name="ck_operator_auth_configuration_key_version",
        ),
        sa.UniqueConstraint("scope", name="uq_operator_auth_configuration_scope"),
    )
    initializations = sa.Table(
        "operator_auth_initializations",
        metadata,
        sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
        sa.Column("scope", sa.String(length=40), nullable=False),
        sa.Column("initialized_at", sa.DateTime(timezone=True), nullable=False),
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
        sa.CheckConstraint("scope = 'SYSTEM'", name="ck_operator_auth_initialization_scope"),
        sa.UniqueConstraint("scope", name="uq_operator_auth_initialization_scope"),
    )
    return configurations, initializations


def upgrade() -> None:
    configurations, initializations = _tables()
    bind = op.get_bind()
    initializations.create(bind=bind, checkfirst=True)
    configurations.create(bind=bind, checkfirst=True)


def downgrade() -> None:
    configurations, initializations = _tables()
    bind = op.get_bind()
    configurations.drop(bind=bind, checkfirst=True)
    initializations.drop(bind=bind, checkfirst=True)
