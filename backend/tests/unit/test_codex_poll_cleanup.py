from __future__ import annotations

from datetime import UTC, datetime, timedelta

import httpx

import codex_chatgpt_auth as codex_auth_service
from codex_chatgpt_auth import DEVICE_AUTH_USERCODE_URL, poll_device_login, start_device_login
from db.codex_auth_models import CodexChatgptLoginAttempt, LOGIN_PENDING
from db.session import create_session_factory


class _PendingOwnedClient:
    def __init__(self) -> None:
        self.closed = False

    def post(self, url: str, **kwargs: object) -> httpx.Response:
        del kwargs
        return httpx.Response(400, json={"error": "authorization_pending"})

    def close(self) -> None:
        self.closed = True


def test_authorization_pending_releases_poll_lease_and_closes_owned_client(
    engine,
    settings,
    monkeypatch,
) -> None:  # type: ignore[no-untyped-def]
    now = datetime(2026, 9, 2, 1, 0, tzinfo=UTC)
    factory = create_session_factory(engine)
    start_client = httpx.Client(
        transport=httpx.MockTransport(
            lambda request: httpx.Response(
                200,
                json={
                    "device_auth_id": "device-secret",
                    "user_code": "ABCD-EFGH",
                    "interval": 1,
                    "expires_in": 600,
                },
            )
            if str(request.url) == DEVICE_AUTH_USERCODE_URL
            else httpx.Response(500)
        )
    )

    with factory() as session:
        view = start_device_login(session, settings, start_client, now=lambda: now)
        session.commit()
        attempt = session.get(CodexChatgptLoginAttempt, view.login_id)
        assert attempt is not None
        attempt.next_poll_at = now - timedelta(seconds=1)
        session.commit()

    owned_client = _PendingOwnedClient()
    monkeypatch.setattr(codex_auth_service.httpx, "Client", lambda *args, **kwargs: owned_client)

    with factory() as session:
        result = poll_device_login(session, settings, view.login_id, now=lambda: now)
        attempt = session.get(CodexChatgptLoginAttempt, view.login_id)
        assert result.status == LOGIN_PENDING
        assert attempt is not None
        assert attempt.poll_lease_until is None

    assert owned_client.closed is True
