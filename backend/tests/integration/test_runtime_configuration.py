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


def test_runtime_configuration_deduplicates_key_retry_with_encrypted_receipt_secret(
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
        secret_record = receipt.normalized_request["codex_api_key_secret"]
        assert isinstance(secret_record, dict)
        assert secret_record["ciphertext"]
        assert secret_record["nonce"]


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


def test_idempotency_receipt_claim_serializes_same_key(
    engine: Engine,
    settings: Settings,
) -> None:
    if engine.dialect.name != "postgresql":
        pytest.skip("Concurrent receipt serialization is verified on PostgreSQL")

    factory = create_session_factory(engine)
    payload = RuntimeConfigurationInput.model_validate(_payload())

    with factory() as first_session:
        first_transaction = first_session.begin()
        first_receipt, first_claimed = _claim_idempotency_receipt(
            first_session,
            settings,
            "runtime-config-concurrent",
            payload,
        )
        assert first_claimed is True

        def claim_again() -> tuple[str, bool]:
            with factory.begin() as second_session:
                receipt, claimed = _claim_idempotency_receipt(
                    second_session,
                    settings,
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
