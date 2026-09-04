"""Deterministic health evaluation and research-wake routing."""

from .contracts import (
    DegradationObservation,
    DegradationPolicy,
    EvaluationResult,
    ForwardEvidence,
    HealthSnapshot,
    HealthState,
    ProgramState,
    ScheduledWake,
    SubjectType,
    WakeDisposition,
    WakeRequest,
)
from .policy import (
    allows_auto_live_promotion,
    auto_execution_allowed,
    evaluate_degradation,
    has_active_degradation,
    reconcile_wake,
    schedule_wake,
)

__all__ = [
    "DegradationObservation",
    "DegradationPolicy",
    "EvaluationResult",
    "ForwardEvidence",
    "HealthSnapshot",
    "HealthState",
    "ProgramState",
    "ScheduledWake",
    "SubjectType",
    "WakeDisposition",
    "WakeRequest",
    "allows_auto_live_promotion",
    "auto_execution_allowed",
    "evaluate_degradation",
    "has_active_degradation",
    "reconcile_wake",
    "schedule_wake",
]
