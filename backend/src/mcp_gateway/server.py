"""OAuth-protected MCP mapping for QuaZonai research intelligence."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any
from uuid import UUID

from mcp import types as mcp_types
from mcp.server.auth.settings import AuthSettings
from mcp.server.transport_security import TransportSecuritySettings

from mcp_gateway.auth import JwksTokenVerifier
from mcp_gateway.config import McpGatewaySettings
from mcp_gateway.policy import ScopedMCPServer, current_client, register_scope, require_scope

READ = mcp_types.ToolAnnotations(
    read_only_hint=True,
    destructive_hint=False,
    idempotent_hint=True,
    open_world_hint=False,
)
WRITE = mcp_types.ToolAnnotations(
    read_only_hint=False,
    destructive_hint=False,
    idempotent_hint=True,
    open_world_hint=False,
)
DECISION = mcp_types.ToolAnnotations(
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
    return server.tool(name=name, annotations=annotations, meta={"qf_required_scope": scope})


async def _write(
    settings: McpGatewaySettings,
    method: str,
    path: str,
    body: dict[str, Any],
    idempotency_key: UUID,
) -> Any:
    return await current_client(settings).request(
        method,
        path,
        json_body=body,
        extra_headers={"Idempotency-Key": str(idempotency_key)},
    )


def create_server(settings: McpGatewaySettings) -> ScopedMCPServer:
    server = ScopedMCPServer(
        name="QuaZonai",
        instructions=(
            "Operate QuaZonai as a research-intelligence and portfolio-construction workbench. "
            "Never request broker credentials, place orders, manage positions, or control a downstream runtime. "
            "Human approval is limited to immutable Candidate decisions."
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

    @_tool(server, name="quazonai.system.status", scope="quazonai:read", annotations=READ)
    async def system_status() -> dict[str, Any]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/system/health")

    @_tool(server, name="quazonai.readiness", scope="quazonai:read", annotations=READ)
    async def readiness() -> dict[str, Any]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/readiness")

    @_tool(server, name="quazonai.research.list", scope="quazonai:read", annotations=READ)
    async def research_list() -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/research-programs")

    @_tool(server, name="quazonai.research.show", scope="quazonai:read", annotations=READ)
    async def research_show(program_id: UUID) -> dict[str, Any]:
        require_scope("quazonai:read")
        return await current_client(settings).get(f"/api/v1/research-programs/{program_id}")

    @_tool(server, name="quazonai.research.missions", scope="quazonai:read", annotations=READ)
    async def research_missions(program_id: UUID) -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get(
            f"/api/v1/research-programs/{program_id}/missions"
        )

    @_tool(server, name="quazonai.research.activity", scope="quazonai:read", annotations=READ)
    async def research_activity(program_id: UUID) -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get(
            f"/api/v1/research-programs/{program_id}/activity"
        )

    @_tool(server, name="quazonai.research.create", scope="quazonai:research:write", annotations=WRITE)
    async def research_create(idea: str, idempotency_key: UUID) -> dict[str, Any]:
        require_scope("quazonai:research:write")
        return await _write(
            settings,
            "POST",
            "/api/v1/research-programs",
            {"idea": idea, "answers": {}},
            idempotency_key,
        )

    async def _program_action(
        program_id: UUID,
        action: str,
        reason: str | None,
        idempotency_key: UUID,
    ) -> dict[str, Any]:
        return await _write(
            settings,
            "POST",
            f"/api/v1/research-programs/{program_id}/{action}",
            {"reason": reason},
            idempotency_key,
        )

    @_tool(server, name="quazonai.research.pause", scope="quazonai:research:write", annotations=WRITE)
    async def research_pause(
        program_id: UUID,
        idempotency_key: UUID,
        reason: str | None = None,
    ) -> dict[str, Any]:
        require_scope("quazonai:research:write")
        return await _program_action(program_id, "pause", reason, idempotency_key)

    @_tool(server, name="quazonai.research.resume", scope="quazonai:research:write", annotations=WRITE)
    async def research_resume(
        program_id: UUID,
        idempotency_key: UUID,
        reason: str | None = None,
    ) -> dict[str, Any]:
        require_scope("quazonai:research:write")
        return await _program_action(program_id, "resume", reason, idempotency_key)

    @_tool(server, name="quazonai.alpha.list", scope="quazonai:read", annotations=READ)
    async def alpha_list() -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/alpha-library")

    @_tool(server, name="quazonai.alpha.show", scope="quazonai:read", annotations=READ)
    async def alpha_show(qualification_id: UUID) -> dict[str, Any]:
        require_scope("quazonai:read")
        return await current_client(settings).get(f"/api/v1/alpha-library/{qualification_id}")

    @_tool(server, name="quazonai.portfolio.mandates", scope="quazonai:read", annotations=READ)
    async def portfolio_mandates() -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/portfolio-mandates")

    @_tool(server, name="quazonai.portfolio.programs", scope="quazonai:read", annotations=READ)
    async def portfolio_programs() -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/portfolio-programs")

    @_tool(server, name="quazonai.portfolio.candidate", scope="quazonai:read", annotations=READ)
    async def portfolio_candidate(candidate_id: UUID) -> dict[str, Any]:
        require_scope("quazonai:read")
        return await current_client(settings).get(f"/api/v1/portfolio-candidates/{candidate_id}")

    @_tool(server, name="quazonai.approval.list", scope="quazonai:read", annotations=READ)
    async def approval_list() -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/approvals")

    @_tool(server, name="quazonai.approval.show", scope="quazonai:read", annotations=READ)
    async def approval_show(approval_id: UUID) -> dict[str, Any]:
        require_scope("quazonai:read")
        return await current_client(settings).get(f"/api/v1/approvals/{approval_id}")

    @_tool(server, name="quazonai.approval.approve", scope="quazonai:approval:write", annotations=DECISION)
    async def approval_approve(
        approval_id: UUID,
        downstream_system_id: UUID,
        expected_state: str,
        idempotency_key: UUID,
    ) -> dict[str, Any]:
        require_scope("quazonai:approval:write")
        return await _write(
            settings,
            "POST",
            f"/api/v1/approvals/{approval_id}/approve",
            {
                "downstream_system_id": str(downstream_system_id),
                "expected_state": expected_state,
            },
            idempotency_key,
        )

    @_tool(server, name="quazonai.approval.reject", scope="quazonai:approval:write", annotations=DECISION)
    async def approval_reject(
        approval_id: UUID,
        reason_code: str,
        expected_state: str,
        idempotency_key: UUID,
        note: str | None = None,
    ) -> dict[str, Any]:
        require_scope("quazonai:approval:write")
        return await _write(
            settings,
            "POST",
            f"/api/v1/approvals/{approval_id}/reject",
            {"reason_code": reason_code, "note": note, "expected_state": expected_state},
            idempotency_key,
        )

    @_tool(server, name="quazonai.handoff.list", scope="quazonai:read", annotations=READ)
    async def handoff_list() -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/handoffs")

    @_tool(server, name="quazonai.handoff.revoke", scope="quazonai:handoff:write", annotations=DECISION)
    async def handoff_revoke(
        handoff_id: UUID,
        reason_code: str,
        idempotency_key: UUID,
    ) -> dict[str, Any]:
        require_scope("quazonai:handoff:write")
        return await _write(
            settings,
            "POST",
            f"/api/v1/handoffs/{handoff_id}/revoke",
            {"reason_code": reason_code},
            idempotency_key,
        )

    @_tool(server, name="quazonai.data_source.list", scope="quazonai:read", annotations=READ)
    async def data_source_list() -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/data-sources")

    @_tool(server, name="quazonai.data_source.create", scope="quazonai:admin:write", annotations=WRITE)
    async def data_source_create(
        name: str,
        idempotency_key: UUID,
        provider: str | None = None,
        fields: list[str] | None = None,
    ) -> dict[str, Any]:
        require_scope("quazonai:admin:write")
        return await _write(
            settings,
            "POST",
            "/api/v1/data-sources",
            {"name": name, "provider": provider, "fields": fields or []},
            idempotency_key,
        )

    @_tool(server, name="quazonai.dataset.list", scope="quazonai:read", annotations=READ)
    async def dataset_list() -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/datasets")

    @_tool(server, name="quazonai.universe.list", scope="quazonai:read", annotations=READ)
    async def universe_list() -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/universes")

    @_tool(server, name="quazonai.downstream.list", scope="quazonai:read", annotations=READ)
    async def downstream_list() -> list[dict[str, Any]]:
        require_scope("quazonai:read")
        return await current_client(settings).get("/api/v1/downstream-systems")

    return server
