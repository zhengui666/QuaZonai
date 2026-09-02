from __future__ import annotations

import base64
import json
from datetime import UTC, datetime, timedelta

import httpx
import pytest
from sqlalchemy import select
from uuid import uuid4

from codex_chatgpt_auth import (
    DEVICE_AUTH_TOKEN_URL,
    DEVICE_AUTH_USERCODE_URL,
    OAUTH_TOKEN_URL,
    _install_bundle,
    auth_status,
    cancel_device_login,
    codex_auth_readiness,
    disconnect_chatgpt,
    get_auth_configuration,
    get_valid_access_bundle,
    initialize_codex_auth,
    poll_device_login,
    start_device_login,
)
from db.codex_auth_models import (
    CHATGPT_AUTH_REAUTH_REQUIRED,
    CodexChatgptLoginAttempt,
    LOGIN_CANCELLED,
    LOGIN_FAILED,
)
from db.models import Event, RuntimeConfiguration
from db.session import create_session_factory
from errors import QfError
from runtime_config import _api_key_aad
from crypto import encrypt_bound_secret


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
        assert view.created is True
        attempt = session.get(CodexChatgptLoginAttempt, view.login_id)
        assert attempt is not None
        assert attempt.device_auth_id_ciphertext != b"device-secret"
        assert attempt.user_code == "ABCD-EFGH"
        attempt.next_poll_at = now - timedelta(seconds=1)
        session.commit()
        result = poll_device_login(session, settings, view.login_id, client, now=lambda: now)
        assert result.status == "SUCCEEDED"
        assert result.transitioned is True
        auth = get_auth_configuration(session)
        assert auth is not None
        assert auth.chatgpt_account_id == "acct-1"
        assert auth.access_token_ciphertext is not None
        assert b"refresh-secret-1" not in auth.refresh_token_ciphertext
        events = session.scalars(
            select(Event).where(Event.kind == "CODEX_CHATGPT_AUTH_CONNECTED")
        ).all()
        assert len(events) == 1
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


def test_reusing_pending_device_login_is_marked_without_creating_an_attempt(engine, settings) -> None:
    now = datetime(2026, 9, 2, 1, 0, tzinfo=UTC)
    factory = create_session_factory(engine)
    start_client = httpx.Client(transport=httpx.MockTransport(lambda request: httpx.Response(200, json={
        "device_auth_id": "device-secret",
        "user_code": "ABCD-EFGH",
        "interval": 1,
        "expires_in": 600,
    })))
    with factory() as session:
        first = start_device_login(session, settings, start_client, now=lambda: now)
        session.commit()

    with factory() as session:
        reused = start_device_login(session, settings, now=lambda: now)

    assert first.created is True
    assert reused.created is False
    assert reused.login_id == first.login_id


def test_repeated_device_cancel_reports_only_the_first_transition(engine, settings) -> None:
    now = datetime(2026, 9, 2, 1, 0, tzinfo=UTC)
    login_id = uuid4()
    factory = create_session_factory(engine)
    with factory.begin() as session:
        session.add(
            CodexChatgptLoginAttempt(
                id=login_id,
                state="PENDING",
                verification_url="https://auth.openai.com/codex/device",
                poll_interval_seconds=5,
                expires_at=now + timedelta(minutes=10),
                next_poll_at=now + timedelta(seconds=5),
            )
        )

    with factory() as session:
        first = cancel_device_login(session, login_id)
        session.commit()
    with factory() as session:
        second = cancel_device_login(session, login_id)

    assert first.status == LOGIN_CANCELLED
    assert first.transitioned is True
    assert second.status == LOGIN_CANCELLED
    assert second.transitioned is False


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


def test_slow_down_persists_backoff_and_moves_next_poll_time(engine, settings) -> None:
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
        original_interval = attempt.poll_interval_seconds
        attempt.next_poll_at = now - timedelta(seconds=1)
        session.commit()

    slow_client = httpx.Client(transport=httpx.MockTransport(lambda request: httpx.Response(400, json={"error": "slow_down"})))
    with factory() as session:
        result = poll_device_login(session, settings, view.login_id, slow_client, now=lambda: now)
        attempt = session.get(CodexChatgptLoginAttempt, view.login_id)
        assert result.status == "PENDING"
        assert result.poll_after_seconds == original_interval + 5
        assert attempt is not None
        assert attempt.poll_interval_seconds == original_interval + 5
        assert attempt.next_poll_at.replace(tzinfo=UTC) >= now + timedelta(seconds=original_interval + 5) - timedelta(microseconds=1)


def test_poll_lease_prevents_a_second_upstream_exchange(engine, settings) -> None:
    now = datetime(2026, 9, 2, 1, 0, tzinfo=UTC)
    login_id = uuid4()
    factory = create_session_factory(engine)
    with factory.begin() as session:
        session.add(
            CodexChatgptLoginAttempt(
                id=login_id,
                state="PENDING",
                verification_url="https://auth.openai.com/codex/device",
                poll_interval_seconds=5,
                expires_at=now + timedelta(minutes=10),
                next_poll_at=now - timedelta(seconds=1),
                poll_lease_until=now + timedelta(seconds=60),
            )
        )

    calls = 0

    def should_not_call(request: httpx.Request) -> httpx.Response:
        nonlocal calls
        calls += 1
        raise AssertionError("an in-flight poll lease must prevent upstream I/O")

    client = httpx.Client(transport=httpx.MockTransport(should_not_call))
    with factory() as session:
        result = poll_device_login(session, settings, login_id, client, now=lambda: now)

    assert result.status == "PENDING"
    assert result.poll_after_seconds >= 59
    assert calls == 0


def test_reauthentication_rotates_canonical_auth_identity(engine, settings) -> None:
    now = datetime(2026, 9, 2, 1, 0, tzinfo=UTC)
    access_token = _jwt({"chatgpt_account_id": "acct-old"})
    factory = create_session_factory(engine)
    with factory() as session:
        _install_bundle(
            session,
            settings,
            access_token=access_token,
            refresh_token="refresh-old",
            id_token=None,
            response={"access_token": access_token},
            authenticated_at=now,
        )
        session.commit()
        auth = get_auth_configuration(session)
        assert auth is not None
        old_id = auth.id
        auth.state = CHATGPT_AUTH_REAUTH_REQUIRED
        session.commit()

        new_access = _jwt({"chatgpt_account_id": "acct-new"})
        new_auth = _install_bundle(
            session,
            settings,
            access_token=new_access,
            refresh_token="refresh-new",
            id_token=None,
            response={"access_token": new_access},
            authenticated_at=now,
        )
        session.commit()

    assert new_auth.id != old_id
    assert new_auth.chatgpt_account_id == "acct-new"


def test_corrupt_connected_auth_is_publicly_reauth_and_can_start_again(engine, settings) -> None:
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
            response={"access_token": access_token},
            authenticated_at=now,
        )
        session.commit()
        auth = get_auth_configuration(session)
        assert auth is not None
        auth.refresh_token_ciphertext = b"corrupt"
        session.commit()

    start_client = httpx.Client(transport=httpx.MockTransport(lambda request: httpx.Response(200, json={
        "device_auth_id": "device-secret",
        "user_code": "ABCD-EFGH",
        "interval": 1,
        "expires_in": 600,
    })))
    with factory() as session:
        assert auth_status(session, settings)["state"] == CHATGPT_AUTH_REAUTH_REQUIRED
        view = start_device_login(session, settings, start_client, now=lambda: now)

    assert view.created is True


def test_initialize_legacy_cleanup_tolerates_concurrent_disappearance(engine, settings, monkeypatch) -> None:
    settings.codex_home.mkdir(parents=True, exist_ok=True)
    legacy = settings.codex_home / "auth.json"
    legacy.write_text(json.dumps({
        "auth_mode": "chatgpt",
        "chatgpt_account_id": "acct-1",
        "tokens": {"access_token": _jwt({"chatgpt_account_id": "acct-1"}), "refresh_token": "refresh-secret"},
    }), encoding="utf-8")

    def disappeared(path) -> None:
        raise FileNotFoundError(path)

    monkeypatch.setattr(type(legacy), "unlink", disappeared)
    initialize_codex_auth(create_session_factory(engine), settings)


def test_readiness_rejects_connected_auth_with_undecryptable_refresh_token(engine, settings) -> None:
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
            response={"access_token": access_token},
            authenticated_at=now,
        )
        session.commit()
        auth = get_auth_configuration(session)
        assert auth is not None
        auth.refresh_token_ciphertext = b"corrupt"
        session.commit()

    with factory() as session:
        assert codex_auth_readiness(session, settings) == (False, "REAUTH_REQUIRED")


def test_readiness_rejects_undecryptable_persisted_custom_provider_key(engine, settings) -> None:
    factory = create_session_factory(engine)
    with factory.begin() as session:
        item = RuntimeConfiguration(
            scope="SYSTEM",
            revision=1,
            codex_base_url="https://gateway.example.test/v1",
            max_plugin_wheel_bytes=settings.max_plugin_wheel_bytes,
            plugin_validation_timeout_seconds=settings.plugin_validation_timeout_seconds,
            bundle_build_timeout_seconds=settings.bundle_build_timeout_seconds,
            plugin_job_timeout_seconds=settings.plugin_job_timeout_seconds,
            mission_job_timeout_seconds=settings.mission_job_timeout_seconds,
            job_poll_seconds=settings.job_poll_seconds,
            job_lease_seconds=settings.job_lease_seconds,
        )
        session.add(item)
        session.flush()
        encrypted = encrypt_bound_secret(
            "provider-secret",
            master_key=settings.master_key_bytes(),
            associated_data=_api_key_aad(item.id, 1),
        )
        item.codex_api_key_ciphertext = encrypted.ciphertext
        item.codex_api_key_nonce = encrypted.nonce
        item.codex_api_key_key_version = encrypted.key_version

    with factory() as session:
        assert codex_auth_readiness(session, settings) == (True, "CUSTOM_PROVIDER")
        item = session.scalar(select(RuntimeConfiguration))
        assert item is not None
        item.codex_api_key_ciphertext = b"corrupt"
        session.commit()

    with factory() as session:
        assert codex_auth_readiness(session, settings) == (False, "CUSTOM_PROVIDER_REAUTH_REQUIRED")


def test_readiness_rejects_partially_persisted_custom_provider_key(engine, settings) -> None:
    factory = create_session_factory(engine)
    with factory.begin() as session:
        session.add(
            RuntimeConfiguration(
                scope="SYSTEM",
                revision=1,
                codex_api_key_ciphertext=b"partial",
                max_plugin_wheel_bytes=settings.max_plugin_wheel_bytes,
                plugin_validation_timeout_seconds=settings.plugin_validation_timeout_seconds,
                bundle_build_timeout_seconds=settings.bundle_build_timeout_seconds,
                plugin_job_timeout_seconds=settings.plugin_job_timeout_seconds,
                mission_job_timeout_seconds=settings.mission_job_timeout_seconds,
                job_poll_seconds=settings.job_poll_seconds,
                job_lease_seconds=settings.job_lease_seconds,
            )
        )

    with factory() as session:
        assert codex_auth_readiness(session, settings) == (False, "CUSTOM_PROVIDER_REAUTH_REQUIRED")


def test_access_bundle_rejects_replaced_canonical_auth_row(engine, settings) -> None:
    now = datetime(2026, 9, 2, 1, 0, tzinfo=UTC)
    access_a = _jwt({"chatgpt_account_id": "acct-a"})
    access_b = _jwt({"chatgpt_account_id": "acct-b"})
    factory = create_session_factory(engine)
    with factory() as session:
        _install_bundle(
            session,
            settings,
            access_token=access_a,
            refresh_token="refresh-a",
            id_token=None,
            response={"access_token": access_a},
            authenticated_at=now,
        )
        session.commit()
        bundle_a = get_valid_access_bundle(session, settings, now=lambda: now)
        disconnect_chatgpt(session)
        _install_bundle(
            session,
            settings,
            access_token=access_b,
            refresh_token="refresh-b",
            id_token=None,
            response={"access_token": access_b},
            authenticated_at=now,
        )
        session.commit()

    with factory() as session:
        with pytest.raises(QfError) as error:
            get_valid_access_bundle(
                session,
                settings,
                expected_auth_id=bundle_a.auth_id,
                expected_account_id=bundle_a.chatgpt_account_id,
                now=lambda: now,
            )
        assert error.value.code == "CODEX_CHATGPT_AUTH_REAUTH_REQUIRED"
