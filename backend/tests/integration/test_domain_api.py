from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import uuid4

from fastapi.testclient import TestClient
from sqlalchemy import Engine

from db.models import (
    ApprovalSnapshot,
    DownstreamSystem,
    PortfolioCandidate,
    PortfolioProgram,
)
from db.session import create_session_factory
from main import create_app
from settings import Settings


def _client(engine: Engine, settings: Settings) -> TestClient:
    return TestClient(create_app(settings=settings, engine=engine))


def test_program_creation_is_idempotent_and_starts_a_real_mission(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    headers = {"Idempotency-Key": "program-create-1"}
    payload = {
        "idea": "Test post-earnings drift in liquid US equities after realistic costs.",
        "answers": {},
    }

    created = client.post("/api/v1/research-programs", headers=headers, json=payload)
    assert created.status_code == 201, created.text
    program = created.json()
    assert program["state"] == "ACTIVE"
    assert program["mission_count"] == 1

    replay = client.post("/api/v1/research-programs", headers=headers, json=payload)
    assert replay.status_code == 201, replay.text
    assert replay.json()["id"] == program["id"]

    conflict = client.post(
        "/api/v1/research-programs",
        headers=headers,
        json={"idea": "Test crypto carry after realistic costs.", "answers": {}},
    )
    assert conflict.status_code == 409
    assert conflict.json()["error"]["code"] == "IDEMPOTENCY_KEY_REUSED"

    missions = client.get(f"/api/v1/research-programs/{program['id']}/missions")
    assert missions.status_code == 200
    assert missions.json()[0]["type"] == "ALPHA_DISCOVERY"
    assert missions.json()[0]["state"] == "RUNNING"

    activity = client.get(f"/api/v1/research-programs/{program['id']}/activity")
    assert activity.status_code == 200
    assert {event["kind"] for event in activity.json()} >= {"PROGRAM_CREATED", "MISSION_STARTED"}


def _seed_candidate_approval(engine: Engine) -> tuple[str, str]:
    factory = create_session_factory(engine)
    with factory() as session, session.begin():
        downstream = DownstreamSystem(
            name="Paper Lab",
            environment_type="PAPER",
            enabled=True,
            package_contract_version="1",
            feedback_contract_version="1",
            compatibility=[],
            preflight_state="READY",
            public_config={},
        )
        program = PortfolioProgram(
            mandate_version_id=uuid4(),
            mandate_name="Core Growth",
            state="CANDIDATE_READY",
        )
        session.add_all([downstream, program])
        session.flush()
        candidate = PortfolioCandidate(
            portfolio_program_id=program.id,
            mandate_version_id=program.mandate_version_id,
            mandate_name=program.mandate_name,
            state="READY",
            members=[],
            metrics={"search_adjusted_quality": 0.78},
            created_at=datetime.now(UTC),
        )
        session.add(candidate)
        session.flush()
        program.current_candidate_id = candidate.id
        approval = ApprovalSnapshot(
            candidate_id=candidate.id,
            purpose="PAPER",
            state="PENDING",
            valid_until=datetime.now(UTC) + timedelta(days=7),
            recommendation_rationale="Independent evidence materially improves the frontier.",
            human_report={},
            evidence_summary={"search_adjusted_quality": 0.78},
            capital_context={"base_currency": "USD", "deployable_capital": 100000},
            risk_summary={},
            cost_summary={},
            capacity_summary={},
            changes_summary={},
        )
        session.add(approval)
        session.flush()
        return str(approval.id), str(downstream.id)


def test_approval_creates_one_immutable_package_and_handoff(
    engine: Engine,
    settings: Settings,
) -> None:
    approval_id, downstream_id = _seed_candidate_approval(engine)
    client = _client(engine, settings)
    headers = {"Idempotency-Key": "approve-1"}
    body = {"downstream_system_id": downstream_id, "expected_state": "PENDING"}

    approved = client.post(f"/api/v1/approvals/{approval_id}/approve", headers=headers, json=body)
    assert approved.status_code == 200, approved.text
    assert approved.json()["state"] == "APPROVED"

    replay = client.post(f"/api/v1/approvals/{approval_id}/approve", headers=headers, json=body)
    assert replay.status_code == 200
    assert replay.json()["state"] == "APPROVED"

    handoffs = client.get("/api/v1/handoffs")
    assert handoffs.status_code == 200
    assert len(handoffs.json()) == 1
    handoff = handoffs.json()[0]
    assert handoff["state"] == "AVAILABLE"

    unauthorized = client.post(
        f"/api/v1/handoffs/{handoff['id']}/claim",
        headers={"Idempotency-Key": "claim-wrong", "X-QZ-Downstream-Id": str(uuid4())},
        json={},
    )
    assert unauthorized.status_code == 403

    claimed = client.post(
        f"/api/v1/handoffs/{handoff['id']}/claim",
        headers={"Idempotency-Key": "claim-1", "X-QZ-Downstream-Id": downstream_id},
        json={"expected_state": "AVAILABLE"},
    )
    assert claimed.status_code == 200, claimed.text
    assert claimed.json()["state"] == "CLAIMED"

    package = client.get(
        f"/api/v1/handoffs/{handoff['id']}/package",
        headers={"X-QZ-Downstream-Id": downstream_id},
    )
    assert package.status_code == 200, package.text
    assert package.json()["purpose"] == "PAPER"
    assert package.json()["candidate"]["state"] == "READY"

    accepted = client.post(
        f"/api/v1/handoffs/{handoff['id']}/accept",
        headers={"Idempotency-Key": "accept-1", "X-QZ-Downstream-Id": downstream_id},
        json={"expected_state": "CLAIMED"},
    )
    assert accepted.status_code == 200
    assert accepted.json()["state"] == "DOWNSTREAM_ACCEPTED"

    feedback = client.post(
        f"/api/v1/handoffs/{handoff['id']}/feedback",
        headers={"Idempotency-Key": "feedback-1", "X-QZ-Downstream-Id": downstream_id},
        json={"state": "FEEDBACK_COMPLETE", "evidence": {"periods": 30, "return": 0.04}},
    )
    assert feedback.status_code == 200
    assert feedback.json()["state"] == "FEEDBACK_COMPLETE"


def test_data_source_registration_updates_readiness(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    before = client.get("/api/v1/readiness")
    assert before.status_code == 200
    assert before.json()["RESEARCH_READY"] is False

    created = client.post(
        "/api/v1/data-sources",
        headers={"Idempotency-Key": "source-1"},
        json={
            "name": "Primary PIT Data",
            "provider": "Approved provider",
            "fields": ["event_time", "available_time", "close", "volume"],
            "state": "STAGED",
        },
    )
    assert created.status_code == 201, created.text
    assert created.json()["state"] == "ACTIVE"
    assert created.json()["preflight_state"] == "READY"

    after = client.get("/api/v1/readiness")
    assert after.status_code == 200
    assert after.json()["RESEARCH_READY"] is True
