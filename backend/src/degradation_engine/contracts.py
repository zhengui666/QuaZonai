"""Typed, execution-free contracts for forward-evidence degradation policy."""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
from math import isfinite


class SubjectType(StrEnum):
    ALPHA = "ALPHA"
    PORTFOLIO = "PORTFOLIO"


class HealthState(StrEnum):
    HEALTHY = "HEALTHY"
    WATCH = "WATCH"
    DEGRADING = "DEGRADING"
    FAILED = "FAILED"
    RECOVERED = "RECOVERED"


class ProgramState(StrEnum):
    ACTIVE = "ACTIVE"
    COOLING = "COOLING"
    APPROVAL_PENDING = "APPROVAL_PENDING"
    WAITING_FOR_FEEDBACK = "WAITING_FOR_FEEDBACK"
    BLOCKED = "BLOCKED"
    PAUSED = "PAUSED"
    ARCHIVED = "ARCHIVED"


class WakeDisposition(StrEnum):
    READY = "READY"
    PENDING = "PENDING"


def _required_text(value: str, field_name: str) -> None:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{field_name} must be non-empty")


def _fraction(value: float, field_name: str) -> None:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not isfinite(value):
        raise ValueError(f"{field_name} must be finite")
    if not 0.0 <= value <= 1.0:
        raise ValueError(f"{field_name} must be in [0, 1]")


@dataclass(frozen=True, slots=True)
class ForwardEvidence:
    """A trusted, pre-scored alpha or portfolio observation from one evidence source."""

    source_id: str
    program_id: str
    subject_type: SubjectType
    subject_id: str
    metric_name: str
    severity: float
    confidence: float = 1.0

    def __post_init__(self) -> None:
        for field_name in ("source_id", "program_id", "subject_id", "metric_name"):
            _required_text(getattr(self, field_name), field_name)
        _fraction(self.severity, "severity")
        _fraction(self.confidence, "confidence")


@dataclass(frozen=True, slots=True)
class DegradationPolicy:
    """Versioned thresholds; upstream evaluators own metric scoring, not this policy."""

    policy_revision: str
    watch_threshold: float = 0.25
    degrading_threshold: float = 0.50
    failed_threshold: float = 0.80
    recovery_threshold: float = 0.10
    minimum_confidence: float = 0.80
    minimum_consecutive_breaches: int = 1

    def __post_init__(self) -> None:
        _required_text(self.policy_revision, "policy_revision")
        for field_name in (
            "watch_threshold",
            "degrading_threshold",
            "failed_threshold",
            "recovery_threshold",
            "minimum_confidence",
        ):
            _fraction(getattr(self, field_name), field_name)
        if not (
            self.recovery_threshold
            <= self.watch_threshold
            <= self.degrading_threshold
            <= self.failed_threshold
        ):
            raise ValueError("thresholds must satisfy recovery <= watch <= degrading <= failed")
        if self.minimum_consecutive_breaches < 1:
            raise ValueError("minimum_consecutive_breaches must be at least one")


@dataclass(frozen=True, slots=True)
class HealthSnapshot:
    state: HealthState = HealthState.HEALTHY
    consecutive_breaches: int = 0

    def __post_init__(self) -> None:
        if self.consecutive_breaches < 0:
            raise ValueError("consecutive_breaches must be non-negative")


@dataclass(frozen=True, slots=True)
class DegradationObservation:
    source_id: str
    subject_type: SubjectType
    subject_id: str
    metric_name: str
    severity: float
    confidence: float
    policy_revision: str
    reason_code: str
    state: HealthState
    consecutive_breaches: int
    evaluated: bool


@dataclass(frozen=True, slots=True)
class WakeRequest:
    """A research wake request, never a downstream execution command."""

    program_id: str
    subject_type: SubjectType
    subject_id: str
    source_id: str
    policy_revision: str
    reason_code: str

    def __post_init__(self) -> None:
        for field_name in (
            "program_id",
            "subject_id",
            "source_id",
            "policy_revision",
            "reason_code",
        ):
            _required_text(getattr(self, field_name), field_name)

    @property
    def deduplication_fields(self) -> tuple[str, str, str, str, str, str]:
        """Structured causal identity for a database unique constraint; never a hash."""
        return (
            self.program_id,
            self.subject_type.value,
            self.subject_id,
            self.source_id,
            self.policy_revision,
            self.reason_code,
        )


@dataclass(frozen=True, slots=True)
class ScheduledWake:
    request: WakeRequest
    disposition: WakeDisposition


@dataclass(frozen=True, slots=True)
class EvaluationResult:
    state: HealthState
    snapshot: HealthSnapshot
    observation: DegradationObservation
    wake_request: WakeRequest | None = None
