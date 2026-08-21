"""Refuse legacy or unowned schemas before applying the fresh baseline."""

from __future__ import annotations

import sys

from sqlalchemy import Engine, inspect, text

from db.session import create_database_engine
from errors import QfError
from settings import Settings

EXPECTED_REVISION = "0001_initial"


def check_engine_schema(engine: Engine) -> None:
    inspector = inspect(engine)
    tables = set(inspector.get_table_names())
    if not tables:
        return

    if "alembic_version" not in tables:
        raise QfError(
            code="OLD_SCHEMA_REQUIRES_NEW_VOLUME",
            message="Database contains tables not owned by the fresh QuaZonai baseline.",
            status_code=409,
            details={"table_count": len(tables)},
        )

    with engine.connect() as connection:
        revision = connection.execute(
            text("SELECT version_num FROM alembic_version")
        ).scalar_one_or_none()
    if revision != EXPECTED_REVISION:
        raise QfError(
            code="OLD_SCHEMA_REQUIRES_NEW_VOLUME",
            message="Database Alembic revision is not the QuaZonai fresh baseline.",
            status_code=409,
            details={"revision": revision, "expected_revision": EXPECTED_REVISION},
        )


def check_schema() -> None:
    settings = Settings.from_env()
    check_engine_schema(create_database_engine(settings))


def main() -> int:
    try:
        check_schema()
    except QfError as exc:
        print(str(exc), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
