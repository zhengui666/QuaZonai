"""Configuration for the optional HTTPS MCP Gateway."""

from __future__ import annotations

import os
from dataclasses import dataclass
from urllib.parse import urlparse


class McpConfigurationError(ValueError):
    """Raised when the optional MCP edge is configured unsafely."""


def _csv(name: str) -> tuple[str, ...]:
    return tuple(item.strip() for item in os.environ.get(name, "").split(",") if item.strip())


@dataclass(frozen=True, slots=True)
class McpGatewaySettings:
    host: str
    port: int
    public_url: str
    issuer_url: str
    jwks_url: str
    audience: str
    core_url: str
    internal_token: str
    allowed_hosts: tuple[str, ...]
    allowed_origins: tuple[str, ...]
    jwt_algorithms: tuple[str, ...]
    request_timeout_seconds: float

    @classmethod
    def from_env(cls) -> McpGatewaySettings:
        public_url = os.environ.get("QF_MCP_PUBLIC_URL", "").strip()
        issuer_url = os.environ.get("QF_MCP_ISSUER_URL", "").strip()
        jwks_url = os.environ.get("QF_MCP_JWKS_URL", "").strip()
        core_url = os.environ.get("QF_MCP_CORE_URL", "http://api:8000").strip()
        internal_token = os.environ.get("QF_MCP_INTERNAL_TOKEN", "")
        if not public_url or not issuer_url or not jwks_url or not internal_token:
            raise McpConfigurationError(
                "QF_MCP_PUBLIC_URL, QF_MCP_ISSUER_URL, QF_MCP_JWKS_URL, and "
                "QF_MCP_INTERNAL_TOKEN are required"
            )
        parsed_public = urlparse(public_url)
        if parsed_public.scheme != "https" or not parsed_public.netloc:
            raise McpConfigurationError("QF_MCP_PUBLIC_URL must be an absolute HTTPS URL")
        if parsed_public.path.rstrip("/") != "/mcp":
            raise McpConfigurationError("QF_MCP_PUBLIC_URL must end with /mcp")
        parsed_issuer = urlparse(issuer_url)
        parsed_jwks = urlparse(jwks_url)
        if parsed_issuer.scheme != "https" or not parsed_issuer.netloc:
            raise McpConfigurationError("QF_MCP_ISSUER_URL must use HTTPS")
        if parsed_jwks.scheme != "https" or not parsed_jwks.netloc:
            raise McpConfigurationError("QF_MCP_JWKS_URL must use HTTPS")
        parsed_core = urlparse(core_url)
        if parsed_core.scheme != "http" or parsed_core.hostname not in {
            "api",
            "127.0.0.1",
            "localhost",
            "::1",
        }:
            raise McpConfigurationError(
                "QF_MCP_CORE_URL must target the internal api service or loopback"
            )
        allowed_hosts = _csv("QF_MCP_ALLOWED_HOSTS") or (parsed_public.netloc,)
        allowed_origins = _csv("QF_MCP_ALLOWED_ORIGINS")
        algorithms = _csv("QF_MCP_JWT_ALGORITHMS") or ("RS256",)
        if any(item.lower() == "none" for item in algorithms):
            raise McpConfigurationError("Unsigned JWT algorithms are not allowed")
        try:
            port = int(os.environ.get("QF_MCP_PORT", "8001"))
            timeout = float(os.environ.get("QF_MCP_CORE_TIMEOUT_SECONDS", "30"))
        except ValueError as exc:
            raise McpConfigurationError("MCP port and timeout must be numeric") from exc
        if not 1 <= port <= 65535 or timeout <= 0:
            raise McpConfigurationError("MCP port or timeout is outside its valid range")
        return cls(
            host=os.environ.get("QF_MCP_HOST", "0.0.0.0"),
            port=port,
            public_url=public_url,
            issuer_url=issuer_url.rstrip("/"),
            jwks_url=jwks_url,
            audience=os.environ.get("QF_MCP_AUDIENCE", public_url).strip(),
            core_url=core_url.rstrip("/"),
            internal_token=internal_token,
            allowed_hosts=allowed_hosts,
            allowed_origins=allowed_origins,
            jwt_algorithms=algorithms,
            request_timeout_seconds=timeout,
        )
