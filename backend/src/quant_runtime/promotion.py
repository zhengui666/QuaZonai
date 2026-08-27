"""Evidence-gated promotion from Nautilus experiments into Alpha and Portfolio facts.

The Core never fabricates research evidence here. Discovery must already have a
successful remote Nautilus Search Ledger record, Alpha Qualification requires an
independent sealed rerun with aggregate-only disclosure, and a Portfolio
Candidate requires its own real Nautilus PORTFOLIO simulation before an approval
snapshot is created.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session, sessionmaker

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    DatasetRevision,
    PortfolioCandidate,
    PortfolioProgram,
    ResearchCharter,
    ResearchProgram,
    SearchLedgerEntry,
)
from errors import QfError
from quant_runtime.contracts import (
    PINNED_NAUTILUS_VERSION,
    BacktestExperimentRequest,
    ExperimentMode,
    StrategyArtifact,
)
from quant_runtime.ledger import ExperimentCoordinator

CATALOG_URI_PREFIX = "nautilus-catalog://"


def _now() -> datetime:
    return datetime.now(UTC)


def _require_real_transaction_evidence(entry: SearchLedgerEntry) -> dict[str, Any]:
    if (
        entry.state != "SUCCEEDED"
        or entry.runtime_name != "NAUTILUS_TRADER"
        or entry.runtime_version != PINNED_NAUTILUS_VERSION
    ):
        raise QfError(
            "NAUTILUS_EVIDENCE_NOT_QUALIFIABLE",
            "Promotion requires successful evidence from the pinned Nautilus runtime.",
            422,
        )
    evidence = entry.evidence_json or {}
    missing = [
        name
        for name in ("orders", "fills", "positions", "pnl", "statistics")
        if name not in evidence
    ]
    if missing:
        raise QfError(
            "NAUTILUS_EVIDENCE_INCOMPLETE",
            "Promotion requires the canonical Nautilus transaction record.",
            422,
            {"missing": missing},
        )
    if not evidence["orders"] or not evidence["fills"] or not evidence["positions"]:
        raise QfError(
            "NAUTILUS_TRANSACTION_EVIDENCE_EMPTY",
            "Promotion requires at least one real order, fill, and position.",
            422,
        )
    return evidence


def _source_request(entry: SearchLedgerEntry) -> BacktestExperimentRequest:
    try:
        request = BacktestExperimentRequest.model_validate(entry.request_json)
    except ValueError as exc:
        raise QfError(
            "EXPERIMENT_REQUEST_INVALID",
            "Search Ledger experiment request is not a valid Nautilus contract.",
            422,
        ) from exc
    if request.strategy.kind != "SOURCE_BUNDLE":
        raise QfError(
            "STRATEGY_ARTIFACT_NOT_PROMOTABLE",
            "Only a self-contained SOURCE_BUNDLE strategy can cross the research boundary.",
            422,
        )
    return request


def _catalog_key(dataset: DatasetRevision) -> str:
    uri = dataset.catalog_uri or ""
    if not uri.startswith(CATALOG_URI_PREFIX):
        raise QfError(
            "NAUTILUS_CATALOG_MISSING",
            "Dataset Revision is not linked to a Nautilus catalog.",
            422,
        )
    key = uri.removeprefix(CATALOG_URI_PREFIX)
    if not key:
        raise QfError("NAUTILUS_CATALOG_MISSING", "Nautilus catalog key is empty.", 422)
    return key


def _discovery_summary(entry: SearchLedgerEntry, evidence: dict[str, Any]) -> dict[str, Any]:
    return {
        "experiment_id": str(entry.id),
        "remote_run_id": entry.remote_run_id,
        "dataset_revision_id": str(entry.dataset_revision_id),
        "runtime_version": entry.runtime_version,
        "order_count": len(evidence.get("orders", [])),
        "fill_count": len(evidence.get("fills", [])),
        "position_count": len(evidence.get("positions", [])),
        "statistics": evidence.get("statistics", {}),
        "pnl": evidence.get("pnl", {}),
    }


def _select_sealed_dataset(
    session: Session,
    source_dataset: DatasetRevision,
    requested_id: UUID,
) -> DatasetRevision:
    dataset = session.get(DatasetRevision, requested_id)
    if dataset is None:
        raise QfError("DATASET_REVISION_NOT_FOUND", "Sealed Dataset Revision does not exist.", 404)
    if dataset.partition != "SEALED":
        raise QfError(
            "DATASET_PARTITION_MISMATCH",
            "Alpha Qualification requires an isolated SEALED Dataset Revision.",
            422,
        )
    if dataset.quality_state != "VALID" or dataset.point_in_time_state != "VALID":
        raise QfError(
            "DATASET_GOVERNANCE_FAILED",
            "Sealed Dataset Revision must pass quality and point-in-time governance.",
            422,
        )
    source_scope = set(source_dataset.instrument_scope or [])
    sealed_scope = set(dataset.instrument_scope or [])
    if not source_scope or not source_scope.issubset(sealed_scope):
        raise QfError(
            "SEALED_DATASET_SCOPE_MISMATCH",
            "Sealed Dataset Revision does not cover the Discovery instrument scope.",
            422,
        )
    return dataset


def qualify_alpha(
    factory: sessionmaker[Session],
    *,
    source_experiment_id: UUID,
    sealed_dataset_revision_id: UUID,
    name: str | None = None,
    role: str = "PRIMARY_ALPHA",
) -> AlphaQualification:
    """Run independent sealed evaluation and promote the real Discovery evidence."""
    with factory() as session:
        existing = session.scalar(
            select(AlphaQualification).where(
                AlphaQualification.source_experiment_id == source_experiment_id,
                AlphaQualification.state == "ACTIVE",
            )
        )
        if existing is not None:
            session.expunge(existing)
            return existing
        source = session.get(SearchLedgerEntry, source_experiment_id)
        if source is None:
            raise QfError("SEARCH_LEDGER_ENTRY_NOT_FOUND", "Discovery experiment does not exist.", 404)
        if source.mode != ExperimentMode.DISCOVERY.value:
            raise QfError(
                "ALPHA_SOURCE_MODE_INVALID",
                "Alpha Qualification requires a Discovery experiment.",
                422,
            )
        evidence = _require_real_transaction_evidence(source)
        source_request = _source_request(source)
        source_dataset = session.get(DatasetRevision, source.dataset_revision_id)
        if source_dataset is None:
            raise QfError("DATASET_REVISION_NOT_FOUND", "Discovery Dataset Revision is missing.", 404)
        sealed_dataset = _select_sealed_dataset(session, source_dataset, sealed_dataset_revision_id)
        sealed_request = source_request.model_copy(
            update={
                "experiment_id": uuid4(),
                "mode": ExperimentMode.SEALED,
                "dataset_revision_id": sealed_dataset.id,
                "catalog_key": _catalog_key(sealed_dataset),
            }
        )
        source_program_id = source.program_id
        source_branch_id = source.branch_id
        source_dataset_id = source_dataset.id
        source_universe = source_dataset.universe_name
        source_universe_version_id = source_dataset.universe_version_id
        discovery_summary = _discovery_summary(source, evidence)
        strategy_artifact = source_request.strategy.model_dump(mode="json")

    sealed_entry = ExperimentCoordinator(factory).execute(
        mission_id=None,
        program_id=source_program_id,
        branch_id=source_branch_id,
        request=sealed_request,
        sealed=True,
    )
    disclosure = sealed_entry.disclosure_json or {}
    if sealed_entry.evidence_json:
        raise QfError(
            "SEALED_RAW_EVIDENCE_PERSISTED",
            "Sealed evaluation crossed the Core disclosure boundary.",
            500,
        )
    if not disclosure.get("passed"):
        raise QfError(
            "SEALED_EVALUATION_FAILED",
            "Alpha did not pass independent sealed evaluation.",
            422,
            {"sealed_experiment_id": str(sealed_entry.id)},
        )

    with factory() as session, session.begin():
        source = session.get(SearchLedgerEntry, source_experiment_id)
        sealed = session.get(SearchLedgerEntry, sealed_entry.id)
        if source is None or sealed is None:
            raise QfError("SEARCH_LEDGER_ENTRY_NOT_FOUND", "Evaluation lineage disappeared.", 500)
        sealed.parent_entry_id = source.id
        existing = session.scalar(
            select(AlphaQualification).where(
                AlphaQualification.source_experiment_id == source_experiment_id,
                AlphaQualification.state == "ACTIVE",
            )
        )
        if existing is not None:
            session.expunge(existing)
            return existing
        program = session.get(ResearchProgram, source_program_id)
        charter = session.get(ResearchCharter, program.charter_id) if program is not None else None
        quality = min(1.0, 0.5 + min(float(disclosure.get("fill_count", 0)), 50.0) / 100.0)
        alpha = AlphaQualification(
            program_id=source_program_id,
            universe_version_id=source_universe_version_id,
            universe=source_universe or (charter.market_scope if charter else None),
            horizon=charter.prediction_horizon if charter else None,
            role=role,
            state="ACTIVE",
            name=name or f"Nautilus alpha {str(source_experiment_id)[:8]}",
            scope_json={
                "instrument_ids": source_request.instrument_ids,
                "dataset_revision_id": str(source_dataset_id),
            },
            degradation_state="HEALTHY",
            metrics={
                "search_adjusted_quality": quality,
                "discovery": discovery_summary,
                "sealed_disclosure": disclosure,
                "strategy_artifact": strategy_artifact,
            },
            lineage=[
                {
                    "kind": "NAUTILUS_DISCOVERY",
                    "experiment_id": str(source.id),
                    "remote_run_id": source.remote_run_id,
                    "dataset_revision_id": str(source.dataset_revision_id),
                },
                {
                    "kind": "SEALED_EVALUATION",
                    "experiment_id": str(sealed.id),
                    "remote_run_id": sealed.remote_run_id,
                    "dataset_revision_id": str(sealed.dataset_revision_id),
                    "raw_evidence_withheld": True,
                },
            ],
            created_at=_now(),
            source_experiment_id=source.id,
        )
        session.add(alpha)
        session.flush()
        session.expunge(alpha)
        return alpha


def _alpha_score(alpha: AlphaQualification) -> float:
    try:
        return float((alpha.metrics or {}).get("search_adjusted_quality", 0.0))
    except (TypeError, ValueError):
        return 0.0


def _load_portfolio_source(
    session: Session,
    alpha_ids: list[UUID],
) -> tuple[AlphaQualification, SearchLedgerEntry, BacktestExperimentRequest]:
    if not alpha_ids:
        raise QfError("ALPHA_SELECTION_EMPTY", "At least one qualified Alpha is required.", 422)
    alphas = list(
        session.scalars(
            select(AlphaQualification).where(
                AlphaQualification.id.in_(alpha_ids),
                AlphaQualification.state == "ACTIVE",
                AlphaQualification.degradation_state == "HEALTHY",
            )
        )
    )
    if len(alphas) != len(set(alpha_ids)):
        raise QfError(
            "ALPHA_NOT_PORTFOLIO_READY",
            "Every selected Alpha must be active, healthy, and qualified.",
            422,
        )
    selected = max(alphas, key=lambda item: (_alpha_score(item), str(item.id)))
    if selected.source_experiment_id is None:
        raise QfError("ALPHA_LINEAGE_MISSING", "Qualified Alpha has no source experiment.", 422)
    source = session.get(SearchLedgerEntry, selected.source_experiment_id)
    if source is None:
        raise QfError("SEARCH_LEDGER_ENTRY_NOT_FOUND", "Alpha source experiment is missing.", 500)
    _require_real_transaction_evidence(source)
    request = _source_request(source)
    if not (selected.metrics or {}).get("sealed_disclosure", {}).get("passed"):
        raise QfError(
            "ALPHA_SEALED_EVIDENCE_MISSING",
            "Portfolio construction requires a passed sealed disclosure.",
            422,
        )
    return selected, source, request


@dataclass(frozen=True, slots=True)
class CandidatePromotion:
    candidate_id: UUID
    approval_id: UUID
    simulation_experiment_id: UUID
    selected_alpha_id: UUID


def simulate_portfolio_candidate(
    factory: sessionmaker[Session],
    *,
    portfolio_program_id: UUID,
    alpha_ids: list[UUID],
) -> CandidatePromotion:
    """Optimize Alpha selection, run a real PORTFOLIO simulation, and freeze approval facts."""
    with factory() as session:
        portfolio_program = session.get(PortfolioProgram, portfolio_program_id)
        if portfolio_program is None:
            raise QfError("PORTFOLIO_PROGRAM_NOT_FOUND", "Portfolio Program does not exist.", 404)
        alpha, source, request = _load_portfolio_source(session, alpha_ids)
        simulation_request = request.model_copy(
            update={
                "experiment_id": uuid4(),
                "mode": ExperimentMode.PORTFOLIO,
                "tags": {
                    **request.tags,
                    "portfolio_program_id": str(portfolio_program_id),
                    "alpha_qualification_id": str(alpha.id),
                    "optimizer": "MAX_SEARCH_ADJUSTED_QUALITY_V1",
                    "target_weight": "1.0",
                },
            }
        )
        source_program_id = source.program_id
        source_branch_id = source.branch_id
        alpha_id = alpha.id
        alpha_name = alpha.name
        alpha_universe = alpha.universe
        alpha_quality = _alpha_score(alpha)
        sealed_summary = dict((alpha.metrics or {}).get("sealed_disclosure", {}))
        discovery_summary = dict((alpha.metrics or {}).get("discovery", {}))

    simulation = ExperimentCoordinator(factory).execute(
        mission_id=None,
        program_id=source_program_id,
        branch_id=source_branch_id,
        request=simulation_request,
        sealed=False,
    )
    simulation_evidence = _require_real_transaction_evidence(simulation)

    with factory() as session, session.begin():
        portfolio_program = session.execute(
            select(PortfolioProgram)
            .where(PortfolioProgram.id == portfolio_program_id)
            .with_for_update()
        ).scalar_one_or_none()
        if portfolio_program is None:
            raise QfError("PORTFOLIO_PROGRAM_NOT_FOUND", "Portfolio Program disappeared.", 500)
        persisted_alpha = session.get(AlphaQualification, alpha_id)
        if (
            persisted_alpha is None
            or persisted_alpha.state != "ACTIVE"
            or persisted_alpha.degradation_state != "HEALTHY"
        ):
            raise QfError("ALPHA_NOT_PORTFOLIO_READY", "Selected Alpha changed before promotion.", 409)
        request_json = BacktestExperimentRequest.model_validate(simulation.request_json)
        strategy = StrategyArtifact.model_validate(request_json.strategy)
        candidate = PortfolioCandidate(
            portfolio_program_id=portfolio_program.id,
            mandate_version_id=portfolio_program.mandate_version_id,
            mandate_name=portfolio_program.mandate_name,
            state="READY",
            universe_set_json=[alpha_universe] if alpha_universe else [],
            members=[
                {
                    "alpha_qualification_id": str(persisted_alpha.id),
                    "alpha_name": alpha_name,
                    "role": persisted_alpha.role,
                    "target_weight": 1.0,
                    "universe": alpha_universe,
                    "instrument_ids": request_json.instrument_ids,
                }
            ],
            metrics={
                "search_adjusted_quality": alpha_quality,
                "optimizer": {
                    "name": "MAX_SEARCH_ADJUSTED_QUALITY_V1",
                    "considered_alpha_ids": [str(item) for item in alpha_ids],
                    "selected_alpha_id": str(persisted_alpha.id),
                    "target_weight": 1.0,
                },
                "nautilus": {
                    "strategy_artifact": strategy.model_dump(mode="json"),
                    "evidence": simulation_evidence,
                    "dataset_revision_ids": [str(simulation.dataset_revision_id)],
                    "alpha_qualification_ids": [str(persisted_alpha.id)],
                    "instrument_scope": request_json.instrument_ids,
                    "data_requirements": {"nautilus_data_type": "QuoteTick"},
                    "backtest_run_config": {
                        "catalog_key": request_json.catalog_key,
                        "catalog_uri": f"{CATALOG_URI_PREFIX}{request_json.catalog_key}",
                        "mode": ExperimentMode.PORTFOLIO.value,
                        "start_time": (
                            request_json.start_time.isoformat() if request_json.start_time else None
                        ),
                        "end_time": (
                            request_json.end_time.isoformat() if request_json.end_time else None
                        ),
                    },
                    "venue_config": request_json.venue_config,
                    "risk_config": request_json.risk_config,
                    "discovery_summary": discovery_summary,
                    "sealed_summary": sealed_summary,
                    "robustness_summary": {
                        "status": "PASSED_SEALED_AND_PORTFOLIO_SIMULATION"
                    },
                },
            },
            created_at=_now(),
            simulation_experiment_id=simulation.id,
        )
        session.add(candidate)
        session.flush()
        portfolio_program.current_candidate_id = candidate.id
        portfolio_program.state = "CANDIDATE_READY"
        approval = ApprovalSnapshot(
            candidate_id=candidate.id,
            purpose="PAPER",
            state="PENDING",
            valid_until=_now() + timedelta(days=7),
            recommendation_rationale=(
                "Candidate passed sealed Alpha qualification and a real Nautilus transaction-level "
                "portfolio simulation."
            ),
            human_report={
                "summary": "Paper runtime validation is the next governed step.",
                "selected_alpha_id": str(persisted_alpha.id),
            },
            evidence_summary={
                "search_adjusted_quality": alpha_quality,
                "portfolio_simulation_experiment_id": str(simulation.id),
                "remote_run_id": simulation.remote_run_id,
            },
            capital_context={},
            risk_summary={},
            cost_summary={},
            capacity_summary={},
            changes_summary={},
        )
        session.add(approval)
        session.flush()
        return CandidatePromotion(
            candidate_id=candidate.id,
            approval_id=approval.id,
            simulation_experiment_id=simulation.id,
            selected_alpha_id=persisted_alpha.id,
        )
