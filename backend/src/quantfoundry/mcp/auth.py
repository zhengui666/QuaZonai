"""OAuth access-token verification for the MCP resource server."""

from __future__ import annotations

import asyncio
from typing import Any

import jwt
from mcp.server.auth.provider import AccessToken, TokenVerifier

from quantfoundry.mcp.config import McpGatewaySettings


class JwksTokenVerifier(TokenVerifier):
    """Verify signed JWT access tokens without forwarding them downstream."""

    def __init__(self, settings: McpGatewaySettings) -> None:
        self.settings = settings
        self.jwks = jwt.PyJWKClient(settings.jwks_url)

    def _decode(self, token: str) -> dict[str, Any]:
        signing_key = self.jwks.get_signing_key_from_jwt(token).key
        value = jwt.decode(
            token,
            signing_key,
            algorithms=list(self.settings.jwt_algorithms),
            audience=self.settings.audience,
            issuer=self.settings.issuer_url,
            options={"require": ["exp", "iss", "aud"]},
        )
        if not isinstance(value, dict):
            raise jwt.InvalidTokenError("JWT claims must be an object")
        return value

    async def verify_token(self, token: str) -> AccessToken | None:
        try:
            claims = await asyncio.to_thread(self._decode, token)
            client_id = str(claims.get("azp") or claims.get("client_id") or "").strip()
            if not client_id:
                return None
            raw_scope = claims.get("scope", "")
            scopes = (
                [item for item in str(raw_scope).split() if item]
                if not isinstance(raw_scope, list)
                else [str(item) for item in raw_scope if str(item)]
            )
            raw_scp = claims.get("scp")
            if isinstance(raw_scp, list):
                scopes.extend(str(item) for item in raw_scp if str(item))
            expires_at = int(claims["exp"])
            return AccessToken(
                token=token,
                client_id=client_id,
                scopes=sorted(set(scopes)),
                expires_at=expires_at,
                resource=self.settings.audience,
                subject=(str(claims["sub"]) if claims.get("sub") is not None else None),
                claims=claims,
            )
        except (jwt.PyJWTError, ValueError, TypeError, KeyError):
            return None
