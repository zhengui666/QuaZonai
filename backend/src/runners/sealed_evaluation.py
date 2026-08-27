"""Isolated sealed-data evaluator using a separately configured remote runtime."""

from __future__ import annotations

from uuid import UUID, uuid4

from db.session import create_database_engine, create_session_factory
from db.models import SearchLedgerEntry
from errors import QfError
from quant_runtime.contracts import BacktestExperimentRequest, ExperimentMode
from quant_runtime.ledger import ExperimentCoordinator
from settings import Settings


def run_sealed_evaluation(
    settings: Settings,
    *,
    source_experiment_id: UUID,
    sealed_dataset_revision_id: UUID,
) -> SearchLedgerEntry:
    """Re-run the same artifact/config against sealed data and return disclosure only."""
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    try:
        with factory() as session:
            source = session.get(SearchLedgerEntry, source_experiment_id)
            if source is None:
                raise QfError(
                    "SEARCH_LEDGER_ENTRY_NOT_FOUND",
                    "Source Discovery experiment does not exist.",
                    404,
                )
            if source.state != "SUCCEEDED" or source.mode == ExperimentMode.SEALED.value:
                raise QfError(
                    "SEALED_EVALUATION_SOURCE_INVALID",
                    "Sealed evaluation requires a successful Discovery/Portfolio experiment.",
                    422,
                )
            request = BacktestExperimentRequest.model_validate(source.request_json).model_copy(
                update={
                    "experiment_id": uuid4(),
                    "mode": ExperimentMode.SEALED,
                    "dataset_revision_id": sealed_dataset_revision_id,
                }
            )
            program_id = source.program_id
            branch_id = source.branch_id
        coordinator = ExperimentCoordinator(factory)
        return coordinator.execute(
            mission_id=None,
            program_id=program_id,
            branch_id=branch_id,
            request=request,
            sealed=True,
        )
    finally:
        engine.dispose()
