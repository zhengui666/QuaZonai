"""Research lifecycle, experiment, and run API."""

from __future__ import annotations

from datetime import UTC, datetime
from typing import Any
from uuid import UUID

from fastapi import APIRouter, Depends, Query
from pydantic import BaseModel, ConfigDict, Field, model_validator
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from quantfoundry.api.dependencies import get_session
from quantfoundry.db.models import (
    CatalogDataset,
    Experiment,
    PluginRuntimeBundle,
    Report,
    ResearchCase,
    ResearchSectionRevision,
    Run,
    StrategyVersion,
)
from quantfoundry.errors import QfError
from quantfoundry.events import append_event
from quantfoundry.jobs import enqueue_job

router = APIRouter(prefix="/api/v1", tags=["research"])

SECTIONS = (
    "HYPOTHESIS",
    "MARKET_CONTEXT",
    "DATA",
    "METHOD",
    "RESULTS",
    "RISKS",
    "CONCLUSION",
)


class ResearchCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")

    title: str = Field(min_length=1, max_length=300)
    strategy_version_id: UUID | None = None


class ResearchPatch(BaseModel):
    model_config = ConfigDict(extra="forbid")

    title: str | None = Field(default=None, min_length=1, max_length=300)
    strategy_version_id: UUID | None = None

    @model_validator(mode="after")
    def non_empty(self) -> "ResearchPatch":
        if self.title is None and self.strategy_version_id is None:
            raise ValueError("at least one research field must be supplied")
        return self


class SectionWrite(BaseModel):
    model_config = ConfigDict(extra="forbid")

    section: str
    markdown: str = Field(min_length=1, max_length=1_000_000)


class SectionView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    section: str
    revision_no: int
    markdown: str
    created_at: str


class ResearchView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    title: str
    strategy_version_id: UUID | None
    state: str
    content_revision: int
    sections: dict[str, SectionView]


class ExperimentCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")

    dataset_id: UUID
    runtime_bundle_id: UUID
    train_start: datetime
    train_end: datetime
    holdout_start: datetime
    holdout_end: datetime
    seed: int = Field(ge=0, le=2_147_483_647)

    @model_validator(mode="after")
    def valid_ranges(self) -> "ExperimentCreate":
        for field_name in ("train_start", "train_end", "holdout_start", "holdout_end"):
            value = getattr(self, field_name)
            if value.tzinfo is None or value.utcoffset() is None:
                raise ValueError(f"{field_name} must include a timezone")
        if not self.train_start < self.train_end:
            raise ValueError("train_start must be before train_end")
        if not self.holdout_start < self.holdout_end:
            raise ValueError("holdout_start must be before holdout_end")
        if not (
            self.train_end <= self.holdout_start or self.holdout_end <= self.train_start
        ):
            raise ValueError("training and holdout ranges must not overlap")
        return self


class ExperimentView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    research_id: UUID
    strategy_version_id: UUID
    dataset_id: UUID
    runtime_bundle_id: UUID
    train_start: str
    train_end: str
    holdout_start: str
    holdout_end: str
    seed: int
    objective_directions: list[str]
    selected_trial_no: int | None


class RunView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    experiment_id: UUID | None
    runtime_bundle_id: UUID | None
    type: str
    state: str
    attempt: int
    summary: dict[str, Any]
    error_code: str | None
    error_message: str | None
    started_at: str | None
    finished_at: str | None


class ReportView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    run_id: UUID
    kind: str
    relative_path: str
    media_type: str
    row_count: int | None
    created_at: str


def _latest_sections(session: Session, research_id: UUID) -> dict[str, SectionView]:
    items = list(
        session.scalars(
            select(ResearchSectionRevision)
            .where(ResearchSectionRevision.research_id == research_id)
            .order_by(
                ResearchSectionRevision.section.asc(),
                ResearchSectionRevision.revision_no.asc(),
            )
        )
    )
    latest: dict[str, ResearchSectionRevision] = {}
    for item in items:
        latest[item.section] = item
    return {
        name: SectionView(
            id=item.id,
            section=item.section,
            revision_no=item.revision_no,
            markdown=item.markdown,
            created_at=item.created_at.isoformat(),
        )
        for name, item in latest.items()
    }


def _research_view(session: Session, item: ResearchCase) -> ResearchView:
    return ResearchView(
        id=item.id,
        title=item.title,
        strategy_version_id=item.strategy_version_id,
        state=item.state,
        content_revision=item.content_revision,
        sections=_latest_sections(session, item.id),
    )


def _experiment_view(item: Experiment) -> ExperimentView:
    return ExperimentView(
        id=item.id,
        research_id=item.research_id,
        strategy_version_id=item.strategy_version_id,
        dataset_id=item.dataset_id,
        runtime_bundle_id=item.runtime_bundle_id,
        train_start=item.train_start.isoformat(),
        train_end=item.train_end.isoformat(),
        holdout_start=item.holdout_start.isoformat(),
        holdout_end=item.holdout_end.isoformat(),
        seed=item.seed,
        objective_directions=item.objective_directions,
        selected_trial_no=item.selected_trial_no,
    )


def _run_view(item: Run) -> RunView:
    return RunView(
        id=item.id,
        experiment_id=item.experiment_id,
        runtime_bundle_id=item.runtime_bundle_id,
        type=item.type,
        state=item.state,
        attempt=item.attempt,
        summary=item.summary,
        error_code=item.error_code,
        error_message=item.error_message,
        started_at=item.started_at.isoformat() if item.started_at else None,
        finished_at=item.finished_at.isoformat() if item.finished_at else None,
    )


def _strategy_version(session: Session, version_id: UUID | None) -> StrategyVersion | None:
    if version_id is None:
        return None
    item = session.get(StrategyVersion, version_id)
    if item is None:
        raise QfError("STRATEGY_VERSION_UNKNOWN", "Strategy version does not exist.", 404)
    return item


@router.get("/research-cases", response_model=list[ResearchView])
def list_research(session: Session = Depends(get_session)) -> list[ResearchView]:
    items = list(session.scalars(select(ResearchCase).order_by(ResearchCase.created_at.asc())))
    return [_research_view(session, item) for item in items]


@router.post("/research-cases", response_model=ResearchView, status_code=201)
def create_research(
    payload: ResearchCreate,
    session: Session = Depends(get_session),
) -> ResearchView:
    with session.begin():
        _strategy_version(session, payload.strategy_version_id)
        item = ResearchCase(
            title=payload.title.strip(),
            strategy_version_id=payload.strategy_version_id,
            state="DRAFT",
        )
        session.add(item)
        session.flush()
        append_event(
            session,
            kind="RESEARCH_CREATED",
            aggregate_type="research",
            aggregate_id=item.id,
            payload={
                "strategy_version_id": (
                    str(item.strategy_version_id) if item.strategy_version_id else None
                )
            },
            actor_kind="LOCAL_OPERATOR",
        )
    return _research_view(session, item)


@router.get("/research-cases/{research_id}", response_model=ResearchView)
def show_research(
    research_id: UUID,
    session: Session = Depends(get_session),
) -> ResearchView:
    item = session.get(ResearchCase, research_id)
    if item is None:
        raise QfError("RESEARCH_UNKNOWN", "Research case does not exist.", 404)
    return _research_view(session, item)


@router.patch("/research-cases/{research_id}", response_model=ResearchView)
def patch_research(
    research_id: UUID,
    payload: ResearchPatch,
    session: Session = Depends(get_session),
) -> ResearchView:
    with session.begin():
        item = session.execute(
            select(ResearchCase).where(ResearchCase.id == research_id).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("RESEARCH_UNKNOWN", "Research case does not exist.", 404)
        if item.state != "DRAFT":
            raise QfError(
                "RESEARCH_INVALID_STATE",
                "Research identity fields can only change in DRAFT.",
                409,
                {"state": item.state},
            )
        if payload.title is not None:
            item.title = payload.title.strip()
        if payload.strategy_version_id is not None:
            _strategy_version(session, payload.strategy_version_id)
            item.strategy_version_id = payload.strategy_version_id
        item.content_revision += 1
        append_event(
            session,
            kind="RESEARCH_UPDATED",
            aggregate_type="research",
            aggregate_id=item.id,
            payload={"content_revision": item.content_revision},
            actor_kind="LOCAL_OPERATOR",
        )
    return _research_view(session, item)


@router.post(
    "/research-cases/{research_id}/sections",
    response_model=SectionView,
    status_code=201,
)
def set_research_section(
    research_id: UUID,
    payload: SectionWrite,
    session: Session = Depends(get_session),
) -> SectionView:
    section = payload.section.upper()
    if section not in SECTIONS:
        raise QfError(
            "RESEARCH_SECTION_INVALID",
            "Research section is not part of the fixed seven-section contract.",
            422,
            {"allowed": list(SECTIONS)},
        )
    with session.begin():
        research = session.execute(
            select(ResearchCase).where(ResearchCase.id == research_id).with_for_update()
        ).scalar_one_or_none()
        if research is None:
            raise QfError("RESEARCH_UNKNOWN", "Research case does not exist.", 404)
        if research.state not in {"DRAFT", "ACTIVE"}:
            raise QfError(
                "RESEARCH_INVALID_STATE",
                "Research sections can only change in DRAFT or ACTIVE.",
                409,
                {"state": research.state},
            )
        latest = session.scalar(
            select(func.max(ResearchSectionRevision.revision_no)).where(
                ResearchSectionRevision.research_id == research_id,
                ResearchSectionRevision.section == section,
            )
        )
        item = ResearchSectionRevision(
            research_id=research_id,
            section=section,
            revision_no=int(latest or 0) + 1,
            markdown=payload.markdown.strip(),
        )
        session.add(item)
        research.content_revision += 1
        session.flush()
        append_event(
            session,
            kind="RESEARCH_SECTION_REVISED",
            aggregate_type="research",
            aggregate_id=research.id,
            payload={
                "section": section,
                "revision_no": item.revision_no,
                "content_revision": research.content_revision,
            },
            actor_kind="LOCAL_OPERATOR",
        )
    return SectionView(
        id=item.id,
        section=item.section,
        revision_no=item.revision_no,
        markdown=item.markdown,
        created_at=item.created_at.isoformat(),
    )


@router.post("/research-cases/{research_id}/activate", response_model=ResearchView)
def activate_research(
    research_id: UUID,
    session: Session = Depends(get_session),
) -> ResearchView:
    with session.begin():
        item = session.execute(
            select(ResearchCase).where(ResearchCase.id == research_id).with_for_update()
        ).scalar_one_or_none()
        if item is None:
            raise QfError("RESEARCH_UNKNOWN", "Research case does not exist.", 404)
        if item.state != "DRAFT":
            raise QfError(
                "RESEARCH_INVALID_STATE",
                "Only DRAFT research can be activated.",
                409,
                {"state": item.state},
            )
        _strategy_version(session, item.strategy_version_id)
        latest = _latest_sections(session, item.id)
        missing = [
            name
            for name in SECTIONS
            if name not in latest or not latest[name].markdown.strip()
        ]
        if missing:
            raise QfError(
                "RESEARCH_INCOMPLETE",
                "All seven research sections are required before activation.",
                422,
                {"missing_sections": missing},
            )
        item.state = "ACTIVE"
        append_event(
            session,
            kind="RESEARCH_ACTIVATED",
            aggregate_type="research",
            aggregate_id=item.id,
            payload={"content_revision": item.content_revision},
            actor_kind="LOCAL_OPERATOR",
        )
    return _research_view(session, item)


@router.post(
    "/research-cases/{research_id}/experiments",
    response_model=ExperimentView,
    status_code=201,
)
def create_experiment(
    research_id: UUID,
    payload: ExperimentCreate,
    session: Session = Depends(get_session),
) -> ExperimentView:
    with session.begin():
        research = session.get(ResearchCase, research_id)
        if research is None:
            raise QfError("RESEARCH_UNKNOWN", "Research case does not exist.", 404)
        if research.state != "ACTIVE":
            raise QfError(
                "RESEARCH_INVALID_STATE",
                "Experiments require ACTIVE research.",
                409,
                {"state": research.state},
            )
        strategy = _strategy_version(session, research.strategy_version_id)
        assert strategy is not None
        dataset = session.get(CatalogDataset, payload.dataset_id)
        if dataset is None or dataset.state != "READY":
            raise QfError(
                "DATASET_NOT_READY",
                "Experiment dataset must exist and be READY.",
                422,
            )
        bundle = session.get(PluginRuntimeBundle, payload.runtime_bundle_id)
        if bundle is None or bundle.state != "READY":
            raise QfError(
                "PLUGIN_RUNTIME_UNAVAILABLE",
                "Experiment runtime bundle must be READY.",
                503,
            )
        item = Experiment(
            research_id=research.id,
            strategy_version_id=strategy.id,
            dataset_id=dataset.id,
            runtime_bundle_id=bundle.id,
            train_start=payload.train_start.astimezone(UTC),
            train_end=payload.train_end.astimezone(UTC),
            holdout_start=payload.holdout_start.astimezone(UTC),
            holdout_end=payload.holdout_end.astimezone(UTC),
            seed=payload.seed,
            objective_directions=strategy.objective_directions,
        )
        session.add(item)
        session.flush()
        append_event(
            session,
            kind="EXPERIMENT_CREATED",
            aggregate_type="experiment",
            aggregate_id=item.id,
            payload={"research_id": str(research.id), "dataset_id": str(dataset.id)},
            actor_kind="LOCAL_OPERATOR",
        )
    return _experiment_view(item)


@router.get("/experiments/{experiment_id}", response_model=ExperimentView)
def show_experiment(
    experiment_id: UUID,
    session: Session = Depends(get_session),
) -> ExperimentView:
    item = session.get(Experiment, experiment_id)
    if item is None:
        raise QfError("EXPERIMENT_UNKNOWN", "Experiment does not exist.", 404)
    return _experiment_view(item)


@router.post("/experiments/{experiment_id}/start", response_model=RunView, status_code=202)
def start_experiment(
    experiment_id: UUID,
    session: Session = Depends(get_session),
) -> RunView:
    with session.begin():
        experiment = session.execute(
            select(Experiment).where(Experiment.id == experiment_id).with_for_update()
        ).scalar_one_or_none()
        if experiment is None:
            raise QfError("EXPERIMENT_UNKNOWN", "Experiment does not exist.", 404)
        research = session.get(ResearchCase, experiment.research_id)
        if research is None or research.state != "ACTIVE":
            raise QfError(
                "RESEARCH_INVALID_STATE",
                "Experiment can only start while research is ACTIVE.",
                409,
            )
        active = session.scalar(
            select(func.count())
            .select_from(Run)
            .where(
                Run.experiment_id == experiment.id,
                Run.state.in_(["QUEUED", "RUNNING"]),
            )
        )
        if active:
            raise QfError(
                "OPERATION_IN_PROGRESS",
                "Experiment already has queued or running work.",
                409,
            )
        run = Run(
            experiment_id=experiment.id,
            runtime_bundle_id=experiment.runtime_bundle_id,
            type="OPTIMIZATION",
            state="QUEUED",
            summary={
                "trial_count": 100,
                "population_size": 20,
                "max_parallel_processes": 4,
            },
        )
        session.add(run)
        session.flush()
        enqueue_job(
            session,
            kind="OPTIMIZATION",
            resource_type="run",
            resource_id=run.id,
        )
        append_event(
            session,
            kind="OPTIMIZATION_QUEUED",
            aggregate_type="run",
            aggregate_id=run.id,
            payload={"experiment_id": str(experiment.id)},
            actor_kind="LOCAL_OPERATOR",
        )
    return _run_view(run)


@router.get("/runs", response_model=list[RunView])
def list_runs(
    experiment_id: UUID | None = Query(default=None),
    session: Session = Depends(get_session),
) -> list[RunView]:
    query = select(Run).order_by(Run.id.asc())
    if experiment_id is not None:
        query = query.where(Run.experiment_id == experiment_id)
    return [_run_view(item) for item in session.scalars(query)]


@router.get("/runs/{run_id}", response_model=RunView)
def show_run(run_id: UUID, session: Session = Depends(get_session)) -> RunView:
    item = session.get(Run, run_id)
    if item is None:
        raise QfError("RUN_UNKNOWN", "Run does not exist.", 404)
    return _run_view(item)


@router.get("/runs/{run_id}/reports", response_model=list[ReportView])
def list_reports(
    run_id: UUID,
    session: Session = Depends(get_session),
) -> list[ReportView]:
    if session.get(Run, run_id) is None:
        raise QfError("RUN_UNKNOWN", "Run does not exist.", 404)
    items = list(
        session.scalars(
            select(Report).where(Report.run_id == run_id).order_by(Report.kind.asc())
        )
    )
    return [
        ReportView(
            id=item.id,
            run_id=item.run_id,
            kind=item.kind,
            relative_path=item.relative_path,
            media_type=item.media_type,
            row_count=item.row_count,
            created_at=item.created_at.isoformat(),
        )
        for item in items
    ]
