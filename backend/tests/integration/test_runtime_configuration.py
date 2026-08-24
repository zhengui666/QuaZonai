from __future__ import annotations

from dataclasses import replace
from pathlib import Path

from fastapi.testclient import TestClient
from sqlalchemy import Engine, select

from db.models import Event, RuntimeConfiguration
from db.session import create_session_factory
from main import create_app
from runtime_config import effective_settings
from runners.research_missions import CUSTOM_CODEX_PROVIDER_ID, _codex_launch_configuration
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
    assert defaults.json()["codex_api_key_configured"] is False
    assert defaults.json()["max_plugin_wheel_bytes"] == settings.max_plugin_wheel_bytes

    response = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(codex_api_key="sk-runtime-secret"),
    )
    assert response.status_code == 200, response.text
    body = response.json()
    assert body["codex_model"] == "gpt-5.6-sol"
    assert body["codex_base_url"] == "https://gateway.example.test/v1"
    assert body["codex_api_key_configured"] is True
    assert "api_key" not in " ".join(body.keys()).replace("codex_api_key_configured", "")

    factory = create_session_factory(engine)
    with factory() as session:
        stored = session.scalar(select(RuntimeConfiguration))
        assert stored is not None
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
    assert client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(codex_api_key="sk-to-clear"),
    ).status_code == 200

    response = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(clear_codex_api_key=True),
    )
    assert response.status_code == 200
    assert response.json()["codex_api_key_configured"] is False


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


def test_codex_launch_uses_custom_provider_without_secret_in_overrides(
    settings: Settings,
    tmp_path: Path,
) -> None:
    configured = replace(
        settings,
        codex_model="gpt-5.6-sol",
        codex_base_url="https://gateway.example.test/v1",
        codex_api_key="sk-provider-secret",
    )
    config, provider_id = _codex_launch_configuration(configured, tmp_path)

    assert provider_id == CUSTOM_CODEX_PROVIDER_ID
    assert config.env is not None
    assert config.env["OPENAI_API_KEY"] == ""
    assert config.env["CODEX_API_KEY"] == ""
    assert config.env["QUAZONAI_CODEX_API_KEY"] == "sk-provider-secret"
    joined = "\n".join(config.config_overrides)
    assert "https://gateway.example.test/v1" in joined
    assert "wire_api=\"responses\"" in joined
    assert "QUAZONAI_CODEX_API_KEY" in joined
    assert "sk-provider-secret" not in joined
