"""Durable, encrypted storage for the installation's canonical TOTP binding."""

from __future__ import annotations

import logging
import secrets
import uuid
from datetime import UTC, datetime

from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from crypto import EncryptedSecret, decrypt_bound_secret, encrypt_bound_secret
from db.auth_models import OperatorAuthConfiguration, OperatorAuthInitialization
from db.session import SessionFactory
from settings import Settings, SettingsError, normalize_totp_secret

logger = logging.getLogger(__name__)

SYSTEM_SCOPE = "SYSTEM"
OPERATOR_TOTP_KEY_VERSION = 1


def _secret_aad(binding_id: uuid.UUID, key_version: int) -> bytes:
    return (
        f"quazonai|operator_auth_configuration={binding_id}|"
        f"field=totp_secret|key_version={key_version}"
    ).encode("utf-8")


def _encrypted_binding(binding: OperatorAuthConfiguration) -> EncryptedSecret:
    if (
        not binding.totp_secret_ciphertext
        or not binding.totp_secret_nonce
        or binding.totp_secret_key_version <= 0
        or binding.bound_at is None
    ):
        raise SettingsError("The persisted Operator TOTP binding is incomplete")
    return EncryptedSecret(
        ciphertext=binding.totp_secret_ciphertext,
        nonce=binding.totp_secret_nonce,
        key_version=binding.totp_secret_key_version,
    )


def get_binding(session: Session) -> OperatorAuthConfiguration | None:
    """Return the unique SYSTEM binding, including a possibly malformed row."""
    return session.scalar(
        select(OperatorAuthConfiguration).where(
            OperatorAuthConfiguration.scope == SYSTEM_SCOPE
        )
    )


def get_initialization_marker(session: Session) -> OperatorAuthInitialization | None:
    """Return the durable marker left after the first successful binding."""
    return session.scalar(
        select(OperatorAuthInitialization).where(
            OperatorAuthInitialization.scope == SYSTEM_SCOPE
        )
    )


def _ensure_initialization_marker(
    session: Session,
    *,
    initialized_at: datetime | None = None,
) -> None:
    if get_initialization_marker(session) is not None:
        return
    session.add(
        OperatorAuthInitialization(
            id=uuid.uuid4(),
            scope=SYSTEM_SCOPE,
            initialized_at=initialized_at or datetime.now(UTC),
        )
    )
    session.flush()


def _decrypt_binding(
    binding: OperatorAuthConfiguration,
    settings: Settings,
) -> str:
    try:
        encrypted = _encrypted_binding(binding)
        secret = decrypt_bound_secret(
            encrypted,
            master_key=settings.master_key_bytes(),
            associated_data=_secret_aad(binding.id, encrypted.key_version),
        )
        return normalize_totp_secret(secret)
    except (SettingsError, UnicodeError, ValueError) as exc:
        raise SettingsError("The persisted Operator TOTP binding is invalid") from exc
    except Exception as exc:  # cryptography/DB data errors fail closed alike
        raise SettingsError("The persisted Operator TOTP binding is invalid") from exc


def load_canonical_secret(session: Session, settings: Settings) -> str | None:
    """Load and authenticate the canonical secret; never fall back to the env value."""
    binding = get_binding(session)
    if binding is None:
        if get_initialization_marker(session) is not None:
            raise SettingsError(
                "The Operator TOTP binding is missing after initialization"
            )
        return None
    try:
        return _decrypt_binding(binding, settings)
    except SettingsError as exc:
        raise SettingsError("The persisted Operator TOTP binding is invalid") from exc


def create_binding_if_absent(
    session: Session,
    settings: Settings,
    secret: str,
    *,
    bound_at: datetime | None = None,
) -> OperatorAuthConfiguration:
    """Insert one binding; the database unique constraint arbitrates first claim."""
    normalized = normalize_totp_secret(secret)
    binding_id = uuid.uuid4()
    encrypted = encrypt_bound_secret(
        normalized,
        master_key=settings.master_key_bytes(),
        associated_data=_secret_aad(binding_id, OPERATOR_TOTP_KEY_VERSION),
        key_version=OPERATOR_TOTP_KEY_VERSION,
    )
    binding = OperatorAuthConfiguration(
        id=binding_id,
        scope=SYSTEM_SCOPE,
        totp_secret_ciphertext=encrypted.ciphertext,
        totp_secret_nonce=encrypted.nonce,
        totp_secret_key_version=encrypted.key_version,
        bound_at=bound_at or datetime.now(UTC),
    )
    session.add(binding)
    # Flush inside the caller's transaction so an IntegrityError is raised before
    # any session/cookie is issued.  The caller decides how to present the race.
    session.flush()
    _ensure_initialization_marker(session, initialized_at=binding.bound_at)
    return binding


def _same_as_legacy(
    binding: OperatorAuthConfiguration,
    settings: Settings,
    legacy_secret: str,
) -> bool:
    canonical = _decrypt_binding(binding, settings)
    return secrets.compare_digest(canonical.encode("ascii"), legacy_secret.encode("ascii"))


def initialize_operator_auth(
    factory: SessionFactory,
    settings: Settings,
) -> str | None:
    """Validate the durable auth state before the app starts serving requests.

    ``None`` means setup is required.  A non-None return is the canonical secret
    for the in-memory auth runtime.  Legacy environment import is deliberately
    performed here, after the database is available, rather than in Settings.
    """
    if not settings.auth_enabled:
        return None

    legacy = settings.operator_totp_secret
    if legacy is not None:
        legacy = normalize_totp_secret(legacy)

    try:
        with factory.begin() as session:
            binding = get_binding(session)
            marker = get_initialization_marker(session)
            if binding is None and marker is not None:
                raise SettingsError(
                    "The Operator TOTP binding is missing after initialization"
                )
            if binding is None and legacy is not None:
                binding = create_binding_if_absent(session, settings, legacy)
                logger.warning(
                    "Imported legacy Operator TOTP configuration into the durable binding; "
                    "remove QUAZONAI_AUTH_TOTP_SECRET after verifying the upgrade."
                )
            elif binding is not None and legacy is not None:
                _ensure_initialization_marker(session, initialized_at=binding.bound_at)
                if not _same_as_legacy(binding, settings, legacy):
                    raise SettingsError(
                        "The durable Operator TOTP binding conflicts with "
                        "QUAZONAI_AUTH_TOTP_SECRET"
                    )
                logger.warning(
                    "QUAZONAI_AUTH_TOTP_SECRET is deprecated; the durable Operator TOTP "
                    "binding remains canonical."
                )
            elif binding is not None:
                _ensure_initialization_marker(session, initialized_at=binding.bound_at)
            return _decrypt_binding(binding, settings) if binding is not None else None
    except IntegrityError:
        # Another API process won a simultaneous legacy import.  Reload the row
        # in a fresh transaction; an identical legacy secret is safe to continue,
        # while a mismatch remains an explicit fail-closed configuration error.
        with factory() as session:
            binding = get_binding(session)
            if binding is None:
                raise SettingsError(
                    "Operator TOTP binding could not be initialized"
                ) from None
            canonical = _decrypt_binding(binding, settings)
            if legacy is not None and not secrets.compare_digest(
                canonical.encode("ascii"), legacy.encode("ascii")
            ):
                raise SettingsError(
                    "The durable Operator TOTP binding conflicts with "
                    "QUAZONAI_AUTH_TOTP_SECRET"
                ) from None
            return canonical
