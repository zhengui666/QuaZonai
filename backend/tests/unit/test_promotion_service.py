from __future__ import annotations

from datetime import UTC, datetime, timedelta
from decimal import Decimal
from uuid import uuid4

import pytest
from sqlalchemy import create_engine, event, select
from sqlalchemy.orm import Session

from candidate_packages import (
    finalize_candidate_package_build,
    prepare_candidate_package_build,
    write_candidate_package_archive,
)
from db.models import (
    ApprovalSnapshot,
    Base,
    FeedbackContractMetricRequirement,
    FeedbackContractVersion,
    DownstreamConnectionVersion,
    Job,
    PreflightReceipt,
    PromotionEvaluation,
    PromotionPolicyGate,
)
from errors import QfError
from jobs import JobLease, claim_next_job
from portfolio_evaluation_service import (
    accept_portfolio_evaluation_result,
    ensure_portfolio_evaluation,
    prepare_portfolio_evaluation,
)
from promotion_service import (
    FeedbackHeader,
    TypedFeedbackMetric,
    accept_paper_feedback,
    approve_typed_live_handoff,
    approve_typed_paper_handoff,
    maybe_enqueue_p2l,
    maybe_enqueue_p2p,
)
from test_candidate_package_build import _assembled_candidate
from test_portfolio_input_service import _portfolio_result_input
from runners import promotion as promotion_runner


def _engine(tmp_path):
    engine = create_engine(f"sqlite+pysqlite:///{tmp_path / 'promotion.sqlite'}")

    @event.listens_for(engine, "connect")
    def _foreign_keys(connection, _record):  # type: ignore[no-untyped-def]
        connection.execute("PRAGMA foreign_keys = ON")

    Base.metadata.create_all(engine)
    return engine


def test_package_and_portfolio_pass_create_one_frozen_paper_approval(tmp_path, settings, monkeypatch) -> None:
    pytest.importorskip("numpy")
    engine = _engine(tmp_path)
    try:
        candidate_id, _ = _assembled_candidate(engine)
        with Session(engine) as session:
            evaluation_assignment = ensure_portfolio_evaluation(session, candidate_id=candidate_id)
            descriptor = prepare_portfolio_evaluation(session, evaluation_assignment.id)
            episode = accept_portfolio_evaluation_result(
                session, _portfolio_result_input(descriptor)
            )
            assert maybe_enqueue_p2p(session, portfolio_evaluation_episode_id=episode.id) is None
            session.commit()

        with Session(engine) as session:
            build = prepare_candidate_package_build(session, candidate_id)
            session.commit()
        write_candidate_package_archive(settings, build)
        with Session(engine) as session:
            package = finalize_candidate_package_build(session, build)
            assert package.state == "AVAILABLE"
            session.commit()
        with Session(engine) as session, session.begin():
            job = claim_next_job(
                session,
                owner="promotion-worker",
                lease_seconds=60,
                kind="PORTFOLIO_TO_PAPER_PROMOTION",
            )
            assert job is not None and job.lease_owner is not None
            lease = JobLease(job.id, job.lease_owner, job.attempt)
        monkeypatch.setattr(promotion_runner, "create_database_engine", lambda _: engine)
        promotion_runner.run_portfolio_to_paper_promotion(settings, lease)

        with Session(engine) as session:
            evaluation = session.scalar(select(PromotionEvaluation))
            approval = session.scalar(select(ApprovalSnapshot))
            assert evaluation is not None
            assert evaluation.purpose == "PORTFOLIO_TO_PAPER"
            assert evaluation.outcome == "PASS"
            assert evaluation.action == "MANUAL_APPROVAL"
            assert approval is not None
            assert approval.state == "PENDING"
            assert approval.promotion_evaluation_id == evaluation.id
            assert approval.promotion_purpose == "PORTFOLIO_TO_PAPER"
            assert approval.purpose == "PAPER"
    finally:
        engine.dispose()


def test_p2p_worker_rejects_nonempty_payload_before_loading_facts(tmp_path, settings, monkeypatch) -> None:
    engine = _engine(tmp_path)
    try:
        with Session(engine) as session, session.begin():
            job = Job(
                kind="PORTFOLIO_TO_PAPER_PROMOTION",
                resource_type="portfolio_evaluation_episode",
                resource_id=uuid4(),
                state="LEASED",
                payload={"episode_id": "forbidden"},
                attempt=1,
                lease_owner="promotion-worker",
                lease_expires_at=datetime.now(UTC) + timedelta(seconds=60),
            )
            session.add(job)
            session.flush()
            lease = JobLease(job.id, "promotion-worker", 1)
        monkeypatch.setattr(promotion_runner, "create_database_engine", lambda _: engine)
        with pytest.raises(QfError, match="PORTFOLIO_TO_PAPER_PAYLOAD_FORBIDDEN"):
            promotion_runner.run_portfolio_to_paper_promotion(settings, lease)
    finally:
        engine.dispose()


def test_complete_typed_paper_feedback_queues_and_decides_live_promotion(tmp_path, settings, monkeypatch) -> None:
    pytest.importorskip("numpy")
    engine = _engine(tmp_path)
    try:
        candidate_id, _ = _assembled_candidate(engine)
        with Session(engine) as session:
            evaluation_assignment = ensure_portfolio_evaluation(session, candidate_id=candidate_id)
            descriptor = prepare_portfolio_evaluation(session, evaluation_assignment.id)
            accept_portfolio_evaluation_result(
                session, _portfolio_result_input(descriptor)
            )
            for connection in session.scalars(select(DownstreamConnectionVersion)):
                connection.package_contract_version = "1"
            for receipt in session.scalars(select(PreflightReceipt)):
                receipt.contract_version = "1"
            session.commit()

        with Session(engine) as session:
            build = prepare_candidate_package_build(session, candidate_id)
            session.commit()
        write_candidate_package_archive(settings, build)
        with Session(engine) as session:
            finalize_candidate_package_build(session, build)
            job = claim_next_job(
                session,
                owner="promotion-worker",
                lease_seconds=60,
                kind="PORTFOLIO_TO_PAPER_PROMOTION",
            )
            assert job is not None
            assert job.lease_owner is not None
            p2p_lease = JobLease(job.id, job.lease_owner, job.attempt)
            session.commit()

        monkeypatch.setattr(promotion_runner, "create_database_engine", lambda _: engine)
        promotion_runner.run_portfolio_to_paper_promotion(settings, p2p_lease)

        with Session(engine) as session:
            approval = session.scalar(select(ApprovalSnapshot))
            assert approval is not None
            for receipt in session.scalars(select(PreflightReceipt)):
                receipt.valid_until = datetime.now(UTC) + timedelta(days=1)
            handoff = approve_typed_paper_handoff(session, approval.id)
            handoff.state = "DOWNSTREAM_ACCEPTED"
            handoff.feedback_state = "FEEDBACK_PENDING"
            contracts = list(session.scalars(select(FeedbackContractVersion)))
            assert len(contracts) == 2
            for contract in contracts:
                session.add(
                    FeedbackContractMetricRequirement(
                        feedback_contract_version_id=contract.id,
                        metric_code="return",
                        ordinal=1,
                    )
                )
            target_policy_id = handoff.paper_to_live_policy_version_id
            assert target_policy_id is not None
            session.add(
                PromotionPolicyGate(
                    policy_version_id=target_policy_id,
                    metric_code="return",
                    comparator="MINIMUM",
                    threshold=0,
                    ordinal=1,
                )
            )
            session.flush()
            forward = accept_paper_feedback(
                session,
                handoff_id=handoff.id,
                header=FeedbackHeader(
                    observation_start=datetime(2026, 9, 3, tzinfo=UTC),
                    observation_end=datetime(2026, 9, 4, tzinfo=UTC),
                    sample_size=1,
                ),
                metrics=(TypedFeedbackMetric("return", "AVAILABLE", Decimal("0.1")),),
            )
            assert (
                accept_paper_feedback(
                    session,
                    handoff_id=handoff.id,
                    header=FeedbackHeader(
                        observation_start=datetime(2026, 9, 3, tzinfo=UTC),
                        observation_end=datetime(2026, 9, 4, tzinfo=UTC),
                        sample_size=1,
                    ),
                    metrics=(TypedFeedbackMetric("return", "AVAILABLE", Decimal("0.1")),),
                ).id
                == forward.id
            )
            with pytest.raises(QfError, match="FEEDBACK_CONTRACT_CONFLICT"):
                accept_paper_feedback(
                    session,
                    handoff_id=handoff.id,
                    header=FeedbackHeader(
                        observation_start=datetime(2026, 9, 3, tzinfo=UTC),
                        observation_end=datetime(2026, 9, 4, tzinfo=UTC),
                        sample_size=1,
                    ),
                    metrics=(TypedFeedbackMetric("return", "AVAILABLE", Decimal("0.2")),),
                )
            forward_id = forward.id
            session.commit()

        with Session(engine) as session:
            job = session.scalar(
                select(Job).where(
                    Job.kind == "PAPER_TO_LIVE_PROMOTION",
                        Job.resource_id == forward_id,
                )
            )
            assert job is not None and job.payload == {}
            live_eval = maybe_enqueue_p2l(session, forward_evidence_episode_id=forward_id)
            assert live_eval is not None and live_eval.outcome == "PASS"
            live_approval = session.scalar(
                select(ApprovalSnapshot).where(
                    ApprovalSnapshot.promotion_evaluation_id == live_eval.id
                )
            )
            assert live_approval is not None
            assert live_approval.purpose == "LIVE"
            assert live_approval.state == "PENDING"
            live_handoff = approve_typed_live_handoff(session, live_approval.id)
            assert live_handoff.purpose == "LIVE"
            assert live_handoff.promotion_purpose == "PAPER_TO_LIVE"
            assert live_handoff.state == "AVAILABLE"
    finally:
        engine.dispose()
