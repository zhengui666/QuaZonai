from __future__ import annotations

import sys
from contextlib import contextmanager
from datetime import UTC, datetime, timedelta
from types import ModuleType, SimpleNamespace
from uuid import uuid4

import runners.codex_chatgpt_runtime as runtime
from codex_chatgpt_auth import CodexChatgptAccessBundle


def test_refresh_callback_uses_pinned_camel_case_wire_fields(monkeypatch, settings, tmp_path) -> None:  # type: ignore[no-untyped-def]
    class FakeApprovalMode:
        deny_all = SimpleNamespace(value="deny_all")

    class FakeSandbox:
        workspace_write = SimpleNamespace(value="workspace-write")

    class FakeThread:
        def __init__(self, client, thread_id) -> None:  # type: ignore[no-untyped-def]
            self.client = client
            self.id = thread_id

    class FakeClient:
        instance = None

        def __init__(self, config, approval_handler) -> None:  # type: ignore[no-untyped-def]
            self.approval_handler = approval_handler
            self.login_payload = None
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

    class FakeSession:
        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc, tb) -> None:  # type: ignore[no-untyped-def]
            pass

        def commit(self) -> None:
            pass

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
    ):
        assert FakeClient.instance is not None
        result = FakeClient.instance.approval_handler("account/chatgptAuthTokens/refresh", {})
        assert result == {
            "accessToken": "access-token",
            "chatgptAccountId": "account-1",
            "chatgptPlanType": "pro",
        }
        assert FakeClient.instance.login_payload["accessToken"] == "access-token"

    assert initialized == [(fake_session_factory, settings)]
    assert FakeClient.instance.closed is True
