from __future__ import annotations

from datetime import UTC, datetime, timedelta
from decimal import Decimal
from uuid import uuid4

import pytest
from sqlalchemy import create_engine, event, select
from sqlalchemy.orm import Session

import promotion_service
from candidate_packages import (
    finalize_candidate_package_build,
    prepare_candidate_package_build,
    write_candidate_package_archive,
)
from db.models import (
    AlphaQualification,
    ApprovalSnapshot,
    Base,
    CandidatePackage,
    DegradationObservation,
    FeedbackContractMetricRequirement,
    FeedbackContractVersion,
    DownstreamConnectionVersion,
    Job,
    PortfolioAssemblyInputMember,
    PortfolioCandidate,
    PortfolioCandidateMember,
    PortfolioEvaluationEpisode,
    PreflightReceipt,
    PromotionEvaluation,
    PromotionPolicyGate,
    PromotionPolicyVersion,
    HandoffOffer,
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
from test_portfolio_input_service import (
    _assembled_candidate_for_evaluation,
    _portfolio_result_input,
)
from runners import promotion as promotion_runner


def _engine(tmp_path):
    engine = create_engine(f"sqlite+pysqlite:///{tmp_path / 'promotion.sqlite'}")

    @event.listens_for(engine, "connect")
    def _foreign_keys(connection, _record):  # type: ignore[no-untyped-def]
        connection.execute("PRAGMA foreign_keys = ON")

    Base.metadata.create_all(engine)
    return engine


def _promotion_source(
    session: Session,
    monkeypatch: pytest.MonkeyPatch,
) -> tuple[PortfolioCandidate, CandidatePackage, PromotionPolicyVersion, PortfolioEvaluationEpisode]:
    candidate, input_row, input_assignment = _assembled_candidate_for_evaluation(session)
    members = list(
        session.scalars(
            select(PortfolioAssemblyInputMember).where(
                PortfolioAssemblyInputMember.input_id == input_row.id
            )
        )
    )
    assert members
    session.add_all(
        PortfolioCandidateMember(
            candidate_id=candidate.id,
            alpha_qualification_id=member.alpha_qualification_id,
            role="PRIMARY_ALPHA",
            target_weight=Decimal("0.5"),
        )
        for member in members
    )
    package = CandidatePackage(
        id=uuid4(),
        candidate_id=candidate.id,
        revision=1,
        contract_version="CANDIDATE_PACKAGE_V1",
        state="AVAILABLE",
        manifest_json={},
        relative_path="test/candidate-package.zip",
        payload={},
        created_at=datetime.now(UTC),
    )
    session.add(package)
    session.flush()
    monkeypatch.setattr(promotion_service, "is_trusted_candidate_package", lambda *_: True)
    assignment = ensure_portfolio_evaluation(session, candidate_id=candidate.id)
    descriptor = prepare_portfolio_evaluation(session, assignment.id)
    episode = accept_portfolio_evaluation_result(session, _portfolio_result_input(descriptor))
    policy = session.get(PromotionPolicyVersion, input_assignment.promotion_policy_version_id)
    assert policy is not None
    return candidate, package, policy, episode


def _paper_handoff(
    session: Session,
    monkeypatch: pytest.MonkeyPatch,
    *,
    paper_metric: str = "return",
    live_metric: str = "return",
) -> tuple[PortfolioCandidate, HandoffOffer, FeedbackContractVersion, FeedbackContractVersion]:
    candidate, _package, policy, episode = _promotion_source(session, monkeypatch)
    paper_contract = session.get(FeedbackContractVersion, policy.paper_feedback_contract_version_id)
    live_policy = session.get(PromotionPolicyVersion, policy.paper_to_live_policy_version_id)
    assert paper_contract is not None and live_policy is not None
    live_contract = session.get(FeedbackContractVersion, live_policy.live_feedback_contract_version_id)
    assert live_contract is not None
    session.add_all(
        (
            FeedbackContractMetricRequirement(
                feedback_contract_version_id=paper_contract.id,
                metric_code=paper_metric,
                ordinal=1,
            ),
            FeedbackContractMetricRequirement(
                feedback_contract_version_id=live_contract.id,
                metric_code=live_metric,
                ordinal=1,
            ),
            PromotionPolicyGate(
                policy_version_id=live_policy.id,
                metric_code=live_metric,
                comparator="MINIMUM",
                threshold=0,
                ordinal=1,
            ),
        )
    )
    session.flush()
    p2p = maybe_enqueue_p2p(session, portfolio_evaluation_episode_id=episode.id)
    assert p2p is not None and p2p.outcome == "PASS"
    approval = session.scalar(
        select(ApprovalSnapshot).where(ApprovalSnapshot.promotion_evaluation_id == p2p.id)
    )
    assert approval is not None
    handoff = approve_typed_paper_handoff(session, approval.id)
    handoff.state = "DOWNSTREAM_ACCEPTED"
    handoff.feedback_state = "FEEDBACK_PENDING"
    return candidate, handoff, paper_contract, live_contract


def test_p2p_receipt_requires_actual_package_contract(tmp_path, monkeypatch) -> None:
    engine = _engine(tmp_path)
    try:
        with Session(engine) as session:
            _candidate, _package, policy, episode = _promotion_source(session, monkeypatch)
            connection = session.get(
                DownstreamConnectionVersion, policy.paper_connection_version_id
            )
            receipt = session.get(PreflightReceipt, policy.paper_preflight_receipt_id)
            assert connection is not None and receipt is not None
            connection.package_contract_version = "OTHER_PACKAGE_V1"
            receipt.contract_version = connection.package_contract_version
            with pytest.raises(QfError, match="PROMOTION_PREFLIGHT_STALE"):
                maybe_enqueue_p2p(session, portfolio_evaluation_episode_id=episode.id)
            assert (
                session.scalar(
                    select(PromotionEvaluation).where(
                        PromotionEvaluation.purpose == "PORTFOLIO_TO_PAPER"
                    )
                )
                is None
            )
    finally:
        engine.dispose()


def test_paper_feedback_uses_paper_contract_and_rejects_future_end(
    tmp_path, monkeypatch
) -> None:
    engine = _engine(tmp_path)
    try:
        with Session(engine) as session:
            _candidate, handoff, paper_contract, live_contract = _paper_handoff(
                session,
                monkeypatch,
                paper_metric="paper_return",
                live_metric="live_return",
            )
            live_contract.minimum_valid_sample_size = 99
            live_contract.minimum_observation_seconds = 99
            now = datetime.now(UTC)
            with pytest.raises(QfError, match="FEEDBACK_CONTRACT_INVALID"):
                accept_paper_feedback(
                    session,
                    handoff_id=handoff.id,
                    header=FeedbackHeader(
                        observation_start=now - timedelta(seconds=2),
                        observation_end=now + timedelta(minutes=1),
                        sample_size=paper_contract.minimum_valid_sample_size,
                    ),
                    metrics=(
                        TypedFeedbackMetric("paper_return", "AVAILABLE", Decimal("0.1")),
                    ),
                )
            forward = accept_paper_feedback(
                session,
                handoff_id=handoff.id,
                header=FeedbackHeader(
                    observation_start=now - timedelta(seconds=3),
                    observation_end=now - timedelta(seconds=1),
                    sample_size=paper_contract.minimum_valid_sample_size,
                ),
                metrics=(
                    TypedFeedbackMetric("paper_return", "AVAILABLE", Decimal("0.1")),
                ),
            )
            assert forward.state == "FEEDBACK_COMPLETE"
    finally:
        engine.dispose()


def test_p2l_blocks_degrading_relational_alpha_member(tmp_path, monkeypatch) -> None:
    engine = _engine(tmp_path)
    try:
        with Session(engine) as session:
            candidate, handoff, paper_contract, _live_contract = _paper_handoff(
                session, monkeypatch
            )
            member = session.scalar(
                select(PortfolioCandidateMember).where(
                    PortfolioCandidateMember.candidate_id == candidate.id
                )
            )
            assert member is not None
            alpha = session.get(AlphaQualification, member.alpha_qualification_id)
            assert alpha is not None
            alpha.degradation_state = "DEGRADING"
            now = datetime.now(UTC)
            forward = accept_paper_feedback(
                session,
                handoff_id=handoff.id,
                header=FeedbackHeader(
                    observation_start=now - timedelta(seconds=3),
                    observation_end=now - timedelta(seconds=1),
                    sample_size=paper_contract.minimum_valid_sample_size,
                ),
                metrics=(TypedFeedbackMetric("return", "AVAILABLE", Decimal("0.1")),),
            )
            assert maybe_enqueue_p2l(session, forward_evidence_episode_id=forward.id) is None
            alpha.degradation_state = "HEALTHY"
            assert alpha.program_id is not None
            session.add(
                DegradationObservation(
                    program_id=alpha.program_id,
                    forward_evidence_episode_id=forward.id,
                    subject_type="ALPHA",
                    subject_id=alpha.id,
                    metric_name="return",
                    severity=Decimal("0.5"),
                    confidence=Decimal("1"),
                    policy_revision="degradation-v1",
                    policy_snapshot={},
                    reason_code="ALPHA_DEGRADING",
                    state="DEGRADING",
                    consecutive_breaches=1,
                    evaluated=True,
                    created_at=datetime.now(UTC),
                )
            )
            assert maybe_enqueue_p2l(session, forward_evidence_episode_id=forward.id) is None
            assert (
                session.scalar(
                    select(PromotionEvaluation).where(
                        PromotionEvaluation.purpose == "PAPER_TO_LIVE"
                    )
                )
                is None
            )
    finally:
        engine.dispose()


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
            feedback_header = FeedbackHeader(
                observation_start=datetime.now(UTC) - timedelta(seconds=3),
                observation_end=datetime.now(UTC) - timedelta(seconds=1),
                sample_size=1,
            )
            forward = accept_paper_feedback(
                session,
                handoff_id=handoff.id,
                header=feedback_header,
                metrics=(TypedFeedbackMetric("return", "AVAILABLE", Decimal("0.1")),),
            )
            assert (
                accept_paper_feedback(
                    session,
                    handoff_id=handoff.id,
                    header=feedback_header,
                    metrics=(TypedFeedbackMetric("return", "AVAILABLE", Decimal("0.1")),),
                ).id
                == forward.id
            )
            with pytest.raises(QfError, match="FEEDBACK_CONTRACT_CONFLICT"):
                accept_paper_feedback(
                    session,
                    handoff_id=handoff.id,
                    header=feedback_header,
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
