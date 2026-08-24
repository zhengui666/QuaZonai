from __future__ import annotations

from sqlalchemy import inspect


def test_runtime_hot_path_indexes_are_declared(engine) -> None:  # type: ignore[no-untyped-def]
    inspector = inspect(engine)
    job_indexes = {str(item["name"]) for item in inspector.get_indexes("jobs")}
    event_indexes = {str(item["name"]) for item in inspector.get_indexes("events")}

    assert "ix_jobs_ready_queue" in job_indexes
    assert "ix_jobs_leased_expiry" in job_indexes
    assert "ix_events_aggregate_activity" in event_indexes
