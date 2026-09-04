"""Lease-fenced Promotion workers."""

from __future__ import annotations

import argparse
from collections.abc import Sequence
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from db.models import Job
from db.session import create_database_engine
from errors import QfError
from jobs import JobLease, create_lease_fenced_session_factory
from promotion_service import maybe_enqueue_p2l, maybe_enqueue_p2p
from runtime_config import load_effective_settings
from settings import Settings


def _locked_job(session: Session, lease: JobLease) -> UUID:
    job = session.scalar(select(Job).where(Job.id == lease.job_id).with_for_update())
    if job is None or job.kind != "PAPER_TO_LIVE_PROMOTION":
        raise QfError("JOB_NOT_FOUND", "Paper-to-Live job does not exist.", 404)
    if job.state != "LEASED":
        raise QfError("JOB_STATE_CONFLICT", "Paper-to-Live job is not leased.", 409)
    if job.resource_type != "forward_evidence_episode":
        raise QfError("PAPER_TO_LIVE_RESOURCE_INVALID", "Paper-to-Live job resource is invalid.", 409)
    if job.payload != {}:
        raise QfError("PAPER_TO_LIVE_PAYLOAD_FORBIDDEN", "Paper-to-Live jobs carry no payload.", 409)
    return job.resource_id


def _locked_p2p_job(session: Session, lease: JobLease) -> UUID:
    job = session.scalar(select(Job).where(Job.id == lease.job_id).with_for_update())
    if job is None or job.kind != "PORTFOLIO_TO_PAPER_PROMOTION":
        raise QfError("JOB_NOT_FOUND", "Portfolio-to-Paper job does not exist.", 404)
    if job.state != "LEASED":
        raise QfError("JOB_STATE_CONFLICT", "Portfolio-to-Paper job is not leased.", 409)
    if job.resource_type != "portfolio_evaluation_episode":
        raise QfError("PORTFOLIO_TO_PAPER_RESOURCE_INVALID", "Portfolio-to-Paper job resource is invalid.", 409)
    if job.payload != {}:
        raise QfError("PORTFOLIO_TO_PAPER_PAYLOAD_FORBIDDEN", "Portfolio-to-Paper jobs carry no payload.", 409)
    return job.resource_id


def run_portfolio_to_paper_promotion(settings: Settings, lease: JobLease) -> None:
    """Run one fenced P2P writer from the frozen Portfolio Episode."""
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        with factory.begin() as session:
            episode_id = _locked_p2p_job(session, lease)
            maybe_enqueue_p2p(session, portfolio_evaluation_episode_id=episode_id)
    finally:
        engine.dispose()


def run_paper_to_live_promotion(settings: Settings, lease: JobLease) -> None:
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        with factory.begin() as session:
            episode_id = _locked_job(session, lease)
            maybe_enqueue_p2l(session, forward_evidence_episode_id=episode_id)
    finally:
        engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one trusted Paper-to-Live promotion")
    parser.add_argument("action", choices=["run", "run-p2p"])
    parser.add_argument("job_id")
    parser.add_argument("--lease-owner", required=True)
    parser.add_argument("--lease-attempt", required=True, type=int)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    settings = load_effective_settings(Settings.from_env())
    lease = JobLease(
        job_id=UUID(args.job_id), owner=args.lease_owner, attempt=args.lease_attempt
    )
    if args.action == "run-p2p":
        run_portfolio_to_paper_promotion(settings, lease)
    else:
        run_paper_to_live_promotion(settings, lease)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
