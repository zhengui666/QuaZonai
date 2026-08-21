"""Write-only credential management for local human operators."""

from __future__ import annotations

from typing import Any
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, Request
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import delete, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from quantfoundry.api.dependencies import get_session
from quantfoundry.crypto import EncryptedSecret, decrypt_secret, encrypt_secret
from quantfoundry.db.models import CredentialSecret, CredentialSet, PluginRelease
from quantfoundry.errors import QfError
from quantfoundry.events import append_event
from quantfoundry.schema_validation import validate_schema_value
from quantfoundry.settings import Settings, SettingsError

router = APIRouter(prefix="/api/v1", tags=["credentials"])


class CredentialWrite(BaseModel):
    model_config = ConfigDict(extra="forbid")

    plugin_release_id: UUID
    name: str = Field(min_length=1, max_length=200)
    public_config: dict[str, Any] = Field(default_factory=dict)
    secrets: dict[str, str] = Field(default_factory=dict)


class CredentialView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    plugin_release_id: UUID
    name: str
    public_config: dict[str, Any]
    configured_secrets: dict[str, bool]


def _release(session: Session, release_id: UUID) -> PluginRelease:
    release = session.get(PluginRelease, release_id)
    if release is None:
        raise QfError("PLUGIN_UNKNOWN", "Plugin release does not exist.", 404)
    if release.state not in {"STAGED", "ACTIVE", "DRAINING", "INACTIVE"}:
        raise QfError(
            "PLUGIN_INVALID_STATE",
            "Credentials can only bind to a validated plugin release.",
            409,
            {"state": release.state},
        )
    return release


def _secret_names(session: Session, credential_set_id: UUID) -> set[str]:
    return set(
        session.scalars(
            select(CredentialSecret.field_name).where(
                CredentialSecret.credential_set_id == credential_set_id
            )
        )
    )


def _view(session: Session, item: CredentialSet) -> CredentialView:
    release = session.get(PluginRelease, item.plugin_release_id)
    if release is None:
        raise QfError("PLUGIN_UNKNOWN", "Plugin release does not exist.", 404)
    schema = release.descriptor_snapshot.get("secret_config_schema", {})
    properties = schema.get("properties", {}) if isinstance(schema, dict) else {}
    configured = _secret_names(session, item.id)
    return CredentialView(
        id=item.id,
        plugin_release_id=item.plugin_release_id,
        name=item.name,
        public_config=item.public_config,
        configured_secrets={name: name in configured for name in sorted(properties)},
    )


def _validate_payload(release: PluginRelease, payload: CredentialWrite) -> None:
    snapshot = release.descriptor_snapshot
    validate_schema_value(
        payload.public_config,
        snapshot.get("public_config_schema", {"type": "object"}),
    )
    validate_schema_value(
        payload.secrets,
        snapshot.get("secret_config_schema", {"type": "object"}),
    )
    required_names = set(snapshot.get("required_secret_names", []))
    missing = sorted(required_names - set(payload.secrets))
    if missing:
        raise QfError(
            "CREDENTIAL_INVALID",
            "Required secret fields are missing.",
            422,
            {"missing": missing},
        )


def _master_key(settings: Settings) -> bytes:
    try:
        return settings.master_key_bytes()
    except SettingsError as exc:
        raise QfError(
            "CREDENTIAL_KEY_UNAVAILABLE",
            "The credential master key is unavailable or invalid.",
            503,
        ) from exc


@router.get("/credential-sets", response_model=list[CredentialView])
def list_credentials(session: Session = Depends(get_session)) -> list[CredentialView]:
    items = list(session.scalars(select(CredentialSet).order_by(CredentialSet.name.asc())))
    return [_view(session, item) for item in items]


@router.get("/credential-sets/{credential_id}", response_model=CredentialView)
def show_credential(
    credential_id: UUID,
    session: Session = Depends(get_session),
) -> CredentialView:
    item = session.get(CredentialSet, credential_id)
    if item is None:
        raise QfError("CREDENTIAL_UNKNOWN", "Credential set does not exist.", 404)
    return _view(session, item)


@router.post("/credential-sets", response_model=CredentialView, status_code=201)
def create_credential(
    payload: CredentialWrite,
    request: Request,
    session: Session = Depends(get_session),
) -> CredentialView:
    settings: Settings = request.app.state.settings
    master_key = _master_key(settings)
    credential_id = uuid4()
    try:
        with session.begin():
            release = _release(session, payload.plugin_release_id)
            _validate_payload(release, payload)
            item = CredentialSet(
                id=credential_id,
                plugin_release_id=payload.plugin_release_id,
                name=payload.name.strip(),
                public_config=payload.public_config,
            )
            session.add(item)
            for field_name, value in payload.secrets.items():
                encrypted = encrypt_secret(
                    value,
                    master_key=master_key,
                    credential_set_id=credential_id,
                    plugin_release_id=payload.plugin_release_id,
                    field_name=field_name,
                )
                session.add(
                    CredentialSecret(
                        credential_set_id=credential_id,
                        field_name=field_name,
                        ciphertext=encrypted.ciphertext,
                        nonce=encrypted.nonce,
                        key_version=encrypted.key_version,
                    )
                )
            append_event(
                session,
                kind="CREDENTIAL_SET_CREATED",
                aggregate_type="credential_set",
                aggregate_id=credential_id,
                payload={"plugin_release_id": str(payload.plugin_release_id)},
                actor_kind="LOCAL_OPERATOR",
            )
    except IntegrityError as exc:
        session.rollback()
        raise QfError(
            "CREDENTIAL_INVALID",
            "A credential set with this name already exists for the plugin release.",
            409,
        ) from exc
    return _view(session, item)


@router.put("/credential-sets/{credential_id}", response_model=CredentialView)
def replace_credential(
    credential_id: UUID,
    payload: CredentialWrite,
    request: Request,
    session: Session = Depends(get_session),
) -> CredentialView:
    settings: Settings = request.app.state.settings
    master_key = _master_key(settings)
    with session.begin():
        item = session.execute(
            select(CredentialSet).where(CredentialSet.id == credential_id).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("CREDENTIAL_UNKNOWN", "Credential set does not exist.", 404)
        if item.plugin_release_id != payload.plugin_release_id:
            raise QfError(
                "CREDENTIAL_INVALID",
                "A credential set cannot move to another plugin release.",
                409,
            )
        release = _release(session, item.plugin_release_id)
        _validate_payload(release, payload)
        item.name = payload.name.strip()
        item.public_config = payload.public_config
        session.execute(
            delete(CredentialSecret).where(
                CredentialSecret.credential_set_id == credential_id
            )
        )
        for field_name, value in payload.secrets.items():
            encrypted = encrypt_secret(
                value,
                master_key=master_key,
                credential_set_id=credential_id,
                plugin_release_id=item.plugin_release_id,
                field_name=field_name,
            )
            session.add(
                CredentialSecret(
                    credential_set_id=credential_id,
                    field_name=field_name,
                    ciphertext=encrypted.ciphertext,
                    nonce=encrypted.nonce,
                    key_version=encrypted.key_version,
                )
            )
        append_event(
            session,
            kind="CREDENTIAL_SET_REPLACED",
            aggregate_type="credential_set",
            aggregate_id=credential_id,
            payload={"plugin_release_id": str(item.plugin_release_id)},
            actor_kind="LOCAL_OPERATOR",
        )
    return _view(session, item)


def decrypt_credential_secrets(
    session: Session,
    settings: Settings,
    credential_set: CredentialSet,
) -> dict[str, str]:
    master_key = _master_key(settings)
    rows = list(
        session.scalars(
            select(CredentialSecret).where(
                CredentialSecret.credential_set_id == credential_set.id
            )
        )
    )
    return {
        item.field_name: decrypt_secret(
            EncryptedSecret(
                ciphertext=item.ciphertext,
                nonce=item.nonce,
                key_version=item.key_version,
            ),
            master_key=master_key,
            credential_set_id=credential_set.id,
            plugin_release_id=credential_set.plugin_release_id,
            field_name=item.field_name,
        )
        for item in rows
    }
