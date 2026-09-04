from __future__ import annotations

import importlib.util
from datetime import UTC, datetime
from pathlib import Path
from uuid import UUID, uuid4

import pytest
from alembic.migration import MigrationContext
from alembic.operations import Operations
from sqlalchemy import (
    Column,
    DateTime,
    ForeignKey,
    Integer,
    JSON,
    MetaData,
    String,
    Table,
    Text,
    Uuid,
    inspect,
    select,
)
from sqlalchemy.exc import IntegrityError

from db.models import Base


def _migration() -> object:
    path = Path(__file__).parents[2] / "alembic/versions/0020_package_before_approval.py"
    spec = importlib.util.spec_from_file_location("migration_0020", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _migration_0027() -> object:
    path = Path(__file__).parents[2] / "alembic/versions/0027_candidate_package_build.py"
    spec = importlib.util.spec_from_file_location("migration_0027", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _package_build_legacy_tables(
    metadata: MetaData, *, candidate_has_assembly_input: bool = False
) -> tuple[Table, Table, Table]:
    candidate_columns = [Column("id", Uuid(), primary_key=True)]
    if candidate_has_assembly_input:
        candidate_columns.append(Column("assembly_input_id", Uuid()))
    candidates = Table("portfolio_candidates", metadata, *candidate_columns)
    packages = Table(
        "candidate_packages",
        metadata,
        Column("id", Uuid(), primary_key=True),
        Column("candidate_id", Uuid(), ForeignKey("portfolio_candidates.id"), nullable=False),
        Column("revision", Integer(), nullable=False),
        Column("contract_version", String(40), nullable=False),
        Column("state", String(40), nullable=False),
        Column("manifest_json", JSON(), nullable=False),
        Column("relative_path", Text(), nullable=False),
        Column("payload", JSON(), nullable=False),
        Column("created_at", DateTime(timezone=True), nullable=False),
    )
    jobs = Table(
        "jobs",
        metadata,
        Column("id", Uuid(), primary_key=True),
        Column("kind", String(100), nullable=False),
        Column("resource_type", String(100), nullable=False),
        Column("resource_id", Uuid(), nullable=False),
        Column("state", String(20), nullable=False),
    )
    return candidates, packages, jobs


def test_0020_stales_legacy_packages_and_binds_their_history(engine) -> None:
    legacy = MetaData()
    candidates = Table("portfolio_candidates", legacy, Column("id", Uuid(), primary_key=True))
    approvals = Table(
        "approval_snapshots",
        legacy,
        Column("id", Uuid(), primary_key=True),
        Column("candidate_id", Uuid(), ForeignKey("portfolio_candidates.id"), nullable=False),
        Column("purpose", String(40), nullable=False),
        Column("state", String(40), nullable=False),
        Column("stale_reason", Text()),
        Column("revision", Integer(), nullable=False),
    )
    packages = Table(
        "candidate_packages",
        legacy,
        Column("id", Uuid(), primary_key=True),
        Column(
            "approval_id", Uuid(), ForeignKey("approval_snapshots.id"), nullable=False, unique=True
        ),
        Column("candidate_id", Uuid(), ForeignKey("portfolio_candidates.id"), nullable=False),
        Column("contract_version", String(40), nullable=False),
        Column("state", String(40), nullable=False),
        Column("manifest_json", JSON(), nullable=False),
        Column("relative_path", Text(), nullable=False),
        Column("payload", JSON(), nullable=False),
        Column("created_at", DateTime(timezone=True), nullable=False),
    )

    Base.metadata.drop_all(engine)
    legacy.create_all(engine)
    pending_candidate_id, approved_candidate_id = uuid4(), uuid4()
    pending_approval_id, approved_approval_id = uuid4(), uuid4()
    pending_package_id, approved_package_id = uuid4(), uuid4()
    now = datetime.now(UTC)
    with engine.begin() as connection:
        connection.execute(
            candidates.insert(),
            [{"id": pending_candidate_id}, {"id": approved_candidate_id}],
        )
        connection.execute(
            approvals.insert(),
            [
                {
                    "id": pending_approval_id,
                    "candidate_id": pending_candidate_id,
                    "purpose": "PAPER",
                    "state": "PENDING",
                    "revision": 1,
                },
                {
                    "id": approved_approval_id,
                    "candidate_id": approved_candidate_id,
                    "purpose": "PAPER",
                    "state": "APPROVED",
                    "revision": 1,
                },
            ],
        )
        connection.execute(
            packages.insert(),
            [
                {
                    "id": pending_package_id,
                    "approval_id": pending_approval_id,
                    "candidate_id": pending_candidate_id,
                    "contract_version": "1",
                    "state": "AVAILABLE",
                    "manifest_json": {},
                    "relative_path": "legacy/pending.zip",
                    "payload": {},
                    "created_at": now,
                },
                {
                    "id": approved_package_id,
                    "approval_id": approved_approval_id,
                    "candidate_id": approved_candidate_id,
                    "contract_version": "1",
                    "state": "AVAILABLE",
                    "manifest_json": {},
                    "relative_path": "legacy/approved.zip",
                    "payload": {},
                    "created_at": now,
                },
            ],
        )
        context = MigrationContext.configure(connection)
        with Operations.context(context):
            _migration().upgrade()

        inspector = inspect(connection)
        assert "approval_id" not in {
            column["name"] for column in inspector.get_columns("candidate_packages")
        }
        assert {"candidate_package_id", "candidate_package_revision"} <= {
            column["name"] for column in inspector.get_columns("approval_snapshots")
        }

        migrated_approvals = Table("approval_snapshots", MetaData(), autoload_with=connection)
        migrated_packages = Table("candidate_packages", MetaData(), autoload_with=connection)
        pending = (
            connection.execute(
                select(migrated_approvals).where(migrated_approvals.c.id == pending_approval_id)
            )
            .mappings()
            .one()
        )
        approved = (
            connection.execute(
                select(migrated_approvals).where(migrated_approvals.c.id == approved_approval_id)
            )
            .mappings()
            .one()
        )
        assert UUID(str(pending["candidate_package_id"])) == pending_package_id
        assert pending["candidate_package_revision"] == 1
        assert pending["state"] == "STALE"
        assert pending["stale_reason"] == "PACKAGE_BEFORE_APPROVAL_REQUIRED"
        assert pending["revision"] == 2
        assert UUID(str(approved["candidate_package_id"])) == approved_package_id
        assert approved["candidate_package_revision"] == 1
        assert approved["state"] == "APPROVED"
        assert connection.execute(
            select(migrated_packages.c.state).order_by(migrated_packages.c.id)
        ).scalars().all() == ["STALE", "STALE"]


def test_0027_preserves_historical_package_state_and_adds_build_identity(engine) -> None:
    legacy = MetaData()
    candidates = Table("portfolio_candidates", legacy, Column("id", Uuid(), primary_key=True))
    packages = Table(
        "candidate_packages",
        legacy,
        Column("id", Uuid(), primary_key=True),
        Column("candidate_id", Uuid(), ForeignKey("portfolio_candidates.id"), nullable=False),
        Column("revision", Integer(), nullable=False),
        Column("contract_version", String(40), nullable=False),
        Column("state", String(40), nullable=False),
        Column("manifest_json", JSON(), nullable=False),
        Column("relative_path", Text(), nullable=False),
        Column("payload", JSON(), nullable=False),
        Column("created_at", DateTime(timezone=True), nullable=False),
    )
    Table(
        "jobs",
        legacy,
        Column("id", Uuid(), primary_key=True),
        Column("kind", String(100), nullable=False),
        Column("resource_type", String(100), nullable=False),
        Column("resource_id", Uuid(), nullable=False),
        Column("state", String(20), nullable=False),
    )
    Base.metadata.drop_all(engine)
    legacy.create_all(engine)
    candidate_id, package_id = uuid4(), uuid4()
    now = datetime.now(UTC)
    with engine.begin() as connection:
        connection.execute(candidates.insert(), {"id": candidate_id})
        connection.execute(
            packages.insert(),
            {
                "id": package_id,
                "candidate_id": candidate_id,
                "revision": 1,
                "contract_version": "1",
                "state": "LEGACY_NON_EXECUTABLE",
                "manifest_json": {},
                "relative_path": "legacy/archive.zip",
                "payload": {},
                "created_at": now,
            },
        )
        context = MigrationContext.configure(connection)
        with Operations.context(context):
            _migration_0027().upgrade()

        migrated = Table("candidate_packages", MetaData(), autoload_with=connection)
        assert connection.execute(
            select(migrated.c.state).where(migrated.c.id == package_id)
        ).scalar_one() == "LEGACY_NON_EXECUTABLE"
        indexes = {index["name"] for index in inspect(connection).get_indexes("jobs")}
        assert "uq_candidate_package_build_job_active" in indexes
        with pytest.raises(IntegrityError):
            connection.execute(
                migrated.insert(),
                {
                    "id": uuid4().hex,
                    "candidate_id": candidate_id.hex,
                    "revision": 1,
                    "contract_version": "1",
                    "state": "STALE",
                    "manifest_json": {},
                    "relative_path": "legacy/duplicate.zip",
                    "payload": {},
                    "created_at": now,
                },
            )


def test_0027_fails_closed_on_historical_duplicate_identity(engine) -> None:
    legacy = MetaData()
    candidates = Table("portfolio_candidates", legacy, Column("id", Uuid(), primary_key=True))
    packages = Table(
        "candidate_packages",
        legacy,
        Column("id", Uuid(), primary_key=True),
        Column("candidate_id", Uuid(), ForeignKey("portfolio_candidates.id"), nullable=False),
        Column("revision", Integer(), nullable=False),
        Column("contract_version", String(40), nullable=False),
        Column("state", String(40), nullable=False),
        Column("manifest_json", JSON(), nullable=False),
        Column("relative_path", Text(), nullable=False),
        Column("payload", JSON(), nullable=False),
        Column("created_at", DateTime(timezone=True), nullable=False),
    )
    Table(
        "jobs",
        legacy,
        Column("id", Uuid(), primary_key=True),
        Column("kind", String(100), nullable=False),
        Column("resource_type", String(100), nullable=False),
        Column("resource_id", Uuid(), nullable=False),
        Column("state", String(20), nullable=False),
    )
    Base.metadata.drop_all(engine)
    legacy.create_all(engine)
    candidate_id = uuid4()
    now = datetime.now(UTC)
    with engine.begin() as connection:
        connection.execute(candidates.insert(), {"id": candidate_id})
        connection.execute(
            packages.insert(),
            [
                {
                    "id": uuid4(),
                    "candidate_id": candidate_id,
                    "revision": 1,
                    "contract_version": "1",
                    "state": "STALE",
                    "manifest_json": {},
                    "relative_path": "legacy/one.zip",
                    "payload": {},
                    "created_at": now,
                },
                {
                    "id": uuid4(),
                    "candidate_id": candidate_id,
                    "revision": 1,
                    "contract_version": "1",
                    "state": "AVAILABLE",
                    "manifest_json": {},
                    "relative_path": "legacy/two.zip",
                    "payload": {},
                    "created_at": now,
                },
            ],
        )
        context = MigrationContext.configure(connection)
        with Operations.context(context), pytest.raises(
            RuntimeError, match="CANDIDATE_PACKAGE_LEGACY_IDENTITY_CONFLICT"
        ):
            _migration_0027().upgrade()


def test_0027_fails_closed_on_unknown_historical_package_state(engine) -> None:
    legacy = MetaData()
    candidates, packages, _jobs = _package_build_legacy_tables(legacy)
    Base.metadata.drop_all(engine)
    legacy.create_all(engine)
    candidate_id = uuid4()
    with engine.begin() as connection:
        connection.execute(candidates.insert(), {"id": candidate_id})
        connection.execute(
            packages.insert(),
            {
                "id": uuid4(),
                "candidate_id": candidate_id,
                "revision": 1,
                "contract_version": "1",
                "state": "UNKNOWN_HISTORICAL_STATE",
                "manifest_json": {},
                "relative_path": "legacy/unknown.zip",
                "payload": {},
                "created_at": datetime.now(UTC),
            },
        )
        context = MigrationContext.configure(connection)
        with Operations.context(context), pytest.raises(
            RuntimeError, match="CANDIDATE_PACKAGE_LEGACY_STATE_CONFLICT"
        ):
            _migration_0027().upgrade()


def test_0027_downgrade_removes_constraints_without_typed_package_facts(engine) -> None:
    legacy = MetaData()
    candidates, packages, _jobs = _package_build_legacy_tables(
        legacy, candidate_has_assembly_input=True
    )
    Base.metadata.drop_all(engine)
    legacy.create_all(engine)
    candidate_id = uuid4()
    with engine.begin() as connection:
        connection.execute(candidates.insert(), {"id": candidate_id, "assembly_input_id": None})
        connection.execute(
            packages.insert(),
            {
                "id": uuid4(),
                "candidate_id": candidate_id,
                "revision": 1,
                "contract_version": "1",
                "state": "STALE",
                "manifest_json": {},
                "relative_path": "legacy/stale.zip",
                "payload": {},
                "created_at": datetime.now(UTC),
            },
        )
        context = MigrationContext.configure(connection)
        with Operations.context(context):
            _migration_0027().upgrade()
        with Operations.context(context):
            _migration_0027().downgrade()

        inspector = inspect(connection)
        package_indexes = {
            index["name"] for index in inspector.get_indexes("candidate_packages")
        }
        job_indexes = {index["name"] for index in inspector.get_indexes("jobs")}
        assert "uq_candidate_package_building_candidate" not in package_indexes
        assert "uq_candidate_package_build_job_active" not in job_indexes


def test_0027_downgrade_refuses_typed_package_facts(engine) -> None:
    legacy = MetaData()
    candidates, packages, _jobs = _package_build_legacy_tables(
        legacy, candidate_has_assembly_input=True
    )
    Base.metadata.drop_all(engine)
    legacy.create_all(engine)
    candidate_id = uuid4()
    with engine.begin() as connection:
        connection.execute(
            candidates.insert(),
            {"id": candidate_id, "assembly_input_id": uuid4()},
        )
        connection.execute(
            packages.insert(),
            {
                "id": uuid4(),
                "candidate_id": candidate_id,
                "revision": 1,
                "contract_version": "1",
                "state": "BUILDING",
                "manifest_json": {},
                "relative_path": "typed/building.zip",
                "payload": {},
                "created_at": datetime.now(UTC),
            },
        )
        context = MigrationContext.configure(connection)
        with Operations.context(context):
            _migration_0027().upgrade()
        with Operations.context(context), pytest.raises(
            RuntimeError, match="CANDIDATE_PACKAGE_BUILD_DOWNGRADE_BLOCKED"
        ):
            _migration_0027().downgrade()


def test_0027_downgrade_refuses_active_candidate_package_job(engine) -> None:
    legacy = MetaData()
    candidates, _packages, jobs = _package_build_legacy_tables(
        legacy, candidate_has_assembly_input=True
    )
    Base.metadata.drop_all(engine)
    legacy.create_all(engine)
    candidate_id = uuid4()
    with engine.begin() as connection:
        connection.execute(candidates.insert(), {"id": candidate_id, "assembly_input_id": None})
        connection.execute(
            jobs.insert(),
            {
                "id": uuid4(),
                "kind": "CANDIDATE_PACKAGE_BUILD",
                "resource_type": "portfolio_candidate",
                "resource_id": candidate_id,
                "state": "READY",
            },
        )
        context = MigrationContext.configure(connection)
        with Operations.context(context):
            _migration_0027().upgrade()
        with Operations.context(context), pytest.raises(
            RuntimeError, match="CANDIDATE_PACKAGE_BUILD_DOWNGRADE_BLOCKED"
        ):
            _migration_0027().downgrade()
