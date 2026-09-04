"""Pure contracts for deterministic Paper-to-Live promotion decisions."""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
from math import isfinite


class PromotionPurpose(StrEnum):
    ALPHA_DISCOVERY_TO_SEALED = "ALPHA_DISCOVERY_TO_SEALED"
    SEALED_TO_QUALIFIED = "SEALED_TO_QUALIFIED"
    PORTFOLIO_TO_PAPER = "PORTFOLIO_TO_PAPER"
    PAPER_TO_LIVE = "PAPER_TO_LIVE"


class LivePromotionMode(StrEnum):
    MANUAL_APPROVAL = "MANUAL_APPROVAL"
    AUTO_HANDOFF = "AUTO_HANDOFF"


class GateComparator(StrEnum):
    MINIMUM = "MINIMUM"
    MAXIMUM = "MAXIMUM"


class GateStatus(StrEnum):
    PASS = "PASS"
    FAIL = "FAIL"
    INCONCLUSIVE = "INCONCLUSIVE"
    INVALID = "INVALID"


class PromotionOutcome(StrEnum):
    WAITING_FOR_EVIDENCE = "WAITING_FOR_EVIDENCE"
    REJECTED = "REJECTED"
    MANUAL_LIVE_APPROVAL_READY = "MANUAL_LIVE_APPROVAL_READY"
    AUTO_LIVE_HANDOFF_AVAILABLE = "AUTO_LIVE_HANDOFF_AVAILABLE"


class PromotionAction(StrEnum):
    NONE = "NONE"
    CREATE_PENDING_LIVE_APPROVAL = "CREATE_PENDING_LIVE_APPROVAL"
    CREATE_SYSTEM_APPROVED_LIVE_HANDOFF = "CREATE_SYSTEM_APPROVED_LIVE_HANDOFF"


def _name(value: str, field_name: str) -> str:
    normalized = str(value).strip()
    if not normalized:
        raise ValueError(f"{field_name} must not be blank")
    return normalized


@dataclass(frozen=True, slots=True)
class RevisionRef:
    """An explicit immutable object reference; no derived identity is used."""

    id: str
    revision: int

    def __post_init__(self) -> None:
        object.__setattr__(self, "id", _name(self.id, "id"))
        if self.revision < 1:
            raise ValueError("revision must be positive")


@dataclass(frozen=True, slots=True)
class GateRequirement:
    """A required numeric metric threshold owned by a Promotion Policy version."""

    name: str
    metric: str
    comparator: GateComparator
    threshold: float

    def __post_init__(self) -> None:
        object.__setattr__(self, "name", _name(self.name, "name"))
        object.__setattr__(self, "metric", _name(self.metric, "metric"))
        object.__setattr__(self, "comparator", GateComparator(self.comparator))
        if not isfinite(self.threshold):
            raise ValueError("threshold must be finite")

    @property
    def expected(self) -> str:
        operator = ">=" if self.comparator is GateComparator.MINIMUM else "<="
        return f"{operator} {self.threshold:g}"


@dataclass(frozen=True, slots=True)
class PromotionPolicy:
    identity: RevisionRef
    purpose: PromotionPurpose
    live_promotion_mode: LivePromotionMode
    gates: tuple[GateRequirement, ...]

    def __post_init__(self) -> None:
        object.__setattr__(self, "purpose", PromotionPurpose(self.purpose))
        object.__setattr__(self, "live_promotion_mode", LivePromotionMode(self.live_promotion_mode))
        gates = tuple(self.gates)
        if not gates:
            raise ValueError("gates must not be empty")
        if len({gate.name for gate in gates}) != len(gates):
            raise ValueError("gate names must be unique")
        object.__setattr__(self, "gates", gates)


@dataclass(frozen=True, slots=True)
class MetricObservation:
    name: str
    value: float | None

    def __post_init__(self) -> None:
        object.__setattr__(self, "name", _name(self.name, "name"))
        if self.value is not None:
            if isinstance(self.value, bool):
                raise ValueError("metric values must be numeric")
            object.__setattr__(self, "value", float(self.value))


@dataclass(frozen=True, slots=True)
class PaperFeedback:
    """Frozen Paper feedback evidence, or its explicit absence/incompleteness."""

    identity: RevisionRef | None = None
    complete: bool = False
    contract_valid: bool = False
    data_quality_valid: bool = False

    def __post_init__(self) -> None:
        if self.complete and self.identity is None:
            raise ValueError("complete feedback requires an identity")
        if self.identity is None and (self.contract_valid or self.data_quality_valid):
            raise ValueError("feedback validity requires an identity")


@dataclass(frozen=True, slots=True)
class PromotionReadiness:
    candidate_current: bool
    candidate_package_current: bool
    promotion_policy_current: bool
    dataset_revisions_current: bool
    runtime_current: bool
    live_downstream_ready: bool
    active_degradation: bool


@dataclass(frozen=True, slots=True)
class PromotionBinding:
    """Immutable identities and revisions a promotion transaction re-checks atomically."""

    candidate_id: str
    candidate_package: RevisionRef
    promotion_policy: RevisionRef
    runtime: RevisionRef
    downstream: RevisionRef
    dataset_revisions: tuple[RevisionRef, ...]

    def __post_init__(self) -> None:
        object.__setattr__(self, "candidate_id", _name(self.candidate_id, "candidate_id"))
        dataset_revisions = tuple(self.dataset_revisions)
        if not dataset_revisions:
            raise ValueError("dataset_revisions must not be empty")
        if len(set(dataset_revisions)) != len(dataset_revisions):
            raise ValueError("dataset_revisions must be unique")
        object.__setattr__(self, "dataset_revisions", dataset_revisions)


@dataclass(frozen=True, slots=True)
class PromotionRequest:
    binding: PromotionBinding
    feedback: PaperFeedback
    readiness: PromotionReadiness
    metrics: tuple[MetricObservation, ...]

    def __post_init__(self) -> None:
        metrics = tuple(self.metrics)
        if len({metric.name for metric in metrics}) != len(metrics):
            raise ValueError("metric names must be unique")
        object.__setattr__(self, "metrics", metrics)


@dataclass(frozen=True, slots=True)
class PromotionActionIdentity:
    """Explicit fields suitable for a unique transactional retry constraint."""

    binding: PromotionBinding
    feedback: RevisionRef


@dataclass(frozen=True, slots=True)
class GateEvaluation:
    gate: str
    actual: float | bool | None
    expected: str
    status: GateStatus
    reason_code: str | None = None


@dataclass(frozen=True, slots=True)
class PromotionDecision:
    outcome: PromotionOutcome
    action: PromotionAction
    gates: tuple[GateEvaluation, ...]
    action_identity: PromotionActionIdentity | None = None

    def __post_init__(self) -> None:
        active_actions = {
            PromotionOutcome.MANUAL_LIVE_APPROVAL_READY: PromotionAction.CREATE_PENDING_LIVE_APPROVAL,
            PromotionOutcome.AUTO_LIVE_HANDOFF_AVAILABLE: PromotionAction.CREATE_SYSTEM_APPROVED_LIVE_HANDOFF,
        }
        expected_action = active_actions.get(self.outcome, PromotionAction.NONE)
        if self.action is not expected_action:
            raise ValueError("action does not match outcome")
        if (self.action is PromotionAction.NONE) != (self.action_identity is None):
            raise ValueError("only a promotion action may have an action identity")

    @property
    def reason_codes(self) -> tuple[str, ...]:
        return tuple(gate.reason_code for gate in self.gates if gate.reason_code is not None)
