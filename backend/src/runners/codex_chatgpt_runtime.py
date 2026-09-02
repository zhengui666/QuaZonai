"""Thin adapter for Codex 0.144.4 external ChatGPT authentication."""

from __future__ import annotations

from contextlib import contextmanager
from pathlib import Path
from typing import Any, Iterator
from uuid import UUID

from codex_chatgpt_auth import get_auth_configuration, get_valid_access_bundle, remove_legacy_auth_file
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
    # API startup normally imports this file, but workers can start after
    # migrations and before the API process has completed that initialization.
    # Never destroy the only legacy credential source before a canonical row
    # exists; a later API startup must still be able to import it.
    with session_factory() as session:
        has_canonical_auth = get_auth_configuration(session) is not None
    if has_canonical_auth:
        remove_legacy_auth_file(settings)
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
                "access_token": bundle.access_token,
                "chatgpt_account_id": bundle.chatgpt_account_id,
                "chatgpt_plan_type": bundle.plan_type,
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
        yield Thread(client, response.thread.id)
    finally:
        client.close()


__all__ = ["external_chatgpt_thread"]
