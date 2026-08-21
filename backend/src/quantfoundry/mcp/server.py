"""OAuth-protected MCP Tool and Resource mapping for QuantFoundry."""

from __future__ import annotations

import json
from collections.abc import Awaitable, Callable
from typing import Any
from urllib.parse import urlsplit, urlunsplit
from uuid import UUID

import httpx
from mcp import types as mcp_types
from mcp.server.auth.middleware.auth_context import get_access_token
from mcp.server.auth.settings import AuthSettings
from mcp.server.transport_security import TransportSecuritySettings
from starlette.requests import Request
from starlette.responses import JSONResponse, Response

from quantfoundry.mcp.auth import JwksTokenVerifier
from quantfoundry.mcp.client import CoreApiError, CoreClient
from quantfoundry.mcp.config import McpGatewaySettings
from quantfoundry.mcp.policy import (
    ScopedMCPServer,
    current_client,
    register_scope,
    require_scope,
)

READ = mcp_types.ToolAnnotations(
    read_only_hint=True,
    destructive_hint=False,
    idempotent_hint=True,
    open_world_hint=False,
)
WRITE = mcp_types.ToolAnnotations(
    read_only_hint=False,
    destructive_hint=False,
    idempotent_hint=False,
    open_world_hint=False,
)
HIGH_IMPACT = mcp_types.ToolAnnotations(
    read_only_hint=False,
    destructive_hint=True,
    idempotent_hint=True,
    open_world_hint=False,
)


def _tool(
    server: ScopedMCPServer,
    *,
    name: str,
    scope: str,
    annotations: mcp_types.ToolAnnotations,
) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    register_scope(name, scope)
    return server.tool(
        name=name,
        annotations=annotations,
        meta={"qf_required_scope": scope},
    )


def _assert_expected(current: Any, expected: dict[str, Any]) -> None:
    if not expected:
        raise CoreApiError(
            "PRECONDITION_REQUIRED",
            "Mutation requires current state/version/generation preconditions.",
            409,
            {},
        )
    if not isinstance(current, dict):
        raise CoreApiError(
            "PRECONDITION_FAILED",
            "Target resource is not an object.",
            409,
            {},
        )
    mismatches = {
        key: {"expected": value, "actual": current.get(key)}
        for key, value in expected.items()
        if current.get(key) != value
    }
    if mismatches:
        raise CoreApiError(
            "PRECONDITION_FAILED",
            "Target resource changed after it was read.",
            409,
            {"mismatches": mismatches},
        )


def _receipt_result(value: Any) -> dict[str, Any]:
    return value if isinstance(value, dict) else {"value": value}


async def _mutation(
    client: CoreClient,
    *,
    operation_name: str,
    idempotency_key: UUID,
    normalized_arguments: dict[str, Any],
    action: Callable[[], Awaitable[Any]],
    target_type: str | None = None,
    target_id: UUID | None = None,
) -> Any:
    receipt = await client.post(
        "/api/v1/agent/operations/begin",
        {
            "idempotency_key": str(idempotency_key),
            "operation_name": operation_name,
            "target_type": target_type,
            "target_id": str(target_id) if target_id else None,
            "normalized_arguments": normalized_arguments,
        },
    )
    if receipt["replay"]:
        if receipt["state"] == "SUCCEEDED":
            return receipt["result"]
        if receipt["state"] == "FAILED":
            raise CoreApiError(
                str(receipt.get("error_code") or "OPERATION_FAILED"),
                "The recorded operation previously failed.",
                409,
                receipt.get("result") or {},
            )
        return {
            "operation_state": "IN_PROGRESS",
            "receipt_id": receipt["id"],
            "message": "Read the current target before deciding whether human reconciliation is needed.",
        }
    try:
        result = await action()
    except CoreApiError as exc:
        if exc.code != "CORE_UNAVAILABLE":
            try:
                await client.post(
                    f"/api/v1/agent/operations/{receipt['id']}/fail",
                    {"error_code": exc.code, "result": exc.details},
                )
            except CoreApiError:
                pass
        raise
    await client.post(
        f"/api/v1/agent/operations/{receipt['id']}/complete",
        {"result": _receipt_result(result)},
    )
    return result


async def _consume_impact(
    client: CoreClient,
    *,
    token_id: UUID,
    operation_name: str,
    target_type: str,
    target_id: UUID,
    expected: dict[str, Any],
) -> None:
    await client.post(
        f"/api/v1/agent/impact-tokens/{token_id}/consume",
        {
            "operation_name": operation_name,
            "target_type": target_type,
            "target_id": str(target_id),
            "expected_state": expected,
        },
    )


def _public_origin(public_url: str) -> str:
    parsed = urlsplit(public_url)
    return urlunsplit((parsed.scheme, parsed.netloc, "", "", ""))


def create_server(settings: McpGatewaySettings) -> ScopedMCPServer:
    server = ScopedMCPServer(
        name="QuantFoundry",
        instructions=(
            "Operate the QuantFoundry workstation through current Resources and scoped Tools. "
            "Never request secrets or self-approve capital actions."
        ),
        token_verifier=JwksTokenVerifier(settings),
        auth=AuthSettings(
            issuer_url=settings.issuer_url,
            resource_server_url=settings.public_url,
            required_scopes=[],
        ),
        transport_security=TransportSecuritySettings(
            enable_dns_rebinding_protection=True,
            allowed_hosts=list(settings.allowed_hosts),
            allowed_origins=list(settings.allowed_origins),
        ),
        streamable_http_path="/mcp",
        stateless_http=True,
        json_response=True,
    )

    @_tool(server, name="qf.system.status", scope="qf:read", annotations=READ)
    async def system_status() -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get("/api/v1/system/health")

    @_tool(server, name="qf.plugin.list", scope="qf:read", annotations=READ)
    async def plugin_list() -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get("/api/v1/plugins")

    @_tool(server, name="qf.plugin.show", scope="qf:read", annotations=READ)
    async def plugin_show(release_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/plugin-releases/{release_id}")

    @_tool(server, name="qf.bundle.show", scope="qf:read", annotations=READ)
    async def bundle_show(bundle_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(
            f"/api/v1/plugin-runtime-bundles/{bundle_id}"
        )

    @_tool(server, name="qf.credential.list", scope="qf:read", annotations=READ)
    async def credential_list() -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get("/api/v1/credential-sets")

    @_tool(server, name="qf.credential.show", scope="qf:read", annotations=READ)
    async def credential_show(credential_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/credential-sets/{credential_id}")

    @_tool(server, name="qf.data_source.list", scope="qf:read", annotations=READ)
    async def data_source_list() -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get("/api/v1/data-sources")

    @_tool(server, name="qf.data_source.show", scope="qf:read", annotations=READ)
    async def data_source_show(source_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/data-sources/{source_id}")

    @_tool(server, name="qf.execution_connection.list", scope="qf:read", annotations=READ)
    async def execution_connection_list() -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get("/api/v1/execution-connections")

    @_tool(server, name="qf.execution_connection.show", scope="qf:read", annotations=READ)
    async def execution_connection_show(connection_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(
            f"/api/v1/execution-connections/{connection_id}"
        )

    @_tool(server, name="qf.dataset.list", scope="qf:read", annotations=READ)
    async def dataset_list() -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get("/api/v1/catalog-datasets")

    @_tool(server, name="qf.dataset.show", scope="qf:read", annotations=READ)
    async def dataset_show(dataset_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/catalog-datasets/{dataset_id}")

    @_tool(server, name="qf.strategy.list", scope="qf:read", annotations=READ)
    async def strategy_list() -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get("/api/v1/strategies")

    @_tool(server, name="qf.strategy.show", scope="qf:read", annotations=READ)
    async def strategy_show(strategy_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/strategies/{strategy_id}")

    @_tool(server, name="qf.research.list", scope="qf:read", annotations=READ)
    async def research_list() -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get("/api/v1/research-cases")

    @_tool(server, name="qf.research.show", scope="qf:read", annotations=READ)
    async def research_show(research_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/research-cases/{research_id}")

    @_tool(server, name="qf.experiment.show", scope="qf:read", annotations=READ)
    async def experiment_show(experiment_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/experiments/{experiment_id}")

    @_tool(server, name="qf.run.list", scope="qf:read", annotations=READ)
    async def run_list(experiment_id: UUID | None = None) -> list[dict[str, Any]]:
        require_scope("qf:read")
        params = {"experiment_id": str(experiment_id)} if experiment_id else None
        return await current_client(settings).get("/api/v1/runs", params=params)

    @_tool(server, name="qf.run.show", scope="qf:read", annotations=READ)
    async def run_show(run_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/runs/{run_id}")

    @_tool(server, name="qf.run.report", scope="qf:read", annotations=READ)
    async def run_report(run_id: UUID) -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/runs/{run_id}/reports")

    @_tool(server, name="qf.approval.list", scope="qf:read", annotations=READ)
    async def approval_list() -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get("/api/v1/approvals")

    @_tool(server, name="qf.approval.show", scope="qf:read", annotations=READ)
    async def approval_show(approval_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/approvals/{approval_id}")

    @_tool(server, name="qf.deployment.list", scope="qf:read", annotations=READ)
    async def deployment_list() -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get("/api/v1/deployments")

    @_tool(server, name="qf.deployment.show", scope="qf:read", annotations=READ)
    async def deployment_show(deployment_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/deployments/{deployment_id}")

    @_tool(server, name="qf.universe.show", scope="qf:read", annotations=READ)
    async def universe_show(deployment_id: UUID) -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get(
            f"/api/v1/deployments/{deployment_id}/universe-revisions"
        )

    @_tool(server, name="qf.risk.show", scope="qf:read", annotations=READ)
    async def risk_show(funder_id: str | None = None) -> Any:
        require_scope("qf:read")
        values = await current_client(settings).get("/api/v1/risk-accounts")
        if funder_id is None:
            return values
        return next((item for item in values if item["funder_id"] == funder_id), None)

    @_tool(server, name="qf.event.list", scope="qf:read", annotations=READ)
    async def event_list(after_id: int = 0, limit: int = 200) -> list[dict[str, Any]]:
        require_scope("qf:read")
        return await current_client(settings).get(
            "/api/v1/events",
            params={"after_id": after_id, "limit": min(max(limit, 1), 1000)},
        )

    @_tool(server, name="qf.artifact.show", scope="qf:read", annotations=READ)
    async def artifact_show(artifact_id: UUID) -> dict[str, Any]:
        require_scope("qf:read")
        return await current_client(settings).get(f"/api/v1/agent/artifacts/{artifact_id}")

    @_tool(server, name="qf.artifact.begin_upload", scope="qf:artifact:upload", annotations=WRITE)
    async def artifact_begin_upload(
        kind: str,
        filename: str,
        size_bytes: int,
    ) -> dict[str, Any]:
        require_scope("qf:artifact:upload")
        result = await current_client(settings).post(
            "/api/v1/agent/artifacts",
            {"kind": kind, "filename": filename, "size_bytes": size_bytes},
        )
        result["upload_url"] = f"{_public_origin(settings.public_url)}/agent-artifacts/{result['id']}"
        return result

    @_tool(server, name="qf.artifact.finalize_upload", scope="qf:artifact:upload", annotations=WRITE)
    async def artifact_finalize_upload(artifact_id: UUID) -> dict[str, Any]:
        require_scope("qf:artifact:upload")
        return await current_client(settings).post(
            f"/api/v1/agent/artifacts/{artifact_id}/finalize"
        )

    @_tool(server, name="qf.artifact.delete", scope="qf:artifact:upload", annotations=HIGH_IMPACT)
    async def artifact_delete(artifact_id: UUID) -> dict[str, Any]:
        require_scope("qf:artifact:upload")
        return await current_client(settings).delete(f"/api/v1/agent/artifacts/{artifact_id}")

    @_tool(server, name="qf.plugin.stage", scope="qf:plugin:stage", annotations=WRITE)
    async def plugin_stage(
        artifact_ids: list[UUID],
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:plugin:stage")
        client = current_client(settings)
        arguments = {"artifact_ids": [str(item) for item in artifact_ids]}
        return await _mutation(
            client,
            operation_name="plugin.stage",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            action=lambda: client.post(
                "/api/v1/agent/actions/plugin-releases/from-artifacts",
                arguments,
            ),
        )

    @_tool(server, name="qf.plugin.prewarm", scope="qf:plugin:stage", annotations=WRITE)
    async def plugin_prewarm(
        members: list[dict[str, Any]],
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:plugin:stage")
        client = current_client(settings)
        arguments = {"members": members}
        return await _mutation(
            client,
            operation_name="plugin.prewarm",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            action=lambda: client.post("/api/v1/plugin-runtime-bundles/prewarm", arguments),
        )

    @_tool(server, name="qf.plugin.impact", scope="qf:plugin:activate", annotations=READ)
    async def plugin_impact(release_id: UUID, operation: str) -> dict[str, Any]:
        require_scope("qf:plugin:activate")
        if operation not in {"plugin.activate", "plugin.deactivate"}:
            raise ValueError("operation must be plugin.activate or plugin.deactivate")
        client = current_client(settings)
        release = await client.get(f"/api/v1/plugin-releases/{release_id}")
        impact = await client.get(f"/api/v1/plugin-releases/{release_id}/impact")
        expected = {"state": release["state"], "is_default": release["is_default"]}
        token = await client.post(
            "/api/v1/agent/impact-tokens",
            {
                "operation_name": operation,
                "target_type": "plugin_release",
                "target_id": str(release_id),
                "expected_state": expected,
                "impact_summary": impact,
            },
        )
        return {"impact": impact, "expected": expected, "impact_token": token}

    async def _plugin_lifecycle(
        *,
        release_id: UUID,
        operation: str,
        expected: dict[str, Any],
        impact_token_id: UUID,
        idempotency_key: UUID,
    ) -> Any:
        client = current_client(settings)
        current = await client.get(f"/api/v1/plugin-releases/{release_id}")
        _assert_expected(current, expected)
        await _consume_impact(
            client,
            token_id=impact_token_id,
            operation_name=operation,
            target_type="plugin_release",
            target_id=release_id,
            expected=expected,
        )
        verb = "activate" if operation == "plugin.activate" else "deactivate"
        arguments = {
            "release_id": str(release_id),
            "expected": expected,
            "impact_token_id": str(impact_token_id),
        }
        return await _mutation(
            client,
            operation_name=operation,
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            target_type="plugin_release",
            target_id=release_id,
            action=lambda: client.post(f"/api/v1/plugin-releases/{release_id}/{verb}"),
        )

    @_tool(server, name="qf.plugin.activate", scope="qf:plugin:activate", annotations=HIGH_IMPACT)
    async def plugin_activate(
        release_id: UUID,
        expected: dict[str, Any],
        impact_token_id: UUID,
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:plugin:activate")
        return await _plugin_lifecycle(
            release_id=release_id,
            operation="plugin.activate",
            expected=expected,
            impact_token_id=impact_token_id,
            idempotency_key=idempotency_key,
        )

    @_tool(server, name="qf.plugin.deactivate", scope="qf:plugin:activate", annotations=HIGH_IMPACT)
    async def plugin_deactivate(
        release_id: UUID,
        expected: dict[str, Any],
        impact_token_id: UUID,
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:plugin:activate")
        return await _plugin_lifecycle(
            release_id=release_id,
            operation="plugin.deactivate",
            expected=expected,
            impact_token_id=impact_token_id,
            idempotency_key=idempotency_key,
        )

    @_tool(server, name="qf.data_source.create", scope="qf:data:write", annotations=WRITE)
    async def data_source_create(payload: dict[str, Any], idempotency_key: UUID) -> Any:
        require_scope("qf:data:write")
        client = current_client(settings)
        return await _mutation(
            client,
            operation_name="data_source.create",
            idempotency_key=idempotency_key,
            normalized_arguments=payload,
            action=lambda: client.post("/api/v1/data-sources", payload),
        )

    @_tool(server, name="qf.data_source.update", scope="qf:data:write", annotations=WRITE)
    async def data_source_update(
        source_id: UUID,
        payload: dict[str, Any],
        expected: dict[str, Any],
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:data:write")
        client = current_client(settings)
        current = await client.get(f"/api/v1/data-sources/{source_id}")
        _assert_expected(current, expected)
        arguments = {"source_id": str(source_id), "payload": payload, "expected": expected}
        return await _mutation(
            client,
            operation_name="data_source.update",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            target_type="data_source",
            target_id=source_id,
            action=lambda: client.put(f"/api/v1/data-sources/{source_id}", payload),
        )

    @_tool(server, name="qf.data_source.preflight", scope="qf:data:write", annotations=READ)
    async def data_source_preflight(source_id: UUID) -> dict[str, Any]:
        require_scope("qf:data:write")
        return await current_client(settings).post(f"/api/v1/data-sources/{source_id}/preflight")

    @_tool(server, name="qf.execution_connection.create", scope="qf:connection:write", annotations=WRITE)
    async def execution_connection_create(
        payload: dict[str, Any], idempotency_key: UUID
    ) -> Any:
        require_scope("qf:connection:write")
        client = current_client(settings)
        return await _mutation(
            client,
            operation_name="execution_connection.create",
            idempotency_key=idempotency_key,
            normalized_arguments=payload,
            action=lambda: client.post("/api/v1/execution-connections", payload),
        )

    @_tool(server, name="qf.execution_connection.update", scope="qf:connection:write", annotations=WRITE)
    async def execution_connection_update(
        connection_id: UUID,
        payload: dict[str, Any],
        expected: dict[str, Any],
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:connection:write")
        client = current_client(settings)
        current = await client.get(f"/api/v1/execution-connections/{connection_id}")
        _assert_expected(current, expected)
        arguments = {
            "connection_id": str(connection_id),
            "payload": payload,
            "expected": expected,
        }
        return await _mutation(
            client,
            operation_name="execution_connection.update",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            target_type="execution_connection",
            target_id=connection_id,
            action=lambda: client.put(
                f"/api/v1/execution-connections/{connection_id}", payload
            ),
        )

    @_tool(server, name="qf.execution_connection.preflight", scope="qf:connection:write", annotations=READ)
    async def execution_connection_preflight(connection_id: UUID) -> dict[str, Any]:
        require_scope("qf:connection:write")
        return await current_client(settings).post(
            f"/api/v1/execution-connections/{connection_id}/preflight"
        )

    @_tool(server, name="qf.dataset.import_parquet_l2", scope="qf:data:write", annotations=WRITE)
    async def dataset_import_parquet_l2(
        source_id: UUID,
        artifact_id: UUID,
        instrument_id: str,
        source_label: str,
        metadata: dict[str, Any],
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:data:write")
        client = current_client(settings)
        arguments = {
            "source_id": str(source_id),
            "artifact_id": str(artifact_id),
            "instrument_id": instrument_id,
            "source_label": source_label,
            "metadata": metadata,
        }
        return await _mutation(
            client,
            operation_name="dataset.import_parquet_l2",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            action=lambda: client.post(
                f"/api/v1/agent/actions/datasets/{source_id}/from-artifact"
                f"?artifact_id={artifact_id}",
                {
                    "instrument_id": instrument_id,
                    "source_label": source_label,
                    "metadata": metadata,
                },
            ),
        )

    @_tool(server, name="qf.strategy.create", scope="qf:research:write", annotations=WRITE)
    async def strategy_create(name: str, idempotency_key: UUID) -> Any:
        require_scope("qf:research:write")
        client = current_client(settings)
        arguments = {"name": name}
        return await _mutation(
            client,
            operation_name="strategy.create",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            action=lambda: client.post("/api/v1/strategies", arguments),
        )

    @_tool(server, name="qf.strategy.version_create", scope="qf:research:write", annotations=WRITE)
    async def strategy_version_create(
        strategy_id: UUID,
        artifact_id: UUID,
        default_config: dict[str, Any],
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:research:write")
        client = current_client(settings)
        arguments = {
            "strategy_id": str(strategy_id),
            "artifact_id": str(artifact_id),
            "default_config": default_config,
        }
        return await _mutation(
            client,
            operation_name="strategy.version_create",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            target_type="strategy",
            target_id=strategy_id,
            action=lambda: client.post(
                f"/api/v1/agent/actions/strategy-versions/{strategy_id}/from-artifact"
                f"?artifact_id={artifact_id}",
                {"default_config": default_config},
            ),
        )

    @_tool(server, name="qf.research.create", scope="qf:research:write", annotations=WRITE)
    async def research_create(
        title: str,
        strategy_version_id: UUID | None,
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:research:write")
        client = current_client(settings)
        arguments = {
            "title": title,
            "strategy_version_id": str(strategy_version_id) if strategy_version_id else None,
        }
        return await _mutation(
            client,
            operation_name="research.create",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            action=lambda: client.post("/api/v1/research-cases", arguments),
        )

    @_tool(server, name="qf.research.section_set", scope="qf:research:write", annotations=WRITE)
    async def research_section_set(
        research_id: UUID,
        section: str,
        markdown: str,
        expected: dict[str, Any],
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:research:write")
        client = current_client(settings)
        current = await client.get(f"/api/v1/research-cases/{research_id}")
        _assert_expected(current, expected)
        arguments = {
            "research_id": str(research_id),
            "section": section,
            "markdown": markdown,
            "expected": expected,
        }
        return await _mutation(
            client,
            operation_name="research.section_set",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            target_type="research",
            target_id=research_id,
            action=lambda: client.post(
                f"/api/v1/research-cases/{research_id}/sections",
                {"section": section, "markdown": markdown},
            ),
        )

    @_tool(server, name="qf.research.activate", scope="qf:research:write", annotations=WRITE)
    async def research_activate(
        research_id: UUID,
        expected: dict[str, Any],
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:research:write")
        client = current_client(settings)
        current = await client.get(f"/api/v1/research-cases/{research_id}")
        _assert_expected(current, expected)
        arguments = {"research_id": str(research_id), "expected": expected}
        return await _mutation(
            client,
            operation_name="research.activate",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            target_type="research",
            target_id=research_id,
            action=lambda: client.post(f"/api/v1/research-cases/{research_id}/activate"),
        )

    @_tool(server, name="qf.experiment.create", scope="qf:experiment:run", annotations=WRITE)
    async def experiment_create(
        research_id: UUID,
        payload: dict[str, Any],
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:experiment:run")
        client = current_client(settings)
        arguments = {"research_id": str(research_id), "payload": payload}
        return await _mutation(
            client,
            operation_name="experiment.create",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            target_type="research",
            target_id=research_id,
            action=lambda: client.post(
                f"/api/v1/research-cases/{research_id}/experiments", payload
            ),
        )

    @_tool(server, name="qf.experiment.start", scope="qf:experiment:run", annotations=WRITE)
    async def experiment_start(
        experiment_id: UUID,
        expected: dict[str, Any],
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:experiment:run")
        client = current_client(settings)
        current = await client.get(f"/api/v1/experiments/{experiment_id}")
        _assert_expected(current, expected)
        arguments = {"experiment_id": str(experiment_id), "expected": expected}
        return await _mutation(
            client,
            operation_name="experiment.start",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            target_type="experiment",
            target_id=experiment_id,
            action=lambda: client.post(f"/api/v1/experiments/{experiment_id}/start"),
        )

    @_tool(server, name="qf.approval.prepare_decision", scope="qf:approval:prepare", annotations=READ)
    async def approval_prepare_decision(approval_id: UUID) -> dict[str, Any]:
        require_scope("qf:approval:prepare")
        approval = await current_client(settings).get(f"/api/v1/approvals/{approval_id}")
        return {
            "approval": approval,
            "human_action_required": True,
            "local_cli": f"qf approval show {approval_id}",
            "decision_commands": [
                f"qf approval approve {approval_id}",
                f"qf approval reject {approval_id}",
            ],
            "agent_may_decide": False,
        }

    @_tool(server, name="qf.deployment.create", scope="qf:deployment:create", annotations=WRITE)
    async def deployment_create(payload: dict[str, Any], idempotency_key: UUID) -> Any:
        require_scope("qf:deployment:create")
        client = current_client(settings)
        return await _mutation(
            client,
            operation_name="deployment.create",
            idempotency_key=idempotency_key,
            normalized_arguments=payload,
            action=lambda: client.post("/api/v1/deployments", payload),
        )

    @_tool(server, name="qf.deployment.impact_stop", scope="qf:deployment:stop", annotations=READ)
    async def deployment_impact_stop(deployment_id: UUID) -> dict[str, Any]:
        require_scope("qf:deployment:stop")
        client = current_client(settings)
        deployment = await client.get(f"/api/v1/deployments/{deployment_id}")
        risks = await client.get("/api/v1/risk-accounts")
        expected = {
            "generation": deployment["generation"],
            "desired_state": deployment["desired_state"],
            "observed_state": deployment["observed_state"],
        }
        impact = {
            "deployment_id": str(deployment_id),
            "positions_liquidated": False,
            "open_orders_cancelled": True,
            "risk_account": next(
                (item for item in risks if item["funder_id"] == deployment["funder_id"]),
                None,
            ),
        }
        token = await client.post(
            "/api/v1/agent/impact-tokens",
            {
                "operation_name": "deployment.stop",
                "target_type": "deployment",
                "target_id": str(deployment_id),
                "expected_state": expected,
                "impact_summary": impact,
            },
        )
        return {"impact": impact, "expected": expected, "impact_token": token}

    @_tool(server, name="qf.deployment.stop", scope="qf:deployment:stop", annotations=HIGH_IMPACT)
    async def deployment_stop(
        deployment_id: UUID,
        expected: dict[str, Any],
        impact_token_id: UUID,
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:deployment:stop")
        client = current_client(settings)
        current = await client.get(f"/api/v1/deployments/{deployment_id}")
        _assert_expected(current, expected)
        await _consume_impact(
            client,
            token_id=impact_token_id,
            operation_name="deployment.stop",
            target_type="deployment",
            target_id=deployment_id,
            expected=expected,
        )
        arguments = {
            "deployment_id": str(deployment_id),
            "expected": expected,
            "impact_token_id": str(impact_token_id),
        }
        return await _mutation(
            client,
            operation_name="deployment.stop",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            target_type="deployment",
            target_id=deployment_id,
            action=lambda: client.post(f"/api/v1/deployments/{deployment_id}/stop"),
        )

    @_tool(server, name="qf.deployment.restart_request", scope="qf:deployment:create", annotations=WRITE)
    async def deployment_restart_request(
        deployment_id: UUID,
        expected: dict[str, Any],
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:deployment:create")
        client = current_client(settings)
        current = await client.get(f"/api/v1/deployments/{deployment_id}")
        _assert_expected(current, expected)
        arguments = {"deployment_id": str(deployment_id), "expected": expected}
        return await _mutation(
            client,
            operation_name="deployment.restart_request",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            target_type="deployment",
            target_id=deployment_id,
            action=lambda: client.post(f"/api/v1/deployments/{deployment_id}/restart"),
        )

    @_tool(server, name="qf.universe.impact", scope="qf:universe:propose", annotations=READ)
    async def universe_impact(
        deployment_id: UUID,
        predicate: dict[str, Any],
        cap: int,
        change_kind: str,
    ) -> dict[str, Any]:
        require_scope("qf:universe:propose")
        client = current_client(settings)
        deployment = await client.get(f"/api/v1/deployments/{deployment_id}")
        revisions = await client.get(
            f"/api/v1/deployments/{deployment_id}/universe-revisions"
        )
        expected = {
            "generation": deployment["generation"],
            "active_revision_id": deployment["active_revision_id"],
        }
        impact = {
            "deployment_id": str(deployment_id),
            "current_revisions": revisions,
            "proposed_predicate": predicate,
            "proposed_cap": cap,
            "change_kind": change_kind,
            "controlled_restart_possible": deployment["desired_state"] == "RUNNING",
        }
        token = await client.post(
            "/api/v1/agent/impact-tokens",
            {
                "operation_name": "universe.revision_create",
                "target_type": "deployment",
                "target_id": str(deployment_id),
                "expected_state": expected,
                "impact_summary": impact,
            },
        )
        return {"impact": impact, "expected": expected, "impact_token": token}

    @_tool(server, name="qf.universe.revision_create", scope="qf:universe:propose", annotations=HIGH_IMPACT)
    async def universe_revision_create(
        deployment_id: UUID,
        predicate: dict[str, Any],
        cap: int,
        change_kind: str,
        expected: dict[str, Any],
        impact_token_id: UUID,
        idempotency_key: UUID,
    ) -> Any:
        require_scope("qf:universe:propose")
        client = current_client(settings)
        current = await client.get(f"/api/v1/deployments/{deployment_id}")
        _assert_expected(current, expected)
        await _consume_impact(
            client,
            token_id=impact_token_id,
            operation_name="universe.revision_create",
            target_type="deployment",
            target_id=deployment_id,
            expected=expected,
        )
        body = {"predicate": predicate, "cap": cap, "change_kind": change_kind}
        arguments = {
            "deployment_id": str(deployment_id),
            **body,
            "expected": expected,
            "impact_token_id": str(impact_token_id),
        }
        return await _mutation(
            client,
            operation_name="universe.revision_create",
            idempotency_key=idempotency_key,
            normalized_arguments=arguments,
            target_type="deployment",
            target_id=deployment_id,
            action=lambda: client.post(
                f"/api/v1/deployments/{deployment_id}/universe-revisions", body
            ),
        )

    @server.resource("qf://manifest")
    async def manifest_resource() -> str:
        require_scope("qf:read")
        value = await current_client(settings).get("/api/v1/agent/manifest")
        value["available_tools"] = [
            item.name for item in await server.list_tools()
        ]
        return json.dumps(value, separators=(",", ":"), default=str)

    @server.resource("qf://system/status")
    async def system_resource() -> str:
        return json.dumps(await system_status(), separators=(",", ":"), default=str)

    @server.resource("qf://plugin-releases/{resource_id}")
    async def plugin_resource(resource_id: str) -> str:
        return json.dumps(
            await plugin_show(UUID(resource_id)), separators=(",", ":"), default=str
        )

    @server.resource("qf://datasets/{resource_id}")
    async def dataset_resource(resource_id: str) -> str:
        return json.dumps(
            await dataset_show(UUID(resource_id)), separators=(",", ":"), default=str
        )

    @server.resource("qf://strategies/{resource_id}")
    async def strategy_resource(resource_id: str) -> str:
        return json.dumps(
            await strategy_show(UUID(resource_id)), separators=(",", ":"), default=str
        )

    @server.resource("qf://research/{resource_id}")
    async def research_resource(resource_id: str) -> str:
        return json.dumps(
            await research_show(UUID(resource_id)), separators=(",", ":"), default=str
        )

    @server.resource("qf://experiments/{resource_id}")
    async def experiment_resource(resource_id: str) -> str:
        return json.dumps(
            await experiment_show(UUID(resource_id)), separators=(",", ":"), default=str
        )

    @server.resource("qf://runs/{resource_id}")
    async def run_resource(resource_id: str) -> str:
        return json.dumps(
            await run_show(UUID(resource_id)), separators=(",", ":"), default=str
        )

    @server.resource("qf://approvals/{resource_id}")
    async def approval_resource(resource_id: str) -> str:
        return json.dumps(
            await approval_show(UUID(resource_id)), separators=(",", ":"), default=str
        )

    @server.resource("qf://deployments/{resource_id}")
    async def deployment_resource(resource_id: str) -> str:
        return json.dumps(
            await deployment_show(UUID(resource_id)), separators=(",", ":"), default=str
        )

    @server.resource("qf://deployments/{resource_id}/risk")
    async def deployment_risk_resource(resource_id: str) -> str:
        deployment = await deployment_show(UUID(resource_id))
        return json.dumps(
            await risk_show(deployment["funder_id"]),
            separators=(",", ":"),
            default=str,
        )

    @server.resource("qf://deployments/{resource_id}/universe")
    async def deployment_universe_resource(resource_id: str) -> str:
        return json.dumps(
            await universe_show(UUID(resource_id)), separators=(",", ":"), default=str
        )

    @server.custom_route("/agent-artifacts/{artifact_id}", methods=["HEAD", "PUT"])
    async def artifact_upload_route(request: Request) -> Response:
        require_scope("qf:artifact:upload")
        token = get_access_token()
        if token is None:
            return JSONResponse({"error": "unauthorized"}, status_code=401)
        try:
            artifact_id = UUID(request.path_params["artifact_id"])
        except ValueError:
            return JSONResponse({"error": "invalid artifact id"}, status_code=404)
        client = current_client(settings)
        if request.method == "HEAD":
            try:
                value = await client.get(f"/api/v1/agent/artifacts/{artifact_id}")
            except CoreApiError as exc:
                return JSONResponse(
                    {"error": {"code": exc.code, "message": exc.message, "details": exc.details}},
                    status_code=exc.status_code,
                )
            return Response(
                status_code=204,
                headers={
                    "X-QF-Upload-Offset": str(value["size_received"]),
                    "X-QF-Upload-Length": str(value["size_declared"]),
                    "X-QF-Artifact-State": value["state"],
                },
            )
        try:
            offset = int(request.headers.get("x-qf-upload-offset", ""))
        except ValueError:
            return JSONResponse({"error": "invalid upload offset"}, status_code=422)

        async def bounded_stream() -> Any:
            total = 0
            async for chunk in request.stream():
                total += len(chunk)
                if total > 16 * 1024 * 1024:
                    raise ValueError("one upload request cannot exceed 16 MiB")
                yield chunk

        headers = client.headers()
        headers["X-QF-Upload-Offset"] = str(offset)
        try:
            async with httpx.AsyncClient(
                base_url=settings.core_url,
                timeout=settings.request_timeout_seconds,
            ) as http:
                response = await http.put(
                    f"/api/v1/agent/artifacts/{artifact_id}/content",
                    headers=headers,
                    content=bounded_stream(),
                )
                body = await response.aread()
        except (httpx.RequestError, ValueError) as exc:
            return JSONResponse(
                {"error": {"code": "UPLOAD_FAILED", "message": str(exc)}},
                status_code=503,
            )
        return Response(
            content=body,
            status_code=response.status_code,
            media_type=response.headers.get("content-type", "application/json"),
        )

    return server
