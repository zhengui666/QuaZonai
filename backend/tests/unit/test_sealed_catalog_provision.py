from __future__ import annotations

from datetime import UTC, datetime, timedelta
from traceback import format_exception
from uuid import UUID, uuid4

import pytest
from sqlalchemy import Engine, select
from sqlalchemy.orm import Session, sessionmaker

from db.models import (
    DataQualityResult,
    DatasetRevision,
    Event,
    GovernedDataSource,
    Job,
    MarketUniverseVersion,
    NautilusCatalogBinding,
)
from errors import QfError
from jobs import JobLease, claim_next_job, enqueue_job
from quant_runtime.config import RemoteNautilusConfig
from quant_runtime.contracts import CatalogDescriptor
from runners import finite_worker, sealed_catalog_provision


def _now() -> datetime:
    return datetime(2026, 1, 1, tzinfo=UTC)


def _seed_pending_provision(
    engine: Engine,
    *,
    claim: bool = True,
) -> tuple[JobLease | None, UUID]:
    now = _now()
    catalog_uri = f"catalog://sealed-{uuid4().hex}"
    with Session(engine) as session, session.begin():
        universe = MarketUniverseVersion(
            universe_key=f"TEST-{uuid4().hex[:12]}",
            version_no=1,
            name="Test Universe",
            state="ACTIVE",
            spec_json={},
            created_at=now,
        )
        source = GovernedDataSource(
            name=f"sealed-source-{uuid4()}",
            connector_key="sealed-catalog",
            provider="Test Provider",
            state="ACTIVE",
            universe_scope=[],
            fields=[],
            license_classification="TEST",
            preflight_state="READY",
            # The sealed runner must not load this plugin-shaped source configuration.
            public_config={"plugin_binding": {"must_not": "be_imported"}},
        )
        session.add_all((universe, source))
        session.flush()
        source.universe_scope = [str(universe.id)]
        revision = DatasetRevision(
            data_source_id=source.id,
            universe_version_id=universe.id,
            universe_name=universe.name,
            revision_no=1,
            data_class="VENDOR",
            origin="trusted-sealed-runtime",
            ingested_at=now,
            promotability="NON_PROMOTABLE",
            schema_version="sealed-v1",
            event_start=now,
            event_end=now + timedelta(hours=1),
            available_start=now,
            available_end=now + timedelta(hours=1),
            quality_state="PENDING",
            point_in_time_state="PENDING",
            partition="SEALED",
            materialization_request={
                "sealed_catalog_uri": catalog_uri,
                "instrument_scope": ["TEST.INSTRUMENT"],
                "data_type": "QuoteTick",
            },
            created_at=now,
        )
        session.add(revision)
        session.flush()
        enqueue_job(
            session,
            kind="SEALED_CATALOG_PROVISION",
            resource_type="dataset_revision",
            resource_id=revision.id,
        )
        dataset_id = revision.id
    if claim:
        factory = sessionmaker(bind=engine, expire_on_commit=False)
        with factory.begin() as session:
            claimed = claim_next_job(session, owner="worker", lease_seconds=60)
            assert claimed is not None and claimed.lease_owner is not None
            return JobLease(claimed.id, claimed.lease_owner, claimed.attempt), dataset_id
    return None, dataset_id


class _Runtime:
    def validate_catalog(self, catalog_uri: str) -> CatalogDescriptor:
        now = _now()
        return CatalogDescriptor(
            catalog_uri=catalog_uri,
            provider="Test Provider",
            source_license="TEST",
            source_spec={"raw_locator": "https://sealed.example.invalid/raw.parquet"},
            nautilus_data_type="QuoteTick",
            instrument_scope=["TEST.INSTRUMENT"],
            event_start=now,
            event_end=now + timedelta(hours=1),
            available_start=now,
            available_end=now + timedelta(hours=1),
            row_count=10,
            schema_revision="sealed-v1",
            quality_result={"valid": True, "raw_locator": "sealed-result-sentinel"},
            point_in_time_result={"valid": True, "raw_locator": "sealed-result-sentinel"},
            sealed=True,
        )


class _UnsealedRuntime(_Runtime):
    def validate_catalog(self, catalog_uri: str) -> CatalogDescriptor:
        return super().validate_catalog(catalog_uri).model_copy(update={"sealed": False})


class _LicenseMismatchRuntime(_Runtime):
    def validate_catalog(self, catalog_uri: str) -> CatalogDescriptor:
        return super().validate_catalog(catalog_uri).model_copy(update={"source_license": "OTHER"})


class _UnavailableRuntime:
    def validate_catalog(self, _: str) -> CatalogDescriptor:
        raise QfError(
            "NAUTILUS_RUNTIME_UNAVAILABLE",
            "The remote NautilusTrader runtime could not be reached.",
            503,
        )


class _MalformedRuntime:
    def validate_catalog(self, catalog_uri: str) -> CatalogDescriptor:
        return CatalogDescriptor.model_validate(
            {
                "catalog_uri": catalog_uri,
                "provider": "Test Provider",
                "source_license": "TEST",
                "nautilus_data_type": "QuoteTick",
                "instrument_scope": ["TEST.INSTRUMENT"],
                "event_start": "sealed-descriptor-sentinel",
                "event_end": _now() + timedelta(hours=1),
                "available_start": _now(),
                "available_end": _now() + timedelta(hours=1),
                "row_count": 10,
                "schema_revision": "sealed-v1",
                "quality_result": {"valid": True},
                "point_in_time_result": {"valid": True},
                "sealed": True,
            }
        )


def _patch_engine(monkeypatch: pytest.MonkeyPatch, engine: Engine) -> None:
    monkeypatch.setattr(sealed_catalog_provision, "create_database_engine", lambda _: engine)
    monkeypatch.setattr(engine, "dispose", lambda: None)


def test_sealed_runtime_binds_only_validated_descriptor_evidence(
    engine: Engine, settings, monkeypatch: pytest.MonkeyPatch
) -> None:  # type: ignore[no-untyped-def]
    lease, dataset_id = _seed_pending_provision(engine)
    assert lease is not None
    _patch_engine(monkeypatch, engine)
    monkeypatch.setattr(sealed_catalog_provision, "_runtime", _Runtime)

    sealed_catalog_provision.run_sealed_catalog_provision(settings, lease)

    with Session(engine) as session:
        revision = session.get(DatasetRevision, dataset_id)
        job = session.get(Job, lease.job_id)
        binding = session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.dataset_revision_id == dataset_id
            )
        )
        event = session.scalar(select(Event).where(Event.kind == "SEALED_CATALOG_PROVISIONED"))
        assert revision is not None
        assert job is not None
        assert binding is not None
        assert event is not None
        assert (revision.quality_state, revision.point_in_time_state, revision.promotability) == (
            "VALID",
            "VALID",
            "PROMOTABLE",
        )
        assert binding.sealed is True
        assert binding.quality_result == {"valid": True}
        assert binding.point_in_time_result == {"valid": True}
        assert job.payload == {}
        assert event.payload == {
            "catalog_id": str(binding.id),
            "quality_state": "VALID",
            "point_in_time_state": "VALID",
            "promotability": "PROMOTABLE",
        }
        results = list(
            session.scalars(
                select(DataQualityResult)
                .where(DataQualityResult.dataset_revision_id == dataset_id)
                .order_by(DataQualityResult.check_kind)
            )
        )
        assert [item.state for item in results] == ["VALID", "VALID"]
        assert all(item.summary["remote_result"] == {"valid": True} for item in results)
        assert "sealed-result-sentinel" not in str([item.summary for item in results])
        assert "sealed-result-sentinel" not in str(event.payload)
        assert binding.catalog_uri not in str(event.payload)


@pytest.mark.parametrize("runtime", [_UnsealedRuntime, _LicenseMismatchRuntime])
def test_descriptor_identity_mismatch_is_terminal_non_promotable_evidence(
    engine: Engine, settings, monkeypatch: pytest.MonkeyPatch, runtime
) -> None:  # type: ignore[no-untyped-def]
    lease, dataset_id = _seed_pending_provision(engine)
    assert lease is not None
    _patch_engine(monkeypatch, engine)
    monkeypatch.setattr(sealed_catalog_provision, "_runtime", runtime)

    with pytest.raises(QfError, match="SEALED_CATALOG_DESCRIPTOR_MISMATCH"):
        sealed_catalog_provision.run_sealed_catalog_provision(settings, lease)

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


def test_unavailable_sealed_runtime_leaves_revision_pending(
    engine: Engine, settings, monkeypatch: pytest.MonkeyPatch
) -> None:  # type: ignore[no-untyped-def]
    lease, dataset_id = _seed_pending_provision(engine)
    assert lease is not None
    _patch_engine(monkeypatch, engine)
    monkeypatch.setattr(sealed_catalog_provision, "_runtime", _UnavailableRuntime)

    with pytest.raises(QfError, match="NAUTILUS_RUNTIME_UNAVAILABLE"):
        sealed_catalog_provision.run_sealed_catalog_provision(settings, lease)

    with Session(engine) as session:
        revision = session.get(DatasetRevision, dataset_id)
        assert revision is not None
        assert (revision.quality_state, revision.point_in_time_state, revision.promotability) == (
            "PENDING",
            "PENDING",
            "NON_PROMOTABLE",
        )
        assert session.scalar(
            select(NautilusCatalogBinding).where(
                NautilusCatalogBinding.dataset_revision_id == dataset_id
            )
        ) is None


def test_invalid_descriptor_cannot_leak_into_job_error_or_events(
    engine: Engine, settings, monkeypatch: pytest.MonkeyPatch
) -> None:  # type: ignore[no-untyped-def]
    trace_lease, _ = _seed_pending_provision(engine)
    assert trace_lease is not None
    _patch_engine(monkeypatch, engine)
    monkeypatch.setattr(sealed_catalog_provision, "_runtime", _MalformedRuntime)

    with pytest.raises(QfError) as caught:
        sealed_catalog_provision.run_sealed_catalog_provision(settings, trace_lease)
    assert "sealed-descriptor-sentinel" not in "".join(format_exception(caught.value))

    unclaimed_lease, dataset_id = _seed_pending_provision(engine, claim=False)
    assert unclaimed_lease is None
    with Session(engine) as session:
        job_id = session.scalar(select(Job.id).where(Job.resource_id == dataset_id))
        assert job_id is not None
    monkeypatch.setitem(
        finite_worker.HANDLERS,
        "SEALED_CATALOG_PROVISION",
        lambda current_settings, job, _lease_lost=None: sealed_catalog_provision.run_sealed_catalog_provision(
            current_settings,
            JobLease(job.id, job.lease_owner or "", job.attempt),
        ),
    )

    worked, _ = finite_worker.run_once(
        settings,
        owner="worker",
        factory=sessionmaker(bind=engine, expire_on_commit=False),
    )

    assert worked is True
    with Session(engine) as session:
        job = session.get(Job, job_id)
        revision = session.get(DatasetRevision, dataset_id)
        assert job is not None
        assert revision is not None
        assert job.state == "FAILED"
        assert job.last_error is not None
        assert "sealed-descriptor-sentinel" not in job.last_error
        assert (revision.quality_state, revision.point_in_time_state) == ("INVALID", "INVALID")
        assert all(
            "sealed-descriptor-sentinel" not in str(event.payload)
            for event in session.scalars(select(Event))
        )


def test_runtime_uses_the_independent_sealed_profile(monkeypatch: pytest.MonkeyPatch) -> None:
    expected = RemoteNautilusConfig(base_url="https://sealed.example.invalid", service_token=None)
    observed: dict[str, object] = {}

    def from_env(*, required: bool = False, profile: str = "research") -> RemoteNautilusConfig:
        observed.update(required=required, profile=profile)
        return expected

    monkeypatch.setattr(sealed_catalog_provision.RemoteNautilusConfig, "from_env", from_env)

    assert sealed_catalog_provision._runtime().config is expected
    assert observed == {"required": True, "profile": "sealed"}
