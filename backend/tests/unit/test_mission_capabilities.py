from __future__ import annotations

import asyncio
import json
import sys
import tomllib
from datetime import UTC, datetime, timedelta
from pathlib import Path
from uuid import UUID

import pytest
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from mcp.server.fastmcp import FastMCP
from mcp.server.fastmcp.exceptions import ToolError
from sqlalchemy import select
from sqlalchemy.orm import Session

from agent_harness.contracts import (
    DraftArtifactKind,
    MissionContractV1,
    MissionTool,
    MissionType,
    RoleProfile,
)
from agent_harness.mcp_server import build_frozen_mission_mcp_server, freeze_mission_contract
from agent_harness.mission_capabilities import (
    MissionCapabilityBroker,
    MissionCapabilityService,
    _bounded_non_alpha_payload,
)
from db.models import (
    AgentSession,
    AgentTurn,
    DatasetRevision,
    MissionArtifact,
    QuantRuntimeRun,
    ResearchBranch,
    ResearchCharter,
    ResearchMission,
    ResearchProgram,
    SearchLedgerEntry,
)
from db.session import create_session_factory
from errors import QfError
from runners.research_missions import _mission_mcp_config_override


def _call(server: FastMCP, tool: MissionTool, arguments: dict[str, object]) -> dict[str, object]:
    return json.loads(asyncio.run(server.call_tool(tool, {"arguments": arguments}))[0].text)


def _seed(engine) -> tuple[MissionContractV1, UUID, UUID, UUID, UUID]:  # type: ignore[no-untyped-def]
    now = datetime.now(UTC)
    with Session(engine) as session, session.begin():
        charter = ResearchCharter(
            original_idea_text="Test scoped Discovery evidence.",
            research_question="Can one Mission read only its Discovery facts?",
            market_scope=[], universe_version_ids=[], prediction_horizon="1D", allowed_data_domains=[],
            explicit_exclusions=[], material_assumptions=[], system_assumptions=[],
            clarification_transcript=[], created_at=now,
        )
        session.add(charter)
        session.flush()
        program = ResearchProgram(charter_id=charter.id, title="MCP test", state="ACTIVE", revision=1)
        session.add(program)
        session.flush()
        branch = ResearchBranch(
            program_id=program.id, derivation_type="ROOT", hypothesis="Discovery stays scoped.",
            changed_assumptions=[], preserved_constraints=[], state="ACTIVE", revision_no=1,
            created_at=now,
        )
        session.add(branch)
        session.flush()
        tools = tuple(
            tool
            for tool in MissionTool
            if tool
            in {
                MissionTool.GET_MISSION_CONTRACT,
                MissionTool.GET_CHARTER,
                MissionTool.PROFILE_DATASET,
                MissionTool.LIST_PRIOR_ATTEMPTS,
                MissionTool.QUERY_SEARCH_LEDGER,
                MissionTool.QUERY_ALPHA_LIBRARY,
                MissionTool.GET_RUN_EVIDENCE,
                MissionTool.SUBMIT_MISSION_ARTIFACT,
            }
        )
        mission = ResearchMission(
            program_id=program.id, branch_id=branch.id, mission_type="ALPHA_DISCOVERY",
            role_profile="ALPHA_RESEARCHER", state="RUNNING", objective="Inspect Discovery facts.",
            contract_version="v1", input_snapshot={},
            capability_snapshot={"allowed_tools": [tool.value for tool in tools]}, runtime_snapshot={},
            prompt_version="v1", max_turns=7, max_tool_calls=20, started_at=now, attempt=1, revision=3,
        )
        session.add(mission)
        session.flush()
        agent_session = AgentSession(
            mission_id=mission.id,
            role_profile=mission.role_profile,
            codex_thread_id="mcp-test-thread",
            codex_version="test",
            state="RUNNING",
            started_at=now,
            last_event_at=now,
        )
        session.add(agent_session)
        session.flush()
        session.add(
            AgentTurn(
                agent_session_id=agent_session.id,
                ordinal=1,
                kind="EXECUTE",
                codex_turn_id="mcp-test-turn",
                state="RUNNING",
                started_at=now,
            )
        )
        session.flush()
        datasets = [
            DatasetRevision(
                revision_no=index, data_class="FIXTURE", origin="test", ingested_at=now,
                promotability="NON_PROMOTABLE", schema_version="v1", event_start=now, event_end=now,
                available_start=now, available_end=now, row_count=2, quality_state="VALID",
                point_in_time_state="VALID", partition=partition, materialization_request={}, created_at=now,
            )
            for index, partition in ((1, "DISCOVERY"), (2, "SEALED"))
        ]
        session.add_all(datasets)
        session.flush()
        run = QuantRuntimeRun(
            program_id=program.id, branch_id=branch.id, mission_id=mission.id, mode="DISCOVERY",
            state="SUCCEEDED", experiment_key="bounded-test", family="mean-reversion",
            catalog_uri="catalog://discovery-only", runtime_name="research-engine",
            strategy_artifact={}, parameters={}, evidence={},
        )
        session.add(run)
        session.flush()
        session.add(
            SearchLedgerEntry(
                program_id=program.id, branch_id=branch.id, mission_id=mission.id, run_id=run.id,
                family="mean-reversion", parameters={"lookback": 5}, outcome="REJECTED",
                failure_code="INSUFFICIENT_EVIDENCE", disclosure_level="DISCOVERY_FULL",
                evidence_summary={"observed_rows": 2}, created_at=now,
            )
        )
        contract = MissionContractV1(
            mission_id=mission.id, mission_type=MissionType.ALPHA_DISCOVERY,
            role_profile=RoleProfile.ALPHA_RESEARCHER, objective=mission.objective or "Inspect facts.",
            charter_snapshot={"charter_id": str(charter.id)}, branch_snapshot={"branch_id": str(branch.id)},
            allowed_tools=tools, allowed_dataset_revision_ids=tuple(item.id for item in datasets),
            expected_output_schemas=(DraftArtifactKind.ALPHA_PROPOSAL,),
            success_criteria=("Submit a typed DraftArtifact.",), failure_conditions=("Evidence unavailable.",),
            max_turns=7, max_tool_calls=20, deadline=now + timedelta(hours=1),
        )
        return contract, mission.id, datasets[0].id, datasets[1].id, run.id


def _alpha_payload() -> dict[str, object]:
    return {
        "family_key": "mean-reversion",
        "requested_role": "PRIMARY_ALPHA",
        "universe_version_id": str(UUID("00000000-0000-0000-0000-000000000001")),
        "horizon": "1D",
        "feature_pipeline_ref": str(UUID("00000000-0000-0000-0000-000000000002")),
        "source_path": "alphas/mean_reversion.py",
        "entrypoint": "alphas.mean_reversion:build_alpha",
        "parameters": {"lookback": 20},
        "input_contract": {"feature_schema": "FeatureFrameV1"},
        "output_contract": "AlphaSignalFrameV1",
        "hypothesis": "Lagged cross-sectional returns retain predictive information.",
        "falsification_criteria": ["Discovery rank correlation is non-positive."],
        "known_limitations": ["Sealed evaluation remains required."],
    }


def test_mcp_reads_discovery_and_persists_a_revision_guarded_draft(engine, tmp_path: Path) -> None:  # type: ignore[no-untyped-def]
    contract, mission_id, discovery_id, _, run_id = _seed(engine)
    contract_file = tmp_path / "contract.json"
    freeze_mission_contract(contract_file, contract)
    with MissionCapabilityBroker(contract, create_session_factory(engine)) as broker:
        assert broker.socket_path.stat().st_mode & 0o077 == 0
        server = build_frozen_mission_mcp_server(contract_file, broker.socket_path)
        assert MissionTool.RUN_DISCOVERY_EXPERIMENT not in {
            tool.name for tool in asyncio.run(server.list_tools())
        }
        assert _call(
            server, MissionTool.PROFILE_DATASET,
            {"mission_id": str(mission_id), "dataset_revision_id": str(discovery_id)},
        )["partition"] == "DISCOVERY"
        evidence = _call(
            server, MissionTool.GET_RUN_EVIDENCE,
            {"mission_id": str(mission_id), "run_id": str(run_id)},
        )
        assert evidence["evidence_summary"] == {"observed_rows": 2}
        assert _call(server, MissionTool.LIST_PRIOR_ATTEMPTS, {"mission_id": str(mission_id)})[
            "items"
        ][0]["run_id"] == str(run_id)
        assert _call(
            server, MissionTool.QUERY_SEARCH_LEDGER,
            {"mission_id": str(mission_id), "family": "mean-reversion"},
        )["items"][0]["run_id"] == str(run_id)
        assert _call(server, MissionTool.QUERY_ALPHA_LIBRARY, {"mission_id": str(mission_id)}) == {
            "items": [], "omitted_count": 0
        }
        request = {
            "mission_id": str(mission_id), "idempotency_key": "draft-1", "expected_revision": 3,
            "artifact": {
                "mission_id": str(mission_id), "kind": "ALPHA_PROPOSAL",
                "summary": "Two observed Discovery rows; no promotion claim.",
                "payload": _alpha_payload(),
            },
        }
        submitted = _call(server, MissionTool.SUBMIT_MISSION_ARTIFACT, request)
        assert _call(server, MissionTool.SUBMIT_MISSION_ARTIFACT, request) == submitted
        with pytest.raises(ToolError, match="MISSION_REVISION_CONFLICT"):
            _call(
                server, MissionTool.SUBMIT_MISSION_ARTIFACT,
                {**request, "idempotency_key": "draft-stale", "artifact": {**request["artifact"], "summary": "stale"}},
            )

    with Session(engine) as session:
        artifact = session.get(MissionArtifact, UUID(str(submitted["artifact_id"])))
        mission = session.get(ResearchMission, mission_id)
        assert artifact is not None
        assert artifact.metadata_json["payload"]["source_path"] == "alphas/mean_reversion.py"
        assert artifact.metadata_json["payload"]["output_contract"] == "AlphaSignalFrameV1"
        assert mission is not None and mission.revision == 4


def test_mcp_child_and_fixed_bridge_hard_deny_sealed_secrets_and_control(engine, tmp_path: Path) -> None:  # type: ignore[no-untyped-def]
    contract, mission_id, _, sealed_id, run_id = _seed(engine)
    contract_file = tmp_path / "contract.json"
    freeze_mission_contract(contract_file, contract)

    async def read_child(socket_path: Path) -> dict[str, object]:
        parameters = StdioServerParameters(
            command=sys.executable,
            args=["-m", "agent_harness.mcp_server", "--contract-file", str(contract_file),
                  "--capability-socket", str(socket_path)],
            env={"PYTHONPATH": str(Path(__file__).resolve().parents[2] / "src")},
        )
        async with stdio_client(parameters) as streams:
            async with ClientSession(*streams) as session:
                await session.initialize()
                result = await session.call_tool(
                    MissionTool.GET_RUN_EVIDENCE,
                    {"arguments": {"mission_id": str(mission_id), "run_id": str(run_id)}},
                )
        return json.loads(result.content[0].text)

    factory = create_session_factory(engine)
    with MissionCapabilityBroker(contract, factory) as broker:
        server = build_frozen_mission_mcp_server(contract_file, broker.socket_path)
        assert asyncio.run(read_child(broker.socket_path))["run_id"] == str(run_id)
        with pytest.raises(ToolError, match="MISSION_DATASET_SEALED"):
            _call(server, MissionTool.PROFILE_DATASET, {"mission_id": str(mission_id), "dataset_revision_id": str(sealed_id)})
        with pytest.raises(ToolError, match="MISSION_FACT_CONTAINS_SECRET"):
            _call(
                server, MissionTool.SUBMIT_MISSION_ARTIFACT,
                {"mission_id": str(mission_id), "idempotency_key": "secret", "expected_revision": 3,
                 "artifact": {"mission_id": str(mission_id), "kind": "ALPHA_PROPOSAL", "summary": "no secret", "payload": {"token": "never"}}},
            )
    service = MissionCapabilityService(contract, factory)
    for name in ("credential.read", "approval.approve", "handoff.publish"):
        with pytest.raises(QfError, match="MISSION_TOOL_INVALID"):
            service.invoke(name, {"mission_id": str(mission_id)})


def test_mcp_rejects_alpha_payloads_outside_the_typed_public_contract(engine, tmp_path: Path) -> None:  # type: ignore[no-untyped-def]
    contract, mission_id, _, _, _ = _seed(engine)
    contract_file = tmp_path / "contract.json"
    freeze_mission_contract(contract_file, contract)
    with MissionCapabilityBroker(contract, create_session_factory(engine)) as broker:
        server = build_frozen_mission_mcp_server(contract_file, broker.socket_path)
        with pytest.raises(ToolError, match="ALPHA_ARTIFACT_DRAFT_INVALID"):
            _call(
                server,
                MissionTool.SUBMIT_MISSION_ARTIFACT,
                {
                    "mission_id": str(mission_id),
                    "idempotency_key": "sealed-draft",
                    "expected_revision": 3,
                    "artifact": {
                        "mission_id": str(mission_id),
                        "kind": "ALPHA_PROPOSAL",
                        "summary": "No public artifact may include a sealed identifier.",
                        "payload": {
                            **_alpha_payload(),
                            "parameters": {"sealed_dataset_revision_id": str(UUID(int=3))},
                        },
                    },
                },
            )
        with pytest.raises(ToolError, match="ALPHA_ARTIFACT_DRAFT_INVALID"):
            _call(
                server,
                MissionTool.SUBMIT_MISSION_ARTIFACT,
                {
                    "mission_id": str(mission_id),
                    "idempotency_key": "url-summary",
                    "expected_revision": 3,
                    "artifact": {
                        "mission_id": str(mission_id),
                        "kind": "ALPHA_PROPOSAL",
                        "summary": "Details are at https://example.invalid/alpha.",
                        "payload": _alpha_payload(),
                    },
                },
            )


def test_non_alpha_artifacts_require_kind_specific_facts() -> None:
    with pytest.raises(QfError, match="MISSION_ARTIFACT_SCHEMA_INVALID"):
        _bounded_non_alpha_payload(
            DraftArtifactKind.DATA_QUALITY_REPORT,
            {"quality": {"summary": "ok", "items": ["ok"]}},
        )
    payload = _bounded_non_alpha_payload(
        DraftArtifactKind.DATA_QUALITY_REPORT,
        {
            "quality": {
                "summary": "quality and PIT checks completed",
                "items": ["coverage is complete"],
                "facts": {
                    "dataset_revision_id": "00000000-0000-0000-0000-000000000001",
                    "quality_state": "VALID",
                    "pit_state": "VALID",
                },
            }
        },
    )
    assert payload["quality"]["facts"]["quality_state"] == "VALID"


def test_mission_tool_calls_are_counted_and_budgeted(engine) -> None:
    contract, mission_id, _, _, _ = _seed(engine)
    factory = create_session_factory(engine)
    with factory() as session, session.begin():
        mission = session.get(ResearchMission, mission_id)
        assert mission is not None
        mission.max_tool_calls = 1
    service = MissionCapabilityService(contract, factory)
    args = {"mission_id": str(mission_id)}
    service.invoke(MissionTool.LIST_PRIOR_ATTEMPTS.value, args)
    with pytest.raises(QfError, match="MISSION_TOOL_CALL_BUDGET_EXCEEDED"):
        service.invoke(MissionTool.LIST_PRIOR_ATTEMPTS.value, args)
    with factory() as session:
        turn = session.scalar(select(AgentTurn))
        assert turn is not None and turn.tool_call_count == 1


def test_mcp_config_passes_a_socket_not_a_database_url(tmp_path: Path) -> None:
    override = _mission_mcp_config_override(tmp_path / "contract.json", tmp_path / "m.sock")
    args = tomllib.loads(override)["mcp_servers"]["quazonai_mission"]["args"]
    assert args[-2:] == ["--capability-socket", str(tmp_path / "m.sock")]
    assert "DATABASE_URL" not in override and "TOKEN" not in override
