"""Transactional control-plane event helpers."""

from __future__ import annotations

from typing import Any
from uuid import UUID

from sqlalchemy.orm import Session

from db.models import Event


def append_event(
    session: Session,
    *,
    kind: str,
    aggregate_type: str,
    aggregate_id: UUID | None,
    payload: dict[str, Any] | None = None,
    actor_kind: str = "SYSTEM",
    actor_metadata: dict[str, Any] | None = None,
) -> Event:
    event = Event(
        kind=kind,
        aggregate_type=aggregate_type,
        aggregate_id=aggregate_id,
        actor_kind=actor_kind,
        actor_metadata=actor_metadata or {},
        payload=payload or {},
    )
    session.add(event)
    session.flush()
    return event
