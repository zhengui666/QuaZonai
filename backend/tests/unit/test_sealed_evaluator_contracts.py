from __future__ import annotations

from dataclasses import fields, replace
from datetime import UTC, datetime, timedelta, timezone
from uuid import uuid4

import pytest

from research_engine.sealed_evaluator_contracts import (
    AlphaEvaluationInput,
    AlphaForecast,
    AlphaSignalSummary,
    CalibrationMethod,
    DiscoveryCalibrationArtifact,
    DiscoveryEvaluationResult,
    DiscoveryEvaluationStatus,
    DisclosureClassification,
    DisclosureReasonCode,
    EvaluationPhase,
    EvaluationStatus,
    EvaluationSubject,
    GateCode,
    GateResult,
    GateStatus,
    ImmutableReference,
    LevelOneDisclosure,
    MetricAggregate,
    MetricCode,
    MetricStatus,
    PortfolioCovariance,
    PortfolioCovarianceMethod,
    PortfolioEvaluationInput,
    PortfolioInputAxis,
    PortfolioInputEvaluationInput,
    PortfolioInputEvaluationResult,
    SealedEvaluationResult,
    SealedEvaluator,
)


def _ref(revision: int = 1) -> ImmutableReference:
    return ImmutableReference(uuid4(), revision)


def _alpha_input() -> AlphaEvaluationInput:
    return AlphaEvaluationInput(
        assignment_id=uuid4(),
        episode_id=uuid4(),
        source_mission_artifact=_ref(),
        model_version=_ref(),
        calibration_version=_ref(),
        discovery_dataset=_ref(),
        validation_dataset=_ref(),
        sealed_dataset=_ref(),
        evaluation_design=_ref(),
        promotion_policy=_ref(),
    )


def _portfolio_input() -> PortfolioEvaluationInput:
    return PortfolioEvaluationInput(
        assignment_id=uuid4(),
        episode_id=uuid4(),
        candidate_id=uuid4(),
        candidate_family_id=uuid4(),
        previous_candidate_id=uuid4(),
        assembly_input_id=uuid4(),
        evaluation_dataset_selection=_ref(),
        sealed_dataset=_ref(),
        policy_version=_ref(),
        cause_event_id=1,
    )


def _portfolio_input_evaluation_input() -> PortfolioInputEvaluationInput:
    return PortfolioInputEvaluationInput(
        assignment_id=uuid4(),
        portfolio_program_id=uuid4(),
        mandate_version=_ref(),
        capital_context_version_id=uuid4(),
        evaluation_dataset_selection=_ref(),
        sealed_dataset=_ref(),
        promotion_policy=_ref(),
        cause_event_id=1,
        previous_candidate_id=None,
        as_of_time=datetime(2026, 9, 3, tzinfo=UTC),
        axes=(
            PortfolioInputAxis(0, uuid4(), uuid4(), uuid4(), "AAPL.XNAS"),
            PortfolioInputAxis(1, uuid4(), uuid4(), uuid4(), "MSFT.XNAS"),
        ),
    )


def _alpha_metrics() -> tuple[MetricAggregate, ...]:
    values = {
        MetricCode.ANNUALIZED_VOLATILITY: 0.1,
        MetricCode.COVERAGE: 0.9,
        MetricCode.HIT_RATE: 0.55,
        MetricCode.IC_MEAN: 0.02,
        MetricCode.MAX_DRAWDOWN: 0.1,
        MetricCode.NET_RETURN: 0.02,
        MetricCode.OBSERVATION_COUNT: 100.0,
        MetricCode.RANK_IC_MEAN: 0.02,
        MetricCode.SHARPE_RATIO: 1.2,
        MetricCode.TRIAL_ADJUSTED_SHARPE: None,
    }
    return tuple(
        MetricAggregate(
            EvaluationPhase.SEALED,
            code,
            MetricStatus.NOT_AVAILABLE if value is None else MetricStatus.AVAILABLE,
            value,
        )
        for code, value in sorted(values.items(), key=lambda item: item[0].value)
    )


def _alpha_gates(status: GateStatus = GateStatus.PASS) -> tuple[GateResult, ...]:
    return tuple(
        GateResult(
            code,
            status if code is GateCode.EVIDENCE_VALID else GateStatus.PASS,
            DisclosureReasonCode.EVIDENCE_INCOMPLETE
            if code is GateCode.EVIDENCE_VALID and status is not GateStatus.PASS
            else None,
        )
        for code in sorted(
            (
                GateCode.CALIBRATION_VALID,
                GateCode.EVIDENCE_VALID,
                GateCode.POINT_IN_TIME_VALID,
                GateCode.POLICY_VALID,
                GateCode.STATISTICAL_VALID,
            ),
            key=lambda code: code.value,
        )
    )


def _portfolio_metrics() -> tuple[MetricAggregate, ...]:
    values = {
        MetricCode.ANNUALIZED_VOLATILITY: 0.1,
        MetricCode.CAPACITY_UTILIZATION: 0.4,
        MetricCode.MATERIAL_IMPROVEMENT: 0.0,
        MetricCode.MAX_DRAWDOWN: 0.1,
        MetricCode.NET_RETURN: 0.02,
        MetricCode.OBSERVATION_COUNT: 100.0,
        MetricCode.SHARPE_RATIO: 1.2,
        MetricCode.TURNOVER: 0.05,
    }
    return tuple(
        MetricAggregate(EvaluationPhase.SEALED, code, MetricStatus.AVAILABLE, value)
        for code, value in sorted(values.items(), key=lambda item: item[0].value)
    )


def _portfolio_gates(status: GateStatus = GateStatus.PASS) -> tuple[GateResult, ...]:
    return tuple(
        GateResult(
            code,
            status if code is GateCode.MATERIAL_IMPROVEMENT_VALID else GateStatus.PASS,
            DisclosureReasonCode.MATERIAL_IMPROVEMENT_UNMET
            if code is GateCode.MATERIAL_IMPROVEMENT_VALID and status is not GateStatus.PASS
            else None,
        )
        for code in sorted(
            (
                GateCode.EVIDENCE_VALID,
                GateCode.MATERIAL_IMPROVEMENT_VALID,
                GateCode.POINT_IN_TIME_VALID,
                GateCode.POLICY_VALID,
            ),
            key=lambda code: code.value,
        )
    )


def _signal_and_forecast() -> tuple[AlphaSignalSummary, tuple[AlphaForecast, ...]]:
    current = datetime(2026, 9, 3, tzinfo=UTC)
    return (
        AlphaSignalSummary(
            row_count=1,
            event_start=current,
            event_end=current,
            available_start=current,
            available_end=current,
        ),
        (
            AlphaForecast(
                instrument_id="AAPL.XNAS",
                as_of_time=current,
                effective_from=current,
                effective_until=None,
                expected_return=0.02,
                uncertainty=0.1,
                confidence=0.8,
                max_trade_notional=10_000,
                max_position_notional=50_000,
                max_participation_rate=0.1,
                days_to_liquidate=2,
                stressed_capacity_notional=20_000,
            ),
        ),
    )


def _discovery_result(
    *,
    status: DiscoveryEvaluationStatus = DiscoveryEvaluationStatus.VALID,
    calibration: DiscoveryCalibrationArtifact | None = None,
) -> DiscoveryEvaluationResult:
    return DiscoveryEvaluationResult(
        discovery_evaluation_id=uuid4(),
        model_version=_ref(),
        status=status,
        private_result_id=uuid4(),
        evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
        metrics=(
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
                0.02,
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
        ),
        gates=(
            GateResult(GateCode.CALIBRATION_VALID, GateStatus.PASS),
            GateResult(GateCode.EVIDENCE_VALID, GateStatus.PASS),
            GateResult(GateCode.POINT_IN_TIME_VALID, GateStatus.PASS),
            GateResult(GateCode.POLICY_VALID, GateStatus.PASS),
            GateResult(GateCode.STATISTICAL_VALID, GateStatus.PASS),
        ),
        calibration=calibration,
    )


def test_alpha_result_accepts_only_opaque_references_and_aggregate_evidence() -> None:
    input = _alpha_input()
    signal, forecasts = _signal_and_forecast()
    result = SealedEvaluationResult(
        input=input,
        status=EvaluationStatus.PASS,
        private_result_id=uuid4(),
        evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
        metrics=_alpha_metrics(),
        gates=_alpha_gates(),
        disclosure=LevelOneDisclosure(DisclosureClassification.QUALIFIED),
        signal=signal,
        forecasts=forecasts,
    )

    assert input.subject is EvaluationSubject.ALPHA
    assert result.disclosure.reason_code is None
    assert all(metric.value is None or isinstance(metric.value, float) for metric in result.metrics)


def test_alpha_input_can_omit_calibration_without_faking_a_reference() -> None:
    input = replace(_alpha_input(), calibration_version=None)

    assert input.calibration_version is None


@pytest.mark.parametrize("value", (float("nan"), float("inf"), float("-inf")))
def test_metric_aggregate_rejects_nonfinite_values(value: float) -> None:
    with pytest.raises(ValueError, match="finite"):
        MetricAggregate(
            EvaluationPhase.DISCOVERY,
            MetricCode.IC_MEAN,
            MetricStatus.AVAILABLE,
            value,
        )


def test_discovery_result_is_typed_utc_and_can_carry_one_private_calibration_ref() -> None:
    calibration = DiscoveryCalibrationArtifact(
        method=CalibrationMethod.ISOTONIC,
        training_dataset=_ref(),
        private_artifact_ref=uuid4(),
    )
    result = _discovery_result(calibration=calibration)

    assert result.status is DiscoveryEvaluationStatus.VALID
    assert result.outcome_code == "VALID"
    assert result.evaluated_at.tzinfo is UTC
    assert result.calibration is calibration


def test_discovery_result_rejects_free_form_or_incoherent_evidence() -> None:
    calibration = DiscoveryCalibrationArtifact(
        method=CalibrationMethod.ISOTONIC,
        training_dataset=_ref(),
        private_artifact_ref=uuid4(),
    )
    with pytest.raises(ValueError, match="only a valid"):
        _discovery_result(
            status=DiscoveryEvaluationStatus.INCONCLUSIVE,
            calibration=calibration,
        )
    with pytest.raises(ValueError, match="UTC"):
        replace(
            _discovery_result(),
            evaluated_at=datetime(2026, 9, 3, tzinfo=timezone(timedelta(hours=8))),
        )
    with pytest.raises(ValueError, match="discovery phase"):
        replace(
            _discovery_result(),
            metrics=(
                MetricAggregate(
                    EvaluationPhase.SEALED,
                    MetricCode.IC_MEAN,
                    MetricStatus.AVAILABLE,
                    0.02,
                ),
            ),
        )
    with pytest.raises(ValueError, match="fixed Alpha gate set"):
        replace(_discovery_result(), gates=(GateResult(GateCode.EVIDENCE_VALID, GateStatus.PASS),))


def test_portfolio_result_uses_the_same_protocol_and_categorical_rejection() -> None:
    input = _portfolio_input()
    result = SealedEvaluationResult(
        input=input,
        status=EvaluationStatus.FAIL,
        private_result_id=uuid4(),
        evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
        metrics=_portfolio_metrics(),
        gates=_portfolio_gates(GateStatus.FAIL),
        disclosure=LevelOneDisclosure(
            DisclosureClassification.REJECTED,
            DisclosureReasonCode.MATERIAL_IMPROVEMENT_UNMET,
        ),
    )

    assert input.subject is EvaluationSubject.PORTFOLIO
    assert result.gates[1].reason_code is DisclosureReasonCode.MATERIAL_IMPROVEMENT_UNMET


def test_portfolio_input_evaluation_contract_is_complete_typed_and_finite() -> None:
    input = _portfolio_input_evaluation_input()
    result = PortfolioInputEvaluationResult(
        input=input,
        private_result_id=uuid4(),
        evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
        covariance_method=PortfolioCovarianceMethod.EWMA_SHRINKAGE,
        covariance_observations=20,
        covariance_decay=0.5,
        covariance_shrinkage=0.1,
        covariance_upper_triangle=(
            PortfolioCovariance(0, 0, 0.04),
            PortfolioCovariance(0, 1, 0.01),
            PortfolioCovariance(1, 1, 0.03),
        ),
    )

    assert result.assignment_id == input.assignment_id
    assert result.covariance_method is PortfolioCovarianceMethod.EWMA_SHRINKAGE
    assert result.evaluated_at.tzinfo is UTC


@pytest.mark.parametrize("value", (float("nan"), float("inf"), float("-inf")))
def test_portfolio_input_covariance_contract_rejects_nonfinite_or_incomplete_evidence(
    value: float,
) -> None:
    input = _portfolio_input_evaluation_input()
    with pytest.raises(ValueError, match="finite"):
        PortfolioCovariance(0, 0, value)
    with pytest.raises(ValueError, match="complete and ordered"):
        PortfolioInputEvaluationResult(
            input=input,
            private_result_id=uuid4(),
            evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
            covariance_method=PortfolioCovarianceMethod.EWMA_SHRINKAGE,
            covariance_observations=20,
            covariance_decay=0.5,
            covariance_shrinkage=0.1,
            covariance_upper_triangle=(PortfolioCovariance(0, 0, 0.04),),
        )


def test_portfolio_contract_rejects_free_methods_current_pointer_and_partial_sets() -> None:
    input = _portfolio_input_evaluation_input()
    with pytest.raises(ValueError):
        PortfolioInputEvaluationResult(
            input=input,
            private_result_id=uuid4(),
            evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
            covariance_method="OTHER",  # type: ignore[arg-type]
            covariance_observations=20,
            covariance_decay=0.5,
            covariance_shrinkage=0.1,
            covariance_upper_triangle=(
                PortfolioCovariance(0, 0, 0.04),
                PortfolioCovariance(0, 1, 0.01),
                PortfolioCovariance(1, 1, 0.03),
            ),
        )
    with pytest.raises(ValueError):
        GateCode("CANDIDATE_CURRENT")
    with pytest.raises(ValueError, match="fixed Portfolio metric set"):
        SealedEvaluationResult(
            input=_portfolio_input(),
            status=EvaluationStatus.FAIL,
            private_result_id=uuid4(),
            evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
            metrics=_portfolio_metrics()[:-1],
            gates=_portfolio_gates(GateStatus.FAIL),
            disclosure=LevelOneDisclosure(
                DisclosureClassification.REJECTED,
                DisclosureReasonCode.MATERIAL_IMPROVEMENT_UNMET,
            ),
        )
    with pytest.raises(ValueError, match="fixed Portfolio gate set"):
        SealedEvaluationResult(
            input=_portfolio_input(),
            status=EvaluationStatus.FAIL,
            private_result_id=uuid4(),
            evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
            metrics=_portfolio_metrics(),
            gates=_portfolio_gates(GateStatus.FAIL)[:-1],
            disclosure=LevelOneDisclosure(
                DisclosureClassification.REJECTED,
                DisclosureReasonCode.MATERIAL_IMPROVEMENT_UNMET,
            ),
        )


def test_contract_cannot_accept_raw_or_location_bearing_inputs() -> None:
    with pytest.raises(TypeError, match="UUID"):
        ImmutableReference("https://sealed.example/data", 1)  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="assignment_id must be a UUID"):
        replace(_alpha_input(), assignment_id="not-a-uuid")  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="candidate_id must be a UUID"):
        replace(_portfolio_input(), candidate_id=_ref())  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="unexpected keyword"):
        AlphaEvaluationInput(**{  # type: ignore[arg-type,call-arg]
            **{field.name: _ref() for field in fields(AlphaEvaluationInput)},
            "raw_returns": (),
        })
    with pytest.raises(ValueError, match="RAW_RETURN"):
        MetricAggregate(EvaluationPhase.SEALED, "RAW_RETURN", MetricStatus.AVAILABLE, 0.1)  # type: ignore[arg-type]

    forbidden = {"raw_signals", "raw_returns", "dataset_data", "url", "secret", "orders"}
    contract_fields = {
        field.name
        for contract in (
            AlphaEvaluationInput,
            PortfolioEvaluationInput,
            PortfolioInputEvaluationInput,
            PortfolioInputEvaluationResult,
            PortfolioInputAxis,
            PortfolioCovariance,
            SealedEvaluationResult,
            DiscoveryEvaluationResult,
            DiscoveryCalibrationArtifact,
            AlphaSignalSummary,
            AlphaForecast,
        )
        for field in fields(contract)
    }
    assert forbidden.isdisjoint(contract_fields)
    assert {
        "assignment_id",
        "episode_id",
        "candidate_id",
        "candidate_family_id",
        "assembly_input_id",
        "cause_event_id",
        "private_result_id",
    }.issubset(contract_fields)
    assert not {"assignment_revision", "episode_revision", "candidate_revision", "private_result_revision"} & contract_fields
    assert "current_candidate_id" not in contract_fields


def test_result_rejects_nondeterministic_or_inconsistent_output() -> None:
    with pytest.raises(ValueError, match="ordered"):
        SealedEvaluationResult(
            input=_alpha_input(),
            status=EvaluationStatus.PASS,
            private_result_id=uuid4(),
            evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
            metrics=(
                MetricAggregate(
                    EvaluationPhase.SEALED,
                    MetricCode.SHARPE_RATIO,
                    MetricStatus.AVAILABLE,
                    1.0,
                ),
                MetricAggregate(
                    EvaluationPhase.SEALED,
                    MetricCode.NET_RETURN,
                    MetricStatus.AVAILABLE,
                    0.1,
                ),
            ),
            gates=(GateResult(GateCode.EVIDENCE_VALID, GateStatus.PASS),),
            disclosure=LevelOneDisclosure(DisclosureClassification.QUALIFIED),
        )
    with pytest.raises(ValueError, match="PASS evaluation"):
        SealedEvaluationResult(
            input=_portfolio_input(),
            status=EvaluationStatus.PASS,
            private_result_id=uuid4(),
            evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
            metrics=_portfolio_metrics(),
            gates=_portfolio_gates(GateStatus.FAIL),
            disclosure=LevelOneDisclosure(DisclosureClassification.QUALIFIED),
        )


class _EchoEvaluator:
    def evaluate(self, input: AlphaEvaluationInput | PortfolioEvaluationInput, /) -> SealedEvaluationResult:
        metrics = (
            _alpha_metrics()
            if isinstance(input, AlphaEvaluationInput)
            else _portfolio_metrics()
        )
        gates = (
            _alpha_gates(GateStatus.INCONCLUSIVE)
            if isinstance(input, AlphaEvaluationInput)
            else _portfolio_gates(GateStatus.INCONCLUSIVE)
        )
        return SealedEvaluationResult(
            input=input,
            status=EvaluationStatus.INCONCLUSIVE,
            private_result_id=uuid4(),
            evaluated_at=datetime(2026, 9, 3, tzinfo=UTC),
            metrics=metrics,
            gates=gates,
            disclosure=LevelOneDisclosure(
                DisclosureClassification.INCONCLUSIVE,
                DisclosureReasonCode.EVIDENCE_INCOMPLETE,
            ),
        )


def test_one_protocol_is_structurally_reusable_by_both_consumers() -> None:
    evaluator = _EchoEvaluator()

    assert isinstance(evaluator, SealedEvaluator)
    assert evaluator.evaluate(_alpha_input()).input.subject is EvaluationSubject.ALPHA
    assert evaluator.evaluate(_portfolio_input()).input.subject is EvaluationSubject.PORTFOLIO
