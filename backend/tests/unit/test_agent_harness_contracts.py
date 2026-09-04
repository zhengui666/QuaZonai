from __future__ import annotations

from datetime import UTC, datetime
from uuid import uuid4

import pytest
from pydantic import ValidationError

from agent_harness.contracts import (
    AlphaArtifactDraftV1,
    AutonomyBudgetV1,
    DraftArtifact,
    DraftArtifactKind,
    MissionContractV1,
    MissionGraphProposal,
    MissionNodeProposal,
    MissionTool,
    MissionType,
    RoleProfile,
    effective_allowed_tools,
)


def _contract(**overrides: object) -> MissionContractV1:
    values: dict[str, object] = {
        "mission_id": uuid4(),
        "mission_type": MissionType.PLAN_RESEARCH,
        "role_profile": RoleProfile.RESEARCH_PLANNER,
        "objective": "Plan bounded discovery work.",
        "charter_snapshot": {"charter_id": str(uuid4())},
        "branch_snapshot": {"branch_id": str(uuid4())},
        "expected_output_schemas": (DraftArtifactKind.RESEARCH_PLAN,),
        "success_criteria": ("A finite graph is proposed.",),
        "failure_conditions": ("Required evidence is unavailable.",),
        "max_turns": 4,
        "max_tool_calls": 12,
        "deadline": datetime(2026, 9, 4, tzinfo=UTC),
    }
    values.update(overrides)
    return MissionContractV1.model_validate(values)


def _node(node_id: str, depends_on: tuple[str, ...] = ()) -> MissionNodeProposal:
    return MissionNodeProposal(
        node_id=node_id,
        mission_type=MissionType.PLAN_RESEARCH,
        role_profile=RoleProfile.RESEARCH_PLANNER,
        objective="Plan bounded research.",
        depends_on=depends_on,
    )


def _alpha_payload() -> dict[str, object]:
    return {
        "family_key": "mean-reversion",
        "requested_role": "PRIMARY_ALPHA",
        "universe_version_id": str(uuid4()),
        "horizon": "1D",
        "feature_pipeline_ref": str(uuid4()),
        "source_path": "alphas/mean_reversion.py",
        "entrypoint": "alphas.mean_reversion:build_alpha",
        "parameters": {"lookback": 20},
        "input_contract": {"feature_schema": "FeatureFrameV1"},
        "output_contract": "AlphaSignalFrameV1",
        "hypothesis": "Lagged cross-sectional returns retain predictive information.",
        "falsification_criteria": ("Discovery rank correlation is non-positive.",),
        "known_limitations": ("Sealed evaluation remains required.",),
    }


def test_contract_is_strict_and_only_exposes_fixed_tools() -> None:
    contract = _contract(
        allowed_tools=(MissionTool.PROPOSE_MISSION_GRAPH, MissionTool.RUN_DISCOVERY_EXPERIMENT),
    )

    assert contract.effective_tools == frozenset({MissionTool.PROPOSE_MISSION_GRAPH})
    assert effective_allowed_tools(
        RoleProfile.RESEARCH_PLANNER,
        (MissionTool.PROPOSE_MISSION_GRAPH, MissionTool.RUN_DISCOVERY_EXPERIMENT),
    ) == frozenset({MissionTool.PROPOSE_MISSION_GRAPH})

    with pytest.raises(ValidationError):
        _contract(allowed_tools=("approve_handoff",))
    with pytest.raises(ValidationError):
        _contract(unapproved_field=True)


def test_fixed_autonomy_budget_cannot_be_escalated() -> None:
    assert AutonomyBudgetV1().max_total_missions == 20

    with pytest.raises(ValidationError):
        AutonomyBudgetV1(max_total_missions=21)


def test_draft_artifact_is_an_unprivileged_strict_envelope() -> None:
    artifact = DraftArtifact(
        mission_id=uuid4(),
        kind=DraftArtifactKind.RESEARCH_PLAN,
        summary="A candidate signal requires validation.",
        payload={"plan": "bounded plan"},
    )

    assert artifact.schema_version == "v1"
    assert artifact.payload == {"plan": "bounded plan"}
    with pytest.raises(ValidationError):
        DraftArtifact(
            mission_id=uuid4(),
            kind=DraftArtifactKind.RESEARCH_PLAN,
            summary="candidate",
            state="APPROVED",
        )


def test_alpha_draft_is_typed_worktree_local_and_public() -> None:
    draft = AlphaArtifactDraftV1.model_validate(_alpha_payload())

    assert draft.family_key == "mean-reversion"
    assert draft.horizon == "1D"
    assert draft.source_path == "alphas/mean_reversion.py"

    for override in (
        {"source_path": "../outside.py"},
        {"source_path": "/tmp/outside.py"},
        {"source_path": "C:/outside.py"},
        {"source_path": ".git/config"},
        {"entrypoint": "https://example.invalid/alpha"},
        {"parameters": {"sealed_dataset_revision_id": str(uuid4())}},
        {"parameters": {"return_series": [0.01]}},
        {"parameters": {"metrics": {"sharpe": 1.0}}},
        {"parameters": {"api_key": "never"}},
        {"parameters": {"external_ref": "s3:private-alpha"}},
        {"input_contract": {"hidden_reference": str(uuid4())}},
        {"hypothesis": "api_key=never must not be stored."},
        {"input_contract": {"documentation": "https://example.invalid/schema"}},
        {"output_contract": "FreeFormOutput"},
        {"policy_version_id": str(uuid4())},
    ):
        with pytest.raises(ValidationError):
            AlphaArtifactDraftV1.model_validate({**_alpha_payload(), **override})


@pytest.mark.parametrize(
    ("nodes", "message"),
    [
        ((_node("plan", ("data",)), _node("data", ("plan",))), "cycle"),
        ((_node("plan", ("plan",)),), "cannot depend on itself"),
        ((_node("plan", ("missing",)),), "unknown node"),
    ],
)
def test_graph_rejects_invalid_dependencies(
    nodes: tuple[MissionNodeProposal, ...],
    message: str,
) -> None:
    with pytest.raises(ValidationError, match=message):
        MissionGraphProposal(nodes=nodes)


def test_graph_accepts_fixed_type_dag_and_rejects_custom_type() -> None:
    proposal = MissionGraphProposal(nodes=(_node("plan"), _node("data", ("plan",))))

    assert proposal.nodes[1].depends_on == ("plan",)
    with pytest.raises(ValidationError):
        MissionNodeProposal(
            node_id="custom",
            mission_type="CUSTOM_WORKFLOW",
            role_profile=RoleProfile.RESEARCH_PLANNER,
            objective="Do arbitrary work.",
        )
