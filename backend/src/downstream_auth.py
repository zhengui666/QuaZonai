"""Authentication primitives for downstream handoff consumers."""

from __future__ import annotations

import secrets
from dataclasses import dataclass
from uuid import UUID

from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from db.models import DownstreamSystem
from errors import QfError
from settings import Settings

_KEY_VERSION = 1


@dataclass(frozen=True, slots=True)
class IssuedServiceToken:
    token: str
    ciphertext: bytes
    nonce: bytes
    key_version: int = _KEY_VERSION


def _aad(downstream_id: UUID, key_version: int) -> bytes:
    return f"quazonai|downstream={downstream_id}|key_version={key_version}".encode("utf-8")


def issue_service_token(settings: Settings, downstream_id: UUID) -> IssuedServiceToken:
    """Generate a bearer credential and return only its encrypted-at-rest representation."""
    token = secrets.token_urlsafe(32)
    nonce = secrets.token_bytes(12)
    ciphertext = AESGCM(settings.master_key_bytes()).encrypt(
        nonce,
        token.encode("utf-8"),
        _aad(downstream_id, _KEY_VERSION),
    )
    return IssuedServiceToken(token=token, ciphertext=ciphertext, nonce=nonce)


def install_service_token(downstream: DownstreamSystem, issued: IssuedServiceToken) -> None:
    downstream.service_token_ciphertext = issued.ciphertext
    downstream.service_token_nonce = issued.nonce
    downstream.service_token_key_version = issued.key_version


def _bearer_value(authorization: str | None) -> str:
    if authorization is None:
        raise QfError(
            "DOWNSTREAM_AUTH_REQUIRED",
            "A downstream Bearer service token is required.",
            401,
        )
    scheme, separator, token = authorization.partition(" ")
    if not separator or scheme.casefold() != "bearer" or not token.strip():
        raise QfError(
            "DOWNSTREAM_AUTH_REQUIRED",
            "Authorization must use a Bearer service token.",
            401,
        )
    return token.strip()


def authenticate_downstream(
    settings: Settings,
    downstream: DownstreamSystem,
    authorization: str | None,
) -> None:
    """Verify the caller against the credential bound to one Downstream System."""
    provided = _bearer_value(authorization)
    if (
        downstream.service_token_ciphertext is None
        or downstream.service_token_nonce is None
        or downstream.service_token_key_version is None
    ):
        raise QfError(
            "DOWNSTREAM_CREDENTIAL_NOT_CONFIGURED",
            "This Downstream System has no active service credential.",
            403,
        )
    try:
        expected = AESGCM(settings.master_key_bytes()).decrypt(
            downstream.service_token_nonce,
            downstream.service_token_ciphertext,
            _aad(downstream.id, downstream.service_token_key_version),
        ).decode("utf-8")
    except Exception as exc:  # noqa: BLE001 - authentication boundary must not leak crypto detail
        raise QfError(
            "DOWNSTREAM_CREDENTIAL_INVALID",
            "Downstream service credential cannot be verified.",
            403,
        ) from exc
    if not secrets.compare_digest(expected, provided):
        raise QfError(
            "DOWNSTREAM_UNAUTHORIZED",
            "Downstream service credential does not own this Handoff.",
            403,
        )
