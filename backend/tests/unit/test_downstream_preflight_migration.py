from __future__ import annotations

import importlib.util
from pathlib import Path
from uuid import UUID, uuid4

from alembic.migration import MigrationContext
from alembic.operations import Operations
from sqlalchemy import Boolean, Column, JSON, MetaData, String, Table, Uuid, inspect, select

from db.models import Base


def _migration() -> object:
    path = Path(__file__).parents[2] / "alembic/versions/0022_downstream_preflight.py"
    spec = importlib.util.spec_from_file_location("migration_0022", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_0022_demotes_legacy_downstreams_without_receipts(engine) -> None:
    Base.metadata.drop_all(engine)
    legacy = MetaData()
    systems = Table(
        "downstream_systems",
        legacy,
        Column("id", Uuid(), primary_key=True),
        Column("name", String(200), nullable=False),
        Column("environment_type", String(40), nullable=False),
        Column("enabled", Boolean(), nullable=False),
        Column("package_contract_version", String(40), nullable=False),
        Column("feedback_contract_version", String(40), nullable=False),
        Column("compatibility", JSON(), nullable=False),
        Column("preflight_state", String(40), nullable=False),
        Column("public_config", JSON(), nullable=False),
    )
    legacy.create_all(engine)
    ready_id, pending_id = uuid4(), uuid4()
    with engine.begin() as connection:
        connection.execute(
            systems.insert(),
            [
                {
                    "id": ready_id,
                    "name": "legacy-ready",
                    "environment_type": "PAPER",
                    "enabled": True,
                    "package_contract_version": "1",
                    "feedback_contract_version": "1",
                    "compatibility": [],
                    "preflight_state": "READY",
                    "public_config": {},
                },
                {
                    "id": pending_id,
                    "name": "legacy-pending",
                    "environment_type": "LIVE",
                    "enabled": True,
                    "package_contract_version": "1",
                    "feedback_contract_version": "1",
                    "compatibility": [],
                    "preflight_state": "PENDING",
                    "public_config": {},
                },
            ],
        )
        context = MigrationContext.configure(connection)
        with Operations.context(context):
            _migration().upgrade()

        migrated = Table("downstream_systems", MetaData(), autoload_with=connection)
        rows = connection.execute(
            select(migrated.c.id, migrated.c.preflight_state, migrated.c.revision).order_by(
                migrated.c.name
            )
        ).all()
        assert [(UUID(str(item_id)), state, revision) for item_id, state, revision in rows] == [
            (pending_id, "PENDING", 1),
            (ready_id, "PENDING", 1),
        ]
        assert "revision" in {
            column["name"] for column in inspect(connection).get_columns("downstream_systems")
        }
        assert "ck_downstream_system_revision" in {
            check["name"]
            for check in inspect(connection).get_check_constraints("downstream_systems")
        }

        with Operations.context(context):
            _migration().downgrade()
        downgraded = Table("downstream_systems", MetaData(), autoload_with=connection)
        assert "revision" not in downgraded.c

        with Operations.context(context):
            _migration().upgrade()
        reupgraded = Table("downstream_systems", MetaData(), autoload_with=connection)
        assert [(UUID(str(item_id)), state, revision) for item_id, state, revision in connection.execute(
            select(
                reupgraded.c.id,
                reupgraded.c.preflight_state,
                reupgraded.c.revision,
            ).order_by(reupgraded.c.name)
        )] == [
            (pending_id, "PENDING", 1),
            (ready_id, "PENDING", 1),
        ]
