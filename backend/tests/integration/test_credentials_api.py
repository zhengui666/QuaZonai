from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import Engine, select
from sqlalchemy.orm import sessionmaker

from api.credentials import decrypt_credential_secrets
from db.models import CredentialSet, PluginRelease
from main import create_app
from settings import Settings


def test_credential_api_is_write_only_and_encrypted(
    engine: Engine,
    settings: Settings,
) -> None:
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        release = PluginRelease(
            plugin_id="sample_exec",
            distribution_name="sample-exec",
            version="1.0.0",
            api_version="1",
            state="ACTIVE",
            is_default=True,
            descriptor_snapshot={
                "capabilities": ["EXECUTION"],
                "public_config_schema": {
                    "type": "object",
                    "properties": {"network": {"type": "string"}},
                    "required": ["network"],
                    "additionalProperties": False,
                },
                "secret_config_schema": {
                    "type": "object",
                    "properties": {"api_secret": {"type": "string", "minLength": 1}},
                    "required": ["api_secret"],
                    "additionalProperties": False,
                },
                "required_secret_names": ["api_secret"],
            },
        )
        session.add(release)

    client = TestClient(create_app(settings=settings, engine=engine))
    response = client.post(
        "/api/v1/credential-sets",
        json={
            "plugin_release_id": str(release.id),
            "name": "primary",
            "public_config": {"network": "production"},
            "secrets": {"api_secret": "value-not-returned"},
        },
    )

    assert response.status_code == 201, response.text
    body = response.json()
    assert "secrets" not in body
    assert body["configured_secrets"] == {"api_secret": True}

    with factory() as session:
        credential = session.scalar(select(CredentialSet))
        assert credential is not None
        assert decrypt_credential_secrets(session, settings, credential) == {
            "api_secret": "value-not-returned"
        }
