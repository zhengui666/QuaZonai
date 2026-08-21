from __future__ import annotations

from datetime import UTC, datetime, timedelta

from fastapi.testclient import TestClient
from sqlalchemy import Engine, select
from sqlalchemy.orm import sessionmaker

from quantfoundry.db.models import (
    CatalogDataset,
    DataSource,
    Job,
    PluginRelease,
    PluginRuntimeBundle,
    Strategy,
    StrategyVersion,
)
from quantfoundry.main import create_app
from quantfoundry.settings import Settings

SECTIONS = [
    "HYPOTHESIS",
    "MARKET_CONTEXT",
    "DATA",
    "METHOD",
    "RESULTS",
    "RISKS",
    "CONCLUSION",
]


def test_research_to_optimization_queue(
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
        strategy = Strategy(name="sample")
        bundle = PluginRuntimeBundle(
            state="READY",
            python_version="3.14.0",
            qf_version="0.1.0",
            environment_path="bundles/test",
        )
        session.add_all([release, strategy, bundle])
        session.flush()
        version = StrategyVersion(
            strategy_id=strategy.id,
            version_no=1,
            source_text="pass\n",
            default_config={},
            objective_directions=["maximize", "minimize"],
        )
        source = DataSource(
            plugin_release_id=release.id,
            name="history",
            config={},
            state="ACTIVE",
        )
        session.add_all([version, source])
        session.flush()
        dataset = CatalogDataset(
            data_source_id=source.id,
            instrument_id="TOKEN.POLYMARKET",
            catalog_path="datasets/test",
            metadata={"data_cls": "nautilus_trader.model.data:OrderBookDeltas"},
            state="READY",
        )
        session.add(dataset)

    client = TestClient(create_app(settings=settings, engine=engine))
    response = client.post(
        "/api/v1/research-cases",
        json={"title": "Research", "strategy_version_id": str(version.id)},
    )
    assert response.status_code == 201, response.text
    research_id = response.json()["id"]

    for section in SECTIONS:
        response = client.post(
            f"/api/v1/research-cases/{research_id}/sections",
            json={"section": section, "markdown": f"{section} content"},
        )
        assert response.status_code == 201, response.text

    response = client.post(f"/api/v1/research-cases/{research_id}/activate")
    assert response.status_code == 200, response.text
    assert response.json()["state"] == "ACTIVE"

    start = datetime(2026, 1, 1, tzinfo=UTC)
    response = client.post(
        f"/api/v1/research-cases/{research_id}/experiments",
        json={
            "dataset_id": str(dataset.id),
            "runtime_bundle_id": str(bundle.id),
            "train_start": start.isoformat(),
            "train_end": (start + timedelta(days=1)).isoformat(),
            "holdout_start": (start + timedelta(days=2)).isoformat(),
            "holdout_end": (start + timedelta(days=3)).isoformat(),
            "seed": 7,
        },
    )
    assert response.status_code == 201, response.text
    experiment_id = response.json()["id"]

    response = client.post(f"/api/v1/experiments/{experiment_id}/start")
    assert response.status_code == 202, response.text
    assert response.json()["type"] == "OPTIMIZATION"
    assert response.json()["state"] == "QUEUED"

    with factory() as session:
        job = session.scalar(select(Job).where(Job.kind == "OPTIMIZATION"))
        assert job is not None
        assert str(job.resource_id) == response.json()["id"]
