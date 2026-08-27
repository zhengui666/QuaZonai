"""Execute persisted Discovery and Sealed experiments through remote Nautilus runtimes."""

from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Literal
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    DatasetRevision,
    DownstreamSystem,
    Job,
    NautilusCatalogBinding,
    PortfolioCandidate,
    PortfolioMandate,
    PortfolioProgram,
    QuantExperiment,
    ResearchMission,
    ResearchProgram,
    SearchLedgerEntry,
    SealedEvaluation,
)
from db.session import create_database_engine, create_session_factory
from errors import QfError
from events import append_event
from jobs import enqueue_job
from quant_runtime import (
    BacktestEvidence,
    CatalogReference,
    ExperimentContract,
    PINNED_NAUTILUS_VERSION,
    RemoteNautilusQuantRuntime,
)
from settings import Settings


def _now() -> datetime:
    return datetime.now(UTC)


def _ledger(
    session: Session,
    *,
    experiment: QuantExperiment,
    outcome: str,
    evidence_summary: dict[str, Any] | None = None,
) -> SearchLedgerEntry:
    entry = SearchLedgerEntry(
        program_id=experiment.program_id,
        mission_id=experiment.mission_id,
        experiment_id=experiment.id,
        attempt_kind=f"NAUTILUS_{experiment.zone}_BACKTEST",
        outcome=outcome,
        hypothesis_key=str(experiment.strategy_artifact.get("strategy_path") or "unknown"),
        parameters=dict(experiment.contract_json.get("parameters") or {}),
        evidence_summary=evidence_summary or {},
        created_at=_now(),
    )
    session.add(entry)
    return entry


def _enqueue(session: Session, *, kind: str, resource_id: UUID) -> Job:
    return enqueue_job(session, kind=kind, resource_id=resource_id)


def submit_experiment(
    session: Session,
    *,
    mission_id: UUID,
    contract: ExperimentContract,
) -> QuantExperiment:
    """Validate a Mission-owned experiment contract and append it to the durable queue."""
    mission = session.get(ResearchMission, mission_id)
    if mission is None:
        raise QfError("MISSION_NOT_FOUND", "Research Mission does not exist.", 404)
    if mission.state not in {"READY", "RUNNING", "SUCCEEDED"}:
        raise QfError(
            "MISSION_STATE_CONFLICT",
            "The Mission cannot submit a quant experiment in its current state.",
            409,
            {"state": mission.state},
        )
    if mission.type not in {
        "ALPHA_DISCOVERY",
        "ROBUSTNESS",
        "PROMOTION_REVIEW",
        "PORTFOLIO_ASSEMBLY",
    }:
        raise QfError(
            "MISSION_CAPABILITY_DENIED",
            "This Mission type is not allowed to submit a Nautilus experiment.",
            403,
            {"mission_type": mission.type},
        )
    dataset = session.get(DatasetRevision, contract.catalog.dataset_revision_id)
    binding = session.scalar(
        select(NautilusCatalogBinding).where(
            NautilusCatalogBinding.dataset_revision_id == contract.catalog.dataset_revision_id
        )
    )
    if dataset is None or binding is None:
        raise QfError(
            "NAUTILUS_CATALOG_NOT_GOVERNED",
            "The experiment must reference a governed Dataset Revision with a Nautilus catalog binding.",
            422,
        )
    if dataset.quality_state != "VALID" or dataset.point_in_time_state != "VALID":
        raise QfError(
            "DATASET_NOT_RESEARCH_READY",
            "The Dataset Revision has not passed quality and point-in-time validation.",
            422,
        )
    expected_instruments = set(binding.instrument_scope)
    requested_instruments = set(contract.catalog.instrument_ids)
    if not requested_instruments or not requested_instruments.issubset(expected_instruments):
        raise QfError(
            "NAUTILUS_CATALOG_SCOPE_VIOLATION",
            "The experiment instrument scope exceeds the governed catalog binding.",
            403,
            {
                "allowed": sorted(expected_instruments),
                "requested": sorted(requested_instruments),
            },
        )
    discovery_catalog = contract.catalog.model_copy(
        update={
            "catalog_uri": binding.catalog_uri,
            "nautilus_data_type": binding.nautilus_data_type,
            "partition": "DISCOVERY",
        }
    )
    normalized = contract.model_copy(update={"catalog": discovery_catalog})
    existing = session.scalar(
        select(QuantExperiment).where(
            QuantExperiment.mission_id == mission.id,
            QuantExperiment.zone == "DISCOVERY",
            QuantExperiment.contract_json == normalized.model_dump(mode="json"),
        )
    )
    if existing is not None:
        return existing
    experiment = QuantExperiment(
        mission_id=mission.id,
        program_id=mission.program_id,
        dataset_revision_id=dataset.id,
        zone="DISCOVERY",
        state="READY",
        runtime_name="NAUTILUS_TRADER",
        runtime_version=PINNED_NAUTILUS_VERSION,
        strategy_artifact=normalized.strategy.model_dump(mode="json"),
        contract_json=normalized.model_dump(mode="json"),
    )
    session.add(experiment)
    session.flush()
    _ledger(session, experiment=experiment, outcome="QUEUED")
    _enqueue(
        session,
        kind="NAUTILUS_DISCOVERY_BACKTEST",
        resource_id=experiment.id,
    )
    append_event(
        session,
        kind="QUANT_EXPERIMENT_QUEUED",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=mission.program_id,
        payload={
            "mission_id": str(mission.id),
            "experiment_id": str(experiment.id),
            "zone": experiment.zone,
        },
    )
    return experiment


def submit_contract_from_workspace(
    settings: Settings,
    *,
    mission_id: UUID,
    workspace: Path,
) -> UUID | None:
    """Admit a Codex-authored contract without giving Codex DB or runtime credentials."""
    path = workspace / "experiment-contract.json"
    if not path.is_file():
        return None
    try:
        contract = ExperimentContract.model_validate_json(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise QfError(
            "EXPERIMENT_CONTRACT_INVALID",
            "experiment-contract.json does not satisfy the governed Nautilus contract.",
            422,
        ) from exc
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            return submit_experiment(
                session,
                mission_id=mission_id,
                contract=contract,
            ).id
    finally:
        engine.dispose()


def _runtime(zone: Literal["DISCOVERY", "SEALED"]) -> RemoteNautilusQuantRuntime:
    return RemoteNautilusQuantRuntime.from_env(zone)


def _evidence_summary(evidence: BacktestEvidence) -> dict[str, Any]:
    return {
        "runtime_name": evidence.runtime_name,
        "runtime_version": evidence.runtime_version,
        "run_id": str(evidence.run_id),
        "total_events": evidence.total_events,
        "total_orders": evidence.total_orders,
        "total_positions": evidence.total_positions,
        "statistics": evidence.statistics,
        "disclosure": evidence.disclosure,
    }


def _create_sealed_experiment(session: Session, discovery: QuantExperiment) -> QuantExperiment:
    existing = session.scalar(
        select(QuantExperiment).where(
            QuantExperiment.parent_experiment_id == discovery.id,
            QuantExperiment.zone == "SEALED",
        )
    )
    if existing is not None:
        return existing
    contract = ExperimentContract.model_validate(discovery.contract_json)
    sealed_contract = contract.model_copy(
        update={
            "run_id": uuid4(),
            "catalog": contract.catalog.model_copy(update={"partition": "SEALED"}),
        }
    )
    sealed = QuantExperiment(
        parent_experiment_id=discovery.id,
        mission_id=discovery.mission_id,
        program_id=discovery.program_id,
        dataset_revision_id=discovery.dataset_revision_id,
        zone="SEALED",
        state="READY",
        runtime_name="NAUTILUS_TRADER",
        runtime_version=PINNED_NAUTILUS_VERSION,
        strategy_artifact=dict(discovery.strategy_artifact),
        contract_json=sealed_contract.model_dump(mode="json"),
    )
    session.add(sealed)
    session.flush()
    _ledger(session, experiment=sealed, outcome="QUEUED")
    _enqueue(session, kind="NAUTILUS_SEALED_BACKTEST", resource_id=sealed.id)
    append_event(
        session,
        kind="SEALED_QUANT_EXPERIMENT_QUEUED",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=sealed.program_id,
        payload={
            "discovery_experiment_id": str(discovery.id),
            "sealed_experiment_id": str(sealed.id),
        },
    )
    return sealed


def _promote_from_sealed(
    session: Session,
    experiment: QuantExperiment,
    evidence: BacktestEvidence,
) -> None:
    decision = str(evidence.disclosure.get("decision") or "FAIL")
    evaluation = session.scalar(
        select(SealedEvaluation).where(SealedEvaluation.experiment_id == experiment.id)
    )
    if evaluation is None:
        evaluation = SealedEvaluation(
            experiment_id=experiment.id,
            dataset_revision_id=experiment.dataset_revision_id,
            state="EVALUATED",
            decision=decision,
            runtime_version=evidence.runtime_version,
            disclosure=dict(evidence.disclosure),
            created_at=_now(),
        )
        session.add(evaluation)
        session.flush()
    if decision != "PASS":
        append_event(
            session,
            kind="SEALED_QUANT_EXPERIMENT_REJECTED",
            aggregate_type="RESEARCH_PROGRAM",
            aggregate_id=experiment.program_id,
            payload={
                "experiment_id": str(experiment.id),
                "classification": evidence.disclosure.get("classification"),
            },
        )
        return

    alpha = session.scalar(
        select(AlphaQualification).where(
            AlphaQualification.evaluation_episode_id == evaluation.id
        )
    )
    contract = ExperimentContract.model_validate(experiment.contract_json)
    if alpha is None:
        alpha = AlphaQualification(
            program_id=experiment.program_id,
            alpha_model_version_id=contract.strategy.artifact_id,
            universe=contract.catalog.instrument_ids[0],
            horizon="NAUTILUS_BACKTEST",
            role="PRIMARY_ALPHA",
            state="ACTIVE",
            name=f"Nautilus strategy {contract.strategy.strategy_path}",
            scope_json={
                "dataset_revision_id": str(experiment.dataset_revision_id),
                "catalog_uri": contract.catalog.catalog_uri,
                "runtime_version": evidence.runtime_version,
            },
            evaluation_episode_id=evaluation.id,
            metrics=_evidence_summary(evidence),
            lineage=[
                {
                    "experiment_id": str(experiment.id),
                    "parent_experiment_id": (
                        str(experiment.parent_experiment_id)
                        if experiment.parent_experiment_id
                        else None
                    ),
                }
            ],
            created_at=_now(),
        )
        session.add(alpha)
        session.flush()

    existing_candidate = session.scalar(
        select(PortfolioCandidate).where(
            PortfolioCandidate.evaluation_episode_id == evaluation.id
        )
    )
    if existing_candidate is not None:
        return
    mandate = session.scalar(
        select(PortfolioMandate).where(
            PortfolioMandate.enabled.is_(True),
            PortfolioMandate.state == "ACTIVE",
        )
    )
    mandate_version_id = mandate.latest_version_id if mandate else uuid4()
    mandate_name = mandate.name if mandate else "Nautilus Research Default"
    portfolio_program = PortfolioProgram(
        mandate_version_id=mandate_version_id,
        mandate_name=mandate_name,
        state="CANDIDATE_READY",
    )
    session.add(portfolio_program)
    session.flush()
    members = contract.portfolio_targets or [
        {"instrument_id": contract.catalog.instrument_ids[0], "target_weight": 1.0}
    ]
    discovery = session.get(QuantExperiment, experiment.parent_experiment_id)
    candidate = PortfolioCandidate(
        candidate_family_id=uuid4(),
        portfolio_program_id=portfolio_program.id,
        mandate_version_id=mandate_version_id,
        mandate_name=mandate_name,
        universe_set_json=contract.catalog.instrument_ids,
        policy_version="NAUTILUS_TRANSACTION_SIMULATED_V1",
        risk_model_version=f"NAUTILUS_RISK_ENGINE_{evidence.runtime_version}",
        cost_model_version=f"NAUTILUS_VENUE_{contract.venue.name}",
        capacity_model_version="NAUTILUS_SIMULATION_EVIDENCE_V1",
        constraint_set_version="GOVERNED_MANDATE_V1",
        rebalance_policy_version="STRATEGY_NATIVE_V1",
        evaluation_episode_id=evaluation.id,
        state="READY",
        members=members,
        metrics={
            "strategy_artifact": contract.strategy.model_dump(mode="json"),
            "quant_evidence": {
                "discovery": dict(discovery.evidence_json) if discovery else {},
                "sealed": evidence.model_dump(mode="json"),
            },
            "nautilus_runtime": {
                "name": evidence.runtime_name,
                "version": evidence.runtime_version,
            },
        },
        created_at=_now(),
    )
    session.add(candidate)
    session.flush()
    portfolio_program.current_candidate_id = candidate.id

    downstream = session.scalar(
        select(DownstreamSystem).where(
            DownstreamSystem.enabled.is_(True),
            DownstreamSystem.environment_type == "PAPER",
            DownstreamSystem.preflight_state == "READY",
        )
    )
    if downstream is not None:
        approval = ApprovalSnapshot(
            candidate_id=candidate.id,
            purpose="PAPER",
            state="PENDING",
            downstream_system_id=downstream.id,
            recommendation_rationale=(
                "Discovery and isolated Sealed NautilusTrader runs produced executable "
                "order, fill, position and PnL evidence."
            ),
            human_report={
                "runtime": evidence.runtime_name,
                "runtime_version": evidence.runtime_version,
                "sealed_disclosure": evidence.disclosure,
            },
            evidence_summary=_evidence_summary(evidence),
            capital_context={},
            risk_summary={"execution_risk_owner": "NAUTILUS_TRADER"},
            cost_summary={"simulation_owner": "NAUTILUS_TRADER"},
            capacity_summary={},
            changes_summary={"source": "NAUTILUS_SEALED_EVIDENCE"},
        )
        session.add(approval)
        program = session.get(ResearchProgram, experiment.program_id)
        if program is not None:
            program.state = "APPROVAL_PENDING"
    append_event(
        session,
        kind="NAUTILUS_ALPHA_PROMOTED",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=experiment.program_id,
        payload={
            "experiment_id": str(experiment.id),
            "alpha_qualification_id": str(alpha.id),
            "portfolio_candidate_id": str(candidate.id),
        },
    )


def process_experiment(
    session: Session,
    *,
    experiment_id: UUID,
    runtime: Any | None = None,
) -> QuantExperiment:
    experiment = session.execute(
        select(QuantExperiment)
        .where(QuantExperiment.id == experiment_id)
        .with_for_update()
    ).scalar_one_or_none()
    if experiment is None:
        raise QfError("QUANT_EXPERIMENT_NOT_FOUND", "Quant experiment does not exist.", 404)
    if experiment.state == "SUCCEEDED":
        return experiment
    if experiment.state not in {"READY", "FAILED"}:
        raise QfError(
            "QUANT_EXPERIMENT_STATE_CONFLICT",
            "Quant experiment cannot run in its current state.",
            409,
            {"state": experiment.state},
        )
    experiment.state = "RUNNING"
    experiment.started_at = _now()
    contract = ExperimentContract.model_validate(experiment.contract_json)
    owned_runtime = runtime is None
    client = runtime or _runtime(experiment.zone)  # type: ignore[arg-type]
    try:
        evidence = (
            client.run_backtest(contract)
            if experiment.zone == "DISCOVERY"
            else client.run_sealed_backtest(contract)
        )
        if evidence.runtime_version != PINNED_NAUTILUS_VERSION:
            raise QfError(
                "NAUTILUS_RUNTIME_VERSION_MISMATCH",
                "Remote runtime version does not match the pinned QuaZonai contract.",
                409,
                {
                    "expected": PINNED_NAUTILUS_VERSION,
                    "actual": evidence.runtime_version,
                },
            )
        experiment.state = "SUCCEEDED"
        experiment.runtime_version = evidence.runtime_version
        experiment.evidence_json = evidence.model_dump(mode="json")
        experiment.disclosure_json = dict(evidence.disclosure)
        experiment.error_code = None
        experiment.error_detail = None
        experiment.finished_at = _now()
        _ledger(
            session,
            experiment=experiment,
            outcome="SUCCEEDED",
            evidence_summary=_evidence_summary(evidence),
        )
        if experiment.zone == "DISCOVERY":
            _create_sealed_experiment(session, experiment)
        else:
            _promote_from_sealed(session, experiment, evidence)
        append_event(
            session,
            kind="QUANT_EXPERIMENT_SUCCEEDED",
            aggregate_type="RESEARCH_PROGRAM",
            aggregate_id=experiment.program_id,
            payload={
                "experiment_id": str(experiment.id),
                "zone": experiment.zone,
                "runtime_version": evidence.runtime_version,
            },
        )
        session.flush()
        return experiment
    except Exception as exc:
        experiment.state = "FAILED"
        experiment.error_code = str(getattr(exc, "code", type(exc).__name__))[:100]
        experiment.error_detail = str(exc)[-4000:]
        experiment.finished_at = _now()
        _ledger(
            session,
            experiment=experiment,
            outcome="FAILED",
            evidence_summary={"error_code": experiment.error_code},
        )
        append_event(
            session,
            kind="QUANT_EXPERIMENT_FAILED",
            aggregate_type="RESEARCH_PROGRAM",
            aggregate_id=experiment.program_id,
            payload={
                "experiment_id": str(experiment.id),
                "zone": experiment.zone,
                "error_code": experiment.error_code,
            },
        )
        session.flush()
        raise
    finally:
        if owned_runtime and hasattr(client, "close"):
            client.close()


def run_job(settings: Settings, job_id: UUID) -> None:
    engine = create_database_engine(settings)
    try:
        factory = create_session_factory(engine)
        with factory.begin() as session:
            job = session.get(Job, job_id)
            if job is None or job.kind not in {
                "NAUTILUS_DISCOVERY_BACKTEST",
                "NAUTILUS_SEALED_BACKTEST",
            }:
                raise QfError("JOB_NOT_FOUND", "Nautilus experiment job does not exist.", 404)
            process_experiment(session, experiment_id=job.resource_id)
    finally:
        engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one remote Nautilus experiment")
    parser.add_argument("action", choices=["run"])
    parser.add_argument("job_id")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    run_job(Settings.from_env(), UUID(args.job_id))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
