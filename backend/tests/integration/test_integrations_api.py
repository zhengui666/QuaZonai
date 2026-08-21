from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import Engine
from sqlalchemy.orm import sessionmaker

from quantfoundry.db.models import PluginRelease
from quantfoundry.main import create_app
from quantfoundry.settings import Settings


def test_data_source_requires_active_matching_capability(
    engine: Engine,
    settings: Settings,
) -> None:
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        release = PluginRelease(
            plugin_id="sample_data",
            distribution_name="sample-data",
            version="1.0.0",
            api_version="1",
            state="ACTIVE",
            is_default=True,
            descriptor_snapshot={
                "capabilities": ["HISTORICAL_IMPORT"],
                "public_config_schema": {
                    "type": "object",
                    "properties": {"dataset": {"type": "string"}},
                    "required": ["dataset"],
                    "additionalProperties": False,
                },
            },
        )
        session.add(release)

    client = TestClient(create_app(settings=settings, engine=engine))
    response = client.post(
        "/api/v1/data-sources",
        json={
            "plugin_release_id": str(release.id),
            "name": "history",
            "config": {"dataset": "l2"},
        },
    )

    assert response.status_code == 201, response.text
    assert response.json()["plugin_release_id"] == str(release.id)

    invalid = client.post(
        "/api/v1/data-sources",
        json={
            "plugin_release_id": str(release.id),
            "name": "bad",
            "config": {"unexpected": True},
        },
    )
    assert invalid.status_code == 422
    assert invalid.json()["error"]["code"] == "PLUGIN_CONFIG_INVALID"
