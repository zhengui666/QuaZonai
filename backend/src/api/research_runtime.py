"""Controlled promotion endpoints for the Nautilus-first research pipeline."""

from __future__ import annotations

from uuid import UUID

from fastapi import APIRouter, Request
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
) -> PortfolioSimulationResult:
    result = simulate_portfolio_candidate(
        request.app.state.session_factory,
        portfolio_program_id=portfolio_program_id,
        alpha_ids=payload.alpha_ids,
    )
    return PortfolioSimulationResult(
        candidate_id=result.candidate_id,
        approval_id=result.approval_id,
        simulation_experiment_id=result.simulation_experiment_id,
        selected_alpha_id=result.selected_alpha_id,
    )
