from __future__ import annotations

import io
import zipfile
from uuid import UUID

from fastapi.testclient import TestClient
from sqlalchemy import Engine, select
from sqlalchemy.orm import sessionmaker

from db.models import Job, PluginArtifact, PluginRelease
from main import create_app
from settings import Settings


def wheel_bytes(*, name: str, version: str, plugin_id: str | None) -> bytes:
    buffer = io.BytesIO()
    dist_info = f"{name.replace('-', '_')}-{version}.dist-info"
    with zipfile.ZipFile(buffer, "w") as archive:
        archive.writestr(
            f"{dist_info}/METADATA",
            f"Metadata-Version: 2.4\nName: {name}\nVersion: {version}\n",
        )
        if plugin_id:
            archive.writestr(
                f"{dist_info}/entry_points.txt",
                f"[quazonai.plugins]\n{plugin_id} = sample_plugin:plugin\n",
            )
    return buffer.getvalue()


def test_stage_release_streams_wheels_and_enqueues_install(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    response = client.post(
        "/api/v1/plugin-releases",
        files=[
            (
                "primary",
                (
                    "sample_plugin-1.0.0-py3-none-any.whl",
                    wheel_bytes(name="sample-plugin", version="1.0.0", plugin_id="sample"),
                    "application/octet-stream",
                ),
            ),
            (
                "dependencies",
                (
                    "sample_dep-2.0.0-py3-none-any.whl",
                    wheel_bytes(name="sample-dep", version="2.0.0", plugin_id=None),
                    "application/octet-stream",
                ),
            ),
        ],
    )

    assert response.status_code == 202, response.text
    body = response.json()
    assert body["release"]["plugin_id"] == "sample"
    assert body["release"]["state"] == "RECEIVED"
    assert body["job"]["kind"] == "PLUGIN_INSTALL"
    assert body["job"]["state"] == "READY"

    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory() as session:
        release = session.get(PluginRelease, UUID(body["release"]["id"]))
        job = session.get(Job, UUID(body["job"]["id"]))
        assert release is not None
        artifacts = list(
            session.scalars(
                select(PluginArtifact).where(
                    PluginArtifact.plugin_release_id == release.id
                )
            )
        )
        assert release.version == "1.0.0"
        assert job is not None and job.resource_id == release.id
        assert {artifact.role for artifact in artifacts} == {"PRIMARY", "DEPENDENCY"}
        for artifact in artifacts:
            assert (settings.plugin_root / artifact.relative_path).is_file()
