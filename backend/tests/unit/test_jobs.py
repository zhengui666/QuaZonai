from __future__ import annotations

from datetime import UTC, datetime, timedelta
import os
from pathlib import Path
import signal
import subprocess
from threading import Event as ThreadEvent
import time
from uuid import uuid4

import pytest
from sqlalchemy import select
from sqlalchemy.orm import sessionmaker

from db.models import Event as DomainEvent
from db.models import Job
from errors import QfError
import jobs
from jobs import (
    JobLease,
    claim_next_job,
    complete_job,
    create_lease_fenced_session_factory,
    enqueue_job,
    fail_job,
    release_expired_leases,
    renew_job_lease,
)
from runners import finite_worker
from runners.finite_worker import run_once


def test_claim_and_release_expired_job(engine) -> None:  # type: ignore[no-untyped-def]
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    resource_id = uuid4()
    with factory.begin() as session:
        created = enqueue_job(
            session,
            kind="SYSTEM_NOOP",
            resource_type="system",
            resource_id=resource_id,
        )
        job_id = created.id

    now = datetime.now(UTC)
    with factory.begin() as session:
        claimed = claim_next_job(session, owner="worker-a", lease_seconds=1, now=now)
        assert claimed is not None
        assert claimed.id == job_id
        assert claimed.state == "LEASED"

    with factory.begin() as session:
        job = session.get(Job, job_id)
        assert job is not None
        job.lease_expires_at = now - timedelta(seconds=1)

    with factory.begin() as session:
        released = release_expired_leases(session, now=now)
        assert released == 1

    with factory() as session:
        job = session.get(Job, job_id)
        assert job is not None
        assert job.state == "READY"
        assert job.lease_owner is None


def test_worker_run_once_uses_shared_factory_and_completes_job(
    engine,  # type: ignore[no-untyped-def]
    settings,  # type: ignore[no-untyped-def]
) -> None:
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        created = enqueue_job(
            session,
            kind="SYSTEM_NOOP",
            resource_type="system",
            resource_id=uuid4(),
        )
        job_id = created.id

    worked, poll_seconds = run_once(settings, owner="worker-a", factory=factory)

    assert worked is True
    assert poll_seconds == settings.job_poll_seconds
    with factory() as session:
        job = session.get(Job, job_id)
        assert job is not None
        assert job.state == "SUCCEEDED"
        assert job.lease_owner is None
        assert job.lease_expires_at is None


def test_worker_imports_legacy_auth_before_claiming_a_job(engine, settings, monkeypatch) -> None:  # type: ignore[no-untyped-def]
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        enqueue_job(
            session,
            kind="SYSTEM_NOOP",
            resource_type="system",
            resource_id=uuid4(),
        )
    settings.codex_home.mkdir(parents=True, exist_ok=True)
    (settings.codex_home / "auth.json").write_text("legacy", encoding="utf-8")

    order: list[str] = []
    real_claim = finite_worker.claim_next_job
    monkeypatch.setattr(
        finite_worker,
        "initialize_codex_auth",
        lambda *_args: order.append("legacy-import"),
    )

    def record_claim(*args, **kwargs):  # type: ignore[no-untyped-def]
        order.append("claim")
        return real_claim(*args, **kwargs)

    monkeypatch.setattr(finite_worker, "claim_next_job", record_claim)
    run_once(settings, owner="worker-a", factory=factory)

    assert order[:2] == ["legacy-import", "claim"]


def test_renew_job_lease_extends_only_the_current_owner(
    engine,  # type: ignore[no-untyped-def]
) -> None:
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        created = enqueue_job(
            session,
            kind="SYSTEM_NOOP",
            resource_type="system",
            resource_id=uuid4(),
        )
        job_id = created.id

    now = datetime.now(UTC)
    with factory.begin() as session:
        claimed = claim_next_job(session, owner="worker-a", lease_seconds=60, now=now)
        assert claimed is not None
        lease = JobLease(claimed.id, "worker-a", claimed.attempt)

    renewed_at = now + timedelta(seconds=1)
    with factory.begin() as session:
        assert renew_job_lease(
            session,
            lease=lease,
            lease_seconds=60,
            now=renewed_at,
        )

    with factory.begin() as session:
        assert not renew_job_lease(
            session,
            lease=JobLease(job_id, "worker-b", lease.attempt),
            lease_seconds=60,
            now=renewed_at,
        )

    with factory() as session:
        job = session.get(Job, job_id)
        assert job is not None
        assert job.lease_owner == "worker-a"
        stored_expiry = job.lease_expires_at
        assert stored_expiry is not None
        if stored_expiry.tzinfo is None:
            stored_expiry = stored_expiry.replace(tzinfo=UTC)
        assert stored_expiry == renewed_at + timedelta(seconds=60)


def test_expired_or_reclaimed_attempt_cannot_renew_or_finish(engine) -> None:  # type: ignore[no-untyped-def]
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    now = datetime.now(UTC)
    with factory.begin() as session:
        job = enqueue_job(
            session,
            kind="SYSTEM_NOOP",
            resource_type="system",
            resource_id=uuid4(),
            available_at=now,
        )
        job_id = job.id
        claimed = claim_next_job(session, owner="worker-a", lease_seconds=1, now=now)
        assert claimed is not None and claimed.lease_owner is not None
        stale = JobLease(claimed.id, claimed.lease_owner, claimed.attempt)

    expired_at = now + timedelta(seconds=1)
    with factory.begin() as session:
        assert not renew_job_lease(session, lease=stale, lease_seconds=60, now=expired_at)
        assert not complete_job(session, lease=stale, now=expired_at)
        assert not fail_job(session, "late failure", lease=stale, now=expired_at)
        assert release_expired_leases(session, now=expired_at) == 1
        reclaimed = claim_next_job(session, owner="worker-a", lease_seconds=60, now=expired_at)
        assert reclaimed is not None and reclaimed.attempt == stale.attempt + 1

    with factory.begin() as session:
        assert not renew_job_lease(session, lease=stale, lease_seconds=60, now=expired_at)
        assert not complete_job(session, lease=stale, now=expired_at)
        assert not fail_job(session, "late failure", lease=stale, now=expired_at)

    with factory() as session:
        job = session.get(Job, job_id)
        assert job is not None
        assert (job.state, job.lease_owner, job.attempt) == ("LEASED", "worker-a", 2)


def test_fenced_session_rechecks_the_lease_at_commit(engine, monkeypatch: pytest.MonkeyPatch) -> None:  # type: ignore[no-untyped-def]
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    current = datetime(2026, 1, 1, tzinfo=UTC)
    with factory.begin() as session:
        job = enqueue_job(
            session,
            kind="SYSTEM_NOOP",
            resource_type="system",
            resource_id=uuid4(),
            available_at=current,
        )
        claimed = claim_next_job(session, owner="worker-a", lease_seconds=10, now=current)
        assert claimed is not None and claimed.lease_owner is not None
        lease = JobLease(claimed.id, claimed.lease_owner, claimed.attempt)

    monkeypatch.setattr(jobs, "_now", lambda: current)
    fenced = create_lease_fenced_session_factory(engine, lease)
    with pytest.raises(QfError, match="JOB_LEASE_LOST"):
        with fenced.begin() as session:
            leased = session.get(Job, lease.job_id)
            assert leased is not None
            leased.last_error = "must roll back"
            monkeypatch.setattr(jobs, "_now", lambda: current + timedelta(seconds=11))

    with factory() as session:
        job = session.get(Job, lease.job_id)
        assert job is not None
        assert job.last_error is None


@pytest.mark.parametrize(
    ("kind", "module", "action", "redacted"),
    [
        ("PLUGIN_INSTALL", "runners.plugin_jobs", "install", False),
        ("PLUGIN_BUNDLE_BUILD", "runners.plugin_jobs", "build", False),
        ("PLUGIN_REMOVE", "runners.plugin_jobs", "remove", False),
        ("RESEARCH_MISSION", "runners.research_missions", "run", False),
        ("SEALED_EVALUATION", "runners.sealed_evaluator", "run", False),
        ("DISCOVERY_EVALUATION", "runners.sealed_evaluator", "run-discovery", True),
        ("ALPHA_EVALUATION", "runners.sealed_evaluator", "run-alpha", True),
        ("PORTFOLIO_INPUT_EVALUATION", "runners.sealed_evaluator", "run-portfolio-input", True),
        ("PORTFOLIO_EVALUATION", "runners.sealed_evaluator", "run-portfolio", True),
        ("PORTFOLIO_TO_PAPER_PROMOTION", "runners.promotion", "run-p2p", True),
        ("PAPER_TO_LIVE_PROMOTION", "runners.promotion", "run", True),
        ("SEALED_CATALOG_PROVISION", "runners.sealed_catalog_provision", "run", False),
        ("CANDIDATE_PACKAGE_BUILD", "runners.candidate_package_build", "run", False),
        ("DATASET_MATERIALIZATION", "runners.dataset_materialization", "run", False),
        ("DATA_SOURCE_PREFLIGHT", "runners.data_source_preflight", "run", False),
    ],
)
def test_child_handlers_forward_the_exact_lease_identity(
    settings,  # type: ignore[no-untyped-def]
    monkeypatch: pytest.MonkeyPatch,
    kind: str,
    module: str,
    action: str,
    redacted: bool,
) -> None:
    commands: list[list[str]] = []
    options: list[dict[str, object]] = []

    class CompletedChild:
        pid = 42
        returncode = 0

        def communicate(self, timeout: float | None = None) -> tuple[str, str]:
            del timeout
            return "", ""

    def record(command: list[str], **kwargs: object) -> CompletedChild:
        commands.append(command)
        options.append(kwargs)
        return CompletedChild()

    monkeypatch.setattr(finite_worker.subprocess, "Popen", record)
    job = Job(
        id=uuid4(),
        kind=kind,
        resource_type="system",
        resource_id=uuid4(),
        state="LEASED",
        lease_owner="worker-a",
        attempt=7,
    )

    finite_worker.HANDLERS[kind](settings, job)

    assert commands == [
        [
            finite_worker.sys.executable,
            "-m",
            module,
            action,
            str(job.id),
            "--lease-owner",
            "worker-a",
            "--lease-attempt",
            "7",
        ]
    ]
    assert options[0]["start_new_session"] is True
    assert options[0]["stderr"] is (subprocess.DEVNULL if redacted else subprocess.PIPE)


def test_trusted_evaluator_child_failure_does_not_surface_stderr(
    settings,  # type: ignore[no-untyped-def]
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    class FailedChild:
        pid = 42
        returncode = 1

        def communicate(self, timeout: float | None = None) -> tuple[str, str]:
            del timeout
            return "", "evaluator-private://secret-result"

    options: list[dict[str, object]] = []
    terminated: list[object] = []

    def start_child(_command: list[str], **kwargs: object) -> FailedChild:
        options.append(kwargs)
        return FailedChild()

    monkeypatch.setattr(finite_worker.subprocess, "Popen", start_child)
    monkeypatch.setattr(
        finite_worker,
        "_terminate_child_process_group",
        lambda process: terminated.append(process),
    )
    job = Job(
        id=uuid4(),
        kind="ALPHA_EVALUATION",
        resource_type="alpha_evaluation_assignment",
        resource_id=uuid4(),
        state="LEASED",
        lease_owner="worker-a",
        attempt=1,
    )

    with pytest.raises(QfError, match="TRUSTED_EVALUATOR_FAILED") as error:
        finite_worker.HANDLERS["ALPHA_EVALUATION"](settings, job)

    assert "evaluator-private" not in str(error.value)
    assert options[0]["stderr"] is subprocess.DEVNULL
    assert terminated


@pytest.mark.skipif(os.name != "posix", reason="process groups require POSIX")
def test_trusted_evaluator_nonzero_child_cleans_same_group_descendant(
    settings,  # type: ignore[no-untyped-def]
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    marker = tmp_path / "descendant-state"
    module = tmp_path / "trusted_group_probe.py"
    module.write_text(
        """
import os
import signal
import sys
import time
from pathlib import Path

marker = Path(os.environ[\"TRUSTED_GROUP_MARKER\"])
child = os.fork()
if child:
    marker.write_text(\"spawned\", encoding=\"utf-8\")
    raise SystemExit(17)

def stop(*_args):
    marker.write_text(\"terminated\", encoding=\"utf-8\")
    raise SystemExit(0)

signal.signal(signal.SIGTERM, stop)
while True:
    time.sleep(0.01)
""",
        encoding="utf-8",
    )
    previous_path = os.environ.get("PYTHONPATH")
    monkeypatch.setenv(
        "PYTHONPATH",
        str(tmp_path) if not previous_path else f"{tmp_path}{os.pathsep}{previous_path}",
    )
    monkeypatch.setenv("TRUSTED_GROUP_MARKER", str(marker))
    job = Job(
        id=uuid4(),
        kind="ALPHA_EVALUATION",
        resource_type="alpha_evaluation_assignment",
        resource_id=uuid4(),
        state="LEASED",
        lease_owner="worker-a",
        attempt=1,
    )

    with pytest.raises(QfError, match="TRUSTED_EVALUATOR_FAILED"):
        finite_worker._child_handler(  # noqa: SLF001 - exercise group cleanup boundary
            "trusted_group_probe",
            redact_child_failure=True,
        )(settings, job)

    deadline = time.monotonic() + 2
    while not marker.exists() or marker.read_text(encoding="utf-8") != "terminated":
        assert time.monotonic() < deadline
        time.sleep(0.01)


def test_worker_terminates_a_reclaimed_child_before_it_can_finalize(
    engine,  # type: ignore[no-untyped-def]
    settings,  # type: ignore[no-untyped-def]
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        job = enqueue_job(
            session,
            kind="PLUGIN_INSTALL",
            resource_type="system",
            resource_id=uuid4(),
        )
        job_id = job.id

    lease_lost = ThreadEvent()
    captured: dict[str, object] = {}

    class JoinedThread:
        def join(self) -> None:
            return

    def start_renewer(
        *_args: object, **kwargs: object
    ) -> tuple[ThreadEvent, ThreadEvent, JoinedThread]:
        captured["lease"] = kwargs["lease"]
        return ThreadEvent(), lease_lost, JoinedThread()

    class BlockingChild:
        pid = 42

        def __init__(self) -> None:
            self.returncode: int | None = None
            self.reclaimed = False

        def communicate(self, timeout: float | None = None) -> tuple[str, str]:
            if not self.reclaimed:
                self.reclaimed = True
                lease = captured["lease"]
                assert isinstance(lease, JobLease)
                current = datetime.now(UTC)
                with factory.begin() as session:
                    job = session.get(Job, lease.job_id)
                    assert job is not None
                    job.lease_expires_at = current - timedelta(seconds=1)
                with factory.begin() as session:
                    assert release_expired_leases(session, now=current) == 1
                    reclaimed = claim_next_job(
                        session,
                        owner=lease.owner,
                        lease_seconds=60,
                        now=current,
                    )
                    assert reclaimed is not None
                    assert reclaimed.attempt == lease.attempt + 1
                lease_lost.set()
                raise subprocess.TimeoutExpired("child", timeout)
            return "", ""

    child = BlockingChild()
    signals: list[tuple[int, signal.Signals]] = []

    def start_child(*_args: object, **_kwargs: object) -> BlockingChild:
        return child

    def kill_group(pid: int, signum: signal.Signals) -> None:
        signals.append((pid, signum))
        child.returncode = -int(signum)

    monkeypatch.setattr(finite_worker, "_start_lease_renewer", start_renewer)
    monkeypatch.setattr(finite_worker.subprocess, "Popen", start_child)
    monkeypatch.setattr(finite_worker.os, "killpg", kill_group)

    worked, _ = run_once(settings, owner="worker-a", factory=factory)

    assert worked is True
    assert signals == [(child.pid, signal.SIGTERM), (child.pid, signal.SIGKILL)]
    with factory() as session:
        job = session.get(Job, job_id)
        assert job is not None
        assert (job.state, job.lease_owner, job.attempt) == ("LEASED", "worker-a", 2)
        assert not list(
            session.scalars(
                select(DomainEvent).where(
                    DomainEvent.aggregate_id == job_id,
                    DomainEvent.kind.in_(("JOB_FAILED", "JOB_SUCCEEDED")),
                )
            )
        )
