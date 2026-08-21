"""MCP scope policy shared by discovery and Tool handlers."""

from __future__ import annotations

from collections.abc import Iterable
from typing import Any

from mcp.server.auth.middleware.auth_context import get_access_token
from mcp.server.mcpserver import MCPServer

from mcp.client import AgentIdentity, CoreClient
from mcp.config import McpGatewaySettings

TOOL_SCOPES: dict[str, str] = {}


class ScopedMCPServer(MCPServer):
    async def list_tools(self) -> list[Any]:
        tools = await super().list_tools()
        token = get_access_token()
        if token is None:
            return []
        scopes = set(token.scopes)
        return [
            tool
            for tool in tools
            if TOOL_SCOPES.get(tool.name, "qf:read") in scopes
        ]


def register_scope(tool_name: str, scope: str) -> None:
    TOOL_SCOPES[tool_name] = scope


def require_scope(scope: str) -> None:
    token = get_access_token()
    if token is None or scope not in set(token.scopes):
        raise PermissionError(f"OAuth access token requires scope {scope}")


def require_any_scope(scopes: Iterable[str]) -> str:
    token = get_access_token()
    available = set(token.scopes) if token is not None else set()
    for scope in scopes:
        if scope in available:
            return scope
    raise PermissionError("OAuth access token does not contain an accepted scope")


def current_client(settings: McpGatewaySettings) -> CoreClient:
    token = get_access_token()
    if token is None:
        raise PermissionError("OAuth access token is unavailable")
    return CoreClient(settings, AgentIdentity.from_token(token, settings))
