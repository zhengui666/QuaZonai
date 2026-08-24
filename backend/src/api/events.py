"""Durable event listing and Server-Sent Events projection."""

from __future__ import annotations

import asyncio
import json
from collections.abc import AsyncIterator
from typing import Any

from fastapi import APIRouter, Query, Request
from fastapi.responses import StreamingResponse
from pydantic import BaseModel, ConfigDict
from sqlalchemy import select

from db.models import Event

router = APIRouter(prefix="/api/v1", tags=["events"])
SSE_EVENT_NAME = "qz-event"


class EventView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: int
    kind: str
    aggregate_type: str
    aggregate_id: str | None
    actor_kind: str
    actor_metadata: dict[str, Any]
    payload: dict[str, Any]
    created_at: str


def _view(item: Event) -> EventView:
    return EventView(
        id=item.id,
        kind=item.kind,
        aggregate_type=item.aggregate_type,
        aggregate_id=str(item.aggregate_id) if item.aggregate_id else None,
        actor_kind=item.actor_kind,
        actor_metadata=item.actor_metadata,
        payload=item.payload,
        created_at=item.created_at.isoformat(),
    )


def _read_events(request: Request, after_id: int, limit: int) -> list[EventView]:
    factory = request.app.state.session_factory
    with factory() as session:
        items = list(
            session.scalars(
                select(Event)
                .where(Event.id > after_id)
                .order_by(Event.id.asc())
                .limit(limit)
            )
        )
        return [_view(item) for item in items]


@router.get("/events", response_model=list[EventView])
def list_events(
    request: Request,
    after_id: int = Query(default=0, ge=0),
    limit: int = Query(default=200, ge=1, le=1000),
) -> list[EventView]:
    return _read_events(request, after_id, limit)


async def _stream(request: Request, cursor: int) -> AsyncIterator[str]:
    last_id = cursor
    idle_ticks = 0
    while True:
        if await request.is_disconnected():
            return
        items = _read_events(request, last_id, 200)
        if items:
            idle_ticks = 0
            for item in items:
                last_id = item.id
                payload = json.dumps(item.model_dump(mode="json"), separators=(",", ":"))
                yield f"id: {item.id}\nevent: {SSE_EVENT_NAME}\ndata: {payload}\n\n"
        else:
            idle_ticks += 1
            if idle_ticks >= 15:
                idle_ticks = 0
                yield ": keepalive\n\n"
        await asyncio.sleep(1)


@router.get("/events/stream")
def stream_events(
    request: Request,
    cursor: int | None = Query(default=None, ge=0),
) -> StreamingResponse:
    header = request.headers.get("last-event-id")
    last_event_id = int(header) if header and header.isdigit() else 0
    start = cursor if cursor is not None else last_event_id
    return StreamingResponse(
        _stream(request, start),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-cache", "X-Accel-Buffering": "no"},
    )
