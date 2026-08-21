"""Internal Core API used only by the optional authenticated MCP Gateway."""

from __future__ import annotations

import json
import os
import shutil
from collections.abc import Iterator
from datetime import UTC, datetime, timedelta
from pathlib import Path
from secrets import compare_digest
from typing import Any, Literal
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, Request, Response
from fastapi.responses import StreamingResponse
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import select, text
from sqlalchemy.orm import Session

from api.dependencies import get_session
from db.models import (
    AgentArtifact,
    AgentImpactToken,
    McpTaskBinding,
    OperationReceipt,
)
from errors import QfError
from settings import Settings

router = APIRouter(prefix="/api/v1/agent", tags=["agent-internal"])
CHUNK_BYTES = 1024 * 1024


class AgentPrincipal(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)

    issuer: str
    subject: str
    client_id: str
    scopes: tuple[str, ...] = ()

    @property
    def actor_id(self) -> str:
        return json.dumps(
            [self.issuer, self.subject, self.client_id],
            separators=(",", ":"),
            ensure_ascii=False,
        )


def require_agent_gateway(request: Request) -> AgentPrincipal:
    settings: Settings = request.app.state.settings
    expected = settings.mcp_internal_token
    provided = request.headers.get("x-quazonai-internal-token")
    if expected is None:
        raise QfError(
            "MCP_GATEWAY_DISABLED",
            "The optional MCP Gateway is not enabled for this Core API.",
            503,
        )
    if provided is None or not compare_digest(provided, expected):
        raise QfError("MCP_GATEWAY_UNAUTHORIZED", "Gateway authentication failed.", 401)
    issuer = (request.headers.get("x-quazonai-agent-issuer") or "").strip()
    subject = (request.headers.get("x-quazonai-agent-subject") or "").strip()
    client_id = (request.headers.get("x-quazonai-agent-client-id") or "").strip()
    if not issuer or not client_id:
        raise QfError(
            "MCP_PRINCIPAL_INVALID",
            "Gateway requests require issuer and client identity headers.",
            401,
        )
    scopes = tuple(
        sorted(
            {
                item
                for item in (request.headers.get("x-quazonai-agent-scopes") or "").split()
                if item
            }
        )
    )
    return AgentPrincipal(
        issuer=issuer,
        subject=subject,
        client_id=client_id,
        scopes=scopes,
    )


class OperationBegin(BaseModel):
    model_config = ConfigDict(extra="forbid")

    idempotency_key: UUID
    operation_name: str = Field(min_length=1, max_length=200)
    target_type: str | None = Field(default=None, max_length=100)
    target_id: UUID | None = None
    normalized_arguments: dict[str, Any] = Field(default_factory=dict)


class OperationComplete(BaseModel):
    model_config = ConfigDict(extra="forbid")

    result: dict[str, Any] = Field(default_factory=dict)


class OperationFailure(BaseModel):
    model_config = ConfigDict(extra="forbid")

    error_code: str = Field(min_length=1, max_length=100)
    result: dict[str, Any] = Field(default_factory=dict)


class OperationReceiptView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    idempotency_key: UUID
    operation_name: str
    target_type: str | None
    target_id: UUID | None
    state: str
    result: dict[str, Any] | None
    error_code: str | None
    replay: bool = False


def _receipt_view(item: OperationReceipt, *, replay: bool = False) -> OperationReceiptView:
    return OperationReceiptView(
        id=item.id,
        idempotency_key=item.idempotency_key,
        operation_name=item.operation_name,
        target_type=item.target_type,
        target_id=item.target_id,
        state=item.state,
        result=item.result,
        error_code=item.error_code,
        replay=replay,
    )


def _lock_idempotency(session: Session, key: UUID) -> None:
    if session.bind is not None and session.bind.dialect.name == "postgresql":
        number = int.from_bytes(key.bytes[:8], byteorder="big", signed=False)
        signed = number if number < 2**63 else number - 2**64
        session.execute(text("SELECT pg_advisory_xact_lock(:key)"), {"key": signed})


@router.post("/operations/begin", response_model=OperationReceiptView)
def begin_operation(
    payload: OperationBegin,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> OperationReceiptView:
    with session.begin():
        _lock_idempotency(session, payload.idempotency_key)
        existing = session.execute(
            select(OperationReceipt)
            .where(
                OperationReceipt.actor_kind == "MCP",
                OperationReceipt.actor_id == principal.actor_id,
                OperationReceipt.idempotency_key == payload.idempotency_key,
            )
            .with_for_update()
        ).scalar_one_or_none()
        if existing is not None:
            if (
                existing.operation_name != payload.operation_name
                or existing.target_type != payload.target_type
                or existing.target_id != payload.target_id
                or existing.normalized_arguments != payload.normalized_arguments
            ):
                raise QfError(
                    "IDEMPOTENCY_KEY_REUSED",
                    "The idempotency key belongs to a different operation.",
                    409,
                )
            return _receipt_view(existing, replay=True)
        item = OperationReceipt(
            actor_kind="MCP",
            actor_id=principal.actor_id,
            idempotency_key=payload.idempotency_key,
            operation_name=payload.operation_name,
            target_type=payload.target_type,
            target_id=payload.target_id,
            normalized_arguments=payload.normalized_arguments,
            state="IN_PROGRESS",
        )
        session.add(item)
        session.flush()
    return _receipt_view(item)


def _owned_receipt(
    session: Session,
    receipt_id: UUID,
    principal: AgentPrincipal,
) -> OperationReceipt:
    item = session.execute(
        select(OperationReceipt)
        .where(
            OperationReceipt.id == receipt_id,
            OperationReceipt.actor_kind == "MCP",
            OperationReceipt.actor_id == principal.actor_id,
        )
        .with_for_update()
    ).scalar_one_or_none()
    if item is None:
        raise QfError("OPERATION_RECEIPT_UNKNOWN", "Operation receipt does not exist.", 404)
    return item


@router.get("/operations/{receipt_id}", response_model=OperationReceiptView)
def show_operation(
    receipt_id: UUID,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> OperationReceiptView:
    item = session.execute(
        select(OperationReceipt).where(
            OperationReceipt.id == receipt_id,
            OperationReceipt.actor_kind == "MCP",
            OperationReceipt.actor_id == principal.actor_id,
        )
    ).scalar_one_or_none()
    if item is None:
        raise QfError("OPERATION_RECEIPT_UNKNOWN", "Operation receipt does not exist.", 404)
    return _receipt_view(item)


@router.post("/operations/{receipt_id}/complete", response_model=OperationReceiptView)
def complete_operation(
    receipt_id: UUID,
    payload: OperationComplete,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> OperationReceiptView:
    with session.begin():
        item = _owned_receipt(session, receipt_id, principal)
        if item.state == "SUCCEEDED":
            return _receipt_view(item, replay=True)
        if item.state != "IN_PROGRESS":
            raise QfError("OPERATION_INVALID_STATE", "Operation cannot be completed.", 409)
        item.state = "SUCCEEDED"
        item.result = payload.result
        item.error_code = None
        item.completed_at = datetime.now(UTC)
    return _receipt_view(item)


@router.post("/operations/{receipt_id}/fail", response_model=OperationReceiptView)
def fail_operation(
    receipt_id: UUID,
    payload: OperationFailure,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> OperationReceiptView:
    with session.begin():
        item = _owned_receipt(session, receipt_id, principal)
        if item.state == "FAILED":
            return _receipt_view(item, replay=True)
        if item.state != "IN_PROGRESS":
            raise QfError("OPERATION_INVALID_STATE", "Operation cannot be failed.", 409)
        item.state = "FAILED"
        item.result = payload.result
        item.error_code = payload.error_code
        item.completed_at = datetime.now(UTC)
    return _receipt_view(item)


ArtifactKind = Literal["STRATEGY_SOURCE", "PLUGIN_WHEEL", "PARQUET_L2"]


class ArtifactBegin(BaseModel):
    model_config = ConfigDict(extra="forbid")

    kind: ArtifactKind
    filename: str = Field(min_length=1, max_length=255)
    size_bytes: int = Field(gt=0)


class ArtifactConsume(BaseModel):
    model_config = ConfigDict(extra="forbid")

    resource_type: str = Field(min_length=1, max_length=100)
    resource_id: UUID


class ArtifactView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    kind: str
    filename: str
    state: str
    size_declared: int
    size_received: int
    expires_at: datetime
    consumed_by_type: str | None
    consumed_by_id: UUID | None
    upload_endpoint: str


def _artifact_view(item: AgentArtifact) -> ArtifactView:
    return ArtifactView(
        id=item.id,
        kind=item.kind,
        filename=item.filename,
        state=item.state,
        size_declared=item.size_declared,
        size_received=item.size_received,
        expires_at=item.expires_at,
        consumed_by_type=item.consumed_by_type,
        consumed_by_id=item.consumed_by_id,
        upload_endpoint=f"/api/v1/agent/artifacts/{item.id}/content",
    )


def _artifact_limit(settings: Settings, kind: ArtifactKind) -> int:
    if kind == "STRATEGY_SOURCE":
        return settings.max_strategy_source_bytes
    if kind == "PLUGIN_WHEEL":
        return settings.max_plugin_wheel_bytes
    return settings.max_parquet_upload_bytes


def _safe_artifact_path(settings: Settings, relative_path: str) -> Path:
    root = settings.agent_artifact_root.resolve()
    path = (root / relative_path).resolve()
    if not path.is_relative_to(root):
        raise QfError("AGENT_ARTIFACT_INVALID", "Artifact path escaped its storage root.", 500)
    return path


def _owned_artifact(
    session: Session,
    artifact_id: UUID,
    principal: AgentPrincipal,
    *,
    for_update: bool = False,
) -> AgentArtifact:
    statement = select(AgentArtifact).where(
        AgentArtifact.id == artifact_id,
        AgentArtifact.owner_issuer == principal.issuer,
        AgentArtifact.owner_subject == principal.subject,
        AgentArtifact.owner_client_id == principal.client_id,
    )
    if for_update:
        statement = statement.with_for_update()
    item = session.execute(statement).scalar_one_or_none()
    if item is None:
        raise QfError("AGENT_ARTIFACT_UNKNOWN", "Agent artifact does not exist.", 404)
    return item


def _aware(value: datetime) -> datetime:
    return value if value.tzinfo is not None else value.replace(tzinfo=UTC)


def _require_unexpired(item: AgentArtifact) -> None:
    if _aware(item.expires_at) <= datetime.now(UTC):
        item.state = "EXPIRED"
        raise QfError("AGENT_ARTIFACT_EXPIRED", "Agent artifact has expired.", 410)


@router.post("/artifacts", response_model=ArtifactView, status_code=201)
def begin_artifact(
    payload: ArtifactBegin,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> ArtifactView:
    filename = payload.filename.strip()
    if Path(filename).name != filename or filename in {".", ".."}:
        raise QfError("AGENT_ARTIFACT_INVALID", "Artifact filename must be a basename.", 422)
    limit = _artifact_limit(request.app.state.settings, payload.kind)
    if payload.size_bytes > limit:
        raise QfError(
            "AGENT_ARTIFACT_TOO_LARGE",
            "Artifact exceeds the configured size limit.",
            413,
            {"max_bytes": limit},
        )
    artifact_id = uuid4()
    relative_path = str(Path("staging") / str(artifact_id) / "payload")
    path = _safe_artifact_path(request.app.state.settings, relative_path)
    try:
        path.parent.mkdir(parents=True, exist_ok=False)
        descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        os.close(descriptor)
        with session.begin():
            item = AgentArtifact(
                id=artifact_id,
                owner_issuer=principal.issuer,
                owner_subject=principal.subject,
                owner_client_id=principal.client_id,
                kind=payload.kind,
                filename=filename,
                state="STAGING",
                size_declared=payload.size_bytes,
                size_received=0,
                relative_path=relative_path,
                expires_at=datetime.now(UTC)
                + timedelta(seconds=request.app.state.settings.agent_artifact_ttl_seconds),
            )
            session.add(item)
            session.flush()
        return _artifact_view(item)
    except Exception:
        shutil.rmtree(path.parent, ignore_errors=True)
        raise


@router.get("/artifacts/{artifact_id}", response_model=ArtifactView)
def show_artifact(
    artifact_id: UUID,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> ArtifactView:
    item = _owned_artifact(session, artifact_id, principal)
    _require_unexpired(item)
    return _artifact_view(item)


@router.head("/artifacts/{artifact_id}/content")
def head_artifact_content(
    artifact_id: UUID,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> Response:
    item = _owned_artifact(session, artifact_id, principal)
    _require_unexpired(item)
    return Response(
        status_code=204,
        headers={
            "X-QF-Upload-Offset": str(item.size_received),
            "X-QF-Upload-Length": str(item.size_declared),
            "X-QF-Artifact-State": item.state,
        },
    )


@router.put("/artifacts/{artifact_id}/content", response_model=ArtifactView)
async def upload_artifact_content(
    artifact_id: UUID,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> ArtifactView:
    try:
        offset = int(request.headers.get("x-quazonai-upload-offset", ""))
    except ValueError as exc:
        raise QfError("AGENT_ARTIFACT_INVALID", "Upload offset must be an integer.", 422) from exc
    item = _owned_artifact(session, artifact_id, principal)
    _require_unexpired(item)
    if item.state != "STAGING":
        raise QfError("AGENT_ARTIFACT_INVALID_STATE", "Artifact is not accepting bytes.", 409)
    if offset != item.size_received:
        raise QfError(
            "AGENT_ARTIFACT_OFFSET_CONFLICT",
            "Upload offset does not match the accepted offset.",
            409,
            {"accepted_offset": item.size_received},
        )
    declared = item.size_declared
    path = _safe_artifact_path(request.app.state.settings, item.relative_path)
    session.rollback()

    lock_path = path.with_name(".upload.lock")
    try:
        lock_descriptor = os.open(lock_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError as exc:
        raise QfError("OPERATION_IN_PROGRESS", "Artifact upload is already in progress.", 409) from exc
    os.close(lock_descriptor)
    total = offset
    try:
        if not path.is_file() or path.stat().st_size != offset:
            raise QfError(
                "AGENT_ARTIFACT_INVALID",
                "Artifact file length and database offset differ.",
                500,
            )
        with path.open("r+b") as target:
            target.seek(offset)
            try:
                async for chunk in request.stream():
                    if not chunk:
                        continue
                    total += len(chunk)
                    if total > declared:
                        raise QfError(
                            "AGENT_ARTIFACT_TOO_LARGE",
                            "Upload exceeded its declared byte length.",
                            413,
                        )
                    target.write(chunk)
                target.flush()
                os.fsync(target.fileno())
            except BaseException:
                target.truncate(offset)
                target.flush()
                os.fsync(target.fileno())
                raise
        with session.begin():
            current = _owned_artifact(session, artifact_id, principal, for_update=True)
            _require_unexpired(current)
            if current.state != "STAGING" or current.size_received != offset:
                raise QfError(
                    "AGENT_ARTIFACT_OFFSET_CONFLICT",
                    "Artifact state changed while bytes were uploaded.",
                    409,
                    {"accepted_offset": current.size_received},
                )
            current.size_received = total
        return _artifact_view(current)
    finally:
        lock_path.unlink(missing_ok=True)


@router.post("/artifacts/{artifact_id}/finalize", response_model=ArtifactView)
def finalize_artifact(
    artifact_id: UUID,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> ArtifactView:
    with session.begin():
        item = _owned_artifact(session, artifact_id, principal, for_update=True)
        _require_unexpired(item)
        if item.state == "READY":
            return _artifact_view(item)
        if item.state != "STAGING":
            raise QfError("AGENT_ARTIFACT_INVALID_STATE", "Artifact cannot be finalized.", 409)
        path = _safe_artifact_path(request.app.state.settings, item.relative_path)
        actual = path.stat().st_size if path.is_file() else -1
        if actual != item.size_declared or item.size_received != item.size_declared:
            raise QfError(
                "AGENT_ARTIFACT_INCOMPLETE",
                "Artifact has not received its declared byte length.",
                409,
                {
                    "declared": item.size_declared,
                    "received": item.size_received,
                    "actual": actual,
                },
            )
        item.state = "READY"
    return _artifact_view(item)


@router.post("/artifacts/{artifact_id}/consume", response_model=ArtifactView)
def consume_artifact(
    artifact_id: UUID,
    payload: ArtifactConsume,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> ArtifactView:
    with session.begin():
        item = _owned_artifact(session, artifact_id, principal, for_update=True)
        _require_unexpired(item)
        if item.state == "CONSUMED":
            if (
                item.consumed_by_type == payload.resource_type
                and item.consumed_by_id == payload.resource_id
            ):
                return _artifact_view(item)
            raise QfError(
                "AGENT_ARTIFACT_ALREADY_CONSUMED",
                "Artifact was consumed by another resource.",
                409,
            )
        if item.state != "READY":
            raise QfError("AGENT_ARTIFACT_INVALID_STATE", "Artifact is not READY.", 409)
        item.state = "CONSUMED"
        item.consumed_by_type = payload.resource_type
        item.consumed_by_id = payload.resource_id
    return _artifact_view(item)


def _iter_file(path: Path) -> Iterator[bytes]:
    with path.open("rb") as source:
        while chunk := source.read(CHUNK_BYTES):
            yield chunk


@router.get("/artifacts/{artifact_id}/content")
def download_artifact_content(
    artifact_id: UUID,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> StreamingResponse:
    item = _owned_artifact(session, artifact_id, principal)
    _require_unexpired(item)
    if item.state not in {"READY", "CONSUMED"}:
        raise QfError("AGENT_ARTIFACT_INVALID_STATE", "Artifact content is not readable.", 409)
    path = _safe_artifact_path(request.app.state.settings, item.relative_path)
    if not path.is_file() or path.stat().st_size != item.size_declared:
        raise QfError("AGENT_ARTIFACT_INVALID", "Artifact content is unavailable.", 503)
    return StreamingResponse(
        _iter_file(path),
        media_type="application/octet-stream",
        headers={"Content-Length": str(item.size_declared)},
    )


@router.delete("/artifacts/{artifact_id}", response_model=ArtifactView)
def delete_artifact(
    artifact_id: UUID,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> ArtifactView:
    with session.begin():
        item = _owned_artifact(session, artifact_id, principal, for_update=True)
        if item.state == "CONSUMED":
            raise QfError(
                "AGENT_ARTIFACT_ALREADY_CONSUMED",
                "Consumed artifacts cannot be deleted through the Agent channel.",
                409,
            )
        path = _safe_artifact_path(request.app.state.settings, item.relative_path)
        item.state = "EXPIRED"
    shutil.rmtree(path.parent, ignore_errors=True)
    return _artifact_view(item)


class ImpactIssue(BaseModel):
    model_config = ConfigDict(extra="forbid")

    operation_name: str = Field(min_length=1, max_length=200)
    target_type: str = Field(min_length=1, max_length=100)
    target_id: UUID
    expected_state: dict[str, Any]
    impact_summary: dict[str, Any]
    ttl_seconds: int = Field(default=120, ge=15, le=300)


class ImpactConsume(BaseModel):
    model_config = ConfigDict(extra="forbid")

    operation_name: str
    target_type: str
    target_id: UUID
    expected_state: dict[str, Any]


class ImpactView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    operation_name: str
    target_type: str
    target_id: UUID
    expected_state: dict[str, Any]
    impact_summary: dict[str, Any]
    expires_at: datetime
    consumed_at: datetime | None


def _impact_view(item: AgentImpactToken) -> ImpactView:
    return ImpactView.model_validate(item, from_attributes=True)


@router.post("/impact-tokens", response_model=ImpactView, status_code=201)
def issue_impact_token(
    payload: ImpactIssue,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> ImpactView:
    with session.begin():
        item = AgentImpactToken(
            issuer=principal.issuer,
            subject=principal.subject,
            client_id=principal.client_id,
            operation_name=payload.operation_name,
            target_type=payload.target_type,
            target_id=payload.target_id,
            expected_state=payload.expected_state,
            impact_summary=payload.impact_summary,
            expires_at=datetime.now(UTC) + timedelta(seconds=payload.ttl_seconds),
        )
        session.add(item)
        session.flush()
    return _impact_view(item)


@router.post("/impact-tokens/{token_id}/consume", response_model=ImpactView)
def consume_impact_token(
    token_id: UUID,
    payload: ImpactConsume,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> ImpactView:
    with session.begin():
        item = session.execute(
            select(AgentImpactToken)
            .where(
                AgentImpactToken.id == token_id,
                AgentImpactToken.issuer == principal.issuer,
                AgentImpactToken.subject == principal.subject,
                AgentImpactToken.client_id == principal.client_id,
            )
            .with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("IMPACT_TOKEN_UNKNOWN", "Impact token does not exist.", 404)
        if item.consumed_at is not None:
            raise QfError("IMPACT_TOKEN_CONSUMED", "Impact token was already consumed.", 409)
        if _aware(item.expires_at) <= datetime.now(UTC):
            raise QfError("IMPACT_TOKEN_EXPIRED", "Impact token has expired.", 409)
        if (
            item.operation_name != payload.operation_name
            or item.target_type != payload.target_type
            or item.target_id != payload.target_id
            or item.expected_state != payload.expected_state
        ):
            raise QfError(
                "IMPACT_TOKEN_MISMATCH",
                "Impact token does not match the requested operation and state.",
                409,
            )
        item.consumed_at = datetime.now(UTC)
    return _impact_view(item)


class TaskBind(BaseModel):
    model_config = ConfigDict(extra="forbid")

    task_id: str = Field(min_length=1, max_length=300)
    extension_version: str = Field(min_length=1, max_length=100)
    operation_type: Literal["job", "run", "deployment"]
    operation_id: UUID
    ttl_seconds: int = Field(default=3600, ge=60, le=86_400)


class TaskView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    task_id: str
    extension_version: str
    operation_type: str
    operation_id: UUID
    created_at: datetime
    expires_at: datetime


def _task_view(item: McpTaskBinding) -> TaskView:
    return TaskView.model_validate(item, from_attributes=True)


@router.post("/tasks", response_model=TaskView, status_code=201)
def bind_task(
    payload: TaskBind,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> TaskView:
    with session.begin():
        existing = session.get(McpTaskBinding, payload.task_id)
        if existing is not None:
            if (
                existing.issuer == principal.issuer
                and existing.subject == principal.subject
                and existing.client_id == principal.client_id
                and existing.operation_type == payload.operation_type
                and existing.operation_id == payload.operation_id
            ):
                return _task_view(existing)
            raise QfError("MCP_TASK_ACCESS_DENIED", "Task ID belongs to another operation.", 409)
        item = McpTaskBinding(
            task_id=payload.task_id,
            issuer=principal.issuer,
            subject=principal.subject,
            client_id=principal.client_id,
            extension_version=payload.extension_version,
            operation_type=payload.operation_type,
            operation_id=payload.operation_id,
            expires_at=datetime.now(UTC) + timedelta(seconds=payload.ttl_seconds),
        )
        session.add(item)
        session.flush()
    return _task_view(item)


@router.get("/tasks/{task_id}", response_model=TaskView)
def show_task(
    task_id: str,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> TaskView:
    item = session.get(McpTaskBinding, task_id)
    if (
        item is None
        or item.issuer != principal.issuer
        or item.subject != principal.subject
        or item.client_id != principal.client_id
    ):
        raise QfError("MCP_TASK_NOT_FOUND", "MCP task does not exist.", 404)
    if _aware(item.expires_at) <= datetime.now(UTC):
        raise QfError("MCP_TASK_NOT_FOUND", "MCP task has expired.", 404)
    return _task_view(item)


@router.get("/manifest")
def agent_manifest(
    principal: AgentPrincipal = Depends(require_agent_gateway),
) -> dict[str, Any]:
    return {
        "protocol_version": "2026-07-28",
        "principal": {
            "issuer": principal.issuer,
            "subject": principal.subject,
            "client_id": principal.client_id,
            "scopes": principal.scopes,
        },
        "artifact_kinds": ["STRATEGY_SOURCE", "PLUGIN_WHEEL", "PARQUET_L2"],
        "human_only": [
            "credential_secret_write",
            "approval_approve",
            "approval_reject",
            "plugin_force_remove",
            "live_money_canary",
            "master_key_change",
            "raw_order_submission",
            "destructive_database_operation",
        ],
    }
