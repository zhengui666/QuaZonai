from __future__ import annotations

import pytest
from sqlalchemy import create_engine, text

from quantfoundry.db.preflight import EXPECTED_REVISION, check_engine_schema
from quantfoundry.errors import QfError


def test_empty_schema_is_allowed() -> None:
    engine = create_engine("sqlite+pysqlite:///:memory:")
    check_engine_schema(engine)


def test_unowned_schema_is_rejected() -> None:
    engine = create_engine("sqlite+pysqlite:///:memory:")
    with engine.begin() as connection:
        connection.execute(text("CREATE TABLE legacy_table (id INTEGER PRIMARY KEY)"))
    with pytest.raises(QfError, match="OLD_SCHEMA_REQUIRES_NEW_VOLUME"):
        check_engine_schema(engine)


def test_current_revision_is_allowed() -> None:
    engine = create_engine("sqlite+pysqlite:///:memory:")
    with engine.begin() as connection:
        connection.execute(text("CREATE TABLE alembic_version (version_num VARCHAR(32))"))
        connection.execute(
            text("INSERT INTO alembic_version (version_num) VALUES (:revision)"),
            {"revision": EXPECTED_REVISION},
        )
    check_engine_schema(engine)
