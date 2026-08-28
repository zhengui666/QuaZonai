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



def test_job_lease_renewal_requires_current_owner(engine) -> None:
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        created = enqueue_job(
            session,
            kind="SEALED_ALPHA_QUALIFICATION",
            resource_type="SEARCH_LEDGER_ENTRY",
            resource_id=uuid4(),
        )
        job_id = created.id
    now = datetime.now(UTC)
    with factory.begin() as session:
        claimed = claim_next_job(session, owner="sealed-a", lease_seconds=10, now=now)
        assert claimed is not None
    with factory.begin() as session:
        assert renew_job_lease(
            session,
            job_id=job_id,
            owner="sealed-a",
            lease_seconds=60,
            now=now + timedelta(seconds=5),
        ) is True
        assert renew_job_lease(
            session,
            job_id=job_id,
            owner="sealed-b",
            lease_seconds=60,
            now=now + timedelta(seconds=5),
        ) is False
    with factory() as session:
        job = session.get(Job, job_id)
        assert job is not None
        assert job.lease_owner == "sealed-a"
        assert job.lease_expires_at is not None
        assert job.lease_expires_at >= now + timedelta(seconds=65)


def test_claim_next_job_respects_explicit_worker_capabilities(
    engine,  # type: ignore[no-untyped-def]
) -> None:
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    earlier = datetime.now(UTC) - timedelta(minutes=1)
    with factory.begin() as session:
        privileged = enqueue_job(
            session,
            kind="SEALED_ALPHA_QUALIFICATION",
            resource_type="SEARCH_LEDGER_ENTRY",
            resource_id=uuid4(),
            available_at=earlier,
        )
        ordinary = enqueue_job(
            session,
            kind="SYSTEM_NOOP",
            resource_type="system",
            resource_id=uuid4(),
        )
        privileged_id = privileged.id
        ordinary_id = ordinary.id

    with factory.begin() as session:
        claimed = claim_next_job(
            session,
            owner="ordinary-worker",
            lease_seconds=60,
            kinds={"SYSTEM_NOOP"},
        )
        assert claimed is not None
        assert claimed.id == ordinary_id

    with factory() as session:
        privileged = session.get(Job, privileged_id)
        assert privileged is not None
        assert privileged.state == "READY"
        assert privileged.lease_owner is None


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


def test_finite_worker_never_claims_sealed_evaluator_work(
    engine,  # type: ignore[no-untyped-def]
    settings,  # type: ignore[no-untyped-def]
) -> None:
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    earlier = datetime.now(UTC) - timedelta(minutes=1)
    with factory.begin() as session:
        sealed = enqueue_job(
            session,
            kind="SEALED_ALPHA_QUALIFICATION",
            resource_type="SEARCH_LEDGER_ENTRY",
            resource_id=uuid4(),
            payload={"sealed_dataset_revision_id": str(uuid4())},
            available_at=earlier,
        )
        finite = enqueue_job(
            session,
            kind="SYSTEM_NOOP",
            resource_type="system",
            resource_id=uuid4(),
        )
        sealed_id = sealed.id
        finite_id = finite.id

    worked, _ = run_once(settings, owner="finite-worker", factory=factory)

    assert worked is True
    with factory() as session:
        sealed = session.get(Job, sealed_id)
        finite = session.get(Job, finite_id)
        assert sealed is not None and sealed.state == "READY"
        assert finite is not None and finite.state == "SUCCEEDED"
