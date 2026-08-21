"""Trusted Strategy source and immutable version API."""

from __future__ import annotations

from pathlib import Path
from typing import Any
from uuid import UUID

from fastapi import APIRouter, Depends, File, Form, Request, UploadFile
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import func, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from api.dependencies import get_session
from db.models import Strategy, StrategyVersion
from errors import QfError
from events import append_event
from strategy_contract import (
    MAX_STRATEGY_SOURCE_BYTES,
    decode_strategy_source,
    parse_strategy_config,
    validate_strategy_source,
)

router = APIRouter(prefix="/api/v1", tags=["strategies"])


class StrategyCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")

    name: str = Field(min_length=1, max_length=200)


class StrategyVersionView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    strategy_id: UUID
    version_no: int
    default_config: dict[str, Any]
    objective_directions: list[str]
    created_at: str


class StrategyView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    name: str
    versions: list[StrategyVersionView]


def _version_view(item: StrategyVersion) -> StrategyVersionView:
    return StrategyVersionView(
        id=item.id,
        strategy_id=item.strategy_id,
        version_no=item.version_no,
        default_config=item.default_config,
        objective_directions=item.objective_directions,
        created_at=item.created_at.isoformat(),
    )


def _strategy_view(session: Session, item: Strategy) -> StrategyView:
    versions = list(
        session.scalars(
            select(StrategyVersion)
            .where(StrategyVersion.strategy_id == item.id)
            .order_by(StrategyVersion.version_no.asc())
        )
    )
    return StrategyView(
        id=item.id,
        name=item.name,
        versions=[_version_view(version) for version in versions],
    )


@router.get("/strategies", response_model=list[StrategyView])
def list_strategies(session: Session = Depends(get_session)) -> list[StrategyView]:
    items = list(session.scalars(select(Strategy).order_by(Strategy.name.asc())))
    return [_strategy_view(session, item) for item in items]


@router.post("/strategies", response_model=StrategyView, status_code=201)
def create_strategy(
    payload: StrategyCreate,
    session: Session = Depends(get_session),
) -> StrategyView:
    try:
        with session.begin():
            item = Strategy(name=payload.name.strip())
            session.add(item)
            session.flush()
            append_event(
                session,
                kind="STRATEGY_CREATED",
                aggregate_type="strategy",
                aggregate_id=item.id,
                payload={"name": item.name},
                actor_kind="LOCAL_OPERATOR",
            )
    except IntegrityError as exc:
        session.rollback()
        raise QfError("RESOURCE_CONFLICT", "Strategy name already exists.", 409) from exc
    return _strategy_view(session, item)


@router.get("/strategies/{strategy_id}", response_model=StrategyView)
def show_strategy(
    strategy_id: UUID,
    session: Session = Depends(get_session),
) -> StrategyView:
    item = session.get(Strategy, strategy_id)
    if item is None:
        raise QfError("STRATEGY_UNKNOWN", "Strategy does not exist.", 404)
    return _strategy_view(session, item)


@router.post(
    "/strategies/{strategy_id}/versions",
    response_model=StrategyVersionView,
    status_code=201,
)
async def create_strategy_version(
    strategy_id: UUID,
    request: Request,
    source: UploadFile = File(...),
    default_config_json: str = Form(default="{}"),
    session: Session = Depends(get_session),
) -> StrategyVersionView:
    filename = source.filename or ""
    if Path(filename).name != filename or not filename.endswith(".py"):
        await source.close()
        raise QfError(
            "STRATEGY_FILE_INVALID",
            "Strategy upload must be one .py basename file.",
            422,
        )
    raw = await source.read(MAX_STRATEGY_SOURCE_BYTES + 1)
    await source.close()
    source_text = decode_strategy_source(raw)
    default_config = parse_strategy_config(default_config_json)
    validation = validate_strategy_source(
        source_text,
        default_config,
        staging_root=request.app.state.settings.import_root / "strategy-validation",
        timeout_seconds=request.app.state.settings.strategy_validation_timeout_seconds,
    )

    with session.begin():
        strategy = session.execute(
            select(Strategy).where(Strategy.id == strategy_id).with_for_update()
        ).scalar_one_or_none()
        if strategy is None:
            raise QfError("STRATEGY_UNKNOWN", "Strategy does not exist.", 404)
        latest = session.scalar(
            select(func.max(StrategyVersion.version_no)).where(
                StrategyVersion.strategy_id == strategy_id
            )
        )
        item = StrategyVersion(
            strategy_id=strategy_id,
            version_no=int(latest or 0) + 1,
            source_text=source_text,
            default_config=default_config,
            objective_directions=list(validation.objective_directions),
        )
        session.add(item)
        session.flush()
        append_event(
            session,
            kind="STRATEGY_VERSION_CREATED",
            aggregate_type="strategy_version",
            aggregate_id=item.id,
            payload={"strategy_id": str(strategy_id), "version_no": item.version_no},
            actor_kind="LOCAL_OPERATOR",
        )
    return _version_view(item)
