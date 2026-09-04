from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

import pytest
from sqlalchemy import Engine, select
from sqlalchemy.orm import Session, sessionmaker

from db.models import (
    ArchiveManifest,
    ArchiveManifestShard,
    Event,
    GovernedDataSource,
    MarketUniverseVersion,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
)
from errors import QfError
from jobs import JobLease, claim_next_job, enqueue_job
from quant_runtime.contracts import (
    ArchiveManifestDescriptor,
    ArchiveManifestSpec,
    ArchiveShardDescriptor,
)
from runners import data_source_preflight


def _now() -> datetime:
    return datetime(2026, 1, 1, tzinfo=UTC)


def _source_spec(*, unsafe_field: str | None = None) -> dict[str, object]:
    config: dict[str, object] = {
        "venue": "polymarket_v2",
        "selection": "all_markets",
        "archive_start": "2026-01-01T00:00:00+00:00",
        "archive_end": "2026-01-01T02:00:00+00:00",
    }
    if unsafe_field == "archive_url":
        config[unsafe_field] = "https://example.invalid/archive.parquet"
    if unsafe_field in {"api_key", "api_token"}:
        config[unsafe_field] = "not-a-secret-to-store"
    return {"kind": "plugin", "config": config}


def _seed_preflight(
    engine: Engine,
    *,
    unsafe_field: str | None = None,
    release_state: str = "ACTIVE",
) -> tuple[JobLease, UUID]:
    now = _now()
    with Session(engine) as session, session.begin():
        universe = MarketUniverseVersion(
            universe_key="TEST",
            version_no=1,
            name="Test Universe",
            state="ACTIVE",
            spec_json={},
            created_at=now,
        )
        release = PluginRelease(
            plugin_id=f"archive_{uuid4().hex[:12]}",
            distribution_name="archive-plugin",
            version="1.0.0",
            api_version="1",
            state=release_state,
            descriptor_snapshot={"capabilities": ["HISTORICAL_IMPORT"]},
        )
        bundle = PluginRuntimeBundle(
            state="READY",
            python_version="3.14",
            qf_version="1",
            environment_path="bundles/test",
        )
        session.add_all((universe, release, bundle))
        session.flush()
        session.add(
            PluginRuntimeBundleMember(
                runtime_bundle_id=bundle.id,
                plugin_release_id=release.id,
                member_role="IMPORTER",
            )
        )
        source = GovernedDataSource(
            name=f"archive-source-{uuid4()}",
            connector_key="archive",
            provider="Test Provider",
            state="ACTIVE",
            universe_scope=[str(universe.id)],
            fields=["archive_shard_metadata"],
            field_schema={"archive_shard_metadata": "object"},
            license_classification="TEST_LICENSE",
            availability_semantics={"available_at": "timestamp_received"},
            preflight_state="PENDING",
            public_config={
                "source_spec": _source_spec(unsafe_field=unsafe_field),
                "plugin_binding": {
                    "plugin_release_id": str(release.id),
                    "plugin_runtime_bundle_id": str(bundle.id),
                },
            },
        )
        session.add(source)
        session.flush()
        enqueue_job(
            session,
            kind="DATA_SOURCE_PREFLIGHT",
            resource_type="governed_data_source",
            resource_id=source.id,
        )
        source_id = source.id
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        claimed = claim_next_job(session, owner="worker", lease_seconds=60)
        assert claimed is not None and claimed.lease_owner is not None
        lease = JobLease(claimed.id, claimed.lease_owner, claimed.attempt)
    return lease, source_id


class _Runtime:
    def __init__(self, *, probe_error: bool = False, mismatched_provider: bool = False) -> None:
        self.probe_error = probe_error
        self.mismatched_provider = mismatched_provider
        self.spec: ArchiveManifestSpec | None = None

    def inspect_archive_manifest(self, spec: ArchiveManifestSpec) -> ArchiveManifestDescriptor:
        self.spec = spec
        start = _now()
        available = ArchiveShardDescriptor(
            shard_key="2026-01-01T00:00:00Z",
            source_url="https://generated.example.invalid/archive-00.parquet",
            coverage_start=start,
            coverage_end=start + timedelta(hours=1),
            size_bytes=100,
            state="AVAILABLE",
            observed_at=start,
        )
        second_state = "PROBE_ERROR" if self.probe_error else "MISSING"
        second = ArchiveShardDescriptor(
            shard_key="2026-01-01T01:00:00Z",
            source_url="https://generated.example.invalid/archive-01.parquet",
            coverage_start=start + timedelta(hours=1),
            coverage_end=start + timedelta(hours=2),
            size_bytes=None,
            state=second_state,
            observed_at=start,
        )
        return ArchiveManifestDescriptor(
            manifest_uri=f"manifest://{spec.manifest_name}",
            provider="Unexpected Provider" if self.mismatched_provider else spec.provider,
            source_license=spec.source_license,
            source_spec=spec.source_spec,
            coverage_start=start,
            coverage_end=start + timedelta(hours=2),
            scanned_until=start,
            shard_count=2,
            total_bytes=100,
            missing_shard_count=0 if self.probe_error else 1,
            probe_error_count=1 if self.probe_error else 0,
            schema_revision="manifest-v1",
            point_in_time_result={"valid": True},
            shards=[available, second],
        )


def _prepare_runner(
    engine: Engine,
    monkeypatch: pytest.MonkeyPatch,
    runtime: _Runtime,
) -> None:
    monkeypatch.setattr(data_source_preflight, "create_database_engine", lambda _: engine)
    monkeypatch.setattr(engine, "dispose", lambda: None)
    monkeypatch.setattr(data_source_preflight, "_runtime", lambda: runtime)


def test_real_manifest_preflight_marks_source_ready_and_preserves_missing_evidence(
    engine: Engine, settings, monkeypatch: pytest.MonkeyPatch
) -> None:  # type: ignore[no-untyped-def]
    lease, source_id = _seed_preflight(engine)
    runtime = _Runtime()
    _prepare_runner(engine, monkeypatch, runtime)

    data_source_preflight.run_data_source_preflight(settings, lease)

    assert runtime.spec is not None
    assert runtime.spec.source_spec == _source_spec()
    with Session(engine) as session:
        source = session.get(GovernedDataSource, source_id)
        assert source is not None
        assert source.preflight_state == "READY"
        manifest_id = UUID(source.public_config["archive_manifest_id"])
        manifest = session.get(ArchiveManifest, manifest_id)
        assert manifest is not None
        assert manifest.data_source_id == source.id
        assert manifest.state == "ACTIVE"
        assert (manifest.missing_shard_count, manifest.probe_error_count) == (1, 0)
        shards = list(
            session.scalars(
                select(ArchiveManifestShard)
                .where(ArchiveManifestShard.manifest_id == manifest.id)
                .order_by(ArchiveManifestShard.coverage_start)
            )
        )
        assert [item.state for item in shards] == ["AVAILABLE", "MISSING"]
        event = session.scalar(select(Event).where(Event.kind == "DATA_SOURCE_PREFLIGHT_COMPLETED"))
        assert event is not None
        assert event.payload == {"manifest_id": str(manifest.id), "missing_shard_count": 1}


def test_probe_error_persists_manifest_evidence_without_marking_source_ready(
    engine: Engine, settings, monkeypatch: pytest.MonkeyPatch
) -> None:  # type: ignore[no-untyped-def]
    lease, source_id = _seed_preflight(engine)
    _prepare_runner(engine, monkeypatch, _Runtime(probe_error=True))

    with pytest.raises(QfError, match="DATA_SOURCE_PREFLIGHT_PROBE_ERROR"):
        data_source_preflight.run_data_source_preflight(settings, lease)

    with Session(engine) as session:
        source = session.get(GovernedDataSource, source_id)
        assert source is not None
        assert source.preflight_state == "PENDING"
        assert "archive_manifest_id" not in source.public_config
        manifest = session.scalar(
            select(ArchiveManifest).where(ArchiveManifest.data_source_id == source.id)
        )
        assert manifest is not None
        assert manifest.state == "INCONCLUSIVE"
        assert manifest.probe_error_count == 1
        event = session.scalar(
            select(Event).where(Event.kind == "DATA_SOURCE_PREFLIGHT_INCONCLUSIVE")
        )
        assert event is not None
        assert event.payload == {"manifest_id": str(manifest.id), "probe_error_count": 1}


def test_preflight_rejects_mismatched_remote_descriptor_without_persisting_ready_facts(
    engine: Engine, settings, monkeypatch: pytest.MonkeyPatch
) -> None:  # type: ignore[no-untyped-def]
    lease, source_id = _seed_preflight(engine)
    _prepare_runner(engine, monkeypatch, _Runtime(mismatched_provider=True))

    with pytest.raises(QfError, match="DATA_SOURCE_PREFLIGHT_DESCRIPTOR_MISMATCH"):
        data_source_preflight.run_data_source_preflight(settings, lease)

    with Session(engine) as session:
        source = session.get(GovernedDataSource, source_id)
        assert source is not None
        assert source.preflight_state == "PENDING"
        assert (
            session.scalar(
                select(ArchiveManifest).where(ArchiveManifest.data_source_id == source.id)
            )
            is None
        )


@pytest.mark.parametrize(
    ("unsafe_field", "error_code"),
    [
        ("archive_url", "DATA_SOURCE_PREFLIGHT_LOCATOR_FORBIDDEN"),
        ("api_key", "DATA_SOURCE_PREFLIGHT_SECRET_FORBIDDEN"),
        ("api_token", "DATA_SOURCE_PREFLIGHT_SECRET_FORBIDDEN"),
    ],
)
def test_preflight_rejects_registered_locator_or_credential_before_runtime(
    engine: Engine,
    settings,
    monkeypatch: pytest.MonkeyPatch,
    unsafe_field: str,
    error_code: str,
) -> None:  # type: ignore[no-untyped-def]
    lease, source_id = _seed_preflight(engine, unsafe_field=unsafe_field)
    _prepare_runner(engine, monkeypatch, _Runtime())

    with pytest.raises(QfError, match=error_code):
        data_source_preflight.run_data_source_preflight(settings, lease)

    with Session(engine) as session:
        source = session.get(GovernedDataSource, source_id)
        assert source is not None
        assert source.preflight_state == "PENDING"
        assert (
            session.scalar(
                select(ArchiveManifest).where(ArchiveManifest.data_source_id == source.id)
            )
            is None
        )


def test_preflight_requires_active_plugin_before_contacting_runtime(
    engine: Engine, settings, monkeypatch: pytest.MonkeyPatch
) -> None:  # type: ignore[no-untyped-def]
    lease, source_id = _seed_preflight(engine, release_state="DRAINING")
    _prepare_runner(engine, monkeypatch, _Runtime())

    with pytest.raises(QfError, match="DATA_SOURCE_PREFLIGHT_PLUGIN_RELEASE_NOT_ACTIVE"):
        data_source_preflight.run_data_source_preflight(settings, lease)

    with Session(engine) as session:
        source = session.get(GovernedDataSource, source_id)
        assert source is not None
        assert source.preflight_state == "PENDING"
