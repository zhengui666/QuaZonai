from __future__ import annotations

from datetime import UTC, datetime, timedelta
from decimal import Decimal
from pathlib import Path
from typing import cast
from uuid import UUID, uuid4

import pytest
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from agent_harness.orchestrator import await_mission_validation, finish_mission
from db.models import (
    AgentSession,
    AlphaCalibrationVersion,
    AlphaDiscoveryEvaluation,
    AlphaDiscoveryEvaluationGate,
    AlphaDiscoveryEvaluationMetric,
    AlphaEvaluationAssignment,
    AlphaEvaluationAssignmentDatasetRevision,
    AlphaEvaluationEpisode,
    AlphaModel,
    AlphaModelVersion,
    DatasetRevision,
    EvaluationDatasetSelection,
    EvaluationDesignVersion,
    FeaturePipelineVersion,
    GovernedDataSource,
    Job,
    MarketUniverseVersion,
    MissionArtifact,
    MissionDependency,
    NautilusCatalogBinding,
    PromotionPolicyGate,
    PromotionPolicyVersion,
    ResearchBranch,
    ResearchCharter,
    ResearchCycle,
    ResearchMission,
    ResearchProgram,
)
from errors import QfError
from research_engine.alpha_intake import (
    accept_discovery_evaluation_result,
    stage_alpha_discovery_evaluation,
)
from research_engine.sealed_evaluator_contracts import (
    CalibrationMethod,
    DiscoveryCalibrationArtifact,
    DiscoveryEvaluationResult,
    DiscoveryEvaluationStatus,
    DisclosureReasonCode,
    EvaluationPhase,
    GateCode,
    GateResult,
    GateStatus,
    ImmutableReference,
    MetricAggregate,
    MetricCode,
    MetricStatus,
)


def _alpha_payload(universe_id: UUID, feature_id: UUID) -> dict[str, object]:
    return {
        "family_key": "mean-reversion",
        "requested_role": "PRIMARY_ALPHA",
        "universe_version_id": str(universe_id),
        "horizon": "1D",
        "feature_pipeline_ref": str(feature_id),
        "source_path": "alphas/mean_reversion.py",
        "entrypoint": "alphas.mean_reversion:build_alpha",
        "parameters": {"lookback": 20},
        "input_contract": {"feature_schema": "FeatureFrameV1"},
        "output_contract": "AlphaSignalFrameV1",
        "hypothesis": "Lagged cross-sectional returns retain bounded predictive information.",
        "falsification_criteria": ["Discovery rank correlation is non-positive."],
        "known_limitations": ["Sealed evaluation remains required."],
    }


def _catalog(
    session: Session,
    dataset: DatasetRevision,
    *,
    sealed: bool,
    uri_suffix: str = "",
) -> None:
    session.add(
        NautilusCatalogBinding(
            dataset_revision_id=dataset.id,
            catalog_uri=f"catalog://{dataset.partition.casefold()}{uri_suffix}",
            provider="fixture-provider",
            source_license="fixture-license",
            nautilus_data_type="QuoteTick",
            instrument_scope=["TEST"],
            event_time_range={},
            available_time_range={},
            schema_revision="v1",
            quality_state="VALID",
            quality_result={},
            point_in_time_state="VALID",
            point_in_time_result={},
            sealed=sealed,
        )
    )


def _seed(
    session: Session,
    tmp_path: Path,
    *,
    design_mode: str = "RELATIVE_SCORE",
) -> dict[str, object]:
    now = datetime.now(UTC)
    universe = MarketUniverseVersion(
        universe_key="TEST_UNIVERSE",
        version_no=1,
        name="Test Universe",
        state="ACTIVE",
        spec_json={},
        created_at=now,
    )
    session.add(universe)
    session.flush()
    charter = ResearchCharter(
        original_idea_text="Test one bounded Alpha proposal.",
        research_question="Can one bounded model progress only after independent evidence?",
        market_scope=[],
        universe_version_ids=[str(universe.id)],
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
    program = ResearchProgram(charter_id=charter.id, title="Alpha intake", state="ACTIVE", revision=1)
    session.add(program)
    session.flush()
    cycle = ResearchCycle(
        program_id=program.id,
        cycle_no=1,
        trigger="IDEA_START",
        state="RUNNING",
        mission_budget=3,
        replan_budget=0,
        runtime_configuration_revision=1,
        started_at=now,
        summary={},
        created_at=now,
    )
    session.add(cycle)
    session.flush()
    branch = ResearchBranch(
        program_id=program.id,
        cycle_id=cycle.id,
        derivation_type="ROOT",
        hypothesis="One bounded mean reversion model.",
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
        cycle_id=cycle.id,
        branch_id=branch.id,
        mission_type="ALPHA_DISCOVERY",
        role_profile="ALPHA_RESEARCHER",
        state="RUNNING",
        objective="Produce one typed Alpha proposal.",
        contract_version="v1",
        input_snapshot={
            "charter": {
                "universe_version_ids": [str(universe.id)],
                "prediction_horizon": "1D",
            },
            "branch": {"branch_id": str(branch.id)},
        },
        capability_snapshot={},
        runtime_snapshot={},
        prompt_version="v1",
        max_turns=1,
        max_tool_calls=0,
        started_at=now,
        attempt=1,
        revision=1,
    )
    dependent = ResearchMission(
        program_id=program.id,
        cycle_id=cycle.id,
        branch_id=branch.id,
        mission_type="ROBUSTNESS",
        role_profile="ROBUSTNESS_VALIDATOR",
        state="PLANNED",
        objective="Wait for trusted Alpha evidence.",
        contract_version="v1",
        input_snapshot={},
        capability_snapshot={},
        runtime_snapshot={},
        prompt_version="v1",
        max_turns=1,
        max_tool_calls=0,
        attempt=1,
        revision=1,
    )
    session.add_all((mission, dependent))
    session.flush()
    session.add(
        MissionDependency(
            mission_id=dependent.id,
            depends_on_mission_id=mission.id,
            required_outcome="SUCCEEDED",
        )
    )
    session.add(
        AgentSession(
            mission_id=mission.id,
            role_profile="ALPHA_RESEARCHER",
            codex_thread_id="alpha-intake-thread",
            codex_version="test",
            state="RUNNING",
            started_at=now,
            last_event_at=now,
        )
    )
    source = GovernedDataSource(
        name="Alpha intake source",
        connector_key="fixture",
        state="ACTIVE",
        preflight_state="READY",
        universe_scope=[],
        fields=[],
        field_schema={},
        availability_semantics={},
        public_config={},
    )
    session.add(source)
    session.flush()
    datasets: list[DatasetRevision] = []
    for revision_no, partition in enumerate(("DISCOVERY", "VALIDATION", "SEALED"), start=1):
        dataset = DatasetRevision(
            data_source_id=source.id,
            universe_version_id=universe.id,
            universe_name=universe.name,
            revision_no=revision_no,
            data_class="VENDOR",
            origin="fixture",
            ingested_at=now,
            promotability="PROMOTABLE",
            schema_version="v1",
            event_start=now,
            event_end=now + timedelta(days=1),
            available_start=now,
            available_end=now + timedelta(days=1),
            row_count=10,
            quality_state="VALID",
            point_in_time_state="VALID",
            partition=partition,
            materialization_request={},
            created_at=now,
        )
        session.add(dataset)
        session.flush()
        _catalog(session, dataset, sealed=partition == "SEALED")
        datasets.append(dataset)
    feature = FeaturePipelineVersion(
        pipeline_key="alpha-input-features",
        version_no=1,
        universe_version_id=universe.id,
        artifact_uri="artifact://features/alpha-input-features",
        input_schema={},
        output_schema={},
        point_in_time_policy_version_id=UUID("00000000-0000-0000-0000-000000000001"),
        created_at=now,
    )
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
        contract_version="evaluation-design-v1",
        allowed_model_mode=design_mode,
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
        paper_downstream_system_id=None,
        live_downstream_system_id=None,
        state="ACTIVE",
    )
    session.add_all((feature, selection, design, policy))
    session.flush()
    session.add(
        PromotionPolicyGate(
            policy_version_id=policy.id,
            metric_code="NET_RETURN",
            comparator="MINIMUM",
            threshold=Decimal("0.01"),
            ordinal=1,
        )
    )
    artifact = MissionArtifact(
        mission_id=mission.id,
        kind="ALPHA_PROPOSAL",
        schema_version="v1",
        revision=1,
        state="DRAFT",
        storage_uri="db://mission-artifacts/draft",
        metadata_json={
            "summary": "One bounded candidate; independent evaluation remains required.",
            "payload": _alpha_payload(universe.id, feature.id),
        },
        created_at=now,
    )
    session.add(artifact)
    session.flush()
    workspace = tmp_path / "worktree"
    source_path = workspace / "alphas" / "mean_reversion.py"
    source_path.parent.mkdir(parents=True)
    source_path.write_text("def build_alpha():\n    return None\n", encoding="utf-8")
    return {
        "artifact_id": artifact.id,
        "design_id": design.id,
        "dependent_id": dependent.id,
        "dataset_ids": tuple(dataset.id for dataset in datasets),
        "mission_id": mission.id,
        "policy_id": policy.id,
        "selection_id": selection.id,
        "source_id": source.id,
        "universe_id": universe.id,
        "workspace": workspace,
    }


def _stage(session: Session, facts: dict[str, object], tmp_path: Path) -> UUID:
    result = stage_alpha_discovery_evaluation(
        session,
        mission_id=facts["mission_id"],  # type: ignore[arg-type]
        workspace=facts["workspace"],  # type: ignore[arg-type]
        artifact_root=tmp_path / "qz-artifacts",
    )
    assert result.accepted and result.discovery_evaluation_id is not None
    await_mission_validation(session, facts["mission_id"])  # type: ignore[arg-type]
    return result.discovery_evaluation_id


def _discovery_result(
    session: Session,
    discovery_id: UUID,
    *,
    status: DiscoveryEvaluationStatus = DiscoveryEvaluationStatus.VALID,
    with_calibration: bool = False,
) -> DiscoveryEvaluationResult:
    discovery = session.get(AlphaDiscoveryEvaluation, discovery_id)
    assert discovery is not None
    version = session.get(AlphaModelVersion, discovery.alpha_model_version_id)
    dataset = session.get(DatasetRevision, discovery.discovery_dataset_revision_id)
    assert version is not None and dataset is not None
    metrics = (
        MetricAggregate(
            EvaluationPhase.DISCOVERY,
            MetricCode.ANNUALIZED_VOLATILITY,
            MetricStatus.AVAILABLE,
            0.1,
        ),
        MetricAggregate(
            EvaluationPhase.DISCOVERY,
            MetricCode.COVERAGE,
            MetricStatus.AVAILABLE,
            0.9,
        ),
        MetricAggregate(
            EvaluationPhase.DISCOVERY,
            MetricCode.HIT_RATE,
            MetricStatus.AVAILABLE,
            0.55,
        ),
        MetricAggregate(
            EvaluationPhase.DISCOVERY,
            MetricCode.IC_MEAN,
            MetricStatus.AVAILABLE,
            0.02,
        ),
        MetricAggregate(
            EvaluationPhase.DISCOVERY,
            MetricCode.MAX_DRAWDOWN,
            MetricStatus.AVAILABLE,
            0.1,
        ),
        MetricAggregate(
            EvaluationPhase.DISCOVERY,
            MetricCode.NET_RETURN,
            MetricStatus.AVAILABLE,
            0.020000009,
        ),
        MetricAggregate(
            EvaluationPhase.DISCOVERY,
            MetricCode.OBSERVATION_COUNT,
            MetricStatus.AVAILABLE,
            100.0,
        ),
        MetricAggregate(
            EvaluationPhase.DISCOVERY,
            MetricCode.RANK_IC_MEAN,
            MetricStatus.AVAILABLE,
            0.02,
        ),
        MetricAggregate(
            EvaluationPhase.DISCOVERY,
            MetricCode.SHARPE_RATIO,
            MetricStatus.AVAILABLE,
            1.2,
        ),
        MetricAggregate(
            EvaluationPhase.DISCOVERY,
            MetricCode.TRIAL_ADJUSTED_SHARPE,
            MetricStatus.NOT_AVAILABLE,
            None,
        ),
    )
    gates = (
        GateResult(GateCode.CALIBRATION_VALID, GateStatus.PASS),
        GateResult(
            GateCode.EVIDENCE_VALID,
            (
                GateStatus.INCONCLUSIVE
                if status is DiscoveryEvaluationStatus.INCONCLUSIVE
                else GateStatus.INVALID if status is DiscoveryEvaluationStatus.INVALID else GateStatus.PASS
            ),
            (
                DisclosureReasonCode.EVIDENCE_INCOMPLETE
                if status is DiscoveryEvaluationStatus.INCONCLUSIVE
                else (
                    DisclosureReasonCode.INVALID_EVALUATOR_RESULT
                    if status is DiscoveryEvaluationStatus.INVALID
                    else None
                )
            ),
        ),
        GateResult(GateCode.POINT_IN_TIME_VALID, GateStatus.PASS),
        GateResult(GateCode.POLICY_VALID, GateStatus.PASS),
        GateResult(GateCode.STATISTICAL_VALID, GateStatus.PASS),
    )
    calibration = (
        DiscoveryCalibrationArtifact(
            method=CalibrationMethod.ISOTONIC,
            training_dataset=ImmutableReference(dataset.id, dataset.revision_no),
            private_artifact_ref=uuid4(),
        )
        if with_calibration
        else None
    )
    return DiscoveryEvaluationResult(
        discovery_evaluation_id=discovery.id,
        model_version=ImmutableReference(version.id, version.version_no),
        status=status,
        private_result_id=uuid4(),
        evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
        metrics=metrics,
        gates=gates,
        calibration=calibration,
    )


def _replace_active_evaluation_context(
    session: Session,
    facts: dict[str, object],
) -> tuple[UUID, UUID, UUID, UUID]:
    source = session.get(GovernedDataSource, facts["source_id"])
    universe = session.get(MarketUniverseVersion, facts["universe_id"])
    selection = session.get(EvaluationDatasetSelection, facts["selection_id"])
    design = session.get(EvaluationDesignVersion, facts["design_id"])
    assert source is not None and universe is not None and selection is not None and design is not None
    selection.state = "RETIRED"
    design.state = "RETIRED"
    session.flush()
    now = datetime.now(UTC)
    datasets: list[DatasetRevision] = []
    for revision_no, partition in enumerate(("DISCOVERY", "VALIDATION", "SEALED"), start=11):
        dataset = DatasetRevision(
            data_source_id=source.id,
            universe_version_id=universe.id,
            universe_name=universe.name,
            revision_no=revision_no,
            data_class="VENDOR",
            origin="replacement-fixture",
            ingested_at=now,
            promotability="PROMOTABLE",
            schema_version="v1",
            event_start=now,
            event_end=now + timedelta(days=1),
            available_start=now,
            available_end=now + timedelta(days=1),
            row_count=10,
            quality_state="VALID",
            point_in_time_state="VALID",
            partition=partition,
            materialization_request={},
            created_at=now,
        )
        session.add(dataset)
        session.flush()
        _catalog(
            session,
            dataset,
            sealed=partition == "SEALED",
            uri_suffix="-replacement",
        )
        datasets.append(dataset)
    replacement_selection = EvaluationDatasetSelection(
        universe_version_id=universe.id,
        version_no=2,
        discovery_dataset_revision_id=datasets[0].id,
        validation_dataset_revision_id=datasets[1].id,
        sealed_dataset_revision_id=datasets[2].id,
        state="ENABLED",
    )
    replacement_design = EvaluationDesignVersion(
        version_no=2,
        universe_version_id=universe.id,
        contract_version="evaluation-design-v2",
        allowed_model_mode="RELATIVE_SCORE",
        qualification_role="PRIMARY_ALPHA",
        walk_forward_folds=3,
        annualization_factor=Decimal("252"),
        multiple_testing_method="BONFERRONI",
        multiple_testing_max_trials=2,
        qualification_metric_code="NET_EDGE",
        qualification_comparator="MINIMUM",
        qualification_threshold=Decimal("0.02"),
        pass_disclosure_code="QUALIFIED",
        failure_disclosure_code="INSUFFICIENT_NET_EDGE",
        inconclusive_disclosure_code="INCONCLUSIVE",
        invalid_disclosure_code="DATA_QUALITY_FAILURE",
        state="ACTIVE",
    )
    session.add_all((replacement_selection, replacement_design))
    session.flush()
    return (
        replacement_selection.id,
        replacement_design.id,
        datasets[0].id,
        datasets[2].id,
    )


def test_alpha_proposal_intake_materializes_and_awaits_core_evidence(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        discovery_id = _stage(session, facts, tmp_path)

    with Session(engine) as session:
        mission = session.get(ResearchMission, facts["mission_id"])
        artifact = session.get(MissionArtifact, facts["artifact_id"])
        discovery = session.get(AlphaDiscoveryEvaluation, discovery_id)
        model = session.scalar(select(AlphaModel))
        version = session.scalar(select(AlphaModelVersion))
        agent_session = session.scalar(select(AgentSession))
        job = session.scalar(
            select(Job).where(
                Job.kind == "DISCOVERY_EVALUATION",
                Job.resource_type == "alpha_discovery_evaluation",
            )
        )
        assert mission is not None and mission.state == "AWAITING_VALIDATION"
        assert mission.finished_at is None
        assert artifact is not None and artifact.state == "VALIDATED"
        assert artifact.storage_uri.startswith(f"artifact://mission-artifacts/{artifact.id}/")
        assert discovery is not None and discovery.state == "QUEUED"
        assert discovery.evaluation_dataset_selection_id == facts["selection_id"]
        assert discovery.evaluation_design_version_id == facts["design_id"]
        assert model is not None and model.state == "RESEARCHING"
        assert version is not None and version.state == "DRAFT" and version.mode == "RELATIVE_SCORE"
        assert agent_session is not None and agent_session.state == "SUCCEEDED"
        assert job is not None and job.resource_id == discovery_id and job.payload == {}
        copied = list((tmp_path / "qz-artifacts" / "alpha-models" / str(artifact.id)).rglob("*.py"))
        assert len(copied) == 1
        assert copied[0].read_text(encoding="utf-8") == "def build_alpha():\n    return None\n"
        assert session.scalar(select(func.count()).select_from(AlphaEvaluationAssignment)) == 0


def test_alpha_mission_cannot_bypass_validated_artifact_gate(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        with pytest.raises(QfError) as exc_info:
            finish_mission(session, facts["mission_id"], succeeded=True)  # type: ignore[arg-type]
        assert exc_info.value.code == "MISSION_VALIDATED_ARTIFACT_REQUIRED"
        discovery_id = _stage(session, facts, tmp_path)
        with pytest.raises(QfError) as exc_info:
            finish_mission(session, facts["mission_id"], succeeded=True)  # type: ignore[arg-type]
        assert exc_info.value.code == "ALPHA_DISCOVERY_VALIDATION_REQUIRED"
        mission = session.get(ResearchMission, facts["mission_id"])
        dependent = session.get(ResearchMission, facts["dependent_id"])
        discovery = session.get(AlphaDiscoveryEvaluation, discovery_id)
        agent_session = session.scalar(select(AgentSession))
        assert mission is not None and mission.state == "AWAITING_VALIDATION"
        assert dependent is not None and dependent.state == "PLANNED"
        assert discovery is not None and discovery.state == "QUEUED"
        assert agent_session is not None and agent_session.state == "SUCCEEDED"


@pytest.mark.parametrize(
    ("status", "expected_error"),
    (
        (DiscoveryEvaluationStatus.INVALID, "DISCOVERY_EVIDENCE_INVALID"),
        (DiscoveryEvaluationStatus.INCONCLUSIVE, "DISCOVERY_EVIDENCE_INCONCLUSIVE"),
    ),
)
def test_typed_non_valid_discovery_result_fails_mission_without_unlocking(
    engine,
    tmp_path: Path,
    status: DiscoveryEvaluationStatus,
    expected_error: str,
) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        discovery_id = _stage(session, facts, tmp_path)

    with Session(engine) as session, session.begin():
        result = _discovery_result(session, discovery_id, status=status)
        assert accept_discovery_evaluation_result(session, result) is None

    with Session(engine) as session:
        mission = session.get(ResearchMission, facts["mission_id"])
        dependent = session.get(ResearchMission, facts["dependent_id"])
        discovery = session.get(AlphaDiscoveryEvaluation, discovery_id)
        version = session.scalar(select(AlphaModelVersion))
        agent_session = session.scalar(select(AgentSession))
        assert mission is not None and mission.state == "FAILED"
        assert mission.error_code == expected_error
        assert dependent is not None and dependent.state == "PLANNED"
        assert discovery is not None and discovery.state == status.value
        assert discovery.outcome_code == status.value
        assert discovery.private_result_ref == result.private_result_id
        assert discovery.evaluated_at is not None
        assert version is not None and version.state == "REJECTED"
        assert agent_session is not None and agent_session.state == "SUCCEEDED"
        assert (
            session.scalar(
                select(func.count()).select_from(AlphaDiscoveryEvaluationMetric)
            )
            == 10
        )
        assert session.scalar(select(func.count()).select_from(AlphaDiscoveryEvaluationGate)) == 5
        assert session.scalar(select(func.count()).select_from(AlphaEvaluationAssignment)) == 0
        assert (
            session.scalar(select(func.count()).select_from(Job).where(Job.kind == "ALPHA_EVALUATION"))
            == 0
        )

    with Session(engine) as session, session.begin():
        assert accept_discovery_evaluation_result(session, result) is None


def test_typed_valid_discovery_result_creates_the_only_assignment_chain(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        discovery_id = _stage(session, facts, tmp_path)

    with Session(engine) as session, session.begin():
        result = _discovery_result(session, discovery_id, with_calibration=True)
        assignment = accept_discovery_evaluation_result(session, result)
        assert assignment is not None
        assignment_id = assignment.id

    with Session(engine) as session:
        mission = session.get(ResearchMission, facts["mission_id"])
        dependent = session.get(ResearchMission, facts["dependent_id"])
        discovery = session.get(AlphaDiscoveryEvaluation, discovery_id)
        version = session.scalar(select(AlphaModelVersion))
        calibration = session.scalar(select(AlphaCalibrationVersion))
        assignment = session.get(AlphaEvaluationAssignment, assignment_id)
        episode = session.scalar(
            select(AlphaEvaluationEpisode).where(AlphaEvaluationEpisode.assignment_id == assignment_id)
        )
        job = session.scalar(
            select(Job).where(
                Job.kind == "ALPHA_EVALUATION",
                Job.resource_type == "alpha_evaluation_assignment",
            )
        )
        agent_session = session.scalar(select(AgentSession))
        rows = list(
            session.scalars(
                select(AlphaEvaluationAssignmentDatasetRevision).where(
                    AlphaEvaluationAssignmentDatasetRevision.assignment_id == assignment_id
                )
            )
        )
        assert mission is not None and mission.state == "SUCCEEDED"
        assert dependent is not None and dependent.state == "READY"
        assert discovery is not None and discovery.state == "VALID"
        assert discovery.outcome_code == "VALID"
        assert discovery.private_result_ref == result.private_result_id
        assert discovery.evaluated_at is not None
        assert version is not None and version.state == "VALIDATED"
        assert version.mode == "RELATIVE_SCORE"
        assert calibration is not None and result.calibration is not None
        assert calibration.alpha_model_version_id == version.id
        assert calibration.source_discovery_evaluation_id == discovery.id
        assert calibration.training_dataset_revision_id == discovery.discovery_dataset_revision_id
        assert calibration.private_artifact_ref == result.calibration.private_artifact_ref
        assert calibration.training_dataset_revision_ids == []
        assert calibration.parameters == {}
        assert calibration.metrics == {}
        assert assignment is not None and assignment.state == "QUEUED"
        assert assignment.discovery_evaluation_id == discovery.id
        assert assignment.alpha_calibration_version_id == calibration.id
        assert assignment.evaluation_design_version_id == facts["design_id"]
        assert assignment.promotion_policy_version_id == facts["policy_id"]
        assert {row.phase: row.dataset_revision_id for row in rows} == {
            "DISCOVERY": facts["dataset_ids"][0],  # type: ignore[index]
            "VALIDATION": facts["dataset_ids"][1],  # type: ignore[index]
            "SEALED": facts["dataset_ids"][2],  # type: ignore[index]
        }
        assert episode is not None and episode.state == "ASSIGNED" and episode.result is None
        assert episode.program_id == assignment.program_id
        assert episode.branch_id == assignment.branch_id
        assert episode.alpha_model_version_id == assignment.alpha_model_version_id
        assert episode.sealed_dataset_revision_id == assignment.sealed_dataset_revision_id
        assert episode.promotion_policy_version_id == assignment.promotion_policy_version_id
        assert episode.discovery_run_ids == [] and episode.validation_run_ids == []
        assert episode.sealed_run_id is None
        assert job is not None and job.resource_id == assignment.id and job.payload == {}
        assert agent_session is not None and agent_session.state == "SUCCEEDED"
        assert session.scalar(select(func.count()).select_from(AlphaDiscoveryEvaluationMetric)) == 10
        assert session.scalar(select(func.count()).select_from(AlphaDiscoveryEvaluationGate)) == 5
        net_return = session.get(
            AlphaDiscoveryEvaluationMetric,
            (discovery_id, MetricCode.NET_RETURN.value),
        )
        assert net_return is not None and net_return.value == Decimal("0.02000001")

    with Session(engine) as session, session.begin():
        repeated = accept_discovery_evaluation_result(session, result)
        assert repeated is not None and repeated.id == assignment_id
        assert session.scalar(select(func.count()).select_from(AlphaEvaluationAssignment)) == 1
        assert (
            session.scalar(select(func.count()).select_from(Job).where(Job.kind == "ALPHA_EVALUATION"))
            == 1
        )


def test_valid_result_without_active_sealed_policy_fails_closed(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        discovery_id = _stage(session, facts, tmp_path)
        policy = session.get(PromotionPolicyVersion, facts["policy_id"])
        assert policy is not None
        policy.state = "RETIRED"

    with Session(engine) as session, session.begin():
        result = _discovery_result(session, discovery_id)
        assert accept_discovery_evaluation_result(session, result) is None

    with Session(engine) as session:
        mission = session.get(ResearchMission, facts["mission_id"])
        dependent = session.get(ResearchMission, facts["dependent_id"])
        discovery = session.get(AlphaDiscoveryEvaluation, discovery_id)
        version = session.scalar(select(AlphaModelVersion))
        assert mission is not None and mission.state == "FAILED"
        assert mission.error_code == "ALPHA_EVALUATION_ASSIGNMENT_UNAVAILABLE"
        assert dependent is not None and dependent.state == "PLANNED"
        assert discovery is not None and discovery.state == "VALID"
        assert discovery.private_result_ref == result.private_result_id
        assert discovery.evaluated_at is not None
        assert version is not None and version.state == "VALIDATED"
        assert session.scalar(select(func.count()).select_from(AlphaEvaluationAssignment)) == 0
        assert (
            session.scalar(select(func.count()).select_from(Job).where(Job.kind == "ALPHA_EVALUATION"))
            == 0
        )

    with Session(engine) as session, session.begin():
        assert accept_discovery_evaluation_result(session, result) is None


def test_valid_result_uses_only_the_frozen_selection_and_design(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        discovery_id = _stage(session, facts, tmp_path)
        replacement_selection_id, replacement_design_id, replacement_discovery_id, replacement_sealed_id = (
            _replace_active_evaluation_context(session, facts)
        )
        result = _discovery_result(session, discovery_id)
        assignment = accept_discovery_evaluation_result(session, result)
        assert assignment is not None
        assignment_id = assignment.id

    with Session(engine) as session:
        discovery = session.get(AlphaDiscoveryEvaluation, discovery_id)
        assignment = session.get(AlphaEvaluationAssignment, assignment_id)
        rows = list(
            session.scalars(
                select(AlphaEvaluationAssignmentDatasetRevision).where(
                    AlphaEvaluationAssignmentDatasetRevision.assignment_id == assignment_id
                )
            )
        )
        assert discovery is not None
        assert discovery.evaluation_dataset_selection_id == facts["selection_id"]
        assert discovery.evaluation_dataset_selection_id != replacement_selection_id
        assert discovery.evaluation_design_version_id == facts["design_id"]
        assert discovery.evaluation_design_version_id != replacement_design_id
        assert assignment is not None and assignment.evaluation_design_version_id == facts["design_id"]
        assert assignment.alpha_calibration_version_id is None
        assert assignment.sealed_dataset_revision_id != replacement_sealed_id
        datasets = {row.phase: row.dataset_revision_id for row in rows}
        assert datasets["DISCOVERY"] != replacement_discovery_id
        assert datasets == {
            "DISCOVERY": facts["dataset_ids"][0],  # type: ignore[index]
            "VALIDATION": facts["dataset_ids"][1],  # type: ignore[index]
            "SEALED": facts["dataset_ids"][2],  # type: ignore[index]
        }


def test_alpha_completion_gate_rejects_mismatched_episode_and_forged_job_payload(
    engine,
    tmp_path: Path,
) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        discovery_id = _stage(session, facts, tmp_path)
        result = _discovery_result(session, discovery_id)
        assignment = accept_discovery_evaluation_result(session, result)
        assert assignment is not None
        assignment_id = assignment.id

    with Session(engine) as session:
        mission = session.get(ResearchMission, facts["mission_id"])
        dependent = session.get(ResearchMission, facts["dependent_id"])
        assignment = session.get(AlphaEvaluationAssignment, assignment_id)
        episode = session.scalar(
            select(AlphaEvaluationEpisode).where(AlphaEvaluationEpisode.assignment_id == assignment_id)
        )
        job = session.scalar(
            select(Job).where(
                Job.kind == "ALPHA_EVALUATION",
                Job.resource_type == "alpha_evaluation_assignment",
                Job.resource_id == assignment_id,
            )
        )
        assert mission is not None and dependent is not None and assignment is not None
        assert episode is not None and job is not None
        mission.state = "AWAITING_VALIDATION"
        mission.outcome = None
        mission.finished_at = None
        episode.sealed_dataset_revision_id = facts["dataset_ids"][1]  # type: ignore[index]
        with pytest.raises(QfError) as exc_info:
            finish_mission(session, mission.id, succeeded=True)
        assert exc_info.value.code == "ALPHA_DISCOVERY_VALIDATION_REQUIRED"
        assert mission.state == "AWAITING_VALIDATION"
        assert dependent.state == "READY"

        episode.sealed_dataset_revision_id = assignment.sealed_dataset_revision_id
        job.payload = {"forged": True}
        with pytest.raises(QfError) as exc_info:
            finish_mission(session, mission.id, succeeded=True)
        assert exc_info.value.code == "ALPHA_DISCOVERY_VALIDATION_REQUIRED"
        assert mission.state == "AWAITING_VALIDATION"
        assert dependent.state == "READY"

        job.payload = {}
        sealed_row = session.get(
            AlphaEvaluationAssignmentDatasetRevision,
            (assignment_id, "SEALED", 1),
        )
        assert sealed_row is not None
        session.delete(sealed_row)
        with pytest.raises(QfError) as exc_info:
            finish_mission(session, mission.id, succeeded=True)
        assert exc_info.value.code == "ALPHA_DISCOVERY_VALIDATION_REQUIRED"
        assert mission.state == "AWAITING_VALIDATION"
        assert dependent.state == "READY"
        session.rollback()


def test_alpha_completion_gate_rejects_a_non_passing_discovery_gate(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        discovery_id = _stage(session, facts, tmp_path)
        result = _discovery_result(session, discovery_id)
        assignment = accept_discovery_evaluation_result(session, result)
        assert assignment is not None

    with Session(engine) as session:
        mission = session.get(ResearchMission, facts["mission_id"])
        dependent = session.get(ResearchMission, facts["dependent_id"])
        gate = session.get(
            AlphaDiscoveryEvaluationGate,
            (discovery_id, GateCode.EVIDENCE_VALID.value),
        )
        assert mission is not None and dependent is not None and gate is not None
        mission.state = "AWAITING_VALIDATION"
        mission.outcome = None
        mission.finished_at = None
        gate.status = "FAIL"
        gate.reason_code = DisclosureReasonCode.POLICY_GATE_FAILURE.value
        with pytest.raises(QfError) as exc_info:
            finish_mission(session, mission.id, succeeded=True)
        assert exc_info.value.code == "ALPHA_DISCOVERY_VALIDATION_REQUIRED"
        assert mission.state == "AWAITING_VALIDATION"
        assert dependent.state == "READY"
        session.rollback()


def test_alpha_completion_gate_rejects_uncalibrated_return_mode(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        discovery_id = _stage(session, facts, tmp_path)
        assignment = accept_discovery_evaluation_result(
            session,
            _discovery_result(session, discovery_id),
        )
        assert assignment is not None and assignment.alpha_calibration_version_id is None
        version_id = assignment.alpha_model_version_id

    with Session(engine) as session:
        mission = session.get(ResearchMission, facts["mission_id"])
        dependent = session.get(ResearchMission, facts["dependent_id"])
        version = session.get(AlphaModelVersion, version_id)
        assert mission is not None and dependent is not None and version is not None
        mission.state = "AWAITING_VALIDATION"
        mission.outcome = None
        mission.finished_at = None
        version.mode = "CALIBRATED_RETURN"
        with pytest.raises(QfError) as exc_info:
            finish_mission(session, mission.id, succeeded=True)
        assert exc_info.value.code == "ALPHA_DISCOVERY_VALIDATION_REQUIRED"
        assert mission.state == "AWAITING_VALIDATION"
        assert dependent.state == "READY"
        session.rollback()


def test_bare_discovery_outcome_is_rejected_without_mutation(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        discovery_id = _stage(session, facts, tmp_path)
        with pytest.raises(QfError) as exc_info:
            accept_discovery_evaluation_result(
                session,
                cast(DiscoveryEvaluationResult, discovery_id),
            )
        assert exc_info.value.code == "DISCOVERY_EVALUATION_RESULT_REQUIRED"
        discovery = session.get(AlphaDiscoveryEvaluation, discovery_id)
        mission = session.get(ResearchMission, facts["mission_id"])
        assert discovery is not None and discovery.state == "QUEUED"
        assert discovery.private_result_ref is None
        assert mission is not None and mission.state == "AWAITING_VALIDATION"


def test_calibrated_return_design_is_rejected_before_model_or_job(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path, design_mode="CALIBRATED_RETURN")
        result = stage_alpha_discovery_evaluation(
            session,
            mission_id=facts["mission_id"],  # type: ignore[arg-type]
            workspace=facts["workspace"],  # type: ignore[arg-type]
            artifact_root=tmp_path / "qz-artifacts",
        )
        assert not result.accepted
        assert result.error_code == "ALPHA_DISCOVERY_CONFIGURATION_UNAVAILABLE"

    with Session(engine) as session:
        artifact = session.get(MissionArtifact, facts["artifact_id"])
        assert artifact is not None and artifact.state == "REJECTED"
        assert session.scalar(select(func.count()).select_from(AlphaModelVersion)) == 0
        assert session.scalar(select(func.count()).select_from(AlphaDiscoveryEvaluation)) == 0
        assert session.scalar(select(func.count()).select_from(Job).where(Job.kind == "DISCOVERY_EVALUATION")) == 0


def test_untrusted_frozen_selection_phase_is_rejected_before_model_or_job(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        sealed_dataset = session.get(DatasetRevision, facts["dataset_ids"][2])  # type: ignore[index]
        assert sealed_dataset is not None
        sealed_dataset.point_in_time_state = "INVALID"
        result = stage_alpha_discovery_evaluation(
            session,
            mission_id=facts["mission_id"],  # type: ignore[arg-type]
            workspace=facts["workspace"],  # type: ignore[arg-type]
            artifact_root=tmp_path / "qz-artifacts",
        )
        assert not result.accepted
        assert result.error_code == "ALPHA_DISCOVERY_DATA_UNAVAILABLE"

    with Session(engine) as session:
        artifact = session.get(MissionArtifact, facts["artifact_id"])
        assert artifact is not None and artifact.state == "REJECTED"
        assert session.scalar(select(func.count()).select_from(AlphaModelVersion)) == 0
        assert session.scalar(select(func.count()).select_from(AlphaDiscoveryEvaluation)) == 0
        assert session.scalar(select(func.count()).select_from(Job).where(Job.kind == "DISCOVERY_EVALUATION")) == 0


def test_oversized_alpha_source_is_rejected_without_a_partial_owned_artifact(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        source = facts["workspace"] / "alphas" / "mean_reversion.py"  # type: ignore[operator]
        source.write_bytes(b"x" * (1 * 1024 * 1024 + 1))
        result = stage_alpha_discovery_evaluation(
            session,
            mission_id=facts["mission_id"],  # type: ignore[arg-type]
            workspace=facts["workspace"],  # type: ignore[arg-type]
            artifact_root=tmp_path / "qz-artifacts",
        )
        assert not result.accepted
        assert result.error_code == "ALPHA_PROPOSAL_MATERIALIZATION_FAILED"

    with Session(engine) as session:
        artifact = session.get(MissionArtifact, facts["artifact_id"])
        assert artifact is not None and artifact.state == "REJECTED"
        assert session.scalar(select(func.count()).select_from(AlphaModelVersion)) == 0
        assert session.scalar(select(func.count()).select_from(AlphaDiscoveryEvaluation)) == 0
        model_root = tmp_path / "qz-artifacts" / "alpha-models" / str(facts["artifact_id"])
        assert model_root.is_dir() and not list(model_root.iterdir())


def test_symlinked_source_path_component_is_rejected_before_materialization(engine, tmp_path: Path) -> None:
    with Session(engine) as session, session.begin():
        facts = _seed(session, tmp_path)
        workspace = facts["workspace"]  # type: ignore[assignment]
        original = workspace / "alphas" / "mean_reversion.py"
        alternate = workspace / "alternate-source"
        alternate.mkdir()
        original.rename(alternate / original.name)
        original.parent.rmdir()
        (workspace / "alphas").symlink_to(alternate, target_is_directory=True)
        result = stage_alpha_discovery_evaluation(
            session,
            mission_id=facts["mission_id"],  # type: ignore[arg-type]
            workspace=workspace,
            artifact_root=tmp_path / "qz-artifacts",
        )
        assert not result.accepted
        assert result.error_code == "ALPHA_PROPOSAL_SOURCE_INVALID"

    with Session(engine) as session:
        artifact = session.get(MissionArtifact, facts["artifact_id"])
        assert artifact is not None and artifact.state == "REJECTED"
        assert session.scalar(select(func.count()).select_from(AlphaModelVersion)) == 0
        assert session.scalar(select(func.count()).select_from(AlphaDiscoveryEvaluation)) == 0
