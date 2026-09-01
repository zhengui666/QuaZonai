from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import Engine, select

from api.system import RuntimeConfigurationInput, _idempotency_shape
from db.models import Event, RuntimeConfiguration
from db.session import create_session_factory
from main import create_app
from runtime_config import effective_settings
from settings import Settings


def _payload(**overrides: object) -> dict[str, object]:
    payload: dict[str, object] = {
        "expected_revision": 0,
        "codex_model": "gpt-5.6-sol",
        "codex_reasoning_effort": "high",
        "codex_fast_mode": True,
        "codex_use_default_model_settings": False,
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


def test_codex_default_mode_masks_and_restores_retained_model_controls(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))

    defaults = client.get("/api/v1/system/runtime-configuration")
    assert defaults.status_code == 200
    assert defaults.json()["codex_use_default_model_settings"] is True

    configured = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(),
    )
    assert configured.status_code == 200, configured.text
    assert configured.json()["codex_use_default_model_settings"] is False

    factory = create_session_factory(engine)
    with factory() as session:
        runtime = effective_settings(session, settings)
        assert runtime.codex_model == "gpt-5.6-sol"
        assert runtime.codex_reasoning_effort == "high"
        assert runtime.codex_fast_mode is True
        assert runtime.codex_base_url == "https://gateway.example.test/v1"

    inherited = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(
            expected_revision=configured.json()["revision"],
            codex_use_default_model_settings=True,
        ),
    )
    assert inherited.status_code == 200, inherited.text
    inherited_body = inherited.json()
    assert inherited_body["codex_use_default_model_settings"] is True
    # Stored operator choices remain visible and can be restored later.
    assert inherited_body["codex_model"] == "gpt-5.6-sol"
    assert inherited_body["codex_reasoning_effort"] == "high"
    assert inherited_body["codex_fast_mode"] is True

    with factory() as session:
        runtime = effective_settings(session, settings)
        assert runtime.codex_model is None
        assert runtime.codex_reasoning_effort is None
        assert runtime.codex_fast_mode is False
        # Provider selection and authentication are intentionally independent.
        assert runtime.codex_base_url == "https://gateway.example.test/v1"
        stored = session.scalar(select(RuntimeConfiguration))
        assert stored is not None
        assert stored.codex_model == "gpt-5.6-sol"
        assert stored.codex_reasoning_effort == "high"
        assert stored.codex_fast_mode is True
        event = session.scalar(select(Event).order_by(Event.id.desc()))
        assert event is not None
        assert event.payload["codex_use_default_model_settings"] is True
        assert event.payload["codex_default_model_settings_action"] == "codex-defaults"

    restored = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(
            expected_revision=inherited_body["revision"],
            codex_use_default_model_settings=False,
        ),
    )
    assert restored.status_code == 200, restored.text
    with factory() as session:
        runtime = effective_settings(session, settings)
        assert runtime.codex_model == "gpt-5.6-sol"
        assert runtime.codex_reasoning_effort == "high"
        assert runtime.codex_fast_mode is True


def test_legacy_first_save_with_model_keeps_quazonai_override_behavior(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    legacy_payload = _payload()
    legacy_payload.pop("codex_use_default_model_settings")

    response = client.put(
        "/api/v1/system/runtime-configuration",
        json=legacy_payload,
    )
    assert response.status_code == 200, response.text
    assert response.json()["codex_use_default_model_settings"] is False

    factory = create_session_factory(engine)
    with factory() as session:
        runtime = effective_settings(session, settings)
        assert runtime.codex_model == "gpt-5.6-sol"
        assert runtime.codex_reasoning_effort == "high"
        assert runtime.codex_fast_mode is True


def test_default_mode_omission_is_distinct_and_preserves_existing_state() -> None:
    omitted_payload = _payload()
    omitted_payload.pop("codex_use_default_model_settings")
    omitted = RuntimeConfigurationInput.model_validate(omitted_payload)
    inherited = RuntimeConfigurationInput.model_validate(
        _payload(codex_use_default_model_settings=True)
    )
    overridden = RuntimeConfigurationInput.model_validate(
        _payload(codex_use_default_model_settings=False)
    )

    assert (
        _idempotency_shape(omitted)["codex_default_model_settings_action"]
        == "unchanged"
    )
    assert (
        _idempotency_shape(inherited)["codex_default_model_settings_action"]
        == "codex-defaults"
    )
    assert (
        _idempotency_shape(overridden)["codex_default_model_settings_action"]
        == "quazonai-overrides"
    )
