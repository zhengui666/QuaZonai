"""Persistence for revocable native-operator device sessions."""

from __future__ import annotations

import uuid
from datetime import datetime

from sqlalchemy import CheckConstraint, DateTime, Integer, LargeBinary, String, UniqueConstraint, Uuid
from sqlalchemy.orm import Mapped, mapped_column

from db.base import Base, TimestampMixin


class MobileOperatorDevice(TimestampMixin, Base):
    __tablename__ = "mobile_operator_devices"
    __table_args__ = (UniqueConstraint("installation_id", name="uq_mobile_operator_installation"),)

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, default=uuid.uuid4)
    installation_id: Mapped[str] = mapped_column(String(36), nullable=False)
    display_name: Mapped[str] = mapped_column(String(120), nullable=False)
    device_family: Mapped[str] = mapped_column(String(16), nullable=False)
    credential_generation: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    last_seen_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    refresh_expires_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    revoked_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    client_version: Mapped[str] = mapped_column(String(80), nullable=False)
    app_build: Mapped[str] = mapped_column(String(80), nullable=False)
    os_version: Mapped[str] = mapped_column(String(80), nullable=False)


class OperatorAuthConfiguration(TimestampMixin, Base):
    """The one durable, encrypted TOTP binding owned by this installation."""

    __tablename__ = "operator_auth_configurations"
    __table_args__ = (
        CheckConstraint("scope = 'SYSTEM'", name="ck_operator_auth_configuration_scope"),
        CheckConstraint(
            "totp_secret_key_version > 0",
            name="ck_operator_auth_configuration_key_version",
        ),
        UniqueConstraint("scope", name="uq_operator_auth_configuration_scope"),
    )

    id: Mapped[uuid.UUID] = mapped_column(Uuid, primary_key=True, default=uuid.uuid4)
    scope: Mapped[str] = mapped_column(String(40), nullable=False, default="SYSTEM")
    totp_secret_ciphertext: Mapped[bytes] = mapped_column(LargeBinary, nullable=False)
    totp_secret_nonce: Mapped[bytes] = mapped_column(LargeBinary, nullable=False)
    totp_secret_key_version: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    bound_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
