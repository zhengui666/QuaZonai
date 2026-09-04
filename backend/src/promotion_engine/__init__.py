"""Deterministic, execution-free Promotion Policy decisions."""

from .contracts import (
    GateComparator,
    GateEvaluation,
    GateRequirement,
    GateStatus,
    LivePromotionMode,
    MetricObservation,
    PaperFeedback,
    PromotionAction,
    PromotionActionIdentity,
    PromotionBinding,
    PromotionDecision,
    PromotionOutcome,
    PromotionPolicy,
    PromotionPurpose,
    PromotionReadiness,
    PromotionRequest,
    RevisionRef,
)
from .engine import evaluate_paper_to_live, evaluate_policy_gates

__all__ = [
    "GateComparator",
    "GateEvaluation",
    "GateRequirement",
    "GateStatus",
    "LivePromotionMode",
    "MetricObservation",
    "PaperFeedback",
    "PromotionAction",
    "PromotionActionIdentity",
    "PromotionBinding",
    "PromotionDecision",
    "PromotionOutcome",
    "PromotionPolicy",
    "PromotionPurpose",
    "PromotionReadiness",
    "PromotionRequest",
    "RevisionRef",
    "evaluate_paper_to_live",
    "evaluate_policy_gates",
]
