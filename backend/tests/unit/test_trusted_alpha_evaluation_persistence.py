from __future__ import annotations

import importlib.util
from datetime import UTC, datetime, timedelta
from decimal import Decimal
from pathlib import Path
from uuid import UUID, uuid4

import pytest
from alembic import command
from alembic.config import Config
from alembic.migration import MigrationContext
from alembic.operations import Operations
from sqlalchemy import MetaData, Table, create_engine, inspect, select
from sqlalchemy.exc import DataError, IntegrityError
from sqlalchemy.orm import Session

from db.models import (
    AlphaCalibrationVersion,
    AlphaDiscoveryEvaluation,
    AlphaDiscoveryEvaluationGate,
    AlphaDiscoveryEvaluationMetric,
    AlphaEvaluationAssignment,
    AlphaEvaluationAssignmentDatasetRevision,
    AlphaEvaluationEpisode,
    AlphaEvaluationForecast,
    AlphaEvaluationGate,
    AlphaEvaluationMetric,
    AlphaEvaluationResult,
    AlphaModel,
    AlphaModelVersion,
    AlphaQualification,
    AlphaSignalArtifact,
    Disclosure,
    EvaluationDatasetSelection,
    EvaluationDesignVersion,
    Event,
    EvidenceExposure,
    MarketUniverseVersion,
    MissionArtifact,
    PromotionPolicyGate,
    PromotionPolicyVersion,
    ResearchBranch,
    ResearchCharter,
    ResearchCycle,
    ResearchMission,
    ResearchProgram,
    DatasetRevision,
)


_BACKEND_ROOT = Path(__file__).parents[2]


def _migration() -> object:
    path = _BACKEND_ROOT / "alembic/versions/0023_trusted_alpha_evaluation.py"
    spec = importlib.util.spec_from_file_location("migration_0023", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _migration_0025() -> object:
    path = _BACKEND_ROOT / "alembic/versions/0025_typed_discovery_calibration.py"
    spec = importlib.util.spec_from_file_location("migration_0025", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _seed_assignment(session: Session) -> dict[str, object]:
    now = datetime(2026, 9, 3, tzinfo=UTC)
    charter = ResearchCharter(
        original_idea_text="Evaluate one bounded Alpha proposal.",
        research_question="Can it pass the sealed evaluator?",
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
    program = ResearchProgram(charter_id=charter.id, title="trusted alpha")
    session.add(program)
    session.flush()
    cycle = ResearchCycle(
        program_id=program.id,
        cycle_no=1,
        trigger="IDEA_START",
        state="PLANNED",
        mission_budget=1,
        replan_budget=0,
        runtime_configuration_revision=1,
        summary={},
        created_at=now,
    )
    branch = ResearchBranch(
        program_id=program.id,
        cycle_id=cycle.id,
        derivation_type="ROOT",
        hypothesis="One deterministic Alpha proposal.",
        changed_assumptions=[],
        preserved_constraints=[],
        state="ACTIVE",
        revision_no=1,
        created_at=now,
    )
    session.add_all((cycle, branch))
    session.flush()
    mission = ResearchMission(
        program_id=program.id,
        cycle_id=cycle.id,
        branch_id=branch.id,
        type="ALPHA_DISCOVERY",
        role="ALPHA_RESEARCHER",
        state="SUCCEEDED",
        objective="Create one proposal.",
        contract_version="1",
        input_snapshot={},
        capability_snapshot={},
        runtime_snapshot={},
        prompt_version="1",
        max_turns=1,
        max_tool_calls=0,
        attempt=1,
        revision=1,
        finished_at=now,
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
    artifact = MissionArtifact(
        mission_id=mission.id,
        kind="ALPHA_PROPOSAL",
        schema_version="1",
        revision=1,
        state="VALIDATED",
        storage_uri="artifact://alpha-proposal",
        metadata_json={},
        created_at=now,
    )
    datasets = [
        DatasetRevision(
            universe_version_id=universe.id,
            universe_name=universe.name,
            revision_no=index,
            data_class="VENDOR",
            origin="trusted",
            ingested_at=now,
            promotability="PROMOTABLE",
            schema_version="1",
            event_start=now,
            event_end=now + timedelta(days=1),
            available_start=now,
            available_end=now + timedelta(days=1),
            row_count=1,
            quality_state="VALID",
            point_in_time_state="VALID",
            partition=partition,
            materialization_request={},
            created_at=now,
        )
        for index, partition in enumerate(("DISCOVERY", "VALIDATION", "SEALED"), start=1)
    ]
    session.add_all((artifact, *datasets))
    session.flush()
    selection = EvaluationDatasetSelection(
        universe_version_id=universe.id,
        version_no=1,
        discovery_dataset_revision_id=datasets[0].id,
        validation_dataset_revision_id=datasets[1].id,
        sealed_dataset_revision_id=datasets[2].id,
        state="ENABLED",
    )
    design = EvaluationDesignVersion(
        version_no=1,
        universe_version_id=universe.id,
        contract_version="1",
        allowed_model_mode="CALIBRATED_RETURN",
        qualification_role="PRIMARY_ALPHA",
        walk_forward_folds=2,
        annualization_factor=Decimal("252"),
        multiple_testing_method="BONFERRONI",
        multiple_testing_max_trials=1,
        qualification_metric_code="NET_EDGE",
        qualification_comparator="MINIMUM",
        qualification_threshold=Decimal("0.01"),
        pass_disclosure_code="QUALIFIED",
        failure_disclosure_code="INSUFFICIENT_NET_EDGE",
        inconclusive_disclosure_code="INCONCLUSIVE",
        invalid_disclosure_code="DATA_QUALITY_FAILURE",
        state="ACTIVE",
    )
    policy = PromotionPolicyVersion(
        version_no=1,
        purpose="SEALED_TO_QUALIFIED",
        mode="MANUAL_APPROVAL",
        state="ACTIVE",
    )
    session.add_all((selection, design, policy))
    session.flush()
    session.add(
        PromotionPolicyGate(
            policy_version_id=policy.id,
            metric_code="NET_EDGE",
            comparator="MINIMUM",
            threshold=Decimal("0.01"),
            ordinal=1,
        )
    )
    alpha = AlphaModel(
        alpha_key="trusted-alpha",
        name="Trusted Alpha",
        family="MOMENTUM",
        description="A bounded proposed model.",
        owner_program_id=program.id,
        state="RESEARCHING",
    )
    session.add(alpha)
    session.flush()
    model = AlphaModelVersion(
        alpha_model_id=alpha.id,
        version_no=1,
        source_mission_id=mission.id,
        source_mission_artifact_id=artifact.id,
        source_mission_artifact_revision=artifact.revision,
        universe_version_id=universe.id,
        horizon="1D",
        mode="CALIBRATED_RETURN",
        artifact_uri="artifact://alpha-model",
        entrypoint="alpha:run",
        parameters={},
        input_contract={},
        output_contract={},
        state="VALIDATED",
    )
    session.add(model)
    session.flush()
    event = Event(
        kind="ALPHA_PROPOSAL_VALIDATED", aggregate_type="MISSION", aggregate_id=mission.id
    )
    session.add(event)
    session.flush()
    discovery = AlphaDiscoveryEvaluation(
        source_mission_artifact_id=artifact.id,
        source_mission_artifact_revision=artifact.revision,
        alpha_model_version_id=model.id,
        program_id=program.id,
        cycle_id=cycle.id,
        branch_id=branch.id,
        mission_id=mission.id,
        discovery_dataset_revision_id=datasets[0].id,
        evaluation_dataset_selection_id=selection.id,
        evaluation_design_version_id=design.id,
        cause_event_id=event.id,
        evaluator_contract_version="1",
        state="VALID",
        outcome_code="DISCOVERY_VALIDATED",
        private_result_ref=uuid4(),
        evaluated_at=now,
        completed_at=now,
    )
    session.add(discovery)
    session.flush()
    session.add_all(
        (
            AlphaDiscoveryEvaluationMetric(
                discovery_evaluation_id=discovery.id,
                metric_code="IC_MEAN",
                value=Decimal("0.02"),
                status="AVAILABLE",
            ),
            AlphaDiscoveryEvaluationGate(
                discovery_evaluation_id=discovery.id,
                gate_code="EVIDENCE_VALID",
                status="PASS",
                reason_code=None,
            ),
        )
    )
    calibration = AlphaCalibrationVersion(
        alpha_model_version_id=model.id,
        version_no=1,
        method="ISOTONIC",
        training_dataset_revision_ids=[],
        artifact_uri=None,
        source_discovery_evaluation_id=discovery.id,
        training_dataset_revision_id=datasets[0].id,
        private_artifact_ref=uuid4(),
        parameters={},
        metrics={},
        state="VALIDATED",
    )
    session.add(calibration)
    session.flush()
    assignment = AlphaEvaluationAssignment(
        source_mission_artifact_id=artifact.id,
        source_mission_artifact_revision=artifact.revision,
        discovery_evaluation_id=discovery.id,
        program_id=program.id,
        cycle_id=cycle.id,
        branch_id=branch.id,
        mission_id=mission.id,
        alpha_model_version_id=model.id,
        alpha_calibration_version_id=calibration.id,
        universe_version_id=universe.id,
        sealed_dataset_revision_id=datasets[2].id,
        evaluation_design_version_id=design.id,
        promotion_policy_version_id=policy.id,
        cause_event_id=event.id,
        assignment_no=1,
        evaluator_contract_version="1",
        state="FROZEN",
    )
    session.add(assignment)
    session.flush()
    session.add_all(
        AlphaEvaluationAssignmentDatasetRevision(
            assignment_id=assignment.id,
            dataset_revision_id=dataset.id,
            phase=phase,
            ordinal=1,
        )
        for dataset, phase in zip(datasets, ("DISCOVERY", "VALIDATION", "SEALED"), strict=True)
    )
    episode = AlphaEvaluationEpisode(
        program_id=program.id,
        branch_id=branch.id,
        alpha_model_version_id=model.id,
        assignment_id=assignment.id,
        discovery_run_ids=[],
        validation_run_ids=[],
        sealed_dataset_revision_id=datasets[2].id,
        promotion_policy_version_id=policy.id,
        state="EVALUATED",
        result="PASS",
        gate_results={},
        multiple_testing_summary={},
        disclosure={},
        sealed_at=now,
        evaluated_at=now,
    )
    session.add(episode)
    session.flush()
    result = AlphaEvaluationResult(
        episode_id=episode.id,
        evidence_validity="VALID",
        result="PASS",
        private_result_ref=uuid4(),
        evaluated_at=now,
    )
    session.add(result)
    session.flush()
    signal = AlphaSignalArtifact(
        alpha_model_version_id=model.id,
        dataset_revision_id=datasets[2].id,
        evaluation_result_id=result.id,
        mode="CALIBRATED_RETURN",
        artifact_uri="artifact://alpha-signal",
        row_count=1,
        event_start=now,
        event_end=now,
        available_start=now,
        available_end=now,
        schema_version="1",
    )
    qualification = AlphaQualification(
        program_id=program.id,
        alpha_model_id=alpha.id,
        alpha_model_version_id=model.id,
        calibration_version_id=calibration.id,
        universe_version_id=universe.id,
        universe=universe.name,
        horizon="1D",
        role="PRIMARY_ALPHA",
        state="ACTIVE",
        name=alpha.name,
        scope_json={},
        evaluation_episode_id=episode.id,
        evaluation_result_id=result.id,
        degradation_state="HEALTHY",
        qualification_metrics={},
        lineage=[],
    )
    session.add_all((signal, qualification))
    session.flush()
    forecast = AlphaEvaluationForecast(
        result_id=result.id,
        signal_artifact_id=signal.id,
        instrument_id="US:TEST",
        as_of_time=now,
        effective_from=now,
        effective_until=now + timedelta(days=1),
        expected_return=Decimal("0.02"),
        uncertainty=Decimal("0.01"),
        confidence=Decimal("0.8"),
        max_trade_notional=Decimal("10000"),
        max_position_notional=Decimal("50000"),
        max_participation_rate=Decimal("0.1"),
        days_to_liquidate=Decimal("2"),
        stressed_capacity_notional=Decimal("20000"),
    )
    session.add_all(
        (
            forecast,
            AlphaEvaluationMetric(
                result_id=result.id,
                metric_code="NET_EDGE",
                phase="SEALED",
                value=Decimal("0.02"),
                status="AVAILABLE",
            ),
            AlphaEvaluationGate(
                result_id=result.id,
                gate_code="NET_EDGE",
                status="PASS",
                reason_code=None,
            ),
            EvidenceExposure(
                episode_id=episode.id,
                subject_type="ALPHA_QUALIFICATION",
                subject_id=qualification.id,
                level=1,
            ),
            Disclosure(
                episode_id=episode.id,
                audience="CODEX",
                level=1,
                classification_code="QUALIFIED",
            ),
        )
    )
    return {
        "assignment": assignment,
        "calibration": calibration,
        "discovery": discovery,
        "datasets": datasets,
        "design": design,
        "selection": selection,
        "episode": episode,
        "model": model,
        "mission": mission,
        "qualification": qualification,
        "result": result,
        "signal": signal,
        "forecast": forecast,
    }


def test_typed_alpha_assignment_result_and_exposure_persist(engine) -> None:
    with Session(engine) as session:
        facts = _seed_assignment(session)
        session.commit()

        assignment = session.get(AlphaEvaluationAssignment, facts["assignment"].id)
        assert assignment is not None
        assert assignment.source_mission_artifact_revision == 1
        assert assignment.discovery_evaluation_id == facts["discovery"].id
        discovery = session.get(AlphaDiscoveryEvaluation, facts["discovery"].id)
        assert discovery is not None
        assert discovery.state == "VALID"
        assert discovery.private_result_ref is not None
        assert discovery.evaluated_at is not None
        assert session.get(
            AlphaDiscoveryEvaluationMetric,
            (discovery.id, "IC_MEAN"),
        ).value == Decimal("0.02000000")
        assert (
            session.get(
                AlphaDiscoveryEvaluationGate,
                (discovery.id, "EVIDENCE_VALID"),
            ).status
            == "PASS"
        )
        calibration = session.get(AlphaCalibrationVersion, facts["calibration"].id)
        assert calibration is not None
        assert calibration.source_discovery_evaluation_id == discovery.id
        assert calibration.training_dataset_revision_id == discovery.discovery_dataset_revision_id
        assert calibration.private_artifact_ref is not None
        assert calibration.artifact_uri is None
        assert (
            session.scalar(
                select(AlphaEvaluationAssignmentDatasetRevision).where(
                    AlphaEvaluationAssignmentDatasetRevision.assignment_id == assignment.id
                )
            )
            is not None
        )
        assert (
            session.get(AlphaEvaluationEpisode, facts["episode"].id).assignment_id == assignment.id
        )
        assert (
            session.get(AlphaQualification, facts["qualification"].id).evaluation_result_id
            == facts["result"].id
        )
        assert session.get(AlphaSignalArtifact, facts["signal"].id).run_id is None
        forecast = session.get(
            AlphaEvaluationForecast,
            (facts["result"].id, facts["forecast"].instrument_id),
        )
        assert forecast is not None
        assert forecast.signal_artifact_id == facts["signal"].id
        assert forecast.effective_until is not None
        assert session.scalar(
            select(AlphaEvaluationMetric.value).where(
                AlphaEvaluationMetric.result_id == facts["result"].id
            )
        ) == Decimal("0.02000000")


def test_only_one_enabled_selection_per_universe(engine) -> None:
    with Session(engine) as session:
        facts = _seed_assignment(session)
        assignment = facts["assignment"]
        selection = session.scalar(
            select(EvaluationDatasetSelection).where(
                EvaluationDatasetSelection.universe_version_id == assignment.universe_version_id
            )
        )
        assert selection is not None
        duplicate = EvaluationDatasetSelection(
            universe_version_id=selection.universe_version_id,
            version_no=2,
            discovery_dataset_revision_id=selection.discovery_dataset_revision_id,
            validation_dataset_revision_id=selection.validation_dataset_revision_id,
            sealed_dataset_revision_id=selection.sealed_dataset_revision_id,
            state="ENABLED",
        )
        session.add(duplicate)
        with pytest.raises(IntegrityError):
            session.flush()


def test_mission_accepts_awaiting_validation(engine) -> None:
    with Session(engine) as session:
        facts = _seed_assignment(session)
        mission = session.get(ResearchMission, facts["mission"].id)
        assert mission is not None
        mission.state = "AWAITING_VALIDATION"
        session.flush()


def test_constraints_reject_incoherent_trusted_facts(engine) -> None:
    with Session(engine) as session:
        facts = _seed_assignment(session)
        facts["model"].source_mission_artifact_revision = None
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        discovery = facts["discovery"]
        event = Event(
            kind="SECOND_DISCOVERY_ATTEMPT",
            aggregate_type="MISSION",
            aggregate_id=discovery.mission_id,
        )
        session.add(event)
        session.flush()
        session.add(
            AlphaDiscoveryEvaluation(
                source_mission_artifact_id=discovery.source_mission_artifact_id,
                source_mission_artifact_revision=discovery.source_mission_artifact_revision,
                alpha_model_version_id=discovery.alpha_model_version_id,
                program_id=discovery.program_id,
                cycle_id=discovery.cycle_id,
                branch_id=discovery.branch_id,
                mission_id=discovery.mission_id,
                discovery_dataset_revision_id=discovery.discovery_dataset_revision_id,
                cause_event_id=event.id,
                evaluator_contract_version="1",
                state="VALID",
                completed_at=datetime.now(UTC),
            )
        )
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        facts["discovery"].evaluated_at = None
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        facts["discovery"].evaluation_design_version_id = None
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        session.add(
            AlphaDiscoveryEvaluationMetric(
                discovery_evaluation_id=facts["discovery"].id,
                metric_code="RAW_RETURN_SERIES",
                value=Decimal("0.01"),
                status="AVAILABLE",
            )
        )
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        session.add(
            AlphaDiscoveryEvaluationGate(
                discovery_evaluation_id=facts["discovery"].id,
                gate_code="RAW_DATA_VALID",
                status="PASS",
                reason_code=None,
            )
        )
        with pytest.raises(IntegrityError):
            session.flush()

    with engine.connect() as connection:
        if connection.dialect.name == "sqlite":
            connection.exec_driver_sql("PRAGMA foreign_keys = ON")

    with Session(engine) as session:
        facts = _seed_assignment(session)
        design = facts["design"]
        session.add(
            EvaluationDesignVersion(
                version_no=2,
                universe_version_id=design.universe_version_id,
                contract_version=design.contract_version,
                allowed_model_mode=design.allowed_model_mode,
                qualification_role=design.qualification_role,
                walk_forward_folds=design.walk_forward_folds,
                annualization_factor=design.annualization_factor,
                multiple_testing_method=design.multiple_testing_method,
                multiple_testing_max_trials=design.multiple_testing_max_trials,
                qualification_metric_code=design.qualification_metric_code,
                qualification_comparator=design.qualification_comparator,
                qualification_threshold=design.qualification_threshold,
                pass_disclosure_code=design.pass_disclosure_code,
                failure_disclosure_code=design.failure_disclosure_code,
                inconclusive_disclosure_code=design.inconclusive_disclosure_code,
                invalid_disclosure_code=design.invalid_disclosure_code,
                state="ACTIVE",
            )
        )
        session.flush()
        alternative_design = session.scalar(
            select(EvaluationDesignVersion).where(EvaluationDesignVersion.version_no == 2)
        )
        assert alternative_design is not None
        facts["assignment"].evaluation_design_version_id = alternative_design.id
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        facts["calibration"].training_dataset_revision_id = facts["datasets"][1].id
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        model = facts["model"]
        alternative_model = AlphaModelVersion(
            alpha_model_id=model.alpha_model_id,
            version_no=2,
            source_mission_id=model.source_mission_id,
            source_mission_artifact_id=model.source_mission_artifact_id,
            source_mission_artifact_revision=model.source_mission_artifact_revision,
            universe_version_id=model.universe_version_id,
            horizon=model.horizon,
            mode=model.mode,
            artifact_uri="artifact://alternate-alpha-model",
            entrypoint="alpha:alternate",
            parameters={},
            input_contract={},
            output_contract={},
            state="VALIDATED",
        )
        session.add(alternative_model)
        session.flush()
        session.add(
            AlphaCalibrationVersion(
                alpha_model_version_id=alternative_model.id,
                version_no=1,
                method="ISOTONIC",
                training_dataset_revision_ids=[],
                artifact_uri=None,
                source_discovery_evaluation_id=facts["discovery"].id,
                training_dataset_revision_id=facts["datasets"][0].id,
                private_artifact_ref=uuid4(),
                parameters={},
                metrics={},
                state="VALIDATED",
            )
        )
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        facts["calibration"].artifact_uri = "artifact://not-private"
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        facts["calibration"].method = ""
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        facts["forecast"].instrument_id = " "
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        facts["forecast"].max_participation_rate = Decimal("1.1")
        with pytest.raises(IntegrityError):
            session.flush()


@pytest.mark.parametrize("value", (Decimal("NaN"), Decimal("Infinity"), Decimal("-Infinity")))
def test_discovery_metric_rejects_nonfinite_database_values(engine, value: Decimal) -> None:
    with Session(engine) as session:
        facts = _seed_assignment(session)
        session.add(
            AlphaDiscoveryEvaluationMetric(
                discovery_evaluation_id=facts["discovery"].id,
                metric_code="NET_RETURN",
                value=value,
                status="AVAILABLE",
            )
        )
        with pytest.raises((IntegrityError, DataError)):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        session.add(
            AlphaEvaluationGate(
                result_id=facts["result"].id,
                gate_code="MISSING_REASON",
                status="FAIL",
            )
        )
        with pytest.raises(IntegrityError):
            session.flush()

    with Session(engine) as session:
        facts = _seed_assignment(session)
        session.add(
            Disclosure(
                episode_id=facts["episode"].id,
                audience="OPERATOR",
                level=2,
                classification_code="REJECTED",
            )
        )
        with pytest.raises(IntegrityError):
            session.flush()


def test_0023_upgrade_downgrade_preserves_legacy_episode(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    database_url = f"sqlite+pysqlite:///{tmp_path / 'migration.sqlite'}"
    monkeypatch.setenv("QUAZONAI_ENV", "test")
    monkeypatch.setenv("QUAZONAI_DATABASE_URL", database_url)
    monkeypatch.setenv("QUAZONAI_ALEMBIC_URL", database_url)
    config = Config(str(_BACKEND_ROOT / "alembic.ini"))
    command.upgrade(config, "0022_downstream_preflight")

    engine = create_engine(database_url)
    try:
        with engine.begin() as connection:
            episodes = Table("alpha_evaluation_episodes", MetaData(), autoload_with=connection)
            legacy_id = uuid4()
            connection.execute(
                episodes.insert().values(
                    id=str(legacy_id),
                    program_id=str(uuid4()),
                    branch_id=str(uuid4()),
                    alpha_model_version_id=str(uuid4()),
                    discovery_run_ids=[],
                    validation_run_ids=[],
                    sealed_dataset_revision_id=str(uuid4()),
                    promotion_policy_version_id=str(uuid4()),
                    state="PENDING",
                    result=None,
                    gate_results={},
                    multiple_testing_summary={},
                    disclosure={},
                    created_at=datetime.now(UTC),
                    updated_at=datetime.now(UTC),
                )
            )
            context = MigrationContext.configure(connection)
            with Operations.context(context):
                _migration().upgrade()

            migrated = Table("alpha_evaluation_episodes", MetaData(), autoload_with=connection)
            row = connection.execute(
                select(migrated.c.id, migrated.c.assignment_id, migrated.c.state).where(
                    migrated.c.id == str(legacy_id)
                )
            ).one()
            assert (UUID(str(row.id)), row.assignment_id, row.state) == (legacy_id, None, "PENDING")
            tables = set(inspect(connection).get_table_names())
            assert {
                "evaluation_dataset_selections",
                "evaluation_design_versions",
                "alpha_discovery_evaluations",
                "alpha_evaluation_assignments",
                "alpha_evaluation_assignment_dataset_revisions",
                "alpha_evaluation_results",
                "alpha_evaluation_metrics",
                "alpha_evaluation_gates",
                "evidence_exposures",
                "disclosures",
                "promotion_policy_versions",
                "promotion_policy_gates",
            } <= tables
            assert any(
                "AWAITING_VALIDATION" in check.get("sqltext", "")
                for check in inspect(connection).get_check_constraints("research_missions")
            )
            assert "reason_code" in {
                column["name"] for column in inspect(connection).get_columns("disclosures")
            }

            with Operations.context(context):
                _migration().downgrade()
            downgraded = Table("alpha_evaluation_episodes", MetaData(), autoload_with=connection)
            assert "assignment_id" not in downgraded.c
            assert (
                connection.execute(
                    select(downgraded.c.state).where(downgraded.c.id == str(legacy_id))
                ).scalar_one()
                == "PENDING"
            )

            with Operations.context(context):
                _migration().upgrade()
            reupgraded = Table("alpha_evaluation_episodes", MetaData(), autoload_with=connection)
            assert (
                connection.execute(
                    select(reupgraded.c.assignment_id).where(reupgraded.c.id == str(legacy_id))
                ).scalar_one()
                is None
            )
            policies = Table("promotion_policy_versions", MetaData(), autoload_with=connection)
            connection.execute(
                policies.insert().values(
                    id=str(uuid4()),
                    version_no=1,
                    purpose="SEALED_TO_QUALIFIED",
                    mode="MANUAL_APPROVAL",
                    paper_downstream_system_id=None,
                    live_downstream_system_id=None,
                    state="ACTIVE",
                    created_at=datetime.now(UTC),
                )
            )
            with pytest.raises(RuntimeError, match="TRUSTED_ALPHA_EVALUATION_DOWNGRADE_BLOCKED"):
                with Operations.context(context):
                    _migration().downgrade()
    finally:
        engine.dispose()


def test_0025_upgrade_downgrade_preserves_legacy_calibration_and_blocks_trusted_erasure(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    database_url = f"sqlite+pysqlite:///{tmp_path / 'migration-0025.sqlite'}"
    monkeypatch.setenv("QUAZONAI_ENV", "test")
    monkeypatch.setenv("QUAZONAI_DATABASE_URL", database_url)
    monkeypatch.setenv("QUAZONAI_ALEMBIC_URL", database_url)
    config = Config(str(_BACKEND_ROOT / "alembic.ini"))
    command.upgrade(config, "0024_typed_portfolio_configuration")

    engine = create_engine(database_url)
    try:
        with engine.begin() as connection:
            calibrations = Table("alpha_calibration_versions", MetaData(), autoload_with=connection)
            legacy_id = uuid4()
            connection.execute(
                calibrations.insert().values(
                    id=str(legacy_id),
                    alpha_model_version_id=str(uuid4()),
                    version_no=1,
                    method="ISOTONIC",
                    training_dataset_revision_ids=[],
                    artifact_uri="artifact://legacy-calibration",
                    parameters={},
                    metrics={},
                    state="VALIDATED",
                    created_at=datetime.now(UTC),
                )
            )
            context = MigrationContext.configure(connection)
            with Operations.context(context):
                _migration_0025().upgrade()

            migrated = Table("alpha_calibration_versions", MetaData(), autoload_with=connection)
            row = connection.execute(
                select(
                    migrated.c.artifact_uri,
                    migrated.c.source_discovery_evaluation_id,
                    migrated.c.training_dataset_revision_id,
                    migrated.c.private_artifact_ref,
                ).where(migrated.c.id == str(legacy_id))
            ).one()
            assert tuple(row) == ("artifact://legacy-calibration", None, None, None)
            tables = set(inspect(connection).get_table_names())
            assert {
                "alpha_discovery_evaluation_metrics",
                "alpha_discovery_evaluation_gates",
                "alpha_evaluation_forecasts",
            } <= tables
            discovery_columns = {
                column["name"]: column
                for column in inspect(connection).get_columns("alpha_discovery_evaluations")
            }
            assert {
                "private_result_ref",
                "evaluated_at",
                "evaluation_dataset_selection_id",
                "evaluation_design_version_id",
            } <= set(discovery_columns)
            assert not discovery_columns["evaluation_dataset_selection_id"]["nullable"]
            assert not discovery_columns["evaluation_design_version_id"]["nullable"]
            discovery_fks = inspect(connection).get_foreign_keys("alpha_discovery_evaluations")
            assert any(
                foreign_key["constrained_columns"]
                == ["evaluation_dataset_selection_id", "discovery_dataset_revision_id"]
                for foreign_key in discovery_fks
            )
            forecast_columns = {
                column["name"]
                for column in inspect(connection).get_columns("alpha_evaluation_forecasts")
            }
            assert {
                "result_id",
                "signal_artifact_id",
                "instrument_id",
                "as_of_time",
                "effective_from",
                "effective_until",
                "expected_return",
                "uncertainty",
                "confidence",
                "max_trade_notional",
                "max_position_notional",
                "max_participation_rate",
                "days_to_liquidate",
                "stressed_capacity_notional",
            } <= forecast_columns
            forecast_fks = inspect(connection).get_foreign_keys("alpha_evaluation_forecasts")
            assert any(
                foreign_key["constrained_columns"] == ["signal_artifact_id", "result_id"]
                for foreign_key in forecast_fks
            )

            with Operations.context(context):
                _migration_0025().downgrade()
            downgraded = Table("alpha_calibration_versions", MetaData(), autoload_with=connection)
            assert "private_artifact_ref" not in downgraded.c
            assert (
                connection.execute(
                    select(downgraded.c.artifact_uri).where(downgraded.c.id == str(legacy_id))
                ).scalar_one()
                == "artifact://legacy-calibration"
            )

            with Operations.context(context):
                _migration_0025().upgrade()
            discovery = Table("alpha_discovery_evaluations", MetaData(), autoload_with=connection)
            now = datetime.now(UTC)
            connection.execute(
                discovery.insert().values(
                    id=str(uuid4()),
                    source_mission_artifact_id=str(uuid4()),
                    source_mission_artifact_revision=1,
                    alpha_model_version_id=str(uuid4()),
                    program_id=str(uuid4()),
                    cycle_id=str(uuid4()),
                    branch_id=str(uuid4()),
                    mission_id=str(uuid4()),
                    discovery_dataset_revision_id=str(uuid4()),
                    evaluation_dataset_selection_id=str(uuid4()),
                    evaluation_design_version_id=str(uuid4()),
                    cause_event_id=1,
                    evaluator_contract_version="1",
                    state="FROZEN",
                    outcome_code=None,
                    private_result_ref=None,
                    evaluated_at=None,
                    created_at=now,
                    completed_at=None,
                )
            )
            with pytest.raises(RuntimeError, match="TYPED_DISCOVERY_CALIBRATION_DOWNGRADE_BLOCKED"):
                with Operations.context(context):
                    _migration_0025().downgrade()
    finally:
        engine.dispose()


def test_0025_downgrade_blocks_forecast_erasure(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    database_url = f"sqlite+pysqlite:///{tmp_path / 'migration-forecast-downgrade.sqlite'}"
    monkeypatch.setenv("QUAZONAI_ENV", "test")
    monkeypatch.setenv("QUAZONAI_DATABASE_URL", database_url)
    monkeypatch.setenv("QUAZONAI_ALEMBIC_URL", database_url)
    config = Config(str(_BACKEND_ROOT / "alembic.ini"))
    command.upgrade(config, "0025_typed_discovery_calibration")

    engine = create_engine(database_url)
    try:
        with engine.begin() as connection:
            forecasts = Table("alpha_evaluation_forecasts", MetaData(), autoload_with=connection)
            now = datetime.now(UTC)
            connection.execute(
                forecasts.insert().values(
                    result_id=str(uuid4()),
                    signal_artifact_id=str(uuid4()),
                    instrument_id="US:TEST",
                    as_of_time=now,
                    effective_from=now,
                    effective_until=None,
                    expected_return=Decimal("0.02"),
                    uncertainty=Decimal("0.01"),
                    confidence=Decimal("0.8"),
                    max_trade_notional=Decimal("10000"),
                    max_position_notional=Decimal("50000"),
                    max_participation_rate=Decimal("0.1"),
                    days_to_liquidate=Decimal("2"),
                    stressed_capacity_notional=Decimal("20000"),
                )
            )
            context = MigrationContext.configure(connection)
            with pytest.raises(RuntimeError, match="TYPED_DISCOVERY_CALIBRATION_DOWNGRADE_BLOCKED"):
                with Operations.context(context):
                    _migration_0025().downgrade()
    finally:
        engine.dispose()


@pytest.mark.parametrize(
    ("table", "values", "error_code"),
    [
        (
            "alpha_discovery_evaluations",
            {
                "id": str(uuid4()),
                "source_mission_artifact_id": str(uuid4()),
                "source_mission_artifact_revision": 1,
                "alpha_model_version_id": str(uuid4()),
                "program_id": str(uuid4()),
                "cycle_id": str(uuid4()),
                "branch_id": str(uuid4()),
                "mission_id": str(uuid4()),
                "discovery_dataset_revision_id": str(uuid4()),
                "cause_event_id": 1,
                "evaluator_contract_version": "1",
                "state": "VALID",
                "outcome_code": "VALID",
                "created_at": datetime.now(UTC),
                "completed_at": datetime.now(UTC),
            },
            "TRUSTED_DISCOVERY_RESULT_MIGRATION_BLOCKED",
        ),
        (
            "alpha_discovery_evaluations",
            {
                "id": str(uuid4()),
                "source_mission_artifact_id": str(uuid4()),
                "source_mission_artifact_revision": 1,
                "alpha_model_version_id": str(uuid4()),
                "program_id": str(uuid4()),
                "cycle_id": str(uuid4()),
                "branch_id": str(uuid4()),
                "mission_id": str(uuid4()),
                "discovery_dataset_revision_id": str(uuid4()),
                "cause_event_id": 1,
                "evaluator_contract_version": "1",
                "state": "QUEUED",
                "outcome_code": None,
                "created_at": datetime.now(UTC),
                "completed_at": None,
            },
            "TRUSTED_DISCOVERY_RESULT_MIGRATION_BLOCKED",
        ),
        (
            "alpha_calibration_versions",
            {
                "id": str(uuid4()),
                "alpha_model_version_id": str(uuid4()),
                "version_no": 1,
                "method": "   ",
                "training_dataset_revision_ids": [],
                "artifact_uri": "artifact://legacy-calibration",
                "parameters": {},
                "metrics": {},
                "state": "VALIDATED",
                "created_at": datetime.now(UTC),
            },
            "TRUSTED_CALIBRATION_PROVENANCE_MIGRATION_BLOCKED",
        ),
        (
            "alpha_evaluation_assignments",
            {
                "id": str(uuid4()),
                "source_mission_artifact_id": str(uuid4()),
                "source_mission_artifact_revision": 1,
                "discovery_evaluation_id": str(uuid4()),
                "program_id": str(uuid4()),
                "cycle_id": str(uuid4()),
                "branch_id": str(uuid4()),
                "mission_id": str(uuid4()),
                "alpha_model_version_id": str(uuid4()),
                "alpha_calibration_version_id": None,
                "universe_version_id": str(uuid4()),
                "sealed_dataset_revision_id": str(uuid4()),
                "evaluation_design_version_id": str(uuid4()),
                "promotion_policy_version_id": str(uuid4()),
                "cause_event_id": 1,
                "assignment_no": 1,
                "evaluator_contract_version": "1",
                "state": "FROZEN",
                "created_at": datetime.now(UTC),
            },
            "TRUSTED_ALPHA_ASSIGNMENT_MIGRATION_BLOCKED",
        ),
    ],
)
def test_0025_upgrade_rejects_unmigratable_immutable_facts(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    table: str,
    values: dict[str, object],
    error_code: str,
) -> None:
    database_url = f"sqlite+pysqlite:///{tmp_path / f'migration-guard-{table}.sqlite'}"
    monkeypatch.setenv("QUAZONAI_ENV", "test")
    monkeypatch.setenv("QUAZONAI_DATABASE_URL", database_url)
    monkeypatch.setenv("QUAZONAI_ALEMBIC_URL", database_url)
    config = Config(str(_BACKEND_ROOT / "alembic.ini"))
    command.upgrade(config, "0024_typed_portfolio_configuration")

    engine = create_engine(database_url)
    try:
        with engine.begin() as connection:
            legacy = Table(table, MetaData(), autoload_with=connection)
            connection.execute(legacy.insert().values(**values))
            context = MigrationContext.configure(connection)
            with pytest.raises(RuntimeError, match=error_code):
                with Operations.context(context):
                    _migration_0025().upgrade()
    finally:
        engine.dispose()
