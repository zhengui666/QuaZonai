"""Thin adapter for Codex 0.144.4 external ChatGPT authentication."""

from __future__ import annotations

from contextlib import ExitStack, contextmanager
from pathlib import Path
from typing import Any, Iterator
from uuid import UUID

from codex_chatgpt_auth import (
    get_auth_configuration,
    get_valid_access_bundle,
    initialize_codex_auth,
    lock_codex_auth_operations,
)
from db.codex_auth_models import CHATGPT_AUTH_CONNECTED
from db.session import SessionFactory
from errors import QfError
from settings import Settings


def _decline_approval(method: str) -> dict[str, str]:
    if method in {
        "item/commandExecution/requestApproval",
        "item/fileChange/requestApproval",
    }:
        return {"decision": "decline"}
    raise QfError(
        "CODEX_SERVER_REQUEST_DENIED",
        "The Codex server request is not allowed by the Mission policy.",
        403,
    )


def _lock_expected_auth_for_admission(
    session: Any,
    *,
    expected_auth_id: UUID,
    expected_account_id: str,
) -> None:
    """Order Mission admission against disconnect for the pinned ChatGPT identity."""
    lock_codex_auth_operations(session)
    auth = get_auth_configuration(session, for_update=True)
    if (
        auth is None
        or auth.state != CHATGPT_AUTH_CONNECTED
        or auth.id != expected_auth_id
        or auth.chatgpt_account_id != expected_account_id
    ):
        raise QfError(
            "CODEX_CHATGPT_AUTH_REAUTH_REQUIRED",
            "The ChatGPT authentication used by this Mission changed before admission.",
            503,
        )


class _AdmissionGuardedThread:
    """Hold the auth-operation lock until the caller commits Mission admission.

    ``run_mission`` starts the Codex App Server, enters its READY->RUNNING
    transaction, reads ``thread.id`` for the durable admission record, commits,
    and only then calls ``thread.run``.  Keeping this guard alive until the first
    run therefore orders that admission commit against Disconnect without
    holding the authentication lock for the full Mission execution.
    """

    def __init__(self, thread: Any, admission_guard: ExitStack) -> None:
        self._thread = thread
        self._admission_guard: ExitStack | None = admission_guard

    @property
    def id(self) -> str:
        return self._thread.id

    def release_admission_guard(self) -> None:
        guard = self._admission_guard
        if guard is None:
            return
        self._admission_guard = None
        guard.close()

    def run(self, *args: Any, **kwargs: Any) -> Any:
        self.release_admission_guard()
        return self._thread.run(*args, **kwargs)

    def __getattr__(self, name: str) -> Any:
        return getattr(self._thread, name)


def _acquire_admission_guard(
    session_factory: SessionFactory,
    *,
    expected_auth_id: UUID,
    expected_account_id: str,
) -> ExitStack:
    guard = ExitStack()
    try:
        session = guard.enter_context(session_factory())
        guard.enter_context(session.begin())
        _lock_expected_auth_for_admission(
            session,
            expected_auth_id=expected_auth_id,
            expected_account_id=expected_account_id,
        )
    except Exception:
        guard.close()
        raise
    return guard


@contextmanager
def external_chatgpt_thread(
    config: Any,
    *,
    settings: Settings,
    session_factory: SessionFactory,
    workspace: Path,
    model: str | None,
    service_tier: str | None,
    thread_config: dict[str, Any],
    developer_instructions: str,
) -> Iterator[Any]:
    """Start a ChatGPT-authenticated App Server and yield a public Thread.

    OAuth access tokens are held only by this trusted parent callback and the
    App Server's external-auth memory.  The refresh callback uses the observed
    generation to avoid duplicate refresh-token rotation across workers.
    """
    # Workers can start after migrations and before API initialization.  Admit
    # no Mission until the one-time legacy importer has established the same
    # canonical DB fact the API uses.
    initialize_codex_auth(session_factory, settings)
    from openai_codex import ApprovalMode, Sandbox, Thread
    from openai_codex.client import CodexClient

    observed_generation: int | None = None
    observed_auth_id: UUID | None = None
    observed_account_id: str | None = None

    def server_request_handler(method: str, params: dict[str, Any] | None) -> dict[str, Any]:
        nonlocal observed_account_id, observed_auth_id, observed_generation
        if method == "account/chatgptAuthTokens/refresh":
            with session_factory() as session:
                bundle = get_valid_access_bundle(
                    session,
                    settings,
                    force_refresh=True,
                    observed_generation=observed_generation,
                    expected_auth_id=observed_auth_id,
                    expected_account_id=observed_account_id,
                )
                session.commit()
            observed_generation = bundle.token_generation
            observed_auth_id = bundle.auth_id
            observed_account_id = bundle.chatgpt_account_id
            return {
                "accessToken": bundle.access_token,
                "chatgptAccountId": bundle.chatgpt_account_id,
                "chatgptPlanType": bundle.plan_type,
            }
        return _decline_approval(method)

    client = CodexClient(config, approval_handler=server_request_handler)
    try:
        client.start()
        client.initialize()
        with session_factory() as session:
            bundle = get_valid_access_bundle(session, settings)
            session.commit()
        observed_generation = bundle.token_generation
        observed_auth_id = bundle.auth_id
        observed_account_id = bundle.chatgpt_account_id
        login = client.account_login_start(
            {
                "type": "chatgptAuthTokens",
                "accessToken": bundle.access_token,
                "chatgptAccountId": bundle.chatgpt_account_id,
                "chatgptPlanType": bundle.plan_type,
            }
        )
        if getattr(login.root, "type", None) != "chatgptAuthTokens":
            raise QfError(
                "CODEX_CHATGPT_AUTH_FAILED",
                "Codex did not accept the ChatGPT external authentication.",
                503,
            )
        response = client.thread_start(
            {
                "approvalPolicy": ApprovalMode.deny_all.value,
                "sandbox": Sandbox.workspace_write.value,
                "cwd": str(workspace),
                "model": model,
                "serviceTier": service_tier,
                "config": thread_config,
                "developerInstructions": developer_instructions,
            }
        )
        admission_guard = _acquire_admission_guard(
            session_factory,
            expected_auth_id=bundle.auth_id,
            expected_account_id=bundle.chatgpt_account_id,
        )
        thread = _AdmissionGuardedThread(Thread(client, response.thread.id), admission_guard)
        try:
            yield thread
        finally:
            thread.release_admission_guard()
    finally:
        client.close()


__all__ = ["external_chatgpt_thread"]
