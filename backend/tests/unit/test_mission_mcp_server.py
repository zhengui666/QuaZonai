from __future__ import annotations

import asyncio
import json
import sys
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

import pytest
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from mcp.server.fastmcp import FastMCP
from mcp.server.fastmcp.exceptions import ToolError

from agent_harness.contracts import (
    DraftArtifactKind,
    MissionContractV1,
    MissionTool,
    MissionType,
    RoleProfile,
)
from agent_harness.mcp_server import (
    MissionToolBridge,
    build_frozen_mission_mcp_server,
    build_mission_mcp_server,
    freeze_mission_contract,
    load_mission_contract,
)


def _contract() -> MissionContractV1:
    return MissionContractV1(
        mission_id=uuid4(),
        mission_type=MissionType.PLAN_RESEARCH,
        role_profile=RoleProfile.RESEARCH_PLANNER,
        objective="Propose a bounded Mission graph.",
        charter_snapshot={"charter_id": str(uuid4())},
        branch_snapshot={"branch_id": str(uuid4())},
        allowed_tools=(
            MissionTool.PROPOSE_MISSION_GRAPH,
            MissionTool.RUN_DISCOVERY_EXPERIMENT,
        ),
        expected_output_schemas=(DraftArtifactKind.RESEARCH_PLAN,),
        success_criteria=("A finite graph is proposed.",),
        failure_conditions=("Required evidence is unavailable.",),
        max_turns=4,
        max_tool_calls=12,
        deadline=datetime(2026, 9, 4, tzinfo=UTC),
    )


def test_server_exposes_only_effective_tools_and_uses_fastmcp() -> None:
    called: list[dict[str, object]] = []

    def propose(arguments: dict[str, object]) -> dict[str, object]:
        called.append(arguments)
        return {"accepted": True}

    def discovery(_: dict[str, object]) -> None:
        raise AssertionError("a planner must never invoke discovery")

    server = build_mission_mcp_server(
        _contract(),
        {
            MissionTool.PROPOSE_MISSION_GRAPH: propose,
            MissionTool.RUN_DISCOVERY_EXPERIMENT: discovery,
        },
    )

    assert isinstance(server, FastMCP)
    assert [tool.name for tool in asyncio.run(server.list_tools())] == [
        MissionTool.PROPOSE_MISSION_GRAPH
    ]
    assert asyncio.run(
        server.call_tool(MissionTool.PROPOSE_MISSION_GRAPH, {"arguments": {"nodes": 1}})
    )
    assert called == [{"nodes": 1}]


def test_disallowed_tool_is_denied_before_its_handler() -> None:
    called = False

    def discovery(_: dict[str, object]) -> None:
        nonlocal called
        called = True

    bridge = MissionToolBridge(
        _contract(), {MissionTool.RUN_DISCOVERY_EXPERIMENT: discovery}
    )

    with pytest.raises(ToolError, match="not allowed"):
        asyncio.run(bridge.invoke(MissionTool.RUN_DISCOVERY_EXPERIMENT))

    assert not called


def test_frozen_contract_server_exposes_only_the_two_read_only_facts(tmp_path: Path) -> None:
    contract = _contract().model_copy(
        update={
            "allowed_tools": (
                MissionTool.GET_MISSION_CONTRACT,
                MissionTool.GET_CHARTER,
                MissionTool.RUN_DISCOVERY_EXPERIMENT,
            )
        }
    )
    contract_file = tmp_path / "mission-contract.json"
    freeze_mission_contract(contract_file, contract)

    assert contract_file.stat().st_mode & 0o222 == 0
    assert load_mission_contract(contract_file) == contract
    server = build_frozen_mission_mcp_server(contract_file)

    assert [tool.name for tool in asyncio.run(server.list_tools())] == [
        MissionTool.GET_CHARTER,
        MissionTool.GET_MISSION_CONTRACT,
    ]
    charter = asyncio.run(server.call_tool(MissionTool.GET_CHARTER, {"arguments": {}}))
    assert json.loads(charter[0].text) == contract.charter_snapshot
    with pytest.raises(ToolError, match="does not accept arguments"):
        asyncio.run(server.call_tool(MissionTool.GET_CHARTER, {"arguments": {"extra": True}}))


def test_mission_mcp_cli_serves_the_frozen_contract_over_stdio(tmp_path: Path) -> None:
    contract = _contract().model_copy(
        update={
            "allowed_tools": (MissionTool.GET_MISSION_CONTRACT, MissionTool.GET_CHARTER),
        }
    )
    contract_file = tmp_path / "mission-contract.json"
    freeze_mission_contract(contract_file, contract)

    async def call_server() -> tuple[list[str], dict[str, object]]:
        source_root = Path(__file__).resolve().parents[2] / "src"
        parameters = StdioServerParameters(
            command=sys.executable,
            args=["-m", "agent_harness.mcp_server", "--contract-file", str(contract_file)],
            env={"PYTHONPATH": str(source_root)},
        )
        async with stdio_client(parameters) as streams:
            async with ClientSession(*streams) as session:
                await session.initialize()
                tools = await session.list_tools()
                charter = await session.call_tool(MissionTool.GET_CHARTER, {})
        return [tool.name for tool in tools.tools], json.loads(charter.content[0].text)

    names, charter = asyncio.run(call_server())

    assert names == [MissionTool.GET_CHARTER, MissionTool.GET_MISSION_CONTRACT]
    assert charter == contract.charter_snapshot
