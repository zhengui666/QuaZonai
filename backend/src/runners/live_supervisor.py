"""Supervise one fail-closed live runner process per desired Deployment."""

from __future__ import annotations

import argparse
import json
import logging
import signal
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from crypto import EncryptedSecret, decrypt_secret
from db.models import (
    CredentialSecret,
    DataSource,
    Deployment,
    DeploymentUniverseRevision,
    ExecutionConnection,
    PluginRelease,
    PluginRuntimeBundle,
    ResearchCase,
    RiskAccount,
    StrategyVersion,
)
from db.session import (
    SessionFactory,
    create_database_engine,
    create_session_factory,
    ping_database,
)
from errors import QfError
from events import append_event
from logging_utils import configure_logging
from settings import Settings

LOGGER = logging.getLogger("quazonai.live_supervisor")


@dataclass(slots=True)
class RunnerProcess:
    deployment_id: UUID
    process: subprocess.Popen[str]


class StopFlag:
    requested = False

    def request(self, *_: object) -> None:
        self.requested = True


def _credential_values(
    session: Session,
    *,
    credential_set_id: UUID | None,
    plugin_release_id: UUID,
    master_key: bytes,
) -> dict[str, str]:
    if credential_set_id is None:
        return {}
    rows = list(
        session.scalars(
            select(CredentialSecret)
            .where(CredentialSecret.credential_set_id == credential_set_id)
            .order_by(CredentialSecret.field_name.asc())
        )
    )
    return {
        row.field_name: decrypt_secret(
            EncryptedSecret(
                ciphertext=row.ciphertext,
                nonce=row.nonce,
                key_version=row.key_version,
            ),
            master_key=master_key,
            credential_set_id=credential_set_id,
            plugin_release_id=plugin_release_id,
            field_name=row.field_name,
        )
        for row in rows
    }


def _require_release(session: Session, release_id: UUID) -> PluginRelease:
    release = session.get(PluginRelease, release_id)
    if release is None or release.state not in {"ACTIVE", "DRAINING"}:
        raise QfError(
            "PLUGIN_RUNTIME_UNAVAILABLE",
            "Deployment plugin release is not available to its pinned resource.",
            503,
            {"plugin_release_id": str(release_id)},
        )
    return release


def build_runner_payload(
    session: Session,
    *,
    deployment_id: UUID,
    settings: Settings,
) -> dict[str, Any]:
    deployment = session.get(Deployment, deployment_id)
    if deployment is None:
        raise QfError("DEPLOYMENT_UNKNOWN", "Deployment does not exist.", 404)
    if deployment.desired_state != "RUNNING":
        raise QfError("DEPLOYMENT_NOT_RUNNING", "Deployment is not requested to run.", 409)

    research = session.get(ResearchCase, deployment.research_id)
    strategy = session.get(StrategyVersion, deployment.strategy_version_id)
    data_source = session.get(DataSource, deployment.data_source_id)
    execution = session.get(ExecutionConnection, deployment.execution_connection_id)
    bundle = session.get(PluginRuntimeBundle, deployment.runtime_bundle_id)
    revision = (
        session.get(DeploymentUniverseRevision, deployment.active_revision_id)
        if deployment.active_revision_id
        else None
    )
    if research is None or research.state != "CLOSED":
        raise QfError(
            "APPROVAL_REQUIRED",
            "Deployment Research must be CLOSED by an approved start decision.",
            409,
        )
    if strategy is None or data_source is None or execution is None:
        raise QfError("LIVE_START_FAILED", "Deployment references are incomplete.", 503)
    if data_source.state != "ACTIVE" or execution.state != "ACTIVE":
        raise QfError("LIVE_START_FAILED", "Deployment integrations are not ACTIVE.", 503)
    if bundle is None or bundle.state != "READY":
        raise QfError("PLUGIN_RUNTIME_UNAVAILABLE", "Deployment bundle is not READY.", 503)
    if revision is None or revision.state not in {"APPROVED", "ACTIVE"}:
        raise QfError(
            "APPROVAL_REQUIRED",
            "Deployment has no approved Universe revision.",
            409,
        )

    bundle_python = Path(bundle.environment_path) / "bin" / "python"
    if not bundle_python.is_file():
        raise QfError("PLUGIN_RUNTIME_UNAVAILABLE", "Bundle Python is missing.", 503)

    data_release = _require_release(session, data_source.plugin_release_id)
    execution_release = _require_release(session, execution.plugin_release_id)
    if data_release.descriptor_snapshot.get(
        "compatibility_key"
    ) != execution_release.descriptor_snapshot.get("compatibility_key"):
        raise QfError(
            "DATA_EXEC_INCOMPATIBLE",
            "Pinned data and execution plugins are incompatible.",
            422,
        )

    master_key = settings.master_key_bytes()
    return {
        "deployment_id": str(deployment.id),
        "bundle_python": str(bundle_python),
        "data_plugin_id": data_release.plugin_id,
        "execution_plugin_id": execution_release.plugin_id,
        "data_config": data_source.config,
        "data_secrets": _credential_values(
            session,
            credential_set_id=data_source.credential_set_id,
            plugin_release_id=data_source.plugin_release_id,
            master_key=master_key,
        ),
        "execution_config": execution.config,
        "execution_secrets": _credential_values(
            session,
            credential_set_id=execution.credential_set_id,
            plugin_release_id=execution.plugin_release_id,
            master_key=master_key,
        ),
        "strategy_source": strategy.source_text,
        "strategy_config": strategy.default_config,
        "universe_predicate": revision.predicate,
        "universe_cap": revision.cap,
        "funder_id": deployment.funder_id,
    }


def _mark_blocked(
    factory: SessionFactory,
    deployment_id: UUID,
    error: Exception,
) -> None:
    message = str(error)[-4000:]
    with factory.begin() as session:
        deployment = session.get(Deployment, deployment_id)
        if deployment is None:
            return
        deployment.observed_state = "RECOVERY_BLOCKED"
        deployment.last_error = message
        account = session.get(RiskAccount, deployment.funder_id)
        if account is not None:
            account.status = "BLOCKED"
            account.last_heartbeat_at = None
        append_event(
            session,
            kind="DEPLOYMENT_RECOVERY_BLOCKED",
            aggregate_type="deployment",
            aggregate_id=deployment.id,
            payload={"message": message[-1000:]},
        )


def _spawn(payload: dict[str, Any]) -> subprocess.Popen[str]:
    process = subprocess.Popen(
        [sys.executable, "-m", "quazonai.runners.live", "--payload-stdin"],
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        text=True,
        start_new_session=True,
    )
    assert process.stdin is not None
    process.stdin.write(json.dumps(payload, separators=(",", ":")) + "\n")
    process.stdin.close()
    return process


def _desired_running(factory: SessionFactory) -> set[UUID]:
    with factory() as session:
        return set(
            session.scalars(
                select(Deployment.id).where(Deployment.desired_state == "RUNNING")
            )
        )


def supervise_once(
    *,
    factory: SessionFactory,
    settings: Settings,
    processes: dict[UUID, RunnerProcess],
    next_retry: dict[UUID, float],
    spawn: bool = True,
) -> int:
    now = time.monotonic()
    desired = _desired_running(factory)

    for deployment_id, record in list(processes.items()):
        return_code = record.process.poll()
        if return_code is None:
            continue
        processes.pop(deployment_id, None)
        if deployment_id in desired:
            next_retry[deployment_id] = now + settings.live_recovery_retry_seconds
            LOGGER.warning(
                "live runner exited; recovery retry scheduled",
                extra={"deployment_id": str(deployment_id), "error_code": "LIVE_RUNNER_EXITED"},
            )

    started = 0
    for deployment_id in sorted(desired, key=str):
        if deployment_id in processes or now < next_retry.get(deployment_id, 0.0):
            continue
        try:
            with factory() as session:
                payload = build_runner_payload(
                    session,
                    deployment_id=deployment_id,
                    settings=settings,
                )
            if not spawn:
                started += 1
                continue
            process = _spawn(payload)
            processes[deployment_id] = RunnerProcess(
                deployment_id=deployment_id,
                process=process,
            )
            next_retry.pop(deployment_id, None)
            started += 1
            LOGGER.info("live runner started", extra={"deployment_id": str(deployment_id)})
        except Exception as exc:  # noqa: BLE001 - persist fail-closed state
            _mark_blocked(factory, deployment_id, exc)
            next_retry[deployment_id] = now + settings.live_recovery_retry_seconds
            LOGGER.exception(
                "live runner could not start",
                extra={"deployment_id": str(deployment_id), "error_code": type(exc).__name__},
            )
    return started


def _terminate_all(processes: dict[UUID, RunnerProcess], timeout: int) -> None:
    for record in processes.values():
        if record.process.poll() is None:
            record.process.terminate()
    deadline = time.monotonic() + timeout
    for record in processes.values():
        remaining = max(0.0, deadline - time.monotonic())
        if record.process.poll() is not None:
            continue
        try:
            record.process.wait(timeout=remaining)
        except subprocess.TimeoutExpired:
            record.process.kill()
            record.process.wait()
    processes.clear()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="QuaZonai live supervisor")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--once", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    configure_logging()
    settings = Settings.from_env()
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    if args.check:
        ping_database(engine)
        return 0

    processes: dict[UUID, RunnerProcess] = {}
    next_retry: dict[UUID, float] = {}
    if args.once:
        supervise_once(
            factory=factory,
            settings=settings,
            processes=processes,
            next_retry=next_retry,
            spawn=False,
        )
        return 0

    stop = StopFlag()
    signal.signal(signal.SIGTERM, stop.request)
    signal.signal(signal.SIGINT, stop.request)
    LOGGER.info("live supervisor started")
    try:
        while not stop.requested:
            supervise_once(
                factory=factory,
                settings=settings,
                processes=processes,
                next_retry=next_retry,
            )
            time.sleep(settings.supervisor_poll_seconds)
    finally:
        _terminate_all(processes, settings.live_runner_stop_timeout_seconds)
        engine.dispose()
    LOGGER.info("live supervisor stopped")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
