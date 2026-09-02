"""HTTP-facing reconciliation for ChatGPT device-code polling."""

from __future__ import annotations

from uuid import UUID

from sqlalchemy.orm import Session

from codex_chatgpt_auth import DeviceLoginPollResult, poll_device_login as _poll_device_login
from db.codex_auth_models import CodexChatgptLoginAttempt, LOGIN_FAILED, LOGIN_PENDING
from errors import QfError
from settings import Settings

_UNREADABLE_PENDING_CODES = frozenset({"CREDENTIAL_INVALID", "CODEX_CHATGPT_AUTH_CORRUPT"})


def _terminal_result(attempt: CodexChatgptLoginAttempt) -> DeviceLoginPollResult:
    return DeviceLoginPollResult(status=attempt.state, error_code=attempt.error_code)


def _terminalize_unreadable_attempt(
    session: Session,
    login_id: UUID,
) -> DeviceLoginPollResult:
    attempt = session.get(CodexChatgptLoginAttempt, login_id, with_for_update=True)
    if attempt is None:
        session.rollback()
        raise QfError(
            "CODEX_CHATGPT_LOGIN_NOT_FOUND",
            "ChatGPT login attempt was not found.",
            404,
        )
    if attempt.state != LOGIN_PENDING:
        result = _terminal_result(attempt)
        session.commit()
        return result

    attempt.state = LOGIN_FAILED
    attempt.error_code = "credential_unreadable"
    attempt.device_auth_id_ciphertext = None
    attempt.device_auth_id_nonce = None
    attempt.device_auth_id_key_version = None
    attempt.user_code = None
    attempt.poll_lease_until = None
    session.commit()
    return _terminal_result(attempt)


def _reconcile_committed_state(
    session: Session,
    login_id: UUID,
    result: DeviceLoginPollResult,
) -> DeviceLoginPollResult:
    """Prefer a terminal state committed while the upstream request was in flight."""
    session.expire_all()
    attempt = session.get(CodexChatgptLoginAttempt, login_id)
    if attempt is not None and attempt.state != LOGIN_PENDING:
        return _terminal_result(attempt)
    return result


def poll_device_login(
    session: Session,
    settings: Settings,
    login_id: UUID,
) -> DeviceLoginPollResult:
    """Poll once and reconcile races against cancellation/disconnect.

    The underlying service owns OAuth and credential installation. This wrapper
    owns the HTTP contract: a committed terminal database state always wins over
    a stale upstream-derived result, and unreadable pending secrets are made
    terminal so a subsequent login start cannot keep reusing the broken row.
    """
    try:
        result = _poll_device_login(session, settings, login_id)
    except QfError as exc:
        if exc.code not in _UNREADABLE_PENDING_CODES:
            raise
        return _terminalize_unreadable_attempt(session, login_id)
    return _reconcile_committed_state(session, login_id, result)


__all__ = ["poll_device_login"]
