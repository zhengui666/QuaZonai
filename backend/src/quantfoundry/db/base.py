"""Shared SQLAlchemy model primitives."""

from __future__ import annotations

from datetime import datetime

from sqlalchemy import BigInteger, DateTime, Integer, Numeric, func
from sqlalchemy.dialects.postgresql import JSONB
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column
from sqlalchemy.types import JSON

JSON_VALUE = JSON().with_variant(JSONB, "postgresql")
MONEY = Numeric(38, 12)
IDENTITY_INT = BigInteger().with_variant(Integer, "sqlite")


class Base(DeclarativeBase):
    """Declarative model base."""


class TimestampMixin:
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True),
        nullable=False,
        server_default=func.now(),
        onupdate=func.now(),
    )


