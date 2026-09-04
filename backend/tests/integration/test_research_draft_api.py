from __future__ import annotations

from uuid import UUID

from fastapi.testclient import TestClient
from sqlalchemy import Engine, select

from db.models import Job, ResearchMission
from db.session import create_session_factory
from main import create_app
from settings import Settings


def _client(engine: Engine, settings: Settings) -> TestClient:
    return TestClient(create_app(settings=settings, engine=engine))


def test_idea_draft_freezes_a_charter_and_starts_a_bounded_mission_graph(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    universe = client.post(
        "/api/v1/universes",
        json={
            "universe_key": "US_EQUITIES",
            "name": "US Equities",
            "instrument_schema": {"instrument_id": "string"},
            "membership_rules": {"listing": "NYSE|NASDAQ"},
            "calendar_semantics": {"timezone": "America/New_York"},
            "currency_semantics": {"base_currency": "USD"},
            "data_requirements": {"available_at": "required"},
            "risk_model_family": "EWMA",
            "cost_model_family": "SPREAD",
            "capacity_model_family": "ADV",
        },
    )
    assert universe.status_code == 201, universe.text
    draft_response = client.post(
        "/api/v1/idea-drafts",
        headers={"Idempotency-Key": "draft-create"},
        json={"original_idea_text": "Test a bounded post-earnings signal in liquid equities."},
    )
    assert draft_response.status_code == 201, draft_response.text
    draft = draft_response.json()
    assert draft["stage"] == "CLARIFYING"
    assert draft["next_action"] == "ANSWER_CLARIFICATIONS"
    assert len(draft["clarification_questions"]) == 3

    answered = client.post(
        f"/api/v1/idea-drafts/{draft['id']}/answers",
        headers={"Idempotency-Key": "draft-answers"},
        json={
            "expected_revision": draft["revision"],
            "answers": {
                "market_scope": "Liquid US equities",
                "horizon": "1D rebalance",
                "data_scope": "Licensed daily prices; exclude alternative data",
            },
        },
    )
    assert answered.status_code == 200, answered.text
    assert answered.json()["stage"] == "READY"

    started = client.post(
        f"/api/v1/idea-drafts/{draft['id']}/start",
        headers={"Idempotency-Key": "draft-start"},
        json={"expected_revision": answered.json()["revision"]},
    )
    assert started.status_code == 201, started.text
    program = started.json()
    assert program["state"] == "ACTIVE"
    assert program["mission_count"] == 6
    assert program["charter"]["clarification_transcript"][0]["answer"] == "Liquid US equities"
    assert program["charter"]["universe_version_ids"] == [universe.json()["id"]]

    graph = client.get(f"/api/v1/research-programs/{program['id']}/mission-graph")
    assert graph.status_code == 200, graph.text
    assert [node["mission_type"] for node in graph.json()["nodes"]] == [
        "PLAN_RESEARCH",
        "DATA_QUALITY",
        "ALPHA_DISCOVERY",
        "ROBUSTNESS",
        "PORTFOLIO_ASSEMBLY",
        "SEALED_PROMOTION_REVIEW",
    ]
    assert graph.json()["nodes"][0]["state"] == "READY"

    page = client.get("/api/v1/research-programs")
    assert page.status_code == 200, page.text
    assert page.json()["items"][0]["id"] == program["id"]

    paused = client.post(
        f"/api/v1/research-programs/{program['id']}/pause",
        json={"expected_revision": program["revision"], "reason": "Operator review"},
    )
    assert paused.status_code == 200, paused.text
    assert paused.json()["state"] == "PAUSED"

    factory = create_session_factory(engine)
    with factory() as session:
        mission = session.scalar(
            select(ResearchMission).where(ResearchMission.program_id == UUID(program["id"]))
        )
        assert mission is not None and mission.state == "PLANNED"
        job = session.scalar(select(Job).where(Job.resource_id == mission.id))
        assert job is not None and job.state == "CANCELLED"

    resumed = client.post(
        f"/api/v1/research-programs/{program['id']}/resume",
        json={"expected_revision": paused.json()["revision"]},
    )
    assert resumed.status_code == 200, resumed.text
    assert resumed.json()["state"] == "ACTIVE"

    assert client.post("/api/v1/ideas/preview", json={"idea": "obsolete preview endpoint"}).status_code == 404
