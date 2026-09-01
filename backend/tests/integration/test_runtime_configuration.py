from __future__ import annotations

import socket
import time
from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace
from pathlib import Path

import pytest
from fastapi.testclient import TestClient
from sqlalchemy import Engine, func, select

from api.system import RuntimeConfigurationInput, _claim_idempotency_receipt
from db.models import Event, PublicMutationReceipt, RuntimeConfiguration
from db.session import create_session_factory
from main import create_app
from runtime_config import effective_settings
from runners.codex_provider_auth import fetch_token
from runners.research_missions import (
    BROKER_ACCEPT_POLL_SECONDS,
    CUSTOM_CODEX_PROVIDER_ID,
    _codex_launch_configuration,
    _codex_service_tier,
    _codex_thread_config,
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
        "codex_reasoning_effort": None,
        "codex_fast_mode": False,
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
    assert defaults.json()["codex_reasoning_effort"] is None
    assert defaults.json()["codex_fast_mode"] is False
    assert defaults.json()["max_plugin_wheel_bytes"] == settings.max_plugin_wheel_bytes

    response = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(
            codex_api_key="sk-runtime-secret",
            codex_reasoning_effort="high",
            codex_fast_mode=True,
        ),
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
        assert runtime.codex_reasoning_effort == "high"
        assert runtime.codex_fast_mode is True
        assert runtime.job_poll_seconds == 0.5
        event = session.scalar(
            select(Event).where(Event.kind == "RUNTIME_CONFIGURATION_UPDATED")
        )
        assert event is not None
        assert "sk-runtime-secret" not in str(event.payload)
        assert event.payload["codex_reasoning_effort"] == "high"
        assert event.payload["codex_fast_mode"] is True
        assert event.payload["codex_reasoning_effort_action"] == "set"
        assert event.payload["codex_fast_mode_action"] == "fast"


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


def test_runtime_configuration_deduplicates_key_retry_without_retaining_secret(
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
        serialized = str(receipt.normalized_request)
        assert "sk-idempotent-secret" not in serialized
        assert "codex_api_key" not in receipt.normalized_request
        assert "codex_api_key_secret" not in receipt.normalized_request
        assert receipt.normalized_request["codex_api_key_action"] == "set"


def test_pre_upgrade_runtime_receipt_replays_after_new_controls(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    legacy_payload = _payload()
    for field in ("codex_reasoning_effort", "codex_fast_mode", "codex_use_default_model_settings"):
        legacy_payload.pop(field, None)
    headers = {"Idempotency-Key": "runtime-config-pre-upgrade-retry"}

    first = client.put(
        "/api/v1/system/runtime-configuration",
        json=legacy_payload,
        headers=headers,
    )
    assert first.status_code == 200, first.text

    factory = create_session_factory(engine)
    with factory.begin() as session:
        receipt = session.get(PublicMutationReceipt, headers["Idempotency-Key"])
        assert receipt is not None
        normalized = dict(receipt.normalized_request)
        for field in (
            "codex_reasoning_effort_action",
            "codex_fast_mode_action",
            "codex_default_model_settings_action",
        ):
            normalized.pop(field, None)
        receipt.normalized_request = normalized
        response_json = dict(receipt.response_json)
        response_json.pop("codex_reasoning_effort", None)
        response_json.pop("codex_fast_mode", None)
        response_json.pop("codex_use_default_model_settings", None)
        receipt.response_json = response_json

    replay = client.put(
        "/api/v1/system/runtime-configuration",
        json=legacy_payload,
        headers=headers,
    )
    assert replay.status_code == 200, replay.text
    assert replay.json() == first.json()

    with factory() as session:
        receipt = session.get(PublicMutationReceipt, headers["Idempotency-Key"])
        assert receipt is not None
        assert receipt.response_json["codex_reasoning_effort"] is None
        assert receipt.response_json["codex_fast_mode"] is False
        assert receipt.response_json["codex_use_default_model_settings"] is False


def test_key_bearing_receipt_remains_replayable_after_later_update(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    first_headers = {"Idempotency-Key": "runtime-config-original-key-save"}
    first_payload = _payload(codex_api_key="sk-original-idempotent")

    first = client.put(
        "/api/v1/system/runtime-configuration",
        json=first_payload,
        headers=first_headers,
    )
    assert first.status_code == 200, first.text

    later = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(
            expected_revision=first.json()["revision"],
            codex_model="gpt-5.6-sol-next",
        ),
        headers={"Idempotency-Key": "runtime-config-later-save"},
    )
    assert later.status_code == 200, later.text
    assert later.json()["revision"] == first.json()["revision"] + 1

    replay = client.put(
        "/api/v1/system/runtime-configuration",
        json=first_payload,
        headers=first_headers,
    )
    assert replay.status_code == 200, replay.text
    assert replay.json() == first.json()


def test_key_bearing_idempotency_equivalence_uses_secret_action_not_secret_value(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    headers = {"Idempotency-Key": "runtime-config-write-only-secret"}
    original = _payload(codex_api_key="sk-first-value")

    first = client.put("/api/v1/system/runtime-configuration", json=original, headers=headers)
    assert first.status_code == 200, first.text

    replay = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(codex_api_key="sk-different-retry-value"),
        headers=headers,
    )
    assert replay.status_code == 200, replay.text
    assert replay.json() == first.json()

    factory = create_session_factory(engine)
    with factory() as session:
        runtime = effective_settings(session, settings)
        assert runtime.codex_api_key == "sk-first-value"
        event_count = session.scalar(
            select(func.count()).select_from(Event).where(
                Event.kind == "RUNTIME_CONFIGURATION_UPDATED"
            )
        )
        assert event_count == 1


def test_idempotency_receipt_claim_serializes_same_key(engine: Engine) -> None:
    if engine.dialect.name != "postgresql":
        pytest.skip("Concurrent receipt serialization is verified on PostgreSQL")

    factory = create_session_factory(engine)
    payload = RuntimeConfigurationInput.model_validate(_payload())

    with factory() as first_session:
        first_transaction = first_session.begin()
        first_receipt, first_claimed = _claim_idempotency_receipt(
            first_session,
            "runtime-config-concurrent",
            payload,
        )
        assert first_claimed is True

        def claim_again() -> tuple[str, bool]:
            with factory.begin() as second_session:
                receipt, claimed = _claim_idempotency_receipt(
                    second_session,
                    "runtime-config-concurrent",
                    payload,
                )
                return receipt.idempotency_key, claimed

        with ThreadPoolExecutor(max_workers=1) as pool:
            second = pool.submit(claim_again)
            time.sleep(0.25)
            assert second.done() is False
            first_receipt.response_json = {"revision": 1}
            first_receipt.status_code = 200
            first_transaction.commit()
            key, claimed = second.result(timeout=5)

    assert key == "runtime-config-concurrent"
    assert claimed is False


def test_runtime_configuration_rejects_worker_limits_outside_bounds(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    invalid_payloads = (
        _payload(max_plugin_wheel_bytes=1_073_741_825),
        _payload(max_plugin_wheel_bytes=9_223_372_036_854_775_808),
        _payload(job_poll_seconds=0.001),
        _payload(job_poll_seconds=3600.01),
        _payload(plugin_validation_timeout_seconds=86_401),
        _payload(bundle_build_timeout_seconds=86_401),
        _payload(plugin_job_timeout_seconds=2_147_484),
        _payload(mission_job_timeout_seconds=2_147_484),
        _payload(job_lease_seconds=86_401),
    )
    for payload in invalid_payloads:
        response = client.put("/api/v1/system/runtime-configuration", json=payload)
        assert response.status_code == 422, response.text


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


def test_runtime_configuration_persists_reasoning_and_fast_controls_and_can_restore_defaults(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    created = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(codex_reasoning_effort="xhigh", codex_fast_mode=True),
    )
    assert created.status_code == 200, created.text
    assert created.json()["codex_reasoning_effort"] == "xhigh"
    assert created.json()["codex_fast_mode"] is True

    restored = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(
            expected_revision=created.json()["revision"],
            codex_reasoning_effort=None,
            codex_fast_mode=False,
        ),
    )
    assert restored.status_code == 200, restored.text
    assert restored.json()["codex_reasoning_effort"] is None
    assert restored.json()["codex_fast_mode"] is False


def test_legacy_runtime_client_omission_preserves_reasoning_and_fast_controls(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    created = client.put(
        "/api/v1/system/runtime-configuration",
        json=_payload(codex_reasoning_effort="high", codex_fast_mode=True),
    )
    assert created.status_code == 200
    legacy_payload = _payload(expected_revision=created.json()["revision"])
    legacy_payload.pop("codex_reasoning_effort")
    legacy_payload.pop("codex_fast_mode")

    updated = client.put("/api/v1/system/runtime-configuration", json=legacy_payload)
    assert updated.status_code == 200, updated.text
    assert updated.json()["codex_reasoning_effort"] == "high"
    assert updated.json()["codex_fast_mode"] is True


def test_runtime_configuration_rejects_unsupported_reasoning_effort(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    for value in ("", " high ", "HIGH", "none", "max", "ultra", 5):
        response = client.put(
            "/api/v1/system/runtime-configuration",
            json=_payload(codex_reasoning_effort=value),
        )
        assert response.status_code == 422, response.text


def test_runtime_idempotency_shape_distinguishes_reasoning_and_fast_actions() -> None:
    omitted_payload = _payload()
    omitted_payload.pop("codex_reasoning_effort")
    omitted_payload.pop("codex_fast_mode")
    omitted = RuntimeConfigurationInput.model_validate(omitted_payload)
    inherit_standard = RuntimeConfigurationInput.model_validate(
        _payload(codex_reasoning_effort=None, codex_fast_mode=False)
    )
    set_fast = RuntimeConfigurationInput.model_validate(
        _payload(codex_reasoning_effort="high", codex_fast_mode=True)
    )

    from api.system import _idempotency_shape

    assert _idempotency_shape(omitted)["codex_reasoning_effort_action"] == "unchanged"
    assert _idempotency_shape(omitted)["codex_fast_mode_action"] == "unchanged"
    assert _idempotency_shape(inherit_standard)["codex_reasoning_effort_action"] == "inherit-default"
    assert _idempotency_shape(inherit_standard)["codex_fast_mode_action"] == "standard"
    assert _idempotency_shape(set_fast)["codex_reasoning_effort_action"] == "set:high"
    assert _idempotency_shape(set_fast)["codex_fast_mode_action"] == "fast"


def test_codex_thread_controls_are_per_mission_and_preserve_sandbox_defaults(
    settings: Settings,
) -> None:
    for effort in (None, "minimal", "low", "medium", "high", "xhigh"):
        configured = replace(settings, codex_reasoning_effort=effort, codex_fast_mode=effort == "xhigh")
        config = _codex_thread_config(configured)
        assert config["sandbox_workspace_write"] == {"network_access": False}
        assert config["web_search"] == "disabled"
        if effort is None:
            assert "model_reasoning_effort" not in config
        else:
            assert config["model_reasoning_effort"] == effort
        assert _codex_service_tier(configured) == ("fast" if effort == "xhigh" else None)


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


def test_provider_credential_broker_waits_and_reads_fragmented_request() -> None:
    with _provider_credential_broker("sk-fragmented") as socket_path:
        assert socket_path is not None
        time.sleep(BROKER_ACCEPT_POLL_SECONDS * 3)
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(5.0)
            client.connect(str(socket_path))
            client.sendall(b"TOK")
            time.sleep(0.05)
            client.sendall(b"EN\n")
            chunks: list[bytes] = []
            while True:
                chunk = client.recv(4096)
                if not chunk:
                    break
                chunks.append(chunk)
        assert b"".join(chunks).decode("utf-8") == "sk-fragmented"
    assert not socket_path.exists()
