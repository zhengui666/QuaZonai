"""Execute one Research Mission through the official Codex app-server runtime."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
from collections.abc import Iterator

import httpx
from contextlib import contextmanager
from datetime import UTC, datetime
from pathlib import Path
from typing import Any
from uuid import UUID

from sqlalchemy import select

from db.models import Event, Job, ResearchBranch, ResearchCharter, ResearchMission, ResearchProgram
from db.session import create_database_engine, create_session_factory
from errors import QfError
from quant_runtime.workspace import execute_workspace_experiments, prepare_experiment_workspace
from runtime_config import load_effective_settings
from settings import Settings

CUSTOM_CODEX_PROVIDER_ID = "quazonai_configured"
DEFAULT_OPENAI_API_BASE_URL = "https://api.openai.com/v1"
BROKER_REQUEST = b"TOKEN\n"
BROKER_ACCEPT_POLL_SECONDS = 0.25
RETRYABLE_MISSION_EXIT_CODE = 75
REMOTE_RESULT_UNCERTAIN = "NAUTILUS_REMOTE_RESULT_UNCERTAIN"


class RetryableMissionError(RuntimeError):
    """The remote result is ambiguous and the same durable Mission must be retried."""


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


def _git(
    *args: str, cwd: Path | None = None, check: bool = True
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=cwd,
        check=check,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=30,
    )


def _prepare_worktree(settings: Settings, program_id: UUID, mission_id: UUID) -> Path:
    program_root = settings.mission_root / "programs" / str(program_id)
    repo = program_root / "repo"
    worktrees = settings.mission_root / "worktrees"
    workspace = worktrees / str(mission_id)
    branch = f"mission-{mission_id.hex}"
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
        # A transport-uncertain runtime result must resume the exact experiment
        # files which generated its immutable experiment id. Do not rebuild the
        # worktree and silently replace that contract on a durable retry.
        return workspace
    _git("branch", "-D", branch, cwd=repo, check=False)
    _git("worktree", "add", "-b", branch, str(workspace), "HEAD", cwd=repo)
    return workspace


def _mission_context(
    mission: ResearchMission,
    program: ResearchProgram,
    charter: ResearchCharter,
    branch: ResearchBranch,
) -> str:
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
    )


def _load_mission_context(settings: Settings, job_id: UUID) -> tuple[UUID, UUID, str]:
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    with factory() as session:
        job = session.get(Job, job_id)
        if job is None or job.kind != "RESEARCH_MISSION":
            raise QfError("JOB_NOT_FOUND", "Research Mission job does not exist.", 404)
        mission = session.get(ResearchMission, job.resource_id)
        if mission is None:
            raise QfError("MISSION_NOT_FOUND", "Research Mission does not exist.", 404)
        if mission.state != "READY":
            raise QfError(
                "MISSION_STATE_CONFLICT",
                "Only READY Research Missions may be admitted by the Agent Worker.",
                409,
                {"state": mission.state},
            )
        program = session.get(ResearchProgram, mission.program_id)
        branch = session.get(ResearchBranch, mission.branch_id)
        if program is None or branch is None:
            raise QfError("MISSION_CONTEXT_MISSING", "Mission Program or Branch is missing.", 500)
        charter = session.get(ResearchCharter, program.charter_id)
        if charter is None:
            raise QfError("MISSION_CONTEXT_MISSING", "Mission Charter is missing.", 500)
        return mission.id, program.id, _mission_context(mission, program, charter, branch)


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
        except OSError, TimeoutError:
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


def _codex_launch_configuration(
    settings: Settings,
    workspace: Path,
    *,
    credential_socket: Path | None = None,
) -> tuple[Any, str | None]:
    """Build App Server launch config without placing provider secrets in its environment."""
    from openai_codex import CodexConfig

    environment = {
        "CODEX_HOME": str(settings.codex_home),
        "OPENAI_API_KEY": "",
        "CODEX_API_KEY": "",
        "QUAZONAI_CODEX_API_KEY": "",
        # The App Server does not need Core bootstrap or Operator credentials.
        # Explicitly clear them from the environment inherited by Mission-owned
        # child processes, including non-Compose local launches.
        "QUAZONAI_MASTER_KEY": "",
        "QUAZONAI_DATABASE_URL": "",
        "QUAZONAI_ALEMBIC_URL": "",
        "POSTGRES_PASSWORD": "",
        "QUAZONAI_AUTH_ENABLED": "",
        "QUAZONAI_AUTH_USERNAME": "",
        "QUAZONAI_AUTH_PASSWORD": "",
        "QUAZONAI_AUTH_TOTP_SECRET": "",
        "QUAZONAI_AUTH_COOKIE_KEY": "",
        "QUAZONAI_API_TOKEN": "",
        "QUAZONAI_AUTH_PUBLIC_ORIGIN": "",
        "QUAZONAI_AUTH_SESSION_TTL_SECONDS": "",
        "QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS": "",
        "QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS": "",
        "QUAZONAI_NAUTILUS_RESEARCH_URL": "",
        "QUAZONAI_NAUTILUS_RESEARCH_TOKEN": "",
        "QUAZONAI_NAUTILUS_RESEARCH_EXPECTED_VERSION": "",
        "QUAZONAI_NAUTILUS_RESEARCH_TIMEOUT_SECONDS": "",
        "QUAZONAI_NAUTILUS_RESEARCH_ALLOW_INSECURE_HTTP": "",
        "QUAZONAI_NAUTILUS_SEALED_URL": "",
        "QUAZONAI_NAUTILUS_SEALED_TOKEN": "",
        "QUAZONAI_NAUTILUS_SEALED_EXPECTED_VERSION": "",
        "QUAZONAI_NAUTILUS_SEALED_TIMEOUT_SECONDS": "",
        "QUAZONAI_NAUTILUS_SEALED_ALLOW_INSECURE_HTTP": "",
    }
    overrides = [
        'shell_environment_policy.inherit="core"',
        "shell_environment_policy.ignore_default_excludes=false",
    ]
    provider_id: str | None = None

    if settings.codex_base_url or settings.codex_api_key:
        provider_id = CUSTOM_CODEX_PROVIDER_ID
        base_url = settings.codex_base_url or DEFAULT_OPENAI_API_BASE_URL
        overrides.extend(
            [
                f'model_providers.{provider_id}.name="QuaZonai configured provider"',
                f"model_providers.{provider_id}.base_url={json.dumps(base_url)}",
                f'model_providers.{provider_id}.wire_api="responses"',
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


def _open_codex_thread(
    codex: Any,
    *,
    persisted_thread_id: str | None,
    options: dict[str, Any],
) -> Any:
    """Start a Mission thread once; durable retries resume that exact thread."""
    if persisted_thread_id:
        return codex.thread_resume(persisted_thread_id, **options)
    return codex.thread_start(**options)


def run_mission(settings: Settings, job_id: UUID) -> None:
    """Start app-server first, then atomically admit the Mission into RUNNING."""
    try:
        from openai_codex import ApprovalMode, Codex, Sandbox
    except ImportError as exc:
        raise QfError(
            "CODEX_RUNTIME_UNAVAILABLE",
            "The official OpenAI Codex app-server SDK is not installed.",
            503,
        ) from exc

    mission_id, program_id, context = _load_mission_context(settings, job_id)
    workspace = _prepare_worktree(settings, program_id, mission_id)
    (workspace / "MISSION.md").write_text(context, encoding="utf-8")
    prepare_experiment_workspace(settings, workspace=workspace, mission_id=mission_id)

    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    try:
        retry_thread_id: str | None = None
        # If a previous attempt lost the HTTP response, the worktree still holds
        # the exact contract and Codex thread. Reconcile the immutable experiment
        # before resuming that same conversation.
        if (workspace / "experiments").exists():
            with factory() as session:
                retry_mission = session.get(ResearchMission, mission_id)
                retry_branch_id = retry_mission.branch_id if retry_mission is not None else None
                if (
                    retry_mission is not None
                    and retry_mission.error_code == REMOTE_RESULT_UNCERTAIN
                ):
                    retry_thread_id = retry_mission.codex_thread_id
            if retry_branch_id is None:
                raise QfError(
                    "MISSION_BRANCH_MISSING",
                    "Research Mission has no Branch for experiment reconciliation.",
                    500,
                )
            if (
                retry_mission is not None
                and retry_mission.error_code == REMOTE_RESULT_UNCERTAIN
                and not retry_thread_id
            ):
                raise QfError(
                    "MISSION_RETRY_CONTEXT_MISSING",
                    "Transport-uncertain Mission lost its persisted Codex thread id.",
                    500,
                )
            execute_workspace_experiments(
                settings,
                workspace=workspace,
                mission_id=mission_id,
                program_id=program_id,
                branch_id=retry_branch_id,
                already_executed=set(),
            )
        with _provider_credential_broker(settings.codex_api_key) as credential_socket:
            codex_config, model_provider = _codex_launch_configuration(
                settings,
                workspace,
                credential_socket=credential_socket,
            )
            with Codex(codex_config) as codex:
                developer_instructions = (
                    "You are a QuaZonai Research Mission worker. Work only inside this Mission worktree. "
                    "Read MISSION.md, DATASETS.json, EXPERIMENT_CONTRACT.schema.json, and "
                    "NAUTILUS_EXPERIMENTS.md before making quantitative claims. If DEGRADATION_CONTEXT.json "
                    "exists, read it as immutable prior Strategy/Discovery/Forward Evidence context. Use only governed Discovery "
                    "datasets listed there and declare bounded SOURCE_BUNDLE Nautilus experiments under "
                    "experiments/ when evidence is available. The parent worker, not you, executes those "
                    "contracts against the independent remote runtime. Do not request approvals, do not access "
                    "external networks, do not place trades, do not manage broker state, and do not alter the "
                    "frozen Research Charter."
                )
                thread = _open_codex_thread(
                    codex,
                    persisted_thread_id=retry_thread_id,
                    options={
                        "approval_mode": ApprovalMode.deny_all,
                        "sandbox": Sandbox.workspace_write,
                        "cwd": str(workspace),
                        "model": settings.codex_model,
                        "model_provider": model_provider,
                        "config": {
                            "sandbox_workspace_write": {"network_access": False},
                            "web_search": "disabled",
                        },
                        "developer_instructions": developer_instructions,
                    },
                )
                with factory() as session, session.begin():
                    mission = session.execute(
                        select(ResearchMission)
                        .where(ResearchMission.id == mission_id)
                        .with_for_update()
                    ).scalar_one()
                    if mission.state != "READY":
                        raise QfError(
                            "MISSION_STATE_CONFLICT",
                            "Mission state changed before Codex admission.",
                            409,
                            {"state": mission.state},
                        )
                    mission.state = "RUNNING"
                    if mission.started_at is None:
                        mission.started_at = _now()
                    mission.codex_thread_id = thread.id
                    mission.workspace_path = str(workspace)
                    mission.summary = (
                        "Codex app-server resumed the transport-uncertain Mission thread."
                        if retry_thread_id
                        else "Codex app-server admitted the Mission and started the research turn."
                    )
                    _event(
                        session,
                        kind="MISSION_RESUMED" if retry_thread_id else "MISSION_STARTED",
                        program_id=program_id,
                        mission_id=mission_id,
                        payload={"codex_thread_id": thread.id},
                    )

                if retry_thread_id:
                    result = thread.run(
                        "The preserved remote Nautilus experiment has now been reconciled. Read "
                        "evidence/INDEX.json and every evidence/*.json file, compare the real orders, "
                        "fills, positions, PnL and statistics, and update RESULT.md. Do not rerun the "
                        "research turn and do not create additional experiment contracts."
                    )
                else:
                    result = thread.run(
                        "Execute the Mission in MISSION.md. First read NAUTILUS_EXPERIMENTS.md, DATASETS.json, and "
                        "EXPERIMENT_CONTRACT.schema.json. If governed data are available, back quantitative claims "
                        "with bounded SOURCE_BUNDLE contracts in experiments/. If no usable governed dataset exists, "
                        "record that evidence blocker instead of fabricating results. Produce RESULT.md with evidence, "
                        "assumptions, limitations, and concrete next research actions. Return a concise completion summary."
                    )
                    if mission.branch_id is None:
                        raise QfError(
                            "MISSION_BRANCH_MISSING",
                            "Research Mission has no Branch for experiment lineage.",
                            500,
                        )
                    executed_experiment_ids: set[UUID] = set()
                    experiment_activity = execute_workspace_experiments(
                        settings,
                        workspace=workspace,
                        mission_id=mission.id,
                        program_id=mission.program_id,
                        branch_id=mission.branch_id,
                        already_executed=executed_experiment_ids,
                    )
                    executed_experiment_ids.update(experiment_activity)
                    if experiment_activity.has_activity:
                        result = thread.run(
                            "Read evidence/INDEX.json and every new evidence/*.json file. Compare the real "
                            "Nautilus orders, fills, positions, PnL and statistics, then update RESULT.md. "
                            "Do not create additional experiment contracts in this final evidence turn."
                        )

        _git("add", "-A", cwd=workspace)
        status = _git("status", "--porcelain", cwd=workspace).stdout.strip()
        if status:
            _git("commit", "-m", f"Complete research mission {mission_id}", cwd=workspace)

        final_response = (
            result.final_response or "Mission completed without a textual summary."
        ).strip()
        with factory() as session, session.begin():
            mission = session.execute(
                select(ResearchMission).where(ResearchMission.id == mission_id).with_for_update()
            ).scalar_one()
            mission.state = "SUCCEEDED"
            mission.finished_at = _now()
            mission.error_code = None
            mission.summary = final_response[-12000:]
            _event(
                session,
                kind="MISSION_SUCCEEDED",
                program_id=program_id,
                mission_id=mission_id,
                payload={"summary": mission.summary},
            )
    except httpx.TransportError as exc:
        with factory() as session, session.begin():
            mission = session.get(ResearchMission, mission_id)
            if mission is not None and mission.state in {"READY", "RUNNING"}:
                mission.state = "READY"
                mission.finished_at = None
                mission.error_code = REMOTE_RESULT_UNCERTAIN
                mission.summary = (
                    "Remote Nautilus result is transport-uncertain; the exact Mission worktree and "
                    "experiment id are preserved for durable reconciliation."
                )
                mission.attempt += 1
                _event(
                    session,
                    kind="MISSION_RETRY_SCHEDULED",
                    program_id=program_id,
                    mission_id=mission_id,
                    payload={"error_code": REMOTE_RESULT_UNCERTAIN},
                )
        raise RetryableMissionError(
            "remote Nautilus result uncertain; retry the same Mission contract"
        ) from exc
    except Exception as exc:
        with factory() as session, session.begin():
            mission = session.get(ResearchMission, mission_id)
            if mission is not None and mission.state in {"READY", "RUNNING"}:
                mission.state = "FAILED"
                mission.finished_at = _now()
                mission.error_code = str(getattr(exc, "code", type(exc).__name__))[:100]
                mission.summary = str(exc)[-12000:]
                _event(
                    session,
                    kind="MISSION_FAILED",
                    program_id=program_id,
                    mission_id=mission_id,
                    payload={"error_code": mission.error_code},
                )
        raise


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one QuaZonai Research Mission")
    parser.add_argument("action", choices=["run"])
    parser.add_argument("job_id")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    settings = load_effective_settings(Settings.from_env())
    try:
        run_mission(settings, UUID(args.job_id))
    except RetryableMissionError:
        return RETRYABLE_MISSION_EXIT_CODE
    return 0


if __name__ == "__main__":
    raise SystemExit(main())