"""Evidence-gated promotion from Nautilus experiments into Alpha and Portfolio facts.

The Core never fabricates research evidence here. Discovery must already have a
successful remote Nautilus Search Ledger record, Alpha Qualification requires an
independent sealed rerun with aggregate-only disclosure, and a Portfolio
Candidate requires its own real Nautilus PORTFOLIO simulation before an approval
snapshot is created.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import func, select
from sqlalchemy.orm import Session, sessionmaker

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    DatasetRevision,
    DegradationFollowup,
    DownstreamSystem,
    PortfolioCandidate,
    PortfolioMandate,
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
_UNRESOLVED_HORIZONS = {
    "",
    "system inferred",
    "system_inferred",
    "unknown",
    "unspecified",
    "tbd",
}
_RECOGNIZED_ALPHA_ROLES = {
    "PRIMARY_ALPHA",
    "DIVERSIFIER_ALPHA",
    "HEDGE_ALPHA",
    "REGIME_SIGNAL",
    "RISK_MODULATOR",
    "SHADOW_ALPHA",
}
_PROMOTABLE_ALPHA_ROLES = _RECOGNIZED_ALPHA_ROLES - {"SHADOW_ALPHA"}


def _now() -> datetime:
    return datetime.now(UTC)


def _evidence_program_lineage_ids(session: Session, program_id: UUID) -> set[UUID]:
    """Return the complete immutable evidence-inheritance lineage for scoring exposure."""
    lineage: set[UUID] = set()
    current_id: UUID | None = program_id
    while current_id is not None:
        if current_id in lineage:
            raise QfError(
                "RESEARCH_PROGRAM_EVIDENCE_LINEAGE_CYCLE",
                "Research Program evidence inheritance contains a cycle.",
                500,
            )
        lineage.add(current_id)
        program = session.get(ResearchProgram, current_id)
        if program is None:
            raise QfError(
                "RESEARCH_PROGRAM_NOT_FOUND",
                "Research Program in evidence lineage does not exist.",
                404,
            )
        current_id = program.evidence_inherited_from_program_id
    return lineage


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
    if str(evidence.get("experiment_id", "")) != str(entry.id):
        raise QfError(
            "NAUTILUS_EVIDENCE_IDENTITY_MISMATCH",
            "Nautilus evidence is not bound to its Search Ledger experiment id.",
            422,
        )
    if str(evidence.get("mode", "")) != entry.mode:
        raise QfError(
            "NAUTILUS_EVIDENCE_MODE_MISMATCH",
            "Nautilus evidence mode does not match its Search Ledger entry.",
            422,
        )
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


def _quality_metric(value: Any, aliases: set[str]) -> float | None:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = str(key).casefold().replace(" ", "_").replace("-", "_")
            if normalized in aliases:
                try:
                    metric = float(item)
                except (TypeError, ValueError):
                    metric = None
                if metric is not None and math.isfinite(metric):
                    return metric
            nested = _quality_metric(item, aliases)
            if nested is not None:
                return nested
    elif isinstance(value, list):
        for item in value:
            nested = _quality_metric(item, aliases)
            if nested is not None:
                return nested
    return None


def _discovery_quality_score(
    evidence: dict[str, Any],
    *,
    search_attempt_count: int = 1,
) -> tuple[float, dict[str, Any]]:
    """Produce an auditable score from public Discovery evidence and search exposure."""
    statistics = evidence.get("statistics", {})
    sharpe = _quality_metric(
        statistics,
        {"sharpe", "sharpe_ratio", "sharpe_ratio_(252_days)", "sharpe_ratio_252_days"},
    )
    total_return = _quality_metric(
        statistics,
        {"return", "total_return", "cumulative_return", "annual_return", "annualized_return"},
    )
    max_drawdown = _quality_metric(
        statistics, {"max_drawdown", "maximum_drawdown", "max_drawdown_(all)"}
    )
    profit_factor = _quality_metric(
        {"statistics": statistics, "pnl": evidence.get("pnl", {})},
        {"profit_factor"},
    )
    total_pnl = _quality_metric(
        evidence.get("pnl", {}),
        {"pnl_(total)", "pnl_total", "total_pnl", "net_pnl"},
    )
    fill_count = len(evidence.get("fills", []))

    score = 0.50
    if sharpe is not None:
        score += 0.20 * math.tanh(sharpe / 3.0)
    if total_return is not None:
        score += 0.20 * math.tanh(total_return / 0.25)
    # Absolute PnL is audit evidence only. It is deliberately not a scoring fallback:
    # account size, trade notional and currency scale are Mission-controlled inputs.
    if profit_factor is not None:
        score += 0.10 * math.tanh((profit_factor - 1.0) / 2.0)
    if max_drawdown is not None:
        score -= 0.15 * min(abs(max_drawdown), 1.0)
    score += 0.05 * math.tanh(math.log1p(fill_count) / 3.0)
    attempts = max(1, int(search_attempt_count))
    search_penalty = min(0.20, 0.025 * math.log1p(attempts - 1))
    score -= search_penalty
    bounded = round(min(0.99, max(0.01, score)), 8)
    return bounded, {
        "model": "DISCOVERY_PUBLIC_PERFORMANCE_V2",
        "score": bounded,
        "sharpe": sharpe,
        "total_return": total_return,
        "max_drawdown": max_drawdown,
        "profit_factor": profit_factor,
        "total_pnl": total_pnl,
        "absolute_pnl_used_for_scoring": False,
        "fill_count": fill_count,
        "search_attempt_count": attempts,
        "search_exposure_penalty": round(search_penalty, 8),
        "sealed_evidence_used_for_scoring": False,
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
    if source_dataset.universe_version_id is None:
        raise QfError(
            "DISCOVERY_UNIVERSE_VERSION_MISSING",
            "Discovery evidence must bind a concrete Universe Version before qualification.",
            422,
        )
    if dataset.universe_version_id != source_dataset.universe_version_id:
        raise QfError(
            "SEALED_UNIVERSE_VERSION_MISMATCH",
            "Sealed evaluation must use the same frozen Universe Version as Discovery.",
            422,
            {
                "discovery_universe_version_id": str(source_dataset.universe_version_id),
                "sealed_universe_version_id": (
                    str(dataset.universe_version_id) if dataset.universe_version_id else None
                ),
            },
        )
    source_scope = set(source_dataset.instrument_scope or [])
    sealed_scope = set(dataset.instrument_scope or [])
    if not source_scope or not source_scope.issubset(sealed_scope):
        raise QfError(
            "SEALED_DATASET_SCOPE_MISMATCH",
            "Sealed Dataset Revision does not cover the Discovery instrument scope.",
            422,
        )
    source_catalog_uri = (source_dataset.catalog_uri or "").strip()
    sealed_catalog_uri = (dataset.catalog_uri or "").strip()
    if source_catalog_uri and sealed_catalog_uri and source_catalog_uri == sealed_catalog_uri:
        raise QfError(
            "SEALED_DATASET_NOT_INDEPENDENT",
            "Sealed evaluation must use a catalog revision independent from Discovery.",
            422,
        )

    source_start = source_dataset.event_start
    source_end = source_dataset.event_end
    sealed_start = dataset.event_start
    sealed_end = dataset.event_end
    bounds = (source_start, source_end, sealed_start, sealed_end)
    if any(value is None for value in bounds):
        raise QfError(
            "SEALED_DATASET_INDEPENDENCE_UNVERIFIABLE",
            "Discovery and Sealed revisions require explicit event-time bounds to prove holdout independence.",
            422,
        )
    assert source_start is not None and source_end is not None
    assert sealed_start is not None and sealed_end is not None
    if any(value.tzinfo is None or value.utcoffset() is None for value in bounds if value is not None):
        raise QfError(
            "SEALED_DATASET_INDEPENDENCE_UNVERIFIABLE",
            "Discovery and Sealed event-time bounds must be timezone-aware.",
            422,
        )
    if source_start >= source_end or sealed_start >= sealed_end:
        raise QfError(
            "SEALED_DATASET_INDEPENDENCE_UNVERIFIABLE",
            "Discovery and Sealed revisions require valid event-time intervals.",
            422,
        )
    if source_start <= sealed_end and sealed_start <= source_end:
        raise QfError(
            "SEALED_DATASET_TIME_OVERLAP",
            "Sealed evaluation event-time coverage must not overlap Discovery coverage.",
            422,
        )
    return dataset


def _sealed_dataset_bounds(dataset: DatasetRevision) -> tuple[datetime, datetime]:
    start = dataset.event_start
    end = dataset.event_end
    if start is None or end is None:
        raise QfError(
            "SEALED_DATASET_TIME_RANGE_MISSING",
            "Sealed evaluation requires explicit immutable event-time bounds.",
            422,
        )
    if (
        start.tzinfo is None
        or start.utcoffset() is None
        or end.tzinfo is None
        or end.utcoffset() is None
        or start >= end
    ):
        raise QfError(
            "SEALED_DATASET_TIME_RANGE_INVALID",
            "Sealed Dataset Revision has invalid event-time bounds.",
            422,
        )
    return start, end


def _qualification_contract(
    *,
    sealed_dataset_revision_id: UUID,
    name: str | None,
    role: str,
) -> dict[str, Any]:
    return {
        "sealed_dataset_revision_id": str(sealed_dataset_revision_id),
        "requested_name": name,
        "role": role,
    }


def _assert_qualification_replay(
    existing: AlphaQualification,
    contract: dict[str, Any],
) -> None:
    if (existing.metrics or {}).get("qualification_contract") != contract:
        raise QfError(
            "ALPHA_QUALIFICATION_CONTRACT_REUSED",
            "Existing Alpha Qualification is bound to a different immutable qualification request.",
            409,
        )


def _has_degradation_observation(session: Session, alpha_id: UUID) -> bool:
    return session.scalar(
        select(DegradationFollowup.id)
        .where(DegradationFollowup.alpha_qualification_id == alpha_id)
        .limit(1)
    ) is not None


def qualify_alpha(
    factory: sessionmaker[Session],
    *,
    source_experiment_id: UUID,
    sealed_dataset_revision_id: UUID,
    name: str | None = None,
    role: str = "PRIMARY_ALPHA",
) -> AlphaQualification:
    """Run independent sealed evaluation and promote the real Discovery evidence."""
    if role not in _RECOGNIZED_ALPHA_ROLES:
        raise QfError(
            "ALPHA_ROLE_INVALID",
            "Alpha Qualification role is not recognized by the governed V1 contract.",
            422,
            {"role": role},
        )
    qualification_contract = _qualification_contract(
        sealed_dataset_revision_id=sealed_dataset_revision_id,
        name=name,
        role=role,
    )
    with factory() as session:
        existing = session.scalar(
            select(AlphaQualification).where(
                AlphaQualification.source_experiment_id == source_experiment_id,
                AlphaQualification.state == "ACTIVE",
            )
        )
        if existing is not None:
            _assert_qualification_replay(existing, qualification_contract)
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
        program = session.get(ResearchProgram, source.program_id)
        charter = session.get(ResearchCharter, program.charter_id) if program is not None else None
        if charter is None:
            raise QfError(
                "RESEARCH_CHARTER_MISSING",
                "Discovery evidence has no frozen Research Charter.",
                422,
            )
        source_horizon = (charter.prediction_horizon or "").strip()
        if source_horizon.casefold().replace("-", "_") in _UNRESOLVED_HORIZONS:
            raise QfError(
                "ALPHA_HORIZON_UNRESOLVED",
                "Alpha Qualification requires a concrete frozen prediction horizon.",
                422,
            )
        if source_dataset.universe_version_id is None:
            raise QfError(
                "DISCOVERY_UNIVERSE_VERSION_MISSING",
                "Discovery evidence must bind a concrete Universe Version before qualification.",
                422,
            )
        charter_universe_ids: set[UUID] = set()
        for raw_universe_id in charter.universe_version_ids or []:
            try:
                charter_universe_ids.add(UUID(str(raw_universe_id)))
            except ValueError:
                continue
        if source_dataset.universe_version_id not in charter_universe_ids:
            raise QfError(
                "DISCOVERY_UNIVERSE_CHARTER_MISMATCH",
                "Discovery Dataset Revision is outside the frozen Research Charter universe set.",
                422,
            )
        sealed_dataset = _select_sealed_dataset(session, source_dataset, sealed_dataset_revision_id)
        sealed_start, sealed_end = _sealed_dataset_bounds(sealed_dataset)
        sealed_request = source_request.model_copy(
            update={
                "experiment_id": uuid4(),
                "mode": ExperimentMode.SEALED,
                "dataset_revision_id": sealed_dataset.id,
                "catalog_key": _catalog_key(sealed_dataset),
                "start_time": sealed_start,
                "end_time": sealed_end,
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
        parent_entry_id=source_experiment_id,
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
        if sealed.parent_entry_id != source.id:
            raise QfError(
                "SEALED_LINEAGE_MISMATCH",
                "Sealed exposure was not reserved against its Discovery source.",
                500,
            )
        existing = session.scalar(
            select(AlphaQualification).where(
                AlphaQualification.source_experiment_id == source_experiment_id,
                AlphaQualification.state == "ACTIVE",
            )
        )
        if existing is not None:
            _assert_qualification_replay(existing, qualification_contract)
            session.expunge(existing)
            return existing
        quality_tier = str(disclosure.get("quality_tier", ""))
        if quality_tier != "QUALIFIED":
            raise QfError(
                "SEALED_DISCLOSURE_INVALID",
                "Sealed Level-1 disclosure did not return a recognized qualification category.",
                500,
            )
        evidence_program_ids = _evidence_program_lineage_ids(session, source_program_id)
        search_attempt_count = int(
            session.scalar(
                select(func.count())
                .select_from(SearchLedgerEntry)
                .where(
                    SearchLedgerEntry.program_id.in_(evidence_program_ids),
                    SearchLedgerEntry.mode == ExperimentMode.DISCOVERY.value,
                )
            )
            or 1
        )
        quality, quality_model = _discovery_quality_score(
            evidence, search_attempt_count=search_attempt_count
        )
        alpha = AlphaQualification(
            program_id=source_program_id,
            universe_version_id=source_universe_version_id,
            universe=source_universe or str(source_universe_version_id),
            horizon=source_horizon,
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
                "quality_model": quality_model,
                "discovery": discovery_summary,
                "sealed_disclosure": disclosure,
                "strategy_artifact": strategy_artifact,
                "qualification_contract": qualification_contract,
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


def _canonical_alpha_ids(alpha_ids: list[UUID]) -> list[UUID]:
    return sorted(set(alpha_ids), key=str)


def _candidate_quality(candidate: PortfolioCandidate) -> float:
    raw = (candidate.metrics or {}).get("search_adjusted_quality")
    if raw is None:
        raise QfError(
            "CANDIDATE_BASELINE_EVIDENCE_MISSING",
            "Current Candidate lacks a numeric search-adjusted quality baseline.",
            422,
        )
    try:
        value = float(raw)
    except (TypeError, ValueError) as exc:
        raise QfError(
            "CANDIDATE_BASELINE_EVIDENCE_MISSING",
            "Current Candidate lacks a numeric search-adjusted quality baseline.",
            422,
        ) from exc
    if value != value or value in {float("inf"), float("-inf")}:
        raise QfError(
            "CANDIDATE_BASELINE_EVIDENCE_INVALID",
            "Current Candidate quality baseline must be finite.",
            422,
        )
    return value


def _material_improvement_delta(mandate: PortfolioMandate) -> float:
    spec = mandate.spec_json or {}
    configured = spec.get("material_improvement_gate", {})
    if configured is None:
        configured = {}
    if not isinstance(configured, dict):
        raise QfError(
            "MATERIAL_IMPROVEMENT_POLICY_INVALID",
            "material_improvement_gate must be an object.",
            422,
        )
    raw = configured.get(
        "min_search_adjusted_quality_delta",
        spec.get("min_search_adjusted_quality_delta", 0.0),
    )
    delta = _number(raw, key="min_search_adjusted_quality_delta")
    if delta < 0:
        raise QfError(
            "MATERIAL_IMPROVEMENT_POLICY_INVALID",
            "Material Improvement delta cannot be negative.",
            422,
        )
    return delta


def _require_material_improvement(
    current: PortfolioCandidate | None,
    *,
    proposed_quality: float,
    mandate: PortfolioMandate,
) -> None:
    if current is None:
        return
    baseline = _candidate_quality(current)
    minimum_delta = _material_improvement_delta(mandate)
    required = baseline + minimum_delta
    if proposed_quality <= required:
        raise QfError(
            "CANDIDATE_NOT_MATERIALLY_IMPROVED",
            "Simulation evidence does not materially improve the current Candidate.",
            422,
            {
                "current_quality": baseline,
                "proposed_quality": proposed_quality,
                "minimum_delta": minimum_delta,
                "required_quality_exclusive": required,
            },
        )


def _select_portfolio_alpha(
    mandate: PortfolioMandate,
    alphas: list[AlphaQualification],
) -> AlphaQualification:
    eligible: list[AlphaQualification] = []
    excluded: list[str] = []
    for alpha in alphas:
        try:
            _validate_mandate_before_simulation(mandate, alpha)
        except QfError as exc:
            if exc.code == "PORTFOLIO_MANDATE_UNIVERSE_UNSATISFIED":
                excluded.append(str(alpha.id))
                continue
            raise
        eligible.append(alpha)
    if not eligible:
        raise QfError(
            "PORTFOLIO_MANDATE_NO_ELIGIBLE_ALPHA",
            "No selected Alpha satisfies the bound Portfolio Mandate.",
            422,
            {"excluded_alpha_ids": excluded},
        )
    return max(eligible, key=lambda item: (_alpha_score(item), str(item.id)))


def _load_portfolio_source(
    session: Session,
    alpha_ids: list[UUID],
    mandate: PortfolioMandate,
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
    if len(alphas) != len(set(alpha_ids)) or any(
        _has_degradation_observation(session, item.id) for item in alphas
    ):
        raise QfError(
            "ALPHA_NOT_PORTFOLIO_READY",
            "Every selected Alpha must be active, qualified, and free of degradation observations.",
            422,
        )
    invalid_roles = sorted(
        {item.role for item in alphas if item.role not in _RECOGNIZED_ALPHA_ROLES}
    )
    if invalid_roles:
        raise QfError(
            "ALPHA_ROLE_INVALID",
            "Portfolio promotion encountered an unrecognized Alpha role.",
            422,
            {"roles": invalid_roles},
        )
    if any(item.role == "SHADOW_ALPHA" for item in alphas):
        raise QfError(
            "SHADOW_ALPHA_NOT_HANDOFF_ELIGIBLE",
            "Shadow Alpha cannot directly form a promotable Handoff Candidate.",
            422,
        )
    if any(item.role not in _PROMOTABLE_ALPHA_ROLES for item in alphas):
        raise QfError(
            "ALPHA_ROLE_NOT_PROMOTABLE",
            "Selected Alpha role is not eligible for Candidate promotion.",
            422,
        )
    selected = _select_portfolio_alpha(mandate, alphas)
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


_SUPPORTED_MANDATE_CONSTRAINTS = {
    "max_single_alpha_weight",
    "max_single_weight",
    "max_concentration",
    "allowed_universe_version_ids",
    "allowed_universes",
    "max_drawdown",
    "max_turnover",
    "max_leverage",
    "max_margin_usage",
}

_UNSUPPORTED_MANDATE_CONSTRAINTS = {"max_cost_bps", "min_capacity_ratio"}


def _bound_mandate(session: Session, program: PortfolioProgram) -> PortfolioMandate:
    mandate = session.scalar(
        select(PortfolioMandate)
        .where(
            PortfolioMandate.latest_version_id == program.mandate_version_id,
            PortfolioMandate.enabled.is_(True),
            PortfolioMandate.state == "ACTIVE",
        )
        .limit(1)
    )
    if mandate is None:
        raise QfError(
            "PORTFOLIO_MANDATE_NOT_ACTIVE",
            "Portfolio Program must bind an enabled active Mandate Version.",
            422,
            {"mandate_version_id": str(program.mandate_version_id)},
        )
    return mandate


def _mandate_constraints(mandate: PortfolioMandate) -> dict[str, Any]:
    spec = mandate.spec_json or {}
    declared = spec.get("constraints", {})
    if declared is None:
        declared = {}
    if not isinstance(declared, dict):
        raise QfError(
            "PORTFOLIO_MANDATE_INVALID",
            "Portfolio Mandate constraints must be an object.",
            422,
        )
    constraints = dict(declared)
    for key in _SUPPORTED_MANDATE_CONSTRAINTS | _UNSUPPORTED_MANDATE_CONSTRAINTS:
        if key in spec and key not in constraints:
            constraints[key] = spec[key]
    unknown = sorted(set(constraints) - _SUPPORTED_MANDATE_CONSTRAINTS)
    if unknown:
        raise QfError(
            "PORTFOLIO_MANDATE_CONSTRAINT_UNSUPPORTED",
            "The bound Portfolio Mandate declares constraints this optimizer cannot evaluate.",
            422,
            {"constraints": unknown},
        )
    return constraints


def _number(value: object, *, key: str) -> float:
    try:
        result = float(value)  # type: ignore[arg-type]
    except (TypeError, ValueError) as exc:
        raise QfError(
            "PORTFOLIO_MANDATE_INVALID",
            "Portfolio Mandate numeric constraint is invalid.",
            422,
            {"constraint": key},
        ) from exc
    if result != result or result in {float("inf"), float("-inf")}:
        raise QfError(
            "PORTFOLIO_MANDATE_INVALID",
            "Portfolio Mandate numeric constraint must be finite.",
            422,
            {"constraint": key},
        )
    return result


def _first_constraint(constraints: dict[str, Any], *keys: str) -> tuple[str, Any] | None:
    for key in keys:
        if key in constraints:
            return key, constraints[key]
    return None


def _validate_mandate_before_simulation(
    mandate: PortfolioMandate,
    alpha: AlphaQualification,
) -> dict[str, Any]:
    constraints = _mandate_constraints(mandate)
    concentration = _first_constraint(
        constraints, "max_single_alpha_weight", "max_single_weight", "max_concentration"
    )
    if concentration is not None and _number(concentration[1], key=concentration[0]) < 1.0:
        raise QfError(
            "PORTFOLIO_MANDATE_CONCENTRATION_UNSATISFIED",
            "The current single-Alpha optimizer cannot satisfy the Mandate concentration cap.",
            422,
            {"constraint": concentration[0], "required_weight": 1.0},
        )
    allowed_ids = constraints.get("allowed_universe_version_ids")
    if allowed_ids is not None:
        if not isinstance(allowed_ids, list) or alpha.universe_version_id is None:
            raise QfError(
                "PORTFOLIO_MANDATE_UNIVERSE_UNSATISFIED",
                "Mandate Universe-Version constraint cannot be satisfied by the selected Alpha.",
                422,
            )
        allowed = {str(value) for value in allowed_ids}
        if str(alpha.universe_version_id) not in allowed:
            raise QfError(
                "PORTFOLIO_MANDATE_UNIVERSE_UNSATISFIED",
                "Selected Alpha is outside the Mandate's allowed Universe Versions.",
                422,
            )
    allowed_names = constraints.get("allowed_universes")
    if allowed_names is not None:
        if not isinstance(allowed_names, list) or not alpha.universe:
            raise QfError(
                "PORTFOLIO_MANDATE_UNIVERSE_UNSATISFIED",
                "Mandate Universe constraint cannot be satisfied by the selected Alpha.",
                422,
            )
        if alpha.universe not in {str(value) for value in allowed_names}:
            raise QfError(
                "PORTFOLIO_MANDATE_UNIVERSE_UNSATISFIED",
                "Selected Alpha is outside the Mandate's allowed Universes.",
                422,
            )
    return constraints


def _metric_from_evidence(value: object, aliases: set[str]) -> float | None:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = str(key).casefold().replace(" ", "_").replace("-", "_")
            if normalized in aliases:
                try:
                    return float(item)
                except (TypeError, ValueError):
                    pass
            nested = _metric_from_evidence(item, aliases)
            if nested is not None:
                return nested
    elif isinstance(value, list):
        for item in value:
            nested = _metric_from_evidence(item, aliases)
            if nested is not None:
                return nested
    return None


def _require_evidence_metric(
    evidence: dict[str, Any],
    *,
    constraint: str,
    aliases: set[str],
) -> float:
    value = _metric_from_evidence(evidence, aliases)
    if value is None or not math.isfinite(value):
        raise QfError(
            "PORTFOLIO_MANDATE_CONSTRAINT_EVIDENCE_MISSING",
            "Nautilus simulation did not produce finite evidence required by the Portfolio Mandate.",
            422,
            {"constraint": constraint},
        )
    return value


def _validate_mandate_after_simulation(
    constraints: dict[str, Any],
    evidence: dict[str, Any],
) -> None:
    checks = {
        "max_drawdown": ({"max_drawdown", "maxdrawdown"}, "max"),
        "max_turnover": ({"turnover", "portfolio_turnover"}, "max"),
        "max_leverage": ({"leverage", "gross_leverage"}, "max"),
        "max_margin_usage": ({"margin_usage", "margin_utilization"}, "max"),
    }
    for key, (aliases, direction) in checks.items():
        if key not in constraints:
            continue
        actual = _require_evidence_metric(evidence, constraint=key, aliases=aliases)
        if key == "max_drawdown":
            actual = abs(actual)
        limit = _number(constraints[key], key=key)
        violated = actual > limit if direction == "max" else actual < limit
        if violated:
            raise QfError(
                "PORTFOLIO_MANDATE_CONSTRAINT_FAILED",
                "Nautilus transaction-level simulation violates the bound Portfolio Mandate.",
                422,
                {"constraint": key, "actual": actual, "limit": limit},
            )


def _evidence_number(value: object) -> float | None:
    if isinstance(value, bool) or value is None:
        return None
    if isinstance(value, (int, float)):
        result = float(value)
    elif isinstance(value, str):
        token = value.strip().replace(",", "").replace("_", "").split()
        if not token:
            return None
        try:
            result = float(token[0])
        except ValueError:
            return None
    else:
        return None
    return result if math.isfinite(result) else None


def _exposure_side_sign(value: object) -> float | None:
    token = str(value or "").strip().upper()
    if token in {"LONG", "BUY"}:
        return 1.0
    if token in {"SHORT", "SELL"}:
        return -1.0
    return None


def _executed_instrument_weights(
    evidence: dict[str, Any],
    requested_instruments: list[str],
) -> dict[str, float]:
    requested = list(dict.fromkeys(str(value) for value in requested_instruments))
    if not requested:
        raise QfError(
            "PORTFOLIO_ALLOCATION_EVIDENCE_MISSING",
            "Portfolio simulation has no governed instrument scope.",
            422,
        )
    if len(requested) == 1:
        return {requested[0]: 1.0}

    requested_set = set(requested)
    last_prices: dict[str, float] = {}
    fill_notionals = {instrument_id: 0.0 for instrument_id in requested}
    for fill in evidence.get("fills", []):
        if not isinstance(fill, dict):
            continue
        instrument_id = str(fill.get("instrument_id") or "")
        quantity = _evidence_number(fill.get("quantity"))
        price = _evidence_number(fill.get("price"))
        if instrument_id not in requested_set or quantity is None or price is None:
            continue
        last_prices[instrument_id] = price
        side_sign = _exposure_side_sign(fill.get("side"))
        signed_quantity = abs(quantity) * side_sign if side_sign is not None else quantity
        fill_notionals[instrument_id] += signed_quantity * price

    position_notionals = {instrument_id: 0.0 for instrument_id in requested}
    for position in evidence.get("positions", []):
        if not isinstance(position, dict):
            continue
        instrument_id = str(position.get("instrument_id") or "")
        quantity = _evidence_number(position.get("quantity"))
        price = last_prices.get(instrument_id)
        if instrument_id not in requested_set or quantity is None or price is None:
            continue
        if position.get("closed_at") not in (None, ""):
            continue
        side_sign = _exposure_side_sign(position.get("side"))
        signed_quantity = abs(quantity) * side_sign if side_sign is not None else quantity
        position_notionals[instrument_id] += signed_quantity * price

    position_gross = sum(abs(value) for value in position_notionals.values())
    notionals = position_notionals if position_gross > 0 else fill_notionals
    nonzero = {key: value for key, value in notionals.items() if abs(value) > 0}
    gross = sum(abs(value) for value in nonzero.values())
    if gross <= 0:
        raise QfError(
            "PORTFOLIO_ALLOCATION_EVIDENCE_MISSING",
            "Multi-instrument Candidate requires executed signed allocation evidence from Nautilus.",
            422,
        )
    return {key: value / gross for key, value in nonzero.items()}


def _approval_simulation_summaries(
    *,
    constraints: dict[str, Any],
    evidence: dict[str, Any],
    request: BacktestExperimentRequest,
    current_candidate: PortfolioCandidate | None,
    proposed_quality: float,
) -> dict[str, dict[str, Any]]:
    venue = dict(request.venue_config or {})
    capital_context = {
        "source": "PINNED_NAUTILUS_VENUE_CONFIG",
        "base_currency": venue.get("base_currency", "USD"),
        "starting_balances": venue.get("starting_balances", ["1_000_000 USD"]),
        "account_type": venue.get("account_type", "MARGIN"),
    }
    observed_risk: dict[str, float] = {}
    for name, aliases in {
        "max_drawdown": {"max_drawdown", "maxdrawdown"},
        "turnover": {"turnover", "portfolio_turnover"},
        "leverage": {"leverage", "gross_leverage"},
        "margin_usage": {"margin_usage", "margin_utilization"},
    }.items():
        value = _metric_from_evidence(evidence, aliases)
        if value is not None and math.isfinite(value):
            observed_risk[name] = value
    risk_limits = {
        key: constraints[key]
        for key in ("max_drawdown", "max_turnover", "max_leverage", "max_margin_usage")
        if key in constraints
    }
    commissions = [
        fill.get("commission")
        for fill in evidence.get("fills", [])
        if isinstance(fill, dict) and fill.get("commission") is not None
    ]
    previous_quality = None
    if current_candidate is not None:
        previous_quality = _evidence_number(
            (current_candidate.metrics or {}).get("search_adjusted_quality")
        )
    return {
        "capital_context": capital_context,
        "risk_summary": {
            "status": "MANDATE_CHECK_PASSED",
            "mandate_limits": risk_limits,
            "observed": observed_risk,
        },
        "cost_summary": {
            "status": "COMMISSION_EVIDENCE_ONLY",
            "fill_commissions": commissions,
            "bps_cost_constraint_supported": False,
        },
        "capacity_summary": {
            "status": "NOT_GOVERNED_IN_V1",
            "capacity_constraint_supported": False,
        },
        "changes_summary": {
            "material_improvement_gate": "PASSED",
            "previous_candidate_id": (
                str(current_candidate.id) if current_candidate is not None else None
            ),
            "previous_search_adjusted_quality": previous_quality,
            "proposed_search_adjusted_quality": proposed_quality,
        },
    }


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
    simulation_experiment_id: UUID | None = None,
) -> CandidatePromotion:
    """Optimize Alpha selection, run a real PORTFOLIO simulation, and freeze approval facts."""
    requested_alpha_ids = _canonical_alpha_ids(alpha_ids)
    if not requested_alpha_ids:
        raise QfError("ALPHA_SELECTION_EMPTY", "At least one qualified Alpha is required.", 422)
    with factory() as session:
        portfolio_program = session.get(PortfolioProgram, portfolio_program_id)
        if portfolio_program is None:
            raise QfError("PORTFOLIO_PROGRAM_NOT_FOUND", "Portfolio Program does not exist.", 404)
        experiment_id = simulation_experiment_id or uuid4()
        if simulation_experiment_id is not None:
            existing_candidate = session.scalar(
                select(PortfolioCandidate).where(
                    PortfolioCandidate.simulation_experiment_id == simulation_experiment_id
                )
            )
            if existing_candidate is not None:
                optimizer = (existing_candidate.metrics or {}).get("optimizer", {})
                stored_ids: list[UUID] = []
                for raw_id in optimizer.get("considered_alpha_ids", []):
                    try:
                        stored_ids.append(UUID(str(raw_id)))
                    except (TypeError, ValueError) as exc:
                        raise QfError(
                            "CANDIDATE_ALPHA_LINEAGE_MISSING",
                            "Idempotent Candidate lost its considered Alpha lineage.",
                            500,
                        ) from exc
                if (
                    existing_candidate.portfolio_program_id != portfolio_program_id
                    or _canonical_alpha_ids(stored_ids) != requested_alpha_ids
                ):
                    raise QfError(
                        "CANDIDATE_SIMULATION_IDENTITY_MISMATCH",
                        "Simulation id is bound to a different Portfolio/Alpha contract.",
                        409,
                    )
                existing_approval = session.scalar(
                    select(ApprovalSnapshot)
                    .where(ApprovalSnapshot.candidate_id == existing_candidate.id)
                    .order_by(ApprovalSnapshot.created_at.desc())
                    .limit(1)
                )
                if existing_approval is None:
                    raise QfError(
                        "CANDIDATE_APPROVAL_MISSING",
                        "Idempotent Candidate exists without its Approval Snapshot.",
                        500,
                    )
                selected = existing_candidate.members[0] if existing_candidate.members else {}
                try:
                    selected_alpha_id = UUID(str(selected.get("alpha_qualification_id")))
                except (TypeError, ValueError) as exc:
                    raise QfError(
                        "CANDIDATE_ALPHA_LINEAGE_MISSING",
                        "Idempotent Candidate lost its selected Alpha lineage.",
                        500,
                    ) from exc
                return CandidatePromotion(
                    candidate_id=existing_candidate.id,
                    approval_id=existing_approval.id,
                    simulation_experiment_id=simulation_experiment_id,
                    selected_alpha_id=selected_alpha_id,
                )
        mandate = _bound_mandate(session, portfolio_program)
        alpha, source, request = _load_portfolio_source(
            session, requested_alpha_ids, mandate
        )
        mandate_constraints = _validate_mandate_before_simulation(mandate, alpha)
        mandate_id = mandate.id
        simulation_request = request.model_copy(
            update={
                "experiment_id": experiment_id,
                "mode": ExperimentMode.PORTFOLIO,
                "tags": {
                    **request.tags,
                    "portfolio_program_id": str(portfolio_program_id),
                    "alpha_qualification_id": str(alpha.id),
                    "optimizer": "MAX_SEARCH_ADJUSTED_QUALITY_V1",
                    "allocation_policy": "DERIVE_FROM_EXECUTED_NOTIONAL_V1",
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
    _validate_mandate_after_simulation(mandate_constraints, simulation_evidence)

    with factory() as session, session.begin():
        portfolio_program = session.execute(
            select(PortfolioProgram)
            .where(PortfolioProgram.id == portfolio_program_id)
            .with_for_update()
        ).scalar_one_or_none()
        if portfolio_program is None:
            raise QfError("PORTFOLIO_PROGRAM_NOT_FOUND", "Portfolio Program disappeared.", 500)
        persisted_mandate = session.get(PortfolioMandate, mandate_id)
        if (
            persisted_mandate is None
            or not persisted_mandate.enabled
            or persisted_mandate.state != "ACTIVE"
            or persisted_mandate.latest_version_id != portfolio_program.mandate_version_id
        ):
            raise QfError(
                "PORTFOLIO_MANDATE_CHANGED",
                "The bound Portfolio Mandate changed during transaction-level simulation.",
                409,
            )
        persisted_alpha = session.get(AlphaQualification, alpha_id)
        if (
            persisted_alpha is None
            or persisted_alpha.state != "ACTIVE"
            or persisted_alpha.degradation_state != "HEALTHY"
            or _has_degradation_observation(session, alpha_id)
        ):
            raise QfError("ALPHA_NOT_PORTFOLIO_READY", "Selected Alpha changed before promotion.", 409)
        current_constraints = _validate_mandate_before_simulation(
            persisted_mandate, persisted_alpha
        )
        if current_constraints != mandate_constraints:
            raise QfError(
                "PORTFOLIO_MANDATE_CHANGED",
                "The bound Portfolio Mandate constraints changed during simulation.",
                409,
            )
        _validate_mandate_after_simulation(current_constraints, simulation_evidence)
        current_candidate = (
            session.get(PortfolioCandidate, portfolio_program.current_candidate_id)
            if portfolio_program.current_candidate_id is not None
            else None
        )
        _require_material_improvement(
            current_candidate,
            proposed_quality=alpha_quality,
            mandate=persisted_mandate,
        )
        request_json = BacktestExperimentRequest.model_validate(simulation.request_json)
        strategy = StrategyArtifact.model_validate(request_json.strategy)
        executed_instrument_weights = _executed_instrument_weights(
            simulation_evidence, request_json.instrument_ids
        )
        instrument_ids = list(executed_instrument_weights)
        downstreams = list(
            session.scalars(
                select(DownstreamSystem)
                .where(
                    DownstreamSystem.environment_type == "PAPER",
                    DownstreamSystem.enabled.is_(True),
                    DownstreamSystem.preflight_state == "READY",
                    DownstreamSystem.package_contract_version == "2",
                )
                .order_by(DownstreamSystem.name, DownstreamSystem.id)
            )
        )
        paper_downstream = next(
            (
                downstream
                for downstream in downstreams
                if not downstream.compatibility
                or persisted_alpha.universe in downstream.compatibility
            ),
            None,
        )
        if paper_downstream is None:
            raise QfError(
                "PAPER_DOWNSTREAM_NOT_READY",
                "No ready Candidate Bundle v2 Paper downstream matches the selected Alpha.",
                409,
            )
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
                    "target_weight": executed_instrument_weights[instrument_id],
                    "confidence": alpha_quality,
                    "universe": alpha_universe,
                    "universe_version_id": (
                        str(persisted_alpha.universe_version_id)
                        if persisted_alpha.universe_version_id is not None
                        else None
                    ),
                    "instrument_id": instrument_id,
                }
                for instrument_id in instrument_ids
            ],
            metrics={
                "search_adjusted_quality": alpha_quality,
                "mandate": {
                    "mandate_id": str(persisted_mandate.id),
                    "mandate_version_id": str(portfolio_program.mandate_version_id),
                    "constraints": current_constraints,
                },
                "optimizer": {
                    "name": "MAX_SEARCH_ADJUSTED_QUALITY_V1",
                    "considered_alpha_ids": [str(item) for item in requested_alpha_ids],
                    "selected_alpha_id": str(persisted_alpha.id),
                    "target_weight": 1.0,
                    "instrument_weights": executed_instrument_weights,
                    "allocation_policy": "EXECUTED_NOTIONAL_V1",
                },
                "nautilus": {
                    "strategy_artifact": strategy.model_dump(mode="json"),
                    "evidence": simulation_evidence,
                    "dataset_revision_ids": [str(simulation.dataset_revision_id)],
                    "alpha_qualification_ids": [str(persisted_alpha.id)],
                    "universe_version_id": (
                        str(persisted_alpha.universe_version_id)
                        if persisted_alpha.universe_version_id is not None
                        else None
                    ),
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
        portfolio_program.state = "CANDIDATE_READY"
        approval_summaries = _approval_simulation_summaries(
            constraints=current_constraints,
            evidence=simulation_evidence,
            request=request_json,
            current_candidate=current_candidate,
            proposed_quality=alpha_quality,
        )
        approval = ApprovalSnapshot(
            candidate_id=candidate.id,
            purpose="PAPER",
            state="PENDING",
            downstream_system_id=paper_downstream.id,
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
                "order_count": len(simulation_evidence.get("orders", [])),
                "fill_count": len(simulation_evidence.get("fills", [])),
                "position_count": len(simulation_evidence.get("positions", [])),
                "statistics": simulation_evidence.get("statistics", {}),
            },
            capital_context=approval_summaries["capital_context"],
            risk_summary=approval_summaries["risk_summary"],
            cost_summary=approval_summaries["cost_summary"],
            capacity_summary=approval_summaries["capacity_summary"],
            changes_summary=approval_summaries["changes_summary"],
        )
        session.add(approval)
        session.flush()
        return CandidatePromotion(
            candidate_id=candidate.id,
            approval_id=approval.id,
            simulation_experiment_id=simulation.id,
            selected_alpha_id=persisted_alpha.id,
        )
