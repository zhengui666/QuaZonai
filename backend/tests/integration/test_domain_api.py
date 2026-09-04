from __future__ import annotations

from dataclasses import replace
from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

from fastapi.testclient import TestClient
from sqlalchemy import Engine, func, select

from db.models import (
    ApprovalSnapshot,
    CandidatePackage,
    DatasetRevision,
    DownstreamSystem,
    ForwardEvidenceEpisode,
    HandoffOffer,
    Job,
    PortfolioCandidate,
    PortfolioProgram,
    PreflightReceipt,
)
from db.session import create_session_factory
from downstream_auth import install_service_token, issue_service_token
from downstream_contracts import feedback_contract_snapshot
from main import create_app
from settings import Settings


def _client(engine: Engine, settings: Settings) -> TestClient:
    return TestClient(create_app(settings=settings, engine=engine))


def test_idea_draft_start_is_idempotent_and_queues_a_durable_mission_graph(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    universe = client.post(
        "/api/v1/universes",
        json={
            "universe_key": "DRAFT_TEST",
            "name": "Draft test universe",
            "instrument_schema": {"instrument_id": "string"},
            "membership_rules": {"listing": "test"},
            "calendar_semantics": {"timezone": "UTC"},
            "currency_semantics": {"base_currency": "USD"},
            "data_requirements": {"available_at": "required"},
            "risk_model_family": "EWMA",
            "cost_model_family": "SPREAD",
            "capacity_model_family": "ADV",
        },
    )
    assert universe.status_code == 201, universe.text
    headers = {"Idempotency-Key": "draft-create-1"}
    payload = {"original_idea_text": "Test post-earnings drift in liquid US equities."}
    created = client.post("/api/v1/idea-drafts", headers=headers, json=payload)
    assert created.status_code == 201, created.text
    draft = created.json()

    replay = client.post("/api/v1/idea-drafts", headers=headers, json=payload)
    assert replay.status_code == 201, replay.text
    assert replay.json()["id"] == draft["id"]

    conflict = client.post(
        "/api/v1/idea-drafts",
        headers=headers,
        json={"original_idea_text": "Test crypto carry after realistic costs."},
    )
    assert conflict.status_code == 409
    assert conflict.json()["error"]["code"] == "IDEMPOTENCY_KEY_REUSED"

    answers = client.post(
        f"/api/v1/idea-drafts/{draft['id']}/answers",
        json={
            "expected_revision": draft["revision"],
            "answers": {
                "market_scope": "Liquid US equities",
                "horizon": "1D",
                "data_scope": "Licensed daily prices",
            },
        },
    )
    assert answers.status_code == 200, answers.text
    start_headers = {"Idempotency-Key": "draft-start-1"}
    start_payload = {"expected_revision": answers.json()["revision"]}
    started = client.post(
        f"/api/v1/idea-drafts/{draft['id']}/start",
        headers=start_headers,
        json=start_payload,
    )
    assert started.status_code == 201, started.text
    program = started.json()
    assert program["state"] == "ACTIVE"
    assert program["mission_count"] == 6

    replay_start = client.post(
        f"/api/v1/idea-drafts/{draft['id']}/start",
        headers=start_headers,
        json=start_payload,
    )
    assert replay_start.status_code == 201, replay_start.text
    assert replay_start.json()["id"] == program["id"]

    graph = client.get(f"/api/v1/research-programs/{program['id']}/mission-graph")
    assert graph.status_code == 200, graph.text
    missions = graph.json()["nodes"]
    assert len(missions) == 6
    assert missions[0]["mission_type"] == "PLAN_RESEARCH"
    assert missions[0]["state"] == "READY"

    factory = create_session_factory(engine)
    with factory() as session:
        job = session.scalar(
            select(Job).where(
                Job.kind == "RESEARCH_MISSION",
                Job.resource_id == UUID(missions[0]["id"]),
            )
        )
        assert job is not None
        assert job.state == "READY"


def _seed_candidate_approval(
    engine: Engine,
    settings: Settings,
    *,
    expired: bool = False,
    bind_package: bool = True,
    expected_package_revision: int | None = None,
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
                    "accepted_package_contracts": ["1"],
                    "accepted_arrow_contracts": ["arrow-ipc-file-v1"],
                    "disclosure_policy": "FULL",
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
        candidate_id = uuid4()
        created_at = datetime.now(UTC)
        candidate = PortfolioCandidate(
            id=candidate_id,
            portfolio_program_id=program.id,
            mandate_version_id=program.mandate_version_id,
            mandate_name=program.mandate_name,
            state="READY",
            created_at=created_at,
        )
        session.add(candidate)
        session.flush()
        candidate_package = (
            CandidatePackage(
                id=uuid4(),
                candidate_id=candidate.id,
                revision=1,
                contract_version="1",
                state="AVAILABLE",
                manifest_json={},
                relative_path="legacy/candidate-package.zip",
                payload={},
                created_at=created_at,
            )
            if bind_package
            else None
        )
        if candidate_package is not None:
            session.add(candidate_package)
            session.flush()
        approval = ApprovalSnapshot(
            candidate_id=candidate.id,
            candidate_package_id=(candidate_package.id if candidate_package else None),
            candidate_package_revision=(
                expected_package_revision
                if expected_package_revision is not None
                else candidate_package.revision
                if candidate_package
                else None
            ),
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
        session.add_all(
            (
                PreflightReceipt(
                    resource_type="DOWNSTREAM_SYSTEM",
                    resource_id=downstream.id,
                    resource_revision=downstream.revision,
                    revision=1,
                    status="READY",
                    reason_codes=[],
                    capabilities=downstream.compatibility,
                    contract_version=downstream.feedback_contract_version,
                    checked_at=created_at,
                    valid_until=created_at + timedelta(days=1),
                    checker_version="test",
                ),
                approval,
            )
        )
        session.flush()
        return str(approval.id), str(downstream.id), issued.token


def test_approval_rejects_legacy_available_package_before_handoff(
    engine: Engine,
    settings: Settings,
) -> None:
    approval_id, downstream_id, _ = _seed_candidate_approval(engine, settings)
    client = _client(engine, settings)
    headers = {"Idempotency-Key": "approve-1"}
    body = {"downstream_system_id": downstream_id, "expected_state": "PENDING"}
    factory = create_session_factory(engine)
    with factory() as session:
        package = session.scalar(select(CandidatePackage))
        assert package is not None
        assert package.state == "AVAILABLE"
        assert package.revision == 1

    approved = client.post(
        f"/api/v1/approvals/{approval_id}/approve",
        headers=headers,
        json=body,
    )
    assert approved.status_code == 409
    assert approved.json()["error"]["code"] == "CANDIDATE_PACKAGE_STALE"
    with factory() as session:
        assert session.scalar(select(func.count()).select_from(CandidatePackage)) == 1
        assert session.scalar(select(func.count()).select_from(ForwardEvidenceEpisode)) == 0


def test_claimed_handoff_download_rejects_legacy_available_package(
    engine: Engine,
    settings: Settings,
) -> None:
    approval_id, downstream_id, token = _seed_candidate_approval(engine, settings)
    factory = create_session_factory(engine)
    with factory() as session, session.begin():
        approval = session.get(ApprovalSnapshot, UUID(approval_id))
        downstream = session.get(DownstreamSystem, UUID(downstream_id))
        package = session.scalar(select(CandidatePackage))
        assert approval is not None
        assert downstream is not None
        assert package is not None
        approval.state = "APPROVED"
        handoff = HandoffOffer(
            approval_id=approval.id,
            candidate_package_id=package.id,
            candidate_id=approval.candidate_id,
            purpose="PAPER",
            downstream_system_id=downstream.id,
            state="CLAIMED",
            claimed_at=datetime.now(UTC),
            feedback_state="PENDING",
            feedback_contract_snapshot=feedback_contract_snapshot(downstream, "PAPER"),
        )
        session.add(handoff)
        session.flush()
        handoff_id = handoff.id

    client = _client(engine, settings)
    response = client.get(
        f"/api/v1/handoffs/{handoff_id}/package",
        headers={"Authorization": f"Bearer {token}"},
    )
    assert response.status_code == 409
    assert response.json()["error"]["code"] == "CANDIDATE_PACKAGE_STALE"


def test_approval_requires_a_prebuilt_candidate_package(
    engine: Engine,
    settings: Settings,
) -> None:
    approval_id, downstream_id, _ = _seed_candidate_approval(
        engine,
        settings,
        bind_package=False,
    )
    client = _client(engine, settings)
    response = client.post(
        f"/api/v1/approvals/{approval_id}/approve",
        headers={"Idempotency-Key": "approval-package-required"},
        json={"downstream_system_id": downstream_id, "expected_state": "PENDING"},
    )

    assert response.status_code == 409
    assert response.json()["error"]["code"] == "CANDIDATE_PACKAGE_REQUIRED"
    factory = create_session_factory(engine)
    with factory() as session:
        assert session.scalar(select(func.count()).select_from(CandidatePackage)) == 0
        approval = session.get(ApprovalSnapshot, UUID(approval_id))
        assert approval is not None
        assert approval.state == "PENDING"


def test_approval_requires_the_expected_candidate_package_revision(
    engine: Engine,
    settings: Settings,
) -> None:
    approval_id, downstream_id, _ = _seed_candidate_approval(
        engine,
        settings,
        expected_package_revision=2,
    )
    client = _client(engine, settings)
    response = client.post(
        f"/api/v1/approvals/{approval_id}/approve",
        headers={"Idempotency-Key": "approval-package-revision"},
        json={"downstream_system_id": downstream_id, "expected_state": "PENDING"},
    )

    assert response.status_code == 409
    assert response.json()["error"]["code"] == "CANDIDATE_PACKAGE_STALE"


def test_approval_keeps_paper_and_live_downstreams_separate(
    engine: Engine,
    settings: Settings,
) -> None:
    approval_id, downstream_id, _ = _seed_candidate_approval(engine, settings)
    factory = create_session_factory(engine)
    with factory() as session, session.begin():
        downstream = session.get(DownstreamSystem, UUID(downstream_id))
        assert downstream is not None
        downstream.environment_type = "LIVE"

    client = _client(engine, settings)
    response = client.post(
        f"/api/v1/approvals/{approval_id}/approve",
        headers={"Idempotency-Key": "approval-live-downstream"},
        json={"downstream_system_id": downstream_id, "expected_state": "PENDING"},
    )

    assert response.status_code == 409
    assert response.json()["error"]["code"] == "DOWNSTREAM_INCOMPATIBLE"


def test_readiness_and_approval_require_a_current_downstream_preflight_receipt(
    engine: Engine,
    settings: Settings,
) -> None:
    approval_id, downstream_id, _ = _seed_candidate_approval(engine, settings)
    client = _client(engine, settings)
    assert client.get("/api/v1/readiness").json()["PAPER_HANDOFF_READY"] is True

    factory = create_session_factory(engine)
    with factory() as session, session.begin():
        receipt = session.scalar(
            select(PreflightReceipt).where(
                PreflightReceipt.resource_type == "DOWNSTREAM_SYSTEM",
                PreflightReceipt.resource_id == UUID(downstream_id),
            )
        )
        assert receipt is not None
        session.delete(receipt)

    readiness = client.get("/api/v1/readiness")
    assert readiness.status_code == 200
    assert readiness.json()["PAPER_HANDOFF_READY"] is False
    response = client.post(
        f"/api/v1/approvals/{approval_id}/approve",
        json={"downstream_system_id": downstream_id, "expected_state": "PENDING"},
    )
    assert response.status_code == 409
    assert response.json()["error"]["code"] == "DOWNSTREAM_NOT_READY"


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


def test_readiness_requires_a_valid_promotable_pit_dataset(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, replace(settings, codex_api_key="test-api-key"))
    before = client.get("/api/v1/readiness")
    assert before.status_code == 200
    assert before.json()["RESEARCH_READY"] is False

    universe = client.post(
        "/api/v1/universes",
        json={
            "universe_key": "READINESS",
            "name": "Readiness universe",
            "instrument_schema": {"instrument_id": "string"},
            "membership_rules": {"listing": "test"},
            "calendar_semantics": {"timezone": "UTC"},
            "currency_semantics": {"base_currency": "USD"},
            "data_requirements": {"available_at": "required"},
            "risk_model_family": "EWMA",
            "cost_model_family": "SPREAD",
            "capacity_model_family": "ADV",
        },
    )
    assert universe.status_code == 201, universe.text
    source = client.post(
        "/api/v1/data-sources",
        headers={"Idempotency-Key": "source-1"},
        json={
            "name": "Primary PIT Data",
            "connector_key": "primary-pit",
            "provider": "Approved provider",
            "universe_scope": [universe.json()["id"]],
            "field_schema": {"event_time": "timestamp", "available_time": "timestamp"},
            "license_classification": "LICENSED",
            "availability_semantics": {"available_at_field": "available_time"},
        },
    )
    assert source.status_code == 201, source.text
    assert source.json()["preflight_state"] == "PENDING"

    pending = client.get("/api/v1/readiness")
    assert pending.status_code == 200
    assert pending.json()["RESEARCH_READY"] is False
    assert "PROMOTABLE_DATASET_REQUIRED" in pending.json()["RESEARCH_READY_REASONS"]

    factory = create_session_factory(engine)
    now = datetime.now(UTC)
    with factory.begin() as session:
        session.add(
            DatasetRevision(
                data_source_id=UUID(source.json()["id"]),
                universe_version_id=UUID(universe.json()["id"]),
                universe_name="Readiness universe",
                revision_no=1,
                data_class="VENDOR",
                origin="verified-materialization",
                ingested_at=now,
                promotability="PROMOTABLE",
                schema_version="v1",
                event_start=now,
                event_end=now,
                available_start=now,
                available_end=now,
                quality_state="VALID",
                point_in_time_state="VALID",
                partition="DISCOVERY",
                created_at=now,
            )
        )

    after = client.get("/api/v1/readiness")
    assert after.status_code == 200
    assert after.json()["RESEARCH_READY"] is True
