"""Executable entry point for the optional QuantFoundry MCP Gateway."""

from __future__ import annotations

from starlette.applications import Starlette

from quantfoundry.mcp.config import McpGatewaySettings
from quantfoundry.mcp.server import create_server


def create_app(settings: McpGatewaySettings | None = None) -> Starlette:
    runtime_settings = settings or McpGatewaySettings.from_env()
    return create_server(runtime_settings).streamable_http_app()


def main() -> int:
    settings = McpGatewaySettings.from_env()
    server = create_server(settings)
    server.run(
        transport="streamable-http",
        host=settings.host,
        port=settings.port,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
