"""Database-owned ChatGPT Device Code OAuth and token lifecycle.

This module is deliberately the only backend module that knows the OpenAI
Device Auth endpoints.  Access and refresh tokens are decrypted only inside
the trusted caller and are never part of an API model, event, or log message.
"""

from __future__ import annotations

import base64
import binascii
import json
import logging
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any, Callable, Mapping
from uuid import UUID, uuid4

import httpx
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from crypto import EncryptedSecret, decrypt_bound_secret, encrypt_bound_secret
from db.codex_auth_models import (
    CHATGPT_AUTH_CONNECTED,
    CHATGPT_AUTH_REAUTH_REQUIRED,
    CodexChatgptAuthConfiguration,
    CodexChatgptAuthOperationLock,
    CodexChatgptLoginAttempt,
    LOGIN_CANCELLED,
    LOGIN_EXPIRED,
    LOGIN_FAILED,
    LOGIN_PENDING,
    LOGIN_SUCCEEDED,
    SYSTEM_SCOPE,
)
from db.models import RuntimeConfiguration
from errors import QfError
from events import append_event
from runtime_config import codex_api_key_configured, codex_api_key_decryptable
from settings import Settings, SettingsError

logger = logging.getLogger(__name__)

CODEX_OAUTH_CLIENT_ID = "app_EMoamEEZ73f0CkXaXp7hrann"
DEVICE_AUTH_USERCODE_URL = "https://auth.openai.com/api/accounts/deviceauth/usercode"
DEVICE_AUTH_TOKEN_URL = "https://auth.openai.com/api/accounts/deviceauth/token"
OAUTH_TOKEN_URL = "https://auth.openai.com/oauth/token"
DEVICE_VERIFICATION_URL = "https://auth.openai.com/codex/device"
DEVICE_REDIRECT_URI = "https://auth.openai.com/deviceauth/callback"
TOKEN_REFRESH_BUFFER_SECONDS = 120
DEVICE_CODE_DEFAULT_EXPIRES_IN = 900
POLLING_SAFETY_MARGIN_SECONDS = 3
OAUTH_HTTP_TIMEOUT_SECONDS = 30
APP_SERVER_REFRESH_HTTP_TIMEOUT_SECONDS = 5
ACCESS_TOKEN_FALLBACK_TTL_SECONDS = 300
MAX_POLL_INTERVAL_SECONDS = 3600
POLL_LEASE_SECONDS = OAUTH_HTTP_TIMEOUT_SECONDS * 2 + 5

_ACCOUNT_ID_CLAIMS = (
    "chatgpt_account_id",
    "https://api.openai.com/auth.chatgpt_account_id",
)
_EMAIL_CLAIMS = ("email", "https://api.openai.com/auth.email")
_PLAN_CLAIMS = ("chatgpt_plan_type", "plan_type", "https://api.openai.com/auth.chatgpt_plan_type")


def _now() -> datetime:
    return datetime.now(UTC)


def _aware(value: datetime) -> datetime:
    return value.replace(tzinfo=UTC) if value.tzinfo is None else value


def _as_positive_int(value: object, default: int, *, maximum: int | None = None) -> int:
    try:
        result = int(value) if isinstance(value, (int, float, str)) else default
    except (TypeError, ValueError):
        result = default
    if result <= 0:
        result = default
    if maximum is not None:
        result = min(result, maximum)
    return result


def _safe_string(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    value = value.strip()
    return value or None


def _jwt_payload(token: str | None) -> dict[str, Any]:
    """Decode untrusted JWT claims for metadata/expiry only.

    Signature verification is intentionally not attempted here.  OAuth HTTPS
    responses establish the credential; claims are only UI metadata and a
    conservative expiry hint.
    """
    if not token or token.count(".") != 2:
        return {}
    try:
        encoded = token.split(".", 2)[1]
        encoded += "=" * (-len(encoded) % 4)
        decoded = base64.urlsafe_b64decode(encoded.encode("ascii"))
        payload = json.loads(decoded.decode("utf-8"))
    except (ValueError, UnicodeError, json.JSONDecodeError, binascii.Error):
        return {}
    return payload if isinstance(payload, dict) else {}


def _claim(payload: Mapping[str, Any], names: tuple[str, ...]) -> str | None:
    for name in names:
        value = _safe_string(payload.get(name))
        if value is not None:
            return value
    auth = payload.get("https://api.openai.com/auth")
    if isinstance(auth, Mapping):
        for name in names:
            short_name = name.rsplit(".", 1)[-1]
            value = _safe_string(auth.get(name) or auth.get(short_name))
            if value is not None:
                return value
    return None


def _metadata(
    response: Mapping[str, Any],
    *,
    access_token: str,
    id_token: str | None,
    now: datetime,
) -> tuple[str | None, str | None, str | None, datetime]:
    access_claims = _jwt_payload(access_token)
    id_claims = _jwt_payload(id_token)
    claims = (response, id_claims, access_claims)

    def first_claim(names: tuple[str, ...]) -> str | None:
        for item in claims:
            value = _claim(item, names)
            if value is not None:
                return value
        return None

    account_id = first_claim(_ACCOUNT_ID_CLAIMS)
    email = first_claim(_EMAIL_CLAIMS)
    plan = first_claim(_PLAN_CLAIMS)
    expires_in = response.get("expires_in")
    if expires_in is not None:
        try:
            expires_at = now + timedelta(seconds=max(1, int(float(expires_in))))
        except (TypeError, ValueError):
            expires_at = now + timedelta(seconds=ACCESS_TOKEN_FALLBACK_TTL_SECONDS)
    else:
        exp = next(
            (item.get("exp") for item in (id_claims, access_claims) if item.get("exp") is not None),
            None,
        )
        try:
            expires_at = datetime.fromtimestamp(float(exp), tz=UTC) if exp is not None else now + timedelta(seconds=ACCESS_TOKEN_FALLBACK_TTL_SECONDS)
        except (TypeError, ValueError, OverflowError, OSError):
            expires_at = now + timedelta(seconds=ACCESS_TOKEN_FALLBACK_TTL_SECONDS)
    return account_id, email, plan, expires_at


def _aad(auth_id: UUID, field: str, key_version: int) -> bytes:
    return (
        f"quazonai|codex_chatgpt_auth={auth_id}|field={field}|key_version={key_version}"
    ).encode("utf-8")


def _encrypt(
    value: str,
    *,
    auth_id: UUID,
    field: str,
    settings: Settings,
) -> EncryptedSecret:
    return encrypt_bound_secret(
        value,
        master_key=settings.master_key_bytes(),
        associated_data=_aad(auth_id, field, 1),
        key_version=1,
    )


def _decrypt(
    ciphertext: bytes | None,
    nonce: bytes | None,
    key_version: int | None,
    *,
    auth_id: UUID,
    field: str,
    settings: Settings,
) -> str:
    if ciphertext is None or nonce is None or key_version is None or key_version <= 0:
        raise QfError("CODEX_CHATGPT_AUTH_CORRUPT", "Stored ChatGPT authentication is incomplete.", 503)
    return decrypt_bound_secret(
        EncryptedSecret(ciphertext=ciphertext, nonce=nonce, key_version=key_version),
        master_key=settings.master_key_bytes(),
        associated_data=_aad(auth_id, field, key_version),
    )


class _OAuthFailure(Exception):
    def __init__(
        self,
        kind: str,
        code: str = "oauth_request_failed",
        *,
        retry_after: int | None = None,
    ) -> None:
        super().__init__(code)
        self.kind = kind
        self.code = code
        self.retry_after = retry_after


def _response_json(response: httpx.Response) -> Mapping[str, Any]:
    try:
        value = response.json()
    except (ValueError, json.JSONDecodeError) as exc:
        raise _OAuthFailure("permanent", "malformed_response") from exc
    if not isinstance(value, Mapping):
        raise _OAuthFailure("permanent", "malformed_response")
    return value


def _post_json(
    client: Any,
    url: str,
    *,
    timeout: float,
    json_body: Mapping[str, Any] | None = None,
    form: Mapping[str, str] | None = None,
) -> Mapping[str, Any]:
    try:
        response = client.post(url, json=json_body, data=form, timeout=timeout)
    except httpx.TimeoutException as exc:
        raise _OAuthFailure("transient", "timeout") from exc
    except httpx.RequestError as exc:
        raise _OAuthFailure("transient", "connection_error") from exc
    status = response.status_code
    if status == 429 or status >= 500:
        retry_after: int | None = None
        if status == 429:
            retry_after = _as_positive_int(response.headers.get("Retry-After"), 0)
        raise _OAuthFailure(
            "transient",
            "rate_limited" if status == 429 else "upstream_5xx",
            retry_after=retry_after,
        )
    if status in {401, 403}:
        payload = _response_json(response)
        code = _safe_string(payload.get("error")) or _safe_string(payload.get("error_code"))
        raise _OAuthFailure("permanent", code or "invalid_credential")
    if status >= 400:
        payload = _response_json(response)
        code = _safe_string(payload.get("error")) or _safe_string(payload.get("error_code"))
        if code in {"authorization_pending", "slow_down"}:
            raise _OAuthFailure("pending", code)
        if code in {"expired_token", "device_code_expired"}:
            raise _OAuthFailure("expired", code)
        if code in {"access_denied", "invalid_grant", "invalid_request"}:
            raise _OAuthFailure("permanent", code or "invalid_request")
        raise _OAuthFailure("permanent", code or "upstream_rejected")
    return _response_json(response)


def _http_client(client: Any | None) -> tuple[Any, bool]:
    if client is not None:
        return client, False
    return httpx.Client(timeout=OAUTH_HTTP_TIMEOUT_SECONDS), True


@dataclass(frozen=True, slots=True)
class DeviceLoginView:
    login_id: UUID
    status: str
    verification_url: str
    user_code: str
    expires_at: datetime
    poll_after_seconds: int
    created: bool = True


@dataclass(frozen=True, slots=True)
class DeviceLoginPollResult:
    status: str
    expires_at: datetime | None = None
    poll_after_seconds: int | None = None
    auth: dict[str, Any] | None = None
    error_code: str | None = None
    transitioned: bool = False


@dataclass(frozen=True, slots=True)
class CodexChatgptAccessBundle:
    auth_id: UUID
    access_token: str
    chatgpt_account_id: str
    plan_type: str | None
    token_generation: int
    expires_at: datetime


def get_auth_configuration(
    session: Session,
    *,
    for_update: bool = False,
) -> CodexChatgptAuthConfiguration | None:
    query = select(CodexChatgptAuthConfiguration).where(
        CodexChatgptAuthConfiguration.scope == SYSTEM_SCOPE
    )
    if for_update:
        query = query.with_for_update().execution_options(populate_existing=True)
    return session.scalar(query)


def get_pending_attempt(
    session: Session,
    *,
    for_update: bool = False,
) -> CodexChatgptLoginAttempt | None:
    query = select(CodexChatgptLoginAttempt).where(
        CodexChatgptLoginAttempt.scope == SYSTEM_SCOPE,
        CodexChatgptLoginAttempt.state == LOGIN_PENDING,
    )
    if for_update:
        query = query.with_for_update().execution_options(populate_existing=True)
    return session.scalar(query)


def _lock_auth_operations(session: Session) -> CodexChatgptAuthOperationLock:
    """Lock the durable auth singleton, including when no auth row exists."""
    lock = session.scalar(
        select(CodexChatgptAuthOperationLock)
        .where(CodexChatgptAuthOperationLock.scope == SYSTEM_SCOPE)
        .with_for_update()
    )
    if lock is None:
        # Production migrations seed this row.  Lazy creation keeps metadata
        # based test databases and pre-seeded development databases usable.
        lock = CodexChatgptAuthOperationLock(scope=SYSTEM_SCOPE)
        session.add(lock)
        session.flush()
    return lock


def lock_codex_auth_operations(session: Session) -> CodexChatgptAuthOperationLock:
    """Lock the auth operation singleton for a multi-step service action."""
    return _lock_auth_operations(session)


def is_chatgpt_connected(session: Session) -> bool:
    auth = get_auth_configuration(session)
    return auth is not None and auth.state == CHATGPT_AUTH_CONNECTED


def _clear_attempt(attempt: CodexChatgptLoginAttempt) -> None:
    attempt.device_auth_id_ciphertext = None
    attempt.device_auth_id_nonce = None
    attempt.device_auth_id_key_version = None
    attempt.user_code = None
    attempt.poll_lease_until = None


def _attempt_view(
    attempt: CodexChatgptLoginAttempt,
    *,
    created: bool = True,
) -> DeviceLoginView:
    if not attempt.user_code:
        raise QfError("CODEX_CHATGPT_AUTH_CORRUPT", "Pending ChatGPT login is incomplete.", 503)
    return DeviceLoginView(
        login_id=attempt.id,
        status=attempt.state,
        verification_url=DEVICE_VERIFICATION_URL,
        user_code=attempt.user_code,
        expires_at=_aware(attempt.expires_at),
        poll_after_seconds=attempt.poll_interval_seconds,
        created=created,
    )


def _connected_auth_usable(
    auth: CodexChatgptAuthConfiguration,
    settings: Settings,
) -> bool:
    """Check that a CONNECTED row still has decryptable credentials."""
    try:
        _bundle_from_auth(auth, settings)
        _decrypt(
            auth.refresh_token_ciphertext,
            auth.refresh_token_nonce,
            auth.refresh_token_key_version,
            auth_id=auth.id,
            field="refresh_token",
            settings=settings,
        )
    except (QfError, SettingsError):
        return False
    return True


def _install_bundle(
    session: Session,
    settings: Settings,
    *,
    access_token: str,
    refresh_token: str,
    id_token: str | None,
    response: Mapping[str, Any],
    authenticated_at: datetime,
) -> CodexChatgptAuthConfiguration:
    auth = get_auth_configuration(session, for_update=True)
    if auth is None or auth.state == CHATGPT_AUTH_REAUTH_REQUIRED or not _connected_auth_usable(auth, settings):
        if auth is not None:
            session.delete(auth)
            session.flush()
        auth = CodexChatgptAuthConfiguration(
            id=uuid4(), scope=SYSTEM_SCOPE, state=CHATGPT_AUTH_CONNECTED, token_generation=1
        )
        session.add(auth)
    account_id, email, plan, expires_at = _metadata(
        response, access_token=access_token, id_token=id_token, now=authenticated_at
    )
    if not account_id:
        raise QfError(
            "CODEX_CHATGPT_LOGIN_FAILED",
            "ChatGPT authorization did not include an account identity.",
            502,
        )
    access = _encrypt(access_token, auth_id=auth.id, field="access_token", settings=settings)
    refresh = _encrypt(refresh_token, auth_id=auth.id, field="refresh_token", settings=settings)
    auth.state = CHATGPT_AUTH_CONNECTED
    auth.chatgpt_account_id = account_id
    auth.email = email
    auth.plan_type = plan
    auth.access_token_ciphertext = access.ciphertext
    auth.access_token_nonce = access.nonce
    auth.access_token_key_version = access.key_version
    auth.access_token_expires_at = expires_at
    auth.refresh_token_ciphertext = refresh.ciphertext
    auth.refresh_token_nonce = refresh.nonce
    auth.refresh_token_key_version = refresh.key_version
    auth.authenticated_at = authenticated_at
    auth.last_refresh_at = None
    auth.reauth_required_at = None
    return auth


def start_device_login(
    session: Session,
    settings: Settings,
    http_client: Any | None = None,
    *,
    now: Callable[[], datetime] = _now,
) -> DeviceLoginView:
    _lock_auth_operations(session)
    current = now()
    auth = get_auth_configuration(session)
    if auth is not None and auth.state == CHATGPT_AUTH_CONNECTED and _connected_auth_usable(auth, settings):
        raise QfError("CODEX_CHATGPT_ALREADY_CONNECTED", "ChatGPT is already connected.", 409)
    pending = get_pending_attempt(session, for_update=True)
    if pending is not None:
        if _aware(pending.expires_at) <= current:
            pending.state = LOGIN_EXPIRED
            pending.error_code = "expired"
            _clear_attempt(pending)
            session.flush()
        else:
            return _attempt_view(pending, created=False)

    # A successful poll installs credentials while holding the attempt row,
    # not the singleton operation lock.  Re-read the canonical auth row after
    # waiting on that row so start cannot create a second login from a stale
    # pre-poll snapshot.
    auth = get_auth_configuration(session, for_update=True)
    if auth is not None and auth.state == CHATGPT_AUTH_CONNECTED and _connected_auth_usable(auth, settings):
        raise QfError("CODEX_CHATGPT_ALREADY_CONNECTED", "ChatGPT is already connected.", 409)

    client, owned = _http_client(http_client)
    try:
        payload = _post_json(
            client,
            DEVICE_AUTH_USERCODE_URL,
            timeout=OAUTH_HTTP_TIMEOUT_SECONDS,
            json_body={"client_id": CODEX_OAUTH_CLIENT_ID},
        )
    except _OAuthFailure as exc:
        code = "CODEX_CHATGPT_LOGIN_FAILED" if exc.kind != "transient" else "CODEX_CHATGPT_LOGIN_FAILED"
        raise QfError(code, "ChatGPT device authorization is temporarily unavailable.", 503) from exc
    finally:
        if owned:
            client.close()

    device_auth_id = _safe_string(payload.get("device_auth_id"))
    user_code = _safe_string(payload.get("user_code"))
    if not device_auth_id or not user_code:
        raise QfError("CODEX_CHATGPT_LOGIN_FAILED", "ChatGPT device authorization was incomplete.", 502)
    interval = _as_positive_int(
        payload.get("interval"), 5, maximum=MAX_POLL_INTERVAL_SECONDS - POLLING_SAFETY_MARGIN_SECONDS
    ) + POLLING_SAFETY_MARGIN_SECONDS
    expires_in = _as_positive_int(payload.get("expires_in"), DEVICE_CODE_DEFAULT_EXPIRES_IN)
    expires_at = current + timedelta(seconds=expires_in)
    attempt = CodexChatgptLoginAttempt(
        id=uuid4(),
        scope=SYSTEM_SCOPE,
        state=LOGIN_PENDING,
        verification_url=DEVICE_VERIFICATION_URL,
        poll_interval_seconds=interval,
        expires_at=expires_at,
        next_poll_at=current + timedelta(seconds=interval),
    )
    encrypted = _encrypt(
        device_auth_id,
        auth_id=attempt.id,
        field="device_auth_id",
        settings=settings,
    )
    attempt.device_auth_id_ciphertext = encrypted.ciphertext
    attempt.device_auth_id_nonce = encrypted.nonce
    attempt.device_auth_id_key_version = encrypted.key_version
    attempt.user_code = user_code
    try:
        with session.begin_nested():
            session.add(attempt)
            session.flush()
    except IntegrityError as exc:
        session.expire_all()
        existing = get_pending_attempt(session, for_update=True)
        if existing is None:
            raise QfError("CODEX_CHATGPT_LOGIN_FAILED", "ChatGPT login could not be started.", 503) from exc
        return _attempt_view(existing, created=False)
    return _attempt_view(attempt)


def _pending_result(attempt: CodexChatgptLoginAttempt, *, now: datetime) -> DeviceLoginPollResult:
    remaining = max(1, int((_aware(attempt.expires_at) - now).total_seconds()))
    wait_until = _aware(attempt.next_poll_at)
    if attempt.poll_lease_until is not None:
        wait_until = max(wait_until, _aware(attempt.poll_lease_until))
    wait_seconds = max(1, int((wait_until - now).total_seconds()))
    return DeviceLoginPollResult(
        status=LOGIN_PENDING,
        expires_at=_aware(attempt.expires_at),
        poll_after_seconds=max(1, min(wait_seconds, remaining)),
    )


def _terminal_result(
    attempt: CodexChatgptLoginAttempt,
    *,
    transitioned: bool = False,
) -> DeviceLoginPollResult:
    return DeviceLoginPollResult(
        status=attempt.state,
        error_code=attempt.error_code,
        transitioned=transitioned,
    )


def _locked_login_attempt(session: Session, login_id: UUID) -> CodexChatgptLoginAttempt | None:
    return session.execute(
        select(CodexChatgptLoginAttempt)
        .where(CodexChatgptLoginAttempt.id == login_id)
        .with_for_update()
        .execution_options(populate_existing=True)
    ).scalar_one_or_none()


def _exchange_code(
    client: Any,
    authorization_code: str,
    code_verifier: str,
) -> Mapping[str, Any]:
    return _post_json(
        client,
        OAUTH_TOKEN_URL,
        timeout=OAUTH_HTTP_TIMEOUT_SECONDS,
        form={
            "grant_type": "authorization_code",
            "code": authorization_code,
            "redirect_uri": DEVICE_REDIRECT_URI,
            "client_id": CODEX_OAUTH_CLIENT_ID,
            "code_verifier": code_verifier,
        },
    )


def _poll_device(
    client: Any,
    *,
    device_auth_id: str,
    user_code: str,
) -> Mapping[str, Any]:
    return _post_json(
        client,
        DEVICE_AUTH_TOKEN_URL,
        timeout=OAUTH_HTTP_TIMEOUT_SECONDS,
        json_body={"device_auth_id": device_auth_id, "user_code": user_code},
    )


def _release_poll_lease(session: Session, login_id: UUID, lease_until: datetime) -> None:
    with session.begin():
        locked = _locked_login_attempt(session, login_id)
        # An upstream exchange can outlive this lease.  Do not clear a newer
        # lease acquired by another poll after the original one expired.
        if locked is not None and locked.poll_lease_until is not None and _aware(locked.poll_lease_until) == _aware(lease_until):
            locked.poll_lease_until = None


def poll_device_login(
    session: Session,
    settings: Settings,
    login_id: UUID,
    http_client: Any | None = None,
    *,
    now: Callable[[], datetime] = _now,
) -> DeviceLoginPollResult:
    """Poll once, committing the poll gate before network I/O.

    The first commit releases the attempt row lock.  The second lock after the
    token exchange is the late-poll/cancel race gate that prevents a cancelled
    login from installing credentials.
    """
    session.commit()
    current = now()
    attempt = session.execute(
        select(CodexChatgptLoginAttempt)
        .where(CodexChatgptLoginAttempt.id == login_id)
        .with_for_update()
    ).scalar_one_or_none()
    if attempt is None:
        raise QfError("CODEX_CHATGPT_LOGIN_NOT_FOUND", "ChatGPT login attempt was not found.", 404)
    if attempt.state != LOGIN_PENDING:
        return _terminal_result(attempt)
    if _aware(attempt.expires_at) <= current:
        attempt.state = LOGIN_EXPIRED
        attempt.error_code = "expired"
        _clear_attempt(attempt)
        session.commit()
        return _terminal_result(attempt)
    if attempt.poll_lease_until is not None and _aware(attempt.poll_lease_until) > current:
        return _pending_result(attempt, now=current)
    if _aware(attempt.next_poll_at) > current:
        return _pending_result(attempt, now=current)
    device_auth_id = _decrypt(
        attempt.device_auth_id_ciphertext,
        attempt.device_auth_id_nonce,
        attempt.device_auth_id_key_version,
        auth_id=attempt.id,
        field="device_auth_id",
        settings=settings,
    )
    if not attempt.user_code:
        raise QfError("CODEX_CHATGPT_AUTH_CORRUPT", "Pending ChatGPT login is incomplete.", 503)
    user_code = attempt.user_code
    attempt.next_poll_at = current + timedelta(seconds=attempt.poll_interval_seconds)
    lease_until = current + timedelta(seconds=POLL_LEASE_SECONDS)
    attempt.poll_lease_until = lease_until
    interval = attempt.poll_interval_seconds
    expires_at = _aware(attempt.expires_at)
    session.commit()

    client = None
    owned = False
    try:
        client, owned = _http_client(http_client)
        try:
            upstream = _poll_device(client, device_auth_id=device_auth_id, user_code=user_code)
        except _OAuthFailure as exc:
            if exc.kind == "pending":
                poll_after_seconds = interval
                if exc.code == "slow_down":
                    with session.begin():
                        locked = _locked_login_attempt(session, login_id)
                        if locked is not None and locked.state == LOGIN_PENDING:
                            locked.poll_interval_seconds = min(
                                MAX_POLL_INTERVAL_SECONDS,
                                locked.poll_interval_seconds + 5,
                            )
                            locked.next_poll_at = max(
                                _aware(locked.next_poll_at),
                                now() + timedelta(seconds=locked.poll_interval_seconds),
                            )
                            poll_after_seconds = locked.poll_interval_seconds
                return DeviceLoginPollResult(
                    status=LOGIN_PENDING,
                    expires_at=expires_at,
                    poll_after_seconds=poll_after_seconds,
                )
            if exc.kind == "expired":
                with session.begin():
                    locked = _locked_login_attempt(session, login_id)
                    if locked is not None and locked.state == LOGIN_PENDING:
                        locked.state = LOGIN_EXPIRED
                        locked.error_code = "expired"
                        _clear_attempt(locked)
                return DeviceLoginPollResult(status=LOGIN_EXPIRED, error_code="expired")
            if exc.kind == "transient":
                if exc.retry_after is not None:
                    with session.begin():
                        locked = _locked_login_attempt(session, login_id)
                        if locked is not None and locked.state == LOGIN_PENDING:
                            locked.next_poll_at = max(
                                _aware(locked.next_poll_at),
                                now() + timedelta(seconds=exc.retry_after),
                            )
                return DeviceLoginPollResult(
                    status=LOGIN_PENDING,
                    expires_at=expires_at,
                    poll_after_seconds=interval,
                )
            with session.begin():
                locked = _locked_login_attempt(session, login_id)
                if locked is not None and locked.state == LOGIN_PENDING:
                    locked.state = LOGIN_FAILED
                    locked.error_code = "authorization_failed"
                    _clear_attempt(locked)
            return DeviceLoginPollResult(status=LOGIN_FAILED, error_code="authorization_failed")

        authorization_code = _safe_string(upstream.get("authorization_code")) or _safe_string(upstream.get("code"))
        code_verifier = _safe_string(upstream.get("code_verifier"))
        if not authorization_code or not code_verifier:
            # Some implementations return pending as a 2xx response.
            return DeviceLoginPollResult(status=LOGIN_PENDING, expires_at=expires_at, poll_after_seconds=interval)
        token_payload = _exchange_code(client, authorization_code, code_verifier)
        access_token = _safe_string(token_payload.get("access_token"))
        refresh_token = _safe_string(token_payload.get("refresh_token"))
        id_token = _safe_string(token_payload.get("id_token"))
        if not access_token or not refresh_token:
            raise QfError("CODEX_CHATGPT_LOGIN_FAILED", "ChatGPT token exchange was incomplete.", 502)
    except QfError:
        with session.begin():
            locked = _locked_login_attempt(session, login_id)
            if locked is not None and locked.state == LOGIN_PENDING:
                locked.state = LOGIN_FAILED
                locked.error_code = "token_exchange_failed"
                _clear_attempt(locked)
        return DeviceLoginPollResult(status=LOGIN_FAILED, error_code="token_exchange_failed")
    except _OAuthFailure as exc:
        if exc.kind == "transient":
            if exc.retry_after is not None:
                with session.begin():
                    locked = _locked_login_attempt(session, login_id)
                    if locked is not None and locked.state == LOGIN_PENDING:
                        locked.next_poll_at = max(
                            _aware(locked.next_poll_at),
                            now() + timedelta(seconds=exc.retry_after),
                        )
            return DeviceLoginPollResult(status=LOGIN_PENDING, expires_at=expires_at, poll_after_seconds=interval)
        with session.begin():
            locked = _locked_login_attempt(session, login_id)
            if locked is not None and locked.state == LOGIN_PENDING:
                locked.state = LOGIN_FAILED
                locked.error_code = "token_exchange_failed"
                _clear_attempt(locked)
        return DeviceLoginPollResult(status=LOGIN_FAILED, error_code="token_exchange_failed")

    try:
        with session.begin():
            locked = _locked_login_attempt(session, login_id)
            if locked is None:
                raise QfError("CODEX_CHATGPT_LOGIN_NOT_FOUND", "ChatGPT login attempt was not found.", 404)
            current = now()
            if locked.state != LOGIN_PENDING:
                return _terminal_result(locked)
            if _aware(locked.expires_at) <= current:
                locked.state = LOGIN_EXPIRED
                locked.error_code = "expired"
                _clear_attempt(locked)
                return _terminal_result(locked)
            _install_bundle(
                session,
                settings,
                access_token=access_token,
                refresh_token=refresh_token,
                id_token=id_token,
                response=token_payload,
                authenticated_at=current,
            )
            session.flush()
            locked.state = LOGIN_SUCCEEDED
            locked.error_code = None
            _clear_attempt(locked)
            append_event(
                session,
                kind="CODEX_CHATGPT_AUTH_CONNECTED",
                aggregate_type="CODEX_CHATGPT_AUTH",
                aggregate_id=login_id,
                payload={"auth_mode": "CHATGPT"},
                actor_kind="LOCAL_OPERATOR",
            )
            return DeviceLoginPollResult(
                status=LOGIN_SUCCEEDED,
                auth=auth_status(session, settings),
                transitioned=True,
            )
    except QfError as exc:
        if exc.code == "CODEX_CHATGPT_LOGIN_NOT_FOUND":
            raise
        with session.begin():
            locked = _locked_login_attempt(session, login_id)
            if locked is not None and locked.state == LOGIN_PENDING:
                locked.state = LOGIN_FAILED
                locked.error_code = "credential_install_failed"
                _clear_attempt(locked)
        return DeviceLoginPollResult(status=LOGIN_FAILED, error_code="credential_install_failed")
    finally:
        try:
            if owned and client is not None:
                client.close()
        finally:
            _release_poll_lease(session, login_id, lease_until)


def cancel_device_login(session: Session, login_id: UUID) -> DeviceLoginPollResult:
    attempt = session.execute(
        select(CodexChatgptLoginAttempt)
        .where(CodexChatgptLoginAttempt.id == login_id)
        .with_for_update()
    ).scalar_one_or_none()
    if attempt is None:
        raise QfError("CODEX_CHATGPT_LOGIN_NOT_FOUND", "ChatGPT login attempt was not found.", 404)
    transitioned = attempt.state == LOGIN_PENDING
    if transitioned:
        attempt.state = LOGIN_CANCELLED
        attempt.error_code = None
        _clear_attempt(attempt)
    return _terminal_result(attempt, transitioned=transitioned)


def _clear_auth(auth: CodexChatgptAuthConfiguration) -> None:
    auth.access_token_ciphertext = None
    auth.access_token_nonce = None
    auth.access_token_key_version = None
    auth.access_token_expires_at = None
    auth.refresh_token_ciphertext = None
    auth.refresh_token_nonce = None
    auth.refresh_token_key_version = None


def _mark_reauth(auth: CodexChatgptAuthConfiguration, when: datetime) -> None:
    auth.state = CHATGPT_AUTH_REAUTH_REQUIRED
    auth.reauth_required_at = when
    auth.token_generation += 1
    _clear_auth(auth)


def disconnect_chatgpt(session: Session) -> bool:
    _lock_auth_operations(session)
    pending = get_pending_attempt(session, for_update=True)
    if pending is not None:
        pending.state = LOGIN_CANCELLED
        _clear_attempt(pending)
    auth = get_auth_configuration(session, for_update=True)
    transitioned = pending is not None or auth is not None
    if auth is not None:
        session.delete(auth)
    return transitioned


def _custom_provider_configuration(session: Session, settings: Settings) -> RuntimeConfiguration | None:
    runtime = session.scalar(select(RuntimeConfiguration).where(RuntimeConfiguration.scope == SYSTEM_SCOPE))
    if settings.codex_base_url or settings.codex_api_key:
        return runtime
    if runtime and (
        runtime.codex_base_url
        or codex_api_key_configured(runtime)
        or any(
            value is not None
            for value in (
                runtime.codex_api_key_ciphertext,
                runtime.codex_api_key_nonce,
                runtime.codex_api_key_key_version,
            )
        )
    ):
        return runtime
    return None


def _is_custom_provider(session: Session, settings: Settings) -> bool:
    return settings.codex_base_url is not None or settings.codex_api_key is not None or _custom_provider_configuration(session, settings) is not None


def _custom_provider_ready(session: Session, settings: Settings) -> bool:
    runtime = _custom_provider_configuration(session, settings)
    if runtime is not None:
        key_parts = (
            runtime.codex_api_key_ciphertext,
            runtime.codex_api_key_nonce,
            runtime.codex_api_key_key_version,
        )
        if any(value is not None for value in key_parts):
            return codex_api_key_configured(runtime) and codex_api_key_decryptable(runtime, settings)
    return True


def codex_auth_readiness(session: Session, settings: Settings) -> tuple[bool, str]:
    """Return the non-secret Codex readiness state used by health/readiness APIs."""
    if _is_custom_provider(session, settings):
        ready = _custom_provider_ready(session, settings)
        return ready, "CUSTOM_PROVIDER" if ready else "CUSTOM_PROVIDER_REAUTH_REQUIRED"
    auth = get_auth_configuration(session)
    if auth is not None and auth.state == CHATGPT_AUTH_CONNECTED:
        return (
            (True, CHATGPT_AUTH_CONNECTED)
            if _connected_auth_usable(auth, settings)
            else (False, CHATGPT_AUTH_REAUTH_REQUIRED)
        )
    if auth is not None and auth.state == CHATGPT_AUTH_REAUTH_REQUIRED:
        return False, CHATGPT_AUTH_REAUTH_REQUIRED
    return False, "DISCONNECTED"


def auth_status(session: Session, settings: Settings) -> dict[str, Any]:
    auth = get_auth_configuration(session)
    pending = get_pending_attempt(session)
    state = auth.state if auth is not None else "DISCONNECTED"
    if auth is not None and auth.state == CHATGPT_AUTH_CONNECTED and not _connected_auth_usable(auth, settings):
        state = CHATGPT_AUTH_REAUTH_REQUIRED
    return {
        "state": state,
        "active": state == CHATGPT_AUTH_CONNECTED and not _is_custom_provider(session, settings),
        "email": auth.email if auth is not None else None,
        "plan_type": auth.plan_type if auth is not None else None,
        "authenticated_at": auth.authenticated_at.isoformat() if auth and auth.authenticated_at else None,
        "last_refresh_at": auth.last_refresh_at.isoformat() if auth and auth.last_refresh_at else None,
        "reauth_required_at": auth.reauth_required_at.isoformat() if auth and auth.reauth_required_at else None,
        "pending_login": (
            {
                "login_id": str(pending.id),
                "expires_at": _aware(pending.expires_at).isoformat(),
                "poll_after_seconds": pending.poll_interval_seconds,
            }
            if pending is not None
            else None
        ),
        "legacy_auth_file_present": _legacy_auth_path(settings).exists(),
    }


def _valid_chatgpt_tokens(document: object) -> tuple[str, str, str | None] | None:
    if not isinstance(document, Mapping):
        return None
    auth_mode = _safe_string(document.get("auth_mode"))
    if auth_mode not in {"chatgpt", "chatgpt_auth", "chatgptAuthTokens"}:
        return None
    tokens = document.get("tokens")
    if not isinstance(tokens, Mapping):
        return None
    access = _safe_string(tokens.get("access_token"))
    refresh = _safe_string(tokens.get("refresh_token"))
    id_token = _safe_string(tokens.get("id_token"))
    return (access, refresh, id_token) if access and refresh else None


def _legacy_auth_path(settings: Settings) -> Path:
    return settings.codex_home / "auth.json"


def remove_legacy_auth_file(settings: Settings) -> None:
    """Remove the legacy auth source, failing closed for unsafe cleanup."""
    path = _legacy_auth_path(settings)
    if not path.exists():
        return
    try:
        if path.is_symlink() or not path.is_file():
            raise OSError("legacy auth path is not a regular file")
        try:
            path.unlink()
        except FileNotFoundError:
            pass
    except OSError as exc:
        raise QfError(
            "CODEX_LEGACY_AUTH_CLEANUP_FAILED",
            "The legacy Codex auth file could not be removed; official Codex login is disabled.",
            503,
        ) from exc


def _read_legacy_auth(path: Path) -> tuple[str, str, str | None, Mapping[str, Any]] | None:
    if not path.is_file() or path.is_symlink():
        return None
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        return None
    tokens = _valid_chatgpt_tokens(document)
    if tokens is None:
        return None
    access, refresh, id_token = tokens
    return access, refresh, id_token, document if isinstance(document, Mapping) else {}


def initialize_codex_auth(factory: Any, settings: Settings) -> None:
    """Import legacy ChatGPT auth once, then remove the old file.

    An invalid legacy file is left untouched so an operator can inspect or
    remove it; it can never make a Mission appear authenticated.
    """
    path = _legacy_auth_path(settings)
    should_cleanup = False
    with factory.begin() as session:
        _lock_auth_operations(session)
        auth = get_auth_configuration(session)
        if auth is not None and path.exists():
            should_cleanup = True
        elif auth is None and path.is_file():
            parsed = _read_legacy_auth(path)
            if parsed is not None:
                access, refresh, id_token, document = parsed
                account_id, _, _, _ = _metadata(
                    document,
                    access_token=access,
                    id_token=id_token,
                    now=_now(),
                )
                if account_id:
                    auth = _install_bundle(
                        session,
                        settings,
                        access_token=access,
                        refresh_token=refresh,
                        id_token=id_token,
                        response=document,
                        authenticated_at=_now(),
                    )
                    logger.info("Imported legacy Codex ChatGPT authentication into PostgreSQL")
                    should_cleanup = True
        # A canonical row always wins.  The file is only a one-time import source.
    if should_cleanup and path.exists():
        try:
            path.unlink()
        except FileNotFoundError:
            pass
        except OSError as exc:
            raise QfError(
                "CODEX_LEGACY_AUTH_CLEANUP_FAILED",
                "The legacy Codex auth file could not be removed; official Codex login is disabled.",
                503,
            ) from exc


def _bundle_from_auth(auth: CodexChatgptAuthConfiguration, settings: Settings) -> CodexChatgptAccessBundle:
    if auth.state != CHATGPT_AUTH_CONNECTED or not auth.chatgpt_account_id or auth.access_token_expires_at is None:
        raise QfError("CODEX_CHATGPT_AUTH_REAUTH_REQUIRED", "ChatGPT authentication requires re-authentication.", 503)
    token = _decrypt(
        auth.access_token_ciphertext,
        auth.access_token_nonce,
        auth.access_token_key_version,
        auth_id=auth.id,
        field="access_token",
        settings=settings,
    )
    return CodexChatgptAccessBundle(
        auth_id=auth.id,
        access_token=token,
        chatgpt_account_id=auth.chatgpt_account_id,
        plan_type=auth.plan_type,
        token_generation=auth.token_generation,
        expires_at=_aware(auth.access_token_expires_at),
    )


def get_valid_access_bundle(
    session: Session,
    settings: Settings,
    http_client: Any | None = None,
    *,
    force_refresh: bool = False,
    observed_generation: int | None = None,
    expected_auth_id: UUID | None = None,
    expected_account_id: str | None = None,
    now: Callable[[], datetime] = _now,
) -> CodexChatgptAccessBundle:
    """Return an access token, serializing refreshes on the auth DB row."""
    current = now()
    auth = get_auth_configuration(session)
    if auth is None:
        if expected_auth_id is not None:
            raise QfError(
                "CODEX_CHATGPT_AUTH_REAUTH_REQUIRED",
                "The ChatGPT authentication used by this Mission is no longer available.",
                503,
            )
        raise QfError("CODEX_CHATGPT_AUTH_REQUIRED", "ChatGPT authentication is required.", 503)
    if auth.state != CHATGPT_AUTH_CONNECTED:
        raise QfError("CODEX_CHATGPT_AUTH_REAUTH_REQUIRED", "ChatGPT authentication requires re-authentication.", 503)
    if expected_auth_id is not None and auth.id != expected_auth_id:
        raise QfError(
            "CODEX_CHATGPT_AUTH_REAUTH_REQUIRED",
            "The ChatGPT authentication used by this Mission is no longer available.",
            503,
        )
    if expected_account_id is not None and auth.chatgpt_account_id != expected_account_id:
        raise QfError(
            "CODEX_CHATGPT_AUTH_REAUTH_REQUIRED",
            "The ChatGPT account used by this Mission is no longer available.",
            503,
        )
    initial_generation = auth.token_generation
    if observed_generation is not None and auth.token_generation > observed_generation:
        return _bundle_from_auth(auth, settings)
    if not force_refresh and auth.access_token_expires_at is not None and _aware(auth.access_token_expires_at) - current > timedelta(seconds=TOKEN_REFRESH_BUFFER_SECONDS):
        return _bundle_from_auth(auth, settings)

    locked = session.execute(
        select(CodexChatgptAuthConfiguration)
        .where(CodexChatgptAuthConfiguration.scope == SYSTEM_SCOPE)
        .with_for_update()
        .execution_options(populate_existing=True)
    ).scalar_one_or_none()
    if locked is None or locked.state != CHATGPT_AUTH_CONNECTED:
        raise QfError("CODEX_CHATGPT_AUTH_REAUTH_REQUIRED", "ChatGPT authentication requires re-authentication.", 503)
    if expected_auth_id is not None and locked.id != expected_auth_id:
        raise QfError(
            "CODEX_CHATGPT_AUTH_REAUTH_REQUIRED",
            "The ChatGPT authentication used by this Mission is no longer available.",
            503,
        )
    if expected_account_id is not None and locked.chatgpt_account_id != expected_account_id:
        raise QfError(
            "CODEX_CHATGPT_AUTH_REAUTH_REQUIRED",
            "The ChatGPT account used by this Mission is no longer available.",
            503,
        )
    if locked.token_generation != initial_generation:
        return _bundle_from_auth(locked, settings)
    if observed_generation is not None and locked.token_generation > observed_generation:
        return _bundle_from_auth(locked, settings)
    current = now()
    if not force_refresh and locked.access_token_expires_at is not None and _aware(locked.access_token_expires_at) - current > timedelta(seconds=TOKEN_REFRESH_BUFFER_SECONDS):
        return _bundle_from_auth(locked, settings)
    refresh_token = _decrypt(
        locked.refresh_token_ciphertext,
        locked.refresh_token_nonce,
        locked.refresh_token_key_version,
        auth_id=locked.id,
        field="refresh_token",
        settings=settings,
    )
    client, owned = _http_client(http_client)
    try:
        try:
            payload = _post_json(
                client,
                OAUTH_TOKEN_URL,
                timeout=APP_SERVER_REFRESH_HTTP_TIMEOUT_SECONDS if force_refresh else OAUTH_HTTP_TIMEOUT_SECONDS,
                form={
                    "grant_type": "refresh_token",
                    "refresh_token": refresh_token,
                    "client_id": CODEX_OAUTH_CLIENT_ID,
                },
            )
        except _OAuthFailure as exc:
            if exc.kind == "transient":
                raise QfError(
                    "CODEX_CHATGPT_AUTH_REFRESH_FAILED",
                    "ChatGPT authentication refresh is temporarily unavailable.",
                    503,
                ) from exc
            _mark_reauth(locked, now())
            session.flush()
            session.commit()
            raise QfError(
                "CODEX_CHATGPT_AUTH_REAUTH_REQUIRED",
                "ChatGPT authentication requires re-authentication.",
                503,
            ) from exc
        access_token = _safe_string(payload.get("access_token"))
        if not access_token:
            raise QfError("CODEX_CHATGPT_AUTH_REFRESH_FAILED", "ChatGPT refresh response was incomplete.", 503)
        replacement_refresh = _safe_string(payload.get("refresh_token")) or refresh_token
        id_token = _safe_string(payload.get("id_token"))
        account_id, email, plan, expires_at = _metadata(
            payload, access_token=access_token, id_token=id_token, now=now()
        )
        if account_id and account_id != locked.chatgpt_account_id:
            _mark_reauth(locked, now())
            session.flush()
            session.commit()
            raise QfError("CODEX_CHATGPT_AUTH_REAUTH_REQUIRED", "ChatGPT account identity changed; re-authentication is required.", 503)
        access = _encrypt(access_token, auth_id=locked.id, field="access_token", settings=settings)
        refresh = _encrypt(replacement_refresh, auth_id=locked.id, field="refresh_token", settings=settings)
        locked.access_token_ciphertext = access.ciphertext
        locked.access_token_nonce = access.nonce
        locked.access_token_key_version = access.key_version
        locked.access_token_expires_at = expires_at
        locked.refresh_token_ciphertext = refresh.ciphertext
        locked.refresh_token_nonce = refresh.nonce
        locked.refresh_token_key_version = refresh.key_version
        locked.email = email or locked.email
        locked.plan_type = plan or locked.plan_type
        locked.last_refresh_at = now()
        locked.token_generation += 1
        return _bundle_from_auth(locked, settings)
    finally:
        if owned:
            client.close()


def legacy_auth_file_present(settings: Settings) -> bool:
    return _legacy_auth_path(settings).exists()


__all__ = [
    "APP_SERVER_REFRESH_HTTP_TIMEOUT_SECONDS",
    "CODEX_OAUTH_CLIENT_ID",
    "CodexChatgptAccessBundle",
    "DEVICE_AUTH_TOKEN_URL",
    "DEVICE_AUTH_USERCODE_URL",
    "DEVICE_REDIRECT_URI",
    "DEVICE_VERIFICATION_URL",
    "DeviceLoginPollResult",
    "DeviceLoginView",
    "OAUTH_TOKEN_URL",
    "auth_status",
    "cancel_device_login",
    "codex_auth_readiness",
    "disconnect_chatgpt",
    "get_auth_configuration",
    "get_pending_attempt",
    "get_valid_access_bundle",
    "is_chatgpt_connected",
    "initialize_codex_auth",
    "legacy_auth_file_present",
    "lock_codex_auth_operations",
    "poll_device_login",
    "remove_legacy_auth_file",
    "start_device_login",
]
