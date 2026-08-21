"""Durable finite-job worker with isolated child processes for plugin and research code."""

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

from quantfoundry.db.models import Job
from quantfoundry.db.session import (
    create_database_engine,
    create_session_factory,
    ping_database,
)
from quantfoundry.events import append_event
from quantfoundry.jobs import claim_next_job, complete_job, fail_job, release_expired_leases
from quantfoundry.logging_utils import configure_logging
from quantfoundry.settings import Settings

LOGGER = logging.getLogger("quantfoundry.finite_worker")
Handler = Callable[[Settings, Job], None]


def _noop_handler(_: Settings, __: Job) -> None:
    return


def _child_handler(module: str, *fixed_arguments: str) -> Handler:
    def handler(settings: Settings, job: Job) -> None:
        timeout = (
            settings.plugin_job_timeout_seconds
            if module.endswith("plugin_jobs")
            else settings.research_job_timeout_seconds
        )
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
            raise RuntimeError(
                f"{job.kind} child failed with exit code {exc.returncode}"
            ) from exc

    return handler


HANDLERS: dict[str, Handler] = {
    "SYSTEM_NOOP": _noop_handler,
    "PLUGIN_INSTALL": _child_handler("quantfoundry.runners.plugin_jobs", "install"),
    "PLUGIN_BUNDLE_BUILD": _child_handler("quantfoundry.runners.plugin_jobs", "build"),
    "PLUGIN_REMOVE": _child_handler("quantfoundry.runners.plugin_jobs", "remove"),
    "PARQUET_IMPORT": _child_handler("quantfoundry.runners.import_parquet"),
    "BACKTEST": _child_handler("quantfoundry.runners.research_jobs", "backtest"),
    "OPTIMIZATION": _child_handler("quantfoundry.runners.research_jobs", "optimization"),
    "HOLDOUT": _child_handler("quantfoundry.runners.research_jobs", "holdout"),
}


class StopFlag:
    requested = False

    def request(self, *_: object) -> None:
        self.requested = True


def run_once(settings: Settings, *, owner: str) -> bool:
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    with factory.begin() as session:
        release_expired_leases(session)
        job = claim_next_job(
            session,
            owner=owner,
            lease_seconds=settings.job_lease_seconds,
        )
        if job is None:
            return False
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
        return True

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
    return True


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="QuantFoundry finite worker")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--once", action="store_true")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    configure_logging()
    settings = Settings.from_env()
    settings.ensure_worker_directories()
    engine = create_database_engine(settings)
    if args.check:
        ping_database(engine)
        return 0

    owner = f"{socket.gethostname()}:{os.getpid()}"
    if args.once:
        run_once(settings, owner=owner)
        return 0

    stop = StopFlag()
    signal.signal(signal.SIGTERM, stop.request)
    signal.signal(signal.SIGINT, stop.request)
    LOGGER.info("finite worker started")
    while not stop.requested:
        worked = run_once(settings, owner=owner)
        if not worked:
            time.sleep(settings.job_poll_seconds)
    LOGGER.info("finite worker stopped")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
