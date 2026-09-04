from __future__ import annotations

from datetime import UTC, datetime, timedelta
from threading import Event, Thread
from uuid import uuid4

import pytest
from sqlalchemy import create_engine, text
from sqlalchemy.exc import SQLAlchemyError

from codex_poll_guard import hold_device_poll_execution
from db.codex_auth_models import CodexChatgptLoginAttempt, LOGIN_PENDING
from db.session import create_session_factory


def _seed_attempt(factory, login_id) -> None:  # type: ignore[no-untyped-def]
    now = datetime.now(UTC)
    with factory.begin() as session:
        session.add(
            CodexChatgptLoginAttempt(
                id=login_id,
                state=LOGIN_PENDING,
                verification_url="https://auth.openai.com/codex/device",
                poll_interval_seconds=5,
                expires_at=now + timedelta(minutes=10),
                next_poll_at=now,
            )
        )


def test_postgres_row_guard_serializes_same_device_login(engine) -> None:  # type: ignore[no-untyped-def]
    if engine.dialect.name != "postgresql":
        pytest.skip("PostgreSQL row-lock integration test")

    factory = create_session_factory(engine)
    login_id = uuid4()
    _seed_attempt(factory, login_id)

    attempting = Event()
    entered = Event()
    errors: list[SQLAlchemyError] = []

    def contender() -> None:
        try:
            with factory() as session:
                attempting.set()
                with hold_device_poll_execution(session, login_id):
                    entered.set()
        except SQLAlchemyError as exc:  # pragma: no cover - surfaced below
            errors.append(exc)
            entered.set()

    with factory() as owner_session:
        with hold_device_poll_execution(owner_session, login_id):
            thread = Thread(target=contender, daemon=True)
            thread.start()
            assert attempting.wait(timeout=2)
            assert not entered.wait(timeout=0.25)

    assert entered.wait(timeout=2)
    thread.join(timeout=2)
    assert not thread.is_alive()
    assert errors == []


def test_postgres_same_login_waiters_do_not_exhaust_owner_pool(engine) -> None:  # type: ignore[no-untyped-def]
    """Queued same-login polls must wait before checking out pooled connections."""
    if engine.dialect.name != "postgresql":
        pytest.skip("PostgreSQL pool-saturation integration test")

    saturated_engine = create_engine(
        engine.url,
        pool_pre_ping=True,
        pool_size=2,
        max_overflow=0,
        pool_timeout=0.5,
    )
    factory = create_session_factory(saturated_engine)
    login_id = uuid4()
    _seed_attempt(factory, login_id)

    started = [Event() for _ in range(8)]
    any_entered = Event()
    errors: list[SQLAlchemyError] = []

    def contender(index: int) -> None:
        try:
            with factory() as session:
                started[index].set()
                with hold_device_poll_execution(session, login_id):
                    any_entered.set()
        except SQLAlchemyError as exc:  # pragma: no cover - surfaced below
            errors.append(exc)
            any_entered.set()

    threads: list[Thread] = []
    try:
        with factory() as owner_session:
            with hold_device_poll_execution(owner_session, login_id):
                for index in range(len(started)):
                    thread = Thread(target=contender, args=(index,), daemon=True)
                    threads.append(thread)
                    thread.start()
                assert all(event.wait(timeout=2) for event in started)
                assert not any_entered.wait(timeout=0.2)

                # The guard owns one of only two pool connections. Even with
                # eight queued polls, the owner can still check out the second
                # connection needed by poll_device_login for attempt updates,
                # credential installation, and lease cleanup.
                assert owner_session.execute(text("SELECT 1")).scalar_one() == 1

        for thread in threads:
            thread.join(timeout=3)
        assert all(not thread.is_alive() for thread in threads)
        assert errors == []
    finally:
        saturated_engine.dispose()
