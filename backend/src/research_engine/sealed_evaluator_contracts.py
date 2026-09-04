"""Opaque-reference contracts shared by Alpha and Portfolio sealed evaluators."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from enum import StrEnum
from math import isfinite
from typing import Protocol, TypeAlias, runtime_checkable
from uuid import UUID


class EvaluationSubject(StrEnum):
    ALPHA = "ALPHA"
    PORTFOLIO = "PORTFOLIO"


class EvaluationStatus(StrEnum):
    PASS = "PASS"
    FAIL = "FAIL"
    INCONCLUSIVE = "INCONCLUSIVE"
    INVALID = "INVALID"


class DiscoveryEvaluationStatus(StrEnum):
    """The only terminal outcomes a Discovery evaluator may return."""

    VALID = "VALID"
    INCONCLUSIVE = "INCONCLUSIVE"
    INVALID = "INVALID"


class CalibrationMethod(StrEnum):
    """The sole V1 evaluator-produced calibration method."""

    ISOTONIC = "ISOTONIC"


class PortfolioCovarianceMethod(StrEnum):
    """The one V1 covariance method accepted at the trusted boundary."""

    EWMA_SHRINKAGE = "EWMA_SHRINKAGE"


class EvaluationPhase(StrEnum):
    DISCOVERY = "DISCOVERY"
    VALIDATION = "VALIDATION"
    SEALED = "SEALED"


class MetricStatus(StrEnum):
    AVAILABLE = "AVAILABLE"
    NOT_AVAILABLE = "NOT_AVAILABLE"


class MetricCode(StrEnum):
    OBSERVATION_COUNT = "OBSERVATION_COUNT"
    COVERAGE = "COVERAGE"
    IC_MEAN = "IC_MEAN"
    RANK_IC_MEAN = "RANK_IC_MEAN"
    HIT_RATE = "HIT_RATE"
    NET_RETURN = "NET_RETURN"
    ANNUALIZED_VOLATILITY = "ANNUALIZED_VOLATILITY"
    SHARPE_RATIO = "SHARPE_RATIO"
    MAX_DRAWDOWN = "MAX_DRAWDOWN"
    TRIAL_ADJUSTED_SHARPE = "TRIAL_ADJUSTED_SHARPE"
    TURNOVER = "TURNOVER"
    CAPACITY_UTILIZATION = "CAPACITY_UTILIZATION"
    MATERIAL_IMPROVEMENT = "MATERIAL_IMPROVEMENT"


class GateCode(StrEnum):
    EVIDENCE_VALID = "EVIDENCE_VALID"
    POINT_IN_TIME_VALID = "POINT_IN_TIME_VALID"
    CALIBRATION_VALID = "CALIBRATION_VALID"
    STATISTICAL_VALID = "STATISTICAL_VALID"
    POLICY_VALID = "POLICY_VALID"
    MATERIAL_IMPROVEMENT_VALID = "MATERIAL_IMPROVEMENT_VALID"


class GateStatus(StrEnum):
    PASS = "PASS"
    FAIL = "FAIL"
    INCONCLUSIVE = "INCONCLUSIVE"
    INVALID = "INVALID"


class DisclosureClassification(StrEnum):
    QUALIFIED = "QUALIFIED"
    REJECTED = "REJECTED"
    INCONCLUSIVE = "INCONCLUSIVE"
    INVALID = "INVALID"


class DisclosureReasonCode(StrEnum):
    INSUFFICIENT_NET_EDGE = "INSUFFICIENT_NET_EDGE"
    TEMPORAL_INSTABILITY = "TEMPORAL_INSTABILITY"
    REGIME_INSTABILITY = "REGIME_INSTABILITY"
    CALIBRATION_FAILURE = "CALIBRATION_FAILURE"
    POINT_IN_TIME_FAILURE = "POINT_IN_TIME_FAILURE"
    POLICY_GATE_FAILURE = "POLICY_GATE_FAILURE"
    MATERIAL_IMPROVEMENT_UNMET = "MATERIAL_IMPROVEMENT_UNMET"
    EVIDENCE_INCOMPLETE = "EVIDENCE_INCOMPLETE"
    INVALID_EVALUATOR_RESULT = "INVALID_EVALUATOR_RESULT"


ALPHA_METRIC_CODES = frozenset(
    {
        MetricCode.OBSERVATION_COUNT,
        MetricCode.COVERAGE,
        MetricCode.IC_MEAN,
        MetricCode.RANK_IC_MEAN,
        MetricCode.HIT_RATE,
        MetricCode.NET_RETURN,
        MetricCode.ANNUALIZED_VOLATILITY,
        MetricCode.SHARPE_RATIO,
        MetricCode.MAX_DRAWDOWN,
        MetricCode.TRIAL_ADJUSTED_SHARPE,
    }
)
_PORTFOLIO_METRICS = frozenset(
    {
        MetricCode.OBSERVATION_COUNT,
        MetricCode.NET_RETURN,
        MetricCode.ANNUALIZED_VOLATILITY,
        MetricCode.SHARPE_RATIO,
        MetricCode.MAX_DRAWDOWN,
        MetricCode.TURNOVER,
        MetricCode.CAPACITY_UTILIZATION,
        MetricCode.MATERIAL_IMPROVEMENT,
    }
)
PORTFOLIO_METRIC_CODES = _PORTFOLIO_METRICS
_ALPHA_GATES = frozenset(
    {
        GateCode.EVIDENCE_VALID,
        GateCode.POINT_IN_TIME_VALID,
        GateCode.CALIBRATION_VALID,
        GateCode.STATISTICAL_VALID,
        GateCode.POLICY_VALID,
    }
)
_PORTFOLIO_GATES = frozenset(
    {
        GateCode.EVIDENCE_VALID,
        GateCode.POINT_IN_TIME_VALID,
        GateCode.POLICY_VALID,
        GateCode.MATERIAL_IMPROVEMENT_VALID,
    }
)


def _uuid(value: object, field_name: str) -> None:
    if not isinstance(value, UUID):
        raise TypeError(f"{field_name} must be a UUID")


def _uuid_fields(value: object, field_names: tuple[str, ...]) -> None:
    for field_name in field_names:
        _uuid(getattr(value, field_name), field_name)


def _positive_integer(value: object, field_name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{field_name} must be an integer")
    if value < 1:
        raise ValueError(f"{field_name} must be positive")
    return value


def _reference(value: object, field_name: str) -> None:
    if not isinstance(value, ImmutableReference):
        raise TypeError(f"{field_name} must be an ImmutableReference")


def _reference_fields(value: object, field_names: tuple[str, ...]) -> None:
    for field_name in field_names:
        _reference(getattr(value, field_name), field_name)


def _utc_datetime(value: object, field_name: str) -> datetime:
    if not isinstance(value, datetime):
        raise TypeError(f"{field_name} must be a datetime")
    if value.tzinfo is None or value.utcoffset() != timedelta(0):
        raise ValueError(f"{field_name} must be UTC")
    return value.astimezone(UTC)


def _finite_float(value: object, field_name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError(f"{field_name} must be numeric")
    numeric = float(value)
    if not isfinite(numeric):
        raise ValueError(f"{field_name} must be finite")
    return numeric


@dataclass(frozen=True, slots=True)
class ImmutableReference:
    """An explicit UUID/revision identity for a versioned object, never a URL or payload."""

    id: UUID
    revision: int

    def __post_init__(self) -> None:
        if not isinstance(self.id, UUID):
            raise TypeError("id must be a UUID")
        if isinstance(self.revision, bool) or not isinstance(self.revision, int):
            raise TypeError("revision must be an integer")
        if self.revision < 1:
            raise ValueError("revision must be positive")


@dataclass(frozen=True, slots=True)
class AlphaEvaluationInput:
    """Frozen Alpha evaluator references; the evaluator resolves all private inputs itself."""

    assignment_id: UUID
    episode_id: UUID
    source_mission_artifact: ImmutableReference
    model_version: ImmutableReference
    # Only an evaluator-returned Discovery calibration may occupy this reference.
    # A RELATIVE_SCORE model can therefore form a CalibratedAlphaFrame without
    # creating a parallel relabelled model version.
    calibration_version: ImmutableReference | None
    discovery_dataset: ImmutableReference
    validation_dataset: ImmutableReference
    sealed_dataset: ImmutableReference
    evaluation_design: ImmutableReference
    promotion_policy: ImmutableReference

    def __post_init__(self) -> None:
        _uuid_fields(self, ("assignment_id", "episode_id"))
        _reference_fields(
            self,
            (
                "source_mission_artifact",
                "model_version",
                "discovery_dataset",
                "validation_dataset",
                "sealed_dataset",
                "evaluation_design",
                "promotion_policy",
            ),
        )
        if self.calibration_version is not None:
            _reference(self.calibration_version, "calibration_version")

    @property
    def subject(self) -> EvaluationSubject:
        return EvaluationSubject.ALPHA


@dataclass(frozen=True, slots=True)
class PortfolioInputAxis:
    """One frozen, complete Alpha axis for covariance evaluation."""

    axis_index: int
    alpha_qualification_id: UUID
    alpha_evaluation_result_id: UUID
    alpha_signal_artifact_id: UUID
    instrument_id: str

    def __post_init__(self) -> None:
        if isinstance(self.axis_index, bool) or not isinstance(self.axis_index, int):
            raise TypeError("axis_index must be an integer")
        if self.axis_index < 0:
            raise ValueError("axis_index must not be negative")
        _uuid_fields(
            self,
            (
                "alpha_qualification_id",
                "alpha_evaluation_result_id",
                "alpha_signal_artifact_id",
            ),
        )
        if not isinstance(self.instrument_id, str):
            raise TypeError("instrument_id must be text")
        instrument_id = self.instrument_id.strip()
        if not instrument_id:
            raise ValueError("instrument_id must not be blank")
        object.__setattr__(self, "instrument_id", instrument_id)


@dataclass(frozen=True, slots=True)
class PortfolioInputEvaluationInput:
    """Frozen opaque references for the sole V1 covariance evaluator."""

    assignment_id: UUID
    portfolio_program_id: UUID
    mandate_version: ImmutableReference
    capital_context_version_id: UUID
    evaluation_dataset_selection: ImmutableReference
    sealed_dataset: ImmutableReference
    promotion_policy: ImmutableReference
    cause_event_id: int
    previous_candidate_id: UUID | None
    as_of_time: datetime
    axes: tuple[PortfolioInputAxis, ...]

    def __post_init__(self) -> None:
        _uuid_fields(
            self,
            ("assignment_id", "portfolio_program_id", "capital_context_version_id"),
        )
        if self.previous_candidate_id is not None:
            _uuid(self.previous_candidate_id, "previous_candidate_id")
        _reference_fields(
            self,
            (
                "mandate_version",
                "evaluation_dataset_selection",
                "sealed_dataset",
                "promotion_policy",
            ),
        )
        _positive_integer(self.cause_event_id, "cause_event_id")
        object.__setattr__(self, "as_of_time", _utc_datetime(self.as_of_time, "as_of_time"))
        axes = tuple(self.axes)
        if len(axes) < 2:
            raise ValueError("Portfolio input evaluation requires at least two axes")
        if not all(isinstance(axis, PortfolioInputAxis) for axis in axes):
            raise TypeError("axes must contain PortfolioInputAxis values")
        axis_indices = tuple(axis.axis_index for axis in axes)
        qualification_ids = tuple(axis.alpha_qualification_id for axis in axes)
        instrument_ids = tuple(axis.instrument_id for axis in axes)
        if axis_indices != tuple(range(len(axes))):
            raise ValueError("axes must be ordered by contiguous axis_index")
        if len(set(qualification_ids)) != len(qualification_ids):
            raise ValueError("axes must have unique alpha qualifications")
        if len(set(instrument_ids)) != len(instrument_ids):
            raise ValueError("axes must have unique instruments")
        object.__setattr__(self, "axes", axes)


@dataclass(frozen=True, slots=True)
class PortfolioCovariance:
    """One finite upper-triangle covariance cell, never a matrix payload."""

    left_axis_index: int
    right_axis_index: int
    covariance: float

    def __post_init__(self) -> None:
        for field_name in ("left_axis_index", "right_axis_index"):
            value = getattr(self, field_name)
            if isinstance(value, bool) or not isinstance(value, int):
                raise TypeError(f"{field_name} must be an integer")
        if self.left_axis_index < 0 or self.right_axis_index < self.left_axis_index:
            raise ValueError("covariance must use a nonnegative upper-triangle index")
        object.__setattr__(self, "covariance", _finite_float(self.covariance, "covariance"))


@dataclass(frozen=True, slots=True)
class PortfolioInputEvaluationResult:
    """Typed covariance evidence bound to one frozen Portfolio input assignment."""

    input: PortfolioInputEvaluationInput
    private_result_id: UUID
    evaluated_at: datetime
    covariance_method: PortfolioCovarianceMethod
    covariance_observations: int
    covariance_decay: float
    covariance_shrinkage: float
    covariance_upper_triangle: tuple[PortfolioCovariance, ...]

    def __post_init__(self) -> None:
        if not isinstance(self.input, PortfolioInputEvaluationInput):
            raise TypeError("input must be a PortfolioInputEvaluationInput")
        _uuid(self.private_result_id, "private_result_id")
        object.__setattr__(self, "evaluated_at", _utc_datetime(self.evaluated_at, "evaluated_at"))
        object.__setattr__(
            self,
            "covariance_method",
            PortfolioCovarianceMethod(self.covariance_method),
        )
        _positive_integer(self.covariance_observations, "covariance_observations")
        if self.covariance_observations < 2:
            raise ValueError("covariance_observations must be at least two")
        decay = _finite_float(self.covariance_decay, "covariance_decay")
        shrinkage = _finite_float(self.covariance_shrinkage, "covariance_shrinkage")
        if not 0 < decay < 1:
            raise ValueError("covariance_decay must be between zero and one")
        if not 0 <= shrinkage <= 1:
            raise ValueError("covariance_shrinkage must be between zero and one")
        object.__setattr__(self, "covariance_decay", decay)
        object.__setattr__(self, "covariance_shrinkage", shrinkage)

        covariance_upper_triangle = tuple(self.covariance_upper_triangle)
        if not all(isinstance(item, PortfolioCovariance) for item in covariance_upper_triangle):
            raise TypeError("covariance_upper_triangle must contain PortfolioCovariance values")
        axis_count = len(self.input.axes)
        expected = tuple(
            (left_axis_index, right_axis_index)
            for left_axis_index in range(axis_count)
            for right_axis_index in range(left_axis_index, axis_count)
        )
        keys = tuple(
            (item.left_axis_index, item.right_axis_index) for item in covariance_upper_triangle
        )
        if keys != expected:
            raise ValueError("covariance_upper_triangle must be complete and ordered")
        if any(
            item.left_axis_index == item.right_axis_index and item.covariance < 0
            for item in covariance_upper_triangle
        ):
            raise ValueError("covariance diagonal must not be negative")
        object.__setattr__(self, "covariance_upper_triangle", covariance_upper_triangle)

    @property
    def assignment_id(self) -> UUID:
        """Compatibility identity for the Core writer; the input remains authoritative."""

        return self.input.assignment_id


@dataclass(frozen=True, slots=True)
class PortfolioEvaluationInput:
    """Frozen Portfolio references: immutable facts use UUIDs; versions use explicit revisions."""

    assignment_id: UUID
    episode_id: UUID
    candidate_id: UUID
    candidate_family_id: UUID
    previous_candidate_id: UUID | None
    assembly_input_id: UUID
    evaluation_dataset_selection: ImmutableReference
    sealed_dataset: ImmutableReference
    policy_version: ImmutableReference
    cause_event_id: int

    def __post_init__(self) -> None:
        _uuid_fields(
            self,
            ("assignment_id", "episode_id", "candidate_id", "candidate_family_id", "assembly_input_id"),
        )
        if self.previous_candidate_id is not None:
            _uuid(self.previous_candidate_id, "previous_candidate_id")
            if self.previous_candidate_id == self.candidate_id:
                raise ValueError("previous_candidate_id must differ from candidate_id")
        _reference_fields(
            self,
            (
                "evaluation_dataset_selection",
                "sealed_dataset",
                "policy_version",
            ),
        )
        _positive_integer(self.cause_event_id, "cause_event_id")

    @property
    def subject(self) -> EvaluationSubject:
        return EvaluationSubject.PORTFOLIO


SealedEvaluationInput: TypeAlias = AlphaEvaluationInput | PortfolioEvaluationInput


@dataclass(frozen=True, slots=True)
class MetricAggregate:
    """One finite aggregate only; a signal frame, raw return series, or dataset cannot fit."""

    phase: EvaluationPhase
    code: MetricCode
    status: MetricStatus
    value: float | None

    def __post_init__(self) -> None:
        object.__setattr__(self, "phase", EvaluationPhase(self.phase))
        object.__setattr__(self, "code", MetricCode(self.code))
        object.__setattr__(self, "status", MetricStatus(self.status))
        if self.status is MetricStatus.NOT_AVAILABLE:
            if self.value is not None:
                raise ValueError("NOT_AVAILABLE metric must not have a value")
            return
        if isinstance(self.value, bool) or not isinstance(self.value, (int, float)):
            raise TypeError("AVAILABLE metric value must be numeric")
        value = float(self.value)
        if not isfinite(value):
            raise ValueError("AVAILABLE metric value must be finite")
        object.__setattr__(self, "value", value)


@dataclass(frozen=True, slots=True)
class GateResult:
    """A categorical evaluator gate; policy thresholds remain in the frozen assignment."""

    code: GateCode
    status: GateStatus
    reason_code: DisclosureReasonCode | None = None

    def __post_init__(self) -> None:
        object.__setattr__(self, "code", GateCode(self.code))
        object.__setattr__(self, "status", GateStatus(self.status))
        if self.reason_code is not None:
            object.__setattr__(self, "reason_code", DisclosureReasonCode(self.reason_code))
        if (self.status is GateStatus.PASS) != (self.reason_code is None):
            raise ValueError("only a passing gate may omit a reason code")


@dataclass(frozen=True, slots=True)
class DiscoveryCalibrationArtifact:
    """An evaluator-private calibration artifact tied to the frozen Discovery Dataset."""

    method: CalibrationMethod
    training_dataset: ImmutableReference
    private_artifact_ref: UUID

    def __post_init__(self) -> None:
        object.__setattr__(self, "method", CalibrationMethod(self.method))
        _reference(self.training_dataset, "training_dataset")
        _uuid(self.private_artifact_ref, "private_artifact_ref")


@dataclass(frozen=True, slots=True)
class DiscoveryEvaluationResult:
    """Typed terminal Discovery evidence; it carries no raw data, URI, or job payload."""

    discovery_evaluation_id: UUID
    model_version: ImmutableReference
    status: DiscoveryEvaluationStatus
    private_result_id: UUID
    evaluated_at: datetime
    metrics: tuple[MetricAggregate, ...]
    gates: tuple[GateResult, ...]
    calibration: DiscoveryCalibrationArtifact | None = None

    def __post_init__(self) -> None:
        _uuid(self.discovery_evaluation_id, "discovery_evaluation_id")
        _reference(self.model_version, "model_version")
        object.__setattr__(self, "status", DiscoveryEvaluationStatus(self.status))
        _uuid(self.private_result_id, "private_result_id")
        object.__setattr__(self, "evaluated_at", _utc_datetime(self.evaluated_at, "evaluated_at"))

        metrics = tuple(self.metrics)
        gates = tuple(self.gates)
        if not metrics:
            raise ValueError("metrics must not be empty")
        if not gates:
            raise ValueError("gates must not be empty")
        if not all(isinstance(metric, MetricAggregate) for metric in metrics):
            raise TypeError("metrics must contain MetricAggregate values")
        if not all(isinstance(gate, GateResult) for gate in gates):
            raise TypeError("gates must contain GateResult values")
        metric_keys = tuple((metric.phase.value, metric.code.value) for metric in metrics)
        gate_codes = tuple(gate.code.value for gate in gates)
        if len(set(metric_keys)) != len(metric_keys):
            raise ValueError("metric phase/code pairs must be unique")
        if len(set(gate_codes)) != len(gate_codes):
            raise ValueError("gate codes must be unique")
        if metric_keys != tuple(sorted(metric_keys)):
            raise ValueError("metrics must be ordered by phase and code")
        if gate_codes != tuple(sorted(gate_codes)):
            raise ValueError("gates must be ordered by code")
        if any(metric.phase is not EvaluationPhase.DISCOVERY for metric in metrics):
            raise ValueError("Discovery metrics must use the discovery phase")
        if frozenset(metric.code for metric in metrics) != ALPHA_METRIC_CODES:
            raise ValueError("Discovery evaluation requires the fixed Alpha metric set")
        if frozenset(gate.code for gate in gates) != _ALPHA_GATES:
            raise ValueError("Discovery evaluation requires the fixed Alpha gate set")
        object.__setattr__(self, "metrics", metrics)
        object.__setattr__(self, "gates", gates)

        if self.calibration is not None and not isinstance(
            self.calibration, DiscoveryCalibrationArtifact
        ):
            raise TypeError("calibration must be a DiscoveryCalibrationArtifact")
        if self.status is not DiscoveryEvaluationStatus.VALID and self.calibration is not None:
            raise ValueError("only a valid Discovery evaluation may include calibration")
        statuses = {gate.status for gate in gates}
        if self.status is DiscoveryEvaluationStatus.VALID and statuses != {GateStatus.PASS}:
            raise ValueError("VALID Discovery evaluation requires only passing gates")
        if self.status is DiscoveryEvaluationStatus.INCONCLUSIVE and (
            GateStatus.INCONCLUSIVE not in statuses
            or GateStatus.FAIL in statuses
            or GateStatus.INVALID in statuses
        ):
            raise ValueError(
                "INCONCLUSIVE Discovery evaluation requires passing or inconclusive gates"
            )
        if self.status is DiscoveryEvaluationStatus.INVALID and GateStatus.INVALID not in statuses:
            raise ValueError("INVALID Discovery evaluation requires an invalid gate")

    @property
    def outcome_code(self) -> str:
        """Persist the fixed terminal outcome without accepting a free-form string."""

        return self.status.value


@dataclass(frozen=True, slots=True)
class LevelOneDisclosure:
    """The only evaluator result intended for Codex: categorical and metric-free."""

    classification: DisclosureClassification
    reason_code: DisclosureReasonCode | None = None

    def __post_init__(self) -> None:
        object.__setattr__(self, "classification", DisclosureClassification(self.classification))
        if self.reason_code is not None:
            object.__setattr__(self, "reason_code", DisclosureReasonCode(self.reason_code))
        if (self.classification is DisclosureClassification.QUALIFIED) != (self.reason_code is None):
            raise ValueError("only a qualified disclosure may omit a reason code")


_DISCLOSURE_FOR_STATUS = {
    EvaluationStatus.PASS: DisclosureClassification.QUALIFIED,
    EvaluationStatus.FAIL: DisclosureClassification.REJECTED,
    EvaluationStatus.INCONCLUSIVE: DisclosureClassification.INCONCLUSIVE,
    EvaluationStatus.INVALID: DisclosureClassification.INVALID,
}


@dataclass(frozen=True, slots=True)
class AlphaSignalSummary:
    """The bounded metadata needed to persist one evaluator-private signal artifact."""

    row_count: int
    event_start: datetime
    event_end: datetime
    available_start: datetime
    available_end: datetime

    def __post_init__(self) -> None:
        if isinstance(self.row_count, bool) or not isinstance(self.row_count, int):
            raise TypeError("row_count must be an integer")
        if self.row_count <= 0:
            raise ValueError("row_count must be positive")
        event_start = _utc_datetime(self.event_start, "event_start")
        event_end = _utc_datetime(self.event_end, "event_end")
        available_start = _utc_datetime(self.available_start, "available_start")
        available_end = _utc_datetime(self.available_end, "available_end")
        if event_start > event_end:
            raise ValueError("event_start must not be after event_end")
        if available_start > available_end:
            raise ValueError("available_start must not be after available_end")
        if event_start > available_start or event_end > available_end:
            raise ValueError("signal summary must preserve point-in-time ordering")
        object.__setattr__(self, "event_start", event_start)
        object.__setattr__(self, "event_end", event_end)
        object.__setattr__(self, "available_start", available_start)
        object.__setattr__(self, "available_end", available_end)


@dataclass(frozen=True, slots=True)
class AlphaForecast:
    """One finite Alpha forecast aggregate; never a signal frame or URI."""

    instrument_id: str
    as_of_time: datetime
    effective_from: datetime
    effective_until: datetime | None
    expected_return: float
    uncertainty: float
    confidence: float
    max_trade_notional: float
    max_position_notional: float
    max_participation_rate: float
    days_to_liquidate: float
    stressed_capacity_notional: float

    def __post_init__(self) -> None:
        if not isinstance(self.instrument_id, str):
            raise TypeError("instrument_id must be text")
        instrument_id = self.instrument_id.strip()
        if not instrument_id:
            raise ValueError("instrument_id must not be blank")
        as_of_time = _utc_datetime(self.as_of_time, "as_of_time")
        effective_from = _utc_datetime(self.effective_from, "effective_from")
        effective_until = (
            _utc_datetime(self.effective_until, "effective_until")
            if self.effective_until is not None
            else None
        )
        if as_of_time > effective_from:
            raise ValueError("as_of_time must not be after effective_from")
        if effective_until is not None and effective_until < effective_from:
            raise ValueError("effective_until must not be before effective_from")

        expected_return = _finite_float(self.expected_return, "expected_return")
        uncertainty = _finite_float(self.uncertainty, "uncertainty")
        confidence = _finite_float(self.confidence, "confidence")
        max_trade_notional = _finite_float(self.max_trade_notional, "max_trade_notional")
        max_position_notional = _finite_float(self.max_position_notional, "max_position_notional")
        max_participation_rate = _finite_float(
            self.max_participation_rate, "max_participation_rate"
        )
        days_to_liquidate = _finite_float(self.days_to_liquidate, "days_to_liquidate")
        stressed_capacity_notional = _finite_float(
            self.stressed_capacity_notional, "stressed_capacity_notional"
        )
        if uncertainty < 0:
            raise ValueError("uncertainty must not be negative")
        if not 0 <= confidence <= 1:
            raise ValueError("confidence must be between zero and one")
        if max_trade_notional <= 0 or max_position_notional <= 0:
            raise ValueError("capacity notionals must be positive")
        if not 0 <= max_participation_rate <= 1:
            raise ValueError("max_participation_rate must be between zero and one")
        if days_to_liquidate <= 0 or stressed_capacity_notional <= 0:
            raise ValueError("capacity envelope must be positive")
        object.__setattr__(self, "instrument_id", instrument_id)
        object.__setattr__(self, "as_of_time", as_of_time)
        object.__setattr__(self, "effective_from", effective_from)
        object.__setattr__(self, "effective_until", effective_until)
        object.__setattr__(self, "expected_return", expected_return)
        object.__setattr__(self, "uncertainty", uncertainty)
        object.__setattr__(self, "confidence", confidence)
        object.__setattr__(self, "max_trade_notional", max_trade_notional)
        object.__setattr__(self, "max_position_notional", max_position_notional)
        object.__setattr__(self, "max_participation_rate", max_participation_rate)
        object.__setattr__(self, "days_to_liquidate", days_to_liquidate)
        object.__setattr__(self, "stressed_capacity_notional", stressed_capacity_notional)


@dataclass(frozen=True, slots=True)
class SealedEvaluationResult:
    """Validated private aggregates plus one deterministic Level-1 disclosure."""

    input: SealedEvaluationInput
    status: EvaluationStatus
    private_result_id: UUID
    evaluated_at: datetime
    metrics: tuple[MetricAggregate, ...]
    gates: tuple[GateResult, ...]
    disclosure: LevelOneDisclosure
    signal: AlphaSignalSummary | None = None
    forecasts: tuple[AlphaForecast, ...] = ()

    def __post_init__(self) -> None:
        if not isinstance(self.input, (AlphaEvaluationInput, PortfolioEvaluationInput)):
            raise TypeError("input must be an AlphaEvaluationInput or PortfolioEvaluationInput")
        object.__setattr__(self, "status", EvaluationStatus(self.status))
        _uuid(self.private_result_id, "private_result_id")
        object.__setattr__(self, "evaluated_at", _utc_datetime(self.evaluated_at, "evaluated_at"))
        metrics = tuple(self.metrics)
        gates = tuple(self.gates)
        if not metrics:
            raise ValueError("metrics must not be empty")
        if not gates:
            raise ValueError("gates must not be empty")
        if not all(isinstance(metric, MetricAggregate) for metric in metrics):
            raise TypeError("metrics must contain MetricAggregate values")
        if not all(isinstance(gate, GateResult) for gate in gates):
            raise TypeError("gates must contain GateResult values")
        metric_keys = tuple((metric.phase.value, metric.code.value) for metric in metrics)
        gate_codes = tuple(gate.code.value for gate in gates)
        if len(set(metric_keys)) != len(metric_keys):
            raise ValueError("metric phase/code pairs must be unique")
        if len(set(gate_codes)) != len(gate_codes):
            raise ValueError("gate codes must be unique")
        if metric_keys != tuple(sorted(metric_keys)):
            raise ValueError("metrics must be ordered by phase and code")
        if gate_codes != tuple(sorted(gate_codes)):
            raise ValueError("gates must be ordered by code")
        if isinstance(self.input, AlphaEvaluationInput):
            if any(metric.phase is not EvaluationPhase.SEALED for metric in metrics):
                raise ValueError("Alpha metrics must be sealed")
            if frozenset(metric.code for metric in metrics) != ALPHA_METRIC_CODES:
                raise ValueError("Alpha evaluation requires the fixed Alpha metric set")
            if frozenset(gate.code for gate in gates) != _ALPHA_GATES:
                raise ValueError("Alpha evaluation requires the fixed Alpha gate set")
        else:
            if any(metric.phase is not EvaluationPhase.SEALED for metric in metrics):
                raise ValueError("Portfolio metrics must be sealed")
            if frozenset(metric.code for metric in metrics) != _PORTFOLIO_METRICS:
                raise ValueError("Portfolio evaluation requires the fixed Portfolio metric set")
            if frozenset(gate.code for gate in gates) != _PORTFOLIO_GATES:
                raise ValueError("Portfolio evaluation requires the fixed Portfolio gate set")
        object.__setattr__(self, "metrics", metrics)
        object.__setattr__(self, "gates", gates)
        if not isinstance(self.disclosure, LevelOneDisclosure):
            raise TypeError("disclosure must be a LevelOneDisclosure")
        if self.disclosure.classification is not _DISCLOSURE_FOR_STATUS[self.status]:
            raise ValueError("disclosure classification must match status")
        signal = self.signal
        if signal is not None and not isinstance(signal, AlphaSignalSummary):
            raise TypeError("signal must be an AlphaSignalSummary")
        forecasts = tuple(self.forecasts)
        if not all(isinstance(forecast, AlphaForecast) for forecast in forecasts):
            raise TypeError("forecasts must contain AlphaForecast values")
        instrument_ids = tuple(forecast.instrument_id for forecast in forecasts)
        if len(set(instrument_ids)) != len(instrument_ids):
            raise ValueError("forecast instruments must be unique")
        if instrument_ids != tuple(sorted(instrument_ids)):
            raise ValueError("forecasts must be ordered by instrument")
        if isinstance(self.input, AlphaEvaluationInput):
            if self.status is EvaluationStatus.PASS:
                if signal is None or len(forecasts) != 1:
                    raise ValueError("PASS Alpha evaluation requires one signal and one forecast")
                if forecasts[0].as_of_time < signal.available_end:
                    raise ValueError(
                        "forecast as_of_time must not precede signal available_end"
                    )
            elif signal is not None or forecasts:
                raise ValueError("non-PASS Alpha evaluation must not contain signal or forecasts")
        elif signal is not None or forecasts:
            raise ValueError("Portfolio evaluation must not contain Alpha signal or forecasts")
        object.__setattr__(self, "signal", signal)
        object.__setattr__(self, "forecasts", forecasts)
        statuses = {gate.status for gate in gates}
        if self.status is EvaluationStatus.PASS and statuses != {GateStatus.PASS}:
            raise ValueError("PASS evaluation requires only passing gates")
        if self.status is EvaluationStatus.FAIL and (
            GateStatus.FAIL not in statuses or GateStatus.INVALID in statuses
        ):
            raise ValueError("FAIL evaluation requires a failed, non-invalid gate")
        if self.status is EvaluationStatus.INCONCLUSIVE and (
            GateStatus.INCONCLUSIVE not in statuses
            or GateStatus.FAIL in statuses
            or GateStatus.INVALID in statuses
        ):
            raise ValueError("INCONCLUSIVE evaluation requires only passing or inconclusive gates")
        if self.status is EvaluationStatus.INVALID and GateStatus.INVALID not in statuses:
            raise ValueError("INVALID evaluation requires an invalid gate")


@runtime_checkable
class SealedEvaluator(Protocol):
    """The same isolated evaluator boundary serves Alpha and Portfolio assignments."""

    def evaluate(self, input: SealedEvaluationInput, /) -> SealedEvaluationResult: ...


__all__ = [
    "ALPHA_METRIC_CODES",
    "PORTFOLIO_METRIC_CODES",
    "AlphaForecast",
    "AlphaEvaluationInput",
    "AlphaSignalSummary",
    "CalibrationMethod",
    "DiscoveryCalibrationArtifact",
    "DiscoveryEvaluationResult",
    "DiscoveryEvaluationStatus",
    "DisclosureClassification",
    "DisclosureReasonCode",
    "EvaluationPhase",
    "EvaluationStatus",
    "EvaluationSubject",
    "GateCode",
    "GateResult",
    "GateStatus",
    "ImmutableReference",
    "LevelOneDisclosure",
    "MetricAggregate",
    "MetricCode",
    "MetricStatus",
    "PortfolioCovariance",
    "PortfolioCovarianceMethod",
    "PortfolioEvaluationInput",
    "PortfolioInputAxis",
    "PortfolioInputEvaluationInput",
    "PortfolioInputEvaluationResult",
    "SealedEvaluationInput",
    "SealedEvaluationResult",
    "SealedEvaluator",
]
