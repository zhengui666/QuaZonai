from __future__ import annotations

from dataclasses import replace
from pathlib import Path

from fastapi.testclient import TestClient
from sqlalchemy import Engine, func, select

from db.models import Event, PublicMutationReceipt, RuntimeConfiguration
from db.session import create_session_factory
from main import create_app
from runtime_config import effective_settings
from runners.codex_provider_auth import fetch_token
from runners.research_missions import (
    CUSTOM_CODEX_PROVIDER_ID,
    _codex_launch_configuration,
    _provider_credential_broker,
)
from settings import (
    DEFAULT_BUNDLE_BUILD_TIMEOUT_SECONDS,
    DEFAULT_JOB_LEASE_SECONDS,
    DEFAULT_JOB_POLL_SECONDS,
    DEFAULT_MAX_PLUGIN_WHEEL_BYTES,
    DEFAULT_MISSION_JOB_TIMEOUT_SECONDS,
    DEFAULT_PLUGIN_JOB_TIMEOUT_SECONDS,
    DEFAULT_PLUGIN_VALIDATION_TIMEOUT_SECONDS,
    Settings,
)


def _payload(**overrides: object) -> dict[str, object]:
    payload: dict[str, object] = {
        "expected_revision": 0,
        "codex_model": "gpt-5.6-sol",
        "codex_base_url": "https://gateway.example.test/v1",
        "clear_codex_api_key": False,
        "max_plugin_wheel_bytes": 134_217_728,
        "plugin_validation_timeout_seconds": 120,
        "bundle_build_timeout_seconds": 480,
        "plugin_job_timeout_seconds": 720,
        "mission_job_timeout_seconds": 1440,
        "job_poll_seconds": 0.5,
        "job_lease_seconds": 45,
    }
    payload.update(overrides)
    return payload


def test_runtime_configuration_round_trip_encrypts_codex_key(
    engine: Engine,
    settings: Settings,
) -> None:
    app = create_app(settings=settings, engine=engine)
    client = TestClient(app)

    defaults = client.get("/api/v1/system/runtime-configuration")
    assert defaults.status_code == 200
    assert defaults.json()["revision"] == 0
    assert defaults.json()["codex_api_key_configured"] is False
    assert defaults.json()["max_plugin_wheel_bytes"] == settings.max_plugin_wheel_bytes

    response = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(codex_api_key="sk-runtime-secret"),
    )
    assert response.status_code == 200, response.text
    body = response.json()
    assert body["revision"] == 1
    assert body["codex_model"] == "gpt-5.6-sol"
    assert body["codex_base_url"] == "https://gateway.example.test/v1"
    assert body["codex_api_key_configured"] is True
    assert "api_key" not in " ".join(body.keys()).replace("codex_api_key_configured", "")

    factory = create_session_factory(engine)
    with factory() as session:
        stored = session.scalar(select(RuntimeConfiguration))
        assert stored is not None
        assert stored.revision == 1
        assert stored.codex_api_key_ciphertext is not None
        assert b"sk-runtime-secret" not in stored.codex_api_key_ciphertext
        runtime = effective_settings(session, settings)
        assert runtime.codex_model == "gpt-5.6-sol"
        assert runtime.codex_base_url == "https://gateway.example.test/v1"
        assert runtime.codex_api_key == "sk-runtime-secret"
        assert runtime.job_poll_seconds == 0.5
        event = session.scalar(
            select(Event).where(Event.kind == "RUNTIME_CONFIGURATION_UPDATED")
        )
        assert event is not None
        assert "sk-runtime-secret" not in str(event.payload)


def test_runtime_configuration_can_clear_codex_key(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    created = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(codex_api_key="sk-to-clear"),
    )
    assert created.status_code == 200

    response = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(expected_revision=created.json()["revision"], clear_codex_api_key=True),
    )
    assert response.status_code == 200
    assert response.json()["revision"] == 2
    assert response.json()["codex_api_key_configured"] is False


def test_runtime_configuration_requires_key_reentry_when_base_url_changes(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    created = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(codex_api_key="sk-original-provider"),
    )
    assert created.status_code == 200
    revision = created.json()["revision"]

    rejected = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(
            expected_revision=revision,
            codex_base_url="https://attacker.example.test/v1",
        ),
    )
    assert rejected.status_code == 409
    assert rejected.json()["error"]["code"] == "CODEX_PROVIDER_CREDENTIAL_REENTRY_REQUIRED"
    current = client.get("/api/v1/system/runtime-configuration")
    assert current.status_code == 200
    assert current.json()["revision"] == revision
    assert current.json()["codex_base_url"] == "https://gateway.example.test/v1"
    assert current.json()["codex_api_key_configured"] is True

    replaced = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(
            expected_revision=revision,
            codex_base_url="https://replacement.example.test/v1",
            codex_api_key="sk-replacement-provider",
        ),
    )
    assert replaced.status_code == 200, replaced.text
    assert replaced.json()["revision"] == revision + 1
    assert replaced.json()["codex_base_url"] == "https://replacement.example.test/v1"
    assert replaced.json()["codex_api_key_configured"] is True


def test_runtime_configuration_rejects_stale_revision(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    assert client.put("/api/v1/system/runtime-configuration", json=_payload()).status_code == 200

    stale = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(codex_model="stale-overwrite"),
    )
    assert stale.status_code == 409
    assert stale.json()["error"]["code"] == "RUNTIME_CONFIGURATION_STALE"


def test_runtime_configuration_deduplicates_idempotent_retry_without_storing_secret(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    headers = {"Idempotency-Key": "runtime-config-retry"}
    payload = _payload(codex_api_key="sk-idempotent-secret")

    first = client.put("/api/v1/system/runtime-configuration", json=payload, headers=headers)
    second = client.put("/api/v1/system/runtime-configuration", json=payload, headers=headers)
    assert first.status_code == 200, first.text
    assert second.status_code == 200, second.text
    assert first.json() == second.json()

    factory = create_session_factory(engine)
    with factory() as session:
        event_count = session.scalar(
            select(func.count()).select_from(Event).where(
                Event.kind == "RUNTIME_CONFIGURATION_UPDATED"
            )
        )
        assert event_count == 1
        receipt = session.get(PublicMutationReceipt, "runtime-config-retry")
        assert receipt is not None
        assert "sk-idempotent-secret" not in str(receipt.normalized_request)
        assert "codex_api_key" not in receipt.normalized_request
        assert receipt.normalized_request["codex_api_key_action"] == "set"


def test_runtime_configuration_rejects_poll_interval_below_floor(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    response = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(job_poll_seconds=0.001),
    )
    assert response.status_code == 422


def test_runtime_configuration_rejects_credential_bearing_base_url(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    response = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(codex_base_url="https://token@example.test/v1"),
    )
    assert response.status_code == 422


def test_runtime_tuning_is_not_loaded_from_environment(monkeypatch: object) -> None:
    monkeypatch.setenv("QUAZONAI_CODEX_MODEL", "legacy-env-model")  # type: ignore[attr-defined]
    monkeypatch.setenv("OPENAI_API_KEY", "legacy-env-key")  # type: ignore[attr-defined]
    monkeypatch.setenv("QUAZONAI_MAX_PLUGIN_WHEEL_BYTES", "1")  # type: ignore[attr-defined]
    monkeypatch.setenv("QUAZONAI_PLUGIN_VALIDATION_TIMEOUT_SECONDS", "1")  # type: ignore[attr-defined]
    monkeypatch.setenv("QUAZONAI_BUNDLE_BUILD_TIMEOUT_SECONDS", "1")  # type: ignore[attr-defined]
    monkeypatch.setenv("QUAZONAI_PLUGIN_JOB_TIMEOUT_SECONDS", "1")  # type: ignore[attr-defined]
    monkeypatch.setenv("QUAZONAI_MISSION_JOB_TIMEOUT_SECONDS", "1")  # type: ignore[attr-defined]
    monkeypatch.setenv("QUAZONAI_JOB_POLL_SECONDS", "9")  # type: ignore[attr-defined]
    monkeypatch.setenv("QUAZONAI_JOB_LEASE_SECONDS", "1")  # type: ignore[attr-defined]

    loaded = Settings.from_env()
    assert loaded.codex_model is None
    assert loaded.codex_api_key is None
    assert loaded.max_plugin_wheel_bytes == DEFAULT_MAX_PLUGIN_WHEEL_BYTES
    assert loaded.plugin_validation_timeout_seconds == DEFAULT_PLUGIN_VALIDATION_TIMEOUT_SECONDS
    assert loaded.bundle_build_timeout_seconds == DEFAULT_BUNDLE_BUILD_TIMEOUT_SECONDS
    assert loaded.plugin_job_timeout_seconds == DEFAULT_PLUGIN_JOB_TIMEOUT_SECONDS
    assert loaded.mission_job_timeout_seconds == DEFAULT_MISSION_JOB_TIMEOUT_SECONDS
    assert loaded.job_poll_seconds == DEFAULT_JOB_POLL_SECONDS
    assert loaded.job_lease_seconds == DEFAULT_JOB_LEASE_SECONDS


def test_codex_launch_uses_command_auth_without_secret_in_process_configuration(
    settings: Settings,
    tmp_path: Path,
) -> None:
    configured = replace(
        settings,
        codex_model="gpt-5.6-sol",
        codex_base_url="https://gateway.example.test/v1",
        codex_api_key="sk-provider-secret",
    )
    socket_path = tmp_path / "provider-token.sock"
    config, provider_id = _codex_launch_configuration(
        configured,
        tmp_path,
        credential_socket=socket_path,
    )

    assert provider_id == CUSTOM_CODEX_PROVIDER_ID
    assert config.env is not None
    assert config.env["OPENAI_API_KEY"] == ""
    assert config.env["CODEX_API_KEY"] == ""
    assert config.env["QUAZONAI_CODEX_API_KEY"] == ""
    assert config.env["QUAZONAI_MASTER_KEY"] == ""
    joined = "\n".join(config.config_overrides)
    assert "https://gateway.example.test/v1" in joined
    assert "wire_api=\"responses\"" in joined
    assert "runners.codex_provider_auth" in joined
    assert str(socket_path) in joined
    assert "refresh_interval_ms = 0" in joined
    assert "env_key" not in joined
    assert "sk-provider-secret" not in joined


def test_provider_credential_broker_is_one_shot() -> None:
    with _provider_credential_broker("sk-one-shot") as socket_path:
        assert socket_path is not None
        assert fetch_token(socket_path) == "sk-one-shot"
    assert not socket_path.exists()
