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
BROWSER_EPOCH_COOKIE_NAME = "quazonai_browser_epoch"
STREAM_ADMISSION_GENERATION_STATE_ATTRIBUTE = "operator_auth_stream_generation"
COOKIE_VERSION = 2
COOKIE_NONCE_BYTES = 12
COOKIE_BROWSER_EPOCH_BYTES = 32
COOKIE_ISSUANCE_EPOCH_BYTES = 32
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


@dataclass(frozen=True, slots=True)
class _CookieClaims:
    """Authenticated contents shared by all operator-auth cookies."""

    username: str
    cookie_generation: int | None
    cookie_issuance_epoch: str | None
    browser_epoch: str | None


@dataclass(frozen=True, slots=True)
class CookieIssuance:
    """The process-local state that authorizes browser-cookie issuance."""

    generation: int
    process_epoch: str


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
    """Coordinate login throttling, TOTP replay, logout, and cookie issuance."""

    def __init__(self, *, login_limiter: OperatorLoginLimiter | None = None) -> None:
        self.login_limiter = login_limiter or OperatorLoginLimiter()
        self._stream_generation = 0
        self._cookie_generation = 0
        # A counter alone is unsafe after restart because it would return to zero
        # while valid pre-logout cookies retain the same AEAD key and local epoch.
        # This fresh, high-entropy process epoch makes every pre-restart browser
        # credential fail closed without requiring durable auth state.
        self._cookie_process_epoch = _new_cookie_issuance_epoch()
        self._stream_lock = Lock()
        self._accepted_totp_steps: set[int] = set()
        self._totp_lock = Lock()

    def stream_generation(self) -> int:
        with self._stream_lock:
            return self._stream_generation

    def revoke_active_streams(self) -> None:
        with self._stream_lock:
            self._stream_generation += 1

    def cookie_generation(self) -> int:
        """Return the process-wide authenticated-logout issuance epoch."""
        with self._stream_lock:
            return self._cookie_generation

    def cookie_issuance(self) -> CookieIssuance:
        """Return the current counter plus non-repeating process issuance epoch.

        The random process component deliberately changes on every API runtime
        construction. A restart therefore invalidates all existing browser
        session and trusted-browser cookies instead of allowing a reset counter
        to revive a credential revoked before the restart.
        """
        with self._stream_lock:
            return CookieIssuance(
                generation=self._cookie_generation,
                process_epoch=self._cookie_process_epoch,
            )

    def _cookie_issuance_is_current(self, issuance: CookieIssuance) -> bool:
        return self._cookie_generation == issuance.generation and secrets.compare_digest(
            self._cookie_process_epoch,
            issuance.process_epoch,
        )

    def complete_logout(
        self,
        response: Response,
        settings: Settings,
        *,
        revoke_streams: bool,
    ) -> None:
        """Atomically record logout intent and mutate browser cookies.

        A credentialed browser logout revokes process-wide browser-cookie
        issuance along with active streams. An anonymous caller must not be
        allowed to advance that global epoch: raw clients can forge an Origin
        header and otherwise starve legitimate logins. Every logout still writes
        a sealed caller-local browser epoch. Credentials bind to that epoch, so a
        delayed response from this browser cannot become valid after its logout.
        """
        with self._stream_lock:
            if revoke_streams:
                self._stream_generation += 1
                self._cookie_generation += 1
            if settings.auth_enabled:
                set_browser_epoch_cookie(response, settings)
                set_logout_barrier_cookie(response, settings)
            clear_auth_cookies(response, settings)

    def renew_session_if_current(
        self,
        response: Response,
        settings: Settings,
        *,
        cookie_issuance: CookieIssuance,
        browser_epoch: str | None,
    ) -> bool:
        """Issue a trusted-browser session only when logout has not won the race.

        The cookie-generation comparison and response mutation share the logout
        lock so a concurrent logout cannot advance the epoch between this check
        and the ``Set-Cookie`` write.
        """
        with self._stream_lock:
            if not self._cookie_issuance_is_current(cookie_issuance):
                return False
            set_session_cookie(
                response,
                settings,
                cookie_issuance=cookie_issuance,
                browser_epoch=browser_epoch,
            )
            return True

    def complete_login_if_current(
        self,
        response: Response,
        settings: Settings,
        *,
        cookie_issuance: CookieIssuance,
        browser_epoch: str | None,
        trust_browser: bool,
    ) -> bool:
        """Finish a full login only when no logout intervened.

        A password + TOTP check can take place concurrently with logout. Keep the
        cookie-generation comparison and every login-related ``Set-Cookie``
        mutation in the same critical section as logout, so an earlier login
        cannot delete a newer logout barrier or restore a session after logout
        wins.
        """
        with self._stream_lock:
            if not self._cookie_issuance_is_current(cookie_issuance):
                return False
            # Only a successful password + TOTP login clears the browser-local
            # logout barrier; automatic trusted-browser renewal intentionally
            # cannot do this. It deliberately leaves the separate browser epoch
            # untouched, so a stale response cannot overwrite a newer logout.
            clear_logout_barrier_cookie(response, settings)
            set_session_cookie(
                response,
                settings,
                cookie_issuance=cookie_issuance,
                browser_epoch=browser_epoch,
            )
            if trust_browser:
                set_trusted_browser_cookie(
                    response,
                    settings,
                    cookie_issuance=cookie_issuance,
                    browser_epoch=browser_epoch,
                )
            else:
                clear_trusted_browser_cookie(response, settings)
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


def _new_cookie_issuance_epoch() -> str:
    """Create the opaque identifier for one in-memory API runtime."""
    return _urlsafe_encode(secrets.token_bytes(COOKIE_ISSUANCE_EPOCH_BYTES))


def _issue_cookie(
    settings: Settings,
    *,
    kind: str,
    ttl_seconds: int,
    cookie_issuance: CookieIssuance | None = None,
    browser_epoch: str | None = None,
) -> str:
    assert settings.operator_username is not None
    issued_at = int(time.time())
    payload: dict[str, object] = {
        "v": COOKIE_VERSION,
        "kind": kind,
        "sub": settings.operator_username,
        "iat": issued_at,
        "exp": issued_at + ttl_seconds,
    }
    if cookie_issuance is not None:
        payload["cookie_generation"] = cookie_issuance.generation
        payload["cookie_issuance_epoch"] = cookie_issuance.process_epoch
    if browser_epoch is not None:
        payload["browser_epoch"] = browser_epoch
    serialized = json.dumps(
        payload,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    nonce = secrets.token_bytes(COOKIE_NONCE_BYTES)
    ciphertext = AESGCM(settings.auth_cookie_key_bytes()).encrypt(
        nonce,
        serialized,
        _cookie_aad(kind),
    )
    return _urlsafe_encode(nonce + ciphertext)


def _valid_opaque_epoch(value: object, *, byte_length: int) -> bool:
    """Accept one canonical URL-safe encoding of fixed-size random bytes."""
    if not isinstance(value, str):
        return False
    try:
        decoded = _urlsafe_decode(value)
    except _InvalidCookie:
        return False
    return len(decoded) == byte_length and _urlsafe_encode(decoded) == value


def _valid_browser_epoch(value: object) -> bool:
    """Accept the opaque random value carried by a browser-local epoch cookie."""
    return _valid_opaque_epoch(value, byte_length=COOKIE_BROWSER_EPOCH_BYTES)


def _valid_cookie_issuance_epoch(value: object) -> bool:
    """Accept the opaque runtime identifier bound into browser credentials."""
    return _valid_opaque_epoch(value, byte_length=COOKIE_ISSUANCE_EPOCH_BYTES)


def _read_cookie(
    settings: Settings,
    value: str | None,
    *,
    kind: str,
) -> _CookieClaims | None:
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
        cookie_generation = payload.get("cookie_generation")
        if cookie_generation is not None and (
            isinstance(cookie_generation, bool)
            or not isinstance(cookie_generation, int)
            or cookie_generation < 0
        ):
            raise _InvalidCookie
        cookie_issuance_epoch = payload.get("cookie_issuance_epoch")
        if (
            cookie_issuance_epoch is not None
            and not _valid_cookie_issuance_epoch(cookie_issuance_epoch)
        ):
            raise _InvalidCookie
        if kind in {"session", "trusted-browser"}:
            # Browser credentials must have every process issuance component.
            # Rejecting partial/older schemas fails closed when the API gains a
            # new revocation boundary.
            if cookie_generation is None or cookie_issuance_epoch is None:
                raise _InvalidCookie
        elif cookie_generation is not None or cookie_issuance_epoch is not None:
            raise _InvalidCookie
        browser_epoch = payload.get("browser_epoch")
        if browser_epoch is not None and not _valid_browser_epoch(browser_epoch):
            raise _InvalidCookie
        return _CookieClaims(
            username=username,
            cookie_generation=cookie_generation,
            cookie_issuance_epoch=cookie_issuance_epoch,
            browser_epoch=browser_epoch,
        )
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
) -> _CookieClaims | None:
    """Return a valid cookie of ``kind`` without allowing a duplicate to shadow it.

    A parent-Domain cookie can be sent beside a host-only cookie with the same
    name. Raw Cookie field order is not a security boundary, so scan every
    value and accept the first one that verifies under the expected AEAD kind.
    """
    for value in _request_cookie_values(request, name):
        claims = _read_cookie(settings, value, kind=kind)
        if claims is not None:
            return claims
    return None


def browser_cookie_epoch(request: Request, settings: Settings) -> str | None:
    """Return the current sealed caller-local logout epoch, if one exists.

    The epoch is deliberately separate from the logout barrier. A successful
    password + TOTP login clears the short-term barrier but leaves this value in
    place, so a response emitted before a later logout cannot reintroduce a
    credential that validates in this browser profile.
    """
    claims = _read_request_cookie(
        request,
        settings,
        name=BROWSER_EPOCH_COOKIE_NAME,
        kind="browser-epoch",
    )
    if (
        claims is None
        or claims.cookie_generation is not None
        or claims.cookie_issuance_epoch is not None
    ):
        return None
    return claims.browser_epoch


def _current_cookie_issuance(request: Request) -> CookieIssuance:
    runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime
    return runtime.cookie_issuance()


def _matches_browser_issuance(
    claims: _CookieClaims,
    *,
    cookie_issuance: CookieIssuance,
    browser_epoch: str | None,
) -> bool:
    if claims.cookie_generation != cookie_issuance.generation:
        return False
    if claims.cookie_issuance_epoch is None or not secrets.compare_digest(
        claims.cookie_issuance_epoch,
        cookie_issuance.process_epoch,
    ):
        return False
    if claims.browser_epoch is None or browser_epoch is None:
        return claims.browser_epoch is browser_epoch
    return secrets.compare_digest(claims.browser_epoch, browser_epoch)


def _read_current_browser_credential(
    request: Request,
    settings: Settings,
    *,
    name: str,
    kind: str,
    cookie_issuance: CookieIssuance,
    browser_epoch: str | None,
) -> _CookieClaims | None:
    """Return any same-name credential matching all current issuance boundaries."""
    for value in _request_cookie_values(request, name):
        claims = _read_cookie(settings, value, kind=kind)
        if claims is not None and _matches_browser_issuance(
            claims,
            cookie_issuance=cookie_issuance,
            browser_epoch=browser_epoch,
        ):
            return claims
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
    cookie_issuance = _current_cookie_issuance(request)
    browser_epoch = browser_cookie_epoch(request, settings)
    session = _read_current_browser_credential(
        request,
        settings,
        name=SESSION_COOKIE_NAME,
        kind="session",
        cookie_issuance=cookie_issuance,
        browser_epoch=browser_epoch,
    )
    if session is not None:
        return OperatorIdentity(username=session.username, source="session")
    trusted_browser = _read_current_browser_credential(
        request,
        settings,
        name=TRUSTED_BROWSER_COOKIE_NAME,
        kind="trusted-browser",
        cookie_issuance=cookie_issuance,
        browser_epoch=browser_epoch,
    )
    if trusted_browser is not None:
        return OperatorIdentity(
            username=trusted_browser.username,
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
        _read_current_browser_credential(
            request,
            settings,
            name=TRUSTED_BROWSER_COOKIE_NAME,
            kind="trusted-browser",
            cookie_issuance=_current_cookie_issuance(request),
            browser_epoch=browser_cookie_epoch(request, settings),
        )
        is not None
    )


def set_session_cookie(
    response: Response,
    settings: Settings,
    *,
    cookie_issuance: CookieIssuance,
    browser_epoch: str | None,
) -> None:
    token = _issue_cookie(
        settings,
        kind="session",
        ttl_seconds=settings.auth_session_ttl_seconds,
        cookie_issuance=cookie_issuance,
        browser_epoch=browser_epoch,
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


def set_trusted_browser_cookie(
    response: Response,
    settings: Settings,
    *,
    cookie_issuance: CookieIssuance,
    browser_epoch: str | None,
) -> None:
    ttl_seconds = settings.auth_trusted_browser_ttl_days * 24 * 60 * 60
    token = _issue_cookie(
        settings,
        kind="trusted-browser",
        ttl_seconds=ttl_seconds,
        cookie_issuance=cookie_issuance,
        browser_epoch=browser_epoch,
    )
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


def set_browser_epoch_cookie(response: Response, settings: Settings) -> None:
    """Write a durable caller-local generation without exposing it to scripts."""
    epoch = _urlsafe_encode(secrets.token_bytes(COOKIE_BROWSER_EPOCH_BYTES))
    token = _issue_cookie(
        settings,
        kind="browser-epoch",
        ttl_seconds=LOGOUT_BARRIER_MAX_AGE_SECONDS,
        browser_epoch=epoch,
    )
    response.set_cookie(
        BROWSER_EPOCH_COOKIE_NAME,
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
