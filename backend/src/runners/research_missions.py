"""Execute one Research Mission through the official Codex app-server runtime."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
from collections.abc import Iterator
from contextlib import ExitStack, contextmanager
from dataclasses import replace
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any
from uuid import UUID

from sqlalchemy import func, select

from agent_harness.contracts import (
    FIXED_AUTONOMY_BUDGET,
    MissionContractV1,
    MissionTool,
    MissionType,
    RoleProfile,
)
from agent_harness.mission_capabilities import MissionCapabilityBroker
from agent_harness.mcp_server import freeze_mission_contract, load_mission_contract
from agent_harness.orchestrator import await_mission_validation, expected_output_kind, finish_mission
from agent_harness.runtime import begin_turn, finish_turn, record_session_admission
from db.models import (
    AgentSession,
    AgentTurn,
    Event,
    Job,
    ResearchBranch,
    ResearchCharter,
    ResearchMission,
    ResearchProgram,
)
from db.session import SessionFactory, create_database_engine, create_session_factory
from errors import QfError
from jobs import JobLease, create_lease_fenced_session_factory
from research_engine.alpha_intake import stage_alpha_discovery_evaluation
from runtime_config import load_effective_settings
from runners.codex_sandbox import codex_sandbox_preflight
from settings import Settings

CUSTOM_CODEX_PROVIDER_ID = "quazonai_configured"
DEFAULT_OPENAI_API_BASE_URL = "https://api.openai.com/v1"
BROKER_REQUEST = b"TOKEN\n"
BROKER_ACCEPT_POLL_SECONDS = 0.25
_EFFECTIVE_CODEX_RUNTIME_KEY = "effective_codex_runtime"

def _execution_turns(mission_type: MissionType) -> tuple[tuple[str, str], ...]:
    if mission_type is MissionType.DEGRADATION_DIAGNOSIS:
        return (
            (
                "DIAGNOSE",
                "Diagnose only the persisted degradation context in MISSION.md. Inspect permitted "
                "evidence, write a concise DIAGNOSIS.md and RESULT.md, and do not change formal facts "
                "or contact a downstream system.",
            ),
        )
    if mission_type is MissionType.REPLAN:
        return (
            (
                "REPLAN",
                "Propose only a bounded replan from the persisted degradation context. Write RESULT.md "
                "with falsifiable next research steps; do not change formal facts or contact a downstream "
                "system.",
            ),
        )
    return (
        (
            "PLAN",
            "Plan the bounded Mission in MISSION.md. Inspect only the permitted worktree and "
            "Mission-scoped MCP evidence. Write a concise PLAN.md with falsifiable acceptance "
            "criteria; do not claim results yet.",
        ),
        (
            "IMPLEMENT",
            "Implement the approved plan in MISSION.md using only permitted Mission-scoped "
            "tools. Prepare bounded research artifacts without claiming results that have not "
            "been observed.",
        ),
        (
            "VALIDATE",
            "Validate the current work against MISSION.md and actual permitted evidence. Write "
            "VALIDATION.md with observed checks, failures, and limitations; do not infer or invent "
            "evidence.",
        ),
        (
            "EXECUTE",
            "Execute only the bounded research steps accepted by VALIDATION.md using actual "
            "Mission-scoped tools or evidence. Update RESULT.md with only observed outputs and "
            "state any unavailable evidence explicitly.",
        ),
    )


def _required_turn_kinds(mission_type: MissionType) -> tuple[str, ...]:
    return tuple(kind for kind, _ in _execution_turns(mission_type)) + ("REVIEW",)


def _now() -> datetime:
    return datetime.now(UTC)


def _event(
    session: object,
    *,
    kind: str,
    program_id: UUID,
    mission_id: UUID,
    payload: dict[str, object] | None = None,
) -> None:
    session.add(  # type: ignore[attr-defined]
        Event(
            kind=kind,
            aggregate_type="RESEARCH_PROGRAM",
            aggregate_id=program_id,
            actor_kind="SYSTEM",
            actor_metadata={},
            payload={"mission_id": str(mission_id), **(payload or {})},
        )
    )


def _git(*args: str, cwd: Path | None = None, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=cwd,
        check=check,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=30,
    )


def _prepare_worktree(
    settings: Settings,
    program_id: UUID,
    mission_id: UUID,
    *,
    recorded_workspace: str | None = None,
    resuming: bool = False,
) -> Path:
    program_root = settings.mission_root / "programs" / str(program_id)
    repo = program_root / "repo"
    worktrees = settings.mission_root / "worktrees"
    workspace = worktrees / str(mission_id)
    branch = f"mission-{mission_id.hex}"

    if recorded_workspace is not None:
        stored = Path(recorded_workspace).resolve(strict=False)
        expected = workspace.resolve(strict=False)
        if stored != expected:
            raise QfError(
                "MISSION_WORKSPACE_CONFLICT",
                "Mission workspace is outside its durable worktree location.",
                409,
                {"workspace": recorded_workspace},
            )
        if not workspace.exists():
            raise QfError(
                "MISSION_WORKSPACE_MISSING",
                "A durable Mission workspace was removed and cannot be silently recreated.",
                409,
                {"workspace": recorded_workspace},
            )
    elif resuming:
        raise QfError(
            "MISSION_WORKSPACE_MISSING",
            "An interrupted Mission has no durable workspace to resume.",
            409,
        )

    program_root.mkdir(parents=True, exist_ok=True)
    worktrees.mkdir(parents=True, exist_ok=True)

    if not (repo / ".git").exists():
        repo.mkdir(parents=True, exist_ok=True)
        _git("init", "--initial-branch=main", str(repo))
        _git("config", "user.name", "QuaZonai Agent Worker", cwd=repo)
        _git("config", "user.email", "quazonai-agent@localhost", cwd=repo)
        (repo / "README.md").write_text(
            "# QuaZonai Research Program Workspace\n\nDurable research artifacts are produced in isolated Mission worktrees.\n",
            encoding="utf-8",
        )
        _git("add", "README.md", cwd=repo)
        _git("commit", "-m", "Initialize research program workspace", cwd=repo)

    if workspace.exists():
        valid = _git("rev-parse", "--is-inside-work-tree", cwd=workspace, check=False)
        if valid.returncode == 0 and valid.stdout.strip() == "true":
            return workspace
        raise QfError(
            "MISSION_WORKSPACE_CONFLICT",
            "Mission workspace exists but is not the durable Git worktree.",
            409,
            {"workspace": str(workspace)},
        )
    branch_exists = _git(
        "show-ref", "--verify", "--quiet", f"refs/heads/{branch}", cwd=repo, check=False
    ).returncode == 0
    if branch_exists:
        _git("worktree", "add", str(workspace), branch, cwd=repo)
    else:
        _git("worktree", "add", "-b", branch, str(workspace), "HEAD", cwd=repo)
    return workspace


def _effective_codex_runtime(settings: Settings) -> dict[str, object]:
    """Persist only the non-secret settings actually passed to a Mission thread."""
    return {
        "model": settings.codex_model,
        "reasoning_effort": settings.codex_reasoning_effort,
        "fast_mode": settings.codex_fast_mode,
        "base_url": settings.codex_base_url,
        "api_key_configured": settings.codex_api_key is not None,
    }


def _frozen_mission_settings(settings: Settings, runtime_snapshot: dict[str, object]) -> Settings:
    frozen = runtime_snapshot.get(_EFFECTIVE_CODEX_RUNTIME_KEY)
    if frozen is None:
        return settings
    if not isinstance(frozen, dict):
        raise QfError(
            "MISSION_RUNTIME_SNAPSHOT_INVALID",
            "Mission runtime snapshot is invalid.",
            409,
        )

    model = frozen.get("model")
    reasoning_effort = frozen.get("reasoning_effort")
    fast_mode = frozen.get("fast_mode")
    base_url = frozen.get("base_url")
    api_key_configured = frozen.get("api_key_configured")
    if (
        (model is not None and not isinstance(model, str))
        or (reasoning_effort is not None and not isinstance(reasoning_effort, str))
        or not isinstance(fast_mode, bool)
        or (base_url is not None and not isinstance(base_url, str))
        or not isinstance(api_key_configured, bool)
    ):
        raise QfError(
            "MISSION_RUNTIME_SNAPSHOT_INVALID",
            "Mission runtime snapshot is invalid.",
            409,
        )
    if base_url != settings.codex_base_url or api_key_configured != (
        settings.codex_api_key is not None
    ):
        raise QfError(
            "MISSION_RUNTIME_CONFIGURATION_CHANGED",
            "A durable Mission cannot resume through a different provider route.",
            409,
        )
    return replace(
        settings,
        codex_model=model,
        codex_reasoning_effort=reasoning_effort,
        codex_fast_mode=fast_mode,
        codex_base_url=base_url,
    )


def _freeze_effective_codex_runtime(mission: ResearchMission, settings: Settings) -> None:
    if _EFFECTIVE_CODEX_RUNTIME_KEY in mission.runtime_snapshot:
        return
    mission.runtime_snapshot = {
        **mission.runtime_snapshot,
        _EFFECTIVE_CODEX_RUNTIME_KEY: _effective_codex_runtime(settings),
    }


def _mission_context(
    mission: ResearchMission,
    program: ResearchProgram,
    charter: ResearchCharter,
    branch: ResearchBranch,
) -> str:
    degradation = mission.input_snapshot.get("degradation")
    degradation_context = ""
    if isinstance(degradation, dict):
        degradation_context = (
            "\n## Persisted Degradation Context\n"
            f"Forward Evidence episode: {degradation.get('forward_evidence_episode_id')}\n"
            f"Subject: {degradation.get('subject_type')} {degradation.get('subject_id')}\n"
            f"Metric: {degradation.get('metric_name')}\n"
            f"Severity: {degradation.get('severity')}; confidence: {degradation.get('confidence')}\n"
            f"Policy revision: {degradation.get('policy_revision')}\n"
            f"Policy snapshot: {degradation.get('policy_snapshot')}\n"
            f"Reason: {degradation.get('reason_code')}\n"
            f"State: {degradation.get('state')}\n"
            f"Consecutive breaches: {degradation.get('consecutive_breaches')}; "
            f"evaluated: {degradation.get('evaluated')}\n"
        )
    return (
        "# QuaZonai Research Mission\n\n"
        f"Program: {program.title}\n"
        f"Mission ID: {mission.id}\n"
        f"Role: {mission.role or 'RESEARCH_AGENT'}\n"
        f"Mission type: {mission.type}\n"
        f"Objective: {mission.objective or charter.research_question}\n\n"
        "## Frozen Research Charter\n"
        f"Original idea: {charter.original_idea_text}\n"
        f"Research question: {charter.research_question}\n"
        f"Market scope: {charter.market_scope}\n"
        f"Prediction horizon: {charter.prediction_horizon}\n"
        f"Explicit exclusions: {charter.explicit_exclusions}\n"
        f"Material assumptions: {charter.material_assumptions}\n\n"
        "## Branch\n"
        f"Hypothesis: {branch.hypothesis}\n"
        f"Changed assumptions: {branch.changed_assumptions}\n"
        f"Preserved constraints: {branch.preserved_constraints}\n"
        f"{degradation_context}"
    )


def _frozen_contract_file(settings: Settings, mission: ResearchMission) -> Path:
    """Persist one immutable, non-secret contract outside the writable worktree."""
    path = settings.mission_root / "contracts" / f"{mission.id}.json"
    try:
        if path.exists():
            contract = load_mission_contract(path)
        else:
            snapshot = mission.input_snapshot
            charter_snapshot = snapshot.get("charter")
            branch_snapshot = snapshot.get("branch")
            if not isinstance(charter_snapshot, dict) or not isinstance(branch_snapshot, dict):
                raise ValueError("Mission has no frozen Charter and Branch snapshots")
            mission_type = MissionType(mission.mission_type)
            role_profile = RoleProfile(mission.role_profile)
            contract = MissionContractV1(
                mission_id=mission.id,
                mission_type=mission_type,
                role_profile=role_profile,
                objective=mission.objective or "Produce the contract-required typed mission artifact.",
                charter_snapshot=charter_snapshot,
                branch_snapshot=branch_snapshot,
                allowed_tools=tuple(
                    MissionTool(tool)
                    for tool in mission.capability_snapshot.get("allowed_tools", ())
                ),
                allowed_dataset_revision_ids=tuple(
                    UUID(str(dataset_id))
                    for dataset_id in mission.capability_snapshot.get(
                        "allowed_dataset_revision_ids", ()
                    )
                ),
                expected_output_schemas=(expected_output_kind(mission_type.value),),
                success_criteria=("Produce only the contract-required typed mission artifact.",),
                failure_conditions=("Required permitted evidence is unavailable.",),
                max_turns=mission.max_turns,
                max_tool_calls=mission.max_tool_calls,
                deadline=_now()
                + timedelta(seconds=FIXED_AUTONOMY_BUDGET.max_wall_clock_seconds_per_mission),
            )
            freeze_mission_contract(path, contract)
    except (KeyError, OSError, TypeError, ValueError) as exc:
        raise QfError("MISSION_CONTRACT_INVALID", "Mission contract cannot be admitted.", 409) from exc
    if contract.mission_id != mission.id:
        raise QfError("MISSION_CONTRACT_INVALID", "Mission contract belongs to another Mission.", 409)
    return path


def _load_mission_context(
    settings: Settings,
    factory: SessionFactory,
    lease: JobLease,
) -> tuple[UUID, UUID, str, str | None, str | None, dict[str, object], Path]:
    with factory() as session:
        job = session.get(Job, lease.job_id)
        if job is None or job.kind != "RESEARCH_MISSION":
            raise QfError("JOB_NOT_FOUND", "Research Mission job does not exist.", 404)
        if job.state != "LEASED":
            raise QfError("JOB_STATE_CONFLICT", "Research Mission job is not leased.", 409)
        mission = session.get(ResearchMission, job.resource_id)
        if mission is None:
            raise QfError("MISSION_NOT_FOUND", "Research Mission does not exist.", 404)
        if mission.state not in {"READY", "INTERRUPTED"}:
            raise QfError(
                "MISSION_STATE_CONFLICT",
                "Only READY or INTERRUPTED Research Missions may be admitted by the Agent Worker.",
                409,
                {"state": mission.state},
            )
        program = session.get(ResearchProgram, mission.program_id)
        branch = session.get(ResearchBranch, mission.branch_id)
        if program is None or branch is None:
            raise QfError("MISSION_CONTEXT_MISSING", "Mission Program or Branch is missing.", 500)
        if program.state != "ACTIVE":
            raise QfError(
                "PROGRAM_NOT_ACTIVE",
                "Paused or archived Programs cannot admit a Mission.",
                409,
                {"state": program.state},
            )
        charter = session.get(ResearchCharter, program.charter_id)
        if charter is None:
            raise QfError("MISSION_CONTEXT_MISSING", "Mission Charter is missing.", 500)
        return (
            mission.id,
            program.id,
            _mission_context(mission, program, charter, branch),
            mission.codex_thread_id,
            mission.workspace_path,
            dict(mission.runtime_snapshot),
            _frozen_contract_file(settings, mission),
        )


@contextmanager
def _provider_credential_broker(api_key: str | None) -> Iterator[Path | None]:
    """Expose a configured provider key exactly once to Codex's auth helper.

    The broker remains available for the lifetime of the pending Codex session
    instead of expiring on an arbitrary startup deadline. Mission commands cannot
    execute until the first provider request succeeds, so the one-shot socket is
    consumed before Mission-owned shell code can run. The plaintext never enters
    the App Server environment or command line.
    """
    if not api_key:
        yield None
        return

    root = Path(tempfile.mkdtemp(prefix="quazonai-codex-auth-"))
    os.chmod(root, 0o700)
    socket_path = root / "token.sock"
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(str(socket_path))
    os.chmod(socket_path, 0o600)
    server.listen(1)
    server.settimeout(BROKER_ACCEPT_POLL_SECONDS)
    stop_event = threading.Event()

    def serve_once() -> None:
        try:
            connection: socket.socket | None = None
            while not stop_event.is_set():
                try:
                    connection, _ = server.accept()
                    break
                except socket.timeout:
                    continue
            if connection is None:
                return

            with connection:
                connection.settimeout(5.0)
                request = bytearray()
                while len(request) < len(BROKER_REQUEST):
                    chunk = connection.recv(len(BROKER_REQUEST) - len(request))
                    if not chunk:
                        break
                    request.extend(chunk)
                if bytes(request) != BROKER_REQUEST:
                    return
                connection.sendall(api_key.encode("utf-8"))
        except (OSError, TimeoutError):
            return
        finally:
            try:
                server.close()
            except OSError:
                pass

    thread = threading.Thread(target=serve_once, name="codex-provider-auth", daemon=True)
    thread.start()
    try:
        yield socket_path
    finally:
        stop_event.set()
        try:
            server.close()
        except OSError:
            pass
        thread.join(timeout=2.0)
        shutil.rmtree(root, ignore_errors=True)


def _mission_mcp_config_override(
    contract_file: Path, capability_socket: Path | None = None
) -> str:
    """Serialize the only Mission MCP server as one TOML config override."""
    arguments = ["-m", "agent_harness.mcp_server", "--contract-file", str(contract_file)]
    if capability_socket is not None:
        arguments.extend(("--capability-socket", str(capability_socket)))
    serialized_arguments = ", ".join(json.dumps(argument) for argument in arguments)
    return (
        "mcp_servers = { quazonai_mission = { command = "
        f"{json.dumps(sys.executable)}, args = [{serialized_arguments}] }} }}"
    )


def _codex_launch_configuration(
    settings: Settings,
    workspace: Path,
    *,
    credential_socket: Path | None = None,
    mission_contract_file: Path | None = None,
    mission_capability_socket: Path | None = None,
) -> tuple[Any, str | None]:
    """Build App Server launch config without placing provider secrets in its environment."""
    from openai_codex import CodexConfig

    environment = {
        "CODEX_HOME": str(settings.codex_home),
        "OPENAI_API_KEY": "",
        "CODEX_API_KEY": "",
        "QUAZONAI_CODEX_API_KEY": "",
        # The App Server does not need Core bootstrap, Operator, or quant-runtime secrets.
        # Explicitly clear them from the environment inherited by Mission-owned
        # child processes, including non-Compose local launches.
        "QUAZONAI_MASTER_KEY": "",
        "QUAZONAI_DATABASE_URL": "",
        "QUAZONAI_ALEMBIC_URL": "",
        "POSTGRES_PASSWORD": "",
        "QUAZONAI_AUTH_ENABLED": "",
        # Deprecated browser credentials are still scrubbed so stale host secrets
        # can never leak into Mission-owned child processes.
        "QUAZONAI_AUTH_USERNAME": "",
        "QUAZONAI_AUTH_PASSWORD": "",
        "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT": "",
        "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT": "",
        "QUAZONAI_AUTH_TOTP_SECRET": "",
        "QUAZONAI_AUTH_COOKIE_KEY": "",
        "QUAZONAI_API_TOKEN": "",
        "QUAZONAI_AUTH_PUBLIC_ORIGIN": "",
        "QUAZONAI_AUTH_SESSION_TTL_SECONDS": "",
        "QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS": "",
        "QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS": "",
        "QUAZONAI_NAUTILUS_RUNTIME_URL": "",
        "QUAZONAI_NAUTILUS_RUNTIME_TOKEN": "",
        "QUAZONAI_NAUTILUS_SEALED_RUNTIME_URL": "",
        "QUAZONAI_NAUTILUS_SEALED_RUNTIME_TOKEN": "",
        "QUAZONAI_NAUTILUS_VERSION": "",
        "QUAZONAI_NAUTILUS_CONTRACT_VERSION": "",
        "QUAZONAI_NAUTILUS_RUNTIME_TIMEOUT_SECONDS": "",
        "QUAZONAI_NAUTILUS_RUNTIME_POLL_SECONDS": "",
        "QUAZONAI_NAUTILUS_RUNTIME_VERIFY_TLS": "",
    }
    overrides = [
        'shell_environment_policy.inherit="core"',
        "shell_environment_policy.ignore_default_excludes=false",
    ]
    if mission_contract_file is not None:
        overrides.append(
            _mission_mcp_config_override(mission_contract_file, mission_capability_socket)
        )
    provider_id: str | None = None

    if settings.codex_base_url or settings.codex_api_key:
        provider_id = CUSTOM_CODEX_PROVIDER_ID
        base_url = settings.codex_base_url or DEFAULT_OPENAI_API_BASE_URL
        overrides.extend(
            [
                f"model_providers.{provider_id}.name=\"QuaZonai configured provider\"",
                f"model_providers.{provider_id}.base_url={json.dumps(base_url)}",
                f"model_providers.{provider_id}.wire_api=\"responses\"",
            ]
        )
        if settings.codex_api_key:
            if credential_socket is None:
                raise QfError(
                    "CODEX_PROVIDER_AUTH_UNAVAILABLE",
                    "Codex provider credential broker is unavailable.",
                    503,
                )
            auth_cwd = Path(__file__).resolve().parents[2]
            auth_args = ["-m", "runners.codex_provider_auth", str(credential_socket)]
            auth_config = (
                "{ command = "
                f"{json.dumps(sys.executable)}, args = {json.dumps(auth_args)}, "
                "timeout_ms = 5000, refresh_interval_ms = 0, cwd = "
                f"{json.dumps(str(auth_cwd))} }}"
            )
            overrides.append(f"model_providers.{provider_id}.auth={auth_config}")

    return (
        CodexConfig(
            cwd=str(workspace),
            env=environment,
            config_overrides=tuple(overrides),
        ),
        provider_id,
    )


def _codex_thread_config(settings: Settings) -> dict[str, Any]:
    """Build the per-Mission config without changing the global Codex profile."""
    config: dict[str, object] = {
        "sandbox_workspace_write": {"network_access": False},
        "web_search": "disabled",
    }
    if settings.codex_reasoning_effort is not None:
        config["model_reasoning_effort"] = settings.codex_reasoning_effort
    return config


def _codex_service_tier(settings: Settings) -> str | None:
    return "fast" if settings.codex_fast_mode else None


@contextmanager
def _mission_codex_thread(
    settings: Settings,
    workspace: Path,
    auth_factory: SessionFactory,
    *,
    developer_instructions: str,
    existing_thread_id: str | None,
    mission_contract_file: Path,
    mission_capability_socket: Path | None = None,
) -> Iterator[Any]:
    """Yield a Mission thread through exactly one explicit auth route."""
    from openai_codex import ApprovalMode, Codex, Sandbox

    if settings.codex_base_url or settings.codex_api_key:
        with _provider_credential_broker(settings.codex_api_key) as credential_socket:
            codex_config, model_provider = _codex_launch_configuration(
                settings,
                workspace,
                credential_socket=credential_socket,
                mission_contract_file=mission_contract_file,
                mission_capability_socket=mission_capability_socket,
            )
            with Codex(codex_config) as codex:
                if existing_thread_id:
                    yield codex.thread_resume(
                        existing_thread_id,
                        approval_mode=ApprovalMode.deny_all,
                        sandbox=Sandbox.workspace_write,
                        cwd=str(workspace),
                        model=settings.codex_model,
                        model_provider=model_provider,
                        service_tier=_codex_service_tier(settings),
                        config=_codex_thread_config(settings),
                        developer_instructions=developer_instructions,
                    )
                else:
                    yield codex.thread_start(
                        approval_mode=ApprovalMode.deny_all,
                        sandbox=Sandbox.workspace_write,
                        cwd=str(workspace),
                        model=settings.codex_model,
                        model_provider=model_provider,
                        service_tier=_codex_service_tier(settings),
                        config=_codex_thread_config(settings),
                        developer_instructions=developer_instructions,
                    )
        return

    from runners.codex_chatgpt_runtime import external_chatgpt_thread

    codex_config, _ = _codex_launch_configuration(
        settings,
        workspace,
        mission_contract_file=mission_contract_file,
        mission_capability_socket=mission_capability_socket,
    )
    with external_chatgpt_thread(
        codex_config,
        settings=settings,
        session_factory=auth_factory,
        workspace=workspace,
        model=settings.codex_model,
        service_tier=_codex_service_tier(settings),
        thread_config=_codex_thread_config(settings),
        developer_instructions=developer_instructions,
        existing_thread_id=existing_thread_id,
    ) as thread:
        yield thread


def _has_result(workspace: Path) -> bool:
    try:
        return (workspace / "RESULT.md").stat().st_size > 0
    except OSError:
        return False


def _run_observable_turn(
    thread: Any,
    factory: Any,
    agent_session_id: UUID,
    *,
    kind: str,
    prompt: str,
) -> str:
    """Run one visible App Server turn and persist only its observable result."""
    handle = thread.turn(prompt)
    with factory() as session, session.begin():
        agent_session = session.get(AgentSession, agent_session_id)
        if agent_session is None:
            raise QfError("AGENT_SESSION_NOT_FOUND", "Mission AgentSession is missing.", 500)
        begin_turn(session, agent_session, kind=kind, codex_turn_id=handle.id)
    result = handle.run()
    summary = str(getattr(result, "final_response", None) or "Mission turn completed.").strip()
    with factory() as session, session.begin():
        agent_session = session.get(AgentSession, agent_session_id)
        turn = session.scalar(
            select(AgentTurn).where(
                AgentTurn.agent_session_id == agent_session_id,
                AgentTurn.codex_turn_id == handle.id,
            )
        )
        if agent_session is None or turn is None:
            raise QfError("AGENT_TURN_NOT_FOUND", "Mission AgentTurn is missing.", 500)
        finish_turn(turn, agent_session, summary=summary)
    return summary


def run_mission(settings: Settings, lease: JobLease) -> None:
    """Resume or start one bounded Mission without recreating its workspace or Thread."""
    if importlib.util.find_spec("openai_codex") is None:
        raise QfError(
            "CODEX_RUNTIME_UNAVAILABLE",
            "The official OpenAI Codex app-server SDK is not installed.",
            503,
        )

    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    auth_factory = create_session_factory(engine)
    mission_id: UUID | None = None
    program_id: UUID | None = None
    execution_turns: tuple[tuple[str, str], ...] = ()
    try:
        (
            mission_id,
            program_id,
            context,
            existing_thread_id,
            recorded_workspace,
            runtime_snapshot,
            mission_contract_file,
        ) = _load_mission_context(settings, factory, lease)
        mission_settings = _frozen_mission_settings(settings, runtime_snapshot)
        codex_sandbox_preflight()
        workspace = _prepare_worktree(
            mission_settings,
            program_id,
            mission_id,
            recorded_workspace=recorded_workspace,
            resuming=existing_thread_id is not None,
        )
        (workspace / "MISSION.md").write_text(context, encoding="utf-8")
        developer_instructions = (
            "You are a QuaZonai Research Mission worker. Work only inside this Mission worktree. "
            "Read MISSION.md and perform the bounded research task. Always write durable findings "
            "to RESULT.md. Do not invent performance, orders, fills, positions, PnL, or statistics. "
            "Do not request approvals, access external networks, place trades, manage broker state, "
            "or alter the frozen Charter. Each Mission MCP evidence call requires the frozen mission_id; "
            "artifact submission also requires an idempotency_key and current expected_revision. "
            "Submit only a concise, falsifiable research conclusion."
        )
        with ExitStack() as stack:
            capability_broker = stack.enter_context(
                MissionCapabilityBroker(load_mission_contract(mission_contract_file), factory)
            )
            thread = stack.enter_context(
                _mission_codex_thread(
                    mission_settings,
                    workspace,
                    auth_factory,
                    developer_instructions=developer_instructions,
                    existing_thread_id=existing_thread_id,
                    mission_contract_file=mission_contract_file,
                    mission_capability_socket=capability_broker.socket_path,
                )
            )
            with factory() as session, session.begin():
                mission = session.execute(
                    select(ResearchMission)
                    .where(ResearchMission.id == mission_id)
                    .with_for_update()
                ).scalar_one()
                program = session.get(ResearchProgram, mission.program_id)
                if program is None or program.state != "ACTIVE":
                    raise QfError("PROGRAM_NOT_ACTIVE", "Program is not active for Mission admission.", 409)
                if mission.state not in {"READY", "INTERRUPTED"}:
                    raise QfError(
                        "MISSION_STATE_CONFLICT",
                        "Mission state changed before Codex admission.",
                        409,
                        {"state": mission.state},
                    )
                successful_turns = int(
                    session.scalar(
                        select(func.count())
                        .select_from(AgentTurn)
                        .join(AgentSession)
                        .where(
                            AgentSession.mission_id == mission.id,
                            AgentTurn.state == "SUCCEEDED",
                        )
                    )
                    or 0
                )
                completed_kinds = set(
                    session.scalars(
                        select(AgentTurn.kind)
                        .join(AgentSession)
                        .where(
                            AgentSession.mission_id == mission.id,
                            AgentTurn.state == "SUCCEEDED",
                        )
                    )
                )
                successful_repairs = int(
                    session.scalar(
                        select(func.count())
                        .select_from(AgentTurn)
                        .join(AgentSession)
                        .where(
                            AgentSession.mission_id == mission.id,
                            AgentTurn.state == "SUCCEEDED",
                            AgentTurn.kind == "REPAIR",
                        )
                    )
                    or 0
                )
                remaining_turns = mission.max_turns - successful_turns
                execution_turns = _execution_turns(MissionType(mission.mission_type))
                required_turns = sum(
                    kind not in completed_kinds
                    for kind in _required_turn_kinds(MissionType(mission.mission_type))
                )
                if remaining_turns < required_turns:
                    raise QfError("MISSION_TURN_BUDGET_EXCEEDED", "Mission turn budget is exhausted.", 409)
                agent_session = record_session_admission(
                    session,
                    mission,
                    thread_id=thread.id,
                    codex_version="openai_codex",
                    model=mission_settings.codex_model,
                    reasoning_effort=mission_settings.codex_reasoning_effort,
                    service_tier=_codex_service_tier(mission_settings),
                )
                _freeze_effective_codex_runtime(mission, mission_settings)
                mission.workspace_path = str(workspace)
                mission.summary = "Codex app-server admitted the Mission and started the research turn."
                session.flush()
                agent_session_id = agent_session.id
                _event(
                    session,
                    kind="MISSION_STARTED",
                    program_id=program_id,
                    mission_id=mission_id,
                    payload={
                        "codex_thread_id": thread.id,
                        "requested_codex_model": mission_settings.codex_model,
                        "requested_codex_reasoning_effort": mission_settings.codex_reasoning_effort,
                        "requested_codex_fast_mode": mission_settings.codex_fast_mode,
                        "requested_codex_service_tier": _codex_service_tier(mission_settings),
                    },
                )

            final_response = "Mission completed with a durable RESULT.md."
            for kind, prompt in execution_turns:
                if kind in completed_kinds:
                    continue
                final_response = _run_observable_turn(
                    thread,
                    factory,
                    agent_session_id,
                    kind=kind,
                    prompt=prompt,
                )
                completed_kinds.add(kind)
                remaining_turns -= 1

            while (
                not _has_result(workspace)
                and successful_repairs < FIXED_AUTONOMY_BUDGET.max_repair_turns
                and remaining_turns > (0 if "REVIEW" in completed_kinds else 1)
            ):
                final_response = _run_observable_turn(
                    thread,
                    factory,
                    agent_session_id,
                    kind="REPAIR",
                    prompt=(
                        "RESULT.md is missing or empty. Repair only that validation failure: inspect your "
                        "actual work, write a concise durable RESULT.md, and do not invent evidence."
                    ),
                )
                successful_repairs += 1
                remaining_turns -= 1

            if "REVIEW" not in completed_kinds:
                final_response = _run_observable_turn(
                    thread,
                    factory,
                    agent_session_id,
                    kind="REVIEW",
                    prompt=(
                        "Review MISSION.md, RESULT.md, and actual available evidence. Correct any unsupported "
                        "claim in RESULT.md, retain only a falsifiable conclusion, and return a concise summary."
                    ),
                )
                completed_kinds.add("REVIEW")

            if not _has_result(workspace):
                raise QfError(
                    "MISSION_RESULT_MISSING",
                    "Mission did not produce a non-empty RESULT.md after bounded repair.",
                    422,
                )
        _git("add", "-A", cwd=workspace)
        status = _git("status", "--porcelain", cwd=workspace).stdout.strip()
        if status:
            _git("commit", "-m", f"Complete research mission {mission_id}", cwd=workspace)

        completion_error: QfError | None = None
        with factory() as session, session.begin():
            mission = session.scalar(
                select(ResearchMission)
                .where(ResearchMission.id == mission_id)
                .with_for_update()
            )
            if mission is None:
                raise QfError("MISSION_NOT_FOUND", "Mission was not found.", 404)
            if mission.mission_type == MissionType.ALPHA_DISCOVERY.value:
                intake = stage_alpha_discovery_evaluation(
                    session,
                    mission_id=mission.id,
                    workspace=workspace,
                    artifact_root=mission_settings.mission_root / "artifacts",
                )
                if intake.accepted:
                    await_mission_validation(session, mission.id, summary=final_response)
                else:
                    error_code = intake.error_code or "ALPHA_PROPOSAL_INVALID"
                    finish_mission(
                        session,
                        mission.id,
                        succeeded=False,
                        summary=final_response,
                        error_code=error_code,
                    )
                    completion_error = QfError(
                        error_code,
                        "Alpha proposal was not accepted by Core validation.",
                        422,
                    )
            else:
                finish_mission(session, mission.id, succeeded=True, summary=final_response)
        if completion_error is not None:
            raise completion_error
    except Exception as exc:
        if mission_id is not None:
            with factory() as session, session.begin():
                failed_mission = session.get(ResearchMission, mission_id)
                if failed_mission is not None and failed_mission.state == "RUNNING":
                    failed_agent_session = session.scalar(
                        select(AgentSession).where(AgentSession.mission_id == failed_mission.id)
                    )
                    if failed_agent_session is not None:
                        turn = session.scalar(
                            select(AgentTurn).where(
                                AgentTurn.agent_session_id == failed_agent_session.id,
                                AgentTurn.state == "RUNNING",
                            )
                        )
                        if turn is not None:
                            finish_turn(
                                turn,
                                failed_agent_session,
                                summary=str(exc),
                                error_code=str(getattr(exc, "code", type(exc).__name__))[:100],
                            )
                    finish_mission(
                        session,
                        mission_id,
                        succeeded=False,
                        summary=str(exc),
                        error_code=str(getattr(exc, "code", type(exc).__name__))[:100],
                    )
        raise
    finally:
        engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one QuaZonai Research Mission")
    parser.add_argument("action", choices=["run"])
    parser.add_argument("job_id")
    parser.add_argument("--lease-owner", required=True)
    parser.add_argument("--lease-attempt", required=True, type=int)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    settings = load_effective_settings(Settings.from_env())
    run_mission(
        settings,
        JobLease(
            job_id=UUID(args.job_id),
            owner=args.lease_owner,
            attempt=args.lease_attempt,
        ),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
