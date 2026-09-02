from __future__ import annotations

import base64
import json
from datetime import UTC, datetime, timedelta

import httpx

from codex_chatgpt_auth import (
    DEVICE_AUTH_TOKEN_URL,
    DEVICE_AUTH_USERCODE_URL,
    OAUTH_TOKEN_URL,
    get_auth_configuration,
    get_valid_access_bundle,
    poll_device_login,
    start_device_login,
)
from db.codex_auth_models import CodexChatgptLoginAttempt, LOGIN_CANCELLED, LOGIN_FAILED
from db.session import create_session_factory


def _jwt(claims: dict[str, object]) -> str:
    def encode(value: object) -> str:
        return base64.urlsafe_b64encode(json.dumps(value).encode()).decode().rstrip("=")

    return f"{encode({'alg': 'none'})}.{encode(claims)}.signature"


def test_device_code_is_persisted_encrypted_and_refresh_rotates_atomically(engine, settings) -> None:
    now = datetime(2026, 9, 2, 1, 0, tzinfo=UTC)
    calls: list[tuple[str, object]] = []

    def handler(request: httpx.Request) -> httpx.Response:
        calls.append((str(request.url), request.content))
        if str(request.url) == DEVICE_AUTH_USERCODE_URL:
            return httpx.Response(200, json={"device_auth_id": "device-secret", "user_code": "ABCD-EFGH", "interval": "1", "expires_in": 600})
        if str(request.url) == DEVICE_AUTH_TOKEN_URL:
            return httpx.Response(200, json={"authorization_code": "authorization-secret", "code_verifier": "verifier-secret"})
        assert str(request.url) == OAUTH_TOKEN_URL
        form = dict(httpx.QueryParams(request.content.decode()))
        if form.get("grant_type") == "authorization_code":
            return httpx.Response(200, json={
                "access_token": _jwt({"chatgpt_account_id": "acct-1", "email": "a@example.com"}),
                "refresh_token": "refresh-secret-1",
                "expires_in": 3600,
            })
        assert form["refresh_token"] == "refresh-secret-1"
        return httpx.Response(200, json={
            "access_token": _jwt({"chatgpt_account_id": "acct-1", "email": "a@example.com"}),
            "refresh_token": "refresh-secret-2",
            "expires_in": 3600,
        })

    client = httpx.Client(transport=httpx.MockTransport(handler))
    factory = create_session_factory(engine)
    with factory() as session:
        view = start_device_login(session, settings, client, now=lambda: now)
        session.commit()
        assert view.user_code == "ABCD-EFGH"
        attempt = session.get(CodexChatgptLoginAttempt, view.login_id)
        assert attempt is not None
        assert attempt.device_auth_id_ciphertext != b"device-secret"
        assert attempt.user_code == "ABCD-EFGH"
        attempt.next_poll_at = now - timedelta(seconds=1)
        session.commit()
        result = poll_device_login(session, settings, view.login_id, client, now=lambda: now)
        assert result.status == "SUCCEEDED"
        auth = get_auth_configuration(session)
        assert auth is not None
        assert auth.chatgpt_account_id == "acct-1"
        assert auth.access_token_ciphertext is not None
        assert b"refresh-secret-1" not in auth.refresh_token_ciphertext
        session.commit()

    with factory() as session:
        bundle = get_valid_access_bundle(session, settings, client, force_refresh=True, now=lambda: now)
        assert bundle.chatgpt_account_id == "acct-1"
        assert bundle.token_generation == 2
        assert bundle.access_token.startswith("ey")
        auth = get_auth_configuration(session)
        assert auth is not None
        assert auth.refresh_token_ciphertext is not None

    assert [url for url, _ in calls] == [DEVICE_AUTH_USERCODE_URL, DEVICE_AUTH_TOKEN_URL, OAUTH_TOKEN_URL, OAUTH_TOKEN_URL]
    assert calls[0][1] == b'{"client_id":"app_EMoamEEZ73f0CkXaXp7hrann"}'


def test_late_device_poll_cannot_restore_a_cancelled_attempt(engine, settings) -> None:
    now = datetime(2026, 9, 2, 1, 0, tzinfo=UTC)
    factory = create_session_factory(engine)
    start_client = httpx.Client(transport=httpx.MockTransport(lambda request: httpx.Response(200, json={
        "device_auth_id": "device-secret",
        "user_code": "ABCD-EFGH",
        "interval": 1,
        "expires_in": 600,
    })))
    with factory() as session:
        view = start_device_login(session, settings, start_client, now=lambda: now)
        session.commit()
        attempt = session.get(CodexChatgptLoginAttempt, view.login_id)
        assert attempt is not None
        attempt.next_poll_at = now - timedelta(seconds=1)
        session.commit()

    def late_handler(request: httpx.Request) -> httpx.Response:
        with factory.begin() as cancelling:
            late_attempt = cancelling.get(CodexChatgptLoginAttempt, view.login_id, with_for_update=True)
            assert late_attempt is not None
            late_attempt.state = LOGIN_CANCELLED
            late_attempt.device_auth_id_ciphertext = None
            late_attempt.device_auth_id_nonce = None
            late_attempt.device_auth_id_key_version = None
            late_attempt.user_code = None
        if str(request.url) == DEVICE_AUTH_TOKEN_URL:
            return httpx.Response(200, json={"authorization_code": "authorization-secret", "code_verifier": "verifier-secret"})
        return httpx.Response(200, json={"access_token": _jwt({"chatgpt_account_id": "acct-1"}), "refresh_token": "refresh-secret", "expires_in": 3600})

    client = httpx.Client(transport=httpx.MockTransport(late_handler))
    with factory() as session:
        result = poll_device_login(session, settings, view.login_id, client, now=lambda: now)
        assert result.status == LOGIN_CANCELLED
        assert get_auth_configuration(session) is None


def test_device_poll_without_account_identity_is_terminal_failure(engine, settings) -> None:
    now = datetime(2026, 9, 2, 1, 0, tzinfo=UTC)
    factory = create_session_factory(engine)
    start_client = httpx.Client(transport=httpx.MockTransport(lambda request: httpx.Response(200, json={
        "device_auth_id": "device-secret",
        "user_code": "ABCD-EFGH",
        "interval": 1,
        "expires_in": 600,
    })))
    with factory() as session:
        view = start_device_login(session, settings, start_client, now=lambda: now)
        session.commit()
        attempt = session.get(CodexChatgptLoginAttempt, view.login_id)
        assert attempt is not None
        attempt.next_poll_at = now - timedelta(seconds=1)
        session.commit()

    def incomplete_handler(request: httpx.Request) -> httpx.Response:
        if str(request.url) == DEVICE_AUTH_TOKEN_URL:
            return httpx.Response(200, json={"authorization_code": "authorization-secret", "code_verifier": "verifier-secret"})
        return httpx.Response(200, json={"access_token": "access-secret", "refresh_token": "refresh-secret", "expires_in": 3600})

    client = httpx.Client(transport=httpx.MockTransport(incomplete_handler))
    with factory() as session:
        result = poll_device_login(session, settings, view.login_id, client, now=lambda: now)
        assert result.status == LOGIN_FAILED
        assert result.error_code == "credential_install_failed"
        attempt = session.get(CodexChatgptLoginAttempt, view.login_id)
        assert attempt is not None
        assert attempt.state == LOGIN_FAILED
        assert attempt.user_code is None
        assert get_auth_configuration(session) is None
