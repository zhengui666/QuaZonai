"""Lease-fenced target-only Candidate Package builder."""

from __future__ import annotations

import argparse
from collections.abc import Sequence
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from candidate_packages import (
    CandidatePackageBuild,
    candidate_package_filesystem_lock,
    finalize_candidate_package_build,
    prepare_candidate_package_build,
    write_candidate_package_archive,
)
from db.models import Job
from db.session import create_database_engine
from errors import QfError
from jobs import JobLease, create_lease_fenced_session_factory
from runtime_config import load_effective_settings
from settings import Settings


def _locked_candidate_package_job(session: Session, lease: JobLease) -> UUID:
    """Reject all caller-controlled job facts before loading the Candidate."""
    job = session.scalar(select(Job).where(Job.id == lease.job_id).with_for_update())
    if job is None or job.kind != "CANDIDATE_PACKAGE_BUILD":
        raise QfError("JOB_NOT_FOUND", "Candidate Package build job does not exist.", 404)
    if job.state != "LEASED":
        raise QfError("JOB_STATE_CONFLICT", "Candidate Package build job is not leased.", 409)
    if job.payload != {}:
        raise QfError(
            "CANDIDATE_PACKAGE_BUILD_PAYLOAD_FORBIDDEN",
            "Candidate Package build inputs must not be carried in a job payload.",
            409,
        )
    if job.resource_type != "portfolio_candidate":
        raise QfError(
            "CANDIDATE_PACKAGE_BUILD_RESOURCE_INVALID",
            "Candidate Package build job must reference a Portfolio Candidate.",
            409,
        )
    return job.resource_id


def _revalidate_building_package(
    session: Session,
    lease: JobLease,
    expected_build: CandidatePackageBuild,
) -> None:
    """Fence after taking the filesystem lock before touching deterministic paths."""
    candidate_id = _locked_candidate_package_job(session, lease)
    if candidate_id != expected_build.candidate_id:
        raise QfError(
            "CANDIDATE_PACKAGE_BUILD_RESOURCE_INVALID",
            "Candidate Package build job changed its Candidate resource.",
            409,
        )
    build = prepare_candidate_package_build(session, candidate_id)
    if build != expected_build:
        raise QfError(
            "CANDIDATE_PACKAGE_V1_CONFLICT",
            "Candidate Package reservation changed before filesystem work.",
            409,
        )


def run_candidate_package_build(settings: Settings, lease: JobLease) -> None:
    """Reserve, build/verify outside SQL, then finalize the same Package row."""
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        with factory.begin() as session:
            candidate_id = _locked_candidate_package_job(session, lease)
            build = prepare_candidate_package_build(session, candidate_id)
        with candidate_package_filesystem_lock(settings, build.package_id):
            with factory.begin() as session:
                _revalidate_building_package(session, lease, build)
            write_candidate_package_archive(settings, build)
            with factory.begin() as session:
                _revalidate_building_package(session, lease, build)
                finalize_candidate_package_build(session, build)
    finally:
        engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Build one target-only Candidate Package")
    parser.add_argument("action", choices=["run"])
    parser.add_argument("job_id")
    parser.add_argument("--lease-owner", required=True)
    parser.add_argument("--lease-attempt", required=True, type=int)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    settings = load_effective_settings(Settings.from_env())
    run_candidate_package_build(
        settings,
        JobLease(
            job_id=UUID(args.job_id),
            owner=args.lease_owner,
            attempt=args.lease_attempt,
        ),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
