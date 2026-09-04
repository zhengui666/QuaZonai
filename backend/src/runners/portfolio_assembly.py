"""Lease-fenced deterministic Portfolio Assembly worker."""

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
from portfolio_input_service import assemble_trusted_portfolio_input
from runtime_config import load_effective_settings
from settings import Settings


def _locked_job(session: Session, lease: JobLease) -> UUID:
    job = session.scalar(select(Job).where(Job.id == lease.job_id).with_for_update())
    if job is None or job.kind != "PORTFOLIO_ASSEMBLY":
        raise QfError("JOB_NOT_FOUND", "Portfolio Assembly job does not exist.", 404)
    if job.state != "LEASED":
        raise QfError("JOB_STATE_CONFLICT", "Portfolio Assembly job is not leased.", 409)
    if job.resource_type != "portfolio_assembly_input":
        raise QfError(
            "PORTFOLIO_ASSEMBLY_RESOURCE_INVALID",
            "Portfolio Assembly job must reference an Assembly Input.",
            409,
        )
    if job.payload != {}:
        raise QfError(
            "PORTFOLIO_ASSEMBLY_PAYLOAD_FORBIDDEN",
            "Portfolio Assembly inputs must not be carried in a job payload.",
            409,
        )
    return job.resource_id


def run_portfolio_assembly(settings: Settings, lease: JobLease) -> None:
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        with factory.begin() as session:
            input_id = _locked_job(session, lease)
            assemble_trusted_portfolio_input(session, input_id)
    finally:
        engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one trusted Portfolio Assembly")
    parser.add_argument("action", choices=["run"])
    parser.add_argument("job_id")
    parser.add_argument("--lease-owner", required=True)
    parser.add_argument("--lease-attempt", required=True, type=int)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    settings = load_effective_settings(Settings.from_env())
    run_portfolio_assembly(
        settings,
        JobLease(UUID(args.job_id), args.lease_owner, args.lease_attempt),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
