from __future__ import annotations

from threading import Event, Thread
from uuid import uuid4

from codex_poll_guard import hold_device_poll_execution
from db.session import create_session_factory


def test_same_login_poll_guard_outlives_any_lease_ttl(engine) -> None:  # type: ignore[no-untyped-def]
    """A second poll cannot enter while the first owns the full execution guard."""
    factory = create_session_factory(engine)
    login_id = uuid4()
    attempting = Event()
    entered = Event()
    errors: list[BaseException] = []

    def contender() -> None:
        try:
            with factory() as session:
                attempting.set()
                with hold_device_poll_execution(session, login_id):
                    entered.set()
        except BaseException as exc:  # pragma: no cover - surfaced below
            errors.append(exc)
            entered.set()

    with factory() as owner_session:
        with hold_device_poll_execution(owner_session, login_id):
            thread = Thread(target=contender, daemon=True)
            thread.start()
            assert attempting.wait(timeout=2)
            # This wait is deliberately longer than a scheduler handoff but
            # unrelated to poll_lease_until: ownership is held by the guard,
            # not by a fixed lease duration.
            assert not entered.wait(timeout=0.2)

    assert entered.wait(timeout=2)
    thread.join(timeout=2)
    assert not thread.is_alive()
    assert errors == []


def test_different_login_ids_do_not_share_poll_guard(engine) -> None:  # type: ignore[no-untyped-def]
    factory = create_session_factory(engine)
    first_login = uuid4()
    second_login = uuid4()

    with factory() as first_session, factory() as second_session:
        with hold_device_poll_execution(first_session, first_login):
            with hold_device_poll_execution(second_session, second_login):
                pass
