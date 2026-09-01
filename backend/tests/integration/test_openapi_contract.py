from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from settings import Settings


REQUIRED_PATHS = {
    "/api/v1/ideas/preview",
    "/api/v1/research-programs",
    "/api/v1/research-programs/{program_id}",
    "/api/v1/research-programs/{program_id}/pause",
    "/api/v1/research-programs/{program_id}/resume",
    "/api/v1/research-programs/{program_id}/archive",
    "/api/v1/research-programs/{program_id}/restore",
    "/api/v1/research-programs/{program_id}/activity",
    "/api/v1/research-programs/{program_id}/missions",
    "/api/v1/alpha-library",
    "/api/v1/alpha-library/{qualification_id}",
    "/api/v1/portfolio-mandates",
    "/api/v1/portfolio-mandates/{mandate_id}/enable",
    "/api/v1/portfolio-mandates/{mandate_id}/disable",
    "/api/v1/portfolio-programs",
    "/api/v1/portfolio-candidates/{candidate_id}",
    "/api/v1/approvals",
    "/api/v1/approvals/{approval_id}",
    "/api/v1/approvals/{approval_id}/approve",
    "/api/v1/approvals/{approval_id}/reject",
    "/api/v1/handoffs",
    "/api/v1/handoffs/{handoff_id}/revoke",
    "/api/v1/handoffs/{handoff_id}/claim",
    "/api/v1/handoffs/{handoff_id}/accept",
    "/api/v1/handoffs/{handoff_id}/reject",
    "/api/v1/handoffs/{handoff_id}/package",
    "/api/v1/handoffs/{handoff_id}/feedback",
    "/api/v1/data-sources",
    "/api/v1/data-sources/{source_id}",
    "/api/v1/datasets",
    "/api/v1/universes",
    "/api/v1/downstream-systems",
    "/api/v1/downstream-systems/{downstream_id}",
    "/api/v1/readiness",
    "/api/v1/events/stream",
    "/api/v1/plugin-releases",
    "/api/v1/system/health",
    "/api/v1/system/runtime-configuration",
}

FORBIDDEN_PREFIXES = (
    "/api/v1/deployments",
    "/api/v1/risk-accounts",
    "/api/v1/execution-connections",
    "/api/v1/strategies",
    "/api/v1/runs",
)


def test_openapi_matches_research_intelligence_contract(
    engine: Engine,
    settings: Settings,
) -> None:
    client = TestClient(create_app(settings=settings, engine=engine))
    response = client.get("/api/v1/openapi.json")
    assert response.status_code == 200, response.text
    schema = response.json()
    paths = set(schema["paths"])

    missing = REQUIRED_PATHS - paths
    assert not missing, f"missing DESIGN API paths: {sorted(missing)}"

    forbidden = sorted(
        path for path in paths if any(path.startswith(prefix) for prefix in FORBIDDEN_PREFIXES)
    )
    assert not forbidden, f"execution-owned API paths leaked into QuaZonai: {forbidden}"

    request_ref = schema["paths"]["/api/v1/system/runtime-configuration"]["put"][
        "requestBody"
    ]["content"]["application/json"]["schema"]["$ref"]
    request_schema = schema["components"]["schemas"][request_ref.rsplit("/", 1)[-1]]
    model_mode = request_schema["properties"]["codex_use_default_model_settings"]
    assert model_mode["type"] == "boolean"
    assert "codex_use_default_model_settings" not in request_schema.get("required", [])
