from __future__ import annotations

import importlib.util
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

import pytest
from alembic import command
from alembic.config import Config
from alembic.migration import MigrationContext
from alembic.operations import Operations
from sqlalchemy import Engine, MetaData, Table, create_engine, inspect, select, text
from sqlalchemy.exc import IntegrityError


_BACKEND_ROOT = Path(__file__).parents[2]


def _migration() -> object:
    path = _BACKEND_ROOT / "alembic/versions/0024_typed_portfolio_configuration.py"
    spec = importlib.util.spec_from_file_location("migration_0024", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_0024_preserves_legacy_configuration_without_guessing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    database_url = f"sqlite+pysqlite:///{tmp_path / 'migration.sqlite'}"
    monkeypatch.setenv("QUAZONAI_ENV", "test")
    monkeypatch.setenv("QUAZONAI_DATABASE_URL", database_url)
    monkeypatch.setenv("QUAZONAI_ALEMBIC_URL", database_url)
    config = Config(str(_BACKEND_ROOT / "alembic.ini"))
    command.upgrade(config, "0023_trusted_alpha_evaluation")

    engine = create_engine(database_url)
    now = datetime(2026, 9, 3, tzinfo=UTC)
    mandate_id, version_id, capital_id = uuid4(), uuid4(), uuid4()
    try:
        with engine.begin() as connection:
            mandates = Table("portfolio_mandates", MetaData(), autoload_with=connection)
            versions = Table("portfolio_mandate_versions", MetaData(), autoload_with=connection)
            capitals = Table("capital_context_versions", MetaData(), autoload_with=connection)
            connection.execute(
                mandates.insert().values(
                    id=str(mandate_id),
                    key="legacy-mandate",
                    name="Legacy Mandate",
                    enabled=True,
                    latest_version_id=str(version_id),
                    spec_json={},
                    state="ACTIVE",
                    created_at=now,
                    updated_at=now,
                )
            )
            connection.execute(
                versions.insert().values(
                    id=str(version_id),
                    portfolio_mandate_id=str(mandate_id),
                    version_no=1,
                    base_currency="USD",
                    objective="MAXIMIZE_NET_RETURN",
                    eligible_alpha_roles=["PRIMARY_ALPHA"],
                    eligible_universe_version_ids=[],
                    minimum_alpha_count=2,
                    capital_config={"deployable_capital": 1},
                    risk_config={"model": "LEGACY"},
                    cost_config={"model": "LEGACY"},
                    capacity_config={"model": "LEGACY"},
                    promotion_policy={"paper": "LEGACY"},
                    constraint_config={"gross_exposure_limit": 1},
                    created_at=now,
                )
            )
            connection.execute(
                capitals.insert().values(
                    id=str(capital_id),
                    source_type="ADMIN",
                    source_downstream_system_id=None,
                    base_currency="USD",
                    deployable_capital="1000",
                    observed_at=now,
                    valid_until=datetime(2026, 10, 3, tzinfo=UTC),
                    notes=None,
                    created_at=now,
                    updated_at=now,
                )
            )

            context = MigrationContext.configure(connection)
            with Operations.context(context):
                _migration().upgrade()

            migrated_versions = Table(
                "portfolio_mandate_versions", MetaData(), autoload_with=connection
            )
            migrated_capitals = Table("capital_context_versions", MetaData(), autoload_with=connection)
            assert connection.execute(
                select(migrated_versions.c.policy_family).where(
                    migrated_versions.c.id == str(version_id)
                )
            ).scalar_one() is None
            assert connection.execute(
                select(migrated_capitals.c.configuration_contract_version).where(
                    migrated_capitals.c.id == str(capital_id)
                )
            ).scalar_one() is None
            with pytest.raises(IntegrityError):
                connection.execute(
                    migrated_versions.insert().values(
                        id=str(uuid4()),
                        portfolio_mandate_id=str(mandate_id),
                        version_no=2,
                        base_currency="USD",
                        objective="MAXIMIZE_NET_RETURN",
                        eligible_alpha_roles=[],
                        eligible_universe_version_ids=[],
                        minimum_alpha_count=2,
                        capital_config={},
                        risk_config={},
                        cost_config={},
                        capacity_config={},
                        promotion_policy={},
                        constraint_config={},
                        policy_family="LONG_ONLY_MEAN_VARIANCE_V1",
                    )
                )
            assert "configuration_contract_version" in {
                column["name"]
                for column in inspect(connection).get_columns("capital_context_versions")
            }

            with Operations.context(context):
                _migration().downgrade()
            downgraded = Table("portfolio_mandate_versions", MetaData(), autoload_with=connection)
            assert "policy_family" not in downgraded.c

            with Operations.context(context):
                _migration().upgrade()
            reupgraded = Table("portfolio_mandate_versions", MetaData(), autoload_with=connection)
            assert connection.execute(
                select(reupgraded.c.policy_family).where(reupgraded.c.id == str(version_id))
            ).scalar_one() is None
    finally:
        engine.dispose()


def test_0024_widens_postgresql_alembic_version_for_its_revision(engine: Engine) -> None:
    if engine.dialect.name != "postgresql":
        pytest.skip("PostgreSQL-specific Alembic version width check")
    migration = _migration()
    with engine.begin() as connection:
        connection.execute(
            text("CREATE TEMPORARY TABLE alembic_version (version_num VARCHAR(32) NOT NULL) ON COMMIT DROP")
        )
        context = MigrationContext.configure(connection)
        with Operations.context(context):
            migration._widen_alembic_version_for_long_revision()
        connection.execute(
            text("INSERT INTO alembic_version (version_num) VALUES (:version)"),
            {"version": migration.revision},
        )
