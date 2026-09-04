from __future__ import annotations

from uuid import UUID

from fastapi.testclient import TestClient
from sqlalchemy import Engine, select
from sqlalchemy.orm import Session

from db.models import DatasetRevision, Event, GovernedDataSource, Job
from main import create_app
from settings import Settings


def _materialization_payload(source_id: str, universe_id: str) -> dict[str, object]:
    return {
        "data_source_id": source_id,
        "universe_version_id": universe_id,
        "partition": "SEALED",
        "data_class": "VENDOR",
        "origin": "trusted-sealed-runtime",
        "schema_version": "sealed-v1",
        "data_type": "QuoteTick",
        "instrument_scope": ["TEST.INSTRUMENT"],
        "event_start": "2026-01-01T00:00:00Z",
        "event_end": "2026-01-01T01:00:00Z",
        "available_start": "2026-01-01T00:00:00Z",
        "available_end": "2026-01-01T01:00:00Z",
        "quality_requirements": {"coverage": "required"},
        "point_in_time_requirements": {"available_at": "required"},
        "sealed_catalog_uri": "catalog://sealed-integration",
    }


def test_sealed_materialization_uses_an_empty_trusted_provision_job(
    engine: Engine, settings: Settings
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    universe = client.post(
        "/api/v1/universes",
        json={
            "universe_key": "SEALED_TEST",
            "name": "Sealed Test Universe",
            "instrument_schema": {"instrument_id": "string"},
            "membership_rules": {"market": "test"},
            "calendar_semantics": {"timezone": "UTC"},
            "currency_semantics": {"base_currency": "USD"},
            "data_requirements": {"available_at": "required"},
            "risk_model_family": "EWMA",
            "cost_model_family": "SPREAD",
            "capacity_model_family": "ADV",
        },
    )
    assert universe.status_code == 201, universe.text
    universe_id = universe.json()["id"]
    source = client.post(
        "/api/v1/data-sources",
        json={
            "name": "Sealed catalog source",
            "connector_key": "sealed-catalog",
            "provider": "Test Provider",
            "universe_scope": [universe_id],
            "field_schema": {"event_time": "timestamp"},
            "license_classification": "TEST",
            "availability_semantics": {"available_at_field": "available_time"},
            "public_config": {},
        },
    )
    assert source.status_code == 201, source.text
    source_id = source.json()["id"]
    with Session(engine) as session, session.begin():
        item = session.get(GovernedDataSource, UUID(source_id))
        assert item is not None
        item.preflight_state = "READY"

    payload = _materialization_payload(source_id, universe_id)
    queued = client.post(
        "/api/v1/datasets/materializations",
        headers={"Idempotency-Key": "sealed-materialization-1"},
        json=payload,
    )
    assert queued.status_code == 202, queued.text
    operation = queued.json()
    assert operation["kind"] == "SEALED_CATALOG_PROVISION"
    assert operation["resource_type"] == "dataset_revision"
    assert operation["state"] == "READY"
    fetched = client.get(f"/api/v1/operations/{operation['id']}")
    assert fetched.status_code == 200, fetched.text
    assert fetched.json() == operation
    replay = client.post(
        "/api/v1/datasets/materializations",
        headers={"Idempotency-Key": "sealed-materialization-1"},
        json=payload,
    )
    assert replay.status_code == 202, replay.text
    assert replay.json() == operation

    with Session(engine) as session:
        job = session.get(Job, UUID(operation["id"]))
        assert job is not None
        revision = session.get(DatasetRevision, job.resource_id)
        event = session.scalar(
            select(Event).where(Event.kind == "SEALED_CATALOG_PROVISION_REQUESTED")
        )
        assert revision is not None
        assert event is not None
        assert job.payload == {}
        assert revision.materialization_request["sealed_catalog_uri"] == "catalog://sealed-integration"
        assert event.payload == {"job_id": operation["id"], "revision_no": 1}
        assert "catalog://sealed-integration" not in str(job.payload)
        assert "catalog://sealed-integration" not in str(event.payload)

    missing_uri = dict(payload)
    missing_uri.pop("sealed_catalog_uri")
    assert client.post("/api/v1/datasets/materializations", json=missing_uri).status_code == 422
    nonsealed_uri = {**payload, "partition": "DISCOVERY"}
    assert client.post("/api/v1/datasets/materializations", json=nonsealed_uri).status_code == 422
    raw_url = {**payload, "sealed_catalog_uri": "https://sealed.example.invalid/raw.parquet"}
    assert client.post("/api/v1/datasets/materializations", json=raw_url).status_code == 422
    extra_plugin_input = {**payload, "source_spec": {"kind": "plugin"}}
    assert client.post("/api/v1/datasets/materializations", json=extra_plugin_input).status_code == 422
