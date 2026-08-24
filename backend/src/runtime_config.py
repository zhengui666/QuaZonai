"""Operator-managed Codex and worker runtime configuration."""

from __future__ import annotations

from dataclasses import replace
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from crypto import EncryptedSecret, decrypt_bound_secret, encrypt_bound_secret
from db.models import RuntimeConfiguration
from errors import QfError
from settings import Settings, SettingsError

RUNTIME_SCOPE = "SYSTEM"
CODEX_API_KEY_FIELD = "codex_api_key"


def _api_key_aad(configuration_id: UUID, key_version: int) -> bytes:
    return (
        f"quazonai|runtime_configuration={configuration_id}|"
        f"field={CODEX_API_KEY_FIELD}|key_version={key_version}"
    ).encode("utf-8")


def get_runtime_configuration(
    session: Session,
    *,
    for_update: bool = False,
) -> RuntimeConfiguration | None:
    statement = select(RuntimeConfiguration).where(RuntimeConfiguration.scope == RUNTIME_SCOPE)
    if for_update:
        statement = statement.with_for_update()
    return session.scalar(statement)


def _new_runtime_configuration(
    session: Session,
    base_settings: Settings,
) -> RuntimeConfiguration:
    item = RuntimeConfiguration(
        scope=RUNTIME_SCOPE,
        revision=1,
        codex_model=None,
        codex_base_url=None,
        max_plugin_wheel_bytes=base_settings.max_plugin_wheel_bytes,
        plugin_validation_timeout_seconds=base_settings.plugin_validation_timeout_seconds,
        bundle_build_timeout_seconds=base_settings.bundle_build_timeout_seconds,
        plugin_job_timeout_seconds=base_settings.plugin_job_timeout_seconds,
        mission_job_timeout_seconds=base_settings.mission_job_timeout_seconds,
        job_poll_seconds=base_settings.job_poll_seconds,
        job_lease_seconds=base_settings.job_lease_seconds,
    )
    session.add(item)
    session.flush()
    return item


def codex_api_key_configured(item: RuntimeConfiguration | None) -> bool:
    if item is None:
        return False
    parts = (
        item.codex_api_key_ciphertext,
        item.codex_api_key_nonce,
        item.codex_api_key_key_version,
    )
    return all(value is not None for value in parts)


def _decrypt_codex_api_key(item: RuntimeConfiguration, base_settings: Settings) -> str | None:
    if not codex_api_key_configured(item):
        if any(
            value is not None
            for value in (
                item.codex_api_key_ciphertext,
                item.codex_api_key_nonce,
                item.codex_api_key_key_version,
            )
        ):
            raise SettingsError("Stored Codex API key is incomplete")
        return None
    assert item.codex_api_key_ciphertext is not None
    assert item.codex_api_key_nonce is not None
    assert item.codex_api_key_key_version is not None
    encrypted = EncryptedSecret(
        ciphertext=item.codex_api_key_ciphertext,
        nonce=item.codex_api_key_nonce,
        key_version=item.codex_api_key_key_version,
    )
    return decrypt_bound_secret(
        encrypted,
        master_key=base_settings.master_key_bytes(),
        associated_data=_api_key_aad(item.id, encrypted.key_version),
    )


def effective_settings(session: Session, base_settings: Settings) -> Settings:
    """Overlay the latest persisted runtime configuration on bootstrap settings."""
    item = get_runtime_configuration(session)
    if item is None:
        return base_settings
    return replace(
        base_settings,
        codex_model=item.codex_model,
        codex_base_url=item.codex_base_url,
        codex_api_key=_decrypt_codex_api_key(item, base_settings),
        max_plugin_wheel_bytes=item.max_plugin_wheel_bytes,
        plugin_validation_timeout_seconds=item.plugin_validation_timeout_seconds,
        bundle_build_timeout_seconds=item.bundle_build_timeout_seconds,
        plugin_job_timeout_seconds=item.plugin_job_timeout_seconds,
        mission_job_timeout_seconds=item.mission_job_timeout_seconds,
        job_poll_seconds=item.job_poll_seconds,
        job_lease_seconds=item.job_lease_seconds,
    )


def load_effective_settings(base_settings: Settings) -> Settings:
    """Load runtime configuration from the configured database for a short-lived child."""
    from db.session import create_database_engine, create_session_factory

    engine = create_database_engine(base_settings)
    try:
        factory = create_session_factory(engine)
        with factory() as session:
            return effective_settings(session, base_settings)
    finally:
        engine.dispose()


def update_runtime_configuration(
    session: Session,
    base_settings: Settings,
    *,
    expected_revision: int,
    codex_model: str | None,
    codex_base_url: str | None,
    codex_api_key: str | None,
    clear_codex_api_key: bool,
    max_plugin_wheel_bytes: int,
    plugin_validation_timeout_seconds: int,
    bundle_build_timeout_seconds: int,
    plugin_job_timeout_seconds: int,
    mission_job_timeout_seconds: int,
    job_poll_seconds: float,
    job_lease_seconds: int,
) -> RuntimeConfiguration:
    item = get_runtime_configuration(session, for_update=True)
    if item is None:
        if expected_revision != 0:
            raise QfError(
                "RUNTIME_CONFIGURATION_STALE",
                "Runtime configuration has changed since it was loaded.",
                409,
                {"expected_revision": expected_revision, "actual_revision": 0},
            )
        item = _new_runtime_configuration(session, base_settings)
    else:
        if expected_revision != item.revision:
            raise QfError(
                "RUNTIME_CONFIGURATION_STALE",
                "Runtime configuration has changed since it was loaded.",
                409,
                {"expected_revision": expected_revision, "actual_revision": item.revision},
            )
        item.revision += 1

    next_base_url = (
        codex_base_url.strip() if codex_base_url and codex_base_url.strip() else None
    )
    replacement_key = codex_api_key.strip() if codex_api_key and codex_api_key.strip() else None
    if (
        item.codex_base_url != next_base_url
        and codex_api_key_configured(item)
        and replacement_key is None
        and not clear_codex_api_key
    ):
        raise QfError(
            "CODEX_PROVIDER_CREDENTIAL_REENTRY_REQUIRED",
            "Changing the Codex base URL requires re-entering or clearing the stored API key.",
            409,
        )

    item.codex_model = codex_model.strip() if codex_model and codex_model.strip() else None
    item.codex_base_url = next_base_url
    item.max_plugin_wheel_bytes = max_plugin_wheel_bytes
    item.plugin_validation_timeout_seconds = plugin_validation_timeout_seconds
    item.bundle_build_timeout_seconds = bundle_build_timeout_seconds
    item.plugin_job_timeout_seconds = plugin_job_timeout_seconds
    item.mission_job_timeout_seconds = mission_job_timeout_seconds
    item.job_poll_seconds = job_poll_seconds
    item.job_lease_seconds = job_lease_seconds

    if clear_codex_api_key:
        item.codex_api_key_ciphertext = None
        item.codex_api_key_nonce = None
        item.codex_api_key_key_version = None
    elif replacement_key is not None:
        encrypted = encrypt_bound_secret(
            replacement_key,
            master_key=base_settings.master_key_bytes(),
            associated_data=_api_key_aad(item.id, 1),
        )
        item.codex_api_key_ciphertext = encrypted.ciphertext
        item.codex_api_key_nonce = encrypted.nonce
        item.codex_api_key_key_version = encrypted.key_version

    session.flush()
    return item
