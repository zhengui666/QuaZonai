from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import Engine, select
from sqlalchemy.orm import sessionmaker

from db.models import (
    DataSource,
    Job,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
)
from main import create_app
from settings import Settings


def test_parquet_upload_creates_dataset_and_import_job(
    engine: Engine,
    settings: Settings,
) -> None:
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        release = PluginRelease(
            plugin_id="parquet_l2",
            distribution_name="parquet-l2",
            version="1.0.0",
            api_version="1",
            state="ACTIVE",
            is_default=True,
            descriptor_snapshot={
                "capabilities": ["HISTORICAL_IMPORT"],
                "compatibility_key": "polymarket-v2",
            },
        )
        session.add(release)
        session.flush()
        source = DataSource(
            plugin_release_id=release.id,
            name="history",
            config={},
            state="ACTIVE",
        )
        bundle = PluginRuntimeBundle(
            state="READY",
            python_version="3.14.0",
            qf_version="0.1.0",
            environment_path="bundles/test",
        )
        session.add_all([source, bundle])
        session.flush()
        session.add(
            PluginRuntimeBundleMember(
                runtime_bundle_id=bundle.id,
                plugin_release_id=release.id,
                member_role="IMPORTER",
            )
        )

    client = TestClient(create_app(settings=settings, engine=engine))
    response = client.post(
        f"/api/v1/data-sources/{source.id}/imports/parquet-l2",
        files={"file": ("events.parquet", b"not-yet-validated", "application/octet-stream")},
        data={
            "instrument_id": "TOKEN.POLYMARKET",
            "source_label": "fixture",
            "metadata_json": "{}",
        },
    )

    assert response.status_code == 202, response.text
    body = response.json()
    assert body["dataset"]["state"] == "IMPORTING"
    run_id = body["run_id"]
    assert (settings.import_root / run_id / "upload.parquet").read_bytes() == b"not-yet-validated"

    with factory() as session:
        job = session.scalar(select(Job).where(Job.kind == "PARQUET_IMPORT"))
        assert job is not None
        assert str(job.resource_id) == run_id
