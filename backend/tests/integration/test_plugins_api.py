from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import Engine
from sqlalchemy.orm import sessionmaker

from quantfoundry.db.models import PluginRelease
from quantfoundry.main import create_app
from quantfoundry.settings import Settings


def test_activate_release_drains_previous_default(
    engine: Engine,
    settings: Settings,
) -> None:
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        previous = PluginRelease(
            plugin_id="parquet_l2",
            distribution_name="qf-parquet-l2",
            version="1.0.0",
            api_version="1",
            state="ACTIVE",
            is_default=True,
            descriptor_snapshot={"capabilities": ["HISTORICAL_IMPORT"]},
        )
        replacement = PluginRelease(
            plugin_id="parquet_l2",
            distribution_name="qf-parquet-l2",
            version="1.1.0",
            api_version="1",
            state="STAGED",
            is_default=False,
            descriptor_snapshot={"capabilities": ["HISTORICAL_IMPORT"]},
        )
        session.add_all([previous, replacement])

    client = TestClient(create_app(settings=settings, engine=engine))
    response = client.post(f"/api/v1/plugin-releases/{replacement.id}/activate")
    assert response.status_code == 200
    assert response.json()["state"] == "ACTIVE"

    with factory() as session:
        old = session.get(PluginRelease, previous.id)
        new = session.get(PluginRelease, replacement.id)
        assert old is not None and old.state == "DRAINING" and not old.is_default
        assert new is not None and new.state == "ACTIVE" and new.is_default
