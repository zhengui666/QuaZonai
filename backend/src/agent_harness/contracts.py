"""Fixed, unprivileged contracts for Issue #58 research missions.

This module deliberately describes only finite research work.  It is not a
workflow language and it does not expose Secret, Sealed, Approval, Handoff,
or Administration capabilities.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping
from datetime import datetime
from enum import StrEnum
from json import dumps
from math import isfinite
from pathlib import PurePosixPath
import re
from types import MappingProxyType
from typing import Any, Literal
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)


class RoleProfile(StrEnum):
    RESEARCH_PLANNER = "RESEARCH_PLANNER"
    DATA_STEWARD = "DATA_STEWARD"
    ALPHA_RESEARCHER = "ALPHA_RESEARCHER"
    ROBUSTNESS_VALIDATOR = "ROBUSTNESS_VALIDATOR"
    PORTFOLIO_ARCHITECT = "PORTFOLIO_ARCHITECT"
    DEGRADATION_INVESTIGATOR = "DEGRADATION_INVESTIGATOR"
    INDEPENDENT_REVIEWER = "INDEPENDENT_REVIEWER"


AgentRole = RoleProfile


class MissionType(StrEnum):
    PLAN_RESEARCH = "PLAN_RESEARCH"
    DATA_REQUIREMENT = "DATA_REQUIREMENT"
    DATA_QUALITY = "DATA_QUALITY"
    FEATURE_RESEARCH = "FEATURE_RESEARCH"
    ALPHA_DISCOVERY = "ALPHA_DISCOVERY"
    ALPHA_CALIBRATION = "ALPHA_CALIBRATION"
    ROBUSTNESS = "ROBUSTNESS"
    SEALED_PROMOTION_REVIEW = "SEALED_PROMOTION_REVIEW"
    PORTFOLIO_ASSEMBLY = "PORTFOLIO_ASSEMBLY"
    PAPER_EVIDENCE_REVIEW = "PAPER_EVIDENCE_REVIEW"
    LIVE_PROMOTION_REVIEW = "LIVE_PROMOTION_REVIEW"
    DEGRADATION_DIAGNOSIS = "DEGRADATION_DIAGNOSIS"
    REPLAN = "REPLAN"


class MissionTool(StrEnum):
    GET_MISSION_CONTRACT = "get_mission_contract"
    GET_CHARTER = "get_charter"
    PROFILE_DATASET = "profile_dataset"
    LIST_PRIOR_ATTEMPTS = "list_prior_attempts"
    QUERY_SEARCH_LEDGER = "query_search_ledger"
    QUERY_ALPHA_LIBRARY = "query_alpha_library"
    VALIDATE_ALPHA_ARTIFACT = "validate_alpha_artifact"
    RUN_DISCOVERY_EXPERIMENT = "run_discovery_experiment"
    GET_RUN_EVIDENCE = "get_run_evidence"
    SUBMIT_MISSION_ARTIFACT = "submit_mission_artifact"
    PROPOSE_MISSION_GRAPH = "propose_mission_graph"
    GET_PORTFOLIO_CONTEXT = "get_portfolio_context"
    VALIDATE_PORTFOLIO_PROPOSAL = "validate_portfolio_proposal"


_COMMON_TOOLS = frozenset(
    {
        MissionTool.GET_MISSION_CONTRACT,
        MissionTool.GET_CHARTER,
        MissionTool.LIST_PRIOR_ATTEMPTS,
        MissionTool.SUBMIT_MISSION_ARTIFACT,
    }
)

ROLE_TOOL_ALLOWLIST: Mapping[RoleProfile, frozenset[MissionTool]] = MappingProxyType(
    {
        RoleProfile.RESEARCH_PLANNER: _COMMON_TOOLS
        | {
            MissionTool.QUERY_SEARCH_LEDGER,
            MissionTool.PROPOSE_MISSION_GRAPH,
        },
        RoleProfile.DATA_STEWARD: _COMMON_TOOLS
        | {
            MissionTool.PROFILE_DATASET,
            MissionTool.QUERY_SEARCH_LEDGER,
        },
        RoleProfile.ALPHA_RESEARCHER: _COMMON_TOOLS
        | {
            MissionTool.PROFILE_DATASET,
            MissionTool.QUERY_SEARCH_LEDGER,
            MissionTool.QUERY_ALPHA_LIBRARY,
            MissionTool.VALIDATE_ALPHA_ARTIFACT,
            MissionTool.RUN_DISCOVERY_EXPERIMENT,
            MissionTool.GET_RUN_EVIDENCE,
        },
        RoleProfile.ROBUSTNESS_VALIDATOR: _COMMON_TOOLS
        | {
            MissionTool.PROFILE_DATASET,
            MissionTool.QUERY_SEARCH_LEDGER,
            MissionTool.QUERY_ALPHA_LIBRARY,
            MissionTool.VALIDATE_ALPHA_ARTIFACT,
            MissionTool.RUN_DISCOVERY_EXPERIMENT,
            MissionTool.GET_RUN_EVIDENCE,
        },
        RoleProfile.PORTFOLIO_ARCHITECT: _COMMON_TOOLS
        | {
            MissionTool.QUERY_ALPHA_LIBRARY,
            MissionTool.GET_PORTFOLIO_CONTEXT,
            MissionTool.VALIDATE_PORTFOLIO_PROPOSAL,
        },
        RoleProfile.DEGRADATION_INVESTIGATOR: _COMMON_TOOLS
        | {
            MissionTool.QUERY_SEARCH_LEDGER,
            MissionTool.QUERY_ALPHA_LIBRARY,
            MissionTool.GET_RUN_EVIDENCE,
            MissionTool.PROPOSE_MISSION_GRAPH,
        },
        RoleProfile.INDEPENDENT_REVIEWER: _COMMON_TOOLS
        | {
            MissionTool.QUERY_SEARCH_LEDGER,
            MissionTool.QUERY_ALPHA_LIBRARY,
            MissionTool.VALIDATE_ALPHA_ARTIFACT,
            MissionTool.GET_RUN_EVIDENCE,
            MissionTool.GET_PORTFOLIO_CONTEXT,
            MissionTool.VALIDATE_PORTFOLIO_PROPOSAL,
        },
    }
)


def role_allowed_tools(role_profile: RoleProfile | str) -> frozenset[MissionTool]:
    """Return the fixed allowlist for one Issue #58 role."""

    return ROLE_TOOL_ALLOWLIST[RoleProfile(role_profile)]


def effective_allowed_tools(
    role_profile: RoleProfile | str,
    contract_allowed_tools: Iterable[MissionTool | str],
) -> frozenset[MissionTool]:
    """Apply the required role-allowlist ∩ contract-allowlist rule."""

    requested = frozenset(MissionTool(tool) for tool in contract_allowed_tools)
    return role_allowed_tools(role_profile) & requested


class DraftArtifactKind(StrEnum):
    RESEARCH_PLAN = "RESEARCH_PLAN"
    DATA_REQUIREMENT = "DATA_REQUIREMENT"
    DATA_QUALITY_REPORT = "DATA_QUALITY_REPORT"
    FEATURE_PROPOSAL = "FEATURE_PROPOSAL"
    ALPHA_PROPOSAL = "ALPHA_PROPOSAL"
    CALIBRATION_PROPOSAL = "CALIBRATION_PROPOSAL"
    ROBUSTNESS_REPORT = "ROBUSTNESS_REPORT"
    PROMOTION_REVIEW = "PROMOTION_REVIEW"
    PORTFOLIO_PROPOSAL = "PORTFOLIO_PROPOSAL"
    PAPER_EVIDENCE_REVIEW = "PAPER_EVIDENCE_REVIEW"
    LIVE_PROMOTION_REVIEW = "LIVE_PROMOTION_REVIEW"
    DEGRADATION_REPORT = "DEGRADATION_REPORT"
    REPLAN_PROPOSAL = "REPLAN_PROPOSAL"
    MISSION_GRAPH_PROPOSAL = "MISSION_GRAPH_PROPOSAL"


class AlphaRequestedRole(StrEnum):
    """Advisory role vocabulary; Core freezes the effective role later."""

    PRIMARY_ALPHA = "PRIMARY_ALPHA"
    DIVERSIFIER_ALPHA = "DIVERSIFIER_ALPHA"
    HEDGE_ALPHA = "HEDGE_ALPHA"
    REGIME_SIGNAL = "REGIME_SIGNAL"
    RISK_MODULATOR = "RISK_MODULATOR"
    SHADOW_ALPHA = "SHADOW_ALPHA"


_ALPHA_FAMILY_KEY = re.compile(r"^[a-z][a-z0-9_-]{0,159}$")
_ALPHA_HORIZON = re.compile(r"^[1-9][0-9]{0,3}[SMHDW]$")
_ALPHA_ENTRYPOINT = re.compile(
    r"^[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*:[A-Za-z_][A-Za-z0-9_]*$"
)
_ALPHA_URL = re.compile(r"(?i)\b[a-z][a-z0-9+.-]*://|\bmailto:")
_ALPHA_URI_VALUE = re.compile(r"(?i)^[a-z][a-z0-9+.-]*:")
_ALPHA_UUID = re.compile(
    r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b"
)
_ALPHA_SECRET_ASSIGNMENT = re.compile(
    r"(?i)\b(?:access[_-]?token|api[_-]?key|password|private[_-]?key|refresh[_-]?token|"
    r"secret|service[_-]?token|token)\s*[:=]"
)
_ALPHA_SECRET_KEYS = frozenset(
    {
        "accesstoken",
        "apikey",
        "password",
        "privatekey",
        "refreshtoken",
        "secret",
        "secretkey",
        "servicetoken",
        "token",
    }
)
_ALPHA_FORBIDDEN_METADATA_KEYS = frozenset(
    {
        "approvalstate",
        "artifacturi",
        "formalmetrics",
        "handoffstate",
        "metrics",
        "metric",
        "portfolioweight",
        "promotionresult",
        "qualificationstate",
        "raw",
        "rawdata",
        "rawreturn",
        "rawreturns",
        "rawsignal",
        "rawsignals",
        "realizedreturn",
        "returns",
        "returnseries",
        "score",
        "scores",
        "sealedresult",
        "signalframe",
        "signalframes",
        "signals",
        "sourceartifacturi",
        "uri",
        "url",
    }
)
_MAX_ALPHA_METADATA_BYTES = 16 * 1024
_MAX_ALPHA_METADATA_DEPTH = 4
_MAX_ALPHA_METADATA_ITEMS = 64


def _alpha_text(value: str, field_name: str, max_length: int) -> str:
    normalized = value.strip()
    if not normalized:
        raise ValueError(f"{field_name} must not be blank")
    if len(normalized) > max_length:
        raise ValueError(f"{field_name} exceeds its bounded length")
    if _ALPHA_URL.search(normalized):
        raise ValueError(f"{field_name} must not contain a URL")
    if _ALPHA_UUID.search(normalized):
        raise ValueError(f"{field_name} must not contain a governed identifier")
    if _ALPHA_SECRET_ASSIGNMENT.search(normalized):
        raise ValueError(f"{field_name} must not contain a credential")
    return normalized


def _alpha_metadata_key(key: object, path: str) -> str:
    if not isinstance(key, str):
        raise ValueError(f"{path} keys must be strings")
    normalized = key.strip()
    if not normalized or len(normalized) > 80:
        raise ValueError(f"{path} contains an invalid key")
    compact = re.sub(r"[^a-z0-9]", "", normalized.casefold())
    if (
        compact in _ALPHA_SECRET_KEYS
        or compact.endswith("secret")
        or compact.endswith("token")
        or "dataset" in compact
        or "sealed" in compact
        or "policy" in compact
        or "metric" in compact
        or compact in _ALPHA_FORBIDDEN_METADATA_KEYS
        or compact.endswith("uri")
        or compact.endswith("url")
    ):
        raise ValueError(f"{path}.{normalized} is not permitted in an Alpha draft")
    return normalized


def _validate_alpha_metadata(value: object, path: str = "$", depth: int = 0) -> None:
    if depth > _MAX_ALPHA_METADATA_DEPTH:
        raise ValueError("Alpha draft metadata is nested too deeply")
    if isinstance(value, dict):
        if len(value) > _MAX_ALPHA_METADATA_ITEMS:
            raise ValueError(f"{path} has too many items")
        for key, item in value.items():
            _validate_alpha_metadata(item, f"{path}.{_alpha_metadata_key(key, path)}", depth + 1)
        return
    if isinstance(value, (list, tuple)):
        if len(value) > _MAX_ALPHA_METADATA_ITEMS:
            raise ValueError(f"{path} has too many items")
        for ordinal, item in enumerate(value):
            _validate_alpha_metadata(item, f"{path}[{ordinal}]", depth + 1)
        return
    if isinstance(value, str):
        if (
            len(value) > 4_000
            or _ALPHA_URL.search(value)
            or _ALPHA_URI_VALUE.match(value)
            or _ALPHA_UUID.search(value)
            or _ALPHA_SECRET_ASSIGNMENT.search(value)
        ):
            raise ValueError(f"{path} must not contain unbounded text or a URL")
        return
    if value is None or isinstance(value, (bool, int)):
        return
    if isinstance(value, float) and isfinite(value):
        return
    raise ValueError(f"{path} must contain only finite JSON values")


class AlphaArtifactDraftV1(StrictModel):
    """The public, worktree-local proposal an Alpha Mission may submit."""

    family_key: str = Field(min_length=1, max_length=160)
    requested_role: AlphaRequestedRole
    universe_version_id: UUID
    horizon: str = Field(min_length=2, max_length=5)
    feature_pipeline_ref: UUID | None = None
    source_path: str = Field(min_length=1, max_length=500)
    entrypoint: str = Field(min_length=3, max_length=320)
    parameters: dict[str, Any] = Field(default_factory=dict)
    input_contract: dict[str, Any] = Field(min_length=1)
    output_contract: Literal["AlphaSignalFrameV1"]
    hypothesis: str = Field(min_length=1, max_length=4_000)
    falsification_criteria: tuple[str, ...] = Field(min_length=1, max_length=20)
    known_limitations: tuple[str, ...] = Field(min_length=1, max_length=20)

    @field_validator("family_key")
    @classmethod
    def require_family_key(cls, value: str) -> str:
        normalized = value.strip().casefold()
        if not _ALPHA_FAMILY_KEY.fullmatch(normalized):
            raise ValueError("family_key must be a bounded lowercase key")
        return normalized

    @field_validator("horizon")
    @classmethod
    def require_horizon(cls, value: str) -> str:
        normalized = value.strip().upper()
        if not _ALPHA_HORIZON.fullmatch(normalized):
            raise ValueError("horizon must be a bounded duration such as 1D")
        return normalized

    @field_validator("source_path")
    @classmethod
    def require_worktree_relative_source_path(cls, value: str) -> str:
        normalized = value.strip()
        parts = normalized.split("/")
        path = PurePosixPath(normalized)
        if (
            "\\" in normalized
            or "\x00" in normalized
            or ":" in normalized
            or normalized.startswith("~")
            or path.is_absolute()
            or any(part in {"", ".", ".."} or part.startswith(".") for part in parts)
            or len(parts) > 32
        ):
            raise ValueError("source_path must stay worktree-relative")
        return path.as_posix()

    @field_validator("entrypoint")
    @classmethod
    def require_entrypoint(cls, value: str) -> str:
        normalized = value.strip()
        if not _ALPHA_ENTRYPOINT.fullmatch(normalized):
            raise ValueError("entrypoint must be a Python module:callable reference")
        return normalized

    @field_validator("parameters", "input_contract")
    @classmethod
    def require_public_metadata(cls, value: dict[str, Any]) -> dict[str, Any]:
        _validate_alpha_metadata(value)
        try:
            size = len(dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode())
        except (TypeError, ValueError) as exc:
            raise ValueError("Alpha draft metadata must be JSON") from exc
        if size > _MAX_ALPHA_METADATA_BYTES:
            raise ValueError("Alpha draft metadata exceeds its bounded size")
        return value

    @field_validator("hypothesis")
    @classmethod
    def require_hypothesis(cls, value: str) -> str:
        return _alpha_text(value, "hypothesis", 4_000)

    @field_validator("falsification_criteria", "known_limitations")
    @classmethod
    def require_public_text_lists(
        cls, value: tuple[str, ...], info: Any
    ) -> tuple[str, ...]:
        return tuple(_alpha_text(item, info.field_name, 1_000) for item in value)


def validate_alpha_artifact_summary(value: str) -> str:
    """Keep the enclosing public summary free of URLs and governed identifiers too."""

    return _alpha_text(value, "summary", 10_000)


class DraftArtifact(StrictModel):
    """An agent proposal; a Domain Service remains the only formal writer."""

    mission_id: UUID
    kind: DraftArtifactKind
    schema_version: Literal["v1"] = "v1"
    summary: str = Field(min_length=1, max_length=10_000)
    payload: dict[str, Any] = Field(default_factory=dict)

    @field_validator("summary")
    @classmethod
    def require_summary(cls, value: str) -> str:
        value = value.strip()
        if not value:
            raise ValueError("summary must not be blank")
        return value


DraftArtifactV1 = DraftArtifact


class FixedAutonomyBudgetV1(StrictModel):
    """The Issue #58 finite-cycle limits, frozen instead of caller-configurable."""

    max_parallel_missions: Literal[3] = 3
    max_total_missions: Literal[20] = 20
    max_replans: Literal[3] = 3
    max_alpha_experiments_per_mission: Literal[20] = 20
    max_repair_turns: Literal[2] = 2
    max_wall_clock_seconds_per_mission: Literal[3600] = 3600


AutonomyBudgetV1 = FixedAutonomyBudgetV1
FIXED_AUTONOMY_BUDGET = FixedAutonomyBudgetV1()


def _unique(value: tuple[Any, ...], field_name: str) -> tuple[Any, ...]:
    if len(value) != len(set(value)):
        raise ValueError(f"{field_name} must not contain duplicates")
    return value


class MissionContractV1(StrictModel):
    mission_id: UUID
    mission_type: MissionType
    role_profile: RoleProfile
    objective: str = Field(min_length=1, max_length=10_000)
    charter_snapshot: dict[str, Any] = Field(min_length=1)
    branch_snapshot: dict[str, Any] = Field(min_length=1)
    input_artifact_ids: tuple[UUID, ...] = ()
    allowed_tools: tuple[MissionTool, ...] = ()
    allowed_dataset_revision_ids: tuple[UUID, ...] = ()
    expected_output_schemas: tuple[DraftArtifactKind, ...] = Field(min_length=1)
    success_criteria: tuple[str, ...] = Field(min_length=1)
    failure_conditions: tuple[str, ...] = Field(min_length=1)
    max_turns: int = Field(ge=1)
    max_tool_calls: int = Field(ge=0)
    deadline: datetime

    @field_validator("objective")
    @classmethod
    def require_objective(cls, value: str) -> str:
        value = value.strip()
        if not value:
            raise ValueError("objective must not be blank")
        return value

    @field_validator(
        "input_artifact_ids",
        "allowed_tools",
        "allowed_dataset_revision_ids",
        "expected_output_schemas",
    )
    @classmethod
    def reject_duplicate_values(cls, value: tuple[Any, ...], info: Any) -> tuple[Any, ...]:
        return _unique(value, info.field_name)

    @field_validator("success_criteria", "failure_conditions")
    @classmethod
    def normalize_conditions(cls, value: tuple[str, ...], info: Any) -> tuple[str, ...]:
        normalized = tuple(item.strip() for item in value)
        if any(not item for item in normalized):
            raise ValueError(f"{info.field_name} must not contain blank values")
        return _unique(normalized, info.field_name)

    @field_validator("deadline")
    @classmethod
    def require_aware_deadline(cls, value: datetime) -> datetime:
        if value.tzinfo is None or value.utcoffset() is None:
            raise ValueError("deadline must include a timezone")
        return value

    @property
    def effective_tools(self) -> frozenset[MissionTool]:
        return effective_allowed_tools(self.role_profile, self.allowed_tools)


class MissionNodeProposal(StrictModel):
    """One fixed-type node in a proposed, not-yet-persisted Mission DAG."""

    node_id: str = Field(pattern=r"^[a-z][a-z0-9_-]{0,79}$")
    mission_type: MissionType
    role_profile: RoleProfile
    objective: str = Field(min_length=1, max_length=10_000)
    depends_on: tuple[str, ...] = ()

    @field_validator("objective")
    @classmethod
    def require_node_objective(cls, value: str) -> str:
        value = value.strip()
        if not value:
            raise ValueError("objective must not be blank")
        return value

    @field_validator("depends_on")
    @classmethod
    def validate_dependencies(cls, value: tuple[str, ...]) -> tuple[str, ...]:
        for dependency in value:
            if not dependency:
                raise ValueError("dependencies must not contain blank node ids")
        return _unique(value, "depends_on")


class MissionGraphProposal(StrictModel):
    """A finite DAG proposal containing only fixed Issue #58 Mission types."""

    nodes: tuple[MissionNodeProposal, ...] = Field(min_length=1, max_length=20)

    @model_validator(mode="after")
    def validate_dag(self) -> "MissionGraphProposal":
        nodes_by_id = {node.node_id: node for node in self.nodes}
        if len(nodes_by_id) != len(self.nodes):
            raise ValueError("mission graph node_id values must be unique")

        for node in self.nodes:
            for dependency in node.depends_on:
                if dependency == node.node_id:
                    raise ValueError(f"mission graph node {node.node_id} cannot depend on itself")
                if dependency not in nodes_by_id:
                    raise ValueError(
                        f"mission graph node {node.node_id} depends on unknown node {dependency}"
                    )

        visiting: set[str] = set()
        visited: set[str] = set()

        def visit(node_id: str) -> None:
            if node_id in visiting:
                raise ValueError(f"mission graph contains a cycle at {node_id}")
            if node_id in visited:
                return
            visiting.add(node_id)
            for dependency in nodes_by_id[node_id].depends_on:
                visit(dependency)
            visiting.remove(node_id)
            visited.add(node_id)

        for node in self.nodes:
            visit(node.node_id)
        return self


MissionGraphNodeV1 = MissionNodeProposal
