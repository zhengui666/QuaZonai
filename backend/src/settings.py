"""Bootstrap settings loaded from environment variables."""

from __future__ import annotations

import base64
import binascii
import ipaddress
import os
import re
import secrets
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import ParseResult, urlparse

import idna


class SettingsError(ValueError):
    """Raised when a required setting is invalid."""


DEFAULT_MAX_PLUGIN_WHEEL_BYTES = 256 * 1024 * 1024
DEFAULT_PLUGIN_VALIDATION_TIMEOUT_SECONDS = 180
DEFAULT_BUNDLE_BUILD_TIMEOUT_SECONDS = 600
DEFAULT_PLUGIN_JOB_TIMEOUT_SECONDS = 900
DEFAULT_MISSION_JOB_TIMEOUT_SECONDS = 1800
DEFAULT_JOB_POLL_SECONDS = 1.0
DEFAULT_JOB_LEASE_SECONDS = 60
DEFAULT_AUTH_SESSION_TTL_SECONDS = 12 * 60 * 60
DEFAULT_AUTH_TRUSTED_BROWSER_TTL_DAYS = 30
MIN_AUTH_SESSION_TTL_SECONDS = 5 * 60
MAX_AUTH_SESSION_TTL_SECONDS = 24 * 60 * 60
MIN_AUTH_TRUSTED_BROWSER_TTL_DAYS = 1
MAX_AUTH_TRUSTED_BROWSER_TTL_DAYS = 365
MIN_MACHINE_TOKEN_CHARACTERS = 32
MAX_MACHINE_TOKEN_CHARACTERS = 4096
_DEFAULT_ORIGIN_PORTS = {"http": 80, "https": 443}
_ALLOWED_ENVIRONMENTS = frozenset({"development", "production", "test"})
_LEGACY_OPERATOR_AUTH_ENV_MARKERS = (
    ("QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT"),
    ("QUAZONAI_AUTH_PASSWORD", "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT"),
)
TrustedProxyNetwork = ipaddress.IPv4Network | ipaddress.IPv6Network
_BEARER_TOKEN_PATTERN = re.compile(r"[A-Za-z0-9\-._~+/]+={0,}", re.ASCII)
_WHATWG_IPV4_NUMBER_PATTERN = re.compile(
    r"(?:0[xX][0-9A-Fa-f]*|0[0-7]*|[0-9]+)",
    re.ASCII,
)


def _optional_env(name: str) -> str | None:
    value = os.environ.get(name)
    if value is None:
        return None
    clean = value.strip()
    return clean or None


def _optional_raw_env(name: str) -> str | None:
    value = os.environ.get(name)
    if value is None or value == "":
        return None
    return value


def _trusted_evaluator_command(value: str | None) -> Path | None:
    if value is None:
        return None
    path = Path(value)
    if not path.is_absolute():
        raise SettingsError("QUAZONAI_TRUSTED_EVALUATOR_COMMAND must be an absolute path")
    return path


def _normalize_environment(value: str) -> str:
    """Return the canonical deployment environment or reject unknown labels.

    The environment selects security policy, so an arbitrary value must not be
    able to opt out of the production HTTPS and Secure-cookie requirements.
    Keep the existing trim/case-insensitive behavior for the supported labels.
    """
    normalized = value.strip().casefold()
    if normalized not in _ALLOWED_ENVIRONMENTS:
        allowed = ", ".join(sorted(_ALLOWED_ENVIRONMENTS))
        raise SettingsError(f"QUAZONAI_ENV must be one of: {allowed}")
    return normalized


def _env_bool(name: str, default: bool) -> bool:
    raw = _optional_env(name)
    if raw is None:
        return default
    normalized = raw.casefold()
    if normalized in {"1", "true", "yes", "on"}:
        return True
    if normalized in {"0", "false", "no", "off"}:
        return False
    raise SettingsError(f"{name} must be true or false")


def _bounded_env_int(name: str, default: int, *, minimum: int, maximum: int) -> int:
    raw = _optional_env(name)
    if raw is None:
        return default
    try:
        value = int(raw)
    except ValueError as exc:
        raise SettingsError(f"{name} must be an integer") from exc
    return _validate_bounded_int(
        value,
        name=name,
        minimum=minimum,
        maximum=maximum,
    )


def _validate_bounded_int(
    value: object,
    *,
    name: str,
    minimum: int,
    maximum: int,
) -> int:
    """Validate an already-parsed bounded configuration integer.

    ``Settings`` is also constructed directly by tests and embedding callers,
    so this must reject non-integer values (including ``bool``) rather than
    relying solely on the environment parser.
    """
    if isinstance(value, bool) or not isinstance(value, int):
        raise SettingsError(f"{name} must be an integer")
    if not minimum <= value <= maximum:
        raise SettingsError(f"{name} must be between {minimum} and {maximum}")
    return value


def _parse_trusted_proxy_cidrs(value: str | None) -> tuple[TrustedProxyNetwork, ...]:
    """Parse opt-in direct-peer networks allowed to supply client identity.

    The API must never trust a forwarding header solely because it is present.
    A `/0` network is equivalent to that unsafe behavior, so reject it along
    with non-canonical or non-IP values at enabled-auth startup.
    """
    if value is None:
        return ()

    networks: list[TrustedProxyNetwork] = []
    for raw_network in value.split(","):
        candidate = raw_network.strip()
        if not candidate:
            raise SettingsError(
                "QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS must be a comma-separated "
                "list of IP addresses or CIDRs"
            )
        try:
            network = ipaddress.ip_network(candidate, strict=True)
        except ValueError as exc:
            raise SettingsError(
                "QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS must be a comma-separated "
                "list of IP addresses or canonical CIDRs"
            ) from exc
        networks.append(network)
    return _validate_trusted_proxy_cidrs(tuple(networks))


def _validate_trusted_proxy_cidrs(value: object) -> tuple[TrustedProxyNetwork, ...]:
    """Validate direct ``Settings`` input as strictly as the environment parser."""
    if not isinstance(value, tuple):
        raise SettingsError(
            "QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS must be a tuple of IP networks"
        )

    networks: list[TrustedProxyNetwork] = []
    for network in value:
        if not isinstance(network, (ipaddress.IPv4Network, ipaddress.IPv6Network)):
            raise SettingsError(
                "QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS must contain IP networks only"
            )
        if network.prefixlen == 0:
            raise SettingsError(
                "QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS must not trust an all-addresses /0 network"
            )
        networks.append(network)
    return tuple(networks)


def _base64_key(value: str | None, *, name: str) -> bytes:
    if value is None:
        raise SettingsError(f"{name} must be configured")
    try:
        decoded = base64.b64decode(value, validate=True)
    except (binascii.Error, ValueError) as exc:
        raise SettingsError(f"{name} must be valid base64 encoding exactly 32 bytes") from exc
    if len(decoded) != 32:
        raise SettingsError(f"{name} must be valid base64 encoding exactly 32 bytes")
    return decoded


def normalize_totp_secret(value: str) -> str:
    """Canonicalize and validate a legacy or enrollment TOTP setup key."""
    normalized = "".join(value.split()).upper()
    padded = normalized + "=" * ((8 - len(normalized) % 8) % 8)
    try:
        decoded = base64.b32decode(padded, casefold=True)
    except (binascii.Error, ValueError) as exc:
        raise SettingsError(
            "QUAZONAI_AUTH_TOTP_SECRET must be a valid base32 TOTP setup key"
        ) from exc
    if len(decoded) < 20:
        raise SettingsError(
            "QUAZONAI_AUTH_TOTP_SECRET must encode at least 20 bytes of secret material"
        )
    return normalized


def _contains_ascii_control(value: str) -> bool:
    return any(ord(character) < 32 or ord(character) == 127 for character in value)


def _contains_non_ascii_whitespace(value: str) -> bool:
    """Reject whitespace which WHATWG URL parsing does not trim as URL space."""
    return any(character.isspace() and ord(character) > 127 for character in value)



def validate_machine_api_token(value: str) -> None:
    """Validate the RFC 6750 b64token grammar used in the Authorization header."""
    if not MIN_MACHINE_TOKEN_CHARACTERS <= len(value) <= MAX_MACHINE_TOKEN_CHARACTERS:
        raise SettingsError(
            "QUAZONAI_API_TOKEN must contain between "
            f"{MIN_MACHINE_TOKEN_CHARACTERS} and {MAX_MACHINE_TOKEN_CHARACTERS} characters"
        )
    if _BEARER_TOKEN_PATTERN.fullmatch(value) is None:
        raise SettingsError(
            "QUAZONAI_API_TOKEN must use RFC 6750 b64token ASCII characters only"
        )


def _whatwg_hostname_ends_in_number(hostname: str) -> bool:
    """Return whether the URL Standard sends ``hostname`` to its IPv4 parser.

    For special schemes, WHATWG invokes the IPv4 parser whenever the final
    host label is a valid decimal, legacy-octal, or hexadecimal IPv4 number.
    That includes names such as ``example.127``: the IPv4 parser then rejects
    the mixed labels, while accepting it as an IDNA DNS name here would
    configure an origin a browser can never send. Apply the rule after UTS-46
    conversion, because Unicode digits can map to ASCII digits during that
    conversion.
    """
    components = hostname.split(".")
    if components[-1] == "":
        components.pop()
        if not components:
            return False
    final_component = components[-1]
    return _WHATWG_IPV4_NUMBER_PATTERN.fullmatch(final_component) is not None


def _serialize_whatwg_ipv6(address: ipaddress.IPv6Address) -> str:
    """Serialize IPv6 as the URL Standard does, without an IPv4 dotted tail.

    ``ipaddress`` chooses dotted-quad notation for IPv4-mapped addresses, but
    browsers serialize the same 128-bit address using hexadecimal IPv6 pieces
    in an Origin header. Formatting from the address integer keeps configured
    origins and browser origins equivalent for both spellings.
    """
    number = int(address)
    pieces = [format((number >> shift) & 0xFFFF, "x") for shift in range(112, -1, -16)]

    best_start = -1
    best_length = 0
    current_start = 0
    current_length = 0
    for index, piece in enumerate(pieces):
        if piece == "0":
            if current_length == 0:
                current_start = index
            current_length += 1
            if current_length > best_length:
                best_start = current_start
                best_length = current_length
        else:
            current_length = 0

    if best_length < 2:
        return ":".join(pieces)

    before = ":".join(pieces[:best_start])
    after = ":".join(pieces[best_start + best_length :])
    if before and after:
        return f"{before}::{after}"
    if before:
        return f"{before}::"
    return f"::{after}"


def _canonical_origin_host(parsed: ParseResult, *, name: str) -> tuple[str, int | None]:
    try:
        hostname = parsed.hostname
        port = parsed.port
    except ValueError as exc:
        raise SettingsError(f"{name} must contain a valid host and optional TCP port") from exc
    if port == 0:
        raise SettingsError(f"{name} must contain a valid host and optional TCP port")
    if hostname is None or any(
        character.isspace() or ord(character) < 32 for character in hostname
    ):
        raise SettingsError(f"{name} must contain a valid host and optional TCP port")
    if "%" in hostname:
        raise SettingsError(f"{name} contains an invalid hostname")

    try:
        address = ipaddress.ip_address(hostname)
    except ValueError:
        address = None
    if isinstance(address, ipaddress.IPv6Address):
        return f"[{_serialize_whatwg_ipv6(address)}]", port
    if address is not None:
        return address.compressed, port

    try:
        ascii_hostname = idna.encode(hostname, uts46=True, std3_rules=True).decode("ascii").lower()
    except idna.IDNAError as exc:
        raise SettingsError(f"{name} contains an invalid hostname") from exc
    if _whatwg_hostname_ends_in_number(ascii_hostname):
        raise SettingsError(f"{name} contains an invalid hostname")
    if len(ascii_hostname) > 253:
        raise SettingsError(f"{name} contains an invalid hostname")
    labels = ascii_hostname.split(".")
    if any(
        not label
        or len(label) > 63
        or not label[0].isalnum()
        or not label[-1].isalnum()
        or any(not (character.isalnum() or character == "-") for character in label)
        for label in labels
    ):
        raise SettingsError(f"{name} contains an invalid hostname")
    return ascii_hostname, port


def canonicalize_http_origin(value: str, *, name: str = "Origin") -> str:
    """Serialize an absolute HTTP(S) origin using browser-equivalent semantics."""
    if _contains_ascii_control(value):
        raise SettingsError(f"{name} must not contain ASCII control characters")
    if _contains_non_ascii_whitespace(value):
        raise SettingsError(f"{name} must not contain non-ASCII whitespace")
    clean = value.strip()
    try:
        parsed = urlparse(clean)
    except ValueError as exc:
        raise SettingsError(
            f"{name} must be an origin such as https://quazonai.example.com"
        ) from exc
    scheme = parsed.scheme.lower()
    if (
        scheme not in _DEFAULT_ORIGIN_PORTS
        or not parsed.netloc
        or parsed.username is not None
        or parsed.password is not None
        # ``urlparse`` drops empty query, fragment, and path-param delimiters;
        # retain the public-origin contract even when their payload is empty.
        or any(delimiter in clean for delimiter in ("?", "#", ";"))
        # Likewise, a trailing colon has no parsed port but is not an origin.
        or parsed.netloc.endswith(":")
        or parsed.params
        or parsed.query
        or parsed.fragment
        or parsed.path not in {"", "/"}
    ):
        raise SettingsError(f"{name} must be an origin such as https://quazonai.example.com")

    host, port = _canonical_origin_host(parsed, name=name)
    default_port = _DEFAULT_ORIGIN_PORTS[scheme]
    port_suffix = "" if port is None or port == default_port else f":{port}"
    return f"{scheme}://{host}{port_suffix}"


@dataclass(frozen=True, slots=True)
class Settings:
    environment: str
    database_url: str
    alembic_url: str
    master_key: str | None
    plugin_root: Path
    max_plugin_wheel_bytes: int
    plugin_validation_timeout_seconds: int
    bundle_build_timeout_seconds: int
    plugin_job_timeout_seconds: int
    job_poll_seconds: float
    job_lease_seconds: int
    package_root: Path = Path("/var/lib/quazonai/packages")
    mission_root: Path = Path("/var/lib/quazonai/missions")
    codex_home: Path = Path("/home/quazonai/.codex")
    codex_model: str | None = None
    codex_base_url: str | None = None
    codex_reasoning_effort: str | None = None
    codex_fast_mode: bool = False
    codex_api_key: str | None = None
    mission_job_timeout_seconds: int = DEFAULT_MISSION_JOB_TIMEOUT_SECONDS
    trusted_evaluator_command: Path | None = None
    frontend_dist: Path = Path("/workspace/frontend-dist")
    operator_auth_enabled: bool = False
    operator_totp_secret: str | None = None
    auth_cookie_key: str | None = None
    api_token: str | None = None
    auth_public_origin: str | None = None
    auth_session_ttl_seconds: int = DEFAULT_AUTH_SESSION_TTL_SECONDS
    auth_trusted_browser_ttl_days: int = DEFAULT_AUTH_TRUSTED_BROWSER_TTL_DAYS
    auth_trusted_proxy_cidrs: tuple[TrustedProxyNetwork, ...] = ()

    @classmethod
    def from_env(cls) -> "Settings":
        """Load only bootstrap/infrastructure settings from the process environment."""
        environment = _normalize_environment(
            _optional_env("QUAZONAI_ENV") or "development"
        )
        database_url = os.environ.get(
            "QUAZONAI_DATABASE_URL",
            "postgresql+psycopg://quazonai:quazonai-local@127.0.0.1:5432/quazonai",
        )
        alembic_url = os.environ.get("QUAZONAI_ALEMBIC_URL", database_url)
        operator_auth_enabled = _env_bool("QUAZONAI_AUTH_ENABLED", False)
        legacy_auth_variables = tuple(
            legacy_name
            for legacy_name, presence_marker in _LEGACY_OPERATOR_AUTH_ENV_MARKERS
            if (
                _optional_raw_env(legacy_name) is not None
                or _optional_env(presence_marker) is not None
            )
        )
        if operator_auth_enabled and legacy_auth_variables:
            raise SettingsError(
                "Operator authentication no longer supports the deprecated "
                "username/password variables; remove: " + ", ".join(legacy_auth_variables)
            )
        totp_secret = _optional_env("QUAZONAI_AUTH_TOTP_SECRET")
        if totp_secret is not None:
            totp_secret = "".join(totp_secret.split()).upper()
        session_ttl = DEFAULT_AUTH_SESSION_TTL_SECONDS
        trusted_browser_ttl = DEFAULT_AUTH_TRUSTED_BROWSER_TTL_DAYS
        trusted_proxy_cidrs: tuple[TrustedProxyNetwork, ...] = ()
        if operator_auth_enabled:
            session_ttl = _bounded_env_int(
                "QUAZONAI_AUTH_SESSION_TTL_SECONDS",
                DEFAULT_AUTH_SESSION_TTL_SECONDS,
                minimum=MIN_AUTH_SESSION_TTL_SECONDS,
                maximum=MAX_AUTH_SESSION_TTL_SECONDS,
            )
            trusted_browser_ttl = _bounded_env_int(
                "QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS",
                DEFAULT_AUTH_TRUSTED_BROWSER_TTL_DAYS,
                minimum=MIN_AUTH_TRUSTED_BROWSER_TTL_DAYS,
                maximum=MAX_AUTH_TRUSTED_BROWSER_TTL_DAYS,
            )
            trusted_proxy_cidrs = _parse_trusted_proxy_cidrs(
                _optional_env("QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS")
            )
        return cls(
            environment=environment,
            database_url=database_url,
            alembic_url=alembic_url,
            master_key=_optional_env("QUAZONAI_MASTER_KEY"),
            plugin_root=Path(
                os.environ.get("QUAZONAI_PLUGIN_ROOT", "/var/lib/quazonai/plugins")
            ),
            max_plugin_wheel_bytes=DEFAULT_MAX_PLUGIN_WHEEL_BYTES,
            plugin_validation_timeout_seconds=DEFAULT_PLUGIN_VALIDATION_TIMEOUT_SECONDS,
            bundle_build_timeout_seconds=DEFAULT_BUNDLE_BUILD_TIMEOUT_SECONDS,
            plugin_job_timeout_seconds=DEFAULT_PLUGIN_JOB_TIMEOUT_SECONDS,
            job_poll_seconds=DEFAULT_JOB_POLL_SECONDS,
            job_lease_seconds=DEFAULT_JOB_LEASE_SECONDS,
            package_root=Path(
                os.environ.get("QUAZONAI_PACKAGE_ROOT", "/var/lib/quazonai/packages")
            ),
            mission_root=Path(
                os.environ.get("QUAZONAI_MISSION_ROOT", "/var/lib/quazonai/missions")
            ),
            codex_home=Path(os.environ.get("CODEX_HOME", "/home/quazonai/.codex")),
            codex_model=None,
            codex_base_url=None,
            codex_reasoning_effort=None,
            codex_fast_mode=False,
            codex_api_key=None,
            mission_job_timeout_seconds=DEFAULT_MISSION_JOB_TIMEOUT_SECONDS,
            trusted_evaluator_command=_trusted_evaluator_command(
                _optional_env("QUAZONAI_TRUSTED_EVALUATOR_COMMAND")
            ),
            frontend_dist=Path(
                os.environ.get("QUAZONAI_FRONTEND_DIST", "/workspace/frontend-dist")
            ),
            operator_auth_enabled=operator_auth_enabled,
            operator_totp_secret=totp_secret,
            auth_cookie_key=_optional_raw_env("QUAZONAI_AUTH_COOKIE_KEY"),
            api_token=_optional_raw_env("QUAZONAI_API_TOKEN"),
            auth_public_origin=_optional_raw_env("QUAZONAI_AUTH_PUBLIC_ORIGIN"),
            auth_session_ttl_seconds=session_ttl,
            auth_trusted_browser_ttl_days=trusted_browser_ttl,
            auth_trusted_proxy_cidrs=trusted_proxy_cidrs,
        )

    @property
    def master_key_configured(self) -> bool:
        if self.master_key is None:
            return False
        try:
            decoded = base64.b64decode(self.master_key, validate=True)
        except (binascii.Error, ValueError):
            return False
        return len(decoded) == 32

    @property
    def codex_auth_configured(self) -> bool:
        # Official ChatGPT auth is database-owned and therefore cannot be
        # represented by bootstrap Settings.  Runtime code checks the DB
        # configuration explicitly; this property only covers custom API key
        # configuration supplied to a short-lived child.
        return bool(self.codex_api_key)

    @property
    def auth_enabled(self) -> bool:
        return self.operator_auth_enabled

    @property
    def canonical_auth_public_origin(self) -> str | None:
        if self.auth_public_origin is None:
            return None
        return canonicalize_http_origin(
            self.auth_public_origin,
            name="QUAZONAI_AUTH_PUBLIC_ORIGIN",
        )

    @property
    def auth_cookie_secure(self) -> bool:
        if not self.auth_enabled:
            return False
        origin = self.canonical_auth_public_origin
        return origin is not None and origin.startswith("https://")

    def master_key_bytes(self) -> bytes:
        if not self.master_key_configured or self.master_key is None:
            raise SettingsError(
                "QUAZONAI_MASTER_KEY must be valid base64 encoding exactly 32 bytes"
            )
        return base64.b64decode(self.master_key, validate=True)

    def auth_cookie_key_bytes(self) -> bytes:
        return _base64_key(self.auth_cookie_key, name="QUAZONAI_AUTH_COOKIE_KEY")

    def validate_operator_auth(self) -> None:
        """Validate bootstrap auth config; the durable TOTP binding is DB-owned."""
        environment = _normalize_environment(self.environment)
        if not self.operator_auth_enabled:
            return

        _validate_bounded_int(
            self.auth_session_ttl_seconds,
            name="QUAZONAI_AUTH_SESSION_TTL_SECONDS",
            minimum=MIN_AUTH_SESSION_TTL_SECONDS,
            maximum=MAX_AUTH_SESSION_TTL_SECONDS,
        )
        _validate_bounded_int(
            self.auth_trusted_browser_ttl_days,
            name="QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS",
            minimum=MIN_AUTH_TRUSTED_BROWSER_TTL_DAYS,
            maximum=MAX_AUTH_TRUSTED_BROWSER_TTL_DAYS,
        )
        _validate_trusted_proxy_cidrs(self.auth_trusted_proxy_cidrs)

        fields = {
            "QUAZONAI_MASTER_KEY": self.master_key,
            "QUAZONAI_AUTH_COOKIE_KEY": self.auth_cookie_key,
            "QUAZONAI_API_TOKEN": self.api_token,
            "QUAZONAI_AUTH_PUBLIC_ORIGIN": self.auth_public_origin,
        }
        missing = [name for name, value in fields.items() if not value]
        if missing:
            raise SettingsError(
                "Operator authentication is enabled but incomplete; missing: "
                + ", ".join(missing)
            )

        assert self.api_token is not None
        assert self.auth_public_origin is not None

        validate_machine_api_token(self.api_token)

        master_key = self.master_key_bytes()
        cookie_key = self.auth_cookie_key_bytes()
        if secrets.compare_digest(cookie_key, master_key):
            raise SettingsError(
                "QUAZONAI_AUTH_COOKIE_KEY must be different from QUAZONAI_MASTER_KEY"
            )

        if self.operator_totp_secret is not None:
            normalize_totp_secret(self.operator_totp_secret)

        canonical_origin = self.canonical_auth_public_origin
        assert canonical_origin is not None
        if environment == "production" and not canonical_origin.startswith(
            "https://"
        ):
            raise SettingsError("QUAZONAI_AUTH_PUBLIC_ORIGIN must use https in production")

    def validate_database_scheme(self) -> None:
        scheme = urlparse(self.database_url).scheme
        if scheme not in {"postgresql+psycopg", "sqlite+pysqlite", "sqlite"}:
            raise SettingsError(
                "QUAZONAI_DATABASE_URL must use postgresql+psycopg or sqlite for tests"
            )

    def ensure_worker_directories(self) -> None:
        for path in (
            self.plugin_root,
            self.plugin_root / "staging",
            self.plugin_root / "validation",
            self.plugin_root / "releases",
            self.plugin_root / "bundle-staging",
            self.plugin_root / "bundles",
            self.package_root,
            self.package_root / "staging",
            self.mission_root,
            self.mission_root / "programs",
            self.mission_root / "worktrees",
            self.codex_home,
        ):
            path.mkdir(parents=True, exist_ok=True)
