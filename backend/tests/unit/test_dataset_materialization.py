from __future__ import annotations

from dataclasses import replace
from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

import pytest
from sqlalchemy import Engine, select
from sqlalchemy.orm import Session, sessionmaker

from db.models import (
    ArchiveManifest,
    ArchiveManifestShard,
    DataQualityResult,
    DatasetRevision,
    GovernedDataSource,
    Job,
    MarketUniverseVersion,
    NautilusCatalogBinding,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
)
from errors import QfError
from jobs import JobLease, claim_next_job, enqueue_job, release_expired_leases
from quant_runtime.contracts import CatalogDescriptor
from runners import dataset_materialization


def _now() -> datetime:
    return datetime(2026, 1, 1, tzinfo=UTC)


def _seed_pending_materialization(
    engine: Engine,
    *,
    source_ready: bool = True,
    data_class: str = "VENDOR",
    missing_shard: bool = False,
    partition: str = "DISCOVERY",
) -> tuple[JobLease, UUID]:
    now = _now()
    coverage_end = now + timedelta(hours=2 if missing_shard else 1)
    with Session(engine) as session, session.begin():
        universe = MarketUniverseVersion(
            universe_key="TEST",
            version_no=1,
            name="Test Universe",
            state="ACTIVE",
            spec_json={},
            created_at=now,
        )
        source = GovernedDataSource(
            name=f"archive-source-{uuid4()}",
            connector_key="archive",
            provider="Test Provider",
            state="ACTIVE",
            universe_scope=[],
            fields=[],
            preflight_state="READY" if source_ready else "PENDING",
            public_config={},
        )
        release = PluginRelease(
            plugin_id=f"archive_{uuid4().hex[:12]}",
            distribution_name="archive-plugin",
            version="1.0.0",
            api_version="1",
            state="ACTIVE",
            descriptor_snapshot={"capabilities": ["HISTORICAL_IMPORT"]},
        )
        bundle = PluginRuntimeBundle(
            state="READY",
            python_version="3.14",
            qf_version="1",
            environment_path="bundles/test",
        )
        session.add_all((universe, source, release, bundle))
        session.flush()
        session.add(
            PluginRuntimeBundleMember(
                runtime_bundle_id=bundle.id,
                plugin_release_id=release.id,
                member_role="IMPORTER",
            )
        )
        manifest = ArchiveManifest(
            manifest_uri=f"manifest://test-{uuid4()}",
            data_source_id=source.id,
            universe_version_id=universe.id,
            provider="Test Provider",
            source_license="TEST",
            source_spec={"kind": "plugin", "config": {"venue": "test"}},
            coverage_start=now,
            coverage_end=coverage_end,
            scanned_until=now,
            shard_count=2 if missing_shard else 1,
            total_bytes=100,
            schema_revision="manifest-v1",
            state="ACTIVE",
            point_in_time_result={"valid": True},
        )
        session.add(manifest)
        session.flush()
        session.add(
            ArchiveManifestShard(
                manifest_id=manifest.id,
                shard_key="20260101T00",
                source_url="https://example.invalid/archive.parquet",
                coverage_start=now,
                coverage_end=now + timedelta(hours=1),
                size_bytes=100,
                state="AVAILABLE",
                observed_at=now,
            )
        )
        if missing_shard:
            session.add(
                ArchiveManifestShard(
                    manifest_id=manifest.id,
                    shard_key="20260101T01",
                    source_url="https://example.invalid/missing.parquet",
                    coverage_start=now + timedelta(hours=1),
                    coverage_end=coverage_end,
                    size_bytes=None,
                    state="MISSING",
                    observed_at=now,
                )
            )
        source.public_config = {
            "archive_manifest_id": str(manifest.id),
            "plugin_binding": {
                "plugin_release_id": str(release.id),
                "plugin_runtime_bundle_id": str(bundle.id),
            },
        }
        revision = DatasetRevision(
            data_source_id=source.id,
            universe_version_id=universe.id,
            universe_name=universe.name,
            revision_no=1,
            data_class=data_class,
            origin="test",
            ingested_at=now,
            promotability="NON_PROMOTABLE",
            schema_version="actual-v1",
            event_start=now,
            event_end=coverage_end,
            available_start=now,
            available_end=coverage_end,
            quality_state="PENDING",
            point_in_time_state="PENDING",
            partition=partition,
            materialization_request={
                "instrument_scope": ["TEST.INSTRUMENT"],
                "quality_requirements": {"coverage": "required"},
                "point_in_time_requirements": {"available_at": "required"},
            },
            created_at=now,
        )
        session.add(revision)
        session.flush()
        enqueue_job(
            session,
            kind="DATASET_MATERIALIZATION",
            resource_type="dataset_revision",
            resource_id=revision.id,
        )
        dataset_id = revision.id
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        claimed = claim_next_job(session, owner="worker", lease_seconds=60)
        assert claimed is not None and claimed.lease_owner is not None
        lease = JobLease(claimed.id, claimed.lease_owner, claimed.attempt)
    return lease, dataset_id


class _Runtime:
    def ingest(self, spec, *, timeout_seconds):  # type: ignore[no-untyped-def]
        assert spec.catalog_name.startswith("dataset-")
        assert spec.source_shards and len(spec.source_shards) == 1
        assert timeout_seconds >= 120
        now = _now()
        requested_hours = int(spec.source_spec["materialization"]["requested_shard_count"])
        return CatalogDescriptor(
            catalog_uri=f"catalog://{spec.catalog_name}",
            provider=spec.provider,
            source_license=spec.source_license,
            source_spec=spec.source_spec,
            nautilus_data_type="QuoteTick",
            instrument_scope=["TEST.INSTRUMENT"],
            event_start=now,
            event_end=now + timedelta(hours=requested_hours),
            available_start=now,
            available_end=now + timedelta(hours=requested_hours),
            row_count=10,
            schema_revision="actual-v1",
            quality_result={"valid": True, "sorted": True},
            point_in_time_result={"valid": True, "available_time_preserved": True},
        )


class _PointInTimeInvalidRuntime(_Runtime):
    def ingest(self, spec, *, timeout_seconds):  # type: ignore[no-untyped-def]
        return (
            super()
            .ingest(spec, timeout_seconds=timeout_seconds)
            .model_copy(update={"point_in_time_result": {"valid": False}})
        )


class _WrongDescriptorRuntime(_Runtime):
    def ingest(self, spec, *, timeout_seconds):  # type: ignore[no-untyped-def]
        return super().ingest(spec, timeout_seconds=timeout_seconds).model_copy(
            update={"instrument_scope": ["OTHER.INSTRUMENT"]}
        )


def test_registered_archive_worker_persists_true_descriptor_evidence(
    engine: Engine, settings, monkeypatch
) -> None:  # type: ignore[no-untyped-def]
    lease, dataset_id = _seed_pending_materialization(engine)
    monkeypatch.setattr(dataset_materialization, "create_database_engine", lambda _: engine)
    monkeypatch.setattr(engine, "dispose", lambda: None)
    monkeypatch.setattr(dataset_materialization, "_runtime", _Runtime)

    dataset_materialization.run_dataset_materialization(
        replace(settings, plugin_job_timeout_seconds=300), lease
    )

    with Session(engine) as session:
        revision = session.get(DatasetRevision, dataset_id)
        assert revision is not None
        assert (revision.quality_state, revision.point_in_time_state, revision.promotability) == (
            "VALID",
            "VALID",
            "PROMOTABLE",
        )
        assert revision.schema_version == "actual-v1"
        results = list(
            session.scalars(
                select(DataQualityResult)
                .where(DataQualityResult.dataset_revision_id == dataset_id)
                .order_by(DataQualityResult.check_kind)
            )
        )
        assert [(item.check_kind, item.state) for item in results] == [
            ("POINT_IN_TIME", "VALID"),
            ("QUALITY", "VALID"),
        ]
        assert results[1].summary["remote_result"] == {"valid": True, "sorted": True}
        binding = session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.dataset_revision_id == dataset_id
            )
        )
        assert binding is not None
        assert binding.catalog_uri.startswith("catalog://dataset-")


def test_reclaimed_materializer_cannot_persist_stale_descriptor_facts(
    engine: Engine, settings, monkeypatch
) -> None:  # type: ignore[no-untyped-def]
    lease, dataset_id = _seed_pending_materialization(engine)
    monkeypatch.setattr(dataset_materialization, "create_database_engine", lambda _: engine)
    monkeypatch.setattr(engine, "dispose", lambda: None)

    class ReclaimingRuntime(_Runtime):
        def ingest(self, spec, *, timeout_seconds):  # type: ignore[no-untyped-def]
            current = datetime.now(UTC)
            with Session(engine) as session, session.begin():
                job = session.get(Job, lease.job_id)
                assert job is not None
                job.lease_expires_at = current - timedelta(seconds=1)
            with Session(engine) as session, session.begin():
                assert release_expired_leases(session, now=current) == 1
                reclaimed = claim_next_job(session, owner=lease.owner, lease_seconds=60, now=current)
                assert reclaimed is not None and reclaimed.attempt == lease.attempt + 1
            return super().ingest(spec, timeout_seconds=timeout_seconds)

    monkeypatch.setattr(dataset_materialization, "_runtime", ReclaimingRuntime)

    with pytest.raises(QfError, match="JOB_LEASE_LOST"):
        dataset_materialization.run_dataset_materialization(
            replace(settings, plugin_job_timeout_seconds=300), lease
        )

    with Session(engine) as session:
        revision = session.get(DatasetRevision, dataset_id)
        job = session.get(Job, lease.job_id)
        assert revision is not None
        assert job is not None
        assert (revision.quality_state, revision.point_in_time_state) == ("PENDING", "PENDING")
        assert session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.dataset_revision_id == dataset_id
            )
        ) is None
        assert (
            session.scalar(
                select(DataQualityResult).where(DataQualityResult.dataset_revision_id == dataset_id)
            )
            is None
        )
        assert (job.state, job.lease_owner, job.attempt) == (
            "LEASED",
            lease.owner,
            lease.attempt + 1,
        )


def test_unready_source_remains_pending_operationally(
    engine: Engine, settings, monkeypatch
) -> None:  # type: ignore[no-untyped-def]
    lease, dataset_id = _seed_pending_materialization(engine, source_ready=False)
    monkeypatch.setattr(dataset_materialization, "create_database_engine", lambda _: engine)
    monkeypatch.setattr(engine, "dispose", lambda: None)

    with pytest.raises(QfError, match="DATASET_MATERIALIZATION_SOURCE_NOT_READY"):
        dataset_materialization.run_dataset_materialization(settings, lease)

    with Session(engine) as session:
        revision = session.get(DatasetRevision, dataset_id)
        assert revision is not None
        assert (revision.quality_state, revision.point_in_time_state, revision.promotability) == (
            "PENDING",
            "PENDING",
            "NON_PROMOTABLE",
        )
        assert session.scalar(
            select(DataQualityResult).where(DataQualityResult.dataset_revision_id == dataset_id)
        ) is None


def test_sealed_partition_is_rejected_without_creating_a_catalog_binding(
    engine: Engine, settings, monkeypatch
) -> None:  # type: ignore[no-untyped-def]
    lease, dataset_id = _seed_pending_materialization(engine, partition="SEALED")
    monkeypatch.setattr(dataset_materialization, "create_database_engine", lambda _: engine)
    monkeypatch.setattr(engine, "dispose", lambda: None)

    with pytest.raises(QfError, match="DATASET_MATERIALIZATION_SEALED_PARTITION_FORBIDDEN"):
        dataset_materialization.run_dataset_materialization(settings, lease)

    with Session(engine) as session:
        revision = session.get(DatasetRevision, dataset_id)
        binding = session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.dataset_revision_id == dataset_id
            )
        )
        assert revision is not None
        assert (revision.quality_state, revision.point_in_time_state, revision.promotability) == (
            "INVALID",
            "INVALID",
            "NON_PROMOTABLE",
        )
        assert binding is None


def test_fixture_data_remains_non_promotable_after_valid_materialization(
    engine: Engine, settings, monkeypatch
) -> None:  # type: ignore[no-untyped-def]
    lease, dataset_id = _seed_pending_materialization(engine, data_class="FIXTURE")
    monkeypatch.setattr(dataset_materialization, "create_database_engine", lambda _: engine)
    monkeypatch.setattr(engine, "dispose", lambda: None)
    monkeypatch.setattr(dataset_materialization, "_runtime", _Runtime)

    dataset_materialization.run_dataset_materialization(
        replace(settings, plugin_job_timeout_seconds=300), lease
    )

    with Session(engine) as session:
        revision = session.get(DatasetRevision, dataset_id)
        assert revision is not None
        assert (revision.quality_state, revision.point_in_time_state, revision.promotability) == (
            "VALID",
            "VALID",
            "NON_PROMOTABLE",
        )
        quality = session.scalar(
            select(DataQualityResult).where(
                DataQualityResult.dataset_revision_id == dataset_id,
                DataQualityResult.check_kind == "QUALITY",
            )
        )
        assert quality is not None
        assert quality.summary["promotability_reason_codes"] == ["DATA_CLASS_NON_PROMOTABLE"]


def test_point_in_time_failure_persists_non_promotable_evidence(
    engine: Engine, settings, monkeypatch
) -> None:  # type: ignore[no-untyped-def]
    lease, dataset_id = _seed_pending_materialization(engine)
    monkeypatch.setattr(dataset_materialization, "create_database_engine", lambda _: engine)
    monkeypatch.setattr(engine, "dispose", lambda: None)
    monkeypatch.setattr(dataset_materialization, "_runtime", _PointInTimeInvalidRuntime)

    dataset_materialization.run_dataset_materialization(
        replace(settings, plugin_job_timeout_seconds=300), lease
    )

    with Session(engine) as session:
        revision = session.get(DatasetRevision, dataset_id)
        assert revision is not None
        assert (revision.quality_state, revision.point_in_time_state, revision.promotability) == (
            "VALID",
            "INVALID",
            "NON_PROMOTABLE",
        )
        result = session.scalar(
            select(DataQualityResult).where(
                DataQualityResult.dataset_revision_id == dataset_id,
                DataQualityResult.check_kind == "POINT_IN_TIME",
            )
        )
        assert result is not None
        assert result.summary["reason_codes"] == ["REMOTE_POINT_IN_TIME_INVALID"]


def test_materialization_rejects_descriptor_that_changes_frozen_request(
    engine: Engine, settings, monkeypatch
) -> None:  # type: ignore[no-untyped-def]
    lease, dataset_id = _seed_pending_materialization(engine)
    monkeypatch.setattr(dataset_materialization, "create_database_engine", lambda _: engine)
    monkeypatch.setattr(engine, "dispose", lambda: None)
    monkeypatch.setattr(dataset_materialization, "_runtime", _WrongDescriptorRuntime)

    with pytest.raises(QfError, match="DATASET_MATERIALIZATION_DESCRIPTOR_MISMATCH"):
        dataset_materialization.run_dataset_materialization(
            replace(settings, plugin_job_timeout_seconds=300), lease
        )

    with Session(engine) as session:
        revision = session.get(DatasetRevision, dataset_id)
        assert revision is not None
        assert (revision.quality_state, revision.point_in_time_state) == ("PENDING", "PENDING")
        assert session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.dataset_revision_id == dataset_id
            )
        ) is None


def test_missing_shard_is_recorded_as_non_promotable_quality_evidence(
    engine: Engine, settings, monkeypatch
) -> None:  # type: ignore[no-untyped-def]
    lease, dataset_id = _seed_pending_materialization(engine, missing_shard=True)
    monkeypatch.setattr(dataset_materialization, "create_database_engine", lambda _: engine)
    monkeypatch.setattr(engine, "dispose", lambda: None)
    monkeypatch.setattr(dataset_materialization, "_runtime", _Runtime)

    dataset_materialization.run_dataset_materialization(
        replace(settings, plugin_job_timeout_seconds=500), lease
    )

    with Session(engine) as session:
        revision = session.get(DatasetRevision, dataset_id)
        assert revision is not None
        assert (revision.quality_state, revision.point_in_time_state, revision.promotability) == (
            "INVALID",
            "VALID",
            "NON_PROMOTABLE",
        )
        result = session.scalar(
            select(DataQualityResult).where(
                DataQualityResult.dataset_revision_id == dataset_id,
                DataQualityResult.check_kind == "QUALITY",
            )
        )
        assert result is not None
        assert result.summary["reason_codes"] == ["DATA_SHARD_MISSING"]
