"""Durable finite-job worker for bounded research/data infrastructure work."""

from __future__ import annotations

import argparse
import logging
import os
import signal
import socket
import subprocess
import sys
import time
from collections.abc import Callable, Sequence

from db.models import Job
from db.session import SessionFactory, create_database_engine, create_session_factory, ping_database
from events import append_event
from jobs import claim_next_job, complete_job, fail_job, release_expired_leases
from logging_utils import configure_logging
from quant_runtime.degradation import schedule_degradation_missions
from runtime_config import effective_settings
from settings import Settings

LOGGER = logging.getLogger("quazonai.finite_worker")
Handler = Callable[[Settings, Job], None]


def _noop_handler(_: Settings, __: Job) -> None:
    return


def _child_handler(
    module: str,
    *fixed_arguments: str,
    timeout_attribute: str = "plugin_job_timeout_seconds",
) -> Handler:
    def handler(settings: Settings, job: Job) -> None:
        timeout = float(getattr(settings, timeout_attribute))
        try:
            subprocess.run(
                [sys.executable, "-m", module, *fixed_arguments, str(job.id)],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
                text=True,
                timeout=timeout,
                env=os.environ.copy(),
            )
        except subprocess.TimeoutExpired as exc:
            raise RuntimeError(f"{job.kind} child exceeded its time limit") from exc
        except subprocess.CalledProcessError as exc:
            message = (exc.stderr or "").strip()[-2000:]
            raise RuntimeError(
                f"{job.kind} child failed with exit code {exc.returncode}: {message}"
            ) from exc

    return handler


HANDLERS: dict[str, Handler] = {
    "SYSTEM_NOOP": _noop_handler,
    "PLUGIN_INSTALL": _child_handler("runners.plugin_jobs", "install"),
    "PLUGIN_BUNDLE_BUILD": _child_handler("runners.plugin_jobs", "build"),
    "PLUGIN_REMOVE": _child_handler("runners.plugin_jobs", "remove"),
    "RESEARCH_MISSION": _child_handler(
        "runners.research_missions",
        "run",
        timeout_attribute="mission_job_timeout_seconds",
    ),
}


class StopFlag:
    requested = False

    def request(self, *_: object) -> None:
        self.requested = True


def run_once(
    base_settings: Settings,
    *,
    owner: str,
    factory: SessionFactory,
) -> tuple[bool, float]:
    """Admit degradation follow-ups, then claim at most one durable job.

    Runtime configuration is loaded in the same short transaction that claims the
    next job. Explicit degraded Forward Evidence can enqueue a bounded research
    Mission in this transaction; the worker may then claim it immediately. This
    feedback loop only creates research work and never mutates downstream runtime
    or broker state.
    """
    with factory.begin() as session:
        settings = effective_settings(session, base_settings)
        release_expired_leases(session)
        schedule_degradation_missions(session)
        job = claim_next_job(session, owner=owner, lease_seconds=settings.job_lease_seconds)
        if job is None:
            return False, settings.job_poll_seconds
        append_event(
            session,
            kind="JOB_LEASED",
            aggregate_type="job",
            aggregate_id=job.id,
            payload={"kind": job.kind, "attempt": job.attempt},
        )
        session.expunge(job)

    handler = HANDLERS.get(job.kind)
    try:
        if handler is None:
            raise RuntimeError(f"Unsupported job kind: {job.kind}")
        handler(settings, job)
    except Exception as exc:  # noqa: BLE001 - durable job failure boundary
        with factory.begin() as session:
            current = session.get(Job, job.id)
            if current is not None:
                fail_job(session, current, str(exc)[-4000:])
                append_event(
                    session,
                    kind="JOB_FAILED",
                    aggregate_type="job",
                    aggregate_id=current.id,
                    payload={"error_code": type(exc).__name__},
                )
        LOGGER.exception("job failed", extra={"job_id": str(job.id)})
        return True, settings.job_poll_seconds

    with factory.begin() as session:
        current = session.get(Job, job.id)
        if current is not None:
            complete_job(session, current)
            append_event(
                session,
                kind="JOB_SUCCEEDED",
                aggregate_type="job",
                aggregate_id=current.id,
                payload={"kind": current.kind},
            )
    return True, settings.job_poll_seconds


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="QuaZonai finite research worker")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--once", action="store_true")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    configure_logging()
    base_settings = Settings.from_env()
    base_settings.ensure_worker_directories()
    engine = create_database_engine(base_settings)
    factory = create_session_factory(engine)
    try:
        if args.check:
            ping_database(engine)
            return 0

        owner = f"{socket.gethostname()}:{os.getpid()}"
        if args.once:
            run_once(base_settings, owner=owner, factory=factory)
            return 0

        stop = StopFlag()
        signal.signal(signal.SIGTERM, stop.request)
        signal.signal(signal.SIGINT, stop.request)
        LOGGER.info("finite worker started")
        while not stop.requested:
            worked, poll_seconds = run_once(base_settings, owner=owner, factory=factory)
            if not worked:
                time.sleep(poll_seconds)
        LOGGER.info("finite worker stopped")
        return 0
    finally:
        engine.dispose()


if __name__ == "__main__":
    raise SystemExit(main())
