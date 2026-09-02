from __future__ import annotations

from threading import Event, Thread
from uuid import uuid4

import pytest
from sqlalchemy.exc import SQLAlchemyError

from codex_poll_guard import hold_device_poll_execution
from db.session import create_session_factory


def test_postgres_advisory_guard_serializes_same_device_login(engine) -> None:  # type: ignore[no-untyped-def]
    if engine.dialect.name != "postgresql":
        pytest.skip("PostgreSQL advisory-lock integration test")

    factory = create_session_factory(engine)
    login_id = uuid4()
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
