from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import uuid4

from sqlalchemy.orm import sessionmaker

from db.models import Job
from jobs import claim_next_job, enqueue_job, release_expired_leases, renew_job_lease
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
        claim_next_job(session, owner="worker-a", lease_seconds=1, now=now)

    renewed_at = now + timedelta(seconds=2)
    with factory.begin() as session:
        assert renew_job_lease(
            session,
            job_id=job_id,
            owner="worker-a",
            lease_seconds=60,
            now=renewed_at,
        )

    with factory.begin() as session:
        assert not renew_job_lease(
            session,
            job_id=job_id,
            owner="worker-b",
            lease_seconds=60,
            now=renewed_at,
        )

    with factory() as session:
        job = session.get(Job, job_id)
        assert job is not None
        assert job.lease_owner == "worker-a"
        assert job.lease_expires_at == renewed_at + timedelta(seconds=60)
