"""Refuse unowned schemas while allowing supported QuaZonai migrations."""

from __future__ import annotations

import sys
from pathlib import Path

from alembic.config import Config
from alembic.script import ScriptDirectory
from sqlalchemy import Engine, inspect, text

from db.session import create_database_engine
from errors import QfError
from settings import Settings


def _script_directory() -> ScriptDirectory:
    backend_root = Path(__file__).resolve().parents[2]
    config = Config(str(backend_root / "alembic.ini"))
    return ScriptDirectory.from_config(config)


def owned_revisions() -> set[str]:
    """Return every revision on the single supported upgrade lineage."""
    script = _script_directory()
    heads = script.get_heads()
    if len(heads) != 1:
        raise RuntimeError(f"QuaZonai requires exactly one Alembic head, found {heads}")
    return {revision.revision for revision in script.walk_revisions(head=heads[0], base="base")}


def current_revision() -> str:
    script = _script_directory()
    heads = script.get_heads()
    if len(heads) != 1:
        raise RuntimeError(f"QuaZonai requires exactly one Alembic head, found {heads}")
    return heads[0]


def check_engine_schema(engine: Engine) -> None:
    inspector = inspect(engine)
    tables = set(inspector.get_table_names())
    if not tables:
        return

    if "alembic_version" not in tables:
        raise QfError(
            code="OLD_SCHEMA_REQUIRES_NEW_VOLUME",
            message="Database contains tables outside the QuaZonai migration lineage.",
            status_code=409,
            details={"table_count": len(tables)},
        )

    with engine.connect() as connection:
        revision = connection.execute(
            text("SELECT version_num FROM alembic_version")
        ).scalar_one_or_none()

    allowed = owned_revisions()
    if revision not in allowed:
        raise QfError(
            code="OLD_SCHEMA_REQUIRES_NEW_VOLUME",
            message="Database Alembic revision is not on the supported QuaZonai upgrade lineage.",
            status_code=409,
            details={
                "revision": revision,
                "current_revision": current_revision(),
                "owned_revisions": sorted(allowed),
            },
        )


def check_schema() -> None:
    settings = Settings.from_env()
    engine = create_database_engine(settings)
    try:
        check_engine_schema(engine)
    finally:
        engine.dispose()


def main() -> int:
    try:
        check_schema()
    except QfError as exc:
        print(str(exc), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
