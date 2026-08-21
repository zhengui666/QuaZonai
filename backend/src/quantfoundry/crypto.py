"""Authenticated credential encryption owned by the local control plane."""

from __future__ import annotations

import secrets
from dataclasses import dataclass
from uuid import UUID

from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from quantfoundry.errors import QfError

NONCE_BYTES = 12
KEY_VERSION = 1


@dataclass(frozen=True, slots=True)
class EncryptedSecret:
    ciphertext: bytes
    nonce: bytes
    key_version: int = KEY_VERSION


def _aad(
    *,
    credential_set_id: UUID,
    plugin_release_id: UUID,
    field_name: str,
    key_version: int,
) -> bytes:
    return (
        f"quantfoundry|credential_set={credential_set_id}|"
        f"plugin_release={plugin_release_id}|field={field_name}|key_version={key_version}"
    ).encode("utf-8")


def encrypt_secret(
    plaintext: str,
    *,
    master_key: bytes,
    credential_set_id: UUID,
    plugin_release_id: UUID,
    field_name: str,
    key_version: int = KEY_VERSION,
) -> EncryptedSecret:
    if not field_name or not field_name.strip():
        raise QfError("CREDENTIAL_INVALID", "Secret field name must be non-empty.", 422)
    encoded = plaintext.encode("utf-8")
    if not encoded:
        raise QfError("CREDENTIAL_INVALID", "Secret values must be non-empty.", 422)
    nonce = secrets.token_bytes(NONCE_BYTES)
    ciphertext = AESGCM(master_key).encrypt(
        nonce,
        encoded,
        _aad(
            credential_set_id=credential_set_id,
            plugin_release_id=plugin_release_id,
            field_name=field_name,
            key_version=key_version,
        ),
    )
    return EncryptedSecret(ciphertext=ciphertext, nonce=nonce, key_version=key_version)


def decrypt_secret(
    encrypted: EncryptedSecret,
    *,
    master_key: bytes,
    credential_set_id: UUID,
    plugin_release_id: UUID,
    field_name: str,
) -> str:
    try:
        plaintext = AESGCM(master_key).decrypt(
            encrypted.nonce,
            encrypted.ciphertext,
            _aad(
                credential_set_id=credential_set_id,
                plugin_release_id=plugin_release_id,
                field_name=field_name,
                key_version=encrypted.key_version,
            ),
        )
    except Exception as exc:  # cryptography intentionally hides authentication detail
        raise QfError(
            "CREDENTIAL_INVALID",
            "Credential secret could not be authenticated for this field and release.",
            422,
        ) from exc
    return plaintext.decode("utf-8")
