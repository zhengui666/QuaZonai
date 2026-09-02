from __future__ import annotations

from datetime import UTC, datetime, timedelta
from threading import Event, Thread
from uuid import uuid4

import pytest
from sqlalchemy.exc import SQLAlchemyError

from codex_poll_guard import hold_device_poll_execution
from db.codex_auth_models import CodexChatgptLoginAttempt, LOGIN_PENDING
from db.session import create_session_factory


def test_postgres_row_guard_serializes_same_device_login(engine) -> None:  # type: ignore[no-untyped-def]
    if engine.dialect.name != "postgresql":
        pytest.skip("PostgreSQL row-lock integration test")

    factory = create_session_factory(engine)
    login_id = uuid4()
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
