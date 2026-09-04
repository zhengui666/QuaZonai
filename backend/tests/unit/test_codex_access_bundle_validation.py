from __future__ import annotations

import base64
import json
from datetime import UTC, datetime

import pytest

from codex_chatgpt_auth import _install_bundle, get_auth_configuration, get_valid_access_bundle
from db.session import create_session_factory
from errors import QfError


def _jwt(claims: dict[str, object]) -> str:
    def encode(value: object) -> str:
        return base64.urlsafe_b64encode(json.dumps(value).encode()).decode().rstrip("=")

    return f"{encode({'alg': 'none'})}.{encode(claims)}.signature"


class _NoUpstreamClient:
    def __init__(self) -> None:
        self.calls = 0

    def post(self, *args: object, **kwargs: object) -> None:
        del args, kwargs
        self.calls += 1
        raise AssertionError("a valid cached access token must not trigger an upstream refresh")


def test_cached_bundle_rejects_unreadable_refresh_token_before_mission_admission(
    engine,
    settings,
) -> None:  # type: ignore[no-untyped-def]
    now = datetime(2026, 9, 2, 1, 0, tzinfo=UTC)
    access_token = _jwt({"chatgpt_account_id": "acct-1"})
    factory = create_session_factory(engine)

    with factory() as session:
        _install_bundle(
            session,
            settings,
            access_token=access_token,
            refresh_token="refresh-secret",
            id_token=None,
            response={"access_token": access_token, "expires_in": 3600},
            authenticated_at=now,
        )
        session.commit()
        auth = get_auth_configuration(session)
        assert auth is not None
        auth.refresh_token_ciphertext = b"corrupt"
        session.commit()

    client = _NoUpstreamClient()
    with factory() as session:
        with pytest.raises(QfError):
            get_valid_access_bundle(session, settings, client, now=lambda: now)

    assert client.calls == 0
