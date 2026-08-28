"""Controlled promotion endpoints for the Nautilus-first research pipeline."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

from fastapi import APIRouter, Header, Request
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from db.models import DatasetRevision, Job, PublicMutationReceipt, SearchLedgerEntry
from errors import QfError
from jobs import enqueue_job
from quant_runtime.contracts import ExperimentMode
from quant_runtime.promotion import simulate_portfolio_candidate

router = APIRouter(prefix="/api/v1", tags=["research-runtime"])
SEALED_JOB_KIND = "SEALED_ALPHA_QUALIFICATION"
_SIMULATION_PENDING_STATUS = 102
_SIMULATION_STALE_AFTER = timedelta(minutes=35)


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class AlphaQualificationInput(StrictModel):
    sealed_dataset_revision_id: UUID
    name: str | None = Field(default=None, max_length=240)
    role: str = Field(default="PRIMARY_ALPHA", min_length=1, max_length=100)


class AlphaQualificationJobResult(StrictModel):
    job_id: UUID
    source_experiment_id: UUID
    state: str


class PortfolioSimulationInput(StrictModel):
    alpha_ids: list[UUID] = Field(min_length=1)


class PortfolioSimulationResult(StrictModel):
    candidate_id: UUID
    approval_id: UUID
    simulation_experiment_id: UUID
    selected_alpha_id: UUID


def _qualification_payload(payload: AlphaQualificationInput) -> dict[str, object]:
    return {
        "sealed_dataset_revision_id": str(payload.sealed_dataset_revision_id),
        "name": payload.name,
        "role": payload.role,
    }


def _same_qualification_request(job: Job, requested: dict[str, object]) -> bool:
    persisted = dict(job.payload or {})
    return all(persisted.get(key) == value for key, value in requested.items())


def _simulation_receipt_matches(
    receipt: PublicMutationReceipt,
    *,
    operation: str,
    normalized: dict[str, object],
) -> bool:
    return receipt.operation_name == operation and receipt.normalized_request == normalized


def _pending_simulation_experiment_id(receipt: PublicMutationReceipt) -> UUID:
    try:
        return UUID(str((receipt.response_json or {})["simulation_experiment_id"]))
    except (KeyError, TypeError, ValueError) as exc:
        raise QfError(
            "IDEMPOTENCY_RECEIPT_INVALID",
            "Candidate simulation idempotency receipt is missing its experiment identity.",
            500,
        ) from exc


def _claim_simulation_receipt(
    session: Session,
    *,
    key: str,
    operation: str,
    normalized: dict[str, object],
) -> tuple[PublicMutationReceipt, bool, UUID]:
    """Atomically bind a global idempotency key before any remote simulation starts."""
    existing = session.execute(
        select(PublicMutationReceipt)
        .where(PublicMutationReceipt.idempotency_key == key)
        .with_for_update()
    ).scalar_one_or_none()
    if existing is None:
        experiment_id = uuid4()
        now = datetime.now(UTC)
        receipt = PublicMutationReceipt(
            idempotency_key=key,
            operation_name=operation,
            normalized_request=normalized,
            response_json={
                "state": "RUNNING",
                "simulation_experiment_id": str(experiment_id),
                "attempt_started_at": now.isoformat(),
            },
            status_code=_SIMULATION_PENDING_STATUS,
            created_at=now,
        )
        try:
            with session.begin_nested():
                session.add(receipt)
                session.flush()
        except IntegrityError as exc:
            if receipt in session:
                session.expunge(receipt)
            session.expire_all()
            existing = session.execute(
                select(PublicMutationReceipt)
                .where(PublicMutationReceipt.idempotency_key == key)
                .with_for_update()
            ).scalar_one_or_none()
            if existing is None:
                raise QfError(
                    "IDEMPOTENCY_RECEIPT_CONFLICT",
                    "The candidate simulation receipt could not be resolved after a concurrent request.",
                    409,
                ) from exc
        else:
            return receipt, True, experiment_id

    if not _simulation_receipt_matches(existing, operation=operation, normalized=normalized):
        raise QfError(
            "IDEMPOTENCY_KEY_REUSED",
            "The idempotency key belongs to a different request.",
            409,
        )
    experiment_id = _pending_simulation_experiment_id(existing)
    if existing.status_code == 200:
        return existing, False, experiment_id
    if existing.status_code != _SIMULATION_PENDING_STATUS:
        raise QfError(
            "IDEMPOTENCY_RECEIPT_INVALID",
            "Candidate simulation receipt has an unsupported state.",
            500,
            {"status_code": existing.status_code},
        )

    pending = dict(existing.response_json or {})
    try:
        attempt_started_at = datetime.fromisoformat(str(pending["attempt_started_at"]))
    except (KeyError, TypeError, ValueError) as exc:
        raise QfError(
            "IDEMPOTENCY_RECEIPT_INVALID",
            "Candidate simulation receipt is missing its attempt timestamp.",
            500,
        ) from exc
    if attempt_started_at.tzinfo is None or attempt_started_at.utcoffset() is None:
        raise QfError(
            "IDEMPOTENCY_RECEIPT_INVALID",
            "Candidate simulation receipt timestamp must be timezone-aware.",
            500,
        )
    now = datetime.now(UTC)
    if attempt_started_at > now - _SIMULATION_STALE_AFTER:
        raise QfError(
            "CANDIDATE_SIMULATION_IN_PROGRESS",
            "The exact candidate simulation is already running.",
            409,
            {"simulation_experiment_id": str(experiment_id)},
        )
    existing.response_json = {
        "state": "RUNNING",
        "simulation_experiment_id": str(experiment_id),
        "attempt_started_at": now.isoformat(),
    }
    existing.status_code = _SIMULATION_PENDING_STATUS
    session.flush()
    return existing, True, experiment_id


def _mark_simulation_retryable(
    factory: object,
    *,
    key: str,
    operation: str,
    normalized: dict[str, object],
    experiment_id: UUID,
    exc: Exception,
) -> None:
    retry_at = datetime.now(UTC) - _SIMULATION_STALE_AFTER - timedelta(seconds=1)
    with factory.begin() as session:  # type: ignore[attr-defined]
        receipt = session.execute(
            select(PublicMutationReceipt)
            .where(PublicMutationReceipt.idempotency_key == key)
            .with_for_update()
        ).scalar_one_or_none()
        if receipt is None or not _simulation_receipt_matches(
            receipt,
            operation=operation,
            normalized=normalized,
        ):
            return
        if _pending_simulation_experiment_id(receipt) != experiment_id:
            return
        if receipt.status_code != _SIMULATION_PENDING_STATUS:
            return
        receipt.response_json = {
            "state": "RETRYABLE",
            "simulation_experiment_id": str(experiment_id),
            "attempt_started_at": retry_at.isoformat(),
            "last_failure_code": str(getattr(exc, "code", type(exc).__name__))[:100],
        }
        session.flush()


@router.post(
    "/research-experiments/{source_experiment_id}/qualify-alpha",
    response_model=AlphaQualificationJobResult,
    status_code=202,
)
def request_alpha_qualification(
    source_experiment_id: UUID,
    payload: AlphaQualificationInput,
    request: Request,
) -> AlphaQualificationJobResult:
    """Queue sealed evaluation without exposing the sealed credential to the API process."""
    factory = request.app.state.session_factory
    requested = _qualification_payload(payload)
    with factory() as session, session.begin():
        source = session.execute(
            select(SearchLedgerEntry)
            .where(SearchLedgerEntry.id == source_experiment_id)
            .with_for_update()
        ).scalar_one_or_none()
        if source is None:
            raise QfError(
                "SEARCH_LEDGER_ENTRY_NOT_FOUND", "Discovery experiment does not exist.", 404
            )
        if source.mode != ExperimentMode.DISCOVERY.value or source.state != "SUCCEEDED":
            raise QfError(
                "ALPHA_SOURCE_NOT_PROMOTABLE",
                "Alpha Qualification requires a successful Discovery experiment.",
                422,
            )
        sealed_dataset = session.get(DatasetRevision, payload.sealed_dataset_revision_id)
        if sealed_dataset is None or sealed_dataset.partition != "SEALED":
            raise QfError(
                "SEALED_DATASET_REVISION_INVALID",
                "Alpha Qualification requires a governed SEALED Dataset Revision.",
                422,
            )
        existing = session.scalar(
            select(Job)
            .where(
                Job.kind == SEALED_JOB_KIND,
                Job.resource_id == source_experiment_id,
                Job.state.in_(["READY", "LEASED", "SUCCEEDED"]),
            )
            .order_by(Job.created_at.desc())
        )
        if existing is not None:
            if not _same_qualification_request(existing, requested):
                raise QfError(
                    "SEALED_QUALIFICATION_ALREADY_REQUESTED",
                    "This Discovery experiment already has a different active sealed request.",
                    409,
                    {"job_id": str(existing.id), "state": existing.state},
                )
            return AlphaQualificationJobResult(
                job_id=existing.id,
                source_experiment_id=source_experiment_id,
                state=existing.state,
            )
        job = enqueue_job(
            session,
            kind=SEALED_JOB_KIND,
            resource_type="SEARCH_LEDGER_ENTRY",
            resource_id=source_experiment_id,
            payload=requested,
        )
        return AlphaQualificationJobResult(
            job_id=job.id,
            source_experiment_id=source_experiment_id,
            state=job.state,
        )


@router.post(
    "/portfolio-programs/{portfolio_program_id}/simulate-candidate",
    response_model=PortfolioSimulationResult,
)
def simulate_candidate(
    portfolio_program_id: UUID,
    payload: PortfolioSimulationInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> PortfolioSimulationResult:
    key = (idempotency_key or "").strip()
    if not key or len(key) > 200:
        raise QfError(
            "IDEMPOTENCY_KEY_REQUIRED",
            "Candidate simulation requires a 1..200 character Idempotency-Key.",
            422,
        )
    normalized: dict[str, object] = {
        "portfolio_program_id": str(portfolio_program_id),
        "alpha_ids": sorted(str(value) for value in payload.alpha_ids),
    }
    operation = f"SIMULATE_NAUTILUS_CANDIDATE:{portfolio_program_id}"
    factory = request.app.state.session_factory
    with factory.begin() as session:
        receipt, claimed, experiment_id = _claim_simulation_receipt(
            session,
            key=key,
            operation=operation,
            normalized=normalized,
        )
        if not claimed:
            return PortfolioSimulationResult.model_validate(receipt.response_json)

    try:
        result = simulate_portfolio_candidate(
            factory,
            portfolio_program_id=portfolio_program_id,
            alpha_ids=payload.alpha_ids,
            simulation_experiment_id=experiment_id,
        )
    except Exception as exc:
        _mark_simulation_retryable(
            factory,
            key=key,
            operation=operation,
            normalized=normalized,
            experiment_id=experiment_id,
            exc=exc,
        )
        raise

    response = PortfolioSimulationResult(
        candidate_id=result.candidate_id,
        approval_id=result.approval_id,
        simulation_experiment_id=result.simulation_experiment_id,
        selected_alpha_id=result.selected_alpha_id,
    )
    response_json = response.model_dump(mode="json")
    with factory.begin() as session:
        receipt = session.execute(
            select(PublicMutationReceipt)
            .where(PublicMutationReceipt.idempotency_key == key)
            .with_for_update()
        ).scalar_one_or_none()
        if receipt is None or not _simulation_receipt_matches(
            receipt,
            operation=operation,
            normalized=normalized,
        ):
            raise QfError(
                "IDEMPOTENCY_RECEIPT_CONFLICT",
                "Candidate simulation receipt changed before completion.",
                409,
            )
        if receipt.status_code == 200:
            return PortfolioSimulationResult.model_validate(receipt.response_json)
        if _pending_simulation_experiment_id(receipt) != experiment_id:
            raise QfError(
                "IDEMPOTENCY_RECEIPT_CONFLICT",
                "Candidate simulation receipt changed experiment identity.",
                409,
            )
        receipt.response_json = response_json
        receipt.status_code = 200
        session.flush()
    return response
