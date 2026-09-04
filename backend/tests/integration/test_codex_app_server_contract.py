from __future__ import annotations

import openai_codex
from datetime import UTC, datetime
from pathlib import Path
from dataclasses import replace
from uuid import uuid4

from agent_harness.contracts import (
    DraftArtifactKind,
    MissionContractV1,
    MissionTool,
    MissionType,
    RoleProfile,
)
from agent_harness.mcp_server import freeze_mission_contract
from openai_codex.generated.v2_all import ListMcpServerStatusResponse
from settings import Settings
from runners.research_missions import (
    _codex_service_tier,
    _codex_thread_config,
    _mission_mcp_config_override,
)


def test_runtime_controls_round_trip_through_pinned_codex_app_server(
    settings: Settings,
    tmp_path: Path,
) -> None:
    from openai_codex import ApprovalMode, Codex, CodexConfig, Sandbox

    assert openai_codex.__version__ == "0.144.4"
    cases = (
        (
            replace(settings, codex_reasoning_effort="high", codex_fast_mode=True),
            "fast",
        ),
        (
            replace(settings, codex_reasoning_effort=None, codex_fast_mode=False),
            None,
        ),
    )

    codex_home = tmp_path / "codex-home"
    codex_home.mkdir()
    with Codex(
        CodexConfig(
            env={
                "CODEX_HOME": str(codex_home),
                "RUST_LOG": "error",
            }
        )
    ) as codex:
        for configured, expected_service_tier in cases:
            thread = codex.thread_start(
                approval_mode=ApprovalMode.deny_all,
                ephemeral=True,
                cwd=str(tmp_path),
                model=configured.codex_model,
                service_tier=_codex_service_tier(configured),
                sandbox=Sandbox.workspace_write,
                config=_codex_thread_config(configured),
            )
            assert thread.id
            assert _codex_service_tier(configured) == expected_service_tier


def test_pinned_codex_app_server_starts_only_the_frozen_mission_mcp(tmp_path: Path) -> None:
    from openai_codex import Codex, CodexConfig

    codex_home = tmp_path / "codex-home"
    codex_home.mkdir()
    contract_file = tmp_path / "mission-contract.json"
    freeze_mission_contract(
        contract_file,
        MissionContractV1(
            mission_id=uuid4(),
            mission_type=MissionType.PLAN_RESEARCH,
            role_profile=RoleProfile.RESEARCH_PLANNER,
            objective="Read the bounded Charter.",
            charter_snapshot={"research_question": "Test the MCP boundary."},
            branch_snapshot={"branch_id": str(uuid4())},
            allowed_tools=(MissionTool.GET_MISSION_CONTRACT, MissionTool.GET_CHARTER),
            expected_output_schemas=(DraftArtifactKind.RESEARCH_PLAN,),
            success_criteria=("The Charter is available.",),
            failure_conditions=("The contract file is unavailable.",),
            max_turns=1,
            max_tool_calls=0,
            deadline=datetime(2026, 9, 4, tzinfo=UTC),
        ),
    )
    source_root = Path(__file__).parents[2] / "src"
    with Codex(
        CodexConfig(
            cwd=str(tmp_path),
            env={
                "CODEX_HOME": str(codex_home),
                "PYTHONPATH": str(source_root),
                "RUST_LOG": "error",
            },
            config_overrides=(_mission_mcp_config_override(contract_file),),
        )
    ) as codex:
        statuses = codex._client.request(
            "mcpServerStatus/list",
            {},
            response_model=ListMcpServerStatusResponse,
        )

    assert [status.name for status in statuses.data] == ["quazonai_mission"]
    assert set(statuses.data[0].tools) == {
        MissionTool.GET_MISSION_CONTRACT,
        MissionTool.GET_CHARTER,
    }
