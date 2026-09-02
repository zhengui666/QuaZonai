"""Cross-process execution ownership for ChatGPT device-code polling."""

from __future__ import annotations

import hashlib
from collections.abc import Iterator
from contextlib import contextmanager
from threading import Lock
from uuid import UUID

from sqlalchemy import text
from sqlalchemy.exc import SQLAlchemyError
from sqlalchemy.orm import Session

_LOCAL_GUARD_LOCK = Lock()
_LOCAL_POLL_LOCKS: dict[UUID, Lock] = {}


def _postgres_advisory_key(login_id: UUID) -> int:
    """Map the full UUID to a stable signed 64-bit PostgreSQL advisory key."""
    digest = hashlib.blake2b(login_id.bytes, digest_size=8, person=b"qz-poll").digest()
    return int.from_bytes(digest, byteorder="big", signed=True)


def _local_lock(login_id: UUID) -> Lock:
    with _LOCAL_GUARD_LOCK:
        lock = _LOCAL_POLL_LOCKS.get(login_id)
        if lock is None:
            lock = Lock()
            _LOCAL_POLL_LOCKS[login_id] = lock
        return lock


def _release_local_lock(login_id: UUID, lock: Lock) -> None:
    lock.release()
    with _LOCAL_GUARD_LOCK:
        if _LOCAL_POLL_LOCKS.get(login_id) is lock and not lock.locked():
            _LOCAL_POLL_LOCKS.pop(login_id, None)


@contextmanager
def hold_device_poll_execution(session: Session, login_id: UUID) -> Iterator[None]:
    """Serialize one login's upstream poll/exchange for the full HTTP operation.

    PostgreSQL uses a session-level advisory lock on a dedicated autocommit
    connection. The lock is independent of the durable poll-lease TTL and is
    held until the entire upstream poll/exchange/install call returns. If the
    worker process dies, PostgreSQL releases the advisory lock with the lost
    connection. SQLite/non-PostgreSQL test runtimes use a process-local lock.
    """
    bind = session.get_bind()
    engine = getattr(bind, "engine", bind)
    if engine.dialect.name != "postgresql":
        lock = _local_lock(login_id)
        lock.acquire()
        try:
            yield
        finally:
            _release_local_lock(login_id, lock)
        return

    key = _postgres_advisory_key(login_id)
    connection = engine.connect().execution_options(isolation_level="AUTOCOMMIT")
    locked = False
    try:
        connection.execute(text("SELECT pg_advisory_lock(:key)"), {"key": key})
        locked = True
        yield
    finally:
        if locked:
            try:
                connection.execute(text("SELECT pg_advisory_unlock(:key)"), {"key": key})
            except SQLAlchemyError:
                # A lost PostgreSQL connection has already released its
                # session-level locks. Invalidate it so a locked/broken
                # physical connection can never be returned to the pool.
                connection.invalidate()
        connection.close()


__all__ = ["hold_device_poll_execution"]
