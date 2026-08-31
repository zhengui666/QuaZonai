"""Persistence for revocable native-operator device sessions."""

from __future__ import annotations

import uuid
from datetime import datetime

from sqlalchemy import DateTime, Integer, String, UniqueConstraint
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
