"""Single-operator browser and machine authentication primitives."""

from __future__ import annotations

import base64
import binascii
import ipaddress
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
from settings import (
    MAX_AUTH_TRUSTED_BROWSER_TTL_DAYS,
    Settings,
    SettingsError,
    canonicalize_http_origin,
    validate_machine_api_token,
)

SESSION_COOKIE_NAME = "quazonai_session"
TRUSTED_BROWSER_COOKIE_NAME = "quazonai_trusted_browser"
LOGOUT_BARRIER_COOKIE_NAME = "quazonai_logout_barrier"
STREAM_ADMISSION_GENERATION_STATE_ATTRIBUTE = "operator_auth_stream_generation"
COOKIE_VERSION = 1
COOKIE_NONCE_BYTES = 12
LOGOUT_BARRIER_MAX_AGE_SECONDS = MAX_AUTH_TRUSTED_BROWSER_TTL_DAYS * 24 * 60 * 60
LOGIN_MIN_INTERVAL_SECONDS = 1.0
LOGIN_BASE_BACKOFF_SECONDS = 1.0
LOGIN_MAX_BACKOFF_SECONDS = 5.0
LOGIN_STATE_RETENTION_SECONDS = 15 * 60.0
LOGIN_MAX_TRACKED_SOURCES = 2048
MAX_FORWARDED_FOR_HEADER_CHARACTERS = 2048
MAX_FORWARDED_FOR_CHAIN_HOPS = 32
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
    """Coordinate login throttling, TOTP replay, logout revocation, and renewal."""

    def __init__(self, *, login_limiter: OperatorLoginLimiter | None = None) -> None:
        self.login_limiter = login_limiter or OperatorLoginLimiter()
        self._stream_generation = 0
        self._stream_lock = Lock()
        self._accepted_totp_steps: set[int] = set()
        self._totp_lock = Lock()

    def stream_generation(self) -> int:
        with self._stream_lock:
            return self._stream_generation

    def revoke_active_streams(self) -> None:
        with self._stream_lock:
            self._stream_generation += 1

    def renew_session_if_current(
        self,
        response: Response,
        settings: Settings,
        *,
        generation: int,
    ) -> bool:
        """Issue a trusted-browser session only when logout has not won the race.

        The generation comparison and response mutation share the logout lock so a
        concurrent logout cannot advance the generation between this check and the
        ``Set-Cookie`` write.
        """
        with self._stream_lock:
            if self._stream_generation != generation:
                return False
            set_session_cookie(response, settings)
            return True

    def consume_totp_step(self, step: int, *, current_step: int) -> bool:
        """Atomically accept one RFC 6238 time step at most once per API process."""
        with self._totp_lock:
            # A ±1 verification window can only reference these nearby steps. Keeping
            # two older steps avoids replay after a small clock movement while bounding memory.
            self._accepted_totp_steps = {
                accepted
                for accepted in self._accepted_totp_steps
                if accepted >= current_step - 2
            }
            if step in self._accepted_totp_steps:
                return False
            self._accepted_totp_steps.add(step)
            return True


class _InvalidCookie(ValueError):
    pass


def _parse_network_address(value: str) -> ipaddress.IPv4Address | ipaddress.IPv6Address | None:
    """Accept only an unambiguous bare IP address from proxy metadata."""
    if not value or "%" in value:
        return None
    try:
        return ipaddress.ip_address(value)
    except ValueError:
        return None


def _is_trusted_proxy(
    address: ipaddress.IPv4Address | ipaddress.IPv6Address,
    settings: Settings,
) -> bool:
    return any(
        address.version == network.version and address in network
        for network in settings.auth_trusted_proxy_cidrs
    )


def _forwarded_client_address(
    request: Request,
    settings: Settings,
) -> ipaddress.IPv4Address | ipaddress.IPv6Address | None:
    """Return the nearest untrusted client from one trusted proxy chain.

    Each trusted proxy must append its observed peer to ``X-Forwarded-For``.
    Walking the chain right-to-left therefore ignores client-supplied entries
    and known intermediary hops. Ambiguous or malformed forwarding metadata is
    deliberately ignored rather than becoming a caller-controlled limiter key.
    """
    values = request.headers.getlist("x-forwarded-for")
    if len(values) != 1 or len(values[0]) > MAX_FORWARDED_FOR_HEADER_CHARACTERS:
        return None

    addresses: list[ipaddress.IPv4Address | ipaddress.IPv6Address] = []
    raw_addresses = values[0].split(",")
    if not 1 <= len(raw_addresses) <= MAX_FORWARDED_FOR_CHAIN_HOPS:
        return None
    for raw_address in raw_addresses:
        address = _parse_network_address(raw_address.strip())
        if address is None:
            return None
        addresses.append(address)

    while addresses and _is_trusted_proxy(addresses[-1], settings):
        addresses.pop()
    return addresses[-1] if addresses else None


def login_source_key(request: Request, settings: Settings) -> str:
    """Return a limiter key from a direct peer or explicitly trusted proxy.

    Client-provided ``X-Forwarded-For`` is ignored unless the ASGI direct peer
    belongs to a configured trusted-proxy network. Missing or malformed proxy
    metadata falls back to the direct peer, preserving the conservative legacy
    behavior instead of weakening credential throttling.
    """
    if request.client is None or not request.client.host:
        return "unknown"
    peer = _parse_network_address(request.client.host)
    if peer is None:
        return request.client.host[:255]
    if not _is_trusted_proxy(peer, settings):
        return str(peer)
    forwarded_client = _forwarded_client_address(request, settings)
    return str(forwarded_client) if forwarded_client is not None else str(peer)


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


def _constant_time_text_equal(left: str, right: str) -> bool:
    """Compare Unicode credentials while collapsing unencodable request text to false."""
    try:
        left_bytes = left.encode("utf-8")
        right_bytes = right.encode("utf-8")
    except UnicodeEncodeError:
        return False
    return secrets.compare_digest(left_bytes, right_bytes)


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
        if settings.operator_username is None or not _constant_time_text_equal(
            username, settings.operator_username
        ):
            return None
        return username
    except (ValueError, TypeError, json.JSONDecodeError, _InvalidCookie):
        return None
    except Exception:  # noqa: BLE001 - invalid/tampered cookies collapse to anonymous
        return None


def _request_cookie_values(request: Request, name: str) -> tuple[str, ...]:
    """Return every value for one cookie name without losing duplicate fields.

    A sibling subdomain can attach a parent-Domain cookie with the same name as a
    host-only cookie. ``Request.cookies`` keeps only one duplicate value, so use the
    raw Cookie fields when an authenticated cookie must not be shadowed by a forgery.
    """
    values: list[str] = []
    for header in request.headers.getlist("cookie"):
        for pair in header.split(";"):
            cookie_name, separator, value = pair.strip().partition("=")
            if separator and cookie_name == name:
                values.append(value)
    return tuple(values)


def _read_request_cookie(
    request: Request,
    settings: Settings,
    *,
    name: str,
    kind: str,
) -> str | None:
    """Return a valid cookie of ``kind`` without allowing a duplicate to shadow it.

    A parent-Domain cookie can be sent beside a host-only cookie with the same
    name. Raw Cookie field order is not a security boundary, so scan every
    value and accept the first one that verifies under the expected AEAD kind.
    """
    for value in _request_cookie_values(request, name):
        username = _read_cookie(settings, value, kind=kind)
        if username is not None:
            return username
    return None


def _has_valid_logout_barrier(request: Request, settings: Settings) -> bool:
    """Honor only an AEAD-authenticated host-issued logout barrier.

    The browser may send a forged parent-Domain value beside the host-only barrier.
    Any valid barrier must still win, while forged values alone must not create a
    persistent denial of service after the host-only barrier is cleared at login.
    """
    return (
        _read_request_cookie(
            request,
            settings,
            name=LOGOUT_BARRIER_COOKIE_NAME,
            kind="logout-barrier",
        )
        is not None
    )


def _matching_totp_step(settings: Settings, code: str) -> tuple[int, int] | None:
    if len(code) != 6 or any(character < "0" or character > "9" for character in code):
        return None
    assert settings.operator_totp_secret is not None
    totp = pyotp.TOTP(settings.operator_totp_secret)
    current_step = int(time.time()) // totp.interval
    for step in (current_step - 1, current_step, current_step + 1):
        expected = totp.at(step * totp.interval)
        if secrets.compare_digest(expected.encode("ascii"), code.encode("ascii")):
            return step, current_step
    return None


def authenticate_login(
    settings: Settings,
    runtime: OperatorAuthRuntime,
    *,
    username: str,
    password: str,
    totp_code: str,
) -> bool:
    """Check all factors and atomically consume the accepted RFC 6238 time step."""
    if not settings.auth_enabled:
        return False
    assert settings.operator_username is not None
    assert settings.operator_password is not None
    assert settings.operator_totp_secret is not None

    username_valid = _constant_time_text_equal(username, settings.operator_username)
    password_valid = _constant_time_text_equal(password, settings.operator_password)
    matched_step = _matching_totp_step(settings, totp_code)
    if not username_valid or not password_valid or matched_step is None:
        return False
    step, current_step = matched_step
    return runtime.consume_totp_step(step, current_step=current_step)


def authenticate_machine(settings: Settings, authorization: str | None) -> OperatorIdentity | None:
    if not settings.auth_enabled or settings.api_token is None or authorization is None:
        return None
    scheme, separator, token = authorization.partition(" ")
    if not separator or scheme.casefold() != "bearer":
        return None
    # RFC 6750 uses 1*SP between the scheme and b64token.  Only consume ASCII
    # spaces here: accepting generic whitespace would make tabs or line breaks
    # valid Authorization delimiters.
    token = token.lstrip(" ")
    if not token or token != token.strip():
        return None
    try:
        validate_machine_api_token(token)
    except SettingsError:
        return None
    if not _constant_time_text_equal(token, settings.api_token):
        return None
    assert settings.operator_username is not None
    return OperatorIdentity(username=settings.operator_username, source="machine")


def authenticate_browser(request: Request, settings: Settings) -> OperatorIdentity | None:
    if not settings.auth_enabled:
        return None
    # A successful sign-out leaves a browser-local barrier behind. It makes a
    # stale automatic-renewal response harmless even when its Set-Cookie header
    # reaches the browser after the logout response.
    if _has_valid_logout_barrier(request, settings):
        return None
    username = _read_request_cookie(
        request,
        settings,
        name=SESSION_COOKIE_NAME,
        kind="session",
    )
    if username is not None:
        return OperatorIdentity(username=username, source="session")
    username = _read_request_cookie(
        request,
        settings,
        name=TRUSTED_BROWSER_COOKIE_NAME,
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
    """Revalidate a long-lived request without falling across credential classes."""
    if not settings.auth_enabled:
        return True
    authorization = request.headers.get("authorization")
    if authorization is not None:
        return authenticate_machine(settings, authorization) is not None
    return authenticate_browser(request, settings) is not None


def has_valid_trusted_browser(request: Request, settings: Settings) -> bool:
    """Return whether this request carries a currently valid trusted-browser credential."""
    if not settings.auth_enabled:
        return False
    if _has_valid_logout_barrier(request, settings):
        return False
    return (
        _read_request_cookie(
            request,
            settings,
            name=TRUSTED_BROWSER_COOKIE_NAME,
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


def set_logout_barrier_cookie(response: Response, settings: Settings) -> None:
    """Prevent a late trusted-browser renewal response from restoring access."""
    token = _issue_cookie(
        settings,
        kind="logout-barrier",
        ttl_seconds=LOGOUT_BARRIER_MAX_AGE_SECONDS,
    )
    response.set_cookie(
        LOGOUT_BARRIER_COOKIE_NAME,
        token,
        max_age=LOGOUT_BARRIER_MAX_AGE_SECONDS,
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


def clear_logout_barrier_cookie(response: Response, settings: Settings) -> None:
    # A host-only deletion cannot remove a sibling's parent-Domain cookie. Such a
    # value is harmless because readers accept only the authenticated token above.
    _delete_cookie(response, settings, LOGOUT_BARRIER_COOKIE_NAME)


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
