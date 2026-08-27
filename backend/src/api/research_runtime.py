"""Controlled promotion endpoints for the Nautilus-first research pipeline."""

from __future__ import annotations

from datetime import UTC, datetime
from uuid import NAMESPACE_URL, UUID, uuid5

from fastapi import APIRouter, Header, Request

from db.models import PublicMutationReceipt
from errors import QfError
from pydantic import BaseModel, ConfigDict, Field

from quant_runtime.promotion import qualify_alpha, simulate_portfolio_candidate

router = APIRouter(prefix="/api/v1", tags=["research-runtime"])


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class AlphaQualificationInput(StrictModel):
    sealed_dataset_revision_id: UUID
    name: str | None = Field(default=None, max_length=240)
    role: str = Field(default="PRIMARY_ALPHA", min_length=1, max_length=100)


class AlphaQualificationResult(StrictModel):
    alpha_qualification_id: UUID
    source_experiment_id: UUID
    state: str
    degradation_state: str


class PortfolioSimulationInput(StrictModel):
    alpha_ids: list[UUID] = Field(min_length=1)


class PortfolioSimulationResult(StrictModel):
    candidate_id: UUID
    approval_id: UUID
    simulation_experiment_id: UUID
    selected_alpha_id: UUID


@router.post(
    "/research-experiments/{source_experiment_id}/qualify-alpha",
    response_model=AlphaQualificationResult,
)
def qualify_research_experiment(
    source_experiment_id: UUID,
    payload: AlphaQualificationInput,
    request: Request,
) -> AlphaQualificationResult:
    alpha = qualify_alpha(
        request.app.state.session_factory,
        source_experiment_id=source_experiment_id,
        sealed_dataset_revision_id=payload.sealed_dataset_revision_id,
        name=payload.name,
        role=payload.role,
    )
    assert alpha.source_experiment_id is not None
    return AlphaQualificationResult(
        alpha_qualification_id=alpha.id,
        source_experiment_id=alpha.source_experiment_id,
        state=alpha.state,
        degradation_state=alpha.degradation_state,
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
