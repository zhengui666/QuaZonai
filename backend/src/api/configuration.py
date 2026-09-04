"""Canonical, fail-closed administration APIs for a fresh QuaZonai install."""

from __future__ import annotations

from collections.abc import Callable
from datetime import UTC, datetime, timedelta
from decimal import Decimal
from typing import Any, Literal
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, Header, Query, Request
from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    StrictBool,
    StrictInt,
    StrictStr,
    field_validator,
    model_validator,
)
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from api.dependencies import get_session
from db.models import (
    AlphaQualification,
    DataQualityResult,
    DatasetRevision,
    DownstreamSystem,
    EvaluationDatasetSelection,
    EvaluationDesignVersion,
    GovernedDataSource,
    Job,
    MarketUniverseVersion,
    NautilusCatalogBinding,
    CapitalContextVersion,
    PortfolioMandate,
    PortfolioMandateVersion,
    PreflightReceipt,
    PromotionPolicyGate,
    PromotionPolicyVersion,
    PublicMutationReceipt,
)
from downstream_auth import authenticate_downstream, install_service_token, issue_service_token
from downstream_contracts import feedback_contract_snapshot
from errors import QfError
from events import append_event
from jobs import enqueue_job
from research_engine.sealed_evaluator_contracts import ALPHA_METRIC_CODES


router = APIRouter(prefix="/api/v1", tags=["configuration"])

_FORBIDDEN_PUBLIC_FIELDS = frozenset(
    {
        "access_token",
        "api_key",
        "apikey",
        "password",
        "private_key",
        "refresh_token",
        "secret",
        "secret_key",
        "service_token",
        "token",
    }
)


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class UniverseSpecInput(StrictModel):
    name: str = Field(min_length=1, max_length=200)
    instrument_schema: dict[str, Any] = Field(default_factory=dict)
    membership_rules: dict[str, Any] = Field(default_factory=dict)
    calendar_semantics: dict[str, Any] = Field(default_factory=dict)
    currency_semantics: dict[str, Any] = Field(default_factory=dict)
    data_requirements: dict[str, Any] = Field(default_factory=dict)
    risk_model_family: str = Field(min_length=1, max_length=80)
    cost_model_family: str = Field(min_length=1, max_length=80)
    capacity_model_family: str = Field(min_length=1, max_length=80)
    allowed_alpha_roles: list[str] = Field(default_factory=list)
    downstream_compatibility: list[str] = Field(default_factory=list)
    state: Literal["ACTIVE", "INACTIVE"] = "ACTIVE"

    @model_validator(mode="after")
    def require_semantics(self) -> "UniverseSpecInput":
        for name in (
            "instrument_schema",
            "membership_rules",
            "calendar_semantics",
            "currency_semantics",
            "data_requirements",
        ):
            if not getattr(self, name):
                raise ValueError(f"{name} must not be empty")
        return self


class CreateUniverseInput(UniverseSpecInput):
    universe_key: str = Field(
        min_length=1, max_length=120, pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$"
    )


class UniverseView(StrictModel):
    id: UUID
    universe_key: str
    version_no: int
    name: str
    state: str
    spec: dict[str, Any]
    created_at: datetime


class UniversePage(StrictModel):
    items: list[UniverseView]
    next_cursor: UUID | None = None


class CreateDataSourceInput(StrictModel):
    name: str = Field(min_length=1, max_length=200)
    connector_key: str = Field(
        min_length=1, max_length=80, pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$"
    )
    provider: str = Field(min_length=1, max_length=200)
    universe_scope: list[UUID] = Field(min_length=1)
    field_schema: dict[str, Any] = Field(default_factory=dict)
    license_classification: str = Field(min_length=1, max_length=80)
    availability_semantics: dict[str, Any] = Field(default_factory=dict)
    update_cadence: str | None = Field(default=None, max_length=100)
    public_config: dict[str, Any] = Field(default_factory=dict)

    @model_validator(mode="after")
    def require_governance_metadata(self) -> "CreateDataSourceInput":
        if not self.field_schema or not self.availability_semantics:
            raise ValueError("field_schema and availability_semantics must not be empty")
        return self


class DataSourceView(StrictModel):
    id: UUID
    name: str
    connector_key: str
    provider: str | None = None
    state: str
    universe_scope: list[str]
    field_schema: dict[str, Any]
    license_classification: str
    availability_semantics: dict[str, Any]
    update_cadence: str | None = None
    preflight_state: str
    public_config: dict[str, Any]
    created_at: datetime
    updated_at: datetime


class DataSourcePage(StrictModel):
    items: list[DataSourceView]
    next_cursor: UUID | None = None


class DataSourcePreflightInput(StrictModel):
    """Intentionally empty: preflight consumes only registered source facts."""


class DatasetMaterializationInput(StrictModel):
    data_source_id: UUID
    universe_version_id: UUID
    partition: Literal["DISCOVERY", "VALIDATION", "SEALED", "FORWARD"]
    data_class: Literal["SYNTHETIC", "FIXTURE", "VENDOR", "PRODUCTION"]
    origin: str = Field(min_length=1, max_length=200)
    schema_version: str = Field(min_length=1, max_length=100)
    data_type: str = Field(min_length=1, max_length=120)
    instrument_scope: list[str] = Field(min_length=1)
    event_start: datetime
    event_end: datetime
    available_start: datetime
    available_end: datetime
    quality_requirements: dict[str, Any] = Field(default_factory=dict)
    point_in_time_requirements: dict[str, Any] = Field(default_factory=dict)
    sealed_catalog_uri: str | None = Field(
        default=None,
        pattern=r"^catalog://[A-Za-z0-9][A-Za-z0-9._-]{0,119}$",
    )

    @model_validator(mode="after")
    def require_point_in_time_ranges(self) -> "DatasetMaterializationInput":
        times = (
            self.event_start,
            self.event_end,
            self.available_start,
            self.available_end,
        )
        if any(value.tzinfo is None for value in times):
            raise ValueError("dataset timestamps must include an offset")
        if self.event_start > self.event_end or self.available_start > self.available_end:
            raise ValueError("dataset ranges must be ordered")
        if self.event_start > self.available_start or self.event_end > self.available_end:
            raise ValueError("available time must not precede event time")
        if not self.quality_requirements or not self.point_in_time_requirements:
            raise ValueError("quality and point-in-time requirements must not be empty")
        if self.partition == "SEALED" and self.sealed_catalog_uri is None:
            raise ValueError("SEALED materialization requires sealed_catalog_uri")
        if self.partition != "SEALED" and self.sealed_catalog_uri is not None:
            raise ValueError("sealed_catalog_uri is only valid for SEALED materialization")
        return self


class DatasetView(StrictModel):
    id: UUID
    data_source_id: UUID | None = None
    universe_version_id: UUID | None = None
    universe_name: str | None = None
    revision_no: int
    partition: str
    data_class: str | None = None
    origin: str | None = None
    promotability: str | None = None
    schema_version: str | None = None
    event_start: datetime | None = None
    event_end: datetime | None = None
    available_start: datetime | None = None
    available_end: datetime | None = None
    row_count: int | None = None
    quality_state: str
    point_in_time_state: str
    materialization_request: dict[str, Any]
    created_at: datetime


class DatasetPage(StrictModel):
    items: list[DatasetView]
    next_cursor: UUID | None = None


class DatasetQualityResultView(StrictModel):
    id: UUID
    check_kind: str
    revision_no: int
    state: str
    summary: dict[str, Any]
    checker_version: str
    created_at: datetime


class DatasetQualityView(StrictModel):
    dataset_revision_id: UUID
    quality_state: str
    point_in_time_state: str
    promotability: str | None = None
    results: list[DatasetQualityResultView]


class DatasetProfileView(StrictModel):
    dataset_revision_id: UUID
    data_type: str | None = None
    instrument_scope: list[str]
    event_start: datetime | None = None
    event_end: datetime | None = None
    available_start: datetime | None = None
    available_end: datetime | None = None


def _decimal_string(value: object) -> object:
    if not isinstance(value, str):
        raise ValueError("must be a decimal string")
    return value


class EvaluationDatasetSelectionInput(StrictModel):
    universe_version_id: UUID
    discovery_dataset_revision_id: UUID
    validation_dataset_revision_id: UUID
    sealed_dataset_revision_id: UUID
    state: Literal["ENABLED"]

    @model_validator(mode="after")
    def require_distinct_dataset_revisions(self) -> "EvaluationDatasetSelectionInput":
        revisions = {
            self.discovery_dataset_revision_id,
            self.validation_dataset_revision_id,
            self.sealed_dataset_revision_id,
        }
        if len(revisions) != 3:
            raise ValueError("evaluation dataset revisions must be distinct")
        return self


class EvaluationDatasetSelectionView(StrictModel):
    id: UUID
    universe_version_id: UUID
    version_no: int
    discovery_dataset_revision_id: UUID
    validation_dataset_revision_id: UUID
    sealed_dataset_revision_id: UUID
    state: str
    created_at: datetime


class EvaluationDatasetSelectionPage(StrictModel):
    items: list[EvaluationDatasetSelectionView]
    next_cursor: UUID | None = None


_DESIGN_DECIMAL_FIELDS = ("annualization_factor", "qualification_threshold")
_DESIGN_TEXT_FIELDS = (
    "contract_version",
    "qualification_metric_code",
    "pass_disclosure_code",
    "failure_disclosure_code",
    "inconclusive_disclosure_code",
    "invalid_disclosure_code",
)


def _required_text(value: str) -> str:
    normalized = value.strip()
    if not normalized:
        raise ValueError("must not be blank")
    return normalized


class EvaluationDesignVersionInput(StrictModel):
    universe_version_id: UUID
    contract_version: StrictStr = Field(min_length=1, max_length=40)
    allowed_model_mode: Literal["RELATIVE_SCORE"]
    qualification_role: Literal[
        "PRIMARY_ALPHA",
        "DIVERSIFIER_ALPHA",
        "HEDGE_ALPHA",
        "REGIME_SIGNAL",
        "RISK_MODULATOR",
        "SHADOW_ALPHA",
    ]
    walk_forward_folds: StrictInt = Field(ge=1)
    annualization_factor: Decimal
    multiple_testing_method: Literal["BONFERRONI", "BENJAMINI_HOCHBERG"]
    multiple_testing_max_trials: StrictInt = Field(ge=1)
    qualification_metric_code: StrictStr = Field(min_length=1, max_length=100)
    qualification_comparator: Literal["MINIMUM", "MAXIMUM"]
    qualification_threshold: Decimal
    pass_disclosure_code: StrictStr = Field(min_length=1, max_length=100)
    failure_disclosure_code: StrictStr = Field(min_length=1, max_length=100)
    inconclusive_disclosure_code: StrictStr = Field(min_length=1, max_length=100)
    invalid_disclosure_code: StrictStr = Field(min_length=1, max_length=100)
    state: Literal["ACTIVE"]

    @field_validator(*_DESIGN_TEXT_FIELDS)
    @classmethod
    def require_nonblank_text(cls, value: str) -> str:
        return _required_text(value)

    @field_validator("qualification_metric_code")
    @classmethod
    def require_sealed_alpha_metric(cls, value: str) -> str:
        if value not in ALPHA_METRIC_CODES:
            raise ValueError("must be a supported sealed Alpha metric")
        return value

    @field_validator(*_DESIGN_DECIMAL_FIELDS, mode="before")
    @classmethod
    def require_decimal_strings(cls, value: object) -> object:
        return _decimal_string(value)

    @model_validator(mode="after")
    def require_finite_statistics(self) -> "EvaluationDesignVersionInput":
        if not self.annualization_factor.is_finite() or self.annualization_factor <= 0:
            raise ValueError("annualization_factor must be a positive finite decimal")
        if not self.qualification_threshold.is_finite():
            raise ValueError("qualification_threshold must be a finite decimal")
        return self


class EvaluationDesignVersionView(StrictModel):
    id: UUID
    version_no: int
    universe_version_id: UUID
    contract_version: str
    allowed_model_mode: str
    qualification_role: str
    walk_forward_folds: int
    annualization_factor: Decimal
    multiple_testing_method: str
    multiple_testing_max_trials: int
    qualification_metric_code: str
    qualification_comparator: str
    qualification_threshold: Decimal
    pass_disclosure_code: str
    failure_disclosure_code: str
    inconclusive_disclosure_code: str
    invalid_disclosure_code: str
    state: str
    created_at: datetime


class EvaluationDesignVersionPage(StrictModel):
    items: list[EvaluationDesignVersionView]
    next_cursor: UUID | None = None


class PromotionPolicyGateInput(StrictModel):
    metric_code: StrictStr = Field(min_length=1, max_length=100)
    comparator: Literal["MINIMUM", "MAXIMUM"]
    threshold: Decimal
    ordinal: StrictInt = Field(ge=1)

    @field_validator("metric_code")
    @classmethod
    def require_nonblank_metric(cls, value: str) -> str:
        return _required_text(value)

    @field_validator("threshold", mode="before")
    @classmethod
    def require_decimal_string(cls, value: object) -> object:
        return _decimal_string(value)

    @model_validator(mode="after")
    def require_finite_threshold(self) -> "PromotionPolicyGateInput":
        if not self.threshold.is_finite():
            raise ValueError("threshold must be a finite decimal")
        return self


class PromotionPolicyVersionInput(StrictModel):
    purpose: Literal[
        "ALPHA_DISCOVERY_TO_SEALED",
        "SEALED_TO_QUALIFIED",
        "PORTFOLIO_TO_PAPER",
        "PAPER_TO_LIVE",
    ]
    mode: Literal["MANUAL_APPROVAL", "AUTO_HANDOFF"]
    gates: list[PromotionPolicyGateInput] = Field(min_length=1)
    state: Literal["ACTIVE"]

    @model_validator(mode="after")
    def require_ordered_supported_gates(self) -> "PromotionPolicyVersionInput":
        if len({gate.metric_code for gate in self.gates}) != len(self.gates):
            raise ValueError("policy gate metric_code values must be unique")
        ordinals = {gate.ordinal for gate in self.gates}
        if ordinals != set(range(1, len(self.gates) + 1)):
            raise ValueError("policy gate ordinals must be contiguous from one")
        if self.purpose in {"ALPHA_DISCOVERY_TO_SEALED", "SEALED_TO_QUALIFIED"} and any(
            gate.metric_code not in ALPHA_METRIC_CODES for gate in self.gates
        ):
            raise ValueError("Alpha policy gates must use supported sealed Alpha metrics")
        return self


class PromotionPolicyGateView(StrictModel):
    metric_code: str
    comparator: str
    threshold: Decimal
    ordinal: int


class PromotionPolicyVersionView(StrictModel):
    id: UUID
    version_no: int
    purpose: str
    mode: str
    policy_contract_version: str | None = None
    paper_downstream_system_id: UUID | None = None
    live_downstream_system_id: UUID | None = None
    gates: list[PromotionPolicyGateView]
    state: str
    created_at: datetime


class PromotionPolicyVersionPage(StrictModel):
    items: list[PromotionPolicyVersionView]
    next_cursor: UUID | None = None


class OperationView(StrictModel):
    id: UUID
    kind: str
    resource_type: str
    resource_id: UUID
    state: str
    attempt: int
    last_error: str | None = None
    created_at: datetime
    updated_at: datetime


_DECIMAL_INPUT_FIELDS = (
    "minimum_weight",
    "maximum_weight",
    "gross_exposure_limit",
    "net_exposure_target",
    "cash_reserve",
    "turnover_limit",
    "variance_limit",
    "risk_aversion",
    "cost_aversion",
    "uncertainty_aversion",
    "commission_rate",
    "half_spread_rate",
    "slippage_rate",
    "impact_rate",
    "impact_breakpoint",
)


def _currency(value: str) -> str:
    normalized = value.strip().upper()
    if len(normalized) != 3 or not normalized.isalpha():
        raise ValueError("base_currency must be a three-letter code")
    return normalized


class MandateVersionInput(StrictModel):
    policy_family: Literal["LONG_ONLY_MEAN_VARIANCE_V1"]
    base_currency: StrictStr = Field(min_length=1, max_length=20)
    objective: Literal["MAXIMIZE_NET_RETURN"]
    eligible_alpha_role: Literal["PRIMARY_ALPHA"]
    universe_version_id: UUID
    minimum_alpha_count: int = Field(ge=2)
    minimum_weight: Decimal
    maximum_weight: Decimal
    gross_exposure_limit: Decimal
    net_exposure_target: Decimal
    cash_reserve: Decimal
    turnover_limit: Decimal
    variance_limit: Decimal
    risk_aversion: Decimal
    cost_aversion: Decimal
    uncertainty_aversion: Decimal
    commission_rate: Decimal
    half_spread_rate: Decimal
    slippage_rate: Decimal
    impact_rate: Decimal
    impact_breakpoint: Decimal
    state: Literal["ACTIVE", "RETIRED"]

    @field_validator("base_currency")
    @classmethod
    def normalize_currency(cls, value: str) -> str:
        return _currency(value)

    @field_validator(*_DECIMAL_INPUT_FIELDS, mode="before")
    @classmethod
    def require_decimal_strings(cls, value: object) -> object:
        return _decimal_string(value)

    @model_validator(mode="after")
    def require_v1_constraints(self) -> "MandateVersionInput":
        values = tuple(getattr(self, name) for name in _DECIMAL_INPUT_FIELDS)
        if not all(value.is_finite() for value in values):
            raise ValueError("V1 numeric values must be finite")
        if not 0 < self.minimum_weight <= self.maximum_weight <= 1:
            raise ValueError("weights must satisfy 0 < minimum_weight <= maximum_weight <= 1")
        if self.minimum_weight * self.minimum_alpha_count > 1 or (
            self.maximum_weight * self.minimum_alpha_count < 1
        ):
            raise ValueError("weight bounds cannot satisfy minimum_alpha_count")
        if self.gross_exposure_limit != 1 or self.net_exposure_target != 1:
            raise ValueError("LONG_ONLY_MEAN_VARIANCE_V1 requires unit gross and net exposure")
        if self.cash_reserve != 0:
            raise ValueError("LONG_ONLY_MEAN_VARIANCE_V1 requires zero cash_reserve")
        if not 1 <= self.turnover_limit <= 2 or self.variance_limit <= 0:
            raise ValueError("turnover_limit must be in [1, 2] and variance_limit must be positive")
        if any(
            value < 0
            for value in (
                self.risk_aversion,
                self.cost_aversion,
                self.uncertainty_aversion,
                self.commission_rate,
                self.half_spread_rate,
                self.slippage_rate,
                self.impact_rate,
                self.impact_breakpoint,
            )
        ) or any(
            value > 1
            for value in (
                self.commission_rate,
                self.half_spread_rate,
                self.slippage_rate,
                self.impact_rate,
                self.impact_breakpoint,
            )
        ):
            raise ValueError("aversions must be nonnegative and rates must be in [0, 1]")
        return self


class CreateMandateInput(MandateVersionInput):
    key: StrictStr = Field(
        min_length=1, max_length=120, pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$"
    )
    name: StrictStr = Field(min_length=1, max_length=200)
    enabled: StrictBool


class MandateVersionView(StrictModel):
    id: UUID
    portfolio_mandate_id: UUID
    version_no: int
    policy_family: str
    base_currency: str
    objective: str
    eligible_alpha_role: str
    universe_version_id: UUID
    minimum_alpha_count: int
    minimum_weight: Decimal
    maximum_weight: Decimal
    gross_exposure_limit: Decimal
    net_exposure_target: Decimal
    cash_reserve: Decimal
    turnover_limit: Decimal
    variance_limit: Decimal
    risk_aversion: Decimal
    cost_aversion: Decimal
    uncertainty_aversion: Decimal
    commission_rate: Decimal
    half_spread_rate: Decimal
    slippage_rate: Decimal
    impact_rate: Decimal
    impact_breakpoint: Decimal
    state: str
    created_at: datetime


class MandateView(StrictModel):
    id: UUID
    key: str
    name: str
    enabled: bool
    state: str
    configuration_state: Literal["V1_CONFIGURED", "LEGACY_UNAVAILABLE"]
    latest_version: MandateVersionView | None = None
    created_at: datetime
    updated_at: datetime


class MandatePage(StrictModel):
    items: list[MandateView]
    next_cursor: UUID | None = None


class CreateCapitalContextInput(StrictModel):
    base_currency: StrictStr = Field(min_length=1, max_length=20)
    deployable_capital: Decimal
    observed_at: datetime
    valid_until: datetime
    notes: StrictStr | None = Field(default=None, max_length=4000)

    @field_validator("base_currency")
    @classmethod
    def normalize_currency(cls, value: str) -> str:
        return _currency(value)

    @field_validator("deployable_capital", mode="before")
    @classmethod
    def require_decimal_string(cls, value: object) -> object:
        return _decimal_string(value)

    @model_validator(mode="after")
    def require_operator_snapshot(self) -> "CreateCapitalContextInput":
        if not self.deployable_capital.is_finite() or self.deployable_capital <= 0:
            raise ValueError("deployable_capital must be a positive finite decimal")
        for value in (self.observed_at, self.valid_until):
            if value.tzinfo is None or value.utcoffset() != timedelta(0):
                raise ValueError("capital context timestamps must be UTC")
        if self.observed_at >= self.valid_until:
            raise ValueError("observed_at must precede valid_until")
        return self


class CapitalContextView(StrictModel):
    id: UUID
    configuration_contract_version: str | None = None
    configuration_state: Literal["V1_CONFIGURED", "LEGACY_UNAVAILABLE"]
    source_type: str
    source_downstream_system_id: UUID | None = None
    base_currency: str
    deployable_capital: Decimal
    observed_at: datetime
    valid_until: datetime
    notes: str | None = None
    created_at: datetime
    updated_at: datetime


class CapitalContextPage(StrictModel):
    items: list[CapitalContextView]
    next_cursor: UUID | None = None


class CreateDownstreamInput(StrictModel):
    name: str = Field(min_length=1, max_length=200)
    environment_type: Literal["PAPER", "LIVE"]
    enabled: bool = True
    package_contract_version: str = Field(default="1", min_length=1, max_length=40)
    feedback_contract_version: str = Field(default="1", min_length=1, max_length=40)
    compatibility: list[str] = Field(default_factory=list)
    public_config: dict[str, Any] = Field(default_factory=dict)


class DownstreamView(StrictModel):
    id: UUID
    name: str
    environment_type: str
    enabled: bool
    package_contract_version: str
    feedback_contract_version: str
    compatibility: list[str]
    preflight_state: str
    public_config: dict[str, Any]
    created_at: datetime
    updated_at: datetime


class DownstreamRegistrationView(DownstreamView):
    # The service token is only returned by the original registration response.
    service_token: str | None = None
    token_issued: bool


class DownstreamTokenView(StrictModel):
    downstream_system_id: UUID
    service_token: str | None


class RotateDownstreamTokenInput(StrictModel):
    pass


class DownstreamPreflightInput(StrictModel):
    package_contract_version: StrictStr = Field(min_length=1, max_length=40)
    feedback_contract_version: StrictStr = Field(min_length=1, max_length=40)
    compatibility: list[StrictStr]
    valid_until: datetime

    @model_validator(mode="after")
    def require_utc_future_validity(self) -> "DownstreamPreflightInput":
        if self.valid_until.tzinfo is None or self.valid_until.utcoffset() != timedelta(0):
            raise ValueError("valid_until must be UTC")
        if self.valid_until <= _now():
            raise ValueError("valid_until must be in the future")
        return self


class DownstreamPage(StrictModel):
    items: list[DownstreamView]
    next_cursor: UUID | None = None


def _now() -> datetime:
    return datetime.now(UTC)


def _public_payload(payload: BaseModel) -> dict[str, Any]:
    return payload.model_dump(mode="json", exclude_none=True)


def _reject_secret_fields(value: object, path: str = "$") -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = str(key).casefold()
            if normalized in _FORBIDDEN_PUBLIC_FIELDS or normalized.endswith("_secret"):
                raise QfError(
                    "PUBLIC_CONFIGURATION_CONTAINS_SECRET",
                    "Public configuration must not contain credentials.",
                    422,
                    {"path": f"{path}.{key}"},
                )
            _reject_secret_fields(item, f"{path}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _reject_secret_fields(item, f"{path}[{index}]")


def _idempotent(
    session: Session,
    *,
    key: str | None,
    operation: str,
    payload: BaseModel,
    action: Callable[[], dict[str, Any]],
    status_code: int,
) -> dict[str, Any]:
    normalized = _public_payload(payload)
    if key:
        existing = session.get(PublicMutationReceipt, key)
        if existing is not None:
            if existing.operation_name != operation or existing.normalized_request != normalized:
                raise QfError(
                    "IDEMPOTENCY_KEY_REUSED",
                    "The idempotency key belongs to a different request.",
                    409,
                )
            return existing.response_json
    result = action()
    if key:
        session.add(
            PublicMutationReceipt(
                idempotency_key=key,
                operation_name=operation,
                normalized_request=normalized,
                response_json=result,
                status_code=status_code,
                created_at=_now(),
            )
        )
    return result


def _page(
    session: Session,
    model: Any,
    *,
    limit: int,
    cursor: UUID | None,
) -> tuple[list[Any], UUID | None]:
    statement = select(model)
    if cursor is not None:
        statement = statement.where(model.id > cursor)
    rows = list(session.scalars(statement.order_by(model.id).limit(limit + 1)))
    has_more = len(rows) > limit
    items = rows[:limit]
    return items, items[-1].id if has_more and items else None


def _universe_spec(payload: UniverseSpecInput) -> dict[str, Any]:
    return {
        "instrument_schema": payload.instrument_schema,
        "membership_rules": payload.membership_rules,
        "calendar_semantics": payload.calendar_semantics,
        "currency_semantics": payload.currency_semantics,
        "data_requirements": payload.data_requirements,
        "risk_model_family": payload.risk_model_family,
        "cost_model_family": payload.cost_model_family,
        "capacity_model_family": payload.capacity_model_family,
        "allowed_alpha_roles": payload.allowed_alpha_roles,
        "downstream_compatibility": payload.downstream_compatibility,
    }


def _universe_view(item: MarketUniverseVersion) -> UniverseView:
    return UniverseView(
        id=item.id,
        universe_key=item.universe_key,
        version_no=item.version_no,
        name=item.name,
        state=item.state,
        spec=item.spec_json,
        created_at=item.created_at,
    )


def _data_source_view(item: GovernedDataSource) -> DataSourceView:
    return DataSourceView(
        id=item.id,
        name=item.name,
        connector_key=item.connector_key,
        provider=item.provider,
        state=item.state,
        universe_scope=[str(value) for value in item.universe_scope],
        field_schema=item.field_schema,
        license_classification=item.license_classification,
        availability_semantics=item.availability_semantics,
        update_cadence=item.update_cadence,
        preflight_state=item.preflight_state,
        public_config=item.public_config,
        created_at=item.created_at,
        updated_at=item.updated_at,
    )


def _dataset_view(item: DatasetRevision) -> DatasetView:
    return DatasetView(
        id=item.id,
        data_source_id=item.data_source_id,
        universe_version_id=item.universe_version_id,
        universe_name=item.universe_name,
        revision_no=item.revision_no,
        partition=item.partition,
        data_class=item.data_class,
        origin=item.origin,
        promotability=item.promotability,
        schema_version=item.schema_version,
        event_start=item.event_start,
        event_end=item.event_end,
        available_start=item.available_start,
        available_end=item.available_end,
        row_count=item.row_count,
        quality_state=item.quality_state,
        point_in_time_state=item.point_in_time_state,
        materialization_request=item.materialization_request,
        created_at=item.created_at,
    )


def _evaluation_dataset_selection_view(
    item: EvaluationDatasetSelection,
) -> EvaluationDatasetSelectionView:
    return EvaluationDatasetSelectionView(
        id=item.id,
        universe_version_id=item.universe_version_id,
        version_no=item.version_no,
        discovery_dataset_revision_id=item.discovery_dataset_revision_id,
        validation_dataset_revision_id=item.validation_dataset_revision_id,
        sealed_dataset_revision_id=item.sealed_dataset_revision_id,
        state=item.state,
        created_at=item.created_at,
    )


def _evaluation_design_version_view(item: EvaluationDesignVersion) -> EvaluationDesignVersionView:
    return EvaluationDesignVersionView(
        id=item.id,
        version_no=item.version_no,
        universe_version_id=item.universe_version_id,
        contract_version=item.contract_version,
        allowed_model_mode=item.allowed_model_mode,
        qualification_role=item.qualification_role,
        walk_forward_folds=item.walk_forward_folds,
        annualization_factor=item.annualization_factor,
        multiple_testing_method=item.multiple_testing_method,
        multiple_testing_max_trials=item.multiple_testing_max_trials,
        qualification_metric_code=item.qualification_metric_code,
        qualification_comparator=item.qualification_comparator,
        qualification_threshold=item.qualification_threshold,
        pass_disclosure_code=item.pass_disclosure_code,
        failure_disclosure_code=item.failure_disclosure_code,
        inconclusive_disclosure_code=item.inconclusive_disclosure_code,
        invalid_disclosure_code=item.invalid_disclosure_code,
        state=item.state,
        created_at=item.created_at,
    )


def _promotion_policy_version_view(
    session: Session, item: PromotionPolicyVersion
) -> PromotionPolicyVersionView:
    gates = list(
        session.scalars(
            select(PromotionPolicyGate)
            .where(PromotionPolicyGate.policy_version_id == item.id)
            .order_by(PromotionPolicyGate.ordinal)
        )
    )
    return PromotionPolicyVersionView(
        id=item.id,
        version_no=item.version_no,
        purpose=item.purpose,
        mode=item.mode,
        policy_contract_version=item.policy_contract_version,
        paper_downstream_system_id=item.paper_downstream_system_id,
        live_downstream_system_id=item.live_downstream_system_id,
        gates=[
            PromotionPolicyGateView(
                metric_code=gate.metric_code,
                comparator=gate.comparator,
                threshold=gate.threshold,
                ordinal=gate.ordinal,
            )
            for gate in gates
        ],
        state=item.state,
        created_at=item.created_at,
    )


def _operation_view(item: Job) -> OperationView:
    return OperationView(
        id=item.id,
        kind=item.kind,
        resource_type=item.resource_type,
        resource_id=item.resource_id,
        state=item.state,
        attempt=item.attempt,
        last_error=item.last_error,
        created_at=item.created_at,
        updated_at=item.updated_at,
    )


def _is_v1_mandate(item: PortfolioMandateVersion | None) -> bool:
    return item is not None and item.policy_family == "LONG_ONLY_MEAN_VARIANCE_V1" and all(
        getattr(item, name) is not None
        for name in (
            "universe_version_id",
            "eligible_alpha_role",
            "minimum_weight",
            "maximum_weight",
            "gross_exposure_limit",
            "net_exposure_target",
            "cash_reserve",
            "turnover_limit",
            "variance_limit",
            "risk_aversion",
            "cost_aversion",
            "uncertainty_aversion",
            "commission_rate",
            "half_spread_rate",
            "slippage_rate",
            "impact_rate",
            "impact_breakpoint",
            "state",
        )
    )


def _mandate_version_view(item: PortfolioMandateVersion) -> MandateVersionView:
    if not _is_v1_mandate(item):
        raise ValueError("Legacy Mandate Version cannot be represented as V1")
    return MandateVersionView.model_validate(
        {
            "id": item.id,
            "portfolio_mandate_id": item.portfolio_mandate_id,
            "version_no": item.version_no,
            "policy_family": item.policy_family,
            "base_currency": item.base_currency,
            "objective": item.objective,
            "eligible_alpha_role": item.eligible_alpha_role,
            "universe_version_id": item.universe_version_id,
            "minimum_alpha_count": item.minimum_alpha_count,
            "minimum_weight": item.minimum_weight,
            "maximum_weight": item.maximum_weight,
            "gross_exposure_limit": item.gross_exposure_limit,
            "net_exposure_target": item.net_exposure_target,
            "cash_reserve": item.cash_reserve,
            "turnover_limit": item.turnover_limit,
            "variance_limit": item.variance_limit,
            "risk_aversion": item.risk_aversion,
            "cost_aversion": item.cost_aversion,
            "uncertainty_aversion": item.uncertainty_aversion,
            "commission_rate": item.commission_rate,
            "half_spread_rate": item.half_spread_rate,
            "slippage_rate": item.slippage_rate,
            "impact_rate": item.impact_rate,
            "impact_breakpoint": item.impact_breakpoint,
            "state": item.state,
            "created_at": item.created_at,
        }
    )


def _mandate_view(session: Session, item: PortfolioMandate) -> MandateView:
    version = session.get(PortfolioMandateVersion, item.latest_version_id)
    is_v1 = _is_v1_mandate(version)
    return MandateView(
        id=item.id,
        key=item.key,
        name=item.name,
        enabled=item.enabled,
        state=item.state,
        configuration_state="V1_CONFIGURED" if is_v1 else "LEGACY_UNAVAILABLE",
        latest_version=_mandate_version_view(version) if is_v1 and version is not None else None,
        created_at=item.created_at,
        updated_at=item.updated_at,
    )


def _capital_context_view(item: CapitalContextVersion) -> CapitalContextView:
    is_v1 = (
        item.configuration_contract_version == "CAPITAL_CONTEXT_V1"
        and item.source_type == "ADMIN"
        and item.source_downstream_system_id is None
        and item.deployable_capital > 0
        and item.observed_at < item.valid_until
    )
    return CapitalContextView(
        id=item.id,
        configuration_contract_version=item.configuration_contract_version,
        configuration_state="V1_CONFIGURED" if is_v1 else "LEGACY_UNAVAILABLE",
        source_type=item.source_type,
        source_downstream_system_id=item.source_downstream_system_id,
        base_currency=item.base_currency,
        deployable_capital=item.deployable_capital,
        observed_at=item.observed_at,
        valid_until=item.valid_until,
        notes=item.notes,
        created_at=item.created_at,
        updated_at=item.updated_at,
    )


def _downstream_view(item: DownstreamSystem) -> DownstreamView:
    return DownstreamView(
        id=item.id,
        name=item.name,
        environment_type=item.environment_type,
        enabled=item.enabled,
        package_contract_version=item.package_contract_version,
        feedback_contract_version=item.feedback_contract_version,
        compatibility=item.compatibility,
        preflight_state=item.preflight_state,
        public_config=item.public_config,
        created_at=item.created_at,
        updated_at=item.updated_at,
    )


def _require_universes(session: Session, ids: list[UUID]) -> None:
    present = set(session.scalars(select(MarketUniverseVersion.id).where(MarketUniverseVersion.id.in_(ids))))
    missing = [str(item) for item in ids if item not in present]
    if missing:
        raise QfError("UNIVERSE_NOT_FOUND", "One or more Universe Versions do not exist.", 404, {"ids": missing})


def _locked_universe(session: Session, universe_version_id: UUID) -> MarketUniverseVersion:
    universe = session.execute(
        select(MarketUniverseVersion)
        .where(MarketUniverseVersion.id == universe_version_id)
        .with_for_update()
    ).scalar_one_or_none()
    if universe is None:
        raise QfError("UNIVERSE_NOT_FOUND", "Universe Version was not found.", 404)
    return universe


def _trusted_evaluation_dataset(
    session: Session,
    dataset_revision_id: UUID,
    *,
    universe_version_id: UUID,
    phase: Literal["DISCOVERY", "VALIDATION", "SEALED"],
    sealed: bool,
) -> DatasetRevision:
    dataset = session.execute(
        select(DatasetRevision)
        .where(DatasetRevision.id == dataset_revision_id)
        .with_for_update()
    ).scalar_one_or_none()
    if dataset is None:
        raise QfError("DATASET_NOT_FOUND", "Dataset Revision was not found.", 404)
    if (
        dataset.universe_version_id != universe_version_id
        or dataset.partition != phase
        or dataset.data_source_id is None
        or dataset.data_class not in {"VENDOR", "PRODUCTION"}
        or dataset.promotability != "PROMOTABLE"
        or dataset.quality_state != "VALID"
        or dataset.point_in_time_state != "VALID"
    ):
        raise QfError(
            "EVALUATION_DATASET_INVALID",
            "Dataset Revision is not a trusted evaluation input for this phase.",
            409,
            {"phase": phase},
        )
    source = session.execute(
        select(GovernedDataSource)
        .where(GovernedDataSource.id == dataset.data_source_id)
        .with_for_update()
    ).scalar_one_or_none()
    catalog = session.execute(
        select(NautilusCatalogBinding)
        .where(NautilusCatalogBinding.dataset_revision_id == dataset.id)
        .with_for_update()
    ).scalar_one_or_none()
    if (
        source is None
        or source.state != "ACTIVE"
        or source.preflight_state != "READY"
        or catalog is None
        or catalog.sealed is not sealed
        or catalog.quality_state != "VALID"
        or catalog.point_in_time_state != "VALID"
    ):
        raise QfError(
            "EVALUATION_DATASET_INVALID",
            "Dataset Revision lacks a current trusted catalog binding.",
            409,
            {"phase": phase},
        )
    return dataset


def _new_mandate_version(
    *,
    mandate_id: UUID,
    version_no: int,
    payload: MandateVersionInput,
) -> PortfolioMandateVersion:
    return PortfolioMandateVersion(
        portfolio_mandate_id=mandate_id,
        version_no=version_no,
        policy_family=payload.policy_family,
        base_currency=payload.base_currency,
        objective=payload.objective,
        eligible_alpha_role=payload.eligible_alpha_role,
        universe_version_id=payload.universe_version_id,
        minimum_alpha_count=payload.minimum_alpha_count,
        minimum_weight=payload.minimum_weight,
        maximum_weight=payload.maximum_weight,
        gross_exposure_limit=payload.gross_exposure_limit,
        net_exposure_target=payload.net_exposure_target,
        cash_reserve=payload.cash_reserve,
        turnover_limit=payload.turnover_limit,
        variance_limit=payload.variance_limit,
        risk_aversion=payload.risk_aversion,
        cost_aversion=payload.cost_aversion,
        uncertainty_aversion=payload.uncertainty_aversion,
        commission_rate=payload.commission_rate,
        half_spread_rate=payload.half_spread_rate,
        slippage_rate=payload.slippage_rate,
        impact_rate=payload.impact_rate,
        impact_breakpoint=payload.impact_breakpoint,
        state=payload.state,
    )


@router.get("/universes", response_model=UniversePage)
def list_universes(
    limit: int = Query(default=50, ge=1, le=100),
    cursor: UUID | None = None,
    session: Session = Depends(get_session),
) -> UniversePage:
    items, next_cursor = _page(session, MarketUniverseVersion, limit=limit, cursor=cursor)
    return UniversePage(items=[_universe_view(item) for item in items], next_cursor=next_cursor)


@router.post("/universes", response_model=UniverseView, status_code=201)
def create_universe(
    payload: CreateUniverseInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    with session.begin():
        def action() -> dict[str, Any]:
            existing = session.scalar(
                select(MarketUniverseVersion).where(
                    MarketUniverseVersion.universe_key == payload.universe_key
                )
            )
            if existing is not None:
                raise QfError(
                    "UNIVERSE_KEY_EXISTS",
                    "Create the next immutable version from the existing Universe.",
                    409,
                    {"universe_id": str(existing.id)},
                )
            item = MarketUniverseVersion(
                universe_key=payload.universe_key,
                version_no=1,
                name=payload.name.strip(),
                state=payload.state,
                spec_json=_universe_spec(payload),
                created_at=_now(),
            )
            session.add(item)
            session.flush()
            append_event(
                session,
                kind="UNIVERSE_VERSION_CREATED",
                aggregate_type="MARKET_UNIVERSE_VERSION",
                aggregate_id=item.id,
                payload={"universe_key": item.universe_key, "version_no": item.version_no},
                actor_kind="HUMAN",
            )
            return _universe_view(item).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation="configuration.universe.create",
            payload=payload,
            action=action,
            status_code=201,
        )


@router.post("/universes/{universe_id}/versions", response_model=UniverseView, status_code=201)
def create_universe_version(
    universe_id: UUID,
    payload: UniverseSpecInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    del request
    with session.begin():
        def action() -> dict[str, Any]:
            current = session.execute(
                select(MarketUniverseVersion)
                .where(MarketUniverseVersion.id == universe_id)
                .with_for_update()
            ).scalar_one_or_none()
            if current is None:
                raise QfError("UNIVERSE_NOT_FOUND", "Universe Version was not found.", 404)
            last_version = session.scalar(
                select(func.max(MarketUniverseVersion.version_no)).where(
                    MarketUniverseVersion.universe_key == current.universe_key
                )
            )
            item = MarketUniverseVersion(
                universe_key=current.universe_key,
                version_no=int(last_version or 0) + 1,
                name=payload.name.strip(),
                state=payload.state,
                spec_json=_universe_spec(payload),
                created_at=_now(),
            )
            session.add(item)
            session.flush()
            append_event(
                session,
                kind="UNIVERSE_VERSION_CREATED",
                aggregate_type="MARKET_UNIVERSE_VERSION",
                aggregate_id=item.id,
                payload={"universe_key": item.universe_key, "version_no": item.version_no},
                actor_kind="HUMAN",
            )
            return _universe_view(item).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation=f"configuration.universe.version:{universe_id}",
            payload=payload,
            action=action,
            status_code=201,
        )


@router.get("/data-sources", response_model=DataSourcePage)
def list_data_sources(
    limit: int = Query(default=50, ge=1, le=100),
    cursor: UUID | None = None,
    session: Session = Depends(get_session),
) -> DataSourcePage:
    items, next_cursor = _page(session, GovernedDataSource, limit=limit, cursor=cursor)
    return DataSourcePage(items=[_data_source_view(item) for item in items], next_cursor=next_cursor)


@router.post("/data-sources", response_model=DataSourceView, status_code=201)
def create_data_source(
    payload: CreateDataSourceInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    del request
    _reject_secret_fields(payload.public_config)
    _reject_secret_fields(payload.availability_semantics)
    with session.begin():
        def action() -> dict[str, Any]:
            _require_universes(session, payload.universe_scope)
            existing = session.scalar(
                select(GovernedDataSource).where(GovernedDataSource.name == payload.name.strip())
            )
            if existing is not None:
                raise QfError("DATA_SOURCE_NAME_CONFLICT", "Data Source name already exists.", 409)
            item = GovernedDataSource(
                name=payload.name.strip(),
                connector_key=payload.connector_key,
                provider=payload.provider.strip(),
                state="ACTIVE",
                universe_scope=[str(item) for item in payload.universe_scope],
                fields=sorted(payload.field_schema),
                field_schema=payload.field_schema,
                license_classification=payload.license_classification.strip(),
                availability_semantics=payload.availability_semantics,
                update_cadence=payload.update_cadence,
                # Registration is governed, not a fabricated connector preflight.
                preflight_state="PENDING",
                public_config=payload.public_config,
            )
            session.add(item)
            session.flush()
            append_event(
                session,
                kind="DATA_SOURCE_REGISTERED",
                aggregate_type="GOVERNED_DATA_SOURCE",
                aggregate_id=item.id,
                payload={"connector_key": item.connector_key},
                actor_kind="HUMAN",
            )
            return _data_source_view(item).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation="configuration.data-source.create",
            payload=payload,
            action=action,
            status_code=201,
        )


@router.post(
    "/data-sources/{data_source_id}/preflight",
    response_model=OperationView,
    status_code=202,
)
def request_data_source_preflight(
    data_source_id: UUID,
    payload: DataSourcePreflightInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    del request
    with session.begin():
        def action() -> dict[str, Any]:
            source = session.execute(
                select(GovernedDataSource)
                .where(GovernedDataSource.id == data_source_id)
                .with_for_update()
            ).scalar_one_or_none()
            if source is None:
                raise QfError("DATA_SOURCE_NOT_FOUND", "Data Source was not found.", 404)
            if source.state != "ACTIVE":
                raise QfError("DATA_SOURCE_INACTIVE", "Data Source is not active.", 409)
            if source.preflight_state != "PENDING":
                raise QfError(
                    "DATA_SOURCE_PREFLIGHT_STATE_CONFLICT",
                    "Data Source preflight is not pending.",
                    409,
                    {"preflight_state": source.preflight_state},
                )
            existing = session.scalar(
                select(Job.id).where(
                    Job.kind == "DATA_SOURCE_PREFLIGHT",
                    Job.resource_type == "governed_data_source",
                    Job.resource_id == source.id,
                    Job.state.in_(("READY", "LEASED")),
                )
            )
            if existing is not None:
                raise QfError(
                    "DATA_SOURCE_PREFLIGHT_IN_PROGRESS",
                    "Data Source already has a pending preflight operation.",
                    409,
                    {"operation_id": str(existing)},
                )
            job = enqueue_job(
                session,
                kind="DATA_SOURCE_PREFLIGHT",
                resource_type="governed_data_source",
                resource_id=source.id,
            )
            append_event(
                session,
                kind="DATA_SOURCE_PREFLIGHT_REQUESTED",
                aggregate_type="GOVERNED_DATA_SOURCE",
                aggregate_id=source.id,
                payload={"job_id": str(job.id)},
                actor_kind="HUMAN",
            )
            return _operation_view(job).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation=f"configuration.data-source.preflight:{data_source_id}",
            payload=payload,
            action=action,
            status_code=202,
        )


@router.get("/datasets", response_model=DatasetPage)
def list_datasets(
    limit: int = Query(default=50, ge=1, le=100),
    cursor: UUID | None = None,
    session: Session = Depends(get_session),
) -> DatasetPage:
    items, next_cursor = _page(session, DatasetRevision, limit=limit, cursor=cursor)
    return DatasetPage(items=[_dataset_view(item) for item in items], next_cursor=next_cursor)


@router.post("/datasets/materializations", response_model=OperationView, status_code=202)
def request_dataset_materialization(
    payload: DatasetMaterializationInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    del request
    _reject_secret_fields(payload.quality_requirements)
    _reject_secret_fields(payload.point_in_time_requirements)
    with session.begin():
        def action() -> dict[str, Any]:
            source = session.execute(
                select(GovernedDataSource)
                .where(GovernedDataSource.id == payload.data_source_id)
                .with_for_update()
            ).scalar_one_or_none()
            if source is None:
                raise QfError("DATA_SOURCE_NOT_FOUND", "Data Source was not found.", 404)
            if source.state != "ACTIVE":
                raise QfError("DATA_SOURCE_INACTIVE", "Data Source is not active.", 409)
            if source.preflight_state != "READY":
                raise QfError(
                    "DATA_SOURCE_PREFLIGHT_REQUIRED",
                    "Data Source preflight must complete before materialization.",
                    409,
                )
            universe = session.get(MarketUniverseVersion, payload.universe_version_id)
            if universe is None:
                raise QfError("UNIVERSE_NOT_FOUND", "Universe Version was not found.", 404)
            if str(universe.id) not in source.universe_scope:
                raise QfError(
                    "DATA_SOURCE_UNIVERSE_FORBIDDEN",
                    "Data Source is not governed for this Universe Version.",
                    409,
                )
            last_revision = session.scalar(
                select(func.max(DatasetRevision.revision_no)).where(
                    DatasetRevision.data_source_id == source.id,
                    DatasetRevision.universe_version_id == universe.id,
                    DatasetRevision.partition == payload.partition,
                )
            )
            request_json = _public_payload(payload)
            revision = DatasetRevision(
                data_source_id=source.id,
                universe_version_id=universe.id,
                universe_name=universe.name,
                revision_no=int(last_revision or 0) + 1,
                data_class=payload.data_class,
                origin=payload.origin.strip(),
                ingested_at=_now(),
                # A request cannot promote itself before a trusted worker validates it.
                promotability="NON_PROMOTABLE",
                schema_version=payload.schema_version,
                event_start=payload.event_start,
                event_end=payload.event_end,
                available_start=payload.available_start,
                available_end=payload.available_end,
                quality_state="PENDING",
                point_in_time_state="PENDING",
                partition=payload.partition,
                materialization_request=request_json,
                created_at=_now(),
            )
            session.add(revision)
            session.flush()
            job_kind = (
                "SEALED_CATALOG_PROVISION"
                if payload.partition == "SEALED"
                else "DATASET_MATERIALIZATION"
            )
            job = enqueue_job(
                session,
                kind=job_kind,
                resource_type="dataset_revision",
                resource_id=revision.id,
                payload=(
                    {}
                    if job_kind == "SEALED_CATALOG_PROVISION"
                    else {"dataset_revision_id": str(revision.id), "request": request_json}
                ),
            )
            append_event(
                session,
                kind=(
                    "SEALED_CATALOG_PROVISION_REQUESTED"
                    if job_kind == "SEALED_CATALOG_PROVISION"
                    else "DATASET_MATERIALIZATION_REQUESTED"
                ),
                aggregate_type="DATASET_REVISION",
                aggregate_id=revision.id,
                payload={"job_id": str(job.id), "revision_no": revision.revision_no},
                actor_kind="HUMAN",
            )
            return _operation_view(job).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation="configuration.dataset.materialize",
            payload=payload,
            action=action,
            status_code=202,
        )


@router.get("/datasets/{dataset_id}", response_model=DatasetView)
def get_dataset(dataset_id: UUID, session: Session = Depends(get_session)) -> DatasetView:
    item = session.get(DatasetRevision, dataset_id)
    if item is None:
        raise QfError("DATASET_NOT_FOUND", "Dataset Revision was not found.", 404)
    return _dataset_view(item)


@router.get("/datasets/{dataset_id}/quality", response_model=DatasetQualityView)
def get_dataset_quality(dataset_id: UUID, session: Session = Depends(get_session)) -> DatasetQualityView:
    item = session.get(DatasetRevision, dataset_id)
    if item is None:
        raise QfError("DATASET_NOT_FOUND", "Dataset Revision was not found.", 404)
    results = list(
        session.scalars(
            select(DataQualityResult)
            .where(DataQualityResult.dataset_revision_id == item.id)
            .order_by(DataQualityResult.check_kind, DataQualityResult.revision_no)
        )
    )
    return DatasetQualityView(
        dataset_revision_id=item.id,
        quality_state=item.quality_state,
        point_in_time_state=item.point_in_time_state,
        promotability=item.promotability,
        results=[
            DatasetQualityResultView(
                id=result.id,
                check_kind=result.check_kind,
                revision_no=result.revision_no,
                state=result.state,
                summary=result.summary,
                checker_version=result.checker_version,
                created_at=result.created_at,
            )
            for result in results
        ],
    )


@router.get("/datasets/{dataset_id}/profile", response_model=DatasetProfileView)
def get_dataset_profile(dataset_id: UUID, session: Session = Depends(get_session)) -> DatasetProfileView:
    item = session.get(DatasetRevision, dataset_id)
    if item is None:
        raise QfError("DATASET_NOT_FOUND", "Dataset Revision was not found.", 404)
    request = item.materialization_request
    instruments = request.get("instrument_scope", []) if isinstance(request, dict) else []
    return DatasetProfileView(
        dataset_revision_id=item.id,
        data_type=str(request.get("data_type")) if isinstance(request, dict) and request.get("data_type") else None,
        instrument_scope=[str(value) for value in instruments] if isinstance(instruments, list) else [],
        event_start=item.event_start,
        event_end=item.event_end,
        available_start=item.available_start,
        available_end=item.available_end,
    )


@router.get("/evaluation-dataset-selections", response_model=EvaluationDatasetSelectionPage)
def list_evaluation_dataset_selections(
    limit: int = Query(default=50, ge=1, le=100),
    cursor: UUID | None = None,
    session: Session = Depends(get_session),
) -> EvaluationDatasetSelectionPage:
    items, next_cursor = _page(
        session, EvaluationDatasetSelection, limit=limit, cursor=cursor
    )
    return EvaluationDatasetSelectionPage(
        items=[_evaluation_dataset_selection_view(item) for item in items],
        next_cursor=next_cursor,
    )


@router.post(
    "/evaluation-dataset-selections",
    response_model=EvaluationDatasetSelectionView,
    status_code=201,
)
def create_evaluation_dataset_selection(
    payload: EvaluationDatasetSelectionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    del request
    with session.begin():
        def action() -> dict[str, Any]:
            universe = _locked_universe(session, payload.universe_version_id)
            _trusted_evaluation_dataset(
                session,
                payload.discovery_dataset_revision_id,
                universe_version_id=universe.id,
                phase="DISCOVERY",
                sealed=False,
            )
            _trusted_evaluation_dataset(
                session,
                payload.validation_dataset_revision_id,
                universe_version_id=universe.id,
                phase="VALIDATION",
                sealed=False,
            )
            _trusted_evaluation_dataset(
                session,
                payload.sealed_dataset_revision_id,
                universe_version_id=universe.id,
                phase="SEALED",
                sealed=True,
            )
            enabled = list(
                session.scalars(
                    select(EvaluationDatasetSelection)
                    .where(
                        EvaluationDatasetSelection.universe_version_id == universe.id,
                        EvaluationDatasetSelection.state == "ENABLED",
                    )
                    .with_for_update()
                )
            )
            if len(enabled) > 1:
                raise QfError(
                    "EVALUATION_DATASET_SELECTION_AMBIGUOUS",
                    "Universe has more than one enabled Evaluation Dataset Selection.",
                    409,
                )
            last_version = session.scalar(
                select(func.max(EvaluationDatasetSelection.version_no)).where(
                    EvaluationDatasetSelection.universe_version_id == universe.id
                )
            )
            if enabled:
                enabled[0].state = "RETIRED"
                append_event(
                    session,
                    kind="EVALUATION_DATASET_SELECTION_RETIRED",
                    aggregate_type="EVALUATION_DATASET_SELECTION",
                    aggregate_id=enabled[0].id,
                    payload={"version_no": enabled[0].version_no},
                    actor_kind="HUMAN",
                )
            item = EvaluationDatasetSelection(
                universe_version_id=universe.id,
                version_no=int(last_version or 0) + 1,
                discovery_dataset_revision_id=payload.discovery_dataset_revision_id,
                validation_dataset_revision_id=payload.validation_dataset_revision_id,
                sealed_dataset_revision_id=payload.sealed_dataset_revision_id,
                state=payload.state,
            )
            session.add(item)
            session.flush()
            append_event(
                session,
                kind="EVALUATION_DATASET_SELECTION_CREATED",
                aggregate_type="EVALUATION_DATASET_SELECTION",
                aggregate_id=item.id,
                payload={"universe_version_id": str(universe.id), "version_no": item.version_no},
                actor_kind="HUMAN",
            )
            return _evaluation_dataset_selection_view(item).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation=f"configuration.evaluation-dataset-selection:{payload.universe_version_id}",
            payload=payload,
            action=action,
            status_code=201,
        )


@router.get("/evaluation-design-versions", response_model=EvaluationDesignVersionPage)
def list_evaluation_design_versions(
    limit: int = Query(default=50, ge=1, le=100),
    cursor: UUID | None = None,
    session: Session = Depends(get_session),
) -> EvaluationDesignVersionPage:
    items, next_cursor = _page(session, EvaluationDesignVersion, limit=limit, cursor=cursor)
    return EvaluationDesignVersionPage(
        items=[_evaluation_design_version_view(item) for item in items],
        next_cursor=next_cursor,
    )


@router.post(
    "/evaluation-design-versions",
    response_model=EvaluationDesignVersionView,
    status_code=201,
)
def create_evaluation_design_version(
    payload: EvaluationDesignVersionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    del request
    with session.begin():
        def action() -> dict[str, Any]:
            universe = _locked_universe(session, payload.universe_version_id)
            active = list(
                session.scalars(
                    select(EvaluationDesignVersion)
                    .where(
                        EvaluationDesignVersion.universe_version_id == universe.id,
                        EvaluationDesignVersion.state == "ACTIVE",
                    )
                    .with_for_update()
                )
            )
            if len(active) > 1:
                raise QfError(
                    "EVALUATION_DESIGN_AMBIGUOUS",
                    "Universe has more than one active Evaluation Design Version.",
                    409,
                )
            last_version = session.scalar(
                select(func.max(EvaluationDesignVersion.version_no)).where(
                    EvaluationDesignVersion.universe_version_id == universe.id
                )
            )
            if active:
                active[0].state = "RETIRED"
                append_event(
                    session,
                    kind="EVALUATION_DESIGN_VERSION_RETIRED",
                    aggregate_type="EVALUATION_DESIGN_VERSION",
                    aggregate_id=active[0].id,
                    payload={"version_no": active[0].version_no},
                    actor_kind="HUMAN",
                )
            item = EvaluationDesignVersion(
                version_no=int(last_version or 0) + 1,
                universe_version_id=universe.id,
                contract_version=payload.contract_version,
                allowed_model_mode=payload.allowed_model_mode,
                qualification_role=payload.qualification_role,
                walk_forward_folds=payload.walk_forward_folds,
                annualization_factor=payload.annualization_factor,
                multiple_testing_method=payload.multiple_testing_method,
                multiple_testing_max_trials=payload.multiple_testing_max_trials,
                qualification_metric_code=payload.qualification_metric_code,
                qualification_comparator=payload.qualification_comparator,
                qualification_threshold=payload.qualification_threshold,
                pass_disclosure_code=payload.pass_disclosure_code,
                failure_disclosure_code=payload.failure_disclosure_code,
                inconclusive_disclosure_code=payload.inconclusive_disclosure_code,
                invalid_disclosure_code=payload.invalid_disclosure_code,
                state=payload.state,
            )
            session.add(item)
            session.flush()
            append_event(
                session,
                kind="EVALUATION_DESIGN_VERSION_CREATED",
                aggregate_type="EVALUATION_DESIGN_VERSION",
                aggregate_id=item.id,
                payload={"universe_version_id": str(universe.id), "version_no": item.version_no},
                actor_kind="HUMAN",
            )
            return _evaluation_design_version_view(item).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation=f"configuration.evaluation-design-version:{payload.universe_version_id}",
            payload=payload,
            action=action,
            status_code=201,
        )


@router.get("/promotion-policy-versions", response_model=PromotionPolicyVersionPage)
def list_promotion_policy_versions(
    limit: int = Query(default=50, ge=1, le=100),
    cursor: UUID | None = None,
    session: Session = Depends(get_session),
) -> PromotionPolicyVersionPage:
    items, next_cursor = _page(session, PromotionPolicyVersion, limit=limit, cursor=cursor)
    return PromotionPolicyVersionPage(
        items=[_promotion_policy_version_view(session, item) for item in items],
        next_cursor=next_cursor,
    )


@router.post(
    "/promotion-policy-versions",
    response_model=PromotionPolicyVersionView,
    status_code=201,
)
def create_promotion_policy_version(
    payload: PromotionPolicyVersionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    del request
    if payload.purpose in {"PORTFOLIO_TO_PAPER", "PAPER_TO_LIVE"}:
        raise QfError(
            "PROMOTION_POLICY_TYPED_BINDING_UNAVAILABLE",
            "Paper/Live policy creation requires typed connection, feedback-contract, and preflight writers.",
            409,
        )
    with session.begin():
        def action() -> dict[str, Any]:
            active = list(
                session.scalars(
                    select(PromotionPolicyVersion)
                    .where(
                        PromotionPolicyVersion.purpose == payload.purpose,
                        PromotionPolicyVersion.state == "ACTIVE",
                    )
                    .with_for_update()
                )
            )
            if len(active) > 1:
                raise QfError(
                    "PROMOTION_POLICY_AMBIGUOUS",
                    "Purpose has more than one active Promotion Policy Version.",
                    409,
                )
            last_version = session.scalar(
                select(func.max(PromotionPolicyVersion.version_no)).where(
                    PromotionPolicyVersion.purpose == payload.purpose
                )
            )
            if active:
                active[0].state = "RETIRED"
                append_event(
                    session,
                    kind="PROMOTION_POLICY_VERSION_RETIRED",
                    aggregate_type="PROMOTION_POLICY_VERSION",
                    aggregate_id=active[0].id,
                    payload={"purpose": active[0].purpose, "version_no": active[0].version_no},
                    actor_kind="HUMAN",
                )
            item = PromotionPolicyVersion(
                version_no=int(last_version or 0) + 1,
                purpose=payload.purpose,
                mode=payload.mode,
                policy_contract_version="PROMOTION_POLICY_V1",
                state=payload.state,
            )
            session.add(item)
            session.flush()
            session.add_all(
                PromotionPolicyGate(
                    policy_version_id=item.id,
                    metric_code=gate.metric_code,
                    comparator=gate.comparator,
                    threshold=gate.threshold,
                    ordinal=gate.ordinal,
                )
                for gate in payload.gates
            )
            append_event(
                session,
                kind="PROMOTION_POLICY_VERSION_CREATED",
                aggregate_type="PROMOTION_POLICY_VERSION",
                aggregate_id=item.id,
                payload={"purpose": item.purpose, "version_no": item.version_no},
                actor_kind="HUMAN",
            )
            return _promotion_policy_version_view(session, item).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation=f"configuration.promotion-policy-version:{payload.purpose}",
            payload=payload,
            action=action,
            status_code=201,
        )


def _reconcile_initial_portfolio_inputs(session: Session, universe_version_id: UUID) -> None:
    """Reconsider an already-qualified Alpha pool after Mandate configuration."""
    from portfolio_input_service import stage_initial_portfolio_input_evaluations

    qualification_ids = session.scalars(
        select(AlphaQualification.id).where(
            AlphaQualification.universe_version_id == universe_version_id,
            AlphaQualification.role == "PRIMARY_ALPHA",
            AlphaQualification.state == "ACTIVE",
        )
    )
    for qualification_id in qualification_ids:
        stage_initial_portfolio_input_evaluations(session, qualification_id=qualification_id)


@router.get("/operations/{operation_id}", response_model=OperationView)
def get_configuration_operation(
    operation_id: UUID, session: Session = Depends(get_session)
) -> OperationView:
    item = session.get(Job, operation_id)
    if item is None or item.kind not in {
        "DATASET_MATERIALIZATION",
        "DATA_SOURCE_PREFLIGHT",
        "SEALED_CATALOG_PROVISION",
    }:
        raise QfError("OPERATION_NOT_FOUND", "Configuration operation was not found.", 404)
    return _operation_view(item)


@router.get("/portfolio-mandates", response_model=MandatePage)
def list_mandates(
    limit: int = Query(default=50, ge=1, le=100),
    cursor: UUID | None = None,
    session: Session = Depends(get_session),
) -> MandatePage:
    items, next_cursor = _page(session, PortfolioMandate, limit=limit, cursor=cursor)
    return MandatePage(
        items=[_mandate_view(session, item) for item in items], next_cursor=next_cursor
    )


@router.get("/capital-contexts", response_model=CapitalContextPage)
def list_capital_contexts(
    limit: int = Query(default=50, ge=1, le=100),
    cursor: UUID | None = None,
    session: Session = Depends(get_session),
) -> CapitalContextPage:
    items, next_cursor = _page(session, CapitalContextVersion, limit=limit, cursor=cursor)
    return CapitalContextPage(
        items=[_capital_context_view(item) for item in items], next_cursor=next_cursor
    )


@router.post("/capital-contexts", response_model=CapitalContextView, status_code=201)
def create_capital_context(
    payload: CreateCapitalContextInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    del request
    with session.begin():
        def action() -> dict[str, Any]:
            item = CapitalContextVersion(
                configuration_contract_version="CAPITAL_CONTEXT_V1",
                source_type="ADMIN",
                source_downstream_system_id=None,
                base_currency=payload.base_currency,
                deployable_capital=payload.deployable_capital,
                observed_at=payload.observed_at,
                valid_until=payload.valid_until,
                notes=payload.notes,
            )
            session.add(item)
            session.flush()
            append_event(
                session,
                kind="CAPITAL_CONTEXT_VERSION_CREATED",
                aggregate_type="CAPITAL_CONTEXT_VERSION",
                aggregate_id=item.id,
                payload={"configuration_contract_version": item.configuration_contract_version},
                actor_kind="HUMAN",
            )
            return _capital_context_view(item).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation="configuration.capital-context.create",
            payload=payload,
            action=action,
            status_code=201,
        )


@router.post("/portfolio-mandates", response_model=MandateView, status_code=201)
def create_mandate(
    payload: CreateMandateInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    del request
    with session.begin():
        def action() -> dict[str, Any]:
            _require_universes(session, [payload.universe_version_id])
            existing = session.scalar(select(PortfolioMandate).where(PortfolioMandate.key == payload.key))
            if existing is not None:
                raise QfError(
                    "MANDATE_KEY_EXISTS",
                    "Create the next immutable version from the existing Mandate.",
                    409,
                    {"mandate_id": str(existing.id)},
                )
            mandate_id = uuid4()
            version = _new_mandate_version(mandate_id=mandate_id, version_no=1, payload=payload)
            # SQLAlchemy applies UUID defaults only during flush; allocate this
            # immutable child identity before storing the parent pointer.
            version.id = uuid4()
            item = PortfolioMandate(
                id=mandate_id,
                key=payload.key,
                name=payload.name.strip(),
                enabled=payload.enabled,
                latest_version_id=version.id,
                spec_json={},
                state="ACTIVE",
            )
            session.add_all((item, version))
            session.flush()
            _reconcile_initial_portfolio_inputs(session, payload.universe_version_id)
            append_event(
                session,
                kind="PORTFOLIO_MANDATE_VERSION_CREATED",
                aggregate_type="PORTFOLIO_MANDATE",
                aggregate_id=item.id,
                payload={"version_id": str(version.id), "version_no": version.version_no},
                actor_kind="HUMAN",
            )
            return _mandate_view(session, item).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation="configuration.mandate.create",
            payload=payload,
            action=action,
            status_code=201,
        )


@router.post("/portfolio-mandates/{mandate_id}/versions", response_model=MandateView, status_code=201)
def create_mandate_version(
    mandate_id: UUID,
    payload: MandateVersionInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    del request
    with session.begin():
        def action() -> dict[str, Any]:
            _require_universes(session, [payload.universe_version_id])
            item = session.execute(
                select(PortfolioMandate)
                .where(PortfolioMandate.id == mandate_id)
                .with_for_update()
            ).scalar_one_or_none()
            if item is None:
                raise QfError("MANDATE_NOT_FOUND", "Portfolio Mandate was not found.", 404)
            last_version = session.scalar(
                select(func.max(PortfolioMandateVersion.version_no)).where(
                    PortfolioMandateVersion.portfolio_mandate_id == item.id
                )
            )
            version = _new_mandate_version(
                mandate_id=item.id, version_no=int(last_version or 0) + 1, payload=payload
            )
            session.add(version)
            session.flush()
            item.latest_version_id = version.id
            _reconcile_initial_portfolio_inputs(session, payload.universe_version_id)
            append_event(
                session,
                kind="PORTFOLIO_MANDATE_VERSION_CREATED",
                aggregate_type="PORTFOLIO_MANDATE",
                aggregate_id=item.id,
                payload={"version_id": str(version.id), "version_no": version.version_no},
                actor_kind="HUMAN",
            )
            return _mandate_view(session, item).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation=f"configuration.mandate.version:{mandate_id}",
            payload=payload,
            action=action,
            status_code=201,
        )


@router.get("/downstream-systems", response_model=DownstreamPage)
def list_downstream_systems(
    limit: int = Query(default=50, ge=1, le=100),
    cursor: UUID | None = None,
    session: Session = Depends(get_session),
) -> DownstreamPage:
    items, next_cursor = _page(session, DownstreamSystem, limit=limit, cursor=cursor)
    return DownstreamPage(
        items=[_downstream_view(item) for item in items], next_cursor=next_cursor
    )


@router.post("/downstream-systems", response_model=DownstreamRegistrationView, status_code=201)
def create_downstream_system(
    payload: CreateDownstreamInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    _reject_secret_fields(payload.public_config)
    normalized = _public_payload(payload)
    with session.begin():
        if idempotency_key:
            existing = session.get(PublicMutationReceipt, idempotency_key)
            if existing is not None:
                if (
                    existing.operation_name != "configuration.downstream.create"
                    or existing.normalized_request != normalized
                ):
                    raise QfError(
                        "IDEMPOTENCY_KEY_REUSED",
                        "The idempotency key belongs to a different request.",
                        409,
                    )
                # The service token is intentionally never retained in a receipt.
                return {**existing.response_json, "service_token": None, "token_issued": True}
        downstream_existing = session.scalar(
            select(DownstreamSystem).where(DownstreamSystem.name == payload.name.strip())
        )
        if downstream_existing is not None:
            raise QfError("DOWNSTREAM_NAME_CONFLICT", "Downstream name already exists.", 409)
        downstream_id = uuid4()
        issued = issue_service_token(request.app.state.settings, downstream_id)
        item = DownstreamSystem(
            id=downstream_id,
            name=payload.name.strip(),
            environment_type=payload.environment_type,
            enabled=payload.enabled,
            package_contract_version=payload.package_contract_version,
            feedback_contract_version=payload.feedback_contract_version,
            compatibility=payload.compatibility,
            # A registration has a credential, but it is not a completed preflight.
            preflight_state="PENDING",
            public_config=payload.public_config,
        )
        install_service_token(item, issued)
        session.add(item)
        session.flush()
        append_event(
            session,
            kind="DOWNSTREAM_REGISTERED",
            aggregate_type="DOWNSTREAM_SYSTEM",
            aggregate_id=item.id,
            payload={"environment_type": item.environment_type},
            actor_kind="HUMAN",
        )
        response = DownstreamRegistrationView(
            **_downstream_view(item).model_dump(), service_token=issued.token, token_issued=True
        ).model_dump(mode="json")
        if idempotency_key:
            session.add(
                PublicMutationReceipt(
                    idempotency_key=idempotency_key,
                    operation_name="configuration.downstream.create",
                    normalized_request=normalized,
                    response_json={**response, "service_token": None},
                    status_code=201,
                    created_at=_now(),
                )
            )
        return response


@router.post(
    "/downstream-systems/{downstream_id}/preflight",
    response_model=DownstreamView,
)
def preflight_downstream_system(
    downstream_id: UUID,
    payload: DownstreamPreflightInput,
    request: Request,
    authorization: str | None = Header(default=None, alias="Authorization"),
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    """Accept a downstream-owned compatibility assertion; never contact its runtime."""
    with session.begin():
        item = session.execute(
            select(DownstreamSystem)
            .where(DownstreamSystem.id == downstream_id)
            .with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("DOWNSTREAM_NOT_FOUND", "Downstream System was not found.", 404)
        # Authenticate before checking an idempotency receipt: a stale key never
        # replays readiness to a caller without the currently installed token.
        authenticate_downstream(request.app.state.settings, item, authorization)
        operation = f"configuration.downstream.preflight:{downstream_id}:{item.revision}"

        def action() -> dict[str, Any]:
            if not item.enabled:
                raise QfError("DOWNSTREAM_DISABLED", "Downstream System is disabled.", 409)
            if (
                payload.package_contract_version != item.package_contract_version
                or payload.feedback_contract_version != item.feedback_contract_version
                or payload.compatibility != item.compatibility
            ):
                raise QfError(
                    "DOWNSTREAM_PREFLIGHT_CONTRACT_MISMATCH",
                    "Preflight must echo the registered downstream contracts exactly.",
                    409,
                )
            feedback_contract_snapshot(item, item.environment_type)
            checked_at = _now()
            if payload.valid_until <= checked_at:
                raise QfError(
                    "DOWNSTREAM_PREFLIGHT_VALIDITY_INVALID",
                    "valid_until must still be in the future.",
                    422,
                )
            receipt_revision = int(
                session.scalar(
                    select(func.max(PreflightReceipt.revision)).where(
                        PreflightReceipt.resource_type == "DOWNSTREAM_SYSTEM",
                        PreflightReceipt.resource_id == item.id,
                    )
                )
                or 0
            ) + 1
            session.add(
                PreflightReceipt(
                    resource_type="DOWNSTREAM_SYSTEM",
                    resource_id=item.id,
                    resource_revision=item.revision,
                    revision=receipt_revision,
                    status="READY",
                    reason_codes=[],
                    capabilities=list(payload.compatibility),
                    contract_version=item.feedback_contract_version,
                    checked_at=checked_at,
                    valid_until=payload.valid_until,
                    checker_version="downstream-service-v1",
                )
            )
            item.preflight_state = "READY"
            session.flush()
            append_event(
                session,
                kind="DOWNSTREAM_PREFLIGHT_COMPLETED",
                aggregate_type="DOWNSTREAM_SYSTEM",
                aggregate_id=item.id,
                payload={
                    "resource_revision": item.revision,
                    "preflight_revision": receipt_revision,
                    "valid_until": payload.valid_until.isoformat(),
                },
                actor_kind="DOWNSTREAM",
            )
            return _downstream_view(item).model_dump(mode="json")

        return _idempotent(
            session,
            key=idempotency_key,
            operation=operation,
            payload=payload,
            action=action,
            status_code=200,
        )


@router.post(
    "/downstream-systems/{downstream_id}/rotate-service-token",
    response_model=DownstreamTokenView,
)
def rotate_downstream_service_token(
    downstream_id: UUID,
    payload: RotateDownstreamTokenInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    normalized = _public_payload(payload)
    with session.begin():
        operation = f"configuration.downstream.rotate-service-token:{downstream_id}"
        if idempotency_key:
            existing = session.get(PublicMutationReceipt, idempotency_key)
            if existing is not None:
                if existing.operation_name != operation or existing.normalized_request != normalized:
                    raise QfError(
                        "IDEMPOTENCY_KEY_REUSED",
                        "The idempotency key belongs to a different request.",
                        409,
                    )
                return {**existing.response_json, "service_token": None}
        item = session.execute(
            select(DownstreamSystem)
            .where(DownstreamSystem.id == downstream_id)
            .with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("DOWNSTREAM_NOT_FOUND", "Downstream System was not found.", 404)
        issued = issue_service_token(request.app.state.settings, item.id)
        install_service_token(item, issued)
        item.revision += 1
        item.preflight_state = "PENDING"
        append_event(
            session,
            kind="DOWNSTREAM_SERVICE_TOKEN_ROTATED",
            aggregate_type="DOWNSTREAM_SYSTEM",
            aggregate_id=item.id,
            payload={"resource_revision": item.revision},
            actor_kind="HUMAN",
        )
        response = DownstreamTokenView(
            downstream_system_id=item.id, service_token=issued.token
        ).model_dump(mode="json")
        if idempotency_key:
            session.add(
                PublicMutationReceipt(
                    idempotency_key=idempotency_key,
                    operation_name=operation,
                    normalized_request=normalized,
                    response_json={**response, "service_token": None},
                    status_code=200,
                    created_at=_now(),
                )
            )
        return response
