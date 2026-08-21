from __future__ import annotations

from uuid import uuid4

import pytest

from quantfoundry.crypto import EncryptedSecret, decrypt_secret, encrypt_secret
from quantfoundry.errors import QfError


def test_secret_is_bound_to_credential_release_and_field() -> None:
    key = b"m" * 32
    credential_id = uuid4()
    release_id = uuid4()
    encrypted = encrypt_secret(
        "secret-value",
        master_key=key,
        credential_set_id=credential_id,
        plugin_release_id=release_id,
        field_name="api_secret",
    )

    assert decrypt_secret(
        encrypted,
        master_key=key,
        credential_set_id=credential_id,
        plugin_release_id=release_id,
        field_name="api_secret",
    ) == "secret-value"

    with pytest.raises(QfError):
        decrypt_secret(
            EncryptedSecret(
                ciphertext=encrypted.ciphertext,
                nonce=encrypted.nonce,
                key_version=encrypted.key_version,
            ),
            master_key=key,
            credential_set_id=credential_id,
            plugin_release_id=release_id,
            field_name="another_field",
        )
