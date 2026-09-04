from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from settings import Settings


REQUIRED_PATHS = {
    "/api/v1/idea-drafts",
    "/api/v1/idea-drafts/{draft_id}",
    "/api/v1/idea-drafts/{draft_id}/answers",
    "/api/v1/idea-drafts/{draft_id}/start",
    "/api/v1/research-programs",
    "/api/v1/research-programs/{program_id}",
    "/api/v1/research-programs/{program_id}/pause",
    "/api/v1/research-programs/{program_id}/resume",
    "/api/v1/research-programs/{program_id}/archive",
    "/api/v1/research-programs/{program_id}/wake",
    "/api/v1/research-programs/{program_id}/cycles",
    "/api/v1/research-programs/{program_id}/mission-graph",
    "/api/v1/missions/{mission_id}",
    "/api/v1/missions/{mission_id}/turns",
    "/api/v1/missions/{mission_id}/artifacts",
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
    "/api/v1/handoffs/{handoff_id}/degradation-observations",
    "/api/v1/data-sources",
    "/api/v1/data-sources/{data_source_id}/preflight",
    "/api/v1/datasets",
    "/api/v1/universes",
    "/api/v1/downstream-systems",
    "/api/v1/downstream-systems/{downstream_id}/preflight",
    "/api/v1/universes/{universe_id}/versions",
    "/api/v1/datasets/materializations",
    "/api/v1/datasets/{dataset_id}",
    "/api/v1/datasets/{dataset_id}/quality",
    "/api/v1/datasets/{dataset_id}/profile",
    "/api/v1/evaluation-dataset-selections",
    "/api/v1/evaluation-design-versions",
    "/api/v1/operations/{operation_id}",
    "/api/v1/portfolio-mandates/{mandate_id}/versions",
    "/api/v1/capital-contexts",
    "/api/v1/downstream-systems/{downstream_id}/rotate-service-token",
    "/api/v1/promotion-policy-versions",
    "/api/v1/readiness",
    "/api/v1/events/stream",
    "/api/v1/plugin-releases",
    "/api/v1/system/health",
    "/api/v1/system/runtime-configuration",
    "/api/v1/system/codex-auth",
    "/api/v1/system/codex-auth/chatgpt/device/start",
    "/api/v1/system/codex-auth/chatgpt/device/{login_id}/poll",
    "/api/v1/system/codex-auth/chatgpt/device/{login_id}",
    "/api/v1/system/codex-auth/chatgpt",
}

FORBIDDEN_PREFIXES = (
    "/api/v1/deployments",
    "/api/v1/risk-accounts",
    "/api/v1/execution-connections",
    "/api/v1/strategies",
    "/api/v1/runs",
    "/api/v1/configuration",
    "/api/v1/quant-runtime",
)

FORBIDDEN_PATHS = {
    "/api/v1/ideas/preview",
    "/api/v1/research-programs/{program_id}/restore",
    "/api/v1/research-programs/{program_id}/activity",
    "/api/v1/research-programs/{program_id}/missions",
}


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
    assert not paths & FORBIDDEN_PATHS

    preflight = schema["paths"]["/api/v1/data-sources/{data_source_id}/preflight"]["post"]
    preflight_ref = preflight["requestBody"]["content"]["application/json"]["schema"]["$ref"]
    preflight_schema = schema["components"]["schemas"][preflight_ref.rsplit("/", 1)[-1]]
    assert preflight_schema.get("properties", {}) == {}

    downstream_preflight = schema["paths"][
        "/api/v1/downstream-systems/{downstream_id}/preflight"
    ]["post"]
    downstream_preflight_ref = downstream_preflight["requestBody"]["content"][
        "application/json"
    ]["schema"]["$ref"]
    downstream_preflight_schema = schema["components"]["schemas"][
        downstream_preflight_ref.rsplit("/", 1)[-1]
    ]
    assert set(downstream_preflight_schema["properties"]) == {
        "package_contract_version",
        "feedback_contract_version",
        "compatibility",
        "valid_until",
    }
    assert set(downstream_preflight_schema["required"]) == {
        "package_contract_version",
        "feedback_contract_version",
        "compatibility",
        "valid_until",
    }

    for path, required in {
        "/api/v1/evaluation-dataset-selections": {
            "universe_version_id",
            "discovery_dataset_revision_id",
            "validation_dataset_revision_id",
            "sealed_dataset_revision_id",
            "state",
        },
        "/api/v1/evaluation-design-versions": {
            "universe_version_id",
            "contract_version",
            "allowed_model_mode",
            "qualification_role",
            "walk_forward_folds",
            "annualization_factor",
            "multiple_testing_method",
            "multiple_testing_max_trials",
            "qualification_metric_code",
            "qualification_comparator",
            "qualification_threshold",
            "pass_disclosure_code",
            "failure_disclosure_code",
            "inconclusive_disclosure_code",
            "invalid_disclosure_code",
            "state",
        },
        "/api/v1/promotion-policy-versions": {
            "purpose",
            "mode",
            "gates",
            "state",
        },
    }.items():
        operation = schema["paths"][path]["post"]
        request_ref = operation["requestBody"]["content"]["application/json"]["schema"]["$ref"]
        request_schema = schema["components"]["schemas"][request_ref.rsplit("/", 1)[-1]]
        assert set(request_schema["required"]) == required
        assert request_schema.get("additionalProperties") is False

    request_schema_ref = schema["paths"]["/api/v1/system/runtime-configuration"]["put"][
        "requestBody"
    ]["content"]["application/json"]["schema"]["$ref"]
    request_schema = schema["components"]["schemas"][request_schema_ref.rsplit("/", 1)[-1]]
    effort_schema = request_schema["properties"]["codex_reasoning_effort"]
    enum_values = next(item["enum"] for item in effort_schema["anyOf"] if "enum" in item)
    assert enum_values == ["minimal", "low", "medium", "high", "xhigh"]
    assert request_schema["properties"]["codex_fast_mode"]["type"] == "boolean"
    assert (
        request_schema["properties"]["codex_use_default_model_settings"]["type"]
        == "boolean"
    )
    device_start = schema["paths"]["/api/v1/system/codex-auth/chatgpt/device/start"]["post"]
    assert "requestBody" in device_start
    assert "application/json" in device_start["requestBody"]["content"]
    assert device_start["requestBody"].get("required") is True
    auth_schema_names = {name.casefold() for name in schema["components"]["schemas"]}
    assert not {"accesstoken", "refreshtoken", "idtoken"} & auth_schema_names
    auth_json = str(
        [schema["paths"][path] for path in REQUIRED_PATHS if path.startswith("/api/v1/system/codex-auth")]
    ).casefold()
    assert "access_token" not in auth_json
    assert "refresh_token" not in auth_json
    assert "id_token" not in auth_json
