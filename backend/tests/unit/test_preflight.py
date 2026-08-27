from __future__ import annotations

import pytest
from sqlalchemy import create_engine, text

from db.preflight import check_engine_schema, current_revision, owned_revisions
from errors import QfError


def _engine_at_revision(revision: str):
    engine = create_engine("sqlite+pysqlite:///:memory:")
    with engine.begin() as connection:
        connection.execute(text("CREATE TABLE alembic_version (version_num VARCHAR(32))"))
        connection.execute(
            text("INSERT INTO alembic_version (version_num) VALUES (:revision)"),
            {"revision": revision},
        )
    return engine


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
    check_engine_schema(_engine_at_revision(current_revision()))


def test_previous_owned_revision_requires_fresh_volume() -> None:
    revisions = owned_revisions()
    assert "0001_initial" in revisions
    with pytest.raises(QfError, match="OLD_SCHEMA_REQUIRES_NEW_VOLUME"):
        check_engine_schema(_engine_at_revision("0001_initial"))


def test_unknown_revision_is_rejected() -> None:
    with pytest.raises(QfError, match="OLD_SCHEMA_REQUIRES_NEW_VOLUME"):
        check_engine_schema(_engine_at_revision("not_quazonai"))
