"""Independent sealed evaluator using the same pinned remote Nautilus runtime."""

from __future__ import annotations

import argparse
import math
from datetime import UTC, datetime, timedelta
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import select

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    CapitalContextVersion,
    DatasetRevision,
    EvaluationEpisode,
    Event,
    Job,
    MarketUniverseVersion,
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
from quant_runtime.evidence import extract_statistics, persistable_evidence, sealed_error_fields
from quant_runtime.remote import NautilusQuantRuntime
from runtime_config import load_effective_settings
from settings import Settings


def _now() -> datetime:
    return datetime.now(UTC)


def _as_utc(value: datetime) -> datetime:
    if value.tzinfo is None:
        return value.replace(tzinfo=UTC)
    return value.astimezone(UTC)


SEALED_PROMOTION_POLICY: dict[str, float | int] = {
    "minimum_orders": 1,
    "minimum_sharpe": 0.0,
    "maximum_drawdown": 1.0,
}


def _statistics(evidence: RunEvidence) -> dict[str, float | int]:
    metrics = extract_statistics(evidence)
    if metrics is None:
        raise QfError(
            "INVALID_RUNTIME_STATISTICS",
            "Sealed evidence is missing complete finite statistics.",
            422,
        )
    sharpe, max_drawdown, turnover, total_orders, total_positions = metrics
    return {
        "sharpe_ratio": sharpe,
        "max_drawdown": max_drawdown,
        "turnover": turnover,
        "total_orders": total_orders,
        "total_positions": total_positions,
    }


def _classification(statistics: dict[str, float | int]) -> tuple[bool, str]:
    minimum_orders = int(SEALED_PROMOTION_POLICY["minimum_orders"])
    minimum_sharpe = float(SEALED_PROMOTION_POLICY["minimum_sharpe"])
    maximum_drawdown = float(SEALED_PROMOTION_POLICY["maximum_drawdown"])
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
            if run.mode == "SEALED":
                run.error_code, run.error_message = sealed_error_fields(evidence)
                persisted = persistable_evidence(evidence)
                persisted.pop("error_code", None)
                persisted.pop("error_message", None)
                run.evidence = persisted
            else:
                run.error_code = evidence.error_code
                run.error_message = evidence.error_message
                run.evidence = persistable_evidence(evidence)
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
            if run.mode == "SEALED":
                run.error_code = "SEALED_RUNTIME_FAILURE"
                run.error_message = "Sealed runtime failure; disclosure withheld."
            else:
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
            if episode.sealed_dataset_revision_id is None:
                raise QfError(
                    "SEALED_DATASET_NOT_FROZEN",
                    "The Evaluation Episode has no frozen sealed Dataset Revision.",
                    422,
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
                    NautilusCatalogBinding.dataset_revision_id
                    == episode.sealed_dataset_revision_id,
                    NautilusCatalogBinding.sealed.is_(True),
                    NautilusCatalogBinding.quality_state == "VALID",
                    NautilusCatalogBinding.point_in_time_state == "VALID",
                )
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
    )


def _alpha_output_contract(parameters: dict[str, Any]) -> dict[str, Any] | None:
    raw = parameters.get("alpha_output_contract")
    if not isinstance(raw, dict) or raw.get("kind") != "score":
        return None
    fields = raw.get("fields")
    if not isinstance(fields, list) or not {"score", "expected_return", "uncertainty"}.issubset(fields):
        return None
    return {"kind": "score", "fields": [str(item) for item in fields]}


def _mandate_is_eligible(
    mandate: PortfolioMandate,
    *,
    universe_version_id: UUID,
    policy_family: str,
) -> bool:
    spec = mandate.spec_json if isinstance(mandate.spec_json, dict) else {}
    allowed_universes = spec.get("allowed_universe_versions", spec.get("allowed_universe_version_ids", []))
    if allowed_universes and str(universe_version_id) not in {str(item) for item in allowed_universes}:
        return False
    allowed_roles = spec.get("allowed_alpha_roles", [])
    if allowed_roles and "PRIMARY_ALPHA" not in {str(item) for item in allowed_roles}:
        return False
    allowed_policies = spec.get("allowed_policy_families", [])
    if allowed_policies and policy_family not in {str(item) for item in allowed_policies}:
        return False
    concentration = spec.get("concentration_constraints", {})
    if not isinstance(concentration, dict):
        concentration = {}
    maximum_weight = spec.get("max_weight", concentration.get("max_single_weight", 1.0))
    try:
        return math.isfinite(float(maximum_weight)) and float(maximum_weight) >= 1.0
    except (TypeError, ValueError):
        return False


def _load_promotion_plan(settings: Settings, discovery: QuantRuntimeRun) -> dict[str, Any] | None:
    """Resolve all immutable promotion inputs before the Portfolio run is submitted."""
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory() as session:
            binding = session.scalar(
                select(NautilusCatalogBinding).where(
                    NautilusCatalogBinding.catalog_uri == discovery.catalog_uri,
                    NautilusCatalogBinding.sealed.is_(False),
                )
            )
            revision = session.get(DatasetRevision, binding.dataset_revision_id) if binding else None
            universe = (
                session.get(MarketUniverseVersion, revision.universe_version_id)
                if revision and revision.universe_version_id
                else None
            )
            alpha_contract = _alpha_output_contract(discovery.parameters)
            if binding is None or revision is None or universe is None or alpha_contract is None:
                return None
            mandate = next(
                (
                    item
                    for item in session.scalars(
                        select(PortfolioMandate)
                        .where(PortfolioMandate.enabled.is_(True), PortfolioMandate.state == "ACTIVE")
                        .order_by(PortfolioMandate.created_at.desc())
                    ).all()
                    if _mandate_is_eligible(
                        item,
                        universe_version_id=universe.id,
                        policy_family=discovery.family,
                    )
                ),
                None,
            )
            capital = session.scalar(
                select(CapitalContextVersion)
                .where(
                    CapitalContextVersion.observed_at <= _now(),
                    CapitalContextVersion.valid_until > _now(),
                    CapitalContextVersion.deployable_capital > 0,
                )
                .order_by(CapitalContextVersion.observed_at.desc())
            )
            if mandate is None or capital is None:
                return None
            strategy_config = discovery.strategy_artifact.get("config", {})
            instrument_id = str(strategy_config.get("instrument_id", ""))
            if instrument_id not in set(binding.instrument_scope):
                return None
            candidate_id = uuid4()
            target_frame = {
                "schema_version": "1",
                "portfolio_candidate_id": str(candidate_id),
                "portfolio_state": "READY",
                "universe_version_id": str(universe.id),
                "rows": [
                    {
                        "instrument_id": instrument_id,
                        "target_weight": 1.0,
                        "confidence": 1.0,
                    }
                ],
            }
            return {
                "candidate_id": candidate_id,
                "universe_version_id": universe.id,
                "universe_name": universe.name,
                "instrument_id": instrument_id,
                "mandate_id": mandate.id,
                "mandate_version_id": mandate.latest_version_id,
                "mandate_name": mandate.name,
                "capital_context_id": capital.id,
                "target_frame": target_frame,
                "alpha_output_contract": alpha_contract,
            }
    finally:
        engine.dispose()


def _portfolio_experiment(
    discovery: QuantRuntimeRun,
    catalog_uri: str,
    plan: dict[str, Any],
) -> ExperimentSpec:
    parameters = {
        **discovery.parameters,
        "portfolio_target_frame": plan["target_frame"],
        "portfolio_mandate_version_id": str(plan["mandate_version_id"]),
        "capital_context_version_id": str(plan["capital_context_id"]),
    }
    return ExperimentSpec(
        experiment_key=discovery.experiment_key,
        family=discovery.family,
        catalog_uri=catalog_uri,
        strategy=StrategyArtifact.model_validate(discovery.strategy_artifact),
        parameters=parameters,
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
            _apply_sealed_decision(
                session,
                episode=episode,
                run=run,
                passed=passed,
                classification=classification,
            )
    finally:
        engine.dispose()


def _apply_sealed_decision(
    session: Any,
    *,
    episode: EvaluationEpisode,
    run: QuantRuntimeRun,
    passed: bool,
    classification: str,
) -> None:
    episode.sealed_run_id = run.id
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


def _capacity_limit(evidence: RunEvidence) -> float | None:
    raw = evidence.statistics.get("capacity_envelope")
    if not isinstance(raw, dict):
        return None
    value = raw.get("max_deployable_capital")
    if isinstance(value, bool) or not isinstance(value, (int, float, str)):
        return None
    try:
        result = float(value)
    except (TypeError, ValueError):
        return None
    return result if math.isfinite(result) and result > 0 else None


def _materially_improves(
    candidate: PortfolioCandidate,
    statistics: dict[str, float | int],
) -> bool:
    raw = candidate.metrics.get("sealed_statistics")
    if not isinstance(raw, dict):
        return True
    try:
        old_sharpe = float(raw["sharpe_ratio"])
        old_drawdown = float(raw["max_drawdown"])
        new_sharpe = float(statistics["sharpe_ratio"])
        new_drawdown = float(statistics["max_drawdown"])
    except (KeyError, TypeError, ValueError):
        return True
    return new_sharpe >= old_sharpe + 0.05 or new_drawdown <= old_drawdown - 0.01


def _promote_alpha_and_candidate(
    settings: Settings,
    *,
    episode_id: UUID,
    discovery_run_id: UUID,
    sealed_run_id: UUID,
    portfolio_run_id: UUID,
    sealed_statistics: dict[str, float | int],
    portfolio_evidence: RunEvidence,
    plan: dict[str, Any],
) -> None:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            episode = session.execute(
                select(EvaluationEpisode)
                .where(EvaluationEpisode.id == episode_id)
                .with_for_update()
            ).scalar_one_or_none()
            discovery = session.get(QuantRuntimeRun, discovery_run_id)
            sealed_run = session.get(QuantRuntimeRun, sealed_run_id)
            portfolio_run = session.get(QuantRuntimeRun, portfolio_run_id)
            capital = session.get(CapitalContextVersion, plan["capital_context_id"])
            mandate = session.get(PortfolioMandate, plan["mandate_id"])
            if discovery is None:
                raise QfError("PROMOTION_CONTEXT_MISSING", "Discovery context is incomplete.", 500)
            binding = session.scalar(
                select(NautilusCatalogBinding).where(
                    NautilusCatalogBinding.catalog_uri == discovery.catalog_uri,
                    NautilusCatalogBinding.sealed.is_(False),
                )
            )
            revision = session.get(DatasetRevision, binding.dataset_revision_id) if binding else None
            universe = (
                session.get(MarketUniverseVersion, revision.universe_version_id)
                if revision and revision.universe_version_id
                else None
            )
            if (
                episode is None
                or discovery is None
                or sealed_run is None
                or portfolio_run is None
                or capital is None
                or mandate is None
                or revision is None
                or universe is None
                or episode.state != "SEALED_PENDING"
            ):
                raise QfError("PROMOTION_CONTEXT_MISSING", "Promotion context is incomplete.", 500)
            if not _mandate_is_eligible(
                mandate,
                universe_version_id=universe.id,
                policy_family=discovery.family,
            ):
                raise QfError("MANDATE_INELIGIBLE", "No applicable mandate remains eligible for this Alpha.", 422)
            capital_amount = float(capital.deployable_capital)
            if (
                not math.isfinite(capital_amount)
                or _as_utc(capital.valid_until) <= _now()
                or capital_amount <= 0
            ):
                raise QfError("CAPITAL_CONTEXT_INVALID", "The selected Capital Context is no longer valid.", 422)
            capacity_limit = _capacity_limit(portfolio_evidence)
            if capacity_limit is None or capital_amount > capacity_limit:
                raise QfError(
                    "CAPACITY_ENVELOPE_INSUFFICIENT",
                    "The Candidate capacity envelope does not cover the frozen Capital Context.",
                    422,
                )

            family_candidates: list[PortfolioCandidate] = []
            for candidate in session.scalars(select(PortfolioCandidate)).all():
                raw = candidate.metrics.get("nautilus")
                if (
                    isinstance(raw, dict)
                    and raw.get("candidate_family") == discovery.family
                    and candidate.mandate_version_id == mandate.latest_version_id
                    and candidate.universe_set_json == [str(universe.id)]
                ):
                    family_candidates.append(candidate)
            for approval in session.scalars(
                select(ApprovalSnapshot).where(
                    ApprovalSnapshot.state == "PENDING",
                    ApprovalSnapshot.purpose == "PAPER",
                )
            ).all():
                pending_candidate = session.get(PortfolioCandidate, approval.candidate_id)
                raw = pending_candidate.metrics.get("nautilus") if pending_candidate else None
                if isinstance(raw, dict) and raw.get("candidate_family") == discovery.family:
                    _apply_sealed_decision(
                        session,
                        episode=episode,
                        run=sealed_run,
                        passed=True,
                        classification="PENDING_APPROVAL_ALREADY_EXISTS",
                    )
                    return
            if family_candidates and not _materially_improves(family_candidates[-1], sealed_statistics):
                _apply_sealed_decision(
                    session,
                    episode=episode,
                    run=sealed_run,
                    passed=True,
                    classification="NO_MATERIAL_IMPROVEMENT",
                )
                return

            strategy_config = discovery.strategy_artifact.get("config", {})
            instrument_id = str(strategy_config.get("instrument_id", ""))
            bar_type = str(strategy_config.get("bar_type", "UNKNOWN"))
            alpha = AlphaQualification(
                id=uuid4(),
                program_id=episode.program_id,
                alpha_model_version_id=uuid4(),
                calibration_version_id=None,
                universe_version_id=universe.id,
                universe=universe.name,
                horizon=bar_type,
                role="PRIMARY_ALPHA",
                state="ACTIVE",
                name=discovery.experiment_key,
                scope_json={
                    "universe_version_id": str(universe.id),
                    "instrument_scope": [instrument_id],
                    "alpha_output_contract": plan["alpha_output_contract"],
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
                    {"kind": "UNIVERSE_VERSION", "id": str(universe.id)},
                ],
                created_at=_now(),
            )
            session.add(alpha)
            session.flush()

            portfolio_program = PortfolioProgram(
                mandate_version_id=mandate.latest_version_id,
                mandate_name=mandate.name,
                state="CANDIDATE_READY",
            )
            session.add(portfolio_program)
            session.flush()
            family_id = family_candidates[-1].candidate_family_id if family_candidates else uuid4()
            candidate = PortfolioCandidate(
                id=plan["candidate_id"],
                candidate_family_id=family_id,
                portfolio_program_id=portfolio_program.id,
                mandate_version_id=mandate.latest_version_id,
                mandate_name=mandate.name,
                capital_context_version_id=capital.id,
                universe_set_json=[str(universe.id)],
                policy_version="NAUTILUS_FIRST_SINGLE_ALPHA_V1",
                risk_model_version="NAUTILUS_RISK_ENGINE_1.231.0",
                cost_model_version="REMOTE_RUNTIME_VENUE_CONFIG",
                capacity_model_version="REMOTE_RUNTIME_EVIDENCE",
                constraint_set_version="MANDATE_CURRENT",
                rebalance_policy_version="TARGET_PORTFOLIO_FRAME_V1",
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
                        "candidate_family": discovery.family,
                        "runtime_version": sealed_run.runtime_version,
                        "contract_version": sealed_run.contract_version,
                        "strategy_artifact": discovery.strategy_artifact,
                        "target_portfolio_frame": plan["target_frame"],
                        "discovery_run_id": str(discovery.id),
                        "sealed_run_id": str(sealed_run.id),
                        "portfolio_run_id": str(portfolio_run.id),
                        "portfolio_evidence": persistable_evidence(portfolio_evidence),
                    },
                    "sealed_statistics": sealed_statistics,
                    "capacity_envelope": {"max_deployable_capital": capacity_limit},
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
                    "Discovery, independent sealed evaluation, and the frozen TargetPortfolioFrame "
                    "passed the server-owned promotion policy."
                ),
                human_report={
                    "runtime": "NautilusTrader",
                    "runtime_version": sealed_run.runtime_version,
                    "alpha_output_contract": plan["alpha_output_contract"],
                },
                evidence_summary={
                    "discovery_run_id": str(discovery.id),
                    "sealed_run_id": str(sealed_run.id),
                    "portfolio_run_id": str(portfolio_run.id),
                    "sealed_statistics": sealed_statistics,
                },
                capital_context={
                    "capital_context_version_id": str(capital.id),
                    "source_type": capital.source_type,
                    "base_currency": capital.base_currency,
                    "deployable_capital": float(capital.deployable_capital),
                    "observed_at": _as_utc(capital.observed_at).isoformat(),
                    "valid_until": _as_utc(capital.valid_until).isoformat(),
                },
                risk_summary={"source": "NautilusTrader Portfolio/RiskEngine"},
                cost_summary={"source": "NautilusTrader simulated venue"},
                capacity_summary={
                    "source": "remote runtime evidence",
                    "max_deployable_capital": capacity_limit,
                },
                changes_summary={"material_improvement_policy": "SHARPE_PLUS_0.05_OR_DRAWDOWN_MINUS_0.01"},
            )
            session.add(approval)
            _apply_sealed_decision(
                session,
                episode=episode,
                run=sealed_run,
                passed=True,
                classification="PROMOTION_PASSED",
            )
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
    sealed_config = RemoteNautilusConfig.from_env(required=True, profile="sealed")
    research_config = RemoteNautilusConfig.from_env(required=True, profile="research")
    assert sealed_config is not None
    assert research_config is not None
    sealed_runtime = NautilusQuantRuntime(sealed_config)
    research_runtime = NautilusQuantRuntime(research_config)
    sealed_experiment = _experiment(discovery, sealed_catalog.catalog_uri)
    sealed_run_id = _create_remote_run(
        settings,
        episode=episode,
        discovery=discovery,
        mode="SEALED",
        catalog_uri=sealed_catalog.catalog_uri,
    )
    try:
        sealed_evidence = sealed_runtime.run_sealed_backtest(sealed_experiment)
        _complete_remote_run(settings, run_id=sealed_run_id, evidence=sealed_evidence)
        if sealed_evidence.state != "SUCCEEDED":
            raise QfError(
                "SEALED_RUNTIME_RUN_FAILED",
                "The independent Sealed Nautilus runtime did not complete successfully.",
                503,
                {"error_code": sealed_evidence.error_code},
            )
    except Exception as exc:
        _fail_remote_run(settings, run_id=sealed_run_id, exc=exc)
        raise

    try:
        sealed_statistics = _statistics(sealed_evidence)
    except QfError:
        _record_sealed_decision(
            settings,
            episode_id=episode.id,
            sealed_run_id=sealed_run_id,
            passed=False,
            classification="INVALID_RUNTIME_STATISTICS",
        )
        return
    passed, classification = _classification(sealed_statistics)
    if not passed:
        _record_sealed_decision(
            settings,
            episode_id=episode.id,
            sealed_run_id=sealed_run_id,
            passed=False,
            classification=classification,
        )
        return

    plan = _load_promotion_plan(settings, discovery)
    if plan is None:
        _record_sealed_decision(
            settings,
            episode_id=episode.id,
            sealed_run_id=sealed_run_id,
            passed=True,
            classification="PROMOTION_CONTEXT_UNAVAILABLE",
        )
        return

    portfolio_run_id = _create_remote_run(
        settings,
        episode=episode,
        discovery=discovery,
        mode="PORTFOLIO",
        catalog_uri=discovery.catalog_uri,
    )
    try:
        portfolio_evidence = research_runtime.run_portfolio_backtest(
            _portfolio_experiment(discovery, discovery.catalog_uri, plan)
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
    if extract_statistics(portfolio_evidence) is None:
        raise QfError(
            "INVALID_RUNTIME_STATISTICS",
            "Portfolio evidence is missing complete finite statistics.",
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
        plan=plan,
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
