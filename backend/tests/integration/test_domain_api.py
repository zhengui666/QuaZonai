from __future__ import annotations

import io
import json
import zipfile
from dataclasses import replace
from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

from fastapi.testclient import TestClient
from sqlalchemy import Engine, func, select

from db.models import (
    ApprovalSnapshot,
    DownstreamSystem,
    ForwardEvidenceEpisode,
    Job,
    PortfolioCandidate,
    PortfolioProgram,
    ResearchProgram,
)
from db.session import create_session_factory
from downstream_auth import install_service_token, issue_service_token
from main import create_app
from settings import Settings


def _client(engine: Engine, settings: Settings) -> TestClient:
    return TestClient(create_app(settings=settings, engine=engine))


def test_program_creation_is_idempotent_and_queues_a_durable_mission(
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
    assert missions.json()[0]["state"] == "READY"
    assert missions.json()[0]["started_at"] is None

    activity = client.get(f"/api/v1/research-programs/{program['id']}/activity")
    assert activity.status_code == 200
    assert {event["kind"] for event in activity.json()} >= {"PROGRAM_CREATED", "MISSION_READY"}
    assert "MISSION_STARTED" not in {event["kind"] for event in activity.json()}

    factory = create_session_factory(engine)
    with factory() as session:
        job = session.scalar(
            select(Job).where(
                Job.kind == "RESEARCH_MISSION",
                Job.resource_id == UUID(missions.json()[0]["id"]),
            )
        )
        assert job is not None
        assert job.state == "READY"


def test_duplicate_idea_uses_contribution_instead_of_copying_program(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    idea = "Test short-horizon momentum in liquid US equities after realistic costs."
    first = client.post(
        "/api/v1/research-programs",
        headers={"Idempotency-Key": "overlap-first"},
        json={"idea": idea, "answers": {}},
    )
    assert first.status_code == 201, first.text

    preview = client.post("/api/v1/ideas/preview", json={"idea": idea})
    assert preview.status_code == 200
    assert preview.json()["overlap"]["kind"] == "DUPLICATE"

    second = client.post(
        "/api/v1/research-programs",
        headers={"Idempotency-Key": "overlap-second"},
        json={"idea": idea, "answers": {}, "overlap_action": "recommended"},
    )
    assert second.status_code == 201, second.text
    assert second.json()["id"] == first.json()["id"]

    factory = create_session_factory(engine)
    with factory() as session:
        assert session.scalar(select(func.count()).select_from(ResearchProgram)) == 1


def _seed_candidate_approval(
    engine: Engine,
    settings: Settings,
    *,
    expired: bool = False,
) -> tuple[str, str, str]:
    factory = create_session_factory(engine)
    with factory() as session, session.begin():
        downstream_id = uuid4()
        issued = issue_service_token(settings, downstream_id)
        downstream = DownstreamSystem(
            id=downstream_id,
            name=f"Paper Lab {downstream_id.hex[:8]}",
            environment_type="PAPER",
            enabled=True,
            package_contract_version="1",
            feedback_contract_version="1",
            compatibility=["NAUTILUS_TRADER_1.231.0"],
            preflight_state="READY",
            public_config={
                "feedback_contract": {
                    "minimum_observation_duration_seconds": 60,
                    "minimum_valid_sample_size": 10,
                    "required_fields": ["return"],
                }
            },
        )
        install_service_token(downstream, issued)
        program = PortfolioProgram(
            mandate_version_id=uuid4(),
            mandate_name="Core Growth",
            state="CANDIDATE_READY",
        )
        session.add_all([downstream, program])
        session.flush()
        strategy_artifact = {
            "strategy_path": "strategy.example:ExampleStrategy",
            "config_path": "strategy.example:ExampleConfig",
            "config": {
                "instrument_id": "AAPL.SIM",
                "bar_type": "AAPL.SIM-1-MINUTE-LAST-EXTERNAL",
                "trade_size": "1",
            },
            "source_files": {
                "strategy/__init__.py": "",
                "strategy/example.py": (
                    "class ExampleConfig:\n    pass\n\n"
                    "class ExampleStrategy:\n    pass\n"
                ),
            },
            "requirements": ["nautilus-trader==1.231.0"],
        }
        portfolio_evidence = {
            "external_run_id": "fixture-portfolio-run",
            "state": "SUCCEEDED",
            "mode": "PORTFOLIO",
            "runtime_name": "NautilusTrader",
            "nautilus_version": "1.231.0",
            "contract_version": "1",
            "catalog_uri": "catalog://fixture-discovery",
            "strategy_artifact": strategy_artifact,
            "orders": [
                {"instrument_id": "AAPL.SIM", "side": "BUY", "quantity": "1"},
                {"instrument_id": "MSFT.SIM", "side": "BUY", "quantity": "1"},
            ],
            "fills": [
                {"instrument_id": "AAPL.SIM", "quantity": "1"},
                {"instrument_id": "MSFT.SIM", "quantity": "1"},
            ],
            "positions": [
                {"instrument_id": "AAPL.SIM", "quantity": "1"},
                {"instrument_id": "MSFT.SIM", "quantity": "1"},
            ],
            "account": [{"currency": "USD", "balance": "100000"}],
            "statistics": {
                "total_orders": 2,
                "total_positions": 2,
                "sharpe_ratio": 1.2,
                "max_drawdown": 0.08,
                "turnover": 0.25,
            },
        }
        candidate = PortfolioCandidate(
            portfolio_program_id=program.id,
            mandate_version_id=program.mandate_version_id,
            mandate_name=program.mandate_name,
            state="READY",
            members=[
                {"instrument_id": "AAPL.SIM", "target_weight": 0.6},
                {"instrument_id": "MSFT.SIM", "target_weight": 0.4},
            ],
            metrics={
                "search_adjusted_quality": 0.78,
                "sealed_statistics": {"sharpe_ratio": 1.1, "max_drawdown": 0.09},
                "nautilus": {
                    "strategy_artifact": strategy_artifact,
                    "portfolio_evidence": portfolio_evidence,
                    "discovery_run_id": "fixture-discovery-run",
                    "sealed_run_id": "fixture-sealed-run",
                    "portfolio_run_id": "fixture-portfolio-run",
                },
            },
            created_at=datetime.now(UTC),
        )
        session.add(candidate)
        session.flush()
        program.current_candidate_id = candidate.id
        approval = ApprovalSnapshot(
            candidate_id=candidate.id,
            purpose="PAPER",
            state="PENDING",
            valid_until=datetime.now(UTC)
            + (-timedelta(minutes=1) if expired else timedelta(days=7)),
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
        return str(approval.id), str(downstream.id), issued.token


def test_approval_builds_package_and_authenticated_handoff_feedback(
    engine: Engine,
    settings: Settings,
) -> None:
    approval_id, downstream_id, token = _seed_candidate_approval(engine, settings)
    client = _client(engine, settings)
    headers = {"Idempotency-Key": "approve-1"}
    body = {"downstream_system_id": downstream_id, "expected_state": "PENDING"}

    approved = client.post(
        f"/api/v1/approvals/{approval_id}/approve",
        headers=headers,
        json=body,
    )
    assert approved.status_code == 200, approved.text
    assert approved.json()["state"] == "APPROVED"

    replay = client.post(
        f"/api/v1/approvals/{approval_id}/approve",
        headers=headers,
        json=body,
    )
    assert replay.status_code == 200
    assert replay.json()["state"] == "APPROVED"

    handoffs = client.get("/api/v1/handoffs")
    assert handoffs.status_code == 200
    assert len(handoffs.json()) == 1
    handoff = handoffs.json()[0]
    assert handoff["state"] == "AVAILABLE"

    unauthorized = client.post(
        f"/api/v1/handoffs/{handoff['id']}/claim",
        headers={"Idempotency-Key": "claim-wrong", "Authorization": "Bearer wrong-token"},
        json={},
    )
    assert unauthorized.status_code == 403

    auth = {"Authorization": f"Bearer {token}"}
    claimed = client.post(
        f"/api/v1/handoffs/{handoff['id']}/claim",
        headers={**auth, "Idempotency-Key": "claim-1"},
        json={"expected_state": "AVAILABLE"},
    )
    assert claimed.status_code == 200, claimed.text
    assert claimed.json()["state"] == "CLAIMED"

    package = client.get(f"/api/v1/handoffs/{handoff['id']}/package", headers=auth)
    assert package.status_code == 200, package.text
    assert package.headers["content-type"].startswith("application/zip")
    with zipfile.ZipFile(io.BytesIO(package.content)) as archive:
        names = set(archive.namelist())
        required = {
            "manifest.json",
            "requirements.lock",
            "strategy/strategy.whl",
            "strategy/strategy-config.json",
            "strategy/actor-config.json",
            "data/requirements.json",
            "data/instrument-scope.json",
            "runtime/nautilus-version.json",
            "runtime/backtest-run-config.json",
            "runtime/venue-config.json",
            "runtime/risk-config.json",
            "runtime/live-node-template.json",
            "validation/target-portfolio-frame.json",
            "validation/expected-statistics.json",
            "evidence/discovery-summary.json",
            "evidence/sealed-summary.json",
            "evidence/robustness-summary.json",
            "lineage.json",
        }
        assert required <= names
        manifest = json.loads(archive.read("manifest.json"))
        assert manifest["canonical_runtime"]["name"] == "NautilusTrader"
        assert manifest["canonical_runtime"]["version"] == "1.231.0"
        assert manifest["same_strategy_artifact_for_backtest_paper_live"] is True
        assert manifest["strategy"]["wheel"] in names

    accepted = client.post(
        f"/api/v1/handoffs/{handoff['id']}/accept",
        headers={**auth, "Idempotency-Key": "accept-1"},
        json={"expected_state": "CLAIMED"},
    )
    assert accepted.status_code == 200, accepted.text
    assert accepted.json()["state"] == "DOWNSTREAM_ACCEPTED"

    invalid = client.post(
        f"/api/v1/handoffs/{handoff['id']}/feedback",
        headers={**auth, "Idempotency-Key": "feedback-invalid"},
        json={"state": "FEEDBACK_COMPLETE", "evidence": {}},
    )
    assert invalid.status_code == 422
    assert invalid.json()["error"]["code"] == "FEEDBACK_CONTRACT_INVALID"

    factory = create_session_factory(engine)
    with factory() as session:
        assert session.scalar(select(func.count()).select_from(ForwardEvidenceEpisode)) == 0

    start = datetime.now(UTC) - timedelta(minutes=10)
    feedback = client.post(
        f"/api/v1/handoffs/{handoff['id']}/feedback",
        headers={**auth, "Idempotency-Key": "feedback-1"},
        json={
            "state": "FEEDBACK_COMPLETE",
            "observation_start": start.isoformat(),
            "observation_end": datetime.now(UTC).isoformat(),
            "sample_size": 30,
            "evidence": {"periods": 30, "return": 0.04},
        },
    )
    assert feedback.status_code == 200, feedback.text
    assert feedback.json()["state"] == "FEEDBACK_COMPLETE"
    with factory() as session:
        assert session.scalar(select(func.count()).select_from(ForwardEvidenceEpisode)) == 1


def test_expired_approval_state_is_committed_before_conflict(
    engine: Engine,
    settings: Settings,
) -> None:
    approval_id, downstream_id, _ = _seed_candidate_approval(engine, settings, expired=True)
    client = _client(engine, settings)
    response = client.post(
        f"/api/v1/approvals/{approval_id}/approve",
        headers={"Idempotency-Key": "expired-approval"},
        json={"downstream_system_id": downstream_id, "expected_state": "PENDING"},
    )
    assert response.status_code == 409
    assert response.json()["error"]["code"] == "APPROVAL_EXPIRED"

    factory = create_session_factory(engine)
    with factory() as session:
        approval = session.get(ApprovalSnapshot, UUID(approval_id))
        assert approval is not None
        assert approval.state == "EXPIRED"


def test_resource_id_is_part_of_idempotency_identity(
    engine: Engine,
    settings: Settings,
) -> None:
    first_id, _, _ = _seed_candidate_approval(engine, settings)
    second_id, _, _ = _seed_candidate_approval(engine, settings)
    client = _client(engine, settings)
    headers = {"Idempotency-Key": "same-key-different-resource"}
    body = {"reason_code": "NO", "expected_state": "PENDING"}

    first = client.post(f"/api/v1/approvals/{first_id}/reject", headers=headers, json=body)
    assert first.status_code == 200, first.text
    second = client.post(f"/api/v1/approvals/{second_id}/reject", headers=headers, json=body)
    assert second.status_code == 409
    assert second.json()["error"]["code"] == "IDEMPOTENCY_KEY_REUSED"


def test_data_source_registration_updates_readiness(
    engine: Engine,
    settings: Settings,
) -> None:
    # Research readiness now also requires an explicit Codex route. Use the
    # existing custom-provider path so this test remains focused on data setup.
    client = _client(engine, replace(settings, codex_api_key="test-api-key"))
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
