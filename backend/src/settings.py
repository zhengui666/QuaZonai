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
MAX_OPERATOR_USERNAME_CHARACTERS = 200
MIN_OPERATOR_PASSWORD_CHARACTERS = 12
MAX_OPERATOR_PASSWORD_CHARACTERS = 4096
MIN_MACHINE_TOKEN_CHARACTERS = 32
MAX_MACHINE_TOKEN_CHARACTERS = 4096
_DEFAULT_ORIGIN_PORTS = {"http": 80, "https": 443}
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
    if not minimum <= value <= maximum:
        raise SettingsError(f"{name} must be between {minimum} and {maximum}")
    return value


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


def _contains_ascii_control(value: str) -> bool:
    return any(ord(character) < 32 or ord(character) == 127 for character in value)


def _contains_non_ascii_whitespace(value: str) -> bool:
    """Reject whitespace which WHATWG URL parsing does not trim as URL space."""
    return any(character.isspace() and ord(character) > 127 for character in value)


def _validate_utf8_text(value: str, *, name: str) -> None:
    try:
        value.encode("utf-8")
    except UnicodeEncodeError as exc:
        raise SettingsError(f"{name} must contain valid Unicode text") from exc


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


def _looks_like_noncanonical_whatwg_ipv4(hostname: str) -> bool:
    """Return whether a browser treats a non-``ipaddress`` host as numeric IPv4.

    WHATWG URL parsing accepts one-to-four numeric components, including legacy
    hexadecimal and leading-zero forms, then serializes them as canonical dotted
    decimal IPv4. ``ipaddress`` deliberately rejects those spellings. Treating
    them as IDNA hostnames would make the configured origin differ from the
    browser-sent Origin, so reject them rather than silently accepting a
    deployment that cannot authenticate browser mutations.
    """
    components = hostname.split(".")
    return 1 <= len(components) <= 4 and all(
        _WHATWG_IPV4_NUMBER_PATTERN.fullmatch(component) is not None
        for component in components
    )


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

    if _looks_like_noncanonical_whatwg_ipv4(hostname):
        raise SettingsError(f"{name} contains an invalid hostname")

    # A dotted-decimal host made only of digits is intended to be an IPv4
    # literal. Do not reinterpret an invalid address as a DNS name.
    if all(character.isdigit() or character == "." for character in hostname):
        raise SettingsError(f"{name} contains an invalid hostname")

    try:
        ascii_hostname = idna.encode(hostname, uts46=True, std3_rules=True).decode("ascii").lower()
    except idna.IDNAError as exc:
        raise SettingsError(f"{name} contains an invalid hostname") from exc
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
    codex_api_key: str | None = None
    mission_job_timeout_seconds: int = DEFAULT_MISSION_JOB_TIMEOUT_SECONDS
    frontend_dist: Path = Path("/workspace/frontend-dist")
    operator_auth_enabled: bool = False
    operator_username: str | None = None
    operator_password: str | None = None
    operator_totp_secret: str | None = None
    auth_cookie_key: str | None = None
    api_token: str | None = None
    auth_public_origin: str | None = None
    auth_session_ttl_seconds: int = DEFAULT_AUTH_SESSION_TTL_SECONDS
    auth_trusted_browser_ttl_days: int = DEFAULT_AUTH_TRUSTED_BROWSER_TTL_DAYS

    @classmethod
    def from_env(cls) -> "Settings":
        """Load only bootstrap/infrastructure settings from the process environment."""
        environment = _optional_env("QUAZONAI_ENV") or "development"
        database_url = os.environ.get(
            "QUAZONAI_DATABASE_URL",
            "postgresql+psycopg://quazonai:quazonai-local@127.0.0.1:5432/quazonai",
        )
        alembic_url = os.environ.get("QUAZONAI_ALEMBIC_URL", database_url)
        operator_auth_enabled = _env_bool("QUAZONAI_AUTH_ENABLED", False)
        totp_secret = _optional_env("QUAZONAI_AUTH_TOTP_SECRET")
        if totp_secret is not None:
            totp_secret = "".join(totp_secret.split()).upper()
        session_ttl = DEFAULT_AUTH_SESSION_TTL_SECONDS
        trusted_browser_ttl = DEFAULT_AUTH_TRUSTED_BROWSER_TTL_DAYS
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
            codex_api_key=None,
            mission_job_timeout_seconds=DEFAULT_MISSION_JOB_TIMEOUT_SECONDS,
            frontend_dist=Path(
                os.environ.get("QUAZONAI_FRONTEND_DIST", "/workspace/frontend-dist")
            ),
            operator_auth_enabled=operator_auth_enabled,
            operator_username=_optional_raw_env("QUAZONAI_AUTH_USERNAME"),
            operator_password=_optional_raw_env("QUAZONAI_AUTH_PASSWORD"),
            operator_totp_secret=totp_secret,
            auth_cookie_key=_optional_raw_env("QUAZONAI_AUTH_COOKIE_KEY"),
            api_token=_optional_raw_env("QUAZONAI_API_TOKEN"),
            auth_public_origin=_optional_raw_env("QUAZONAI_AUTH_PUBLIC_ORIGIN"),
            auth_session_ttl_seconds=session_ttl,
            auth_trusted_browser_ttl_days=trusted_browser_ttl,
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
        return bool(self.codex_api_key) or (self.codex_home / "auth.json").is_file()

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
        """Validate the complete opt-in operator-authentication configuration."""
        if not self.operator_auth_enabled:
            return

        fields = {
            "QUAZONAI_AUTH_USERNAME": self.operator_username,
            "QUAZONAI_AUTH_PASSWORD": self.operator_password,
            "QUAZONAI_AUTH_TOTP_SECRET": self.operator_totp_secret,
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

        assert self.operator_username is not None
        assert self.operator_password is not None
        assert self.operator_totp_secret is not None
        assert self.api_token is not None
        assert self.auth_public_origin is not None

        _validate_utf8_text(self.operator_username, name="QUAZONAI_AUTH_USERNAME")
        _validate_utf8_text(self.operator_password, name="QUAZONAI_AUTH_PASSWORD")
        if len(self.operator_username) > MAX_OPERATOR_USERNAME_CHARACTERS:
            raise SettingsError(
                f"QUAZONAI_AUTH_USERNAME must contain at most "
                f"{MAX_OPERATOR_USERNAME_CHARACTERS} characters"
            )
        if not (
            MIN_OPERATOR_PASSWORD_CHARACTERS
            <= len(self.operator_password)
            <= MAX_OPERATOR_PASSWORD_CHARACTERS
        ):
            raise SettingsError(
                "QUAZONAI_AUTH_PASSWORD must contain between "
                f"{MIN_OPERATOR_PASSWORD_CHARACTERS} and "
                f"{MAX_OPERATOR_PASSWORD_CHARACTERS} characters"
            )
        validate_machine_api_token(self.api_token)

        cookie_key = self.auth_cookie_key_bytes()
        if self.master_key_configured and secrets.compare_digest(
            cookie_key,
            self.master_key_bytes(),
        ):
            raise SettingsError(
                "QUAZONAI_AUTH_COOKIE_KEY must be different from QUAZONAI_MASTER_KEY"
            )

        padded_secret = self.operator_totp_secret + "=" * (
            (8 - len(self.operator_totp_secret) % 8) % 8
        )
        try:
            decoded_secret = base64.b32decode(padded_secret, casefold=True)
        except (binascii.Error, ValueError) as exc:
            raise SettingsError(
                "QUAZONAI_AUTH_TOTP_SECRET must be a valid base32 TOTP setup key"
            ) from exc
        if len(decoded_secret) < 20:
            raise SettingsError(
                "QUAZONAI_AUTH_TOTP_SECRET must encode at least 20 bytes of secret material"
            )

        canonical_origin = self.canonical_auth_public_origin
        assert canonical_origin is not None
        if self.environment.strip().casefold() == "production" and not canonical_origin.startswith(
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
