"""Isolated sealed-data evaluator using a separately configured remote runtime."""

from __future__ import annotations

from uuid import UUID, uuid4

from db.models import DatasetRevision, SearchLedgerEntry
from db.session import create_database_engine, create_session_factory
from errors import QfError
from quant_runtime.contracts import BacktestExperimentRequest, ExperimentMode
from quant_runtime.ledger import ExperimentCoordinator
from settings import Settings

CATALOG_URI_PREFIX = "nautilus-catalog://"


def run_sealed_evaluation(
    settings: Settings,
    *,
    source_experiment_id: UUID,
    sealed_dataset_revision_id: UUID,
) -> SearchLedgerEntry:
    """Re-run the same artifact/config once against sealed data and return disclosure only."""
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
            sealed_dataset = session.get(DatasetRevision, sealed_dataset_revision_id)
            if sealed_dataset is None:
                raise QfError(
                    "DATASET_REVISION_NOT_FOUND",
                    "Sealed Dataset Revision does not exist.",
                    404,
                )
            if sealed_dataset.partition != "SEALED":
                raise QfError(
                    "DATASET_PARTITION_MISMATCH",
                    "Sealed evaluation requires a SEALED Dataset Revision.",
                    422,
                    {"actual": sealed_dataset.partition},
                )
            catalog_uri = sealed_dataset.catalog_uri or ""
            if not catalog_uri.startswith(CATALOG_URI_PREFIX):
                raise QfError(
                    "NAUTILUS_CATALOG_MISSING",
                    "Sealed Dataset Revision is not linked to a Nautilus catalog.",
                    422,
                )
            catalog_key = catalog_uri.removeprefix(CATALOG_URI_PREFIX)
            if not catalog_key:
                raise QfError(
                    "NAUTILUS_CATALOG_MISSING",
                    "Sealed Dataset Revision catalog URI has no catalog key.",
                    422,
                )
            request = BacktestExperimentRequest.model_validate(source.request_json).model_copy(
                update={
                    "experiment_id": uuid4(),
                    "mode": ExperimentMode.SEALED,
                    "dataset_revision_id": sealed_dataset_revision_id,
                    "catalog_key": catalog_key,
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
            parent_entry_id=source_experiment_id,
        )
    finally:
        engine.dispose()
