"""Independent sealed evaluator using the same pinned remote Nautilus runtime."""

from __future__ import annotations

import argparse
from datetime import UTC, datetime, timedelta
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import select

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    EvaluationEpisode,
    Event,
    Job,
    NautilusCatalogBinding,
    PortfolioCandidate,
    PortfolioMandate,
    PortfolioProgram,
    QuantRuntimeRun,
    SearchLedgerEntry,
)
from db.session import create_database_engine, create_session_factory
from errors import QfError
from quant_runtime.config import RemoteNautilusConfig
from quant_runtime.contracts import ExperimentSpec, RunEvidence, StrategyArtifact
from quant_runtime.remote import NautilusQuantRuntime
from runtime_config import load_effective_settings
from settings import Settings


def _now() -> datetime:
    return datetime.now(UTC)


def _as_float(value: object, default: float = 0.0) -> float:
    if isinstance(value, bool):
        return default
    try:
        return float(value)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return default


def _statistics(evidence: RunEvidence) -> dict[str, float | int]:
    stats = evidence.statistics
    returns = stats.get("returns") if isinstance(stats.get("returns"), dict) else {}
    general = stats.get("general") if isinstance(stats.get("general"), dict) else {}
    return {
        "sharpe_ratio": _as_float(
            stats.get(
                "sharpe_ratio",
                returns.get("Sharpe Ratio", returns.get("SharpeRatio", 0.0)),
            )
        ),
        "max_drawdown": abs(
            _as_float(
                stats.get(
                    "max_drawdown",
                    returns.get("Max Drawdown", returns.get("MaxDrawdown", 0.0)),
                )
            )
        ),
        "turnover": _as_float(
            stats.get("turnover", general.get("Turnover", len(evidence.fills)))
        ),
        "total_orders": int(
            _as_float(
                stats.get("total_orders", general.get("Total Orders", len(evidence.orders)))
            )
        ),
        "total_positions": int(
            _as_float(
                stats.get(
                    "total_positions",
                    general.get("Total Positions", len(evidence.positions)),
                )
            )
        ),
    }


def _classification(
    statistics: dict[str, float | int],
    gate: dict[str, Any],
) -> tuple[bool, str]:
    minimum_orders = int(_as_float(gate.get("minimum_orders"), 1.0))
    minimum_sharpe = _as_float(gate.get("minimum_sharpe"), 0.0)
    maximum_drawdown = _as_float(gate.get("maximum_drawdown"), 1.0)
    if int(statistics["total_orders"]) < minimum_orders:
        return False, "INSUFFICIENT_TRADING_EVIDENCE"
    if float(statistics["sharpe_ratio"]) < minimum_sharpe:
        return False, "INSUFFICIENT_NET_EDGE"
    if float(statistics["max_drawdown"]) > maximum_drawdown:
        return False, "DRAWDOWN_FAILURE"
    return True, "PROMOTION_PASSED"


def _create_remote_run(
    settings: Settings,
    *,
    episode: EvaluationEpisode,
    discovery: QuantRuntimeRun,
    mode: str,
    catalog_uri: str,
) -> UUID:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            run = QuantRuntimeRun(
                program_id=episode.program_id,
                branch_id=episode.branch_id,
                mission_id=discovery.mission_id,
                evaluation_episode_id=episode.id,
                parent_run_id=discovery.id,
                mode=mode,
                state="RUNNING",
                experiment_key=discovery.experiment_key,
                family=discovery.family,
                catalog_uri=catalog_uri,
                runtime_name="NautilusTrader",
                strategy_artifact=discovery.strategy_artifact,
                parameters=discovery.parameters,
                promotion_gate=discovery.promotion_gate,
                started_at=_now(),
            )
            session.add(run)
            session.flush()
            return run.id
    finally:
        engine.dispose()


def _complete_remote_run(
    settings: Settings,
    *,
    run_id: UUID,
    evidence: RunEvidence,
) -> None:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            run = session.get(QuantRuntimeRun, run_id)
            if run is None:
                raise QfError("QUANT_RUN_NOT_FOUND", "Quant runtime run disappeared.", 500)
            run.state = evidence.state
            run.external_run_id = evidence.external_run_id
            run.runtime_name = evidence.runtime_name
            run.runtime_version = evidence.nautilus_version
            run.contract_version = evidence.contract_version
            run.evidence = evidence.model_dump(mode="json")
            run.error_code = evidence.error_code
            run.error_message = evidence.error_message
            run.finished_at = _now()
    finally:
        engine.dispose()


def _fail_remote_run(settings: Settings, *, run_id: UUID, exc: Exception) -> None:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            run = session.get(QuantRuntimeRun, run_id)
            if run is None:
                return
            run.state = "FAILED"
            run.error_code = str(getattr(exc, "code", type(exc).__name__))[:100]
            run.error_message = str(exc)[-4000:]
            run.finished_at = _now()
    finally:
        engine.dispose()


def _load_episode(
    settings: Settings,
    job_id: UUID,
) -> tuple[EvaluationEpisode, QuantRuntimeRun, NautilusCatalogBinding]:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory() as session:
            job = session.get(Job, job_id)
            if job is None or job.kind != "SEALED_EVALUATION":
                raise QfError("JOB_NOT_FOUND", "Sealed Evaluation job does not exist.", 404)
            episode = session.get(EvaluationEpisode, job.resource_id)
            if episode is None:
                raise QfError(
                    "EVALUATION_EPISODE_NOT_FOUND",
                    "Sealed Evaluation episode does not exist.",
                    404,
                )
            if episode.state != "SEALED_PENDING":
                raise QfError(
                    "EVALUATION_STATE_CONFLICT",
                    "Only SEALED_PENDING episodes may be evaluated.",
                    409,
                    {"state": episode.state},
                )
            discovery = session.get(QuantRuntimeRun, episode.discovery_run_id)
            if discovery is None or discovery.state != "SUCCEEDED":
                raise QfError(
                    "DISCOVERY_EVIDENCE_INVALID",
                    "The selected discovery run is not successful.",
                    422,
                )
            sealed = session.scalar(
                select(NautilusCatalogBinding)
                .where(
                    NautilusCatalogBinding.sealed.is_(True),
                    NautilusCatalogBinding.quality_state == "VALID",
                    NautilusCatalogBinding.point_in_time_state == "VALID",
                )
                .order_by(NautilusCatalogBinding.created_at.asc())
            )
            if sealed is None:
                raise QfError(
                    "SEALED_CATALOG_UNAVAILABLE",
                    "No valid sealed Nautilus catalog is available.",
                    503,
                )
            for item in (episode, discovery, sealed):
                session.expunge(item)
            return episode, discovery, sealed
    finally:
        engine.dispose()


def _experiment(discovery: QuantRuntimeRun, catalog_uri: str) -> ExperimentSpec:
    return ExperimentSpec(
        experiment_key=discovery.experiment_key,
        family=discovery.family,
        catalog_uri=catalog_uri,
        strategy=StrategyArtifact.model_validate(discovery.strategy_artifact),
        parameters=discovery.parameters,
        promotion_gate=discovery.promotion_gate,
    )


def _record_sealed_decision(
    settings: Settings,
    *,
    episode_id: UUID,
    sealed_run_id: UUID,
    passed: bool,
    classification: str,
) -> None:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            episode = session.get(EvaluationEpisode, episode_id)
            run = session.get(QuantRuntimeRun, sealed_run_id)
            if episode is None or run is None:
                raise QfError(
                    "EVALUATION_EPISODE_NOT_FOUND",
                    "Sealed Evaluation state disappeared.",
                    500,
                )
            episode.sealed_run_id = sealed_run_id
            episode.state = "PROMOTED" if passed else "CONSUMED"
            episode.failure_code = None if passed else classification
            episode.disclosure = {
                "level": 1,
                "classification": classification,
                "passed": passed,
            }
            session.add(
                SearchLedgerEntry(
                    program_id=run.program_id,
                    branch_id=run.branch_id,
                    mission_id=run.mission_id,
                    run_id=run.id,
                    family=run.family,
                    parameters=run.parameters,
                    outcome="PROMOTED" if passed else "REJECTED",
                    failure_code=None if passed else classification,
                    disclosure_level="SEALED_LEVEL_1",
                    evidence_summary={
                        "classification": classification,
                        "passed": passed,
                    },
                    created_at=_now(),
                )
            )
            session.add(
                Event(
                    kind="SEALED_EVALUATION_COMPLETED",
                    aggregate_type="RESEARCH_PROGRAM",
                    aggregate_id=episode.program_id,
                    actor_kind="SYSTEM",
                    actor_metadata={},
                    payload={
                        "evaluation_episode_id": str(episode.id),
                        "classification": classification,
                        "passed": passed,
                    },
                )
            )
    finally:
        engine.dispose()


def _promote_alpha_and_candidate(
    settings: Settings,
    *,
    episode_id: UUID,
    discovery_run_id: UUID,
    sealed_run_id: UUID,
    portfolio_run_id: UUID,
    sealed_statistics: dict[str, float | int],
    portfolio_evidence: RunEvidence,
) -> None:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            episode = session.get(EvaluationEpisode, episode_id)
            discovery = session.get(QuantRuntimeRun, discovery_run_id)
            sealed_run = session.get(QuantRuntimeRun, sealed_run_id)
            portfolio_run = session.get(QuantRuntimeRun, portfolio_run_id)
            if episode is None or discovery is None or sealed_run is None or portfolio_run is None:
                raise QfError("PROMOTION_CONTEXT_MISSING", "Promotion context is incomplete.", 500)

            strategy_config = discovery.strategy_artifact.get("config", {})
            instrument_id = str(strategy_config.get("instrument_id", "UNKNOWN"))
            bar_type = str(strategy_config.get("bar_type", "UNKNOWN"))
            alpha = AlphaQualification(
                id=uuid4(),
                program_id=episode.program_id,
                alpha_model_version_id=uuid4(),
                calibration_version_id=None,
                universe_version_id=None,
                universe=instrument_id,
                horizon=bar_type,
                role="PRIMARY_ALPHA",
                state="ACTIVE",
                name=discovery.experiment_key,
                scope_json={
                    "instrument_scope": [instrument_id],
                    "strategy_artifact": discovery.strategy_artifact,
                    "runtime": {
                        "name": sealed_run.runtime_name,
                        "version": sealed_run.runtime_version,
                        "contract_version": sealed_run.contract_version,
                    },
                },
                evaluation_episode_id=episode.id,
                degradation_state="HEALTHY",
                metrics={
                    "discovery_run_id": str(discovery.id),
                    "sealed_run_id": str(sealed_run.id),
                    "sealed_statistics": sealed_statistics,
                },
                lineage=[
                    {"kind": "DISCOVERY_RUN", "id": str(discovery.id)},
                    {"kind": "SEALED_RUN", "id": str(sealed_run.id)},
                ],
                created_at=_now(),
            )
            session.add(alpha)

            mandate = session.scalar(
                select(PortfolioMandate)
                .where(PortfolioMandate.enabled.is_(True), PortfolioMandate.state == "ACTIVE")
                .order_by(PortfolioMandate.created_at.asc())
            )
            if mandate is None:
                session.add(
                    Event(
                        kind="ALPHA_QUALIFIED",
                        aggregate_type="RESEARCH_PROGRAM",
                        aggregate_id=episode.program_id,
                        actor_kind="SYSTEM",
                        actor_metadata={},
                        payload={
                            "alpha_qualification_id": str(alpha.id),
                            "portfolio_state": "WAITING_FOR_MANDATE",
                        },
                    )
                )
                return

            portfolio_program = PortfolioProgram(
                mandate_version_id=mandate.latest_version_id,
                mandate_name=mandate.name,
                state="CANDIDATE_READY",
            )
            session.add(portfolio_program)
            session.flush()
            candidate = PortfolioCandidate(
                candidate_family_id=uuid4(),
                portfolio_program_id=portfolio_program.id,
                mandate_version_id=mandate.latest_version_id,
                mandate_name=mandate.name,
                capital_context_version_id=None,
                universe_set_json=[instrument_id],
                policy_version="NAUTILUS_FIRST_SINGLE_ALPHA_V1",
                risk_model_version="NAUTILUS_RISK_ENGINE_1.231.0",
                cost_model_version="REMOTE_RUNTIME_VENUE_CONFIG",
                capacity_model_version="REMOTE_RUNTIME_EVIDENCE",
                constraint_set_version="MANDATE_CURRENT",
                rebalance_policy_version="STRATEGY_NATIVE",
                evaluation_episode_id=episode.id,
                state="READY",
                members=[
                    {
                        "alpha_qualification_id": str(alpha.id),
                        "instrument_id": instrument_id,
                        "target_weight": 1.0,
                    }
                ],
                metrics={
                    "nautilus": {
                        "runtime_version": sealed_run.runtime_version,
                        "contract_version": sealed_run.contract_version,
                        "strategy_artifact": discovery.strategy_artifact,
                        "discovery_run_id": str(discovery.id),
                        "sealed_run_id": str(sealed_run.id),
                        "portfolio_run_id": str(portfolio_run.id),
                        "portfolio_evidence": portfolio_evidence.model_dump(mode="json"),
                    },
                    "sealed_statistics": sealed_statistics,
                },
                created_at=_now(),
            )
            session.add(candidate)
            session.flush()
            portfolio_program.current_candidate_id = candidate.id
            approval = ApprovalSnapshot(
                candidate_id=candidate.id,
                purpose="PAPER",
                state="PENDING",
                valid_until=_now() + timedelta(days=7),
                expires_at=_now() + timedelta(days=7),
                recommendation_rationale=(
                    "Discovery, independent sealed evaluation, and Nautilus transaction-level "
                    "portfolio simulation passed the promotion policy."
                ),
                human_report={
                    "runtime": "NautilusTrader",
                    "runtime_version": sealed_run.runtime_version,
                    "strategy_reusable_for_paper": True,
                },
                evidence_summary={
                    "discovery_run_id": str(discovery.id),
                    "sealed_run_id": str(sealed_run.id),
                    "portfolio_run_id": str(portfolio_run.id),
                    "sealed_statistics": sealed_statistics,
                },
                capital_context={},
                risk_summary={"source": "NautilusTrader Portfolio/RiskEngine"},
                cost_summary={"source": "NautilusTrader simulated venue"},
                capacity_summary={"source": "remote runtime evidence"},
                changes_summary={"first_nautilus_native_candidate": True},
            )
            session.add(approval)
            session.add(
                Event(
                    kind="PORTFOLIO_CANDIDATE_READY",
                    aggregate_type="RESEARCH_PROGRAM",
                    aggregate_id=episode.program_id,
                    actor_kind="SYSTEM",
                    actor_metadata={},
                    payload={
                        "alpha_qualification_id": str(alpha.id),
                        "portfolio_candidate_id": str(candidate.id),
                        "approval_id": str(approval.id),
                    },
                )
            )
    finally:
        engine.dispose()


def run_sealed_evaluation(settings: Settings, job_id: UUID) -> None:
    episode, discovery, sealed_catalog = _load_episode(settings, job_id)
    config = RemoteNautilusConfig.from_env(required=True)
    assert config is not None
    runtime = NautilusQuantRuntime(config)
    sealed_experiment = _experiment(discovery, sealed_catalog.catalog_uri)
    sealed_run_id = _create_remote_run(
        settings,
        episode=episode,
        discovery=discovery,
        mode="SEALED",
        catalog_uri=sealed_catalog.catalog_uri,
    )
    try:
        sealed_evidence = runtime.run_sealed_backtest(sealed_experiment)
        _complete_remote_run(settings, run_id=sealed_run_id, evidence=sealed_evidence)
    except Exception as exc:
        _fail_remote_run(settings, run_id=sealed_run_id, exc=exc)
        raise

    sealed_statistics = _statistics(sealed_evidence)
    passed, classification = _classification(sealed_statistics, discovery.promotion_gate)
    _record_sealed_decision(
        settings,
        episode_id=episode.id,
        sealed_run_id=sealed_run_id,
        passed=passed,
        classification=classification,
    )
    if not passed:
        return

    portfolio_run_id = _create_remote_run(
        settings,
        episode=episode,
        discovery=discovery,
        mode="PORTFOLIO",
        catalog_uri=discovery.catalog_uri,
    )
    try:
        portfolio_evidence = runtime.run_portfolio_backtest(
            _experiment(discovery, discovery.catalog_uri)
        )
        _complete_remote_run(settings, run_id=portfolio_run_id, evidence=portfolio_evidence)
    except Exception as exc:
        _fail_remote_run(settings, run_id=portfolio_run_id, exc=exc)
        raise
    if portfolio_evidence.state != "SUCCEEDED":
        raise QfError(
            "PORTFOLIO_SIMULATION_FAILED",
            "Nautilus transaction-level Portfolio simulation did not succeed.",
            422,
        )

    _promote_alpha_and_candidate(
        settings,
        episode_id=episode.id,
        discovery_run_id=discovery.id,
        sealed_run_id=sealed_run_id,
        portfolio_run_id=portfolio_run_id,
        sealed_statistics=sealed_statistics,
        portfolio_evidence=portfolio_evidence,
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one independent sealed evaluation")
    parser.add_argument("action", choices=["run"])
    parser.add_argument("job_id")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    settings = load_effective_settings(Settings.from_env())
    run_sealed_evaluation(settings, UUID(args.job_id))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
