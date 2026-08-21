"""System health and schema endpoints."""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, Request
from pydantic import BaseModel, ConfigDict
from sqlalchemy.exc import SQLAlchemyError

from db.session import ping_database

router = APIRouter(prefix="/api/v1/system", tags=["system"])


class HealthResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    live: bool
    ready: bool
    database: str
    master_key: str
    plugin_manager: str
    finite_worker: str
    live_supervisor: str
    details: dict[str, Any]


@router.get("/health", response_model=HealthResponse)
def health(request: Request) -> HealthResponse:
    database_state = "ready"
    details: dict[str, Any] = {}
    try:
        ping_database(request.app.state.engine)
    except SQLAlchemyError as exc:
        database_state = "unavailable"
        details["database_error"] = type(exc).__name__

    settings = request.app.state.settings
    master_key_state = "configured" if settings.master_key_configured else "missing_or_invalid"
    ready = database_state == "ready" and master_key_state == "configured"
    return HealthResponse(
        live=True,
        ready=ready,
        database=database_state,
        master_key=master_key_state,
        plugin_manager="ready" if database_state == "ready" else "unavailable",
        finite_worker="not_observed",
        live_supervisor="not_observed",
        details=details,
    )
