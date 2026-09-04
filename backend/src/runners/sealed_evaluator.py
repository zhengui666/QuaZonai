"""Run one trusted, execution-free sealed Alpha evaluation job."""

from __future__ import annotations

import argparse
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from db.models import Job
from db.session import create_database_engine
from errors import QfError
from jobs import JobLease, create_lease_fenced_session_factory
from portfolio_evaluation_service import accept_portfolio_evaluation_result
from portfolio_input_service import accept_portfolio_input_evaluation_result
from research_engine.alpha_intake import accept_discovery_evaluation_result
from research_engine.trusted_evaluator_service import (
    accept_alpha_evaluation_result,
    ensure_trusted_evaluator_available,
    parse_alpha_evaluation_result,
    parse_discovery_evaluation_result,
    parse_portfolio_evaluation_result,
    parse_portfolio_input_evaluation_result,
    prepare_alpha_evaluation,
    prepare_discovery_evaluation,
    prepare_portfolio_evaluation_request,
    prepare_portfolio_input_evaluation,
    run_trusted_evaluator,
)
from runtime_config import load_effective_settings
from settings import Settings


def _locked_evaluator_job(
    session: Session,
    lease: JobLease,
    *,
    kind: str,
    resource_type: str,
) -> UUID:
    """Lock and reject payload-bearing jobs before reading any Core facts."""
    job = session.scalar(select(Job).where(Job.id == lease.job_id).with_for_update())
    if job is None or job.kind != kind:
        raise QfError("JOB_NOT_FOUND", "Trusted evaluator job does not exist.", 404)
    if job.state != "LEASED":
        raise QfError("JOB_STATE_CONFLICT", "Trusted evaluator job is not leased.", 409)
    if job.payload != {}:
        raise QfError(
            f"{kind}_RAW_PAYLOAD_FORBIDDEN",
            "Trusted evaluator inputs must not be carried in a job payload.",
            409,
        )
    if job.resource_type != resource_type:
        raise QfError(
            "ALPHA_EVALUATION_RESOURCE_INVALID",
            "Trusted evaluator job has an invalid resource type.",
            409,
        )
    return job.resource_id


def run_discovery_evaluation(settings: Settings, lease: JobLease) -> None:
    """Run the fixed Discovery evaluator boundary for one frozen Core fact."""
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        with factory.begin() as session:
            discovery_id = _locked_evaluator_job(
                session,
                lease,
                kind="DISCOVERY_EVALUATION",
                resource_type="alpha_discovery_evaluation",
            )
        ensure_trusted_evaluator_available(settings)
        with factory.begin() as session:
            discovery_id = _locked_evaluator_job(
                session,
                lease,
                kind="DISCOVERY_EVALUATION",
                resource_type="alpha_discovery_evaluation",
            )
            request = prepare_discovery_evaluation(session, discovery_id)
        payload = run_trusted_evaluator(settings, request.descriptor)
        result = parse_discovery_evaluation_result(payload, request)
        with factory.begin() as session:
            current_discovery_id = _locked_evaluator_job(
                session,
                lease,
                kind="DISCOVERY_EVALUATION",
                resource_type="alpha_discovery_evaluation",
            )
            if current_discovery_id != result.discovery_evaluation_id:
                raise QfError(
                    "DISCOVERY_EVALUATION_RESOURCE_INVALID",
                    "Trusted evaluator result does not match its frozen Discovery fact.",
                    409,
                )
            accept_discovery_evaluation_result(session, result)
    finally:
        engine.dispose()


def run_alpha_evaluation(settings: Settings, lease: JobLease) -> None:
    """Run the fixed sealed Alpha evaluator boundary for one frozen Assignment."""
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        with factory.begin() as session:
            assignment_id = _locked_evaluator_job(
                session,
                lease,
                kind="ALPHA_EVALUATION",
                resource_type="alpha_evaluation_assignment",
            )
        ensure_trusted_evaluator_available(settings)
        with factory.begin() as session:
            assignment_id = _locked_evaluator_job(
                session,
                lease,
                kind="ALPHA_EVALUATION",
                resource_type="alpha_evaluation_assignment",
            )
            request = prepare_alpha_evaluation(session, assignment_id)
        payload = run_trusted_evaluator(settings, request.descriptor)
        result = parse_alpha_evaluation_result(payload, request)
        with factory.begin() as session:
            current_assignment_id = _locked_evaluator_job(
                session,
                lease,
                kind="ALPHA_EVALUATION",
                resource_type="alpha_evaluation_assignment",
            )
            if current_assignment_id != result.input.assignment_id:
                raise QfError(
                    "ALPHA_EVALUATION_RESOURCE_INVALID",
                    "Trusted evaluator result does not match its frozen Alpha Assignment.",
                    409,
                )
            accept_alpha_evaluation_result(session, result)
    finally:
        engine.dispose()


def run_portfolio_input_evaluation(settings: Settings, lease: JobLease) -> None:
    """Run the fixed covariance evaluator for one frozen Input Assignment."""
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        with factory.begin() as session:
            assignment_id = _locked_evaluator_job(
                session,
                lease,
                kind="PORTFOLIO_INPUT_EVALUATION",
                resource_type="portfolio_input_evaluation_assignment",
            )
        ensure_trusted_evaluator_available(settings)
        with factory.begin() as session:
            assignment_id = _locked_evaluator_job(
                session,
                lease,
                kind="PORTFOLIO_INPUT_EVALUATION",
                resource_type="portfolio_input_evaluation_assignment",
            )
            request = prepare_portfolio_input_evaluation(session, assignment_id)
        payload = run_trusted_evaluator(settings, request.descriptor)
        result = parse_portfolio_input_evaluation_result(payload, request)
        with factory.begin() as session:
            current_assignment_id = _locked_evaluator_job(
                session,
                lease,
                kind="PORTFOLIO_INPUT_EVALUATION",
                resource_type="portfolio_input_evaluation_assignment",
            )
            if current_assignment_id != result.input.assignment_id:
                raise QfError(
                    "PORTFOLIO_INPUT_EVALUATION_RESOURCE_INVALID",
                    "Trusted evaluator result does not match its frozen Input Assignment.",
                    409,
                )
            accept_portfolio_input_evaluation_result(session, result)
    finally:
        engine.dispose()


def run_portfolio_evaluation(settings: Settings, lease: JobLease) -> None:
    """Run the fixed Portfolio evidence evaluator for one frozen Episode."""
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        with factory.begin() as session:
            assignment_id = _locked_evaluator_job(
                session,
                lease,
                kind="PORTFOLIO_EVALUATION",
                resource_type="portfolio_evaluation_assignment",
            )
        ensure_trusted_evaluator_available(settings)
        with factory.begin() as session:
            assignment_id = _locked_evaluator_job(
                session,
                lease,
                kind="PORTFOLIO_EVALUATION",
                resource_type="portfolio_evaluation_assignment",
            )
            request = prepare_portfolio_evaluation_request(session, assignment_id)
        payload = run_trusted_evaluator(settings, request.descriptor)
        result = parse_portfolio_evaluation_result(payload, request)
        with factory.begin() as session:
            current_assignment_id = _locked_evaluator_job(
                session,
                lease,
                kind="PORTFOLIO_EVALUATION",
                resource_type="portfolio_evaluation_assignment",
            )
            if current_assignment_id != result.input.assignment_id:
                raise QfError(
                    "PORTFOLIO_EVALUATION_RESOURCE_INVALID",
                    "Trusted evaluator result does not match its frozen Portfolio Assignment.",
                    409,
                )
            accept_portfolio_evaluation_result(session, result)
    finally:
        engine.dispose()


def run_sealed_evaluation(settings: Settings, lease: JobLease) -> None:
    """Consume only a leased Alpha Evaluation Episode with evaluator-owned inputs."""
    engine = create_database_engine(settings)
    factory = create_lease_fenced_session_factory(engine, lease)
    try:
        with factory.begin() as session:
            job = session.scalar(select(Job).where(Job.id == lease.job_id).with_for_update())
            if job is None or job.kind != "SEALED_EVALUATION":
                raise QfError("JOB_NOT_FOUND", "Sealed evaluation job does not exist.", 404)
            if job.state != "LEASED":
                raise QfError("JOB_STATE_CONFLICT", "Sealed evaluation job is not leased.", 409)
            if job.resource_type != "alpha_evaluation_episode":
                raise QfError(
                    "ALPHA_EVALUATION_RESOURCE_INVALID",
                    "Sealed evaluation job has an invalid resource type.",
                    409,
                )
            if job.payload:
                raise QfError(
                    "SEALED_EVALUATION_RAW_PAYLOAD_FORBIDDEN",
                    "Sealed evaluation inputs must not be carried in a job payload.",
                    409,
                )
            raise QfError(
                "SEALED_EVALUATOR_ASSIGNMENT_UNAVAILABLE",
                "No trusted sealed-evaluation assignment provider is configured.",
                503,
            )
    finally:
        engine.dispose()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one independent sealed Alpha evaluation")
    parser.add_argument(
        "action",
        choices=["run", "run-discovery", "run-alpha", "run-portfolio-input", "run-portfolio"],
    )
    parser.add_argument("job_id")
    parser.add_argument("--lease-owner", required=True)
    parser.add_argument("--lease-attempt", required=True, type=int)
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    settings = load_effective_settings(Settings.from_env())
    lease = JobLease(
        job_id=UUID(args.job_id),
        owner=args.lease_owner,
        attempt=args.lease_attempt,
    )
    if args.action == "run":
        run_sealed_evaluation(settings, lease)
    elif args.action == "run-discovery":
        run_discovery_evaluation(settings, lease)
    elif args.action == "run-alpha":
        run_alpha_evaluation(settings, lease)
    elif args.action == "run-portfolio-input":
        run_portfolio_input_evaluation(settings, lease)
    else:
        run_portfolio_evaluation(settings, lease)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
