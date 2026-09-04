from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

import pytest
from fastapi.testclient import TestClient
from sqlalchemy import Engine, func, select

from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    CandidatePackage,
    DegradationObservation,
    DownstreamSystem,
    Event,
    ForwardEvidenceEpisode,
    HandoffOffer,
    PortfolioCandidate,
    PortfolioProgram,
    PreflightReceipt,
    ResearchCycle,
    ResearchMission,
    ResearchProgram,
    ResearchWakeEvent,
)
from db.session import create_session_factory
from downstream_auth import install_service_token, issue_service_token
from downstream_contracts import feedback_contract_snapshot
from main import create_app
from settings import Settings


@dataclass(frozen=True)
class SeededHandoff:
    program_id: UUID
    program_revision: int
    handoff_id: UUID
    alpha_id: UUID
    outsider_alpha_id: UUID
    candidate_id: UUID
    downstream_token: str


def _client(engine: Engine, settings: Settings) -> TestClient:
    return TestClient(create_app(settings=settings, engine=engine))


def _start_program(client: TestClient) -> dict[str, object]:
    suffix = uuid4().hex[:8]
    universe = client.post(
        "/api/v1/universes",
        json={
            "universe_key": f"WAKE_{suffix}",
            "name": "Wake integration universe",
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
    draft = client.post(
        "/api/v1/idea-drafts",
        json={"original_idea_text": "Test a bounded degradation wake in liquid equities."},
    )
    assert draft.status_code == 201, draft.text
    answered = client.post(
        f"/api/v1/idea-drafts/{draft.json()['id']}/answers",
        json={
            "expected_revision": draft.json()["revision"],
            "answers": {
                "market_scope": "Liquid US equities",
                "horizon": "1D",
                "data_scope": "Licensed daily prices",
            },
        },
    )
    assert answered.status_code == 200, answered.text
    started = client.post(
        f"/api/v1/idea-drafts/{draft.json()['id']}/start",
        json={"expected_revision": answered.json()["revision"]},
    )
    assert started.status_code == 201, started.text
    return started.json()


def _complete_feedback(client: TestClient, handoff_id: UUID, token: str) -> None:
    end = datetime.now(UTC)
    response = client.post(
        f"/api/v1/handoffs/{handoff_id}/feedback",
        headers={"Authorization": f"Bearer {token}", "Idempotency-Key": "wake-feedback"},
        json={
            "state": "FEEDBACK_COMPLETE",
            "observation_start": (end - timedelta(minutes=1)).isoformat(),
            "observation_end": end.isoformat(),
            "sample_size": 1,
            "evidence": {"return": -0.04},
        },
    )
    assert response.status_code == 200, response.text
    assert response.json()["state"] == "FEEDBACK_COMPLETE"


def _seed_handoff(
    engine: Engine,
    settings: Settings,
    client: TestClient,
    *,
    complete_feedback: bool = True,
) -> SeededHandoff:
    program_view = _start_program(client)
    program_id = UUID(str(program_view["id"]))
    factory = create_session_factory(engine)
    with factory() as session, session.begin():
        alpha = AlphaQualification(
            program_id=program_id,
            role="PRIMARY_ALPHA",
            state="ACTIVE",
            scope_json={},
            metrics={},
            lineage=[],
        )
        sibling = AlphaQualification(
            program_id=program_id,
            role="PRIMARY_ALPHA",
            state="ACTIVE",
            scope_json={},
            metrics={},
            lineage=[],
        )
        outsider = AlphaQualification(
            program_id=program_id,
            role="PRIMARY_ALPHA",
            state="ACTIVE",
            scope_json={},
            metrics={},
            lineage=[],
        )
        portfolio_program = PortfolioProgram(
            mandate_version_id=uuid4(),
            mandate_name="Wake mandate",
            state="CANDIDATE_READY",
        )
        session.add_all((alpha, sibling, outsider, portfolio_program))
        session.flush()
        created_at = datetime.now(UTC)
        candidate_id = uuid4()
        candidate = PortfolioCandidate(
            id=candidate_id,
            portfolio_program_id=portfolio_program.id,
            mandate_version_id=portfolio_program.mandate_version_id,
            mandate_name=portfolio_program.mandate_name,
            state="READY",
            members=[
                {
                    "alpha_qualification_id": str(alpha.id),
                    "instrument_id": "AAPL.SIM",
                    "target_weight": 0.5,
                },
                {
                    "alpha_qualification_id": str(sibling.id),
                    "instrument_id": "MSFT.SIM",
                    "target_weight": 0.5,
                },
            ],
            metrics={
                "target_portfolio_frame": {
                    "schema_version": "1",
                    "portfolio_candidate_id": str(candidate_id),
                    "portfolio_state": "READY",
                    "universe_version_id": str(uuid4()),
                    "as_of_time": created_at.isoformat(),
                    "effective_from": created_at.isoformat(),
                    "effective_until": None,
                    "rows": [
                        {"instrument_id": "AAPL.SIM", "target_weight": 0.5, "confidence": 0.8},
                        {"instrument_id": "MSFT.SIM", "target_weight": 0.5, "confidence": 0.8},
                    ],
                }
            },
            created_at=created_at,
        )
        session.add(candidate)
        session.flush()
        package = CandidatePackage(
            id=uuid4(),
            candidate_id=candidate.id,
            revision=1,
            contract_version="1",
            state="LEGACY_NON_EXECUTABLE",
            manifest_json={},
            relative_path="legacy/degradation-package.zip",
            payload={},
            created_at=created_at,
        )
        downstream_id = uuid4()
        issued = issue_service_token(settings, downstream_id)
        downstream = DownstreamSystem(
            id=downstream_id,
            name=f"Wake Paper {downstream_id.hex[:8]}",
            environment_type="PAPER",
            enabled=True,
            package_contract_version="1",
            feedback_contract_version="1",
            preflight_state="READY",
            public_config={
                "feedback_contract": {
                    "minimum_observation_duration_seconds": 0,
                    "minimum_valid_sample_size": 1,
                    "required_fields": ["return"],
                    "accepted_package_contracts": ["1"],
                    "accepted_arrow_contracts": ["arrow-ipc-file-v1"],
                    "disclosure_policy": "FULL",
                }
            },
        )
        install_service_token(downstream, issued)
        approval = ApprovalSnapshot(
            candidate_id=candidate.id,
            candidate_package_id=package.id,
            candidate_package_revision=package.revision,
            purpose="PAPER",
            state="APPROVED",
            downstream_system_id=downstream_id,
            valid_until=created_at + timedelta(days=1),
            human_report={},
            evidence_summary={},
            capital_context={},
            risk_summary={},
            cost_summary={},
            capacity_summary={},
            changes_summary={},
        )
        session.add_all((package, downstream, approval))
        session.flush()
        handoff = HandoffOffer(
            approval_id=approval.id,
            candidate_package_id=package.id,
            candidate_id=candidate.id,
            purpose="PAPER",
            downstream_system_id=downstream_id,
            state="AVAILABLE",
            claim_deadline=created_at + timedelta(days=7),
            feedback_state="PENDING",
            feedback_contract_snapshot=feedback_contract_snapshot(downstream, "PAPER"),
        )
        session.add(handoff)
        session.flush()
        session.add(
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
            )
        )
        handoff_id = handoff.id
        alpha_id = alpha.id
        outsider_alpha_id = outsider.id

    auth = {"Authorization": f"Bearer {issued.token}"}
    claimed = client.post(
        f"/api/v1/handoffs/{handoff_id}/claim",
        headers={**auth, "Idempotency-Key": "wake-claim"},
        json={"expected_state": "AVAILABLE"},
    )
    assert claimed.status_code == 200, claimed.text
    accepted = client.post(
        f"/api/v1/handoffs/{handoff_id}/accept",
        headers={**auth, "Idempotency-Key": "wake-accept"},
        json={"expected_state": "CLAIMED"},
    )
    assert accepted.status_code == 200, accepted.text
    if complete_feedback:
        _complete_feedback(client, handoff_id, issued.token)
    return SeededHandoff(
        program_id=program_id,
        program_revision=int(program_view["revision"]),
        handoff_id=handoff_id,
        alpha_id=alpha_id,
        outsider_alpha_id=outsider_alpha_id,
        candidate_id=candidate_id,
        downstream_token=issued.token,
    )


def _observation_payload(subject_id: UUID) -> dict[str, str]:
    return {
        "subject_type": "ALPHA",
        "subject_id": str(subject_id),
        "metric_name": "realized_drawdown",
        "severity": "0.90",
        "confidence": "1.00",
    }


def test_completed_handoff_observation_is_idempotent_and_creates_only_research_work(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    seeded = _seed_handoff(engine, settings, client)
    payload = _observation_payload(seeded.alpha_id)
    created = client.post(
        f"/api/v1/handoffs/{seeded.handoff_id}/degradation-observations",
        headers={"Idempotency-Key": "wake-observation"},
        json=payload,
    )
    assert created.status_code == 201, created.text
    assert created.json()["state"] == "FAILED"
    assert created.json()["wake_state"] == "CONSUMED"
    assert created.json()["cycle_id"]

    replay = client.post(
        f"/api/v1/handoffs/{seeded.handoff_id}/degradation-observations",
        headers={"Idempotency-Key": "wake-observation"},
        json=payload,
    )
    assert replay.status_code == 201, replay.text
    assert replay.json() == created.json()

    factory = create_session_factory(engine)
    with factory() as session:
        assert session.scalar(select(func.count()).select_from(DegradationObservation)) == 1
        wake = session.scalar(select(ResearchWakeEvent))
        assert wake is not None and wake.state == "CONSUMED" and wake.cycle_id is not None
        cycle = session.get(ResearchCycle, wake.cycle_id)
        assert cycle is not None and cycle.trigger == "DEGRADATION_WAKE"
        mission_types = set(
            session.scalars(
                select(ResearchMission.type).where(ResearchMission.cycle_id == cycle.id)
            )
        )
        assert mission_types == {"DEGRADATION_DIAGNOSIS", "REPLAN"}
        diagnosis = session.scalar(
            select(ResearchMission).where(
                ResearchMission.cycle_id == cycle.id,
                ResearchMission.type == "DEGRADATION_DIAGNOSIS",
            )
        )
        assert diagnosis is not None
        snapshot = diagnosis.input_snapshot["degradation"]
        assert snapshot["metric_name"] == "realized_drawdown"
        assert snapshot["severity"] == "0.90"
        assert snapshot["confidence"] == "1.00"
        assert snapshot["evaluated"] is True
        assert "policy_snapshot" in snapshot
        assert "return" not in snapshot
        handoff = session.get(HandoffOffer, seeded.handoff_id)
        candidate = session.get(PortfolioCandidate, seeded.candidate_id)
        observation_event = session.scalar(
            select(Event).where(Event.kind == "DEGRADATION_OBSERVATION_RECORDED")
        )
        assert handoff is not None and handoff.state == "FEEDBACK_COMPLETE"
        assert candidate is not None and len(candidate.members) == 2
        assert observation_event is not None and observation_event.actor_kind == "HUMAN"


def test_degradation_requires_completed_feedback_and_candidate_membership(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    seeded = _seed_handoff(engine, settings, client, complete_feedback=False)
    incomplete = client.post(
        f"/api/v1/handoffs/{seeded.handoff_id}/degradation-observations",
        json=_observation_payload(seeded.alpha_id),
    )
    assert incomplete.status_code == 409
    assert incomplete.json()["error"]["code"] == "FORWARD_EVIDENCE_REQUIRED"

    _complete_feedback(client, seeded.handoff_id, seeded.downstream_token)
    mismatch = client.post(
        f"/api/v1/handoffs/{seeded.handoff_id}/degradation-observations",
        json=_observation_payload(seeded.outsider_alpha_id),
    )
    assert mismatch.status_code == 409
    assert mismatch.json()["error"]["code"] == "DEGRADATION_SUBJECT_MISMATCH"

    factory = create_session_factory(engine)
    with factory() as session:
        assert session.scalar(select(func.count()).select_from(DegradationObservation)) == 0
        assert session.scalar(select(func.count()).select_from(ResearchWakeEvent)) == 0


def test_nonmonotonic_forward_evidence_is_rejected_before_writing_an_observation(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    seeded = _seed_handoff(engine, settings, client)
    first = client.post(
        f"/api/v1/handoffs/{seeded.handoff_id}/degradation-observations",
        json=_observation_payload(seeded.alpha_id),
    )
    assert first.status_code == 201, first.text
    factory = create_session_factory(engine)
    with factory() as session, session.begin():
        original = session.get(HandoffOffer, seeded.handoff_id)
        assert original is not None
        original_episode = session.scalar(
            select(ForwardEvidenceEpisode).where(
                ForwardEvidenceEpisode.handoff_id == original.id
            )
        )
        assert original_episode is not None
        older_handoff = HandoffOffer(
            approval_id=original.approval_id,
            candidate_package_id=original.candidate_package_id,
            candidate_id=original.candidate_id,
            purpose=original.purpose,
            downstream_system_id=original.downstream_system_id,
            state="FEEDBACK_COMPLETE",
            feedback_state="FEEDBACK_COMPLETE",
            feedback_contract_snapshot=original.feedback_contract_snapshot,
        )
        session.add(older_handoff)
        session.flush()
        end = original_episode.observation_end
        session.add(
            ForwardEvidenceEpisode(
                handoff_id=older_handoff.id,
                state="FEEDBACK_COMPLETE",
                evidence={"return": -0.01},
                observation_start=end - timedelta(minutes=1),
                observation_end=end,
                sample_size=1,
                created_at=datetime.now(UTC),
            )
        )
        older_handoff_id = older_handoff.id

    delayed = client.post(
        f"/api/v1/handoffs/{older_handoff_id}/degradation-observations",
        json=_observation_payload(seeded.alpha_id),
    )
    assert delayed.status_code == 409
    assert delayed.json()["error"]["code"] == "FORWARD_EVIDENCE_OUT_OF_ORDER"
    with factory() as session:
        assert session.scalar(select(func.count()).select_from(DegradationObservation)) == 1
        assert session.scalar(select(func.count()).select_from(ResearchWakeEvent)) == 1


def test_paused_program_retains_wake_until_resume_consumes_it(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    seeded = _seed_handoff(engine, settings, client)
    paused = client.post(
        f"/api/v1/research-programs/{seeded.program_id}/pause",
        json={"expected_revision": seeded.program_revision, "reason": "operator review"},
    )
    assert paused.status_code == 200, paused.text

    observed = client.post(
        f"/api/v1/handoffs/{seeded.handoff_id}/degradation-observations",
        json=_observation_payload(seeded.alpha_id),
    )
    assert observed.status_code == 201, observed.text
    assert observed.json()["wake_state"] == "PENDING"
    assert observed.json()["cycle_id"] is None

    resumed = client.post(
        f"/api/v1/research-programs/{seeded.program_id}/resume",
        json={"expected_revision": paused.json()["revision"]},
    )
    assert resumed.status_code == 200, resumed.text
    assert resumed.json()["state"] == "ACTIVE"
    factory = create_session_factory(engine)
    with factory() as session:
        wake = session.scalar(select(ResearchWakeEvent))
        program = session.get(ResearchProgram, seeded.program_id)
        assert wake is not None and wake.state == "CONSUMED" and wake.cycle_id is not None
        assert program is not None and program.current_cycle_id == wake.cycle_id


def test_archived_program_keeps_its_wake_pending_without_restore(
    engine: Engine,
    settings: Settings,
) -> None:
    client = _client(engine, settings)
    seeded = _seed_handoff(engine, settings, client)
    archived = client.post(
        f"/api/v1/research-programs/{seeded.program_id}/archive",
        json={"expected_revision": seeded.program_revision, "reason": "retired"},
    )
    assert archived.status_code == 200, archived.text
    observed = client.post(
        f"/api/v1/handoffs/{seeded.handoff_id}/degradation-observations",
        json=_observation_payload(seeded.alpha_id),
    )
    assert observed.status_code == 201, observed.text
    assert observed.json()["wake_state"] == "PENDING"
    resume = client.post(
        f"/api/v1/research-programs/{seeded.program_id}/resume",
        json={"expected_revision": archived.json()["revision"]},
    )
    assert resume.status_code == 409
    assert resume.json()["error"]["code"] == "PROGRAM_STATE_CONFLICT"
    factory = create_session_factory(engine)
    with factory() as session:
        wake = session.scalar(select(ResearchWakeEvent))
        program = session.get(ResearchProgram, seeded.program_id)
        assert wake is not None and wake.state == "PENDING" and wake.cycle_id is None
        assert program is not None and program.state == "ARCHIVED"


@pytest.mark.parametrize("state", ("COOLING", "WAITING_FOR_FEEDBACK"))
def test_ordinary_lifecycle_wake_reactivates_and_consumes(
    engine: Engine,
    settings: Settings,
    state: str,
) -> None:
    client = _client(engine, settings)
    seeded = _seed_handoff(engine, settings, client)
    factory = create_session_factory(engine)
    with factory() as session, session.begin():
        program = session.get(ResearchProgram, seeded.program_id)
        assert program is not None
        program.state = state
        program.revision += 1

    observed = client.post(
        f"/api/v1/handoffs/{seeded.handoff_id}/degradation-observations",
        json=_observation_payload(seeded.alpha_id),
    )
    assert observed.status_code == 201, observed.text
    assert observed.json()["wake_state"] == "CONSUMED"
    with factory() as session:
        program = session.get(ResearchProgram, seeded.program_id)
        assert program is not None and program.state == "ACTIVE"
