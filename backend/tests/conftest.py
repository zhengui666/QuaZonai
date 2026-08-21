from __future__ import annotations

import base64
from collections.abc import Iterator
from pathlib import Path

import pytest
from sqlalchemy import Engine, create_engine
from sqlalchemy.pool import StaticPool

from quantfoundry.db.models import Base
from quantfoundry.settings import Settings


@pytest.fixture
def engine() -> Iterator[Engine]:
    database = create_engine(
        "sqlite+pysqlite:///:memory:",
        connect_args={"check_same_thread": False},
        poolclass=StaticPool,
    )
    Base.metadata.create_all(database)
    try:
        yield database
    finally:
        database.dispose()


@pytest.fixture
def settings(tmp_path: Path) -> Settings:
    key = base64.b64encode(b"k" * 32).decode("ascii")
    result = Settings(
        environment="test",
        database_url="sqlite+pysqlite:///:memory:",
        alembic_url="sqlite+pysqlite:///:memory:",
        master_key=key,
        plugin_root=tmp_path / "plugins",
        catalog_root=tmp_path / "catalog",
        report_root=tmp_path / "reports",
        import_root=tmp_path / "imports",
        agent_artifact_root=tmp_path / "agent-artifacts",
        max_parquet_upload_bytes=1024 * 1024,
        max_plugin_wheel_bytes=1024 * 1024,
        plugin_validation_timeout_seconds=30,
        bundle_build_timeout_seconds=60,
        plugin_job_timeout_seconds=90,
        integration_preflight_timeout_seconds=30,
        strategy_validation_timeout_seconds=30,
        parquet_import_timeout_seconds=60,
        backtest_timeout_seconds=60,
        research_job_timeout_seconds=120,
        job_poll_seconds=0.01,
        job_lease_seconds=60,
        supervisor_poll_seconds=0.01,
    )
    result.ensure_worker_directories()
    return result
