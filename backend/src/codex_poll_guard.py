"""Cross-process execution ownership for ChatGPT device-code polling."""

from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass, field
from threading import Lock
from typing import cast
from uuid import UUID

from sqlalchemy import text
from sqlalchemy.engine import Engine
from sqlalchemy.orm import Session

_LOCAL_GUARD_LOCK = Lock()


@dataclass(slots=True)
class _LocalPollLock:
    lock: Lock = field(default_factory=Lock)
    users: int = 0


_LOCAL_POLL_LOCKS: dict[UUID, _LocalPollLock] = {}


def _acquire_local_lock(login_id: UUID) -> _LocalPollLock:
    """Acquire the non-PostgreSQL fallback without dropping queued waiters."""
    with _LOCAL_GUARD_LOCK:
        entry = _LOCAL_POLL_LOCKS.get(login_id)
        if entry is None:
            entry = _LocalPollLock()
            _LOCAL_POLL_LOCKS[login_id] = entry
        entry.users += 1
    entry.lock.acquire()
    return entry


def _release_local_lock(login_id: UUID, entry: _LocalPollLock) -> None:
    entry.lock.release()
    with _LOCAL_GUARD_LOCK:
        entry.users -= 1
        if entry.users == 0 and _LOCAL_POLL_LOCKS.get(login_id) is entry:
            _LOCAL_POLL_LOCKS.pop(login_id, None)


@contextmanager
def hold_device_poll_execution(session: Session, login_id: UUID) -> Iterator[None]:
    """Serialize one login's upstream poll/exchange for the full HTTP operation.

    PostgreSQL uses a dedicated transaction that locks a row whose primary key
    is the complete login UUID. The row lock is independent of the durable
    poll-lease TTL and is held until the entire upstream poll/exchange/install
    call returns. A lost database connection rolls the transaction back and
    releases the lock. SQLite/non-PostgreSQL test runtimes use a process-local
    keyed lock with waiter reference counting.
    """
    bind = session.get_bind()
    engine = cast(Engine, getattr(bind, "engine", bind))
    if engine.dialect.name != "postgresql":
        entry = _acquire_local_lock(login_id)
        try:
            yield
        finally:
            _release_local_lock(login_id, entry)
        return

    with engine.connect() as connection:
        with connection.begin():
            # Create the exact-UUID lock row only for a real durable login
            # attempt. Concurrent inserts for the same UUID serialize on the
            # primary-key constraint; existing rows are then locked below.
            connection.execute(
                text(
                    "INSERT INTO codex_chatgpt_poll_locks (login_id) "
                    "SELECT id FROM codex_chatgpt_login_attempts WHERE id = :login_id "
                    "ON CONFLICT (login_id) DO NOTHING"
                ),
                {"login_id": login_id},
            )
            locked_login_id = connection.execute(
                text(
                    "SELECT login_id FROM codex_chatgpt_poll_locks "
                    "WHERE login_id = :login_id FOR UPDATE"
                ),
                {"login_id": login_id},
            ).scalar_one_or_none()
            if locked_login_id is None:
                # Unknown IDs never reach an upstream OAuth operation, so no
                # durable guard row is created merely by probing the endpoint.
                yield
                return
            yield


__all__ = ["hold_device_poll_execution"]
