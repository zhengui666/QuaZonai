"""Bootstrap settings loaded from environment variables."""

from __future__ import annotations

import base64
import binascii
import os
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse


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


def _optional_env(name: str) -> str | None:
    value = os.environ.get(name)
    if value is None:
        return None
    clean = value.strip()
    return clean or None


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
        database_url = os.environ.get(
            "QUAZONAI_DATABASE_URL",
            "postgresql+psycopg://quazonai:quazonai-local@127.0.0.1:5432/quazonai",
        )
        alembic_url = os.environ.get("QUAZONAI_ALEMBIC_URL", database_url)
        totp_secret = _optional_env("QUAZONAI_AUTH_TOTP_SECRET")
        if totp_secret is not None:
            totp_secret = "".join(totp_secret.split()).upper()
        return cls(
            environment=os.environ.get("QUAZONAI_ENV", "development"),
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
            operator_username=_optional_env("QUAZONAI_AUTH_USERNAME"),
            operator_password=_optional_env("QUAZONAI_AUTH_PASSWORD"),
            operator_totp_secret=totp_secret,
            auth_cookie_key=_optional_env("QUAZONAI_AUTH_COOKIE_KEY"),
            api_token=_optional_env("QUAZONAI_API_TOKEN"),
            auth_public_origin=_optional_env("QUAZONAI_AUTH_PUBLIC_ORIGIN"),
            auth_session_ttl_seconds=_bounded_env_int(
                "QUAZONAI_AUTH_SESSION_TTL_SECONDS",
                DEFAULT_AUTH_SESSION_TTL_SECONDS,
                minimum=MIN_AUTH_SESSION_TTL_SECONDS,
                maximum=MAX_AUTH_SESSION_TTL_SECONDS,
            ),
            auth_trusted_browser_ttl_days=_bounded_env_int(
                "QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS",
                DEFAULT_AUTH_TRUSTED_BROWSER_TTL_DAYS,
                minimum=MIN_AUTH_TRUSTED_BROWSER_TTL_DAYS,
                maximum=MAX_AUTH_TRUSTED_BROWSER_TTL_DAYS,
            ),
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
        return all(
            (
                self.operator_username,
                self.operator_password,
                self.operator_totp_secret,
                self.auth_cookie_key,
                self.api_token,
                self.auth_public_origin,
            )
        )

    @property
    def auth_cookie_secure(self) -> bool:
        return self.environment.casefold() == "production"

    def master_key_bytes(self) -> bytes:
        if not self.master_key_configured or self.master_key is None:
            raise SettingsError(
                "QUAZONAI_MASTER_KEY must be valid base64 encoding exactly 32 bytes"
            )
        return base64.b64decode(self.master_key, validate=True)

    def auth_cookie_key_bytes(self) -> bytes:
        return _base64_key(self.auth_cookie_key, name="QUAZONAI_AUTH_COOKIE_KEY")

    def validate_operator_auth(self) -> None:
        """Fail closed for production and reject partially configured auth everywhere."""
        fields = {
            "QUAZONAI_AUTH_USERNAME": self.operator_username,
            "QUAZONAI_AUTH_PASSWORD": self.operator_password,
            "QUAZONAI_AUTH_TOTP_SECRET": self.operator_totp_secret,
            "QUAZONAI_AUTH_COOKIE_KEY": self.auth_cookie_key,
            "QUAZONAI_API_TOKEN": self.api_token,
            "QUAZONAI_AUTH_PUBLIC_ORIGIN": self.auth_public_origin,
        }
        configured = [name for name, value in fields.items() if value]
        missing = [name for name, value in fields.items() if not value]
        production = self.environment.casefold() == "production"

        if not configured:
            if production:
                raise SettingsError(
                    "Operator authentication must be configured in production: "
                    + ", ".join(fields)
                )
            return
        if missing:
            raise SettingsError(
                "Operator authentication is partially configured; missing: " + ", ".join(missing)
            )

        assert self.operator_username is not None
        assert self.operator_password is not None
        assert self.operator_totp_secret is not None
        assert self.api_token is not None
        assert self.auth_public_origin is not None

        if len(self.operator_username) > 200:
            raise SettingsError("QUAZONAI_AUTH_USERNAME must contain at most 200 characters")
        if len(self.operator_password) < 12:
            raise SettingsError("QUAZONAI_AUTH_PASSWORD must contain at least 12 characters")
        if len(self.api_token) < 32:
            raise SettingsError("QUAZONAI_API_TOKEN must contain at least 32 characters")
        self.auth_cookie_key_bytes()

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

        parsed = urlparse(self.auth_public_origin)
        if (
            parsed.scheme not in {"http", "https"}
            or not parsed.netloc
            or parsed.username
            or parsed.password
            or parsed.query
            or parsed.fragment
            or parsed.path not in {"", "/"}
        ):
            raise SettingsError(
                "QUAZONAI_AUTH_PUBLIC_ORIGIN must be an origin such as https://quazonai.example.com"
            )
        if production and parsed.scheme != "https":
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
