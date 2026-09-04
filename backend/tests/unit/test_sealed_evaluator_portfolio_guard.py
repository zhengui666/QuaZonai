from __future__ import annotations

from dataclasses import replace
from datetime import UTC, datetime, timedelta
from math import nan
from pathlib import Path
from typing import Any
from uuid import uuid4

import pytest
from sqlalchemy import Engine, create_engine, func, select
from sqlalchemy.orm import Session

from db.models import (
    AlphaCalibrationVersion,
    AlphaEvaluationEpisode,
    AlphaModel,
    AlphaModelVersion,
    AlphaQualification,
    AlphaSignalArtifact,
    Base,
    DatasetRevision,
    Job,
    MarketUniverseVersion,
    NautilusCatalogBinding,
    PortfolioCandidate,
    QuantRuntimeRun,
    ResearchBranch,
    ResearchCharter,
    ResearchMission,
    ResearchProgram,
)
from errors import QfError
from jobs import JobLease, claim_next_job
from quant_runtime.alpha_contracts import AlphaPoint, AlphaSignalFrameV1
from research_engine.alpha_assets import persist_trusted_alpha_evaluation
from runners.sealed_evaluator import run_sealed_evaluation
from settings import Settings


def _seed(
    session: Session,
    *,
    quality_state: str = "VALID",
    point_in_time_state: str = "VALID",
    promotability: str | None = "PROMOTABLE",
    partition: str = "SEALED",
    catalog_sealed: bool | None = True,
) -> dict[str, Any]:
    now = datetime(2026, 1, 1, tzinfo=UTC)
    charter = ResearchCharter(
        original_idea_text="Test Alpha evidence.",
        research_question="Can the Alpha signal be evaluated honestly?",
        market_scope=[],
        universe_version_ids=[],
        prediction_horizon="1D",
        allowed_data_domains=[],
        explicit_exclusions=[],
        material_assumptions=[],
        system_assumptions=[],
        clarification_transcript=[],
        created_at=now,
    )
    session.add(charter)
    session.flush()
    program = ResearchProgram(charter_id=charter.id, title="alpha evaluation")
    session.add(program)
    session.flush()
    branch = ResearchBranch(
        program_id=program.id,
        derivation_type="ROOT",
        hypothesis="Signals remain point-in-time valid.",
        changed_assumptions=[],
        preserved_constraints=[],
        state="ACTIVE",
        revision_no=1,
        created_at=now,
    )
    session.add(branch)
    session.flush()
    mission = ResearchMission(
        program_id=program.id,
        branch_id=branch.id,
        type="ALPHA_DISCOVERY",
        role="ALPHA_RESEARCHER",
        state="PLANNED",
        objective="Produce one Alpha signal frame.",
        contract_version="1",
        input_snapshot={},
        capability_snapshot={},
        runtime_snapshot={},
        prompt_version="1",
        max_turns=1,
        max_tool_calls=0,
        attempt=1,
        revision=1,
    )
    universe = MarketUniverseVersion(
        universe_key="US_EQUITIES",
        version_no=1,
        name="US Equities",
        state="ACTIVE",
        spec_json={},
        created_at=now,
    )
    session.add_all((mission, universe))
    session.flush()
    dataset = DatasetRevision(
        universe_version_id=universe.id,
        universe_name=universe.name,
        revision_no=1,
        data_class="VENDOR",
        origin="trusted-sealed-evidence",
        ingested_at=now,
        promotability=promotability,
        schema_version="1",
        event_start=now,
        event_end=now + timedelta(days=2),
        available_start=now,
        available_end=now + timedelta(days=2),
        row_count=2,
        quality_state=quality_state,
        point_in_time_state=point_in_time_state,
        partition=partition,
        created_at=now,
    )
    model = AlphaModel(
        alpha_key=f"alpha-{uuid4()}",
        name="Trusted Alpha",
        family="MOMENTUM",
        description="A pure signal asset.",
        owner_program_id=program.id,
        state="RESEARCHING",
    )
    session.add_all((dataset, model))
    session.flush()
    if catalog_sealed is not None:
        session.add(
            NautilusCatalogBinding(
                dataset_revision_id=dataset.id,
                catalog_uri=f"sealed://catalog/{dataset.id}",
                provider="Trusted Provider",
                source_license="TRUSTED",
                nautilus_data_type="QuoteTick",
                instrument_scope=["AAPL.XNAS"],
                event_time_range={"start": now.isoformat(), "end": (now + timedelta(days=2)).isoformat()},
                available_time_range={
                    "start": now.isoformat(),
                    "end": (now + timedelta(days=2)).isoformat(),
                },
                schema_revision="1",
                quality_state=quality_state,
                quality_result={"valid": quality_state == "VALID"},
                point_in_time_state=point_in_time_state,
                point_in_time_result={"valid": point_in_time_state == "VALID"},
                sealed=catalog_sealed,
            )
        )
    version = AlphaModelVersion(
        alpha_model_id=model.id,
        version_no=1,
        source_mission_id=mission.id,
        universe_version_id=universe.id,
        horizon="1D",
        mode="CALIBRATED_RETURN",
        artifact_uri="trusted://alpha/model",
        entrypoint="alpha:signals",
        parameters={},
        input_contract={},
        output_contract={},
        state="VALIDATED",
    )
    session.add(version)
    session.flush()
    calibration = AlphaCalibrationVersion(
        alpha_model_version_id=version.id,
        version_no=1,
        method="ISOTONIC",
        training_dataset_revision_ids=[],
        artifact_uri="trusted://alpha/calibration",
        parameters={},
        metrics={},
        state="VALIDATED",
    )
    run = QuantRuntimeRun(
        program_id=program.id,
        branch_id=branch.id,
        mission_id=mission.id,
        mode="SEALED",
        state="RUNNING",
        experiment_key="trusted-alpha-evaluation",
        family="MOMENTUM",
        catalog_uri=f"sealed://catalog/{dataset.id}",
        runtime_name="alpha-evaluator",
    )
    session.add_all((calibration, run))
    session.flush()
    episode = AlphaEvaluationEpisode(
        program_id=program.id,
        branch_id=branch.id,
        alpha_model_version_id=version.id,
        discovery_run_ids=[],
        validation_run_ids=[],
        sealed_dataset_revision_id=dataset.id,
        promotion_policy_version_id=uuid4(),
        state="PENDING",
        gate_results={},
        multiple_testing_summary={},
        disclosure={},
    )
    session.add(episode)
    session.flush()
    return {
        "now": now,
        "dataset": dataset,
        "calibration": calibration,
        "episode": episode,
        "run": run,
    }


def _frame(now: datetime) -> AlphaSignalFrameV1:
    return AlphaSignalFrameV1(
        points=(
            AlphaPoint(
                event_time=now,
                available_time=now,
                instrument_id="AAPL.XNAS",
                score=1.0,
                expected_return=0.01,
                uncertainty=0.1,
                horizon_ns=86_400_000_000_000,
            ),
            AlphaPoint(
                event_time=now + timedelta(minutes=1),
                available_time=now + timedelta(minutes=1),
                instrument_id="AAPL.XNAS",
                score=-1.0,
                expected_return=-0.01,
                uncertainty=0.1,
                horizon_ns=86_400_000_000_000,
            ),
        )
    )


def _multi_instrument_frame(now: datetime) -> AlphaSignalFrameV1:
    frame = _frame(now)
    return frame.model_copy(
        update={
            "points": (
                frame.points[0],
                frame.points[1].model_copy(update={"instrument_id": "MSFT.XNAS"}),
            )
        }
    )


def _returns(frame: AlphaSignalFrameV1, values: tuple[float | None, ...]) -> list[dict[str, Any]]:
    return [
        {
            "event_time": point.event_time.isoformat(),
            "instrument_id": point.instrument_id,
            "realized_return": value,
        }
        for point, value in zip(frame.points, values, strict=True)
    ]


def _evaluate(
    session: Session,
    context: dict[str, Any],
    *,
    returns: object,
    signal_frame: AlphaSignalFrameV1 | None = None,
) -> object:
    frame = signal_frame if signal_frame is not None else _frame(context["now"])
    return persist_trusted_alpha_evaluation(
        session,
        episode_id=context["episode"].id,
        run_id=context["run"].id,
        artifact_uri="trusted://alpha/signals",
        signal_frame=frame,
        realized_returns=returns,
        annualization_factor=252,
        trial_count=1,
        calibration_version_id=context["calibration"].id,
        qualification_role="PRIMARY_ALPHA",
    )


def test_valid_promotable_evidence_persists_real_alpha_facts(engine: Engine) -> None:
    with Session(engine) as session:
        context = _seed(session)
        frame = _frame(context["now"])
        outcome = _evaluate(session, context, returns=_returns(frame, (0.02, -0.01)))

        assert outcome.result == "PASS"
        assert outcome.signal_artifact_id is not None
        assert outcome.qualification_id is not None
        assert _evaluate(session, context, returns=_returns(frame, (0.02, -0.01))) == outcome
        episode = session.get(AlphaEvaluationEpisode, context["episode"].id)
        qualification = session.get(AlphaQualification, outcome.qualification_id)
        assert episode is not None and episode.gate_results["evidence"] == "PASS"
        assert qualification is not None and qualification.state == "ACTIVE"
        assert qualification.scope_json["instrument_id"] == "AAPL.XNAS"
        assert qualification.metrics["net_return"]["value"] == pytest.approx(0.01)
        assert session.scalar(select(func.count()).select_from(AlphaSignalArtifact)) == 1
        assert session.scalar(select(func.count()).select_from(AlphaQualification)) == 1
        assert session.scalar(select(func.count()).select_from(PortfolioCandidate)) == 0


def test_primary_alpha_refuses_ambiguous_multi_instrument_scope(engine: Engine) -> None:
    with Session(engine) as session:
        context = _seed(session)
        frame = _multi_instrument_frame(context["now"])
        outcome = _evaluate(
            session,
            context,
            signal_frame=frame,
            returns=_returns(frame, (0.02, -0.01)),
        )

        assert outcome.result == "INCONCLUSIVE"
        assert outcome.qualification_id is None
        episode = session.get(AlphaEvaluationEpisode, context["episode"].id)
        assert episode is not None
        assert episode.gate_results["portfolio_scope"] == "PORTFOLIO_SCOPE_UNSUPPORTED"
        assert session.scalar(select(func.count()).select_from(AlphaQualification)) == 0
        assert session.scalar(select(func.count()).select_from(PortfolioCandidate)) == 0


@pytest.mark.parametrize(
    ("quality_state", "point_in_time_state", "promotability"),
    (("PENDING", "PENDING", None), ("INVALID", "INVALID", "NON_PROMOTABLE")),
)
def test_pending_or_invalid_dataset_never_persists_alpha_asset(
    engine: Engine,
    quality_state: str,
    point_in_time_state: str,
    promotability: str | None,
) -> None:
    with Session(engine) as session:
        context = _seed(
            session,
            quality_state=quality_state,
            point_in_time_state=point_in_time_state,
            promotability=promotability,
        )
        frame = _frame(context["now"])
        outcome = _evaluate(session, context, returns=_returns(frame, (0.02, -0.01)))

        assert outcome.result == "INVALID"
        assert outcome.signal_artifact_id is None
        assert outcome.qualification_id is None
        assert session.scalar(select(func.count()).select_from(AlphaSignalArtifact)) == 0
        assert session.scalar(select(func.count()).select_from(AlphaQualification)) == 0
        assert session.scalar(select(func.count()).select_from(PortfolioCandidate)) == 0


@pytest.mark.parametrize(
    ("partition", "catalog_sealed", "catalog_gate"),
    (("DISCOVERY", True, "PASS"), ("SEALED", False, "INVALID"), ("SEALED", None, "INVALID")),
    ids=("discovery-dataset", "unsealed-catalog", "missing-catalog"),
)
def test_sealed_evaluation_requires_a_sealed_dataset_and_catalog_binding(
    engine: Engine, partition: str, catalog_sealed: bool | None, catalog_gate: str
) -> None:
    with Session(engine) as session:
        context = _seed(session, partition=partition, catalog_sealed=catalog_sealed)
        frame = _frame(context["now"])
        outcome = _evaluate(session, context, returns=_returns(frame, (0.02, -0.01)))

        episode = session.get(AlphaEvaluationEpisode, context["episode"].id)
        assert outcome.result == "INVALID"
        assert outcome.signal_artifact_id is None
        assert outcome.qualification_id is None
        assert episode is not None
        assert episode.gate_results["dataset_partition"] == partition
        assert episode.gate_results["sealed_catalog"] == catalog_gate
        assert session.scalar(select(func.count()).select_from(AlphaSignalArtifact)) == 0
        assert session.scalar(select(func.count()).select_from(AlphaQualification)) == 0


def test_missing_realized_return_is_inconclusive_without_qualification(engine: Engine) -> None:
    with Session(engine) as session:
        context = _seed(session)
        frame = _frame(context["now"])
        outcome = _evaluate(session, context, returns=_returns(frame, (None, -0.01)))

        assert outcome.result == "INCONCLUSIVE"
        assert outcome.signal_artifact_id is not None
        assert outcome.qualification_id is None
        episode = session.get(AlphaEvaluationEpisode, context["episode"].id)
        assert episode is not None and episode.gate_results["evidence"] == "INCONCLUSIVE"
        assert session.scalar(select(func.count()).select_from(AlphaQualification)) == 0
        assert session.scalar(select(func.count()).select_from(PortfolioCandidate)) == 0


def test_nonfinite_realized_return_is_invalid_without_alpha_asset(engine: Engine) -> None:
    with Session(engine) as session:
        context = _seed(session)
        frame = _frame(context["now"])
        outcome = _evaluate(session, context, returns=_returns(frame, (nan, -0.01)))

        assert outcome.result == "INVALID"
        assert outcome.signal_artifact_id is None
        assert outcome.qualification_id is None
        assert session.scalar(select(func.count()).select_from(AlphaSignalArtifact)) == 0
        assert session.scalar(select(func.count()).select_from(AlphaQualification)) == 0
        assert session.scalar(select(func.count()).select_from(PortfolioCandidate)) == 0


def test_worker_rejects_raw_sealed_evaluation_job_payload(
    settings: Settings,
    tmp_path: Path,
) -> None:
    database_url = f"sqlite+pysqlite:///{tmp_path / 'sealed-evaluator.sqlite'}"
    worker_settings = replace(settings, database_url=database_url, alembic_url=database_url)
    database = create_engine(database_url)
    Base.metadata.create_all(database)
    try:
        with Session(database) as session:
            context = _seed(session)
            frame = _frame(context["now"])
            job = Job(
                kind="SEALED_EVALUATION",
                resource_type="alpha_evaluation_episode",
                resource_id=context["episode"].id,
                payload={
                    "run_id": str(context["run"].id),
                    "artifact_uri": "trusted://alpha/signals",
                    "signal_frame": frame.model_dump(mode="json"),
                    "realized_returns": _returns(frame, (0.02, -0.01)),
                    "annualization_factor": 252,
                    "trial_count": 1,
                    "calibration_version_id": str(context["calibration"].id),
                    "qualification_role": "PRIMARY_ALPHA",
                },
            )
            session.add(job)
            session.commit()
            episode_id = context["episode"].id

        with Session(database) as session, session.begin():
            claimed = claim_next_job(session, owner="worker", lease_seconds=60)
            assert claimed is not None and claimed.lease_owner is not None
            lease = JobLease(claimed.id, claimed.lease_owner, claimed.attempt)

        with pytest.raises(QfError, match="SEALED_EVALUATION_RAW_PAYLOAD_FORBIDDEN"):
            run_sealed_evaluation(worker_settings, lease)

        with Session(database) as session:
            episode = session.get(AlphaEvaluationEpisode, episode_id)
            assert episode is not None and episode.result is None
            assert session.scalar(select(func.count()).select_from(AlphaSignalArtifact)) == 0
            assert session.scalar(select(func.count()).select_from(AlphaQualification)) == 0
    finally:
        Base.metadata.drop_all(database)
        database.dispose()


def test_worker_requires_a_trusted_sealed_assignment(
    settings: Settings,
    tmp_path: Path,
) -> None:
    database_url = f"sqlite+pysqlite:///{tmp_path / 'sealed-assignment.sqlite'}"
    worker_settings = replace(settings, database_url=database_url, alembic_url=database_url)
    database = create_engine(database_url)
    Base.metadata.create_all(database)
    try:
        with Session(database) as session:
            context = _seed(session)
            job = Job(
                kind="SEALED_EVALUATION",
                resource_type="alpha_evaluation_episode",
                resource_id=context["episode"].id,
                payload={},
            )
            session.add(job)
            session.commit()
            episode_id = context["episode"].id

        with Session(database) as session, session.begin():
            claimed = claim_next_job(session, owner="worker", lease_seconds=60)
            assert claimed is not None and claimed.lease_owner is not None
            lease = JobLease(claimed.id, claimed.lease_owner, claimed.attempt)

        with pytest.raises(QfError, match="SEALED_EVALUATOR_ASSIGNMENT_UNAVAILABLE"):
            run_sealed_evaluation(worker_settings, lease)

        with Session(database) as session:
            episode = session.get(AlphaEvaluationEpisode, episode_id)
            assert episode is not None and episode.result is None
            assert session.scalar(select(func.count()).select_from(AlphaSignalArtifact)) == 0
            assert session.scalar(select(func.count()).select_from(AlphaQualification)) == 0
    finally:
        Base.metadata.drop_all(database)
        database.dispose()
