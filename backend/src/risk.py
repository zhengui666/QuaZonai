"""Atomic central gross-exposure reservation service."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from typing import Any
from uuid import UUID

from sqlalchemy import func, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from db.models import (
    RiskAccount,
    RiskEvent,
    RiskOpenOrder,
    RiskPosition,
    RiskReservation,
)
from errors import QfError

ACTIVE_ORDER_STATES = ("OPEN", "PENDING_CANCEL", "UNKNOWN")
ACTIVE_RESERVATION_STATES = ("PENDING", "SUBMITTED", "UNKNOWN")


def _sum_positions(session: Session, funder_id: str) -> int:
    return int(
        session.scalar(
            select(func.coalesce(func.sum(RiskPosition.entry_cost_micros), 0)).where(
                RiskPosition.funder_id == funder_id
            )
        )
        or 0
    )


def _sum_open_orders(session: Session, funder_id: str) -> int:
    return int(
        session.scalar(
            select(func.coalesce(func.sum(RiskOpenOrder.increase_debit_micros), 0)).where(
                RiskOpenOrder.funder_id == funder_id,
                RiskOpenOrder.state.in_(ACTIVE_ORDER_STATES),
            )
        )
        or 0
    )


def _sum_reservations(session: Session, funder_id: str) -> int:
    return int(
        session.scalar(
            select(func.coalesce(func.sum(RiskReservation.reserved_micros), 0)).where(
                RiskReservation.funder_id == funder_id,
                RiskReservation.state.in_(ACTIVE_RESERVATION_STATES),
            )
        )
        or 0
    )


def gross_exposure_micros(session: Session, funder_id: str) -> int:
    return (
        _sum_positions(session, funder_id)
        + _sum_open_orders(session, funder_id)
        + _sum_reservations(session, funder_id)
    )


def _account_for_update(session: Session, funder_id: str) -> RiskAccount:
    account = session.execute(
        select(RiskAccount).where(RiskAccount.funder_id == funder_id).with_for_update()
    ).scalar_one_or_none()
    if account is None:
        raise QfError("RISK_UNAVAILABLE", "Funder risk account does not exist.", 503)
    return account


def _require_fresh(
    account: RiskAccount,
    *,
    deployment_id: UUID,
    generation: int,
    now: datetime,
    freshness_seconds: int,
) -> None:
    if account.status != "READY":
        raise QfError(
            "RISK_UNAVAILABLE",
            "Funder risk account is not ready for increased exposure.",
            503,
            {"status": account.status},
        )
    if account.owner_deployment_id != deployment_id or account.owner_generation != generation:
        raise QfError(
            "RISK_UNAVAILABLE",
            "Funder risk ownership does not match the active runner generation.",
            503,
        )
    threshold = now - timedelta(seconds=freshness_seconds)
    if account.last_reconciled_at is None or account.last_reconciled_at < threshold:
        raise QfError("RISK_UNAVAILABLE", "Funder risk reconciliation is stale.", 503)
    if account.last_heartbeat_at is None or account.last_heartbeat_at < threshold:
        raise QfError("RISK_UNAVAILABLE", "Funder risk heartbeat is stale.", 503)


def reserve_exposure(
    session: Session,
    *,
    funder_id: str,
    deployment_id: UUID,
    generation: int,
    client_order_id: str,
    instrument_id: str,
    reserve_micros: int,
    freshness_seconds: int = 30,
    now: datetime | None = None,
) -> RiskReservation:
    if reserve_micros < 0:
        raise QfError("RISK_LIMIT_EXCEEDED", "Reservation amount cannot be negative.", 422)
    current = now or datetime.now(UTC)
    account = _account_for_update(session, funder_id)
    _require_fresh(
        account,
        deployment_id=deployment_id,
        generation=generation,
        now=current,
        freshness_seconds=freshness_seconds,
    )

    existing = session.execute(
        select(RiskReservation).where(
            RiskReservation.funder_id == funder_id,
            RiskReservation.runner_generation == generation,
            RiskReservation.client_order_id == client_order_id,
        )
    ).scalar_one_or_none()
    if existing is not None:
        if (
            existing.instrument_id != instrument_id
            or existing.reserved_micros != reserve_micros
        ):
            raise QfError(
                "RISK_RESERVATION_CONFLICT",
                "Client order ID was already used for another reservation.",
                409,
            )
        return existing

    current_gross = gross_exposure_micros(session, funder_id)
    projected = current_gross + reserve_micros
    if projected > account.gross_limit_micros:
        session.add(
            RiskEvent(
                funder_id=funder_id,
                kind="RESERVATION_REJECTED_LIMIT",
                payload={
                    "client_order_id": client_order_id,
                    "instrument_id": instrument_id,
                    "current_gross_micros": current_gross,
                    "requested_micros": reserve_micros,
                    "limit_micros": account.gross_limit_micros,
                },
            )
        )
        raise QfError(
            "RISK_LIMIT_EXCEEDED",
            "Funder gross exposure limit would be exceeded.",
            409,
            {
                "current_gross_micros": current_gross,
                "requested_micros": reserve_micros,
                "limit_micros": account.gross_limit_micros,
            },
        )

    reservation = RiskReservation(
        funder_id=funder_id,
        runner_generation=generation,
        client_order_id=client_order_id,
        instrument_id=instrument_id,
        reserved_micros=reserve_micros,
        state="PENDING",
    )
    session.add(reservation)
    session.add(
        RiskEvent(
            funder_id=funder_id,
            kind="RESERVATION_CREATED",
            payload={
                "client_order_id": client_order_id,
                "instrument_id": instrument_id,
                "reserved_micros": reserve_micros,
                "projected_gross_micros": projected,
            },
        )
    )
    try:
        session.flush()
    except IntegrityError as exc:
        raise QfError(
            "RISK_RESERVATION_CONFLICT",
            "Concurrent reservation used the same client order ID.",
            409,
        ) from exc
    return reservation


def set_reservation_state(
    session: Session,
    reservation_id: UUID,
    *,
    state: str,
    now: datetime | None = None,
) -> RiskReservation:
    allowed = {"SUBMITTED", "REJECTED", "RELEASED", "UNKNOWN"}
    if state not in allowed:
        raise QfError("RISK_RESERVATION_INVALID", "Reservation state is invalid.", 422)
    reservation = session.execute(
        select(RiskReservation)
        .where(RiskReservation.id == reservation_id)
        .with_for_update()
    ).scalar_one_or_none()
    if reservation is None:
        raise QfError("RISK_RESERVATION_UNKNOWN", "Reservation does not exist.", 404)
    reservation.state = state
    if state in {"REJECTED", "RELEASED"}:
        reservation.resolved_at = now or datetime.now(UTC)
    session.add(
        RiskEvent(
            funder_id=reservation.funder_id,
            kind=f"RESERVATION_{state}",
            payload={
                "reservation_id": str(reservation.id),
                "client_order_id": reservation.client_order_id,
            },
        )
    )
    session.flush()
    return reservation


def replace_projection(
    session: Session,
    *,
    funder_id: str,
    deployment_id: UUID,
    generation: int,
    positions: list[dict[str, Any]],
    open_orders: list[dict[str, Any]],
    observed_at: datetime | None = None,
    mark_ready: bool = True,
) -> RiskAccount:
    current = observed_at or datetime.now(UTC)
    account = _account_for_update(session, funder_id)

    position_ids = [str(item["instrument_id"]) for item in positions]
    order_ids = [str(item["client_order_id"]) for item in open_orders]
    if len(position_ids) != len(set(position_ids)) or len(order_ids) != len(set(order_ids)):
        raise QfError(
            "RISK_RECONCILIATION_INVALID",
            "Venue reconciliation snapshot contains duplicate identities.",
            422,
        )

    session.query(RiskPosition).filter(RiskPosition.funder_id == funder_id).delete()
    session.query(RiskOpenOrder).filter(RiskOpenOrder.funder_id == funder_id).delete()
    for item in positions:
        entry_cost = int(item["entry_cost_micros"])
        if entry_cost < 0:
            raise QfError(
                "RISK_RECONCILIATION_INVALID",
                "Position entry cost cannot be negative.",
                422,
            )
        session.add(
            RiskPosition(
                funder_id=funder_id,
                instrument_id=str(item["instrument_id"]),
                entry_cost_micros=entry_cost,
                observed_at=current,
            )
        )
    for item in open_orders:
        state = str(item.get("state", "OPEN"))
        debit = int(item["increase_debit_micros"])
        if state not in ACTIVE_ORDER_STATES or debit < 0:
            raise QfError(
                "RISK_RECONCILIATION_INVALID",
                "Open-order projection has an invalid state or debit.",
                422,
            )
        session.add(
            RiskOpenOrder(
                funder_id=funder_id,
                client_order_id=str(item["client_order_id"]),
                instrument_id=str(item["instrument_id"]),
                increase_debit_micros=debit,
                state=state,
                observed_at=current,
            )
        )
    account.owner_deployment_id = deployment_id
    account.owner_generation = generation
    account.last_reconciled_at = current
    account.last_heartbeat_at = current if mark_ready else None
    account.status = "READY" if mark_ready else "RECONCILING"
    session.add(
        RiskEvent(
            funder_id=funder_id,
            kind="PROJECTION_RECONCILED",
            payload={
                "deployment_id": str(deployment_id),
                "generation": generation,
                "position_count": len(positions),
                "open_order_count": len(open_orders),
                "ready_for_increased_exposure": mark_ready,
            },
        )
    )
    session.flush()
    return account
