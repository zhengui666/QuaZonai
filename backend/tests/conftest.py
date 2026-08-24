from __future__ import annotations

import base64
import os
from collections.abc import Iterator
from pathlib import Path

import pytest
from sqlalchemy import Engine, create_engine
from sqlalchemy.pool import StaticPool

from db.models import Base
from settings import Settings


def _test_database_url() -> str:
    configured = os.environ.get("QUAZONAI_DATABASE_URL", "")
    if configured.startswith("postgresql"):
        return configured
    return "sqlite+pysqlite:///:memory:"


@pytest.fixture
def engine() -> Iterator[Engine]:
    url = _test_database_url()
    if url.startswith("sqlite"):
        database = create_engine(
            url,
            connect_args={"check_same_thread": False},
            poolclass=StaticPool,
        )
    else:
        database = create_engine(url, pool_pre_ping=True)
    Base.metadata.drop_all(database)
    Base.metadata.create_all(database)
    try:
        yield database
    finally:
        Base.metadata.drop_all(database)
        database.dispose()


@pytest.fixture
def settings(tmp_path: Path) -> Settings:
    key = base64.b64encode(b"k" * 32).decode("ascii")
    database_url = _test_database_url()
    result = Settings(
        environment="test",
        database_url=database_url,
        alembic_url=database_url,
        master_key=key,
        plugin_root=tmp_path / "plugins",
        max_plugin_wheel_bytes=1024 * 1024,
        plugin_validation_timeout_seconds=30,
        bundle_build_timeout_seconds=60,
        plugin_job_timeout_seconds=90,
        job_poll_seconds=0.01,
        job_lease_seconds=60,
    )
    result.ensure_worker_directories()
    return result
