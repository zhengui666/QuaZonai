from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import uuid4

from sqlalchemy.orm import sessionmaker

from quantfoundry.db.models import Job
from quantfoundry.jobs import claim_next_job, enqueue_job, release_expired_leases


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
