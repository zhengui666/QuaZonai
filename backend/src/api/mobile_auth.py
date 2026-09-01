"""Bootstrap and TOTP-only native operator authentication endpoints."""

from __future__ import annotations

import uuid
from datetime import timedelta
from typing import Literal

from fastapi import APIRouter, Request, Response, status
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import select

from db.auth_models import MobileOperatorDevice
from errors import QfError
from mobile_auth import (
    MOBILE_ACCESS_TTL_SECONDS,
    MobileOperatorIdentity,
    credential_from_authorization,
    issue_mobile_credential,
    load_mobile_device_for_update,
    normalize_utc,
    utc_now,
)
from operator_auth import OperatorAuthRuntime, login_source_key
from operator_auth_store import load_canonical_secret
from quazonai import __version__
from settings import Settings
from totp_core import verify_totp_once

router = APIRouter(prefix="/api/v1", tags=["native-auth"])
OPERATOR_CLIENT_CAPABILITY_EPOCH = 1
MINIMUM_IOS_CAPABILITY_EPOCH = 1
MINIMUM_IOS_APP_VERSION = "1.0.0"


class ClientBootstrapView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    server_version: str
    auth_enabled: bool
    operator_client_capability_epoch: int
    minimum_ios_capability_epoch: int
    minimum_ios_app_version: str


class MobileLoginInput(BaseModel):
    model_config = ConfigDict(extra="forbid")

    totp_code: str = Field(min_length=6, max_length=32)
    installation_id: uuid.UUID
    device_name: str = Field(min_length=1, max_length=120)
    device_family: Literal["IPHONE", "IPAD"]
    os_version: str = Field(min_length=1, max_length=80)
    app_version: str = Field(min_length=1, max_length=80)
    app_build: str = Field(min_length=1, max_length=80)
    trust_device: bool = False


class MobileDeviceView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: uuid.UUID
    installation_id: uuid.UUID
    display_name: str
    device_family: str
    credential_generation: int
    created_at: str
    last_seen_at: str | None
    refresh_expires_at: str | None
    revoked_at: str | None
    client_version: str
    app_build: str
    os_version: str


class MobileSessionView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    authenticated: bool
    auth_enabled: bool
    operator_subject: Literal["operator"] = "operator"
    device: MobileDeviceView | None = None


class MobileTokenView(MobileSessionView):
    access_token: str | None = None
    access_expires_in: int = MOBILE_ACCESS_TTL_SECONDS
    refresh_credential: str | None = None
    refresh_expires_at: str | None = None


def _view(device: MobileOperatorDevice) -> MobileDeviceView:
    return MobileDeviceView(
        id=device.id,
        installation_id=uuid.UUID(device.installation_id),
        display_name=device.display_name,
        device_family=device.device_family,
        credential_generation=device.credential_generation,
        created_at=device.created_at.isoformat(),
        last_seen_at=device.last_seen_at.isoformat() if device.last_seen_at else None,
        refresh_expires_at=(
            device.refresh_expires_at.isoformat() if device.refresh_expires_at else None
        ),
        revoked_at=device.revoked_at.isoformat() if device.revoked_at else None,
        client_version=device.client_version,
        app_build=device.app_build,
        os_version=device.os_version,
    )


def _no_store(response: Response) -> None:
    response.headers["Cache-Control"] = "no-store"
    response.headers["Pragma"] = "no-cache"


def _invalid() -> QfError:
    return QfError("AUTH_INVALID", "Invalid operator credentials.", 401)


def _require_mobile_identity(request: Request) -> MobileOperatorIdentity:
    identity = getattr(request.state, "operator", None)
    if not isinstance(identity, MobileOperatorIdentity) or identity.device_id is None:
        raise QfError("AUTH_REQUIRED", "A native operator session is required.", 401)
    return identity


def _token_response(
    settings: Settings,
    device: MobileOperatorDevice,
    *,
    trust_device: bool,
) -> MobileTokenView:
    now = utc_now()
    access_expiry = now + timedelta(seconds=MOBILE_ACCESS_TTL_SECONDS)
    access = issue_mobile_credential(
        settings,
        kind="access",
        device_id=device.id,
        generation=device.credential_generation,
        expires_at=access_expiry,
    )
    refresh: str | None = None
    if trust_device and device.refresh_expires_at is not None:
        refresh = issue_mobile_credential(
            settings,
            kind="refresh",
            device_id=device.id,
            generation=device.credential_generation,
            expires_at=device.refresh_expires_at,
        )
    return MobileTokenView(
        authenticated=True,
        auth_enabled=True,
        device=_view(device),
        access_token=access,
        refresh_credential=refresh,
        refresh_expires_at=(
            device.refresh_expires_at.isoformat() if device.refresh_expires_at else None
        ),
    )


@router.get("/client/bootstrap", response_model=ClientBootstrapView)
def bootstrap(request: Request, response: Response) -> ClientBootstrapView:
    _no_store(response)
    settings: Settings = request.app.state.settings
    return ClientBootstrapView(
        server_version=__version__,
        auth_enabled=settings.auth_enabled,
        operator_client_capability_epoch=OPERATOR_CLIENT_CAPABILITY_EPOCH,
        minimum_ios_capability_epoch=MINIMUM_IOS_CAPABILITY_EPOCH,
        minimum_ios_app_version=MINIMUM_IOS_APP_VERSION,
    )


@router.post("/auth/mobile/login", response_model=MobileTokenView)
def mobile_login(
    payload: MobileLoginInput,
    request: Request,
    response: Response,
) -> MobileTokenView:
    _no_store(response)
    settings: Settings = request.app.state.settings
    if not settings.auth_enabled:
        return MobileTokenView(authenticated=True, auth_enabled=False)

    runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime
    try:
        with request.app.state.session_factory() as session:
            canonical_secret = load_canonical_secret(session, settings)
    except Exception as exc:
        raise QfError(
            "AUTH_CONFIGURATION_INVALID",
            "The Operator authentication configuration is unavailable.",
            500,
        ) from exc
    if canonical_secret is None:
        raise QfError(
            "AUTH_SETUP_REQUIRED",
            "Operator authenticator setup is required.",
            409,
        )
    runtime.set_totp_secret(canonical_secret)
    source = login_source_key(request, settings)
    if not runtime.login_limiter.allow_attempt(source):
        raise _invalid()
    if not verify_totp_once(settings, runtime, payload.totp_code):
        runtime.login_limiter.record_failure(source)
        raise _invalid()

    now = utc_now()
    factory = request.app.state.session_factory
    with factory() as session:
        device = session.scalar(
            select(MobileOperatorDevice)
            .where(MobileOperatorDevice.installation_id == str(payload.installation_id))
            .with_for_update()
        )
        if device is None:
            device = MobileOperatorDevice(
                installation_id=str(payload.installation_id),
                display_name=payload.device_name,
                device_family=payload.device_family,
                credential_generation=1,
                last_seen_at=now,
                refresh_expires_at=None,
                client_version=payload.app_version,
                app_build=payload.app_build,
                os_version=payload.os_version,
            )
            session.add(device)
        else:
            device.credential_generation += 1
            device.display_name = payload.device_name
            device.device_family = payload.device_family
            device.revoked_at = None
            device.last_seen_at = now
            device.client_version = payload.app_version
            device.app_build = payload.app_build
            device.os_version = payload.os_version
        device.refresh_expires_at = (
            now + timedelta(days=settings.auth_trusted_browser_ttl_days)
            if payload.trust_device
            else None
        )
        session.commit()
        session.refresh(device)
        result = _token_response(settings, device, trust_device=payload.trust_device)

    runtime.login_limiter.record_success(source)
    return result


@router.post("/auth/mobile/refresh", response_model=MobileTokenView)
def mobile_refresh(request: Request, response: Response) -> MobileTokenView:
    _no_store(response)
    settings: Settings = request.app.state.settings
    if not settings.auth_enabled:
        return MobileTokenView(authenticated=True, auth_enabled=False)
    claims = credential_from_authorization(
        settings,
        request.headers.get("authorization"),
        expected_kind="refresh",
    )
    if claims is None:
        raise _invalid()

    factory = request.app.state.session_factory
    now = utc_now()
    with factory() as session:
        device = load_mobile_device_for_update(session, claims.device_id)
        refresh_expires_at = (
            normalize_utc(device.refresh_expires_at)
            if device is not None and device.refresh_expires_at is not None
            else None
        )
        if (
            device is None
            or device.revoked_at is not None
            or device.credential_generation != claims.generation
            or refresh_expires_at is None
            or refresh_expires_at <= now
        ):
            raise _invalid()
        device.credential_generation += 1
        device.last_seen_at = now
        device.refresh_expires_at = now + timedelta(days=settings.auth_trusted_browser_ttl_days)
        session.commit()
        session.refresh(device)
        return _token_response(settings, device, trust_device=True)


@router.post("/auth/mobile/logout", status_code=status.HTTP_204_NO_CONTENT)
def mobile_logout(request: Request, response: Response) -> None:
    _no_store(response)
    settings: Settings = request.app.state.settings
    identity = getattr(request.state, "operator", None)
    if isinstance(identity, MobileOperatorIdentity):
        device_id = identity.device_id
        credential_generation = identity.credential_generation
    else:
        # Direct-access mode still has to revoke a native credential when the
        # client presents one. Otherwise a copied refresh credential could be
        # re-enabled after authentication is turned back on.
        claims = credential_from_authorization(
            settings,
            request.headers.get("authorization"),
            expected_kind="access",
        ) or credential_from_authorization(
            settings,
            request.headers.get("authorization"),
            expected_kind="refresh",
        )
        if claims is None:
            if settings.auth_enabled:
                _require_mobile_identity(request)
            return
        device_id = claims.device_id
        credential_generation = claims.generation
    factory = request.app.state.session_factory
    with factory() as session:
        device = load_mobile_device_for_update(session, device_id)
        if device is not None and device.credential_generation == credential_generation:
            device.credential_generation += 1
            device.revoked_at = utc_now()
            device.refresh_expires_at = None
            session.commit()


@router.get("/auth/mobile/session", response_model=MobileSessionView)
def mobile_session(request: Request, response: Response) -> MobileSessionView:
    _no_store(response)
    settings: Settings = request.app.state.settings
    if not settings.auth_enabled:
        return MobileSessionView(authenticated=True, auth_enabled=False)
    identity = _require_mobile_identity(request)
    factory = request.app.state.session_factory
    with factory() as session:
        device = session.get(MobileOperatorDevice, identity.device_id)
        if device is None or device.revoked_at is not None:
            raise QfError("AUTH_REQUIRED", "Operator authentication is required.", 401)
        return MobileSessionView(authenticated=True, auth_enabled=True, device=_view(device))


@router.get("/auth/mobile/devices", response_model=list[MobileDeviceView])
def mobile_devices(request: Request, response: Response) -> list[MobileDeviceView]:
    _no_store(response)
    factory = request.app.state.session_factory
    with factory() as session:
        devices = list(
            session.scalars(select(MobileOperatorDevice).order_by(MobileOperatorDevice.created_at))
        )
        return [_view(device) for device in devices]


@router.post(
    "/auth/mobile/devices/{device_id}/revoke",
    status_code=status.HTTP_204_NO_CONTENT,
)
def revoke_mobile_device(device_id: uuid.UUID, request: Request, response: Response) -> None:
    _no_store(response)
    factory = request.app.state.session_factory
    with factory() as session:
        device = load_mobile_device_for_update(session, device_id)
        if device is None:
            raise QfError("MOBILE_DEVICE_NOT_FOUND", "Mobile device was not found.", 404)
        if device.revoked_at is None:
            device.credential_generation += 1
            device.revoked_at = utc_now()
            device.refresh_expires_at = None
            session.commit()
