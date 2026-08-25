"""Single-operator browser and machine authentication primitives."""

from __future__ import annotations

import base64
import binascii
import json
import re
import secrets
import time
from collections.abc import Callable
from dataclasses import dataclass
from threading import Lock
from typing import Literal

import pyotp
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from fastapi import Request, Response

from errors import QfError
from settings import Settings, SettingsError, canonicalize_http_origin

SESSION_COOKIE_NAME = "quazonai_session"
TRUSTED_BROWSER_COOKIE_NAME = "quazonai_trusted_browser"
COOKIE_VERSION = 1
COOKIE_NONCE_BYTES = 12
LOGIN_MIN_INTERVAL_SECONDS = 1.0
LOGIN_BASE_BACKOFF_SECONDS = 1.0
LOGIN_MAX_BACKOFF_SECONDS = 5.0
LOGIN_STATE_RETENTION_SECONDS = 15 * 60.0
LOGIN_MAX_TRACKED_SOURCES = 2048
_SAFE_METHODS = frozenset({"GET", "HEAD", "OPTIONS"})
_PUBLIC_OPERATOR_ROUTES = frozenset(
    {
        ("GET", "/api/v1/system/health"),
        ("POST", "/api/v1/auth/login"),
        ("GET", "/api/v1/auth/session"),
        ("POST", "/api/v1/auth/logout"),
    }
)
_DOWNSTREAM_ROUTE = re.compile(
    r"^/api/v1/handoffs/[^/]+/(?P<action>claim|accept|reject|package|feedback)$"
)
_DOWNSTREAM_METHODS = {
    "claim": "POST",
    "accept": "POST",
    "reject": "POST",
    "package": "GET",
    "feedback": "POST",
}


@dataclass(frozen=True, slots=True)
class OperatorIdentity:
    username: str
    source: Literal["session", "trusted_browser", "machine"]
    renew_session: bool = False


@dataclass(slots=True)
class _LoginAttemptState:
    next_allowed_at: float
    failures: int
    last_seen_at: float


class OperatorLoginLimiter:
    """Bound credential verification rate per observed network source.

    The limiter is deliberately process-local and short-lived: it reduces online
    guessing without creating a durable account lockout that can strand the one
    local operator. Blocked requests receive the same generic login failure as an
    incorrect factor and never execute password/TOTP verification.
    """

    def __init__(
        self,
        *,
        clock: Callable[[], float] = time.monotonic,
        minimum_interval_seconds: float = LOGIN_MIN_INTERVAL_SECONDS,
        base_backoff_seconds: float = LOGIN_BASE_BACKOFF_SECONDS,
        maximum_backoff_seconds: float = LOGIN_MAX_BACKOFF_SECONDS,
        retention_seconds: float = LOGIN_STATE_RETENTION_SECONDS,
        max_sources: int = LOGIN_MAX_TRACKED_SOURCES,
    ) -> None:
        self._clock = clock
        self._minimum_interval_seconds = minimum_interval_seconds
        self._base_backoff_seconds = base_backoff_seconds
        self._maximum_backoff_seconds = maximum_backoff_seconds
        self._retention_seconds = retention_seconds
        self._max_sources = max_sources
        self._states: dict[str, _LoginAttemptState] = {}
        self._lock = Lock()

    def _prune(self, now: float) -> None:
        expired = [
            source
            for source, state in self._states.items()
            if now - state.last_seen_at > self._retention_seconds
        ]
        for source in expired:
            self._states.pop(source, None)
        while len(self._states) >= self._max_sources:
            oldest = min(self._states, key=lambda source: self._states[source].last_seen_at)
            self._states.pop(oldest, None)

    def allow_attempt(self, source: str) -> bool:
        now = self._clock()
        with self._lock:
            self._prune(now)
            state = self._states.get(source)
            if state is not None and state.next_allowed_at > now:
                return False
            if state is None:
                state = _LoginAttemptState(
                    next_allowed_at=now,
                    failures=0,
                    last_seen_at=now,
                )
                self._states[source] = state
            state.next_allowed_at = now + self._minimum_interval_seconds
            state.last_seen_at = now
            return True

    def record_failure(self, source: str) -> None:
        now = self._clock()
        with self._lock:
            self._prune(now)
            state = self._states.get(source)
            if state is None:
                state = _LoginAttemptState(
                    next_allowed_at=now,
                    failures=0,
                    last_seen_at=now,
                )
                self._states[source] = state
            state.failures = min(state.failures + 1, 32)
            exponent = min(state.failures - 1, 16)
            backoff = min(
                self._maximum_backoff_seconds,
                self._base_backoff_seconds * (2**exponent),
            )
            state.next_allowed_at = max(state.next_allowed_at, now + backoff)
            state.last_seen_at = now

    def record_success(self, source: str) -> None:
        with self._lock:
            self._states.pop(source, None)


class OperatorAuthRuntime:
    """Process-local coordination for login throttling and active streams."""

    def __init__(self, *, login_limiter: OperatorLoginLimiter | None = None) -> None:
        self.login_limiter = login_limiter or OperatorLoginLimiter()
        self._stream_generation = 0
        self._stream_lock = Lock()

    def stream_generation(self) -> int:
        with self._stream_lock:
            return self._stream_generation

    def revoke_active_streams(self) -> None:
        with self._stream_lock:
            self._stream_generation += 1


class _InvalidCookie(ValueError):
    pass


def login_source_key(request: Request) -> str:
    """Return a bounded source key without trusting arbitrary forwarding headers."""
    if request.client is None or not request.client.host:
        return "unknown"
    return request.client.host[:255]


def _urlsafe_encode(value: bytes) -> str:
    return base64.urlsafe_b64encode(value).rstrip(b"=").decode("ascii")


def _urlsafe_decode(value: str) -> bytes:
    padding = "=" * ((4 - len(value) % 4) % 4)
    try:
        return base64.b64decode(value + padding, altchars=b"-_", validate=True)
    except (ValueError, binascii.Error) as exc:
        raise _InvalidCookie from exc


def _cookie_aad(kind: str) -> bytes:
    return f"quazonai|operator-auth|cookie={kind}|version={COOKIE_VERSION}".encode("utf-8")


def _issue_cookie(settings: Settings, *, kind: str, ttl_seconds: int) -> str:
    assert settings.operator_username is not None
    issued_at = int(time.time())
    payload = json.dumps(
        {
            "v": COOKIE_VERSION,
            "kind": kind,
            "sub": settings.operator_username,
            "iat": issued_at,
            "exp": issued_at + ttl_seconds,
        },
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    nonce = secrets.token_bytes(COOKIE_NONCE_BYTES)
    ciphertext = AESGCM(settings.auth_cookie_key_bytes()).encrypt(
        nonce,
        payload,
        _cookie_aad(kind),
    )
    return _urlsafe_encode(nonce + ciphertext)


def _read_cookie(settings: Settings, value: str | None, *, kind: str) -> str | None:
    if not value:
        return None
    try:
        encoded = _urlsafe_decode(value)
        if len(encoded) <= COOKIE_NONCE_BYTES:
            raise _InvalidCookie
        nonce = encoded[:COOKIE_NONCE_BYTES]
        ciphertext = encoded[COOKIE_NONCE_BYTES:]
        plaintext = AESGCM(settings.auth_cookie_key_bytes()).decrypt(
            nonce,
            ciphertext,
            _cookie_aad(kind),
        )
        payload = json.loads(plaintext)
        if not isinstance(payload, dict):
            raise _InvalidCookie
        if payload.get("v") != COOKIE_VERSION or payload.get("kind") != kind:
            raise _InvalidCookie
        username = payload.get("sub")
        expires_at = payload.get("exp")
        if not isinstance(username, str) or not isinstance(expires_at, int):
            raise _InvalidCookie
        if expires_at <= int(time.time()):
            return None
        if settings.operator_username is None or not secrets.compare_digest(
            username, settings.operator_username
        ):
            return None
        return username
    except (ValueError, TypeError, json.JSONDecodeError, _InvalidCookie):
        return None
    except Exception:  # noqa: BLE001 - invalid/tampered cookies collapse to anonymous
        return None


def authenticate_login(
    settings: Settings,
    *,
    username: str,
    password: str,
    totp_code: str,
) -> bool:
    """Check username, password and RFC 6238 TOTP without exposing which check failed."""
    if not settings.auth_enabled:
        return False
    assert settings.operator_username is not None
    assert settings.operator_password is not None
    assert settings.operator_totp_secret is not None

    username_valid = secrets.compare_digest(username, settings.operator_username)
    password_valid = secrets.compare_digest(password, settings.operator_password)
    normalized_code = "".join(totp_code.split())
    code_shape_valid = len(normalized_code) == 6 and normalized_code.isdigit()
    totp_valid = False
    if code_shape_valid:
        try:
            totp_valid = pyotp.TOTP(settings.operator_totp_secret).verify(
                normalized_code,
                valid_window=1,
            )
        except (TypeError, ValueError):
            totp_valid = False
    return username_valid and password_valid and totp_valid


def authenticate_machine(settings: Settings, authorization: str | None) -> OperatorIdentity | None:
    if not settings.auth_enabled or settings.api_token is None or authorization is None:
        return None
    scheme, separator, token = authorization.partition(" ")
    if not separator or scheme.casefold() != "bearer" or not token.strip():
        return None
    if not secrets.compare_digest(token.strip(), settings.api_token):
        return None
    assert settings.operator_username is not None
    return OperatorIdentity(username=settings.operator_username, source="machine")


def authenticate_browser(request: Request, settings: Settings) -> OperatorIdentity | None:
    if not settings.auth_enabled:
        return None
    username = _read_cookie(
        settings,
        request.cookies.get(SESSION_COOKIE_NAME),
        kind="session",
    )
    if username is not None:
        return OperatorIdentity(username=username, source="session")
    username = _read_cookie(
        settings,
        request.cookies.get(TRUSTED_BROWSER_COOKIE_NAME),
        kind="trusted-browser",
    )
    if username is not None:
        return OperatorIdentity(
            username=username,
            source="trusted_browser",
            renew_session=True,
        )
    return None


def reauthenticate_operator_request(request: Request, settings: Settings) -> bool:
    """Revalidate a long-lived request against current credentials and key state."""
    if not settings.auth_enabled:
        return True
    if authenticate_machine(settings, request.headers.get("authorization")) is not None:
        return True
    return authenticate_browser(request, settings) is not None


def has_valid_trusted_browser(request: Request, settings: Settings) -> bool:
    """Return whether this request carries a currently valid trusted-browser credential."""
    if not settings.auth_enabled:
        return False
    return (
        _read_cookie(
            settings,
            request.cookies.get(TRUSTED_BROWSER_COOKIE_NAME),
            kind="trusted-browser",
        )
        is not None
    )


def set_session_cookie(response: Response, settings: Settings) -> None:
    token = _issue_cookie(
        settings,
        kind="session",
        ttl_seconds=settings.auth_session_ttl_seconds,
    )
    response.set_cookie(
        SESSION_COOKIE_NAME,
        token,
        max_age=settings.auth_session_ttl_seconds,
        httponly=True,
        secure=settings.auth_cookie_secure,
        samesite="strict",
        path="/",
    )


def set_trusted_browser_cookie(response: Response, settings: Settings) -> None:
    ttl_seconds = settings.auth_trusted_browser_ttl_days * 24 * 60 * 60
    token = _issue_cookie(settings, kind="trusted-browser", ttl_seconds=ttl_seconds)
    response.set_cookie(
        TRUSTED_BROWSER_COOKIE_NAME,
        token,
        max_age=ttl_seconds,
        httponly=True,
        secure=settings.auth_cookie_secure,
        samesite="strict",
        path="/",
    )


def _delete_cookie(response: Response, settings: Settings, name: str) -> None:
    response.delete_cookie(
        name,
        path="/",
        secure=settings.auth_cookie_secure,
        httponly=True,
        samesite="strict",
    )


def clear_trusted_browser_cookie(response: Response, settings: Settings) -> None:
    _delete_cookie(response, settings, TRUSTED_BROWSER_COOKIE_NAME)


def clear_auth_cookies(response: Response, settings: Settings) -> None:
    _delete_cookie(response, settings, SESSION_COOKIE_NAME)
    clear_trusted_browser_cookie(response, settings)


def require_same_origin(request: Request, settings: Settings) -> None:
    if request.method.upper() in _SAFE_METHODS or not settings.auth_enabled:
        return
    expected = settings.canonical_auth_public_origin
    supplied = request.headers.get("origin")
    try:
        origin = (
            canonicalize_http_origin(supplied, name="Origin")
            if supplied is not None
            else None
        )
    except SettingsError:
        origin = None
    if (
        expected is None
        or origin is None
        or not secrets.compare_digest(origin, expected)
    ):
        raise QfError(
            "AUTH_ORIGIN_REJECTED",
            "The request origin is not allowed for browser-authenticated mutations.",
            403,
        )



def is_operator_auth_exempt(method: str, path: str) -> bool:
    """Return whether one exact method/path belongs outside Operator authentication.

    Downstream-owned Handoff routes remain authenticated by their per-downstream
    service token. Matching both method and path prevents a future Operator route
    that reuses one path with a different HTTP method from becoming public.
    """
    normalized_method = method.upper()
    if (normalized_method, path) in _PUBLIC_OPERATOR_ROUTES:
        return True
    match = _DOWNSTREAM_ROUTE.fullmatch(path)
    if match is None:
        return False
    return _DOWNSTREAM_METHODS[match.group("action")] == normalized_method
