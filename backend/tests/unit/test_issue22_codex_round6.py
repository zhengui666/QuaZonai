from __future__ import annotations

# Regression coverage for the final Issue 22 Codex round6 findings.
from datetime import UTC, datetime
from types import SimpleNamespace
from uuid import UUID, uuid4

from candidate_bundles import _member_payload, _strategy_wheel_filename
from jobs import retry_job


def test_strategy_wheel_version_uses_collision_free_candidate_identity() -> None:
    first = UUID(int=1)
    second = UUID(int=1_000_001)
    assert _strategy_wheel_filename(first) != _strategy_wheel_filename(second)
    assert str(first.int) in _strategy_wheel_filename(first)
    assert str(second.int) in _strategy_wheel_filename(second)


def test_target_portfolio_rows_freeze_canonical_identity_and_validity() -> None:
    candidate_id = uuid4()
    universe_version_id = uuid4()
    alpha_id = uuid4()
    as_of = datetime(2026, 8, 28, 12, tzinfo=UTC)
    effective = datetime(2026, 8, 28, 13, tzinfo=UTC)
    expires = datetime(2026, 9, 4, 13, tzinfo=UTC)
    candidate = SimpleNamespace(
        id=candidate_id,
        created_at=as_of,
        state="READY",
        metrics={"search_adjusted_quality": 0.81},
        members=[
            {
                "alpha_qualification_id": str(alpha_id),
                "universe_version_id": str(universe_version_id),
                "instrument_id": "EUR/USD.SIM",
                "target_weight": 1.0,
            }
        ],
    )
    approval = SimpleNamespace(updated_at=effective, created_at=effective, valid_until=expires)
    row = _member_payload(candidate, approval=approval, runtime={})[0]
    assert row["as_of_time"] == as_of.isoformat()
    assert row["effective_from"] == effective.isoformat()
    assert row["effective_until"] == expires.isoformat()
    assert row["universe_version_id"] == str(universe_version_id)
    assert row["confidence"] == 0.81
    assert row["portfolio_state"] == "READY"
    assert row["portfolio_candidate_id"] == str(candidate_id)


def test_retry_job_releases_lease_without_terminal_failure() -> None:
    job = SimpleNamespace(
        state="LEASED",
        lease_owner="worker",
        lease_expires_at=datetime.now(UTC),
        last_error=None,
        available_at=datetime.now(UTC),
    )

    class Session:
        def flush(self) -> None:
            return None

    retry_job(Session(), job, "uncertain remote result", delay_seconds=3)
    assert job.state == "READY"
    assert job.lease_owner is None
    assert job.lease_expires_at is None
    assert job.last_error == "uncertain remote result"
