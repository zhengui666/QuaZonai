"""Opaque, revocable native operator credentials.

Native credentials intentionally use authenticated encryption rather than JWT and
are cryptographically separated from browser cookies and the machine API token.
The database stores only device state/generation; bearer material is never stored.
"""

from __future__ import annotations

import base64
import binascii
import json
import secrets
import time
import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Literal

from cryptography.exceptions import InvalidTag
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from sqlalchemy import select
from sqlalchemy.orm import Session

from db.auth_models import MobileOperatorDevice
from db.session import SessionFactory
from settings import Settings

MOBILE_TOKEN_PREFIX = "qzm1."
MOBILE_TOKEN_VERSION = 1
MOBILE_TOKEN_NONCE_BYTES = 12
MOBILE_ACCESS_TTL_SECONDS = 15 * 60
_MOBILE_KEY_INFO = b"quazonai/native-operator/mobile-session/v1"


@dataclass(frozen=True, slots=True)
class MobileCredentialClaims:
    kind: Literal["access", "refresh"]
    device_id: uuid.UUID
    generation: int
    expires_at: int


@dataclass(frozen=True, slots=True)
class MobileOperatorIdentity:
    username: str
    device_id: uuid.UUID
    credential_generation: int
    source: Literal["mobile"] = "mobile"
    renew_session: bool = False


def utc_now() -> datetime:
    return datetime.now(timezone.utc)


def normalize_utc(value: datetime) -> datetime:
    """Return a timezone-aware UTC timestamp for persisted domain datetimes."""
    if value.tzinfo is None:
        return value.replace(tzinfo=timezone.utc)
    return value.astimezone(timezone.utc)


def _mobile_key(settings: Settings) -> bytes:
    """Derive a key-separated native credential key from the cookie root key."""
    return HKDF(
        algorithm=hashes.SHA256(),
        length=32,
        salt=None,
        info=_MOBILE_KEY_INFO,
    ).derive(settings.auth_cookie_key_bytes())


def _aad(kind: str) -> bytes:
    return f"quazonai|native-operator|kind={kind}|version={MOBILE_TOKEN_VERSION}".encode()


def _b64encode(value: bytes) -> str:
    return base64.urlsafe_b64encode(value).rstrip(b"=").decode("ascii")


def _b64decode(value: str) -> bytes:
    padding = "=" * ((4 - len(value) % 4) % 4)
    try:
        return base64.b64decode(value + padding, altchars=b"-_", validate=True)
    except (ValueError, binascii.Error) as exc:
        raise ValueError("invalid mobile credential") from exc


def issue_mobile_credential(
    settings: Settings,
    *,
    kind: Literal["access", "refresh"],
    device_id: uuid.UUID,
    generation: int,
    expires_at: datetime,
) -> str:
    payload = json.dumps(
        {
            "v": MOBILE_TOKEN_VERSION,
            "sub": "operator",
            "kind": kind,
            "device_id": str(device_id),
            "generation": generation,
            "exp": int(expires_at.timestamp()),
        },
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    nonce = secrets.token_bytes(MOBILE_TOKEN_NONCE_BYTES)
    ciphertext = AESGCM(_mobile_key(settings)).encrypt(nonce, payload, _aad(kind))
    return MOBILE_TOKEN_PREFIX + _b64encode(nonce + ciphertext)


def decode_mobile_credential(
    settings: Settings,
    token: str,
    *,
    expected_kind: Literal["access", "refresh"],
) -> MobileCredentialClaims | None:
    if not token.startswith(MOBILE_TOKEN_PREFIX):
        return None
    try:
        encoded = _b64decode(token[len(MOBILE_TOKEN_PREFIX) :])
        if len(encoded) <= MOBILE_TOKEN_NONCE_BYTES:
            return None
        plaintext = AESGCM(_mobile_key(settings)).decrypt(
            encoded[:MOBILE_TOKEN_NONCE_BYTES],
            encoded[MOBILE_TOKEN_NONCE_BYTES:],
            _aad(expected_kind),
        )
        payload = json.loads(plaintext)
        if not isinstance(payload, dict):
            return None
        if payload.get("v") != MOBILE_TOKEN_VERSION or payload.get("sub") != "operator":
            return None
        if payload.get("kind") != expected_kind:
            return None
        generation = payload.get("generation")
        expires_at = payload.get("exp")
        if (
            isinstance(generation, bool)
            or not isinstance(generation, int)
            or generation < 1
            or isinstance(expires_at, bool)
            or not isinstance(expires_at, int)
            or expires_at <= int(time.time())
        ):
            return None
        return MobileCredentialClaims(
            kind=expected_kind,
            device_id=uuid.UUID(str(payload.get("device_id"))),
            generation=generation,
            expires_at=expires_at,
        )
    except (ValueError, TypeError, json.JSONDecodeError, InvalidTag):
        return None


def bearer_token(authorization: str | None) -> str | None:
    if authorization is None:
        return None
    scheme, separator, token = authorization.partition(" ")
    if not separator or scheme.casefold() != "bearer":
        return None
    token = token.lstrip(" ")
    if not token or token != token.strip():
        return None
    return token


def credential_from_authorization(
    settings: Settings,
    authorization: str | None,
    *,
    expected_kind: Literal["access", "refresh"],
) -> MobileCredentialClaims | None:
    token = bearer_token(authorization)
    if token is None:
        return None
    return decode_mobile_credential(settings, token, expected_kind=expected_kind)


def authenticate_mobile_access(
    settings: Settings,
    factory: SessionFactory,
    authorization: str | None,
    *,
    touch: bool = True,
) -> MobileOperatorIdentity | None:
    if not settings.auth_enabled:
        return None
    claims = credential_from_authorization(
        settings,
        authorization,
        expected_kind="access",
    )
    if claims is None:
        return None
    with factory() as session:
        device = session.get(MobileOperatorDevice, claims.device_id)
        if (
            device is None
            or device.revoked_at is not None
            or device.credential_generation != claims.generation
        ):
            return None
        if touch:
            device.last_seen_at = utc_now()
            session.commit()
        return MobileOperatorIdentity(
            username="operator",
            device_id=device.id,
            credential_generation=device.credential_generation,
        )


def load_mobile_device_for_update(
    session: Session,
    device_id: uuid.UUID,
) -> MobileOperatorDevice | None:
    """Load one device row under a DB row lock where the backend supports it."""
    return session.scalar(
        select(MobileOperatorDevice)
        .where(MobileOperatorDevice.id == device_id)
        .with_for_update()
    )
