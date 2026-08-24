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
    provided = request.headers.get("x-qz-internal-token")
    if expected is None:
        raise QfError(
            "MCP_GATEWAY_DISABLED",
            "The optional MCP Gateway is not enabled for this Core API.",
            503,
        )
    if provided is None or not compare_digest(provided, expected):
        raise QfError("MCP_GATEWAY_UNAUTHORIZED", "Gateway authentication failed.", 401)
    issuer = (request.headers.get("x-qz-agent-issuer") or "").strip()
    subject = (request.headers.get("x-qz-agent-subject") or "").strip()
    client_id = (request.headers.get("x-qz-agent-client-id") or "").strip()
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
                for item in (request.headers.get("x-qz-agent-scopes") or "").split()
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


@router.post("/operations/{operation_id}/complete", response_model=OperationReceiptView)
def complete_operation(
    operation_id: UUID,
    payload: OperationComplete,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> OperationReceiptView:
    with session.begin():
        item = session.execute(
            select(OperationReceipt)
            .where(
                OperationReceipt.id == operation_id,
                OperationReceipt.actor_kind == "MCP",
                OperationReceipt.actor_id == principal.actor_id,
            )
            .with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("OPERATION_NOT_FOUND", "Operation receipt was not found.", 404)
        if item.state != "IN_PROGRESS":
            raise QfError("OPERATION_NOT_IN_PROGRESS", "Operation receipt is terminal.", 409)
        item.state = "SUCCEEDED"
        item.result = payload.result
        item.finished_at = datetime.now(UTC)
        session.flush()
        return _receipt_view(item)


@router.post("/operations/{operation_id}/fail", response_model=OperationReceiptView)
def fail_operation(
    operation_id: UUID,
    payload: OperationFailure,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> OperationReceiptView:
    with session.begin():
        item = session.execute(
            select(OperationReceipt)
            .where(
                OperationReceipt.id == operation_id,
                OperationReceipt.actor_kind == "MCP",
                OperationReceipt.actor_id == principal.actor_id,
            )
            .with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("OPERATION_NOT_FOUND", "Operation receipt was not found.", 404)
        if item.state != "IN_PROGRESS":
            raise QfError("OPERATION_NOT_IN_PROGRESS", "Operation receipt is terminal.", 409)
        item.state = "FAILED"
        item.error_code = payload.error_code
        item.result = payload.result
        item.finished_at = datetime.now(UTC)
        session.flush()
        return _receipt_view(item)


class AgentArtifactCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")

    kind: str = Field(min_length=1, max_length=100)
    filename: str = Field(min_length=1, max_length=255)
    size_bytes: int = Field(ge=0)
    media_type: str = Field(default="application/octet-stream", max_length=200)


class AgentArtifactView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    kind: str
    filename: str
    media_type: str
    size_bytes: int
    size_received: int
    state: str
    created_at: str
    expires_at: str
    consumed_at: str | None
    resource_type: str | None
    resource_id: UUID | None


def _artifact_view(item: AgentArtifact) -> AgentArtifactView:
    return AgentArtifactView(
        id=item.id,
        kind=item.kind,
        filename=item.filename,
        media_type=item.media_type,
        size_bytes=item.size_bytes,
        size_received=item.size_received,
        state=item.state,
        created_at=item.created_at.isoformat(),
        expires_at=item.expires_at.isoformat(),
        consumed_at=item.consumed_at.isoformat() if item.consumed_at else None,
        resource_type=item.resource_type,
        resource_id=item.resource_id,
    )


def _artifact_root(request: Request) -> Path:
    settings: Settings = request.app.state.settings
    root = settings.artifact_root / "agent"
    root.mkdir(parents=True, exist_ok=True)
    return root


def _safe_filename(filename: str) -> str:
    candidate = Path(filename)
    if candidate.name != filename or filename in {".", ".."}:
        raise QfError("AGENT_ARTIFACT_FILENAME_INVALID", "Artifact filename is invalid.", 422)
    return filename


def _artifact_path(root: Path, item: AgentArtifact) -> Path:
    return root / str(item.id) / _safe_filename(item.filename)


def _owned_artifact(session: Session, artifact_id: UUID, principal: AgentPrincipal) -> AgentArtifact:
    item = session.scalar(
        select(AgentArtifact).where(
            AgentArtifact.id == artifact_id,
            AgentArtifact.actor_id == principal.actor_id,
        )
    )
    if item is None:
        raise QfError("AGENT_ARTIFACT_NOT_FOUND", "Agent artifact was not found.", 404)
    return item


@router.post("/artifacts", response_model=AgentArtifactView, status_code=201)
def create_artifact(
    payload: AgentArtifactCreate,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> AgentArtifactView:
    _safe_filename(payload.filename)
    settings: Settings = request.app.state.settings
    with session.begin():
        item = AgentArtifact(
            actor_id=principal.actor_id,
            kind=payload.kind,
            filename=payload.filename,
            media_type=payload.media_type,
            size_bytes=payload.size_bytes,
            size_received=0,
            state="UPLOADING",
            expires_at=datetime.now(UTC) + timedelta(seconds=settings.agent_artifact_ttl_seconds),
        )
        session.add(item)
        session.flush()
        path = _artifact_path(_artifact_root(request), item)
        path.parent.mkdir(parents=True, exist_ok=False)
        path.touch(exist_ok=False)
        return _artifact_view(item)


@router.get("/artifacts/{artifact_id}", response_model=AgentArtifactView)
def get_artifact(
    artifact_id: UUID,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> AgentArtifactView:
    return _artifact_view(_owned_artifact(session, artifact_id, principal))


@router.put("/artifacts/{artifact_id}/content", response_model=AgentArtifactView)
async def upload_artifact_content(
    artifact_id: UUID,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> AgentArtifactView:
    try:
        offset = int(request.headers.get("x-qz-upload-offset", "-1"))
    except ValueError as exc:
        raise QfError("AGENT_ARTIFACT_OFFSET_INVALID", "Upload offset is invalid.", 422) from exc
    body = await request.body()
    if len(body) > CHUNK_BYTES:
        raise QfError("AGENT_ARTIFACT_CHUNK_TOO_LARGE", "Artifact chunk exceeds 1 MiB.", 413)
    with session.begin():
        item = session.execute(
            select(AgentArtifact).where(
                AgentArtifact.id == artifact_id,
                AgentArtifact.actor_id == principal.actor_id,
            ).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("AGENT_ARTIFACT_NOT_FOUND", "Agent artifact was not found.", 404)
        if item.state != "UPLOADING":
            raise QfError("AGENT_ARTIFACT_NOT_UPLOADING", "Agent artifact is not accepting bytes.", 409)
        if offset != item.size_received:
            raise QfError(
                "AGENT_ARTIFACT_OFFSET_CONFLICT",
                "Upload offset does not match the persisted byte count.",
                409,
                {"expected_offset": item.size_received},
            )
        if item.size_received + len(body) > item.size_bytes:
            raise QfError("AGENT_ARTIFACT_SIZE_EXCEEDED", "Artifact exceeds declared size.", 409)
        path = _artifact_path(_artifact_root(request), item)
        with path.open("ab") as handle:
            handle.write(body)
            handle.flush()
            os.fsync(handle.fileno())
        item.size_received += len(body)
        session.flush()
        return _artifact_view(item)


@router.post("/artifacts/{artifact_id}/finalize", response_model=AgentArtifactView)
def finalize_artifact(
    artifact_id: UUID,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> AgentArtifactView:
    with session.begin():
        item = session.execute(
            select(AgentArtifact).where(
                AgentArtifact.id == artifact_id,
                AgentArtifact.actor_id == principal.actor_id,
            ).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("AGENT_ARTIFACT_NOT_FOUND", "Agent artifact was not found.", 404)
        if item.state != "UPLOADING":
            raise QfError("AGENT_ARTIFACT_NOT_UPLOADING", "Agent artifact cannot be finalized.", 409)
        if item.size_received != item.size_bytes:
            raise QfError(
                "AGENT_ARTIFACT_INCOMPLETE",
                "Artifact byte count is incomplete.",
                409,
                {"expected": item.size_bytes, "received": item.size_received},
            )
        item.state = "READY"
        session.flush()
        return _artifact_view(item)


@router.get("/artifacts/{artifact_id}/content")
def download_artifact_content(
    artifact_id: UUID,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> StreamingResponse:
    item = _owned_artifact(session, artifact_id, principal)
    if item.state not in {"READY", "CONSUMED"}:
        raise QfError("AGENT_ARTIFACT_NOT_READY", "Agent artifact is not ready.", 409)
    path = _artifact_path(_artifact_root(request), item)
    if not path.is_file():
        raise QfError("AGENT_ARTIFACT_CONTENT_MISSING", "Artifact content is missing.", 410)

    def body() -> Iterator[bytes]:
        with path.open("rb") as handle:
            while chunk := handle.read(CHUNK_BYTES):
                yield chunk

    return StreamingResponse(body(), media_type=item.media_type)


class AgentArtifactConsume(BaseModel):
    model_config = ConfigDict(extra="forbid")

    resource_type: str = Field(min_length=1, max_length=100)
    resource_id: UUID


@router.post("/artifacts/{artifact_id}/consume", response_model=AgentArtifactView)
def consume_artifact(
    artifact_id: UUID,
    payload: AgentArtifactConsume,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> AgentArtifactView:
    with session.begin():
        item = session.execute(
            select(AgentArtifact).where(
                AgentArtifact.id == artifact_id,
                AgentArtifact.actor_id == principal.actor_id,
            ).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("AGENT_ARTIFACT_NOT_FOUND", "Agent artifact was not found.", 404)
        if item.state != "READY":
            raise QfError("AGENT_ARTIFACT_NOT_READY", "Agent artifact is not available for consume.", 409)
        item.state = "CONSUMED"
        item.consumed_at = datetime.now(UTC)
        item.resource_type = payload.resource_type
        item.resource_id = payload.resource_id
        session.flush()
        return _artifact_view(item)


@router.delete("/artifacts/{artifact_id}", status_code=204)
def delete_artifact(
    artifact_id: UUID,
    request: Request,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> Response:
    with session.begin():
        item = session.execute(
            select(AgentArtifact).where(
                AgentArtifact.id == artifact_id,
                AgentArtifact.actor_id == principal.actor_id,
            ).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("AGENT_ARTIFACT_NOT_FOUND", "Agent artifact was not found.", 404)
        if item.state == "CONSUMED":
            raise QfError("AGENT_ARTIFACT_ALREADY_CONSUMED", "Consumed artifacts cannot be deleted.", 409)
        path = _artifact_path(_artifact_root(request), item)
        item.state = "DELETED"
        session.flush()
    shutil.rmtree(path.parent, ignore_errors=True)
    return Response(status_code=204)


class AgentImpactTokenCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")

    operation_name: str = Field(min_length=1, max_length=200)
    target_type: str = Field(min_length=1, max_length=100)
    target_id: UUID
    expected_state: dict[str, Any]
    impact_summary: dict[str, Any]
    ttl_seconds: int = Field(default=120, ge=1, le=600)


class AgentImpactTokenConsume(BaseModel):
    model_config = ConfigDict(extra="forbid")

    operation_name: str = Field(min_length=1, max_length=200)
    target_type: str = Field(min_length=1, max_length=100)
    target_id: UUID
    expected_state: dict[str, Any]


class AgentImpactTokenView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    operation_name: str
    target_type: str
    target_id: UUID
    expected_state: dict[str, Any]
    impact_summary: dict[str, Any]
    expires_at: str
    consumed_at: str | None


def _impact_view(item: AgentImpactToken) -> AgentImpactTokenView:
    return AgentImpactTokenView(
        id=item.id,
        operation_name=item.operation_name,
        target_type=item.target_type,
        target_id=item.target_id,
        expected_state=item.expected_state,
        impact_summary=item.impact_summary,
        expires_at=item.expires_at.isoformat(),
        consumed_at=item.consumed_at.isoformat() if item.consumed_at else None,
    )


@router.post("/impact-tokens", response_model=AgentImpactTokenView, status_code=201)
def create_impact_token(
    payload: AgentImpactTokenCreate,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> AgentImpactTokenView:
    with session.begin():
        item = AgentImpactToken(
            actor_id=principal.actor_id,
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


@router.post("/impact-tokens/{token_id}/consume", response_model=AgentImpactTokenView)
def consume_impact_token(
    token_id: UUID,
    payload: AgentImpactTokenConsume,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> AgentImpactTokenView:
    with session.begin():
        item = session.execute(
            select(AgentImpactToken)
            .where(
                AgentImpactToken.id == token_id,
                AgentImpactToken.actor_id == principal.actor_id,
            )
            .with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("IMPACT_TOKEN_NOT_FOUND", "Impact token was not found.", 404)
        if item.consumed_at is not None:
            raise QfError("IMPACT_TOKEN_CONSUMED", "Impact token has already been consumed.", 409)
        if item.expires_at < datetime.now(UTC):
            raise QfError("IMPACT_TOKEN_EXPIRED", "Impact token has expired.", 409)
        if (
            item.operation_name != payload.operation_name
            or item.target_type != payload.target_type
            or item.target_id != payload.target_id
            or item.expected_state != payload.expected_state
        ):
            raise QfError("IMPACT_TOKEN_MISMATCH", "Impact token does not match current state.", 409)
        item.consumed_at = datetime.now(UTC)
        session.flush()
        return _impact_view(item)


class McpTaskCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")

    task_id: str = Field(min_length=1, max_length=255)
    extension_version: str = Field(min_length=1, max_length=200)
    operation_type: Literal["run", "report"]
    operation_id: UUID


class McpTaskView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    task_id: str
    extension_version: str
    operation_type: str
    operation_id: UUID
    state: str
    created_at: str
    updated_at: str


def _task_view(item: McpTaskBinding) -> McpTaskView:
    return McpTaskView(
        task_id=item.task_id,
        extension_version=item.extension_version,
        operation_type=item.operation_type,
        operation_id=item.operation_id,
        state=item.state,
        created_at=item.created_at.isoformat(),
        updated_at=item.updated_at.isoformat(),
    )


@router.post("/tasks", response_model=McpTaskView, status_code=201)
def create_task(
    payload: McpTaskCreate,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> McpTaskView:
    with session.begin():
        existing = session.scalar(
            select(McpTaskBinding).where(
                McpTaskBinding.task_id == payload.task_id,
                McpTaskBinding.actor_id == principal.actor_id,
            )
        )
        if existing is not None:
            if (
                existing.extension_version != payload.extension_version
                or existing.operation_type != payload.operation_type
                or existing.operation_id != payload.operation_id
            ):
                raise QfError("MCP_TASK_CONFLICT", "MCP task id is already bound.", 409)
            return _task_view(existing)
        item = McpTaskBinding(
            actor_id=principal.actor_id,
            task_id=payload.task_id,
            extension_version=payload.extension_version,
            operation_type=payload.operation_type,
            operation_id=payload.operation_id,
            state="BOUND",
        )
        session.add(item)
        session.flush()
        return _task_view(item)


@router.get("/tasks/{task_id}", response_model=McpTaskView)
def get_task(
    task_id: str,
    principal: AgentPrincipal = Depends(require_agent_gateway),
    session: Session = Depends(get_session),
) -> McpTaskView:
    item = session.scalar(
        select(McpTaskBinding).where(
            McpTaskBinding.task_id == task_id,
            McpTaskBinding.actor_id == principal.actor_id,
        )
    )
    if item is None:
        raise QfError("MCP_TASK_NOT_FOUND", "MCP task was not found.", 404)
    return _task_view(item)
