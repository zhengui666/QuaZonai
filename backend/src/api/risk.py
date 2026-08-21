"""Read-only central risk projection API."""

from __future__ import annotations

from fastapi import APIRouter, Depends
from pydantic import BaseModel, ConfigDict
from sqlalchemy import select
from sqlalchemy.orm import Session

from api.dependencies import get_session
from db.models import RiskAccount, RiskReservation
from risk import gross_exposure_micros

router = APIRouter(prefix="/api/v1", tags=["risk"])


class ReservationView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: str
    runner_generation: int
    client_order_id: str
    instrument_id: str
    reserved_micros: int
    state: str


class RiskAccountView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    funder_id: str
    status: str
    gross_limit_micros: int
    gross_exposure_micros: int
    owner_deployment_id: str | None
    owner_generation: int | None
    last_reconciled_at: str | None
    last_heartbeat_at: str | None
    active_reservations: list[ReservationView]


def _view(session: Session, account: RiskAccount) -> RiskAccountView:
    reservations = list(
        session.scalars(
            select(RiskReservation)
            .where(
                RiskReservation.funder_id == account.funder_id,
                RiskReservation.state.in_(["PENDING", "SUBMITTED", "UNKNOWN"]),
            )
            .order_by(RiskReservation.created_at.asc())
        )
    )
    return RiskAccountView(
        funder_id=account.funder_id,
        status=account.status,
        gross_limit_micros=account.gross_limit_micros,
        gross_exposure_micros=gross_exposure_micros(session, account.funder_id),
        owner_deployment_id=(
            str(account.owner_deployment_id) if account.owner_deployment_id else None
        ),
        owner_generation=account.owner_generation,
        last_reconciled_at=(
            account.last_reconciled_at.isoformat() if account.last_reconciled_at else None
        ),
        last_heartbeat_at=(
            account.last_heartbeat_at.isoformat() if account.last_heartbeat_at else None
        ),
        active_reservations=[
            ReservationView(
                id=str(item.id),
                runner_generation=item.runner_generation,
                client_order_id=item.client_order_id,
                instrument_id=item.instrument_id,
                reserved_micros=item.reserved_micros,
                state=item.state,
            )
            for item in reservations
        ],
    )


@router.get("/risk-accounts", response_model=list[RiskAccountView])
def list_risk_accounts(
    session: Session = Depends(get_session),
) -> list[RiskAccountView]:
    items = list(session.scalars(select(RiskAccount).order_by(RiskAccount.funder_id.asc())))
    return [_view(session, item) for item in items]
