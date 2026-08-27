"""Controlled promotion endpoints for the Nautilus-first research pipeline."""

from __future__ import annotations

from datetime import UTC, datetime
from uuid import NAMESPACE_URL, UUID, uuid5

from fastapi import APIRouter, Header, Request
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import select

from db.models import DatasetRevision, Job, PublicMutationReceipt, SearchLedgerEntry
from errors import QfError
from jobs import enqueue_job
from quant_runtime.contracts import ExperimentMode
from quant_runtime.promotion import simulate_portfolio_candidate

router = APIRouter(prefix="/api/v1", tags=["research-runtime"])
SEALED_JOB_KIND = "SEALED_ALPHA_QUALIFICATION"


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
        source = session.get(SearchLedgerEntry, source_experiment_id)
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
    normalized = {
        "portfolio_program_id": str(portfolio_program_id),
        "alpha_ids": sorted(str(value) for value in payload.alpha_ids),
    }
    operation = f"SIMULATE_NAUTILUS_CANDIDATE:{portfolio_program_id}"
    factory = request.app.state.session_factory
    with factory() as session:
        existing = session.get(PublicMutationReceipt, key)
        if existing is not None:
            if existing.operation_name != operation or existing.normalized_request != normalized:
                raise QfError(
                    "IDEMPOTENCY_KEY_REUSED",
                    "The idempotency key belongs to a different request.",
                    409,
                )
            return PortfolioSimulationResult.model_validate(existing.response_json)

    experiment_id = uuid5(
        NAMESPACE_URL,
        f"quazonai:portfolio-candidate:{portfolio_program_id}:{key}",
    )
    result = simulate_portfolio_candidate(
        factory,
        portfolio_program_id=portfolio_program_id,
        alpha_ids=payload.alpha_ids,
        simulation_experiment_id=experiment_id,
    )
    response = PortfolioSimulationResult(
        candidate_id=result.candidate_id,
        approval_id=result.approval_id,
        simulation_experiment_id=result.simulation_experiment_id,
        selected_alpha_id=result.selected_alpha_id,
    )
    response_json = response.model_dump(mode="json")
    with factory() as session, session.begin():
        existing = session.get(PublicMutationReceipt, key)
        if existing is not None:
            if existing.operation_name != operation or existing.normalized_request != normalized:
                raise QfError(
                    "IDEMPOTENCY_KEY_REUSED",
                    "The idempotency key belongs to a different request.",
                    409,
                )
            return PortfolioSimulationResult.model_validate(existing.response_json)
        session.add(
            PublicMutationReceipt(
                idempotency_key=key,
                operation_name=operation,
                normalized_request=normalized,
                response_json=response_json,
                status_code=200,
                created_at=datetime.now(UTC),
            )
        )
    return response
