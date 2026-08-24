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

    @classmethod
    def from_env(cls) -> "Settings":
        """Load only bootstrap/infrastructure settings from the process environment."""
        database_url = os.environ.get(
            "QUAZONAI_DATABASE_URL",
            "postgresql+psycopg://quazonai:quazonai-local@127.0.0.1:5432/quazonai",
        )
        alembic_url = os.environ.get("QUAZONAI_ALEMBIC_URL", database_url)
        return cls(
            environment=os.environ.get("QUAZONAI_ENV", "development"),
            database_url=database_url,
            alembic_url=alembic_url,
            master_key=os.environ.get("QUAZONAI_MASTER_KEY") or None,
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

    def master_key_bytes(self) -> bytes:
        if not self.master_key_configured or self.master_key is None:
            raise SettingsError(
                "QUAZONAI_MASTER_KEY must be valid base64 encoding exactly 32 bytes"
            )
        return base64.b64decode(self.master_key, validate=True)

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
