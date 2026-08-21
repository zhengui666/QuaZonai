"""Human approvals, Deployments, Stop/Restart, and Universe revisions."""

from __future__ import annotations

from datetime import UTC, datetime
from typing import Any, Literal
from uuid import UUID

from fastapi import APIRouter, Depends
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from api.dependencies import get_session
from db.models import (
    Approval,
    DataSource,
    Deployment,
    DeploymentUniverseRevision,
    ExecutionConnection,
    Experiment,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
    ResearchCase,
    RiskAccount,
    Run,
)
from errors import QfError
from events import append_event

router = APIRouter(prefix="/api/v1", tags=["deployments"])


class DeploymentCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")

    research_id: UUID
    data_source_id: UUID
    execution_connection_id: UUID
    runtime_bundle_id: UUID
    funder_id: str = Field(min_length=1, max_length=300)
    universe_predicate: dict[str, Any] = Field(default_factory=dict)
    universe_cap: int = Field(default=100, ge=1, le=100)


class DeploymentView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    research_id: UUID
    strategy_version_id: UUID
    data_source_id: UUID
    execution_connection_id: UUID
    runtime_bundle_id: UUID
    funder_id: str
    desired_state: str
    observed_state: str
    active_revision_id: UUID | None
    generation: int
    last_error: str | None


class ApprovalView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    type: str
    resource_type: str
    resource_id: UUID
    scope: dict[str, Any]
    state: str
    reason: str | None
    created_at: str
    decided_at: str | None


class Decision(BaseModel):
    model_config = ConfigDict(extra="forbid")

    reason: str = Field(min_length=1, max_length=10_000)


class UniverseRevisionCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")

    predicate: dict[str, Any]
    cap: int = Field(ge=1, le=100)
    change_kind: Literal["NARROWING", "EXPANSION"]


class UniverseRevisionView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    deployment_id: UUID
    revision_no: int
    predicate: dict[str, Any]
    cap: int
    state: str
    approval_id: UUID | None


def _deployment_view(item: Deployment) -> DeploymentView:
    return DeploymentView.model_validate(item, from_attributes=True)


def _approval_view(item: Approval) -> ApprovalView:
    return ApprovalView(
        id=item.id,
        type=item.type,
        resource_type=item.resource_type,
        resource_id=item.resource_id,
        scope=item.scope,
        state=item.state,
        reason=item.reason,
        created_at=item.created_at.isoformat(),
        decided_at=item.decided_at.isoformat() if item.decided_at else None,
    )


def _revision_view(item: DeploymentUniverseRevision) -> UniverseRevisionView:
    return UniverseRevisionView.model_validate(item, from_attributes=True)


def _bundle_contains(
    session: Session,
    bundle_id: UUID,
    release_ids: set[UUID],
) -> bool:
    members = set(
        session.scalars(
            select(PluginRuntimeBundleMember.plugin_release_id).where(
                PluginRuntimeBundleMember.runtime_bundle_id == bundle_id
            )
        )
    )
    return release_ids.issubset(members)


def _holdout_for_research(session: Session, research_id: UUID) -> Run:
    run = session.execute(
        select(Run)
        .join(Experiment, Experiment.id == Run.experiment_id)
        .where(
            Experiment.research_id == research_id,
            Run.type == "HOLDOUT",
            Run.state == "SUCCEEDED",
        )
        .order_by(Run.finished_at.desc())
        .limit(1)
    ).scalar_one_or_none()
    if run is None:
        raise QfError(
            "HOLDOUT_REQUIRED",
            "A successful single Holdout is required before Deployment creation.",
            409,
        )
    return run


def _pending_start_approval(session: Session, deployment_id: UUID) -> Approval | None:
    return session.execute(
        select(Approval).where(
            Approval.type == "DEPLOYMENT_START",
            Approval.resource_type == "deployment",
            Approval.resource_id == deployment_id,
            Approval.state == "PENDING",
        )
    ).scalar_one_or_none()


@router.get("/approvals", response_model=list[ApprovalView])
def list_approvals(session: Session = Depends(get_session)) -> list[ApprovalView]:
    return [
        _approval_view(item)
        for item in session.scalars(select(Approval).order_by(Approval.created_at.asc()))
    ]


@router.get("/approvals/{approval_id}", response_model=ApprovalView)
def show_approval(
    approval_id: UUID,
    session: Session = Depends(get_session),
) -> ApprovalView:
    item = session.get(Approval, approval_id)
    if item is None:
        raise QfError("APPROVAL_UNKNOWN", "Approval does not exist.", 404)
    return _approval_view(item)


@router.post("/approvals/{approval_id}/approve", response_model=ApprovalView)
def approve(
    approval_id: UUID,
    decision: Decision,
    session: Session = Depends(get_session),
) -> ApprovalView:
    with session.begin():
        item = session.execute(
            select(Approval).where(Approval.id == approval_id).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("APPROVAL_UNKNOWN", "Approval does not exist.", 404)
        if item.state != "PENDING":
            raise QfError(
                "APPROVAL_INVALID_STATE",
                "Only PENDING approvals can be decided.",
                409,
                {"state": item.state},
            )
        if item.type == "DEPLOYMENT_START":
            deployment = session.execute(
                select(Deployment)
                .where(Deployment.id == item.resource_id)
                .with_for_update()
            ).scalar_one()
            research = session.execute(
                select(ResearchCase)
                .where(ResearchCase.id == deployment.research_id)
                .with_for_update()
            ).scalar_one()
            if research.state not in {"REVIEW", "CLOSED"}:
                raise QfError(
                    "RESEARCH_INVALID_STATE",
                    "Deployment start approval requires REVIEW or previously CLOSED research.",
                    409,
                    {"state": research.state},
                )
            deployment.desired_state = "RUNNING"
            deployment.observed_state = "STARTING"
            deployment.last_error = None
            if research.state == "REVIEW":
                research.state = "CLOSED"
            initial_revision_id = item.scope.get("universe_revision_id")
            if initial_revision_id:
                revision = session.get(DeploymentUniverseRevision, UUID(initial_revision_id))
                if revision is not None:
                    revision.state = "APPROVED"
                    deployment.active_revision_id = revision.id
        elif item.type == "UNIVERSE_EXPANSION":
            revision = session.execute(
                select(DeploymentUniverseRevision)
                .where(DeploymentUniverseRevision.id == item.resource_id)
                .with_for_update()
            ).scalar_one()
            deployment = session.execute(
                select(Deployment)
                .where(Deployment.id == revision.deployment_id)
                .with_for_update()
            ).scalar_one()
            revision.state = "APPROVED"
            deployment.active_revision_id = revision.id
            if deployment.desired_state == "RUNNING":
                deployment.observed_state = "STARTING"
        else:
            raise QfError("APPROVAL_INVALID", "Approval type is unsupported.", 422)
        item.state = "APPROVED"
        item.reason = decision.reason.strip()
        item.decided_at = datetime.now(UTC)
        append_event(
            session,
            kind="APPROVAL_APPROVED",
            aggregate_type="approval",
            aggregate_id=item.id,
            payload={"type": item.type, "resource_id": str(item.resource_id)},
            actor_kind="LOCAL_OPERATOR",
        )
    return _approval_view(item)


@router.post("/approvals/{approval_id}/reject", response_model=ApprovalView)
def reject(
    approval_id: UUID,
    decision: Decision,
    session: Session = Depends(get_session),
) -> ApprovalView:
    with session.begin():
        item = session.execute(
            select(Approval).where(Approval.id == approval_id).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("APPROVAL_UNKNOWN", "Approval does not exist.", 404)
        if item.state != "PENDING":
            raise QfError(
                "APPROVAL_INVALID_STATE",
                "Only PENDING approvals can be decided.",
                409,
                {"state": item.state},
            )
        if item.type == "DEPLOYMENT_START":
            deployment = session.get(Deployment, item.resource_id)
            if deployment is not None:
                research = session.get(ResearchCase, deployment.research_id)
                if research is not None and research.state == "REVIEW":
                    research.state = "ACTIVE"
        elif item.type == "UNIVERSE_EXPANSION":
            revision = session.get(DeploymentUniverseRevision, item.resource_id)
            if revision is not None:
                revision.state = "REJECTED"
        item.state = "REJECTED"
        item.reason = decision.reason.strip()
        item.decided_at = datetime.now(UTC)
        append_event(
            session,
            kind="APPROVAL_REJECTED",
            aggregate_type="approval",
            aggregate_id=item.id,
            payload={"type": item.type, "resource_id": str(item.resource_id)},
            actor_kind="LOCAL_OPERATOR",
        )
    return _approval_view(item)


@router.get("/deployments", response_model=list[DeploymentView])
def list_deployments(session: Session = Depends(get_session)) -> list[DeploymentView]:
    return [
        _deployment_view(item)
        for item in session.scalars(select(Deployment).order_by(Deployment.created_at.asc()))
    ]


@router.get("/deployments/{deployment_id}", response_model=DeploymentView)
def show_deployment(
    deployment_id: UUID,
    session: Session = Depends(get_session),
) -> DeploymentView:
    item = session.get(Deployment, deployment_id)
    if item is None:
        raise QfError("DEPLOYMENT_UNKNOWN", "Deployment does not exist.", 404)
    return _deployment_view(item)


@router.post("/deployments", response_model=dict[str, Any], status_code=201)
def create_deployment(
    payload: DeploymentCreate,
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    with session.begin():
        research = session.execute(
            select(ResearchCase)
            .where(ResearchCase.id == payload.research_id)
            .with_for_update()
        ).scalar_one_or_none()
        if research is None:
            raise QfError("RESEARCH_UNKNOWN", "Research case does not exist.", 404)
        if research.state != "REVIEW" or research.strategy_version_id is None:
            raise QfError(
                "RESEARCH_INVALID_STATE",
                "Deployment creation requires Research in REVIEW.",
                409,
                {"state": research.state},
            )
        holdout = _holdout_for_research(session, research.id)
        data_source = session.get(DataSource, payload.data_source_id)
        execution = session.get(ExecutionConnection, payload.execution_connection_id)
        bundle = session.get(PluginRuntimeBundle, payload.runtime_bundle_id)
        if (
            data_source is None
            or execution is None
            or data_source.state != "ACTIVE"
            or execution.state != "ACTIVE"
        ):
            raise QfError(
                "LIVE_START_FAILED",
                "Deployment integrations must exist and be ACTIVE.",
                409,
            )
        if bundle is None or bundle.state != "READY":
            raise QfError(
                "PLUGIN_RUNTIME_UNAVAILABLE",
                "Deployment runtime bundle must be READY.",
                503,
            )
        data_release = session.get(PluginRelease, data_source.plugin_release_id)
        execution_release = session.get(PluginRelease, execution.plugin_release_id)
        if data_release is None or execution_release is None:
            raise QfError("PLUGIN_RUNTIME_UNAVAILABLE", "Plugin release is missing.", 503)
        if data_release.descriptor_snapshot.get(
            "compatibility_key"
        ) != execution_release.descriptor_snapshot.get("compatibility_key"):
            raise QfError(
                "DATA_EXEC_INCOMPATIBLE",
                "Data and execution plugins have different compatibility keys.",
                422,
            )
        if not _bundle_contains(
            session,
            bundle.id,
            {data_release.id, execution_release.id},
        ):
            raise QfError(
                "PLUGIN_RUNTIME_UNAVAILABLE",
                "Runtime bundle does not contain both integration plugin releases.",
                503,
            )
        item = Deployment(
            research_id=research.id,
            strategy_version_id=research.strategy_version_id,
            data_source_id=data_source.id,
            execution_connection_id=execution.id,
            runtime_bundle_id=bundle.id,
            funder_id=payload.funder_id.strip(),
            desired_state="CREATED",
            observed_state="CREATED",
        )
        session.add(item)
        session.flush()
        revision = DeploymentUniverseRevision(
            deployment_id=item.id,
            revision_no=1,
            predicate=payload.universe_predicate,
            cap=payload.universe_cap,
            state="PENDING",
        )
        session.add(revision)
        session.flush()
        approval = Approval(
            type="DEPLOYMENT_START",
            resource_type="deployment",
            resource_id=item.id,
            scope={
                "deployment_id": str(item.id),
                "research_id": str(research.id),
                "strategy_version_id": str(research.strategy_version_id),
                "holdout_run_id": str(holdout.id),
                "data_source_id": str(data_source.id),
                "data_plugin_release_id": str(data_release.id),
                "execution_connection_id": str(execution.id),
                "execution_plugin_release_id": str(execution_release.id),
                "runtime_bundle_id": str(bundle.id),
                "funder_id": item.funder_id,
                "universe_revision_id": str(revision.id),
                "universe_predicate": revision.predicate,
                "universe_cap": revision.cap,
                "per_order_limit_micros": 25_000_000,
                "funder_gross_limit_micros": 100_000_000,
            },
            state="PENDING",
        )
        session.add(approval)
        session.flush()
        revision.approval_id = approval.id
        if session.get(RiskAccount, item.funder_id) is None:
            session.add(
                RiskAccount(
                    funder_id=item.funder_id,
                    status="BLOCKED",
                    gross_limit_micros=100_000_000,
                )
            )
        append_event(
            session,
            kind="DEPLOYMENT_CREATED",
            aggregate_type="deployment",
            aggregate_id=item.id,
            payload={"approval_id": str(approval.id)},
            actor_kind="LOCAL_OPERATOR",
        )
    return {"deployment": _deployment_view(item), "approval": _approval_view(approval)}


@router.post("/deployments/{deployment_id}/stop", response_model=DeploymentView)
def stop_deployment(
    deployment_id: UUID,
    session: Session = Depends(get_session),
) -> DeploymentView:
    with session.begin():
        item = session.execute(
            select(Deployment).where(Deployment.id == deployment_id).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("DEPLOYMENT_UNKNOWN", "Deployment does not exist.", 404)
        item.desired_state = "STOPPED"
        if item.observed_state not in {"STOPPED", "CREATED"}:
            item.observed_state = "STOPPING"
        account = session.get(RiskAccount, item.funder_id)
        if account is not None:
            account.status = "STOPPED"
        append_event(
            session,
            kind="DEPLOYMENT_STOP_REQUESTED",
            aggregate_type="deployment",
            aggregate_id=item.id,
            payload={"positions_liquidated": False},
            actor_kind="LOCAL_OPERATOR",
        )
    return _deployment_view(item)


@router.post("/deployments/{deployment_id}/restart", response_model=ApprovalView, status_code=201)
def request_restart(
    deployment_id: UUID,
    session: Session = Depends(get_session),
) -> ApprovalView:
    with session.begin():
        item = session.execute(
            select(Deployment).where(Deployment.id == deployment_id).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("DEPLOYMENT_UNKNOWN", "Deployment does not exist.", 404)
        pending = _pending_start_approval(session, item.id)
        if pending is not None:
            raise QfError(
                "OPERATION_IN_PROGRESS",
                "Deployment already has a pending start approval.",
                409,
                {"approval_id": str(pending.id)},
            )
        approval = Approval(
            type="DEPLOYMENT_START",
            resource_type="deployment",
            resource_id=item.id,
            scope={
                "deployment_id": str(item.id),
                "restart_from_generation": item.generation,
                "strategy_version_id": str(item.strategy_version_id),
                "runtime_bundle_id": str(item.runtime_bundle_id),
                "active_revision_id": (
                    str(item.active_revision_id) if item.active_revision_id else None
                ),
                "funder_id": item.funder_id,
                "per_order_limit_micros": 25_000_000,
                "funder_gross_limit_micros": 100_000_000,
            },
            state="PENDING",
        )
        session.add(approval)
        session.flush()
        append_event(
            session,
            kind="DEPLOYMENT_RESTART_REQUESTED",
            aggregate_type="deployment",
            aggregate_id=item.id,
            payload={"approval_id": str(approval.id)},
            actor_kind="LOCAL_OPERATOR",
        )
    return _approval_view(approval)


@router.get(
    "/deployments/{deployment_id}/universe-revisions",
    response_model=list[UniverseRevisionView],
)
def list_universe_revisions(
    deployment_id: UUID,
    session: Session = Depends(get_session),
) -> list[UniverseRevisionView]:
    if session.get(Deployment, deployment_id) is None:
        raise QfError("DEPLOYMENT_UNKNOWN", "Deployment does not exist.", 404)
    return [
        _revision_view(item)
        for item in session.scalars(
            select(DeploymentUniverseRevision)
            .where(DeploymentUniverseRevision.deployment_id == deployment_id)
            .order_by(DeploymentUniverseRevision.revision_no.asc())
        )
    ]


@router.post(
    "/deployments/{deployment_id}/universe-revisions",
    response_model=dict[str, Any],
    status_code=201,
)
def create_universe_revision(
    deployment_id: UUID,
    payload: UniverseRevisionCreate,
    session: Session = Depends(get_session),
) -> dict[str, Any]:
    with session.begin():
        deployment = session.execute(
            select(Deployment).where(Deployment.id == deployment_id).with_for_update()
        ).scalar_one_or_none()
        if deployment is None:
            raise QfError("DEPLOYMENT_UNKNOWN", "Deployment does not exist.", 404)
        latest_no = session.scalar(
            select(func.max(DeploymentUniverseRevision.revision_no)).where(
                DeploymentUniverseRevision.deployment_id == deployment.id
            )
        )
        revision = DeploymentUniverseRevision(
            deployment_id=deployment.id,
            revision_no=int(latest_no or 0) + 1,
            predicate=payload.predicate,
            cap=payload.cap,
            state=("PENDING" if payload.change_kind == "EXPANSION" else "APPROVED"),
        )
        session.add(revision)
        session.flush()
        approval: Approval | None = None
        if payload.change_kind == "EXPANSION":
            approval = Approval(
                type="UNIVERSE_EXPANSION",
                resource_type="universe_revision",
                resource_id=revision.id,
                scope={
                    "deployment_id": str(deployment.id),
                    "revision_no": revision.revision_no,
                    "predicate": revision.predicate,
                    "cap": revision.cap,
                },
                state="PENDING",
            )
            session.add(approval)
            session.flush()
            revision.approval_id = approval.id
        else:
            deployment.active_revision_id = revision.id
            if deployment.desired_state == "RUNNING":
                deployment.observed_state = "STARTING"
        append_event(
            session,
            kind="UNIVERSE_REVISION_CREATED",
            aggregate_type="universe_revision",
            aggregate_id=revision.id,
            payload={
                "deployment_id": str(deployment.id),
                "change_kind": payload.change_kind,
                "approval_id": str(approval.id) if approval else None,
            },
            actor_kind="LOCAL_OPERATOR",
        )
    return {
        "revision": _revision_view(revision),
        "approval": _approval_view(approval) if approval else None,
    }
