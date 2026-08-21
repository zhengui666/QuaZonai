from __future__ import annotations

from dataclasses import replace
from uuid import uuid4

from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from settings import Settings


def _headers(*, subject: str = "operator") -> dict[str, str]:
    return {
        "X-QZ-Internal-Token": "gateway-secret",
        "X-QZ-Agent-Issuer": "https://issuer.example",
        "X-QZ-Agent-Subject": subject,
        "X-QZ-Agent-Client-Id": "agent-client",
        "X-QZ-Agent-Scopes": "quazonai:read quazonai:research:write quazonai:artifact:upload",
    }


def _client(engine: Engine, settings: Settings) -> TestClient:
    configured = replace(settings, mcp_internal_token="gateway-secret")
    configured.ensure_worker_directories()
    return TestClient(create_app(settings=configured, engine=engine))


def test_internal_agent_api_is_disabled_without_gateway_token(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    response = client.get("/api/v1/agent/manifest", headers=_headers())
    assert response.status_code == 503
    assert response.json()["error"]["code"] == "MCP_GATEWAY_DISABLED"


def test_operation_receipt_replays_identical_request_and_rejects_reuse(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    key = str(uuid4())
    payload = {
        "idempotency_key": key,
        "operation_name": "research.create",
        "normalized_arguments": {"title": "Market hypothesis"},
    }
    first = client.post("/api/v1/agent/operations/begin", headers=_headers(), json=payload)
    assert first.status_code == 200, first.text
    assert first.json()["state"] == "IN_PROGRESS"
    assert first.json()["replay"] is False

    replay = client.post("/api/v1/agent/operations/begin", headers=_headers(), json=payload)
    assert replay.status_code == 200
    assert replay.json()["id"] == first.json()["id"]
    assert replay.json()["replay"] is True

    conflict = client.post(
        "/api/v1/agent/operations/begin",
        headers=_headers(),
        json={**payload, "normalized_arguments": {"title": "Changed"}},
    )
    assert conflict.status_code == 409
    assert conflict.json()["error"]["code"] == "IDEMPOTENCY_KEY_REUSED"

    completed = client.post(
        f"/api/v1/agent/operations/{first.json()['id']}/complete",
        headers=_headers(),
        json={"result": {"research_id": str(uuid4())}},
    )
    assert completed.status_code == 200
    assert completed.json()["state"] == "SUCCEEDED"


def test_artifact_upload_is_offset_bound_and_principal_isolated(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    begin = client.post(
        "/api/v1/agent/artifacts",
        headers=_headers(),
        json={"kind": "STRATEGY_SOURCE", "filename": "strategy.py", "size_bytes": 6},
    )
    assert begin.status_code == 201, begin.text
    artifact_id = begin.json()["id"]

    wrong_offset = client.put(
        f"/api/v1/agent/artifacts/{artifact_id}/content",
        headers={**_headers(), "X-QZ-Upload-Offset": "1"},
        content=b"abc",
    )
    assert wrong_offset.status_code == 409
    assert wrong_offset.json()["error"]["code"] == "AGENT_ARTIFACT_OFFSET_CONFLICT"

    first = client.put(
        f"/api/v1/agent/artifacts/{artifact_id}/content",
        headers={**_headers(), "X-QZ-Upload-Offset": "0"},
        content=b"abc",
    )
    assert first.status_code == 200
    assert first.json()["size_received"] == 3

    second = client.put(
        f"/api/v1/agent/artifacts/{artifact_id}/content",
        headers={**_headers(), "X-QZ-Upload-Offset": "3"},
        content=b"def",
    )
    assert second.status_code == 200
    assert second.json()["size_received"] == 6

    finalized = client.post(
        f"/api/v1/agent/artifacts/{artifact_id}/finalize",
        headers=_headers(),
    )
    assert finalized.status_code == 200
    assert finalized.json()["state"] == "READY"

    downloaded = client.get(
        f"/api/v1/agent/artifacts/{artifact_id}/content",
        headers=_headers(),
    )
    assert downloaded.status_code == 200
    assert downloaded.content == b"abcdef"

    isolated = client.get(
        f"/api/v1/agent/artifacts/{artifact_id}",
        headers=_headers(subject="another-user"),
    )
    assert isolated.status_code == 404

    resource_id = str(uuid4())
    consumed = client.post(
        f"/api/v1/agent/artifacts/{artifact_id}/consume",
        headers=_headers(),
        json={"resource_type": "strategy_version", "resource_id": resource_id},
    )
    assert consumed.status_code == 200
    assert consumed.json()["state"] == "CONSUMED"

    deleted = client.delete(
        f"/api/v1/agent/artifacts/{artifact_id}",
        headers=_headers(),
    )
    assert deleted.status_code == 409
    assert deleted.json()["error"]["code"] == "AGENT_ARTIFACT_ALREADY_CONSUMED"


def test_impact_tokens_are_single_use_and_state_bound(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    target_id = str(uuid4())
    issued = client.post(
        "/api/v1/agent/impact-tokens",
        headers=_headers(),
        json={
            "operation_name": "deployment.stop",
            "target_type": "deployment",
            "target_id": target_id,
            "expected_state": {"generation": 7, "observed_state": "RUNNING"},
            "impact_summary": {"positions_liquidated": False},
        },
    )
    assert issued.status_code == 201
    token_id = issued.json()["id"]

    mismatch = client.post(
        f"/api/v1/agent/impact-tokens/{token_id}/consume",
        headers=_headers(),
        json={
            "operation_name": "deployment.stop",
            "target_type": "deployment",
            "target_id": target_id,
            "expected_state": {"generation": 8, "observed_state": "RUNNING"},
        },
    )
    assert mismatch.status_code == 409
    assert mismatch.json()["error"]["code"] == "IMPACT_TOKEN_MISMATCH"

    consumed = client.post(
        f"/api/v1/agent/impact-tokens/{token_id}/consume",
        headers=_headers(),
        json={
            "operation_name": "deployment.stop",
            "target_type": "deployment",
            "target_id": target_id,
            "expected_state": {"generation": 7, "observed_state": "RUNNING"},
        },
    )
    assert consumed.status_code == 200
    assert consumed.json()["consumed_at"] is not None

    duplicate = client.post(
        f"/api/v1/agent/impact-tokens/{token_id}/consume",
        headers=_headers(),
        json={
            "operation_name": "deployment.stop",
            "target_type": "deployment",
            "target_id": target_id,
            "expected_state": {"generation": 7, "observed_state": "RUNNING"},
        },
    )
    assert duplicate.status_code == 409
    assert duplicate.json()["error"]["code"] == "IMPACT_TOKEN_CONSUMED"


def test_task_binding_is_principal_scoped(engine: Engine, settings: Settings) -> None:
    client = _client(engine, settings)
    task_id = "task-1"
    operation_id = str(uuid4())
    bound = client.post(
        "/api/v1/agent/tasks",
        headers=_headers(),
        json={
            "task_id": task_id,
            "extension_version": "io.modelcontextprotocol/tasks@1",
            "operation_type": "run",
            "operation_id": operation_id,
        },
    )
    assert bound.status_code == 201

    visible = client.get(f"/api/v1/agent/tasks/{task_id}", headers=_headers())
    assert visible.status_code == 200
    assert visible.json()["operation_id"] == operation_id

    hidden = client.get(
        f"/api/v1/agent/tasks/{task_id}",
        headers=_headers(subject="another-user"),
    )
    assert hidden.status_code == 404
