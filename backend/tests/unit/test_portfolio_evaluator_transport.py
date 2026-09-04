from __future__ import annotations

import json
from datetime import UTC, datetime
from uuid import uuid4

import pytest

from errors import QfError
from research_engine.sealed_evaluator_contracts import (
    ImmutableReference,
    PortfolioEvaluationInput,
    PortfolioInputAxis,
    PortfolioInputEvaluationInput,
)
from research_engine.trusted_evaluator_service import (
    _PortfolioEvaluationRequest,
    _PortfolioInputRequest,
    parse_portfolio_evaluation_result,
    parse_portfolio_input_evaluation_result,
)


def _input() -> PortfolioInputEvaluationInput:
    return PortfolioInputEvaluationInput(
        assignment_id=uuid4(),
        portfolio_program_id=uuid4(),
        mandate_version=ImmutableReference(uuid4(), 1),
        capital_context_version_id=uuid4(),
        evaluation_dataset_selection=ImmutableReference(uuid4(), 1),
        sealed_dataset=ImmutableReference(uuid4(), 3),
        promotion_policy=ImmutableReference(uuid4(), 2),
        cause_event_id=42,
        previous_candidate_id=None,
        as_of_time=datetime(2026, 9, 3, tzinfo=UTC),
        axes=(
            PortfolioInputAxis(0, uuid4(), uuid4(), uuid4(), "US:A"),
            PortfolioInputAxis(1, uuid4(), uuid4(), uuid4(), "US:B"),
        ),
    )


def test_portfolio_input_result_parser_accepts_only_fixed_covariance_shape() -> None:
    request_input = _input()
    request = _PortfolioInputRequest(
        descriptor={},
        input=request_input,
    )
    payload = {
        "kind": "PORTFOLIO_INPUT_EVALUATION",
        "assignment_id": str(request_input.assignment_id),
        "private_result_id": str(uuid4()),
        "evaluated_at": "2026-09-03T00:00:00Z",
        "covariance_method": "EWMA_SHRINKAGE",
        "covariance_observations": 20,
        "covariance_decay": 0.5,
        "covariance_shrinkage": 0.1,
        "covariance_upper_triangle": [
            {"left_axis_index": 0, "right_axis_index": 0, "covariance": 0.04},
            {"left_axis_index": 0, "right_axis_index": 1, "covariance": 0.01},
            {"left_axis_index": 1, "right_axis_index": 1, "covariance": 0.03},
        ],
    }
    result = parse_portfolio_input_evaluation_result(json.dumps(payload).encode(), request)
    assert result.assignment_id == request_input.assignment_id
    assert result.covariance_method.value == "EWMA_SHRINKAGE"
    assert len(result.covariance_upper_triangle) == 3

    payload["covariance_method"] = "SAMPLE_COVARIANCE"
    with pytest.raises(QfError, match="TRUSTED_EVALUATOR_RESULT_INVALID"):
        parse_portfolio_input_evaluation_result(json.dumps(payload).encode(), request)


def test_portfolio_evaluation_parser_rejects_extra_fields_and_requires_exact_sets() -> None:
    request_input = PortfolioEvaluationInput(
        assignment_id=uuid4(),
        episode_id=uuid4(),
        candidate_id=uuid4(),
        candidate_family_id=uuid4(),
        previous_candidate_id=None,
        assembly_input_id=uuid4(),
        evaluation_dataset_selection=ImmutableReference(uuid4(), 1),
        sealed_dataset=ImmutableReference(uuid4(), 3),
        policy_version=ImmutableReference(uuid4(), 2),
        cause_event_id=42,
    )
    request = _PortfolioEvaluationRequest(descriptor={}, input=request_input)
    metric_codes = (
        "ANNUALIZED_VOLATILITY",
        "CAPACITY_UTILIZATION",
        "MATERIAL_IMPROVEMENT",
        "MAX_DRAWDOWN",
        "NET_RETURN",
        "OBSERVATION_COUNT",
        "SHARPE_RATIO",
        "TURNOVER",
    )
    payload = {
        "kind": "PORTFOLIO_EVALUATION",
        "status": "PASS",
        "private_result_id": str(uuid4()),
        "evaluated_at": "2026-09-03T00:00:00Z",
        "metrics": [
            {
                "phase": "SEALED",
                "code": code,
                "status": "AVAILABLE",
                "value": 0.0,
            }
            for code in metric_codes
        ],
        "gates": [
            {"code": code, "status": "PASS", "reason_code": None}
            for code in (
                "EVIDENCE_VALID",
                "MATERIAL_IMPROVEMENT_VALID",
                "POINT_IN_TIME_VALID",
                "POLICY_VALID",
            )
        ],
        "disclosure": {"classification": "QUALIFIED", "reason_code": None},
    }
    result = parse_portfolio_evaluation_result(json.dumps(payload).encode(), request)
    assert result.input == request_input
    assert result.status.value == "PASS"

    payload["unexpected"] = True
    with pytest.raises(QfError, match="TRUSTED_EVALUATOR_RESULT_INVALID"):
        parse_portfolio_evaluation_result(json.dumps(payload).encode(), request)
