from __future__ import annotations

import sys
from contextlib import contextmanager, nullcontext
from datetime import UTC, datetime, timedelta
from types import ModuleType, SimpleNamespace
from uuid import uuid4

import pytest

import runners.codex_chatgpt_runtime as runtime
from codex_chatgpt_auth import CodexChatgptAccessBundle
from errors import QfError


def _install_fake_codex(monkeypatch):  # type: ignore[no-untyped-def]
    class FakeApprovalMode:
        deny_all = SimpleNamespace(value="deny_all")

    class FakeSandbox:
        workspace_write = SimpleNamespace(value="workspace-write")

    class FakeThread:
        def __init__(self, client, thread_id) -> None:  # type: ignore[no-untyped-def]
            self.client = client
            self.id = thread_id
            self.run_calls = []

        def run(self, *args, **kwargs):  # type: ignore[no-untyped-def]
            self.run_calls.append((args, kwargs))
            return SimpleNamespace(final_response="done")

    class FakeClient:
        instance = None

        def __init__(self, config, approval_handler) -> None:  # type: ignore[no-untyped-def]
            self.approval_handler = approval_handler
            self.login_payload = None
            self.resume_payload = None
            self.closed = False
            FakeClient.instance = self

        def start(self) -> None:
            pass

        def initialize(self) -> None:
            pass

        def account_login_start(self, payload):  # type: ignore[no-untyped-def]
            self.login_payload = payload
            return SimpleNamespace(root=SimpleNamespace(type="chatgptAuthTokens"))

        def thread_start(self, payload):  # type: ignore[no-untyped-def]
            return SimpleNamespace(thread=SimpleNamespace(id="thread-1"))

        def thread_resume(self, thread_id, payload):  # type: ignore[no-untyped-def]
            self.resume_payload = (thread_id, payload)
            return SimpleNamespace(thread=SimpleNamespace(id=thread_id))

        def close(self) -> None:
            self.closed = True

    openai_codex = ModuleType("openai_codex")
    openai_codex.ApprovalMode = FakeApprovalMode
    openai_codex.Sandbox = FakeSandbox
    openai_codex.Thread = FakeThread
    client_module = ModuleType("openai_codex.client")
    client_module.CodexClient = FakeClient
    monkeypatch.setitem(sys.modules, "openai_codex", openai_codex)
    monkeypatch.setitem(sys.modules, "openai_codex.client", client_module)
    return FakeClient


def test_refresh_callback_uses_pinned_camel_case_wire_fields(monkeypatch, settings, tmp_path) -> None:  # type: ignore[no-untyped-def]
    FakeClient = _install_fake_codex(monkeypatch)

    initialized = []
    monkeypatch.setattr(
        runtime,
        "initialize_codex_auth",
        lambda factory, configured_settings: initialized.append((factory, configured_settings)),
    )
    bundle = CodexChatgptAccessBundle(
        auth_id=uuid4(),
        access_token="access-token",
        chatgpt_account_id="account-1",
        plan_type="pro",
        token_generation=1,
        expires_at=datetime.now(UTC) + timedelta(minutes=5),
    )
    monkeypatch.setattr(runtime, "get_valid_access_bundle", lambda *args, **kwargs: bundle)
    locks = []
    monkeypatch.setattr(runtime, "lock_codex_auth_operations", lambda session: locks.append(session))
    monkeypatch.setattr(
        runtime,
        "get_auth_configuration",
        lambda session, for_update=False: SimpleNamespace(
            id=bundle.auth_id,
            state="CONNECTED",
            chatgpt_account_id=bundle.chatgpt_account_id,
        ),
    )

    class FakeSession:
        def commit(self) -> None:
            pass

        def begin(self):  # type: ignore[no-untyped-def]
            return nullcontext(self)

    @contextmanager
    def fake_session_factory():
        yield FakeSession()

    with runtime.external_chatgpt_thread(
        config=SimpleNamespace(),
        settings=settings,
        session_factory=fake_session_factory,
        workspace=tmp_path,
        model=None,
        service_tier=None,
        thread_config={},
        developer_instructions="instructions",
    ) as thread:
        assert thread.id == "thread-1"
        assert FakeClient.instance is not None
        result = FakeClient.instance.approval_handler("account/chatgptAuthTokens/refresh", {})
        assert result == {
            "accessToken": "access-token",
            "chatgptAccountId": "account-1",
            "chatgptPlanType": "pro",
        }
        assert FakeClient.instance.login_payload["accessToken"] == "access-token"

    assert initialized == [(fake_session_factory, settings)]
    assert len(locks) == 1
    assert FakeClient.instance.closed is True


def test_admission_guard_rejects_auth_replaced_before_mission_admission(monkeypatch, settings, tmp_path) -> None:  # type: ignore[no-untyped-def]
    FakeClient = _install_fake_codex(monkeypatch)
    monkeypatch.setattr(runtime, "initialize_codex_auth", lambda *args, **kwargs: None)
    bundle = CodexChatgptAccessBundle(
        auth_id=uuid4(),
        access_token="access-token",
        chatgpt_account_id="account-1",
        plan_type="pro",
        token_generation=1,
        expires_at=datetime.now(UTC) + timedelta(minutes=5),
    )
    monkeypatch.setattr(runtime, "get_valid_access_bundle", lambda *args, **kwargs: bundle)
    monkeypatch.setattr(runtime, "lock_codex_auth_operations", lambda session: None)
    monkeypatch.setattr(runtime, "get_auth_configuration", lambda session, for_update=False: None)

    class FakeSession:
        def commit(self) -> None:
            pass

        def begin(self):  # type: ignore[no-untyped-def]
            return nullcontext(self)

    @contextmanager
    def fake_session_factory():
        yield FakeSession()

    with pytest.raises(QfError) as exc_info:
        with runtime.external_chatgpt_thread(
            config=SimpleNamespace(),
            settings=settings,
            session_factory=fake_session_factory,
            workspace=tmp_path,
            model=None,
            service_tier=None,
            thread_config={},
            developer_instructions="instructions",
        ):
            raise AssertionError("auth replacement must prevent yielding a Mission thread")

    assert exc_info.value.code == "CODEX_CHATGPT_AUTH_REAUTH_REQUIRED"
    assert FakeClient.instance is not None
    assert FakeClient.instance.closed is True


def test_admission_guard_releases_before_mission_turn_runs(monkeypatch, settings, tmp_path) -> None:  # type: ignore[no-untyped-def]
    _install_fake_codex(monkeypatch)
    monkeypatch.setattr(runtime, "initialize_codex_auth", lambda *args, **kwargs: None)
    bundle = CodexChatgptAccessBundle(
        auth_id=uuid4(),
        access_token="access-token",
        chatgpt_account_id="account-1",
        plan_type="pro",
        token_generation=1,
        expires_at=datetime.now(UTC) + timedelta(minutes=5),
    )
    monkeypatch.setattr(runtime, "get_valid_access_bundle", lambda *args, **kwargs: bundle)
    monkeypatch.setattr(runtime, "lock_codex_auth_operations", lambda session: None)
    monkeypatch.setattr(
        runtime,
        "get_auth_configuration",
        lambda session, for_update=False: SimpleNamespace(
            id=bundle.auth_id,
            state="CONNECTED",
            chatgpt_account_id=bundle.chatgpt_account_id,
        ),
    )
    transaction_state = {"open": False}

    class Transaction:
        def __enter__(self):
            transaction_state["open"] = True
            return self

        def __exit__(self, exc_type, exc, tb) -> None:  # type: ignore[no-untyped-def]
            transaction_state["open"] = False

    class FakeSession:
        def commit(self) -> None:
            pass

        def begin(self):  # type: ignore[no-untyped-def]
            return Transaction()

    @contextmanager
    def fake_session_factory():
        yield FakeSession()

    with runtime.external_chatgpt_thread(
        config=SimpleNamespace(),
        settings=settings,
        session_factory=fake_session_factory,
        workspace=tmp_path,
        model=None,
        service_tier=None,
        thread_config={},
        developer_instructions="instructions",
    ) as thread:
        assert transaction_state["open"] is True
        result = thread.run("mission")
        assert result.final_response == "done"
        assert transaction_state["open"] is False


def test_external_chatgpt_thread_resumes_the_existing_thread(monkeypatch, settings, tmp_path) -> None:  # type: ignore[no-untyped-def]
    FakeClient = _install_fake_codex(monkeypatch)
    monkeypatch.setattr(runtime, "initialize_codex_auth", lambda *args, **kwargs: None)
    bundle = CodexChatgptAccessBundle(
        auth_id=uuid4(),
        access_token="access-token",
        chatgpt_account_id="account-1",
        plan_type="pro",
        token_generation=1,
        expires_at=datetime.now(UTC) + timedelta(minutes=5),
    )
    monkeypatch.setattr(runtime, "get_valid_access_bundle", lambda *args, **kwargs: bundle)
    monkeypatch.setattr(runtime, "lock_codex_auth_operations", lambda session: None)
    monkeypatch.setattr(
        runtime,
        "get_auth_configuration",
        lambda session, for_update=False: SimpleNamespace(
            id=bundle.auth_id,
            state="CONNECTED",
            chatgpt_account_id=bundle.chatgpt_account_id,
        ),
    )

    class FakeSession:
        def commit(self) -> None:
            pass

        def begin(self):  # type: ignore[no-untyped-def]
            return nullcontext(self)

    @contextmanager
    def fake_session_factory():
        yield FakeSession()

    with runtime.external_chatgpt_thread(
        config=SimpleNamespace(),
        settings=settings,
        session_factory=fake_session_factory,
        workspace=tmp_path,
        model=None,
        service_tier=None,
        thread_config={},
        developer_instructions="instructions",
        existing_thread_id="thread-existing",
    ) as thread:
        assert thread.id == "thread-existing"

    assert FakeClient.instance is not None
    assert FakeClient.instance.resume_payload is not None
    assert FakeClient.instance.resume_payload[0] == "thread-existing"
