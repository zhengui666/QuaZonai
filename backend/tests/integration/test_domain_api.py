from __future__ import annotations

import io
import json
import zipfile
from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

from fastapi.testclient import TestClient
from sqlalchemy import Engine, func, select

from db.models import (
    ApprovalSnapshot,
    DownstreamSystem,
    DatasetRevision,
    ForwardEvidenceEpisode,
    GovernedDataSource,
    Job,
    MarketUniverseVersion,
    PortfolioCandidate,
    PortfolioProgram,
    ResearchCharter,
    ResearchProgram,
)
from db.session import create_session_factory
from downstream_auth import install_service_token, issue_service_token
from main import create_app
from settings import Settings


def _client(engine: Engine, settings: Settings) -> TestClient:
    return TestClient(create_app(settings=settings, engine=engine))


def _seed_research_scope(
    engine: Engine,
    *,
    name: str = "US Equities",
    key: str = "US_EQUITIES",
    instrument_id: str = "AAPL.XNAS",
) -> UUID:
    factory = create_session_factory(engine)
    now = datetime.now(UTC)
    universe_id = uuid4()
    with factory() as session, session.begin():
        universe = MarketUniverseVersion(
            id=universe_id,
            universe_key=f"{key}_{universe_id.hex[:8]}",
            version_no=1,
            name=name,
            state="ACTIVE",
            spec_json={},
            created_at=now,
        )
        source = GovernedDataSource(
            name=f"Governed quotes {universe_id}",
            provider="Integration provider",
            state="ACTIVE",
            universe_scope=[name],
            fields=["event_time", "available_time", "bid_price", "ask_price"],
            preflight_state="READY",
            public_config={"data_domains": ["quotes", "market_data"]},
        )
        session.add_all([universe, source])
        session.flush()
        session.add(
            DatasetRevision(
                data_source_id=source.id,
                universe_version_id=universe.id,
                universe_name=name,
                revision_no=1,
                event_start=now - timedelta(days=30),
                event_end=now - timedelta(days=20),
                available_start=now - timedelta(days=30),
                available_end=now - timedelta(days=20),
                row_count=100,
                quality_state="VALID",
                point_in_time_state="VALID",
                partition="DISCOVERY",
                created_at=now,
                provider_name="Integration provider",
                source_license="integration-test",
                catalog_uri=f"nautilus-catalog://scope-{universe_id.hex}",
                nautilus_data_type="QuoteTick",
                instrument_scope=[instrument_id],
                schema_revision="quote-v2",
                quality_result={"state": "VALID"},
                point_in_time_result={"state": "VALID"},
                ingested_at=now,
            )
        )
    return universe_id


def _nautilus_candidate_metrics(experiment_id: UUID, alpha_id: UUID) -> dict:
    strategy_source = (
        "from nautilus_trader.examples.strategies.ema_cross import "
        "EMACross as CandidateStrategy, EMACrossConfig as CandidateConfig\n"
    )
    return {
        "search_adjusted_quality": 0.78,
        "nautilus": {
            "strategy_artifact": {
                "artifact_id": "candidate-ema-cross-v1",
                "kind": "SOURCE_BUNDLE",
                "strategy_path": "candidate_strategy:CandidateStrategy",
                "config_path": "candidate_strategy:CandidateConfig",
                "config": {
                    "instrument_id": "EUR/USD.SIM",
                    "bar_type": "EUR/USD.SIM-1-MINUTE-BID-INTERNAL",
                    "trade_size": "100000",
                    "fast_ema_period": 3,
                    "slow_ema_period": 8,
                },
                "source_files": {"candidate_strategy.py": strategy_source},
                "requirements": ["nautilus_trader==1.231.0"],
            },
            "evidence": {
                "experiment_id": str(experiment_id),
                "orders": [
                    {
                        "order_id": "O-1",
                        "instrument_id": "EUR/USD.SIM",
                        "side": "BUY",
                        "order_type": "MARKET",
                        "status": "FILLED",
                        "quantity": "100000",
                        "filled_quantity": "100000",
                    }
                ],
                "fills": [
                    {
                        "trade_id": "T-1",
                        "order_id": "O-1",
                        "instrument_id": "EUR/USD.SIM",
                        "side": "BUY",
                        "quantity": "100000",
                        "price": "1.10000",
                    }
                ],
                "positions": [
                    {
                        "position_id": "P-1",
                        "instrument_id": "EUR/USD.SIM",
                        "side": "LONG",
                        "quantity": "100000",
                    }
                ],
                "pnl": {"realized": "250 USD"},
                "statistics": {"total_orders": 1, "total_fills": 1, "total_positions": 1},
            },
            "dataset_revision_ids": [],
            "alpha_qualification_ids": [str(alpha_id)],
            "instrument_scope": ["EUR/USD.SIM", "GBP/USD.SIM"],
            "data_requirements": {"nautilus_data_type": "QuoteTick"},
            "backtest_run_config": {
                "catalog_uri": "nautilus-catalog://integration-fx-quotes",
                "mode": "PORTFOLIO",
            },
            "venue_config": {"name": "SIM", "oms_type": "HEDGING", "account_type": "MARGIN"},
            "risk_config": {"bypass": False},
            "discovery_summary": {"source": "search-ledger"},
            "sealed_summary": {"raw_evidence_withheld": True},
            "robustness_summary": {"status": "PASS"},
        },
    }


def test_program_creation_is_idempotent_and_queues_a_durable_mission(
    engine: Engine,
    settings: Settings,
) -> None:
    universe_id = _seed_research_scope(engine)
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
    assert program["charter"]["universe_version_ids"] == [str(universe_id)]
    assert set(program["charter"]["allowed_data_domains"]) >= {"quotes", "market_data"}

    factory = create_session_factory(engine)
    with factory() as session:
        charter = session.get(ResearchCharter, UUID(program["charter_id"]))
        assert charter is not None
        assert charter.universe_version_ids == [str(universe_id)]
        assert set(charter.allowed_data_domains) >= {"quotes", "market_data"}

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
    _seed_research_scope(engine)
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


def test_program_creation_rejects_unavailable_inferred_scope(
    engine: Engine,
    settings: Settings,
) -> None:
    _seed_research_scope(engine, name="US Equities", key="US_EQUITIES")
    client = _client(engine, settings)

    response = client.post(
        "/api/v1/research-programs",
        headers={"Idempotency-Key": "scope-unavailable"},
        json={
            "idea": "Test crypto spot momentum after realistic costs and liquidity filters.",
            "answers": {},
        },
    )
    assert response.status_code == 422, response.text
    assert response.json()["error"]["code"] == "RESEARCH_SCOPE_UNAVAILABLE"

    factory = create_session_factory(engine)
    with factory() as session:
        assert session.scalar(select(func.count()).select_from(ResearchProgram)) == 0
        assert session.scalar(select(func.count()).select_from(ResearchCharter)) == 0


def _seed_candidate_approval(
    engine: Engine, settings: Settings, *, expired: bool = False
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
            package_contract_version="2",
            feedback_contract_version="1",
            compatibility=[],
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
        experiment_id = uuid4()
        alpha_id = uuid4()
        candidate = PortfolioCandidate(
            portfolio_program_id=program.id,
            mandate_version_id=program.mandate_version_id,
            mandate_name=program.mandate_name,
            state="READY",
            members=[
                {
                    "alpha_qualification_id": str(alpha_id),
                    "instrument_id": "EUR/USD.SIM",
                    "target_weight": 0.6,
                },
                {
                    "alpha_qualification_id": str(alpha_id),
                    "instrument_id": "GBP/USD.SIM",
                    "target_weight": 0.4,
                },
            ],
            metrics=_nautilus_candidate_metrics(experiment_id, alpha_id),
            created_at=datetime.now(UTC),
        )
        session.add(candidate)
        session.flush()
        program.current_candidate_id = candidate.id
        approval = ApprovalSnapshot(
            candidate_id=candidate.id,
            purpose="PAPER",
            state="PENDING",
            downstream_system_id=downstream.id,
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
    monkeypatch,
) -> None:
    monkeypatch.setattr(
        "api.domain._verify_candidate_bundle_remotely",
        lambda *args, **kwargs: None,
    )
    approval_id, downstream_id, token = _seed_candidate_approval(engine, settings)
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
            "strategy/strategy-config.json",
            "strategy/actor-config.json",
            "data/requirements.json",
            "data/instrument-scope.json",
            "runtime/nautilus-version.json",
            "runtime/backtest-run-config.json",
            "runtime/venue-config.json",
            "runtime/risk-config.json",
            "runtime/live-node-template.json",
            "validation/fixture-catalog/manifest.json",
            "validation/expected-orders.json",
            "validation/expected-fills.json",
            "validation/expected-positions.json",
            "validation/expected-statistics.json",
            "evidence/discovery-summary.json",
            "evidence/sealed-summary.json",
            "evidence/robustness-summary.json",
            "evidence/portfolio-simulation.json",
            "lineage.json",
        }
        assert required <= names
        manifest = json.loads(archive.read("manifest.json"))
        assert manifest["contract"] == "QUAZONAI_NAUTILUS_CANDIDATE_BUNDLE"
        assert manifest["contract_version"] == "2"
        assert manifest["runtime"] == {
            "name": "NAUTILUS_TRADER",
            "version": "1.231.0",
            "deployment": "REMOTE_INDEPENDENT_RUNTIME",
            "paper_live_reuse": "SAME_STRATEGY_WHEEL_AND_CONFIG",
        }
        assert manifest["strategy"]["wheel"] in names
        assert manifest["strategy"]["wheel"].startswith("strategy/quazonai_candidate_strategy-")
        assert manifest["strategy"]["wheel"].endswith("-py3-none-any.whl")

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
