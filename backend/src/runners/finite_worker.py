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
from datetime import UTC, datetime, timedelta
from threading import Event, Thread
from codex_chatgpt_auth import initialize_codex_auth
from db.models import Job
from db.session import SessionFactory, create_database_engine, create_session_factory, ping_database
from errors import QfError
from events import append_event
from jobs import (
    JobLease,
    claim_next_job,
    complete_job,
    fail_job,
    retry_job,
    release_expired_leases,
    renew_job_lease,
)
from logging_utils import configure_logging
from runtime_config import effective_settings
from runners.codex_sandbox import codex_sandbox_preflight
from settings import Settings
from research_engine.trusted_evaluator_service import (
    terminalize_trusted_evaluator_failure,
    trusted_evaluator_assignment_running,
)

LOGGER = logging.getLogger("quazonai.finite_worker")
Handler = Callable[[Settings, Job, Event | None], None]
_TRUSTED_EVALUATOR_MAX_ATTEMPTS = 3


def _noop_handler(_: Settings, __: Job, ___: Event | None = None) -> None:
    return


def _terminate_child_process_group(process: subprocess.Popen[str]) -> None:
    try:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGTERM)
        else:
            process.terminate()
    except ProcessLookupError:
        pass
    try:
        process.communicate(timeout=0.25)
    except subprocess.TimeoutExpired:
        pass
    # When the direct child has already exited, communicate() returns without
    # waiting for a forked descendant in its process group.  Give TERM a short
    # chance to reach that descendant before the final group kill.
    if os.name == "posix":
        time.sleep(0.1)
    try:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        else:
            process.kill()
    except ProcessLookupError:
        pass
    try:
        process.communicate(timeout=0.25)
    except subprocess.TimeoutExpired:
        pass


def _child_handler(
    module: str,
    *fixed_arguments: str,
    timeout_attribute: str = "plugin_job_timeout_seconds",
    redact_child_failure: bool = False,
) -> Handler:
    def handler(settings: Settings, job: Job, lease_lost: Event | None = None) -> None:
        timeout = float(getattr(settings, timeout_attribute))
        if job.lease_owner is None:
            raise RuntimeError("Claimed job is missing its lease owner")
        if lease_lost is not None and lease_lost.is_set():
            raise QfError(
                "JOB_LEASE_LOST",
                "Job lease was lost before its child process could start.",
                409,
            )
        process = subprocess.Popen(
            [
                sys.executable,
                "-m",
                module,
                *fixed_arguments,
                str(job.id),
                "--lease-owner",
                job.lease_owner,
                "--lease-attempt",
                str(job.attempt),
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL if redact_child_failure else subprocess.PIPE,
            text=True,
            env=os.environ.copy(),
            start_new_session=True,
        )
        deadline = time.monotonic() + timeout
        while True:
            if lease_lost is not None and lease_lost.is_set():
                _terminate_child_process_group(process)
                raise QfError(
                    "JOB_LEASE_LOST",
                    "Job lease was lost while its child process was running.",
                    409,
                )
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                _terminate_child_process_group(process)
                raise RuntimeError(f"{job.kind} child exceeded its time limit")
            try:
                _, stderr = process.communicate(timeout=min(0.25, remaining))
                break
            except subprocess.TimeoutExpired:
                continue
        if lease_lost is not None and lease_lost.is_set():
            _terminate_child_process_group(process)
            raise QfError(
                "JOB_LEASE_LOST",
                "Job lease was lost while its child process was running.",
                409,
            )
        if process.returncode:
            # The trusted evaluator inherits this child's new process group.  The
            # child may have exited while a forked evaluator descendant still
            # holds its stdout pipe, so always clear the group before reporting
            # the generic failure.
            _terminate_child_process_group(process)
            if redact_child_failure:
                raise QfError(
                    "TRUSTED_EVALUATOR_FAILED",
                    "Trusted evaluator job failed without a result.",
                    502,
                )
            message = (stderr or "").strip()[-2000:]
            raise RuntimeError(
                f"{job.kind} child failed with exit code {process.returncode}: {message}"
            )

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
    "SEALED_EVALUATION": _child_handler(
        "runners.sealed_evaluator",
        "run",
        timeout_attribute="mission_job_timeout_seconds",
    ),
    "DISCOVERY_EVALUATION": _child_handler(
        "runners.sealed_evaluator",
        "run-discovery",
        timeout_attribute="mission_job_timeout_seconds",
        redact_child_failure=True,
    ),
    "ALPHA_EVALUATION": _child_handler(
        "runners.sealed_evaluator",
        "run-alpha",
        timeout_attribute="mission_job_timeout_seconds",
        redact_child_failure=True,
    ),
    "PORTFOLIO_INPUT_EVALUATION": _child_handler(
        "runners.sealed_evaluator",
        "run-portfolio-input",
        timeout_attribute="mission_job_timeout_seconds",
        redact_child_failure=True,
    ),
    "PORTFOLIO_ASSEMBLY": _child_handler(
        "runners.portfolio_assembly",
        "run",
        timeout_attribute="mission_job_timeout_seconds",
        redact_child_failure=True,
    ),
    "PORTFOLIO_EVALUATION": _child_handler(
        "runners.sealed_evaluator",
        "run-portfolio",
        timeout_attribute="mission_job_timeout_seconds",
        redact_child_failure=True,
    ),
    "PORTFOLIO_TO_PAPER_PROMOTION": _child_handler(
        "runners.promotion",
        "run-p2p",
        timeout_attribute="mission_job_timeout_seconds",
        redact_child_failure=True,
    ),
    "PAPER_TO_LIVE_PROMOTION": _child_handler(
        "runners.promotion",
        "run",
        timeout_attribute="mission_job_timeout_seconds",
        redact_child_failure=True,
    ),
    "SEALED_CATALOG_PROVISION": _child_handler(
        "runners.sealed_catalog_provision",
        "run",
    ),
    "CANDIDATE_PACKAGE_BUILD": _child_handler(
        "runners.candidate_package_build",
        "run",
    ),
    "DATASET_MATERIALIZATION": _child_handler(
        "runners.dataset_materialization",
        "run",
    ),
    "DATA_SOURCE_PREFLIGHT": _child_handler(
        "runners.data_source_preflight",
        "run",
    ),
}


def _start_lease_renewer(
    factory: SessionFactory,
    *,
    lease: JobLease,
    lease_seconds: int,
) -> tuple[Event, Event, Thread]:
    stop = Event()
    lease_lost = Event()
    interval = max(0.1, min(30.0, lease_seconds / 3))

    def renew() -> None:
        while not stop.wait(interval):
            try:
                with factory.begin() as session:
                    renewed = renew_job_lease(
                        session,
                        lease=lease,
                        lease_seconds=lease_seconds,
                    )
                if not renewed:
                    LOGGER.error("job lease was lost", extra={"job_id": str(lease.job_id)})
                    lease_lost.set()
                    return
            except Exception:  # noqa: BLE001 - heartbeat must keep retrying transient DB failures
                LOGGER.exception("job lease renewal failed", extra={"job_id": str(lease.job_id)})

    thread = Thread(
        target=renew,
        name=f"job-lease-renewer-{lease.job_id}",
        daemon=True,
    )
    thread.start()
    return stop, lease_lost, thread


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
    """Claim at most one job using the worker's shared database pool.

    Runtime configuration is loaded in the same short transaction that claims the
    next job. This preserves the rule that every newly admitted job freezes the
    latest effective settings while avoiding per-poll Engine construction.
    """
    with factory() as session:
        settings = effective_settings(session, base_settings)
    legacy_auth_path = settings.codex_home / "auth.json"
    if legacy_auth_path.exists():
        # This must happen before claiming a job: API and worker startup can
        # race during upgrades, and custom-provider jobs must not bypass the
        # shared one-time legacy import/cleanup decision.
        initialize_codex_auth(factory, settings)

    with factory.begin() as session:
        settings = effective_settings(session, base_settings)
        release_expired_leases(session)
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
        if job.lease_owner is None:
            raise RuntimeError("Claimed job is missing its lease owner")
        lease = JobLease(job_id=job.id, owner=job.lease_owner, attempt=job.attempt)
        session.expunge(job)

    handler = HANDLERS.get(job.kind)
    lease_stop: Event | None = None
    lease_thread: Thread | None = None
    lease_lost = Event()
    failure: Exception | None = None
    try:
        if handler is None:
            raise RuntimeError(f"Unsupported job kind: {job.kind}")
        lease_stop, lease_lost, lease_thread = _start_lease_renewer(
            factory,
            lease=lease,
            lease_seconds=settings.job_lease_seconds,
        )
        handler(settings, job, lease_lost)
    except Exception as exc:  # noqa: BLE001 - durable job failure boundary
        failure = exc
    finally:
        if lease_stop is not None:
            lease_stop.set()
        if lease_thread is not None:
            lease_thread.join()

    if lease_lost.is_set():
        failure = QfError(
            "JOB_LEASE_LOST",
            "Job lease was lost before the worker could finalize the child result.",
            409,
        )
    if failure is not None:
        with factory.begin() as session:
            evaluator_retryable = job.kind in {
                "DISCOVERY_EVALUATION",
                "ALPHA_EVALUATION",
                "PORTFOLIO_INPUT_EVALUATION",
                "PORTFOLIO_EVALUATION",
            } and trusted_evaluator_assignment_running(
                session, kind=job.kind, resource_id=job.resource_id
            )
            production_retryable = job.kind in {
                "CANDIDATE_PACKAGE_BUILD",
                "PORTFOLIO_TO_PAPER_PROMOTION",
                "PAPER_TO_LIVE_PROMOTION",
            }
            retryable = (
                (evaluator_retryable or production_retryable)
                and job.attempt < _TRUSTED_EVALUATOR_MAX_ATTEMPTS
            )
            failure_message = str(failure)[-4000:]
            retry_at = datetime.now(UTC) + timedelta(
                seconds=min(60, 2 ** max(job.attempt - 1, 0))
            )
            handled = (
                retry_job(
                    session,
                    failure_message,
                    lease=lease,
                    available_at=retry_at,
                )
                if retryable
                else fail_job(session, failure_message, lease=lease)
            )
            if handled:
                if not retryable and evaluator_retryable:
                    terminalize_trusted_evaluator_failure(
                        session,
                        kind=job.kind,
                        resource_id=job.resource_id,
                        outcome_code="RETRIES_EXHAUSTED",
                    )
                append_event(
                    session,
                    kind="JOB_REQUEUED" if retryable else "JOB_FAILED",
                    aggregate_type="job",
                    aggregate_id=lease.job_id,
                    payload={"error_code": type(failure).__name__, "retryable": retryable},
                )
        LOGGER.error(
            "job failed",
            exc_info=(type(failure), failure, failure.__traceback__),
            extra={"job_id": str(job.id)},
        )
        return True, settings.job_poll_seconds

    with factory.begin() as session:
        if complete_job(session, lease=lease):
            append_event(
                session,
                kind="JOB_SUCCEEDED",
                aggregate_type="job",
                aggregate_id=lease.job_id,
                payload={"kind": job.kind},
            )
        else:
            LOGGER.error("job lease was lost before successful finalization", extra={"job_id": str(job.id)})
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
            codex_sandbox_preflight()
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
