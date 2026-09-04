"""Core-side, contract-scoped facts for one Mission MCP server.

The MCP child receives only a Unix-socket path.  This module stays with the
Mission worker, owns the session factory, and accepts a fixed set of JSON-line
requests.  It deliberately has no generic database, HTTP, file, credential,
approval, handoff, or execution proxy.
"""

from __future__ import annotations

import json
import os
import socket
import socketserver
import tempfile
import threading
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, cast
from uuid import UUID, uuid4

from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from agent_harness.contracts import (
    AlphaArtifactDraftV1,
    DraftArtifact,
    DraftArtifactKind,
    MissionContractV1,
    MissionTool,
    RoleProfile,
    effective_allowed_tools,
    validate_alpha_artifact_summary,
)
from db.models import (
    AgentSession,
    AgentTurn,
    AlphaDiscoveryEvaluation,
    AlphaModel,
    AlphaModelVersion,
    DatasetRevision,
    Event,
    MissionArtifact,
    PublicMutationReceipt,
    QuantRuntimeRun,
    ResearchMission,
    ResearchProgram,
    SearchLedgerEntry,
)
from db.session import SessionFactory
from errors import QfError


_MAX_RPC_BYTES = 96 * 1024
_MAX_FACT_BYTES = 64 * 1024
_MAX_EVIDENCE_ITEMS = 50
_DISCOVERY_DISCLOSURE_LEVEL = "DISCOVERY_FULL"
_IMPLEMENTED_TOOLS = frozenset(
    {
        MissionTool.PROFILE_DATASET,
        MissionTool.LIST_PRIOR_ATTEMPTS,
        MissionTool.QUERY_SEARCH_LEDGER,
        MissionTool.QUERY_ALPHA_LIBRARY,
        MissionTool.GET_RUN_EVIDENCE,
        MissionTool.SUBMIT_MISSION_ARTIFACT,
    }
)
_FORBIDDEN_PUBLIC_FIELDS = frozenset(
    {
        "access_token",
        "api_key",
        "apikey",
        "password",
        "private_key",
        "refresh_token",
        "secret",
        "secret_key",
        "service_token",
        "token",
    }
)


class _ScopedRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    mission_id: UUID


class _DatasetRequest(_ScopedRequest):
    dataset_revision_id: UUID


class _EvidenceListRequest(_ScopedRequest):
    limit: int = Field(default=20, ge=1, le=_MAX_EVIDENCE_ITEMS)


class _SearchLedgerRequest(_EvidenceListRequest):
    family: str | None = Field(default=None, min_length=1, max_length=200)


class _RunEvidenceRequest(_ScopedRequest):
    run_id: UUID


class _SubmitArtifactRequest(_ScopedRequest):
    idempotency_key: str = Field(min_length=1, max_length=200)
    expected_revision: int = Field(ge=1)
    artifact: DraftArtifact

    @field_validator("idempotency_key")
    @classmethod
    def require_idempotency_key(cls, value: str) -> str:
        value = value.strip()
        if not value:
            raise ValueError("idempotency_key must not be blank")
        return value


class MissionCapabilityError(Exception):
    """The unprivileged MCP child sees only this bounded Core error."""

    def __init__(self, code: str, message: str) -> None:
        self.code = code
        self.message = message
        super().__init__(f"{code}: {message}")


def implemented_mission_tools() -> frozenset[MissionTool]:
    """Return the small fixed bridge surface; unsupported contract tools stay absent."""
    return _IMPLEMENTED_TOOLS


def _parse(model: type[BaseModel], arguments: dict[str, Any]) -> Any:
    try:
        return model.model_validate(arguments)
    except ValidationError as exc:
        raise QfError("MISSION_TOOL_ARGUMENT_INVALID", "Mission tool arguments are invalid.", 422) from exc


def _iso(value: datetime | None) -> str | None:
    if value is None:
        return None
    normalized = value.replace(tzinfo=UTC) if value.tzinfo is None else value.astimezone(UTC)
    return normalized.isoformat()


def _reject_sensitive_fields(value: object, path: str = "$") -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = str(key).casefold()
            if normalized in _FORBIDDEN_PUBLIC_FIELDS or normalized.endswith("_secret"):
                raise QfError(
                    "MISSION_FACT_CONTAINS_SECRET",
                    "Mission facts and artifacts must not contain credentials.",
                    422,
                    {"path": f"{path}.{key}"},
                )
            _reject_sensitive_fields(item, f"{path}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _reject_sensitive_fields(item, f"{path}[{index}]")


def _safe_json(value: object) -> Any:
    """Return bounded JSON after rejecting credential-shaped fields."""
    _reject_sensitive_fields(value)
    try:
        encoded = json.dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False)
    except (TypeError, ValueError) as exc:
        raise QfError("MISSION_FACT_INVALID", "Mission facts must be JSON values.", 422) from exc
    if len(encoded.encode("utf-8")) > _MAX_FACT_BYTES:
        raise QfError("MISSION_FACT_TOO_LARGE", "Mission facts exceed the bounded tool payload.", 422)
    return json.loads(encoded)


class MissionCapabilityService:
    """Fixed Core operations with per-call Mission state and scope checks."""

    def __init__(self, contract: MissionContractV1, session_factory: SessionFactory) -> None:
        self._contract = contract
        self._session_factory = session_factory

    def invoke(self, tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        try:
            tool = MissionTool(tool_name)
        except ValueError as exc:
            raise QfError("MISSION_TOOL_INVALID", "Mission tool is not available.", 404) from exc
        if tool not in _IMPLEMENTED_TOOLS:
            raise QfError("MISSION_TOOL_UNAVAILABLE", "Mission tool is not implemented.", 404)
        try:
            mission_id = UUID(str(arguments.get("mission_id")))
        except (AttributeError, TypeError, ValueError) as exc:
            raise QfError("MISSION_TOOL_ARGUMENT_INVALID", "Mission tool arguments are invalid.", 422) from exc
        with self._session_factory() as session, session.begin():
            mission = self._mission(session, tool, mission_id, lock=True)
            turn = session.scalar(
                select(AgentTurn)
                .join(AgentSession, AgentSession.id == AgentTurn.agent_session_id)
                .where(
                    AgentSession.mission_id == mission.id,
                    AgentTurn.state == "RUNNING",
                )
                .with_for_update()
            )
            if turn is None:
                raise QfError(
                    "MISSION_TURN_NOT_RUNNING",
                    "Mission tools require one running AgentTurn.",
                    409,
                )
            limit = min(self._contract.max_tool_calls, mission.max_tool_calls)
            if turn.tool_call_count >= limit:
                raise QfError(
                    "MISSION_TOOL_CALL_BUDGET_EXCEEDED",
                    "Mission tool-call budget is exhausted.",
                    409,
                )
            turn.tool_call_count += 1
        if tool == MissionTool.SUBMIT_MISSION_ARTIFACT:
            return self._submit_artifact(_parse(_SubmitArtifactRequest, arguments))
        if tool == MissionTool.PROFILE_DATASET:
            return self._profile_dataset(_parse(_DatasetRequest, arguments))
        if tool == MissionTool.LIST_PRIOR_ATTEMPTS:
            return self._list_prior_attempts(_parse(_EvidenceListRequest, arguments))
        if tool == MissionTool.QUERY_SEARCH_LEDGER:
            return self._query_search_ledger(_parse(_SearchLedgerRequest, arguments))
        if tool == MissionTool.QUERY_ALPHA_LIBRARY:
            return self._query_alpha_library(_parse(_EvidenceListRequest, arguments))
        if tool == MissionTool.GET_RUN_EVIDENCE:
            return self._get_run_evidence(_parse(_RunEvidenceRequest, arguments))
        raise AssertionError(f"missing Mission capability handler for {tool.value}")

    def _mission(
        self,
        session: Session,
        tool: MissionTool,
        mission_id: UUID,
        *,
        lock: bool = False,
    ) -> ResearchMission:
        if mission_id != self._contract.mission_id:
            raise QfError("MISSION_SCOPE_FORBIDDEN", "Mission tool cannot cross Mission scope.", 403)
        statement = select(ResearchMission).where(ResearchMission.id == mission_id)
        if lock:
            statement = statement.with_for_update()
        mission = session.execute(statement).scalar_one_or_none()
        if mission is None:
            raise QfError("MISSION_NOT_FOUND", "Mission does not exist.", 404)
        if mission.state != "RUNNING":
            raise QfError("MISSION_STATE_CONFLICT", "Mission tools require a RUNNING Mission.", 409)
        if datetime.now(UTC) > self._contract.deadline:
            raise QfError("MISSION_DEADLINE_EXPIRED", "Mission capability deadline has expired.", 409)
        program = session.get(ResearchProgram, mission.program_id)
        if program is None or program.state != "ACTIVE":
            raise QfError("PROGRAM_NOT_ACTIVE", "Mission tools require an ACTIVE Research Program.", 409)
        try:
            role = RoleProfile(mission.role_profile)
            granted = effective_allowed_tools(
                role,
                tuple(MissionTool(value) for value in mission.capability_snapshot.get("allowed_tools", ())),
            )
        except (TypeError, ValueError) as exc:
            raise QfError("MISSION_CONTRACT_INVALID", "Mission capability snapshot is invalid.", 409) from exc
        if (
            mission.mission_type != self._contract.mission_type.value
            or str(mission.branch_id) != str(self._contract.branch_snapshot.get("branch_id"))
            or tool not in self._contract.effective_tools
            or tool not in granted
        ):
            raise QfError("MISSION_TOOL_FORBIDDEN", "Mission tool is not granted by the frozen contract.", 403)
        return mission

    def _profile_dataset(self, request: _DatasetRequest) -> dict[str, Any]:
        with self._session_factory() as session:
            mission = self._mission(session, MissionTool.PROFILE_DATASET, request.mission_id)
            if request.dataset_revision_id not in self._contract.allowed_dataset_revision_ids:
                raise QfError("MISSION_DATASET_FORBIDDEN", "Dataset is not granted to this Mission.", 403)
            dataset = session.get(DatasetRevision, request.dataset_revision_id)
            if dataset is None:
                raise QfError("DATASET_NOT_FOUND", "Dataset Revision does not exist.", 404)
            if dataset.partition != "DISCOVERY":
                raise QfError("MISSION_DATASET_SEALED", "Only Discovery datasets are visible to Codex.", 403)
            return {
                "dataset_revision_id": str(dataset.id),
                "mission_revision": mission.revision,
                "partition": dataset.partition,
                "data_class": dataset.data_class,
                "schema_version": dataset.schema_version,
                "event_start": _iso(dataset.event_start),
                "event_end": _iso(dataset.event_end),
                "available_start": _iso(dataset.available_start),
                "available_end": _iso(dataset.available_end),
                "row_count": dataset.row_count,
                "quality_state": dataset.quality_state,
                "point_in_time_state": dataset.point_in_time_state,
                "promotability": dataset.promotability,
            }

    def _ledger_rows(
        self,
        session: Session,
        mission: ResearchMission,
        *,
        limit: int,
        family: str | None = None,
        run_id: UUID | None = None,
    ) -> list[SearchLedgerEntry]:
        statement = select(SearchLedgerEntry).join(
            QuantRuntimeRun, QuantRuntimeRun.id == SearchLedgerEntry.run_id
        ).where(
            SearchLedgerEntry.program_id == mission.program_id,
            SearchLedgerEntry.branch_id == mission.branch_id,
            SearchLedgerEntry.disclosure_level == _DISCOVERY_DISCLOSURE_LEVEL,
            QuantRuntimeRun.mode == "DISCOVERY",
        )
        if family is not None:
            statement = statement.where(SearchLedgerEntry.family == family)
        if run_id is not None:
            statement = statement.where(SearchLedgerEntry.run_id == run_id)
        return list(
            session.scalars(
                statement.order_by(SearchLedgerEntry.created_at.desc(), SearchLedgerEntry.id).limit(limit)
            )
        )

    def _discovery_rows(
        self,
        session: Session,
        mission: ResearchMission,
        *,
        limit: int,
        family: str | None = None,
    ) -> list[AlphaDiscoveryEvaluation]:
        if family is not None and family != "ALPHA_DISCOVERY":
            return []
        return list(
            session.scalars(
                select(AlphaDiscoveryEvaluation)
                .where(
                    AlphaDiscoveryEvaluation.program_id == mission.program_id,
                    AlphaDiscoveryEvaluation.branch_id == mission.branch_id,
                )
                .order_by(
                    AlphaDiscoveryEvaluation.created_at.desc(),
                    AlphaDiscoveryEvaluation.id,
                )
                .limit(limit)
            )
        )

    @staticmethod
    def _discovery_item(row: AlphaDiscoveryEvaluation) -> dict[str, Any]:
        return {
            "id": str(row.id),
            "run_id": None,
            "mission_id": str(row.mission_id),
            "family": "ALPHA_DISCOVERY",
            "parameters": {
                "alpha_model_version_id": str(row.alpha_model_version_id),
                "evaluation_design_version_id": str(row.evaluation_design_version_id),
                "discovery_dataset_revision_id": str(row.discovery_dataset_revision_id),
            },
            "outcome": row.state,
            "failure_code": row.outcome_code if row.state != "VALID" else None,
            "evidence_summary": {
                "evaluation_design_version_id": str(row.evaluation_design_version_id),
                "discovery_dataset_revision_id": str(row.discovery_dataset_revision_id),
            },
            "created_at": _iso(row.created_at),
        }

    @staticmethod
    def _ledger_item(entry: SearchLedgerEntry) -> dict[str, Any] | None:
        try:
            parameters = _safe_json(entry.parameters)
            evidence_summary = _safe_json(entry.evidence_summary)
        except QfError:
            return None
        return {
            "id": str(entry.id),
            "run_id": str(entry.run_id),
            "mission_id": str(entry.mission_id) if entry.mission_id else None,
            "family": entry.family,
            "parameters": parameters,
            "outcome": entry.outcome,
            "failure_code": entry.failure_code,
            "evidence_summary": evidence_summary,
            "created_at": _iso(entry.created_at),
        }

    def _list_prior_attempts(self, request: _EvidenceListRequest) -> dict[str, Any]:
        with self._session_factory() as session:
            mission = self._mission(session, MissionTool.LIST_PRIOR_ATTEMPTS, request.mission_id)
            entries = self._ledger_rows(session, mission, limit=request.limit)
            discoveries = self._discovery_rows(session, mission, limit=request.limit)
            items = [item for entry in entries if (item := self._ledger_item(entry)) is not None]
            items.extend(self._discovery_item(row) for row in discoveries)
            items.sort(key=lambda item: (item["created_at"] or "", item["id"]), reverse=True)
            omitted = len(entries) + len(discoveries) - len(items[: request.limit])
            return {"items": items[: request.limit], "omitted_count": omitted}

    def _query_search_ledger(self, request: _SearchLedgerRequest) -> dict[str, Any]:
        with self._session_factory() as session:
            mission = self._mission(session, MissionTool.QUERY_SEARCH_LEDGER, request.mission_id)
            family = request.family.strip() if request.family else None
            entries = self._ledger_rows(
                session, mission, limit=request.limit, family=family
            )
            discoveries = self._discovery_rows(session, mission, limit=request.limit, family=family)
            items = [item for entry in entries if (item := self._ledger_item(entry)) is not None]
            items.extend(self._discovery_item(row) for row in discoveries)
            items.sort(key=lambda item: (item["created_at"] or "", item["id"]), reverse=True)
            omitted = len(entries) + len(discoveries) - len(items[: request.limit])
            return {"items": items[: request.limit], "omitted_count": omitted}

    def _query_alpha_library(self, request: _EvidenceListRequest) -> dict[str, Any]:
        with self._session_factory() as session:
            mission = self._mission(session, MissionTool.QUERY_ALPHA_LIBRARY, request.mission_id)
            rows = session.execute(
                select(AlphaModel, AlphaModelVersion)
                .join(AlphaModelVersion, AlphaModelVersion.alpha_model_id == AlphaModel.id)
                .join(ResearchMission, ResearchMission.id == AlphaModelVersion.source_mission_id)
                .where(
                    AlphaModel.owner_program_id == mission.program_id,
                    ResearchMission.branch_id == mission.branch_id,
                    ResearchMission.mission_type == "ALPHA_DISCOVERY",
                )
                .order_by(AlphaModelVersion.created_at.desc(), AlphaModelVersion.id)
                .limit(request.limit)
            ).all()
            items: list[dict[str, Any]] = []
            for model, version in rows:
                try:
                    input_contract = _safe_json(version.input_contract)
                    output_contract = _safe_json(version.output_contract)
                except QfError:
                    continue
                items.append(
                    {
                        "alpha_model_id": str(model.id),
                        "alpha_key": model.alpha_key,
                        "name": model.name,
                        "family": model.family,
                        "alpha_model_version_id": str(version.id),
                        "version_no": version.version_no,
                        "universe_version_id": str(version.universe_version_id),
                        "horizon": version.horizon,
                        "mode": version.mode,
                        "state": version.state,
                        "input_contract": input_contract,
                        "output_contract": output_contract,
                    }
                )
            return {"items": items, "omitted_count": len(rows) - len(items)}

    def _get_run_evidence(self, request: _RunEvidenceRequest) -> dict[str, Any]:
        with self._session_factory() as session:
            mission = self._mission(session, MissionTool.GET_RUN_EVIDENCE, request.mission_id)
            entry = next(iter(self._ledger_rows(session, mission, limit=1, run_id=request.run_id)), None)
            if entry is None:
                raise QfError(
                    "RUN_EVIDENCE_NOT_AVAILABLE",
                    "Only scoped Discovery run evidence is available to this Mission.",
                    404,
                )
            item = self._ledger_item(entry)
            if item is None:
                raise QfError(
                    "RUN_EVIDENCE_FORBIDDEN",
                    "Run evidence contains fields unavailable to this Mission.",
                    403,
                )
            return item

    def _submit_artifact(self, request: _SubmitArtifactRequest) -> dict[str, Any]:
        with self._session_factory() as session, session.begin():
            mission = self._mission(
                session,
                MissionTool.SUBMIT_MISSION_ARTIFACT,
                request.mission_id,
                lock=True,
            )
            if request.artifact.mission_id != mission.id:
                raise QfError("MISSION_SCOPE_FORBIDDEN", "Artifact belongs to another Mission.", 403)
            if request.artifact.kind not in self._contract.expected_output_schemas:
                raise QfError(
                    "MISSION_ARTIFACT_KIND_FORBIDDEN",
                    "Artifact kind is not required by the frozen Mission contract.",
                    422,
                )
            payload = _safe_json(request.artifact.payload)
            summary = request.artifact.summary
            if request.artifact.kind is DraftArtifactKind.ALPHA_PROPOSAL:
                try:
                    payload = AlphaArtifactDraftV1.model_validate(payload).model_dump(mode="json")
                    summary = validate_alpha_artifact_summary(summary)
                except ValueError as exc:
                    raise QfError(
                        "ALPHA_ARTIFACT_DRAFT_INVALID",
                        "Alpha proposals must use the bounded public AlphaArtifactDraftV1 contract.",
                        422,
                    ) from exc
            normalized = request.model_dump(mode="json")
            normalized["artifact"]["summary"] = summary
            normalized["artifact"]["payload"] = payload
            existing = session.get(PublicMutationReceipt, request.idempotency_key)
            operation = f"mission-artifact.submit:{mission.id}"
            if existing is not None:
                if existing.operation_name != operation or existing.normalized_request != normalized:
                    raise QfError(
                        "IDEMPOTENCY_KEY_REUSED",
                        "The idempotency key belongs to a different Mission artifact request.",
                        409,
                    )
                return existing.response_json
            if request.expected_revision != mission.revision:
                raise QfError(
                    "MISSION_REVISION_CONFLICT",
                    "Mission has changed; refresh its revision before submitting an artifact.",
                    409,
                    {"expected_revision": request.expected_revision, "actual_revision": mission.revision},
                )
            revision = int(
                session.scalar(
                    select(func.coalesce(func.max(MissionArtifact.revision), 0)).where(
                        MissionArtifact.mission_id == mission.id,
                        MissionArtifact.kind == request.artifact.kind.value,
                    )
                )
                or 0
            ) + 1
            turn = session.scalar(
                select(AgentTurn)
                .join(AgentSession)
                .where(
                    AgentSession.mission_id == mission.id,
                    AgentTurn.state == "RUNNING",
                )
                .order_by(AgentTurn.ordinal.desc())
                .limit(1)
            )
            artifact = MissionArtifact(
                id=uuid4(),
                mission_id=mission.id,
                turn_id=turn.id if turn else None,
                kind=request.artifact.kind.value,
                schema_version=request.artifact.schema_version,
                revision=revision,
                state="DRAFT",
                storage_uri="",
                metadata_json={"summary": summary, "payload": payload},
                created_at=datetime.now(UTC),
            )
            artifact.storage_uri = f"db://mission-artifacts/{artifact.id}"
            session.add(artifact)
            session.flush()
            if turn is not None:
                turn.output_artifact_ids = [*turn.output_artifact_ids, str(artifact.id)]
            mission.revision += 1
            result = {
                "artifact_id": str(artifact.id),
                "mission_id": str(mission.id),
                "kind": artifact.kind,
                "revision": artifact.revision,
                "state": artifact.state,
                "storage_uri": artifact.storage_uri,
                "mission_revision": mission.revision,
            }
            session.add(
                PublicMutationReceipt(
                    idempotency_key=request.idempotency_key,
                    operation_name=operation,
                    normalized_request=normalized,
                    response_json=result,
                    status_code=201,
                    created_at=datetime.now(UTC),
                )
            )
            session.add(
                Event(
                    kind="MISSION_ARTIFACT_SUBMITTED",
                    aggregate_type="RESEARCH_PROGRAM",
                    aggregate_id=mission.program_id,
                    actor_kind="AGENT",
                    actor_metadata={},
                    payload={
                        "mission_id": str(mission.id),
                        "artifact_id": str(artifact.id),
                        "kind": artifact.kind,
                        "revision": artifact.revision,
                    },
                )
            )
            return result


class _CapabilityRequestHandler(socketserver.StreamRequestHandler):
    def handle(self) -> None:
        server = cast("_CapabilityServer", self.server)
        raw = self.rfile.readline(_MAX_RPC_BYTES + 1)
        if not raw or len(raw) > _MAX_RPC_BYTES:
            self._write_error("MISSION_RPC_INVALID", "Mission capability request is invalid.")
            return
        try:
            request = json.loads(raw)
            if (
                not isinstance(request, dict)
                or not isinstance(request.get("tool"), str)
                or not isinstance(request.get("arguments"), dict)
            ):
                raise ValueError("invalid request")
            result = server.service.invoke(request["tool"], request["arguments"])
            self._write({"ok": True, "result": result})
        except QfError as exc:
            self._write_error(exc.code, exc.message)
        except (TypeError, ValueError, json.JSONDecodeError):
            self._write_error("MISSION_RPC_INVALID", "Mission capability request is invalid.")

    def _write_error(self, code: str, message: str) -> None:
        self._write({"ok": False, "error": {"code": code, "message": message}})

    def _write(self, payload: dict[str, Any]) -> None:
        encoded = json.dumps(payload, separators=(",", ":"), allow_nan=False).encode("utf-8")
        if len(encoded) > _MAX_RPC_BYTES:
            encoded = b'{"ok":false,"error":{"code":"MISSION_RPC_TOO_LARGE","message":"Mission capability response is too large."}}'
        self.wfile.write(encoded + b"\n")
        self.wfile.flush()


class _CapabilityServer(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True
    block_on_close = False

    def __init__(self, path: str, service: MissionCapabilityService) -> None:
        self.service = service
        super().__init__(path, _CapabilityRequestHandler)


class MissionCapabilityBroker:
    """Own a private Unix socket for one running Mission worker."""

    def __init__(self, contract: MissionContractV1, session_factory: SessionFactory) -> None:
        self._service = MissionCapabilityService(contract, session_factory)
        self._root: Path | None = None
        self._server: _CapabilityServer | None = None
        self._thread: threading.Thread | None = None

    @property
    def socket_path(self) -> Path:
        if self._root is None:
            raise RuntimeError("Mission capability broker has not started")
        return self._root / "m.sock"

    def __enter__(self) -> "MissionCapabilityBroker":
        root = Path(tempfile.mkdtemp(prefix="qz-mcp-"))
        os.chmod(root, 0o700)
        server = _CapabilityServer(str(root / "m.sock"), self._service)
        os.chmod(root / "m.sock", 0o600)
        thread = threading.Thread(target=server.serve_forever, name="mission-capabilities", daemon=True)
        thread.start()
        self._root = root
        self._server = server
        self._thread = thread
        return self

    def __exit__(self, *_: object) -> None:
        if self._server is not None:
            self._server.shutdown()
            self._server.server_close()
        if self._thread is not None:
            self._thread.join(timeout=2.0)
        if self._root is not None:
            try:
                self.socket_path.unlink()
            except FileNotFoundError:
                pass
            self._root.rmdir()


class MissionCapabilityClient:
    """The MCP child-side client; it holds only a scoped socket pathname."""

    def __init__(self, socket_path: Path) -> None:
        self._socket_path = socket_path

    def call(self, tool: MissionTool, arguments: dict[str, Any]) -> dict[str, Any]:
        request = json.dumps(
            {"tool": tool.value, "arguments": arguments}, separators=(",", ":"), allow_nan=False
        ).encode("utf-8")
        if len(request) > _MAX_RPC_BYTES:
            raise MissionCapabilityError("MISSION_RPC_TOO_LARGE", "Mission capability request is too large.")
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
                connection.settimeout(5.0)
                connection.connect(str(self._socket_path))
                connection.sendall(request + b"\n")
                response = bytearray()
                while len(response) <= _MAX_RPC_BYTES:
                    chunk = connection.recv(min(4096, _MAX_RPC_BYTES + 1 - len(response)))
                    if not chunk:
                        break
                    response.extend(chunk)
                    if b"\n" in chunk:
                        break
        except OSError as exc:
            raise MissionCapabilityError(
                "MISSION_CAPABILITY_UNAVAILABLE", "Mission capability bridge is unavailable."
            ) from exc
        if not response or len(response) > _MAX_RPC_BYTES:
            raise MissionCapabilityError("MISSION_RPC_INVALID", "Mission capability response is invalid.")
        try:
            payload = json.loads(bytes(response).split(b"\n", 1)[0])
            if not isinstance(payload, dict):
                raise ValueError("invalid response")
        except (ValueError, json.JSONDecodeError) as exc:
            raise MissionCapabilityError("MISSION_RPC_INVALID", "Mission capability response is invalid.") from exc
        if payload.get("ok") is not True:
            error = payload.get("error")
            if not isinstance(error, dict):
                raise MissionCapabilityError("MISSION_RPC_INVALID", "Mission capability response is invalid.")
            code, message = error.get("code"), error.get("message")
            if not isinstance(code, str) or not isinstance(message, str):
                raise MissionCapabilityError("MISSION_RPC_INVALID", "Mission capability response is invalid.")
            raise MissionCapabilityError(code, message)
        result = payload.get("result")
        if not isinstance(result, dict):
            raise MissionCapabilityError("MISSION_RPC_INVALID", "Mission capability response is invalid.")
        return result
