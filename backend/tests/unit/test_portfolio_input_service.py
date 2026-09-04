from __future__ import annotations

from dataclasses import replace
from datetime import UTC, datetime, timedelta, timezone
from decimal import Decimal
from uuid import uuid4

import pytest
from sqlalchemy import event, func, select
from sqlalchemy.exc import IntegrityError, StatementError
from sqlalchemy.orm import Session

from db.models import (
    AlphaDiscoveryEvaluation,
    AlphaEvaluationAssignment,
    AlphaEvaluationAssignmentDatasetRevision,
    AlphaEvaluationEpisode,
    AlphaEvaluationForecast,
    AlphaEvaluationResult,
    AlphaModel,
    AlphaModelVersion,
    AlphaQualification,
    AlphaSignalArtifact,
    CapitalContextVersion,
    DownstreamSystem,
    DownstreamConnectionVersion,
    EvaluationDatasetSelection,
    EvaluationDesignVersion,
    Event,
    FeedbackContractVersion,
    PortfolioAssemblyInput,
    PortfolioAssemblyInputCovariance,
    PortfolioAssemblyInputMember,
    PortfolioCandidate,
    PortfolioCandidateFamily,
    PortfolioCandidateMember,
    PortfolioEvaluationAssignment,
    PortfolioEvaluationEpisode,
    CandidatePackage,
    Job,
    PortfolioInputEvaluationAssignment,
    PortfolioMandate,
    PortfolioMandateVersion,
    PortfolioProgram,
    PreflightReceipt,
    PromotionPolicyGate,
    PromotionPolicyVersion,
)
from errors import QfError
from portfolio_evaluation_service import (
    accept_portfolio_evaluation_result,
    ensure_portfolio_evaluation,
    prepare_portfolio_evaluation,
)
from portfolio_input_service import (
    PortfolioCovariance,
    PortfolioInputEvaluationRequest,
    PortfolioInputEvaluationResult,
    accept_portfolio_input_evaluation_result,
    assemble_trusted_portfolio_input,
    persist_portfolio_input_evaluation,
    prepare_portfolio_input_evaluation,
    stage_initial_portfolio_input_evaluations,
    stage_portfolio_input_evaluation,
)
from research_engine.sealed_evaluator_contracts import (
    DisclosureClassification,
    EvaluationPhase,
    EvaluationStatus,
    GateCode,
    GateResult,
    GateStatus,
    LevelOneDisclosure,
    MetricAggregate,
    MetricCode,
    MetricStatus,
    PortfolioCovariance as EvaluatorPortfolioCovariance,
    PortfolioCovarianceMethod,
    PortfolioInputEvaluationResult as EvaluatorPortfolioInputEvaluationResult,
    SealedEvaluationResult,
)
from test_trusted_alpha_evaluation_persistence import _seed_assignment


def _second_axis(session: Session, facts: dict[str, object]) -> dict[str, object]:
    assignment = facts["assignment"]
    model = facts["model"]
    forecast = facts["forecast"]
    datasets = facts["datasets"]
    selection = facts["selection"]
    design = facts["design"]
    assert isinstance(assignment, AlphaEvaluationAssignment)
    assert isinstance(model, AlphaModelVersion)
    assert isinstance(forecast, AlphaEvaluationForecast)
    assert isinstance(datasets, list)
    assert isinstance(selection, EvaluationDatasetSelection)
    assert isinstance(design, EvaluationDesignVersion)

    alpha = AlphaModel(
        alpha_key="trusted-alpha-two",
        name="Second trusted Alpha",
        family="VALUE",
        description="A second bounded Alpha proposal.",
        owner_program_id=assignment.program_id,
        state="RESEARCHING",
    )
    session.add(alpha)
    session.flush()
    model_version = AlphaModelVersion(
        alpha_model_id=alpha.id,
        version_no=1,
        source_mission_id=model.source_mission_id,
        source_mission_artifact_id=model.source_mission_artifact_id,
        source_mission_artifact_revision=model.source_mission_artifact_revision,
        universe_version_id=model.universe_version_id,
        horizon=model.horizon,
        mode="CALIBRATED_RETURN",
        artifact_uri="artifact://alpha-model-two",
        entrypoint="alpha_two:run",
        parameters={},
        input_contract={},
        output_contract={},
        state="VALIDATED",
    )
    session.add(model_version)
    session.flush()
    discovery_event = Event(
        kind="ALPHA_PROPOSAL_VALIDATED",
        aggregate_type="MISSION",
        aggregate_id=assignment.mission_id,
    )
    session.add(discovery_event)
    session.flush()
    discovery = AlphaDiscoveryEvaluation(
        source_mission_artifact_id=model.source_mission_artifact_id,
        source_mission_artifact_revision=model.source_mission_artifact_revision,
        alpha_model_version_id=model_version.id,
        program_id=assignment.program_id,
        cycle_id=assignment.cycle_id,
        branch_id=assignment.branch_id,
        mission_id=assignment.mission_id,
        discovery_dataset_revision_id=datasets[0].id,
        evaluation_dataset_selection_id=selection.id,
        evaluation_design_version_id=design.id,
        cause_event_id=discovery_event.id,
        evaluator_contract_version="1",
        state="VALID",
        outcome_code="DISCOVERY_VALIDATED",
        private_result_ref=uuid4(),
        evaluated_at=forecast.as_of_time,
        completed_at=forecast.as_of_time,
    )
    session.add(discovery)
    session.flush()
    second_assignment = AlphaEvaluationAssignment(
        source_mission_artifact_id=model.source_mission_artifact_id,
        source_mission_artifact_revision=model.source_mission_artifact_revision,
        discovery_evaluation_id=discovery.id,
        program_id=assignment.program_id,
        cycle_id=assignment.cycle_id,
        branch_id=assignment.branch_id,
        mission_id=assignment.mission_id,
        alpha_model_version_id=model_version.id,
        universe_version_id=model_version.universe_version_id,
        sealed_dataset_revision_id=datasets[2].id,
        evaluation_design_version_id=design.id,
        promotion_policy_version_id=assignment.promotion_policy_version_id,
        cause_event_id=discovery_event.id,
        assignment_no=1,
        evaluator_contract_version="1",
        state="FINALIZED",
    )
    session.add(second_assignment)
    session.flush()
    session.add_all(
        AlphaEvaluationAssignmentDatasetRevision(
            assignment_id=second_assignment.id,
            dataset_revision_id=dataset.id,
            phase=phase,
            ordinal=1,
        )
        for dataset, phase in zip(datasets, ("DISCOVERY", "VALIDATION", "SEALED"), strict=True)
    )
    session.flush()
    episode = AlphaEvaluationEpisode(
        program_id=assignment.program_id,
        branch_id=assignment.branch_id,
        alpha_model_version_id=model_version.id,
        assignment_id=second_assignment.id,
        discovery_run_ids=[],
        validation_run_ids=[],
        sealed_dataset_revision_id=datasets[2].id,
        promotion_policy_version_id=assignment.promotion_policy_version_id,
        state="DISCLOSED",
        result="PASS",
        gate_results={},
        multiple_testing_summary={},
        disclosure={},
        sealed_at=forecast.as_of_time,
        evaluated_at=forecast.as_of_time,
        disclosed_at=forecast.as_of_time,
    )
    session.add(episode)
    session.flush()
    result = AlphaEvaluationResult(
        episode_id=episode.id,
        evidence_validity="VALID",
        result="PASS",
        private_result_ref=uuid4(),
        evaluated_at=forecast.as_of_time,
    )
    session.add(result)
    session.flush()
    signal = AlphaSignalArtifact(
        id=uuid4(),
        alpha_model_version_id=model_version.id,
        dataset_revision_id=datasets[2].id,
        evaluation_result_id=result.id,
        mode="CALIBRATED_RETURN",
        artifact_uri="artifact://alpha-signal-two",
        row_count=1,
        event_start=forecast.as_of_time,
        event_end=forecast.as_of_time,
        available_start=forecast.as_of_time,
        available_end=forecast.as_of_time,
        schema_version="1",
    )
    session.add(signal)
    session.flush()
    qualification = AlphaQualification(
        program_id=assignment.program_id,
        alpha_model_id=alpha.id,
        alpha_model_version_id=model_version.id,
        universe_version_id=model.universe_version_id,
        universe="US Equities",
        horizon=model.horizon,
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
    second_forecast = AlphaEvaluationForecast(
        result_id=result.id,
        signal_artifact_id=signal.id,
        instrument_id="US:SECOND",
        as_of_time=forecast.as_of_time,
        effective_from=forecast.effective_from,
        effective_until=forecast.effective_until,
        expected_return=Decimal("0.03"),
        uncertainty=Decimal("0.02"),
        confidence=Decimal("0.7"),
        max_trade_notional=Decimal("10000"),
        max_position_notional=Decimal("50000"),
        max_participation_rate=Decimal("0.1"),
        days_to_liquidate=Decimal("2"),
        stressed_capacity_notional=Decimal("20000"),
    )
    session.add_all((qualification, second_forecast))
    return {
        "qualification": qualification,
        "signal": signal,
        "forecast": second_forecast,
        "episode": episode,
        "assignment": second_assignment,
    }


def _portfolio_facts(session: Session) -> dict[str, object]:
    facts = _seed_assignment(session)
    source_assignment = facts["assignment"]
    source_episode = facts["episode"]
    assert isinstance(source_assignment, AlphaEvaluationAssignment)
    assert isinstance(source_episode, AlphaEvaluationEpisode)
    source_assignment.state = "FINALIZED"
    source_episode.state = "DISCLOSED"
    source_episode.disclosed_at = source_episode.evaluated_at
    second = _second_axis(session, facts)
    forecast = facts["forecast"]
    assert isinstance(forecast, AlphaEvaluationForecast)
    source_model = facts["model"]
    assert isinstance(source_model, AlphaModelVersion)
    mandate_id = uuid4()
    mandate = PortfolioMandate(
        id=mandate_id,
        key="portfolio-input-test",
        name="Portfolio Input Test",
        latest_version_id=uuid4(),
        spec_json={},
        state="ACTIVE",
    )
    mandate_version = PortfolioMandateVersion(
        id=mandate.latest_version_id,
        portfolio_mandate_id=mandate.id,
        version_no=1,
        base_currency="USD",
        objective="MAXIMIZE_NET_RETURN",
        policy_family="LONG_ONLY_MEAN_VARIANCE_V1",
        universe_version_id=source_model.universe_version_id,
        eligible_alpha_role="PRIMARY_ALPHA",
        eligible_alpha_roles=[],
        eligible_universe_version_ids=[],
        minimum_alpha_count=2,
        minimum_weight=Decimal("0.01"),
        maximum_weight=Decimal("0.75"),
        gross_exposure_limit=Decimal("1"),
        net_exposure_target=Decimal("1"),
        cash_reserve=Decimal("0"),
        turnover_limit=Decimal("2"),
        variance_limit=Decimal("1"),
        risk_aversion=Decimal("1"),
        cost_aversion=Decimal("0"),
        uncertainty_aversion=Decimal("0"),
        commission_rate=Decimal("0"),
        half_spread_rate=Decimal("0"),
        slippage_rate=Decimal("0"),
        impact_rate=Decimal("0"),
        impact_breakpoint=Decimal("0"),
        state="ACTIVE",
        capital_config={},
        risk_config={},
        cost_config={},
        capacity_config={},
        promotion_policy={},
        constraint_config={},
    )
    paper_downstream = DownstreamSystem(
        name="paper-test",
        environment_type="PAPER",
        compatibility=[],
        public_config={},
    )
    live_downstream = DownstreamSystem(
        name="live-test",
        environment_type="LIVE",
        compatibility=[],
        public_config={},
    )
    capital = CapitalContextVersion(
        configuration_contract_version="CAPITAL_CONTEXT_V1",
        source_type="ADMIN",
        source_downstream_system_id=None,
        base_currency="USD",
        deployable_capital=Decimal("10000"),
        observed_at=forecast.as_of_time,
        valid_until=forecast.as_of_time + timedelta(days=1),
    )
    session.add_all((mandate, mandate_version, paper_downstream, live_downstream, capital))
    session.flush()
    paper_feedback = FeedbackContractVersion(
        downstream_system_id=paper_downstream.id,
        version_no=1,
        purpose="PAPER",
        minimum_observation_seconds=1,
        minimum_valid_sample_size=1,
        first_status_deadline_seconds=1,
        complete_feedback_deadline_seconds=1,
        grace_period_seconds=0,
        disclosure_policy="LEVEL_1",
    )
    live_feedback = FeedbackContractVersion(
        downstream_system_id=live_downstream.id,
        version_no=1,
        purpose="LIVE",
        minimum_observation_seconds=1,
        minimum_valid_sample_size=1,
        first_status_deadline_seconds=1,
        complete_feedback_deadline_seconds=1,
        grace_period_seconds=0,
        disclosure_policy="LEVEL_1",
    )
    session.add_all((paper_feedback, live_feedback))
    session.flush()
    paper_connection = DownstreamConnectionVersion(
        downstream_system_id=paper_downstream.id,
        version_no=1,
        package_contract_version="CANDIDATE_PACKAGE_V1",
        feedback_contract_version_id=paper_feedback.id,
        public_config={},
        state="ACTIVE",
    )
    live_connection = DownstreamConnectionVersion(
        downstream_system_id=live_downstream.id,
        version_no=1,
        package_contract_version="CANDIDATE_PACKAGE_V1",
        feedback_contract_version_id=live_feedback.id,
        public_config={},
        state="ACTIVE",
    )
    session.add_all((paper_connection, live_connection))
    session.flush()
    paper_receipt = PreflightReceipt(
        resource_type="DOWNSTREAM_CONNECTION_VERSION",
        resource_id=paper_connection.id,
        resource_revision=paper_connection.version_no,
        revision=1,
        status="READY",
        reason_codes=[],
        capabilities=[],
        contract_version=paper_connection.package_contract_version,
        checked_at=forecast.as_of_time,
        valid_until=forecast.as_of_time + timedelta(days=1),
        checker_version="test",
    )
    live_receipt = PreflightReceipt(
        resource_type="DOWNSTREAM_CONNECTION_VERSION",
        resource_id=live_connection.id,
        resource_revision=live_connection.version_no,
        revision=1,
        status="READY",
        reason_codes=[],
        capabilities=[],
        contract_version=live_connection.package_contract_version,
        checked_at=forecast.as_of_time,
        valid_until=forecast.as_of_time + timedelta(days=1),
        checker_version="test",
    )
    session.add_all((paper_receipt, live_receipt))
    session.flush()
    paper_to_live = PromotionPolicyVersion(
        version_no=1,
        purpose="PAPER_TO_LIVE",
        mode="MANUAL_APPROVAL",
        policy_contract_version="PROMOTION_POLICY_V1",
        paper_downstream_system_id=paper_downstream.id,
        paper_connection_version_id=paper_connection.id,
        paper_feedback_contract_version_id=paper_feedback.id,
        paper_preflight_receipt_id=paper_receipt.id,
        live_downstream_system_id=live_downstream.id,
        live_connection_version_id=live_connection.id,
        live_feedback_contract_version_id=live_feedback.id,
        live_preflight_receipt_id=live_receipt.id,
        state="ACTIVE",
    )
    session.add(paper_to_live)
    session.flush()
    policy = PromotionPolicyVersion(
        version_no=1,
        purpose="PORTFOLIO_TO_PAPER",
        mode="MANUAL_APPROVAL",
        policy_contract_version="PROMOTION_POLICY_V1",
        paper_downstream_system_id=paper_downstream.id,
        paper_connection_version_id=paper_connection.id,
        paper_feedback_contract_version_id=paper_feedback.id,
        paper_preflight_receipt_id=paper_receipt.id,
        paper_to_live_policy_version_id=paper_to_live.id,
        state="ACTIVE",
    )
    event = Event(
        kind="PORTFOLIO_INPUT_TEST", aggregate_type="PORTFOLIO_MANDATE", aggregate_id=mandate.id
    )
    session.add_all((policy, event))
    session.flush()
    session.add(
        PromotionPolicyGate(
            policy_version_id=policy.id,
            metric_code="MATERIAL_IMPROVEMENT",
            comparator="MINIMUM",
            threshold=Decimal("0"),
            ordinal=1,
        )
    )
    session.flush()
    return {
        "mandate": mandate_version,
        "event": event,
        "qualifications": (facts["qualification"], second["qualification"]),
        "episode": facts["episode"],
        "second_episode": second["episode"],
        "second_alpha_assignment": second["assignment"],
        "second_signal": second["signal"],
        "second_forecast": second["forecast"],
        "forecast": forecast,
        "other_dataset_id": facts["datasets"][1].id,
        "policy": policy,
        "paper_to_live": paper_to_live,
        "paper_downstream": paper_downstream,
    }


def _result(
    assignment_id, *, diagonal: Decimal = Decimal("0.04")
) -> PortfolioInputEvaluationResult:
    return PortfolioInputEvaluationResult(
        assignment_id=assignment_id,
        private_result_ref=uuid4(),
        evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
        covariance_method="EWMA_SHRINKAGE",
        covariance_observations=20,
        covariance_decay=Decimal("0.5"),
        covariance_shrinkage=Decimal("0.1"),
        covariance_upper_triangle=(
            PortfolioCovariance(0, 0, diagonal),
            PortfolioCovariance(0, 1, Decimal("0.01")),
            PortfolioCovariance(1, 1, Decimal("0.03")),
        ),
    )


def _typed_result(input_value) -> EvaluatorPortfolioInputEvaluationResult:
    return EvaluatorPortfolioInputEvaluationResult(
        input=input_value,
        private_result_id=uuid4(),
        evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
        covariance_method=PortfolioCovarianceMethod.EWMA_SHRINKAGE,
        covariance_observations=20,
        covariance_decay=0.5,
        covariance_shrinkage=0.1,
        covariance_upper_triangle=(
            EvaluatorPortfolioCovariance(0, 0, 0.04),
            EvaluatorPortfolioCovariance(0, 1, 0.01),
            EvaluatorPortfolioCovariance(1, 1, 0.03),
        ),
    )


@pytest.fixture(autouse=True)
def _sqlite_foreign_keys(engine) -> None:
    if engine.dialect.name == "sqlite":
        with engine.begin() as connection:
            connection.exec_driver_sql("PRAGMA foreign_keys = ON")
    yield
    if engine.dialect.name == "sqlite":
        with engine.begin() as connection:
            connection.exec_driver_sql("PRAGMA foreign_keys = OFF")


def _request(
    facts: dict[str, object], *, cause_event_id: int | None = None
) -> PortfolioInputEvaluationRequest:
    mandate = facts["mandate"]
    event = facts["event"]
    qualifications = facts["qualifications"]
    assert isinstance(mandate, PortfolioMandateVersion)
    assert isinstance(event, Event)
    assert isinstance(qualifications, tuple)
    return PortfolioInputEvaluationRequest(
        mandate_version_id=mandate.id,
        cause_event_id=event.id if cause_event_id is None else cause_event_id,
        alpha_qualification_ids=tuple(item.id for item in qualifications),
    )


def _stage(session: Session, facts: dict[str, object]) -> PortfolioInputEvaluationAssignment:
    assignment = stage_portfolio_input_evaluation(session, _request(facts))
    assert assignment is not None
    return assignment


def _assembled_candidate_for_evaluation(
    session: Session,
) -> tuple[PortfolioCandidate, PortfolioAssemblyInput, PortfolioInputEvaluationAssignment]:
    facts = _portfolio_facts(session)
    input_assignment = _stage(session, facts)
    input_row = persist_portfolio_input_evaluation(session, _result(input_assignment.id))
    assert input_row is not None
    family = session.scalar(
        select(PortfolioCandidateFamily).where(
            PortfolioCandidateFamily.portfolio_program_id == input_row.portfolio_program_id
        )
    )
    assert family is not None
    input_row.state = "ASSEMBLED"
    input_row.outcome_code = "OPTIMAL"
    input_row.completed_at = datetime.now(UTC)
    candidate = PortfolioCandidate(
        id=uuid4(),
        candidate_family_id=family.id,
        portfolio_program_id=input_row.portfolio_program_id,
        mandate_version_id=input_row.mandate_version_id,
        capital_context_version_id=input_row.capital_context_version_id,
        assembly_input_id=input_row.id,
        universe_version_id=input_row.universe_version_id,
        state="ASSEMBLED",
        created_at=datetime.now(UTC),
    )
    session.add(candidate)
    session.flush()
    return candidate, input_row, input_assignment


def _portfolio_result_input(input_value):
    metrics = tuple(
        MetricAggregate(
            phase=EvaluationPhase.SEALED,
            code=code,
            status=MetricStatus.AVAILABLE,
            value=0.0,
        )
        for code in sorted(
            (
                MetricCode.OBSERVATION_COUNT,
                MetricCode.NET_RETURN,
                MetricCode.ANNUALIZED_VOLATILITY,
                MetricCode.SHARPE_RATIO,
                MetricCode.MAX_DRAWDOWN,
                MetricCode.TURNOVER,
                MetricCode.CAPACITY_UTILIZATION,
                MetricCode.MATERIAL_IMPROVEMENT,
            ),
            key=lambda code: code.value,
        )
    )
    gates = tuple(
        GateResult(code=code, status=GateStatus.PASS)
        for code in sorted(
            (
                GateCode.EVIDENCE_VALID,
                GateCode.POINT_IN_TIME_VALID,
                GateCode.POLICY_VALID,
                GateCode.MATERIAL_IMPROVEMENT_VALID,
            ),
            key=lambda code: code.value,
        )
    )
    return SealedEvaluationResult(
        input=input_value,
        status=EvaluationStatus.PASS,
        private_result_id=uuid4(),
        evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
        metrics=metrics,
        gates=gates,
        disclosure=LevelOneDisclosure(DisclosureClassification.QUALIFIED),
    )


def test_trusted_input_persists_complete_graph_then_assembled_candidate(engine) -> None:
    pytest.importorskip("numpy")
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        assignment = _stage(session, facts)
        result = _result(assignment.id)

        input_row = persist_portfolio_input_evaluation(session, result)
        assert input_row is not None
        candidate = assemble_trusted_portfolio_input(session, input_row.id)
        assert candidate is not None
        input_id = input_row.id
        candidate_id = candidate.id
        program_id = candidate.portfolio_program_id
        family_id = candidate.candidate_family_id
        session.commit()

    with Session(engine) as session:
        input_row = session.get(PortfolioAssemblyInput, input_id)
        candidate = session.get(PortfolioCandidate, candidate_id)
        assert input_row is not None
        assert candidate is not None
        assert input_row.state == "ASSEMBLED"
        assert candidate.state == "ASSEMBLED"
        assert candidate.assembly_input_id == input_row.id
        assert candidate.candidate_family_id == family_id
        assert (
            session.scalar(
                select(func.count())
                .select_from(PortfolioAssemblyInputMember)
                .where(PortfolioAssemblyInputMember.input_id == input_row.id)
            )
            == 2
        )
        assert (
            session.scalar(
                select(func.count())
                .select_from(PortfolioAssemblyInputCovariance)
                .where(PortfolioAssemblyInputCovariance.input_id == input_row.id)
            )
            == 3
        )
        assert (
            session.scalar(
                select(func.count())
                .select_from(PortfolioCandidateMember)
                .where(PortfolioCandidateMember.candidate_id == candidate.id)
            )
            == 2
        )
        program = session.get(PortfolioProgram, program_id)
        assert program is not None
        assert program.current_candidate_id is None
        family = session.get(PortfolioCandidateFamily, family_id)
        assert family is not None
        assert family.portfolio_program_id == candidate.portfolio_program_id
        evaluation_assignment = session.scalar(
            select(PortfolioEvaluationAssignment).where(
                PortfolioEvaluationAssignment.candidate_id == candidate.id
            )
        )
        assert evaluation_assignment is not None
        assert evaluation_assignment.state == "QUEUED"
        assert evaluation_assignment.previous_candidate_id is None
        episode = session.scalar(
            select(PortfolioEvaluationEpisode).where(
                PortfolioEvaluationEpisode.assignment_id == evaluation_assignment.id
            )
        )
        assert episode is not None
        assert (episode.state, episode.candidate_id) == ("ASSIGNED", candidate.id)
        assert (
            session.scalar(
                select(func.count())
                .select_from(CandidatePackage)
                .where(CandidatePackage.candidate_id == candidate.id)
            )
            == 0
        )
        jobs = list(
            session.scalars(
                select(Job)
                .where(
                    (Job.kind == "CANDIDATE_PACKAGE_BUILD")
                    | (Job.kind == "PORTFOLIO_EVALUATION")
                )
                .order_by(Job.kind)
            )
        )
        assert [(job.kind, job.state, job.payload) for job in jobs] == [
            ("CANDIDATE_PACKAGE_BUILD", "READY", {}),
            ("PORTFOLIO_EVALUATION", "READY", {}),
        ]


def test_initial_trigger_creates_one_event_assignment_and_empty_job(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        qualifications = facts["qualifications"]
        assert isinstance(qualifications, tuple)

        first = stage_initial_portfolio_input_evaluations(
            session, qualification_id=qualifications[0].id
        )
        policy = facts["policy"]
        assert isinstance(policy, PromotionPolicyVersion)
        policy.state = "RETIRED"
        second = stage_initial_portfolio_input_evaluations(
            session, qualification_id=qualifications[1].id
        )

        assert len(first) == 1
        assert tuple(item.id for item in second) == tuple(item.id for item in first)
        assignment = first[0]
        assert (assignment.state, assignment.previous_candidate_id) == ("QUEUED", None)
        event = session.get(Event, assignment.cause_event_id)
        assert event is not None
        assert (event.kind, event.aggregate_type, event.aggregate_id) == (
            "PORTFOLIO_MANDATE",
            "PORTFOLIO_MANDATE",
            facts["mandate"].portfolio_mandate_id,
        )
        assert session.scalar(select(func.count()).select_from(PortfolioProgram)) == 1
        assert session.scalar(select(func.count()).select_from(PortfolioCandidateFamily)) == 1
        jobs = list(
            session.scalars(
                select(Job).where(
                    Job.kind == "PORTFOLIO_INPUT_EVALUATION",
                    Job.resource_type == "portfolio_input_evaluation_assignment",
                    Job.resource_id == assignment.id,
                )
            )
        )
        assert [(job.state, job.payload) for job in jobs] == [("READY", {})]


def test_initial_trigger_does_not_create_empty_program_when_axes_are_incomplete(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        signal = facts["second_signal"]
        qualifications = facts["qualifications"]
        other_dataset_id = facts["other_dataset_id"]
        assert isinstance(signal, AlphaSignalArtifact)
        assert isinstance(qualifications, tuple)
        signal.dataset_revision_id = other_dataset_id

        assert stage_initial_portfolio_input_evaluations(
            session, qualification_id=qualifications[0].id
        ) == ()
        assert session.scalar(select(func.count()).select_from(PortfolioProgram)) == 0
        assert session.scalar(select(func.count()).select_from(PortfolioCandidateFamily)) == 0
        assert (
            session.scalar(
                select(func.count())
                .select_from(Event)
                .where(Event.kind == "PORTFOLIO_MANDATE")
            )
            == 0
        )


def test_stage_rejects_legacy_or_mismatched_p2p_policy_before_program_creation(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        policy = facts["policy"]
        paper_downstream = facts["paper_downstream"]
        assert isinstance(policy, PromotionPolicyVersion)
        assert isinstance(paper_downstream, DownstreamSystem)
        policy.state = "RETIRED"
        session.add(
            PromotionPolicyVersion(
                version_no=2,
                purpose="PORTFOLIO_TO_PAPER",
                mode="MANUAL_APPROVAL",
                paper_downstream_system_id=paper_downstream.id,
                state="ACTIVE",
            )
        )
        session.flush()

        assert stage_portfolio_input_evaluation(session, _request(facts)) is None
        assert session.scalar(select(func.count()).select_from(PortfolioProgram)) == 0

    with Session(engine) as session:
        facts = _portfolio_facts(session)
        target = facts["paper_to_live"]
        assert isinstance(target, PromotionPolicyVersion)
        target.paper_preflight_receipt_id = target.live_preflight_receipt_id

        assert stage_portfolio_input_evaluation(session, _request(facts)) is None
        assert session.scalar(select(func.count()).select_from(PortfolioProgram)) == 0


def test_input_completion_queues_empty_assembly_job_and_requires_ewma_shrinkage(engine) -> None:
    with Session(engine) as session:
        assignment = _stage(session, _portfolio_facts(session))
        result = _result(assignment.id)
        bad_method = PortfolioInputEvaluationResult(
            assignment_id=result.assignment_id,
            private_result_ref=result.private_result_ref,
            evaluated_at=result.evaluated_at,
            covariance_method="UNKNOWN",
            covariance_observations=result.covariance_observations,
            covariance_decay=result.covariance_decay,
            covariance_shrinkage=result.covariance_shrinkage,
            covariance_upper_triangle=result.covariance_upper_triangle,
        )
        with pytest.raises(QfError, match="PORTFOLIO_INPUT_EVALUATOR_RESULT_INVALID"):
            persist_portfolio_input_evaluation(session, bad_method)

        input_row = persist_portfolio_input_evaluation(session, result)
        assert input_row is not None
        jobs = list(
            session.scalars(
                select(Job).where(
                    Job.kind == "PORTFOLIO_ASSEMBLY",
                    Job.resource_type == "portfolio_assembly_input",
                    Job.resource_id == input_row.id,
                )
            )
        )
        assert [(job.state, job.payload) for job in jobs] == [("READY", {})]


def test_input_evaluator_uses_one_exact_frozen_typed_descriptor(engine) -> None:
    with Session(engine) as session:
        assignment = _stage(session, _portfolio_facts(session))
        descriptor = prepare_portfolio_input_evaluation(session, assignment.id)

        assert descriptor.assignment_id == assignment.id
        assert descriptor.previous_candidate_id is None
        assert tuple(axis.axis_index for axis in descriptor.axes) == (0, 1)

        with pytest.raises(QfError, match="PORTFOLIO_INPUT_EVALUATION_INPUT_MISMATCH"):
            accept_portfolio_input_evaluation_result(
                session,
                _typed_result(replace(descriptor, cause_event_id=descriptor.cause_event_id + 1)),
            )

        input_row = accept_portfolio_input_evaluation_result(session, _typed_result(descriptor))
        assert input_row is not None
        assert (assignment.state, input_row.state) == ("VALID", "PENDING")


def test_portfolio_evaluation_accepts_only_exact_frozen_input_lineage(engine) -> None:
    with Session(engine) as session:
        candidate, _input_row, input_assignment = _assembled_candidate_for_evaluation(session)
        assignment = ensure_portfolio_evaluation(session, candidate_id=candidate.id)
        descriptor_input = prepare_portfolio_evaluation(session, assignment.id)
        episode = accept_portfolio_evaluation_result(
            session, _portfolio_result_input(descriptor_input)
        )

        assert (episode.state, episode.result) == ("DISCLOSED", "PASS")
        assert assignment.state == "FINALIZED"
        assert assignment.previous_candidate_id is None

        for field_name in (
            "evaluation_dataset_selection_id",
            "sealed_dataset_revision_id",
            "promotion_policy_version_id",
            "previous_candidate_id",
        ):
            original = getattr(input_assignment, field_name)
            setattr(input_assignment, field_name, uuid4())
            with session.no_autoflush:
                with pytest.raises(QfError, match="PORTFOLIO_EVALUATION_SOURCE_INVALID"):
                    ensure_portfolio_evaluation(session, candidate_id=candidate.id)
            setattr(input_assignment, field_name, original)

        selection = session.get(
            EvaluationDatasetSelection, input_assignment.evaluation_dataset_selection_id
        )
        assert selection is not None
        original_universe = selection.universe_version_id
        selection.universe_version_id = uuid4()
        with session.no_autoflush:
            with pytest.raises(QfError, match="PORTFOLIO_EVALUATION_SOURCE_INVALID"):
                ensure_portfolio_evaluation(session, candidate_id=candidate.id)
        selection.universe_version_id = original_universe

        candidate_universe = candidate.universe_version_id
        candidate.universe_version_id = uuid4()
        with session.no_autoflush:
            with pytest.raises(QfError, match="PORTFOLIO_EVALUATION_SOURCE_INVALID"):
                ensure_portfolio_evaluation(session, candidate_id=candidate.id)
        candidate.universe_version_id = candidate_universe

        policy = session.get(PromotionPolicyVersion, input_assignment.promotion_policy_version_id)
        assert policy is not None
        policy.policy_contract_version = None
        with session.no_autoflush:
            with pytest.raises(QfError, match="PORTFOLIO_EVALUATION_SOURCE_INVALID"):
                ensure_portfolio_evaluation(session, candidate_id=candidate.id)


def test_partial_covariance_never_creates_input(engine) -> None:
    with Session(engine) as session:
        assignment = _stage(session, _portfolio_facts(session))
        result = _result(assignment.id)
        partial = PortfolioInputEvaluationResult(
            assignment_id=result.assignment_id,
            private_result_ref=result.private_result_ref,
            evaluated_at=result.evaluated_at,
            covariance_method=result.covariance_method,
            covariance_observations=result.covariance_observations,
            covariance_decay=result.covariance_decay,
            covariance_shrinkage=result.covariance_shrinkage,
            covariance_upper_triangle=result.covariance_upper_triangle[:-1],
        )

        assert persist_portfolio_input_evaluation(session, partial) is None
        assert assignment.state == "INVALID"
        assert session.scalar(select(func.count()).select_from(PortfolioAssemblyInput)) == 0


def test_second_phase_covariance_failure_rolls_back_the_input_graph(engine) -> None:
    with Session(engine) as session:
        assignment = _stage(session, _portfolio_facts(session))
        assignment_id = assignment.id
        session.commit()

    with Session(engine) as session:
        flushes = 0

        def corrupt_second_phase(current: Session, *_: object) -> None:
            nonlocal flushes
            flushes += 1
            if flushes == 2:
                covariance = next(
                    item
                    for item in current.new
                    if isinstance(item, PortfolioAssemblyInputCovariance)
                )
                covariance.right_axis_index = 99

        event.listen(session, "before_flush", corrupt_second_phase)
        try:
            with pytest.raises(IntegrityError):
                persist_portfolio_input_evaluation(session, _result(assignment_id))
        finally:
            event.remove(session, "before_flush", corrupt_second_phase)
        session.rollback()
        assert flushes == 2

    with Session(engine) as session:
        assert session.scalar(select(func.count()).select_from(PortfolioAssemblyInput)) == 0
        assert session.scalar(select(func.count()).select_from(PortfolioAssemblyInputMember)) == 0
        assert (
            session.scalar(select(func.count()).select_from(PortfolioAssemblyInputCovariance)) == 0
        )


def test_result_replay_requires_same_canonical_upper_triangle(engine) -> None:
    with Session(engine) as session:
        assignment = _stage(session, _portfolio_facts(session))
        first = _result(assignment.id, diagonal=Decimal("0.0400000001"))
        input_row = persist_portfolio_input_evaluation(session, first)
        assert input_row is not None
        retry = PortfolioInputEvaluationResult(
            assignment_id=first.assignment_id,
            private_result_ref=first.private_result_ref,
            evaluated_at=first.evaluated_at,
            covariance_method=first.covariance_method,
            covariance_observations=first.covariance_observations,
            covariance_decay=Decimal("0.5000000001"),
            covariance_shrinkage=Decimal("0.1000000001"),
            covariance_upper_triangle=(
                PortfolioCovariance(0, 0, Decimal("0.0400000004")),
                *first.covariance_upper_triangle[1:],
            ),
        )
        assert persist_portfolio_input_evaluation(session, retry).id == input_row.id

        changed = PortfolioInputEvaluationResult(
            assignment_id=first.assignment_id,
            private_result_ref=first.private_result_ref,
            evaluated_at=first.evaluated_at,
            covariance_method=first.covariance_method,
            covariance_observations=first.covariance_observations,
            covariance_decay=first.covariance_decay,
            covariance_shrinkage=first.covariance_shrinkage,
            covariance_upper_triangle=(
                PortfolioCovariance(0, 0, Decimal("0.02")),
                *first.covariance_upper_triangle[1:],
            ),
        )
        with pytest.raises(QfError, match="PORTFOLIO_INPUT_EVALUATION_RESULT_CONFLICT"):
            persist_portfolio_input_evaluation(session, changed)


def test_stage_rejects_signal_from_another_sealed_dataset(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        signal = facts["second_signal"]
        other_dataset_id = facts["other_dataset_id"]
        assert isinstance(signal, AlphaSignalArtifact)
        assert isinstance(other_dataset_id, type(signal.dataset_revision_id))
        signal.dataset_revision_id = other_dataset_id

        assert stage_portfolio_input_evaluation(session, _request(facts)) is None
        assert (
            session.scalar(select(func.count()).select_from(PortfolioInputEvaluationAssignment))
            == 0
        )


def test_evaluator_result_must_use_utc(engine) -> None:
    with Session(engine) as session:
        assignment = _stage(session, _portfolio_facts(session))
        result = _result(assignment.id)
        non_utc = PortfolioInputEvaluationResult(
            assignment_id=result.assignment_id,
            private_result_ref=result.private_result_ref,
            evaluated_at=datetime(2026, 9, 3, tzinfo=timezone(timedelta(hours=8))),
            covariance_method=result.covariance_method,
            covariance_observations=result.covariance_observations,
            covariance_decay=result.covariance_decay,
            covariance_shrinkage=result.covariance_shrinkage,
            covariance_upper_triangle=result.covariance_upper_triangle,
        )
        with pytest.raises(QfError, match="PORTFOLIO_INPUT_EVALUATOR_RESULT_INVALID"):
            persist_portfolio_input_evaluation(session, non_utc)


def test_stage_reuses_one_program_family_and_assignment(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        first = _stage(session, facts)
        policy = facts["policy"]
        assert isinstance(policy, PromotionPolicyVersion)
        policy.state = "RETIRED"
        second = _stage(session, facts)

        assert second.id == first.id
        assert session.scalar(select(func.count()).select_from(PortfolioProgram)) == 1
        assert session.scalar(select(func.count()).select_from(PortfolioCandidateFamily)) == 1


def test_stage_requires_enabled_parent_mandate(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        mandate = facts["mandate"]
        assert isinstance(mandate, PortfolioMandateVersion)
        parent = session.get(PortfolioMandate, mandate.portfolio_mandate_id)
        assert parent is not None
        parent.enabled = False

        with pytest.raises(QfError, match="PORTFOLIO_MANDATE_INPUT_INVALID"):
            stage_portfolio_input_evaluation(session, _request(facts))

        assert session.scalar(select(func.count()).select_from(PortfolioProgram)) == 0


def test_stage_requires_cause_event_from_the_mandate(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        wrong_event = Event(
            kind="PORTFOLIO_INPUT_TEST",
            aggregate_type="PORTFOLIO_MANDATE",
            aggregate_id=uuid4(),
        )
        session.add(wrong_event)
        session.flush()

        with pytest.raises(QfError, match="PORTFOLIO_CAUSE_EVENT_INVALID"):
            stage_portfolio_input_evaluation(
                session, _request(facts, cause_event_id=wrong_event.id)
            )

        assert session.scalar(select(func.count()).select_from(PortfolioProgram)) == 0


def test_stage_rejects_result_from_another_sealed_episode(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        episode = facts["second_episode"]
        other_dataset_id = facts["other_dataset_id"]
        assert isinstance(episode, AlphaEvaluationEpisode)
        assert isinstance(other_dataset_id, type(episode.sealed_dataset_revision_id))
        episode.sealed_dataset_revision_id = other_dataset_id

        assert stage_portfolio_input_evaluation(session, _request(facts)) is None
        assert (
            session.scalar(select(func.count()).select_from(PortfolioInputEvaluationAssignment))
            == 0
        )


def test_stage_rejects_qualification_result_episode_mismatch(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        qualifications = facts["qualifications"]
        episode = facts["episode"]
        assert isinstance(qualifications, tuple)
        assert isinstance(episode, AlphaEvaluationEpisode)
        qualifications[1].evaluation_episode_id = episode.id

        assert stage_portfolio_input_evaluation(session, _request(facts)) is None
        assert (
            session.scalar(select(func.count()).select_from(PortfolioInputEvaluationAssignment))
            == 0
        )


def test_stage_rejects_unfinalized_alpha_assignment(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        alpha_assignment = facts["second_alpha_assignment"]
        assert isinstance(alpha_assignment, AlphaEvaluationAssignment)
        alpha_assignment.state = "RUNNING"

        assert stage_portfolio_input_evaluation(session, _request(facts)) is None
        assert (
            session.scalar(select(func.count()).select_from(PortfolioInputEvaluationAssignment))
            == 0
        )


def test_persist_rejects_quarantined_frozen_qualification(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        assignment = _stage(session, facts)
        qualifications = facts["qualifications"]
        assert isinstance(qualifications, tuple)
        qualifications[1].state = "QUARANTINED"

        assert persist_portfolio_input_evaluation(session, _result(assignment.id)) is None
        assert assignment.state == "INVALID"
        assert session.scalar(select(func.count()).select_from(PortfolioAssemblyInput)) == 0


def test_candidate_rejects_mismatched_complete_input_lineage(engine) -> None:
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        input_row = persist_portfolio_input_evaluation(session, _result(_stage(session, facts).id))
        assert input_row is not None
        family = session.scalar(
            select(PortfolioCandidateFamily).where(
                PortfolioCandidateFamily.portfolio_program_id == input_row.portfolio_program_id
            )
        )
        assert family is not None
        session.add(
            PortfolioCandidate(
                id=uuid4(),
                candidate_family_id=family.id,
                portfolio_program_id=input_row.portfolio_program_id,
                mandate_version_id=input_row.mandate_version_id,
                capital_context_version_id=uuid4(),
                assembly_input_id=input_row.id,
                universe_version_id=input_row.universe_version_id,
                state="ASSEMBLED",
                created_at=datetime.now(UTC),
            )
        )

        with pytest.raises(IntegrityError):
            session.flush()


def test_input_member_rejects_nan_capacity(engine) -> None:
    with Session(engine) as session:
        input_row = persist_portfolio_input_evaluation(
            session,
            _result(_stage(session, _portfolio_facts(session)).id),
        )
        assert input_row is not None
        member = session.scalar(
            select(PortfolioAssemblyInputMember).where(
                PortfolioAssemblyInputMember.input_id == input_row.id
            )
        )
        assert member is not None
        member.stressed_capacity = Decimal("NaN")

        with pytest.raises((IntegrityError, StatementError)):
            session.flush()


def test_assembly_runtime_unavailable_keeps_input_pending(
    engine, monkeypatch: pytest.MonkeyPatch
) -> None:
    pytest.importorskip("numpy")
    from portfolio_engine import engine as portfolio_engine

    with Session(engine) as session:
        input_row = persist_portfolio_input_evaluation(
            session,
            _result(_stage(session, _portfolio_facts(session)).id),
        )
        assert input_row is not None
        monkeypatch.setattr(portfolio_engine, "cp", None)

        with pytest.raises(QfError, match="PORTFOLIO_ASSEMBLY_ENGINE_UNAVAILABLE"):
            assemble_trusted_portfolio_input(session, input_row.id)

        assert input_row.state == "PENDING"
        assert session.scalar(select(func.count()).select_from(PortfolioCandidate)) == 0
