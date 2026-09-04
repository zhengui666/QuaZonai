"""Database-owned ChatGPT authentication state."""

from __future__ import annotations

from datetime import datetime
from uuid import UUID, uuid4

from sqlalchemy import (
    BigInteger,
    CheckConstraint,
    DateTime,
    Index,
    Integer,
    LargeBinary,
    String,
    Text,
    UniqueConstraint,
    Uuid,
    text,
)
from sqlalchemy.orm import Mapped, mapped_column

from db.base import Base, TimestampMixin

SYSTEM_SCOPE = "SYSTEM"
CHATGPT_AUTH_CONNECTED = "CONNECTED"
CHATGPT_AUTH_REAUTH_REQUIRED = "REAUTH_REQUIRED"
CHATGPT_AUTH_STATES = frozenset({CHATGPT_AUTH_CONNECTED, CHATGPT_AUTH_REAUTH_REQUIRED})
LOGIN_PENDING = "PENDING"
LOGIN_SUCCEEDED = "SUCCEEDED"
LOGIN_CANCELLED = "CANCELLED"
LOGIN_EXPIRED = "EXPIRED"
LOGIN_FAILED = "FAILED"
LOGIN_TERMINAL_STATES = frozenset(
    {LOGIN_SUCCEEDED, LOGIN_CANCELLED, LOGIN_EXPIRED, LOGIN_FAILED}
)


class CodexChatgptAuthConfiguration(Base, TimestampMixin):
    """The single canonical ChatGPT OAuth token bundle for this installation."""

    __tablename__ = "codex_chatgpt_auth_configurations"
    __table_args__ = (
        CheckConstraint("scope = 'SYSTEM'", name="ck_codex_chatgpt_auth_scope"),
        CheckConstraint(
            "state IN ('CONNECTED', 'REAUTH_REQUIRED')",
            name="ck_codex_chatgpt_auth_state",
        ),
        CheckConstraint("token_generation >= 1", name="ck_codex_chatgpt_auth_generation"),
        CheckConstraint(
            "state = 'REAUTH_REQUIRED' OR "
            "(chatgpt_account_id IS NOT NULL AND "
            "access_token_ciphertext IS NOT NULL AND access_token_nonce IS NOT NULL AND "
            "access_token_key_version IS NOT NULL AND access_token_expires_at IS NOT NULL AND "
            "refresh_token_ciphertext IS NOT NULL AND refresh_token_nonce IS NOT NULL AND "
            "refresh_token_key_version IS NOT NULL AND authenticated_at IS NOT NULL)",
            name="ck_codex_chatgpt_auth_connected_bundle",
        ),
        UniqueConstraint("scope", name="uq_codex_chatgpt_auth_scope"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    scope: Mapped[str] = mapped_column(String(40), nullable=False, default=SYSTEM_SCOPE)
    state: Mapped[str] = mapped_column(String(32), nullable=False)
    token_generation: Mapped[int] = mapped_column(BigInteger, nullable=False, default=1)
    chatgpt_account_id: Mapped[str | None] = mapped_column(String(255))
    email: Mapped[str | None] = mapped_column(String(320))
    plan_type: Mapped[str | None] = mapped_column(String(64))
    access_token_ciphertext: Mapped[bytes | None] = mapped_column(LargeBinary)
    access_token_nonce: Mapped[bytes | None] = mapped_column(LargeBinary)
    access_token_key_version: Mapped[int | None] = mapped_column(Integer)
    access_token_expires_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    refresh_token_ciphertext: Mapped[bytes | None] = mapped_column(LargeBinary)
    refresh_token_nonce: Mapped[bytes | None] = mapped_column(LargeBinary)
    refresh_token_key_version: Mapped[int | None] = mapped_column(Integer)
    authenticated_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    last_refresh_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    reauth_required_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class CodexChatgptAuthOperationLock(Base):
    """Durable singleton row used to order auth operations without a row gap."""

    __tablename__ = "codex_chatgpt_auth_operation_locks"
    __table_args__ = (
        CheckConstraint("scope = 'SYSTEM'", name="ck_codex_chatgpt_auth_lock_scope"),
        UniqueConstraint("scope", name="uq_codex_chatgpt_auth_lock_scope"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    scope: Mapped[str] = mapped_column(String(40), nullable=False, default=SYSTEM_SCOPE)


class CodexChatgptPollLock(Base):
    """Exact-UUID row used to serialize one device-login poll across processes."""

    __tablename__ = "codex_chatgpt_poll_locks"

    login_id: Mapped[UUID] = mapped_column(Uuid, primary_key=True)


class CodexChatgptLoginAttempt(Base, TimestampMixin):
    """Durable state for one OpenAI Device Code authorization attempt."""

    __tablename__ = "codex_chatgpt_login_attempts"
    __table_args__ = (
        CheckConstraint("scope = 'SYSTEM'", name="ck_codex_chatgpt_login_scope"),
        CheckConstraint(
            "state IN ('PENDING', 'SUCCEEDED', 'CANCELLED', 'EXPIRED', 'FAILED')",
            name="ck_codex_chatgpt_login_state",
        ),
        CheckConstraint(
            "poll_interval_seconds > 0 AND poll_interval_seconds <= 3600",
            name="ck_codex_chatgpt_login_interval",
        ),
        Index(
            "uq_codex_chatgpt_login_pending_scope",
            "scope",
            unique=True,
            sqlite_where=text("state = 'PENDING'"),
            postgresql_where=text("state = 'PENDING'"),
        ),
        Index("ix_codex_chatgpt_login_expiry", "state", "expires_at"),
    )

    id: Mapped[UUID] = mapped_column(Uuid, primary_key=True, default=uuid4)
    scope: Mapped[str] = mapped_column(String(40), nullable=False, default=SYSTEM_SCOPE)
    state: Mapped[str] = mapped_column(String(24), nullable=False, default=LOGIN_PENDING)
    device_auth_id_ciphertext: Mapped[bytes | None] = mapped_column(LargeBinary)
    device_auth_id_nonce: Mapped[bytes | None] = mapped_column(LargeBinary)
    device_auth_id_key_version: Mapped[int | None] = mapped_column(Integer)
    user_code: Mapped[str | None] = mapped_column(String(64))
    verification_url: Mapped[str] = mapped_column(Text, nullable=False)
    poll_interval_seconds: Mapped[int] = mapped_column(Integer, nullable=False)
    expires_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    next_poll_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    poll_lease_until: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    error_code: Mapped[str | None] = mapped_column(String(64))


__all__ = [
    "CHATGPT_AUTH_CONNECTED",
    "CHATGPT_AUTH_REAUTH_REQUIRED",
    "CHATGPT_AUTH_STATES",
    "CodexChatgptAuthConfiguration",
    "CodexChatgptAuthOperationLock",
    "CodexChatgptPollLock",
    "CodexChatgptLoginAttempt",
    "LOGIN_CANCELLED",
    "LOGIN_EXPIRED",
    "LOGIN_FAILED",
    "LOGIN_PENDING",
    "LOGIN_SUCCEEDED",
    "LOGIN_TERMINAL_STATES",
    "SYSTEM_SCOPE",
]
