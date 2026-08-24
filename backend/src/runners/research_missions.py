"""Execute one Research Mission through the official Codex app-server runtime."""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
from datetime import UTC, datetime
from pathlib import Path
from uuid import UUID

from sqlalchemy import select

from db.models import Event, Job, ResearchBranch, ResearchCharter, ResearchMission, ResearchProgram
from db.session import create_database_engine, create_session_factory
from errors import QfError
from runtime_config import load_effective_settings
from settings import Settings

CUSTOM_CODEX_PROVIDER_ID = "quazonai-configured"
DEFAULT_OPENAI_API_BASE_URL = "https://api.openai.com/v1"


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
        _git("worktree", "remove", "--force", str(workspace), cwd=repo, check=False)
        shutil.rmtree(workspace, ignore_errors=True)
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


def _codex_launch_configuration(settings: Settings, workspace: Path) -> tuple[object, str | None]:
    """Build app-server launch config without exposing provider secrets to Mission shells."""
    from openai_codex import CodexConfig

    environment = {
        "CODEX_HOME": str(settings.codex_home),
        # Runtime Codex authentication is deliberately not inherited from QuaZonai's
        # bootstrap environment. Existing ChatGPT login remains available via CODEX_HOME.
        "OPENAI_API_KEY": "",
        "CODEX_API_KEY": "",
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
                f"model_providers.{provider_id}.name=\"QuaZonai configured provider\"",
                f"model_providers.{provider_id}.base_url={json.dumps(base_url)}",
                f"model_providers.{provider_id}.wire_api=\"responses\"",
            ]
        )
        if settings.codex_api_key:
            environment["QUAZONAI_CODEX_API_KEY"] = settings.codex_api_key
            overrides.append(
                f"model_providers.{provider_id}.env_key=\"QUAZONAI_CODEX_API_KEY\""
            )

    return (
        CodexConfig(
            cwd=str(workspace),
            env=environment,
            config_overrides=tuple(overrides),
        ),
        provider_id,
    )


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

    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    try:
        codex_config, model_provider = _codex_launch_configuration(settings, workspace)
        with Codex(codex_config) as codex:
            thread = codex.thread_start(
                approval_mode=ApprovalMode.deny_all,
                sandbox=Sandbox.workspace_write,
                cwd=str(workspace),
                model=settings.codex_model,
                model_provider=model_provider,
                config={
                    "sandbox_workspace_write": {"network_access": False},
                    "web_search": "disabled",
                },
                developer_instructions=(
                    "You are a QuaZonai Research Mission worker. Work only inside this Mission worktree. "
                    "Read MISSION.md, perform the bounded research task, and write durable findings to RESULT.md. "
                    "Do not request approvals, do not access external networks, do not place trades, do not manage "
                    "broker state, and do not alter the frozen Research Charter."
                ),
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
                mission.started_at = _now()
                mission.codex_thread_id = thread.id
                mission.workspace_path = str(workspace)
                mission.summary = "Codex app-server admitted the Mission and started the research turn."
                _event(
                    session,
                    kind="MISSION_STARTED",
                    program_id=program_id,
                    mission_id=mission_id,
                    payload={"codex_thread_id": thread.id},
                )

            result = thread.run(
                "Execute the Mission in MISSION.md. Produce RESULT.md with the evidence, assumptions, limitations, "
                "and concrete next research actions. Return a concise completion summary."
            )

        _git("add", "-A", cwd=workspace)
        status = _git("status", "--porcelain", cwd=workspace).stdout.strip()
        if status:
            _git("commit", "-m", f"Complete research mission {mission_id}", cwd=workspace)

        final_response = (result.final_response or "Mission completed without a textual summary.").strip()
        with factory() as session, session.begin():
            mission = session.execute(
                select(ResearchMission)
                .where(ResearchMission.id == mission_id)
                .with_for_update()
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
    run_mission(settings, UUID(args.job_id))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
