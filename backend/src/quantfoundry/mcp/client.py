"""Private HTTP client used by the MCP edge to call the Core API."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any
from uuid import UUID

import httpx
from mcp.server.auth.provider import AccessToken

from quantfoundry.mcp.config import McpGatewaySettings


@dataclass(slots=True)
class CoreApiError(Exception):
    code: str
    message: str
    status_code: int
    details: dict[str, Any]

    def __str__(self) -> str:
        return f"{self.code}: {self.message}"


@dataclass(frozen=True, slots=True)
class AgentIdentity:
    issuer: str
    subject: str
    client_id: str
    scopes: tuple[str, ...]

    @classmethod
    def from_token(
        cls,
        token: AccessToken,
        settings: McpGatewaySettings,
    ) -> AgentIdentity:
        claims = token.claims or {}
        return cls(
            issuer=str(claims.get("iss") or settings.issuer_url),
            subject=token.subject or "",
            client_id=token.client_id,
            scopes=tuple(sorted(set(token.scopes))),
        )


class CoreClient:
    def __init__(
        self,
        settings: McpGatewaySettings,
        identity: AgentIdentity,
    ) -> None:
        self.settings = settings
        self.identity = identity

    def headers(self) -> dict[str, str]:
        return {
            "X-QF-Internal-Token": self.settings.internal_token,
            "X-QF-Agent-Issuer": self.identity.issuer,
            "X-QF-Agent-Subject": self.identity.subject,
            "X-QF-Agent-Client-Id": self.identity.client_id,
            "X-QF-Agent-Scopes": " ".join(self.identity.scopes),
        }

    async def request(
        self,
        method: str,
        path: str,
        *,
        json_body: dict[str, Any] | list[Any] | None = None,
        params: dict[str, Any] | None = None,
        extra_headers: dict[str, str] | None = None,
        content: bytes | None = None,
    ) -> Any:
        headers = self.headers()
        if extra_headers:
            headers.update(extra_headers)
        try:
            async with httpx.AsyncClient(
                base_url=self.settings.core_url,
                timeout=self.settings.request_timeout_seconds,
            ) as client:
                response = await client.request(
                    method,
                    path,
                    json=json_body,
                    params=params,
                    headers=headers,
                    content=content,
                )
        except httpx.RequestError as exc:
            raise CoreApiError(
                "CORE_UNAVAILABLE",
                "QuantFoundry Core API is unavailable.",
                503,
                {},
            ) from exc
        if response.status_code >= 400:
            try:
                body = response.json()
                error = body.get("error", {}) if isinstance(body, dict) else {}
            except ValueError:
                error = {}
            raise CoreApiError(
                str(error.get("code") or "CORE_REQUEST_FAILED"),
                str(error.get("message") or "QuantFoundry Core request failed."),
                response.status_code,
                dict(error.get("details") or {}),
            )
        if response.status_code == 204 or not response.content:
            return None
        try:
            return response.json()
        except ValueError as exc:
            raise CoreApiError(
                "CORE_RESPONSE_INVALID",
                "QuantFoundry Core returned invalid JSON.",
                502,
                {},
            ) from exc

    async def get(self, path: str, *, params: dict[str, Any] | None = None) -> Any:
        return await self.request("GET", path, params=params)

    async def post(self, path: str, body: dict[str, Any] | None = None) -> Any:
        return await self.request("POST", path, json_body=body or {})

    async def put(self, path: str, body: dict[str, Any]) -> Any:
        return await self.request("PUT", path, json_body=body)

    async def patch(self, path: str, body: dict[str, Any]) -> Any:
        return await self.request("PATCH", path, json_body=body)

    async def delete(self, path: str, *, params: dict[str, Any] | None = None) -> Any:
        return await self.request("DELETE", path, params=params)

    async def upload_chunk(self, artifact_id: UUID, offset: int, content: bytes) -> Any:
        return await self.request(
            "PUT",
            f"/api/v1/agent/artifacts/{artifact_id}/content",
            extra_headers={"X-QF-Upload-Offset": str(offset)},
            content=content,
        )
