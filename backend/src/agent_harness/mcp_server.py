"""Small, contract-scoped stdio MCP server for one Research Mission.

The process receives a frozen contract and, when a Worker starts one, a
private scoped Unix socket.  It never receives a database URL, secret,
approval, handoff, or downstream runtime capability.
"""

from __future__ import annotations

import argparse
import inspect
import json
import os
import stat
from collections.abc import Callable, Mapping
from pathlib import Path
from typing import Any, TypeAlias

from mcp.server.fastmcp import FastMCP
from mcp.server.fastmcp.exceptions import ToolError

from agent_harness.contracts import MissionContractV1, MissionTool
from agent_harness.mission_capabilities import (
    MissionCapabilityClient,
    MissionCapabilityError,
    implemented_mission_tools,
)


MissionToolHandler: TypeAlias = Callable[[dict[str, Any]], Any]

_TOOL_DESCRIPTIONS: Mapping[MissionTool, str] = {
    MissionTool.GET_MISSION_CONTRACT: "Return the frozen non-secret Mission contract. No arguments.",
    MissionTool.GET_CHARTER: "Return the frozen non-secret Charter snapshot. No arguments.",
    MissionTool.PROFILE_DATASET: "Read a granted Discovery dataset profile. arguments requires mission_id and dataset_revision_id.",
    MissionTool.LIST_PRIOR_ATTEMPTS: "Read scoped Discovery attempts. arguments requires mission_id and optional limit.",
    MissionTool.QUERY_SEARCH_LEDGER: "Read scoped Discovery ledger entries. arguments requires mission_id, optional family and limit.",
    MissionTool.QUERY_ALPHA_LIBRARY: "Read scoped Alpha schema summaries. arguments requires mission_id and optional limit.",
    MissionTool.GET_RUN_EVIDENCE: "Read one scoped Discovery evidence summary. arguments requires mission_id and run_id.",
    MissionTool.SUBMIT_MISSION_ARTIFACT: (
        "Submit one typed DraftArtifact. ALPHA_PROPOSAL payloads require AlphaArtifactDraftV1. "
        "Non-Alpha v1 payloads are {kind: {summary, items, facts}} with exact facts: "
        "RESEARCH_PLAN(objective,hypotheses), DATA_REQUIREMENT(dataset_scope,requirements), "
        "DATA_QUALITY_REPORT(dataset_revision_id,quality_state,pit_state), "
        "FEATURE_PROPOSAL(family,input_contract), CALIBRATION_PROPOSAL(model_version_id,method), "
        "ROBUSTNESS_REPORT(checks,outcome), PROMOTION_REVIEW(candidate_id,decision), "
        "PORTFOLIO_PROPOSAL(candidate_id,weights), PAPER_EVIDENCE_REVIEW and "
        "LIVE_PROMOTION_REVIEW(evidence_episode_id,decision), DEGRADATION_REPORT(subject_id,state), "
        "REPLAN_PROPOSAL(cause_event_id,changes), MISSION_GRAPH_PROPOSAL(nodes). "
        "DATA_QUALITY_REPORT requires both quality_state and pit_state VALID. "
        "Arguments require mission_id, idempotency_key, expected_revision and artifact."
    ),
}


def freeze_mission_contract(path: Path, contract: MissionContractV1) -> None:
    """Create the one immutable, non-secret contract file for a Mission."""
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    payload = contract.model_dump(mode="json")
    try:
        descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o400)
    except FileExistsError:
        if load_mission_contract(path) != contract:
            raise ValueError("frozen Mission contract does not match the admitted Mission") from None
        return
    with os.fdopen(descriptor, "w", encoding="utf-8") as contract_file:
        json.dump(payload, contract_file, separators=(",", ":"), sort_keys=True)
        contract_file.write("\n")
    os.chmod(path, 0o400)


def load_mission_contract(path: Path) -> MissionContractV1:
    """Read one regular, immutable contract file without touching Core state."""
    details = path.stat(follow_symlinks=False)
    if not stat.S_ISREG(details.st_mode) or details.st_mode & 0o222:
        raise ValueError("Mission contract must be a read-only regular file")
    with path.open(encoding="utf-8") as contract_file:
        return MissionContractV1.model_validate(json.load(contract_file))


class MissionToolBridge:
    """Authorize a Mission tool before its injected handler can run."""

    def __init__(
        self,
        contract: MissionContractV1,
        handlers: Mapping[MissionTool | str, MissionToolHandler],
    ) -> None:
        self._allowed = contract.effective_tools
        self._handlers = {self._tool(name): handler for name, handler in handlers.items()}
        if not all(callable(handler) for handler in self._handlers.values()):
            raise TypeError("Mission MCP handlers must be callable")

    @property
    def exposed_tools(self) -> tuple[MissionTool, ...]:
        """Tools with both a contract grant and an injected implementation."""
        return tuple(sorted(self._allowed & self._handlers.keys(), key=str))

    async def invoke(
        self,
        tool_name: MissionTool | str,
        arguments: Mapping[str, Any] | None = None,
    ) -> Any:
        """Hard-deny non-effective tools before resolving a handler."""
        tool = self._tool(tool_name)
        if tool not in self._allowed:
            raise ToolError(f"Mission tool is not allowed: {tool.value}")
        handler = self._handlers.get(tool)
        if handler is None:
            raise ToolError(f"Mission tool is unavailable: {tool.value}")
        result = handler(dict(arguments or {}))
        return await result if inspect.isawaitable(result) else result

    @staticmethod
    def _tool(tool_name: MissionTool | str) -> MissionTool:
        try:
            return MissionTool(tool_name)
        except (TypeError, ValueError) as exc:
            raise ToolError(f"Unknown Mission tool: {tool_name}") from exc


def build_mission_mcp_server(
    contract: MissionContractV1,
    handlers: Mapping[MissionTool | str, MissionToolHandler],
) -> FastMCP:
    """Build a real stdio-capable FastMCP server for one trusted contract."""
    bridge = MissionToolBridge(contract, handlers)
    server = FastMCP(name=f"quazonai-mission-{contract.mission_id}")
    for tool in bridge.exposed_tools:
        server.tool(name=tool.value, description=_TOOL_DESCRIPTIONS.get(tool, "Mission-scoped capability."))(
            _fastmcp_tool(bridge, tool)
        )
    return server


def run_mission_mcp_server(
    contract: MissionContractV1,
    handlers: Mapping[MissionTool | str, MissionToolHandler],
) -> None:
    """Run the isolated server on the stable stdio transport."""
    build_mission_mcp_server(contract, handlers).run("stdio")


def build_frozen_mission_mcp_server(
    contract_file: Path, capability_socket: Path | None = None
) -> FastMCP:
    """Expose frozen facts plus the fixed Worker-owned capability bridge, if present."""
    contract = load_mission_contract(contract_file)
    contract_payload = contract.model_dump(mode="json")
    charter_payload = dict(contract.charter_snapshot)

    def get_mission_contract(arguments: dict[str, Any]) -> dict[str, Any]:
        if arguments:
            raise ToolError("get_mission_contract does not accept arguments")
        return contract_payload

    def get_charter(arguments: dict[str, Any]) -> dict[str, Any]:
        if arguments:
            raise ToolError("get_charter does not accept arguments")
        return charter_payload

    handlers: dict[MissionTool | str, MissionToolHandler] = {
        MissionTool.GET_MISSION_CONTRACT: get_mission_contract,
        MissionTool.GET_CHARTER: get_charter,
    }
    if capability_socket is not None:
        client = MissionCapabilityClient(capability_socket)

        def capability_handler(tool: MissionTool) -> MissionToolHandler:
            def call(arguments: dict[str, Any]) -> dict[str, Any]:
                try:
                    return client.call(tool, arguments)
                except MissionCapabilityError as exc:
                    raise ToolError(f"{exc.code}: {exc.message}") from exc

            return call

        handlers.update({tool: capability_handler(tool) for tool in implemented_mission_tools()})
    return build_mission_mcp_server(
        contract,
        handlers,
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one contract-scoped QuaZonai Mission MCP server")
    parser.add_argument("--contract-file", type=Path, required=True)
    parser.add_argument("--capability-socket", type=Path)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    build_frozen_mission_mcp_server(args.contract_file, args.capability_socket).run("stdio")
    return 0


def _fastmcp_tool(bridge: MissionToolBridge, tool: MissionTool) -> MissionToolHandler:
    async def call(arguments: dict[str, Any] | None = None) -> Any:
        """Pass the tool-specific JSON object as ``arguments``."""
        return await bridge.invoke(tool, arguments)

    return call


if __name__ == "__main__":
    raise SystemExit(main())
