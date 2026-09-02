from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import uuid4

from api import codex_polling
from codex_chatgpt_auth import DeviceLoginPollResult
from db.codex_auth_models import (
    CodexChatgptLoginAttempt,
    LOGIN_CANCELLED,
    LOGIN_FAILED,
    LOGIN_PENDING,
)
from db.session import create_session_factory
from errors import QfError


def _pending_attempt(login_id, now):  # type: ignore[no-untyped-def]
    return CodexChatgptLoginAttempt(
        id=login_id,
        state=LOGIN_PENDING,
        verification_url="https://auth.openai.com/codex/device",
        poll_interval_seconds=5,
        expires_at=now + timedelta(minutes=10),
        next_poll_at=now,
        device_auth_id_ciphertext=b"ciphertext",
        device_auth_id_nonce=b"nonce-nonce12",
        device_auth_id_key_version=1,
        user_code="ABCD-EFGH",
        poll_lease_until=now + timedelta(seconds=65),
    )


def test_late_pending_result_returns_committed_cancelled_state(
    engine,
    settings,
    monkeypatch,
) -> None:  # type: ignore[no-untyped-def]
    factory = create_session_factory(engine)
    login_id = uuid4()
    now = datetime.now(UTC)
    with factory.begin() as session:
        session.add(_pending_attempt(login_id, now))

    def stale_poll(session, configured_settings, configured_login_id):  # type: ignore[no-untyped-def]
        assert configured_login_id == login_id
        with factory.begin() as cancelling:
            attempt = cancelling.get(CodexChatgptLoginAttempt, login_id, with_for_update=True)
            assert attempt is not None
            attempt.state = LOGIN_CANCELLED
            attempt.error_code = None
            attempt.device_auth_id_ciphertext = None
            attempt.device_auth_id_nonce = None
            attempt.device_auth_id_key_version = None
            attempt.user_code = None
            attempt.poll_lease_until = None
        return DeviceLoginPollResult(
            status=LOGIN_PENDING,
            expires_at=now + timedelta(minutes=10),
            poll_after_seconds=5,
        )

    monkeypatch.setattr(codex_polling, "_poll_device_login", stale_poll)
    with factory() as session:
        result = codex_polling.poll_device_login(session, settings, login_id)

    assert result.status == LOGIN_CANCELLED
    assert result.poll_after_seconds is None


def test_unreadable_pending_secret_is_terminalized(
    engine,
    settings,
    monkeypatch,
) -> None:  # type: ignore[no-untyped-def]
    factory = create_session_factory(engine)
    login_id = uuid4()
    now = datetime.now(UTC)
    with factory.begin() as session:
        session.add(_pending_attempt(login_id, now))

    def unreadable_poll(session, configured_settings, configured_login_id):  # type: ignore[no-untyped-def]
        raise QfError(
            "CREDENTIAL_INVALID",
            "Credential secret could not be authenticated for its binding context.",
            422,
        )

    monkeypatch.setattr(codex_polling, "_poll_device_login", unreadable_poll)
    with factory() as session:
        result = codex_polling.poll_device_login(session, settings, login_id)
        attempt = session.get(CodexChatgptLoginAttempt, login_id)

    assert result.status == LOGIN_FAILED
    assert result.error_code == "credential_unreadable"
    assert attempt is not None
    assert attempt.state == LOGIN_FAILED
    assert attempt.error_code == "credential_unreadable"
    assert attempt.device_auth_id_ciphertext is None
    assert attempt.device_auth_id_nonce is None
    assert attempt.device_auth_id_key_version is None
    assert attempt.user_code is None
    assert attempt.poll_lease_until is None
