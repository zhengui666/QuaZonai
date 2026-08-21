"""Own one Deployment generation and its immutable-bundle live child."""

from __future__ import annotations

import argparse
import json
import os
import selectors
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, TextIO
from uuid import UUID

from sqlalchemy import delete, select, text
from sqlalchemy.engine import Connection

from db.models import (
    Deployment,
    DeploymentGeneration,
    DeploymentInstrument,
    DeploymentUniverseRevision,
    RiskAccount,
)
from db.session import create_database_engine, create_session_factory
from errors import QfError
from events import append_event
from risk import replace_projection
from settings import Settings


def _lock_key(value: UUID) -> int:
    number = int.from_bytes(value.bytes[:8], byteorder="big", signed=False)
    return number if number < 2**63 else number - 2**64


def _acquire_owner(connection: Connection, deployment_id: UUID) -> None:
    if connection.dialect.name != "postgresql":
        return
    acquired = connection.scalar(
        text("SELECT pg_try_advisory_lock(:key)"), {"key": _lock_key(deployment_id)}
    )
    if not acquired:
        raise QfError(
            "LIVE_START_FAILED",
            "Another runner already owns this Deployment.",
            409,
        )


def _decode_message(line: str) -> dict[str, Any]:
    if not line:
        raise QfError("LIVE_START_FAILED", "Live node exited before responding.", 503)
    try:
        value = json.loads(line)
    except json.JSONDecodeError as exc:
        raise QfError("LIVE_START_FAILED", "Live node returned invalid JSON.", 503) from exc
    if not isinstance(value, dict):
        raise QfError("LIVE_START_FAILED", "Live node response must be an object.", 503)
    return value


def _read_message(
    selector: selectors.BaseSelector,
    stream: TextIO,
    *,
    timeout: float,
) -> dict[str, Any]:
    events = selector.select(timeout)
    if not events:
        raise QfError("LIVE_START_FAILED", "Live node response timed out.", 503)
    return _decode_message(stream.readline())


def _send_command(stream: TextIO, command: str, **payload: Any) -> None:
    stream.write(json.dumps({"command": command, **payload}, separators=(",", ":")) + "\n")
    stream.flush()


def _mark_blocked(
    factory: Any,
    *,
    deployment_id: UUID,
    generation: int,
    message: str,
) -> None:
    with factory.begin() as session:
        deployment = session.get(Deployment, deployment_id)
        generation_row = session.execute(
            select(DeploymentGeneration).where(
                DeploymentGeneration.deployment_id == deployment_id,
                DeploymentGeneration.generation == generation,
            )
        ).scalar_one_or_none()
        if deployment is not None:
            deployment.observed_state = "RECOVERY_BLOCKED"
            deployment.last_error = message[-4000:]
            account = session.get(RiskAccount, deployment.funder_id)
            if account is not None:
                account.status = "BLOCKED"
                account.last_heartbeat_at = None
        if generation_row is not None:
            generation_row.state = "RECOVERY_BLOCKED"
            generation_row.last_error = message[-4000:]
            generation_row.stopped_at = datetime.now(UTC)
        append_event(
            session,
            kind="DEPLOYMENT_RECOVERY_BLOCKED",
            aggregate_type="deployment",
            aggregate_id=deployment_id,
            payload={"generation": generation, "message": message[-1000:]},
        )


def run(payload: dict[str, Any], settings: Settings) -> int:
    deployment_id = UUID(str(payload["deployment_id"]))
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    owner_connection = engine.connect()
    process: subprocess.Popen[str] | None = None
    generation = 0
    selector: selectors.BaseSelector | None = None
    try:
        _acquire_owner(owner_connection, deployment_id)
        with factory.begin() as session:
            deployment = session.execute(
                select(Deployment)
                .where(Deployment.id == deployment_id)
                .with_for_update()
            ).scalar_one_or_none()
            if deployment is None:
                raise QfError("DEPLOYMENT_UNKNOWN", "Deployment does not exist.", 404)
            if deployment.desired_state != "RUNNING":
                return 0
            generation = deployment.generation + 1
            deployment.generation = generation
            deployment.observed_state = "STARTING"
            deployment.last_error = None
            row = DeploymentGeneration(
                deployment_id=deployment.id,
                generation=generation,
                state="RECOVERY",
                runner_pid=os.getpid(),
            )
            session.add(row)
            account = session.get(RiskAccount, deployment.funder_id)
            if account is None:
                account = RiskAccount(funder_id=deployment.funder_id)
                session.add(account)
            account.status = "RECOVERING"
            account.owner_deployment_id = deployment.id
            account.owner_generation = generation
            account.last_heartbeat_at = None
            append_event(
                session,
                kind="DEPLOYMENT_RECOVERY_STARTED",
                aggregate_type="deployment",
                aggregate_id=deployment.id,
                payload={"generation": generation},
            )

        bundle_python = Path(str(payload["bundle_python"]))
        if not bundle_python.is_file():
            raise QfError("PLUGIN_RUNTIME_UNAVAILABLE", "Bundle Python is missing.", 503)
        child_payload = {
            **payload,
            "generation": generation,
            "heartbeat_seconds": settings.live_heartbeat_seconds,
        }
        child_environment = os.environ.copy()
        child_environment.pop("QUAZONAI_MASTER_KEY", None)
        process = subprocess.Popen(
            [str(bundle_python), "-m", "quazonai.runners.live_node"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
            env=child_environment,
        )
        assert process.stdin is not None and process.stdout is not None
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        process.stdin.write(json.dumps(child_payload, separators=(",", ":")) + "\n")
        process.stdin.flush()

        recovery = _read_message(
            selector,
            process.stdout,
            timeout=float(settings.live_runner_start_timeout_seconds),
        )
        if recovery.get("kind") != "RECOVERED":
            raise QfError(
                "RECOVERY_BLOCKED",
                "Live node did not complete Recovery.",
                503,
                {"response": recovery},
            )
        instruments = [str(item) for item in recovery.get("instruments") or []]
        if not instruments or len(instruments) != len(set(instruments)):
            raise QfError(
                "RECOVERY_BLOCKED",
                "Recovery must return a unique, non-empty instrument roster.",
                503,
            )
        limits = {instrument_id: 25_000_000 for instrument_id in instruments}

        with factory.begin() as session:
            deployment = session.execute(
                select(Deployment)
                .where(Deployment.id == deployment_id)
                .with_for_update()
            ).scalar_one()
            generation_row = session.execute(
                select(DeploymentGeneration).where(
                    DeploymentGeneration.deployment_id == deployment_id,
                    DeploymentGeneration.generation == generation,
                )
            ).scalar_one()
            replace_projection(
                session,
                funder_id=deployment.funder_id,
                deployment_id=deployment.id,
                generation=generation,
                positions=list(recovery.get("positions") or []),
                open_orders=list(recovery.get("open_orders") or []),
                mark_ready=False,
            )
            generation_row.state = "RECONCILED"
            revision = (
                session.get(DeploymentUniverseRevision, deployment.active_revision_id)
                if deployment.active_revision_id
                else None
            )
            if revision is None or revision.state not in {"APPROVED", "ACTIVE"}:
                raise QfError(
                    "RECOVERY_BLOCKED",
                    "Deployment has no approved active Universe revision.",
                    503,
                )
            session.execute(
                delete(DeploymentInstrument).where(
                    DeploymentInstrument.deployment_id == deployment.id,
                    DeploymentInstrument.revision_id == revision.id,
                )
            )
            for instrument_id in instruments:
                session.add(
                    DeploymentInstrument(
                        deployment_id=deployment.id,
                        revision_id=revision.id,
                        instrument_id=instrument_id,
                        lifecycle_state="ACTIVE",
                        risk_limit_micros=25_000_000,
                        last_reconciled_at=datetime.now(UTC),
                    )
                )
            revision.state = "ACTIVE"
            generation_row.state = "ARMED"
            append_event(
                session,
                kind="DEPLOYMENT_ARMED",
                aggregate_type="deployment",
                aggregate_id=deployment.id,
                payload={"generation": generation, "instrument_count": len(instruments)},
            )

        _send_command(
            process.stdin,
            "ARM",
            instrument_limits_micros=limits,
        )
        ready = _read_message(
            selector,
            process.stdout,
            timeout=float(settings.live_runner_start_timeout_seconds),
        )
        if ready.get("kind") != "STRATEGY_READY":
            raise QfError(
                "LIVE_START_FAILED",
                "Live node did not report Strategy readiness.",
                503,
                {"response": ready},
            )
        with factory.begin() as session:
            generation_row = session.execute(
                select(DeploymentGeneration).where(
                    DeploymentGeneration.deployment_id == deployment_id,
                    DeploymentGeneration.generation == generation,
                )
            ).scalar_one()
            generation_row.state = "STRATEGY_READY"
            append_event(
                session,
                kind="DEPLOYMENT_STRATEGY_READY",
                aggregate_type="deployment",
                aggregate_id=deployment_id,
                payload={"generation": generation},
            )

        _send_command(process.stdin, "START")
        trading = _read_message(
            selector,
            process.stdout,
            timeout=float(settings.live_runner_start_timeout_seconds),
        )
        if trading.get("kind") != "TRADING":
            raise QfError(
                "LIVE_START_FAILED",
                "Live node did not enter TRADING.",
                503,
                {"response": trading},
            )
        with factory.begin() as session:
            deployment = session.get(Deployment, deployment_id)
            generation_row = session.execute(
                select(DeploymentGeneration).where(
                    DeploymentGeneration.deployment_id == deployment_id,
                    DeploymentGeneration.generation == generation,
                )
            ).scalar_one()
            assert deployment is not None
            now = datetime.now(UTC)
            deployment.observed_state = "RUNNING"
            generation_row.state = "TRADING"
            generation_row.last_heartbeat_at = now
            account = session.get(RiskAccount, deployment.funder_id)
            assert account is not None
            account.last_heartbeat_at = now
            account.status = "READY"
            append_event(
                session,
                kind="DEPLOYMENT_TRADING",
                aggregate_type="deployment",
                aggregate_id=deployment.id,
                payload={"generation": generation},
            )

        while True:
            owner_connection.execute(text("SELECT 1"))
            with factory() as session:
                desired = session.scalar(
                    select(Deployment.desired_state).where(Deployment.id == deployment_id)
                )
            if desired != "RUNNING":
                break
            events = selector.select(settings.supervisor_poll_seconds)
            if not events:
                if process.poll() is not None:
                    raise QfError(
                        "RECOVERY_BLOCKED",
                        "Live node exited unexpectedly.",
                        503,
                    )
                continue
            message = _decode_message(process.stdout.readline())
            if message.get("kind") == "HEARTBEAT":
                now = datetime.now(UTC)
                with factory.begin() as session:
                    deployment = session.get(Deployment, deployment_id)
                    generation_row = session.execute(
                        select(DeploymentGeneration).where(
                            DeploymentGeneration.deployment_id == deployment_id,
                            DeploymentGeneration.generation == generation,
                        )
                    ).scalar_one()
                    if deployment is not None:
                        account = session.get(RiskAccount, deployment.funder_id)
                        if account is not None:
                            account.last_heartbeat_at = now
                    generation_row.last_heartbeat_at = now
            elif message.get("kind") == "ERROR":
                raise QfError(
                    "RECOVERY_BLOCKED",
                    str(message.get("message") or "Live node reported an error."),
                    503,
                )

        with factory.begin() as session:
            deployment = session.get(Deployment, deployment_id)
            if deployment is not None:
                deployment.observed_state = "STOPPING"
                generation_row = session.execute(
                    select(DeploymentGeneration).where(
                        DeploymentGeneration.deployment_id == deployment_id,
                        DeploymentGeneration.generation == generation,
                    )
                ).scalar_one()
                generation_row.state = "STOPPING"
        _send_command(process.stdin, "STOP")
        try:
            process.wait(timeout=settings.live_runner_stop_timeout_seconds)
        except subprocess.TimeoutExpired:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        with factory.begin() as session:
            deployment = session.get(Deployment, deployment_id)
            generation_row = session.execute(
                select(DeploymentGeneration).where(
                    DeploymentGeneration.deployment_id == deployment_id,
                    DeploymentGeneration.generation == generation,
                )
            ).scalar_one()
            assert deployment is not None
            deployment.observed_state = "STOPPED"
            generation_row.state = "STOPPED"
            generation_row.stopped_at = datetime.now(UTC)
            account = session.get(RiskAccount, deployment.funder_id)
            if account is not None:
                account.status = "STOPPED"
                account.last_heartbeat_at = None
            append_event(
                session,
                kind="DEPLOYMENT_STOPPED",
                aggregate_type="deployment",
                aggregate_id=deployment.id,
                payload={"generation": generation, "positions_liquidated": False},
            )
        return 0
    except Exception as exc:
        if generation:
            _mark_blocked(
                factory,
                deployment_id=deployment_id,
                generation=generation,
                message=str(exc),
            )
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        raise
    finally:
        if selector is not None:
            selector.close()
        owner_connection.close()
        engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one QuaZonai Deployment generation")
    parser.add_argument("--payload-stdin", action="store_true", required=True)
    return parser


def main() -> int:
    build_parser().parse_args()
    payload = json.load(sys.stdin)
    if not isinstance(payload, dict):
        raise RuntimeError("runner payload must be a JSON object")
    return run(payload, Settings.from_env())


if __name__ == "__main__":
    raise SystemExit(main())
