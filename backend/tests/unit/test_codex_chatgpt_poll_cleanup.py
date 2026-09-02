from __future__ import annotations

from datetime import UTC, datetime, timedelta
from typing import Any

import httpx

import codex_chatgpt_auth as codex_auth_service
from codex_chatgpt_auth import poll_device_login, start_device_login
from db.codex_auth_models import CodexChatgptLoginAttempt
from db.session import create_session_factory


def test_authorization_pending_releases_poll_lease_and_closes_owned_client(
    engine: Any,
    settings: Any,
    monkeypatch: Any,
) -> None:
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
        )
    )
    with factory() as session:
        view = start_device_login(session, settings, start_client, now=lambda: now)
        session.commit()
        attempt = session.get(CodexChatgptLoginAttempt, view.login_id)
        assert attempt is not None
        attempt.next_poll_at = now - timedelta(seconds=1)
        session.commit()
    start_client.close()

    class OwnedPendingClient:
        closed = False

        def post(
            self,
            url: str,
            *,
            json: object | None = None,
            data: object | None = None,
            timeout: float | None = None,
        ) -> httpx.Response:
            return httpx.Response(400, json={"error": "authorization_pending"})

        def close(self) -> None:
            self.closed = True

    owned_client = OwnedPendingClient()
    monkeypatch.setattr(
        codex_auth_service,
        "_http_client",
        lambda supplied: (owned_client, True),
    )

    with factory() as session:
        result = poll_device_login(session, settings, view.login_id, now=lambda: now)
        session.expire_all()
        attempt = session.get(CodexChatgptLoginAttempt, view.login_id)

    assert result.status == "PENDING"
    assert owned_client.closed is True
    assert attempt is not None
    assert attempt.poll_lease_until is None
