"""Runtime settings loaded from environment variables."""

from __future__ import annotations

import base64
import binascii
import os
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse


class SettingsError(ValueError):
    """Raised when a required setting is invalid."""


def _positive_int(name: str, default: int) -> int:
    raw = os.environ.get(name)
    if raw is None or raw == "":
        return default
    try:
        value = int(raw)
    except ValueError as exc:
        raise SettingsError(f"{name} must be an integer") from exc
    if value <= 0:
        raise SettingsError(f"{name} must be greater than zero")
    return value


def _positive_float(name: str, default: float) -> float:
    raw = os.environ.get(name)
    if raw is None or raw == "":
        return default
    try:
        value = float(raw)
    except ValueError as exc:
        raise SettingsError(f"{name} must be a number") from exc
    if value <= 0:
        raise SettingsError(f"{name} must be greater than zero")
    return value


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
    mission_job_timeout_seconds: int = 1800
    frontend_dist: Path = Path("/workspace/frontend-dist")

    @classmethod
    def from_env(cls) -> "Settings":
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
            max_plugin_wheel_bytes=_positive_int(
                "QUAZONAI_MAX_PLUGIN_WHEEL_BYTES", 256 * 1024 * 1024
            ),
            plugin_validation_timeout_seconds=_positive_int(
                "QUAZONAI_PLUGIN_VALIDATION_TIMEOUT_SECONDS", 180
            ),
            bundle_build_timeout_seconds=_positive_int(
                "QUAZONAI_BUNDLE_BUILD_TIMEOUT_SECONDS", 600
            ),
            plugin_job_timeout_seconds=_positive_int(
                "QUAZONAI_PLUGIN_JOB_TIMEOUT_SECONDS", 900
            ),
            job_poll_seconds=_positive_float("QUAZONAI_JOB_POLL_SECONDS", 1.0),
            job_lease_seconds=_positive_int("QUAZONAI_JOB_LEASE_SECONDS", 60),
            package_root=Path(
                os.environ.get("QUAZONAI_PACKAGE_ROOT", "/var/lib/quazonai/packages")
            ),
            mission_root=Path(
                os.environ.get("QUAZONAI_MISSION_ROOT", "/var/lib/quazonai/missions")
            ),
            codex_home=Path(os.environ.get("CODEX_HOME", "/home/quazonai/.codex")),
            codex_model=os.environ.get("QUAZONAI_CODEX_MODEL") or None,
            mission_job_timeout_seconds=_positive_int(
                "QUAZONAI_MISSION_JOB_TIMEOUT_SECONDS", 1800
            ),
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
        return bool(os.environ.get("OPENAI_API_KEY")) or (self.codex_home / "auth.json").is_file()

    def master_key_bytes(self) -> bytes:
        if not self.master_key_configured or self.master_key is None:
            raise SettingsError("QUAZONAI_MASTER_KEY must be valid base64 encoding exactly 32 bytes")
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
