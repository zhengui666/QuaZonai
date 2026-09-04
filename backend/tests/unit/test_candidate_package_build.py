from __future__ import annotations

from datetime import UTC, datetime, timedelta
from io import BytesIO
from pathlib import Path
from uuid import UUID, uuid4
from zipfile import ZipFile

import pytest
from fastapi.testclient import TestClient
from sqlalchemy import create_engine, event, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from candidate_packages import (
    CandidatePackageBuild,
    candidate_package_filesystem_lock,
    finalize_candidate_package_build,
    prepare_candidate_package_build,
    verify_candidate_package_archive,
    write_candidate_package_archive,
)
from db.models import ApprovalSnapshot, Base, CandidatePackage, DownstreamSystem, HandoffOffer, Job
from downstream_auth import install_service_token, issue_service_token
from downstream_contracts import feedback_contract_snapshot
from errors import QfError
from jobs import JobLease, claim_next_job, release_expired_leases
from main import create_app
from portfolio_input_service import assemble_trusted_portfolio_input, persist_portfolio_input_evaluation
from runners import candidate_package_build
from settings import Settings
from test_portfolio_input_service import _portfolio_facts, _result, _stage


def _file_engine(tmp_path: Path):
    engine = create_engine(f"sqlite+pysqlite:///{tmp_path / 'candidate-package.sqlite'}")

    @event.listens_for(engine, "connect")
    def _foreign_keys(connection, _record) -> None:  # type: ignore[no-untyped-def]
        connection.execute("PRAGMA foreign_keys = ON")

    Base.metadata.create_all(engine)
    return engine


def _assembled_candidate(engine) -> tuple[UUID, UUID]:
    pytest.importorskip("numpy")
    with Session(engine) as session:
        facts = _portfolio_facts(session)
        assignment = _stage(session, facts)
        input_row = persist_portfolio_input_evaluation(session, _result(assignment.id))
        assert input_row is not None
        candidate = assemble_trusted_portfolio_input(session, input_row.id)
        assert candidate is not None
        candidate_id, input_id = candidate.id, input_row.id
        session.commit()
    return candidate_id, input_id


def _claim_candidate_job(engine) -> JobLease:
    with Session(engine) as session, session.begin():
        job = claim_next_job(
            session,
            owner="candidate-package-worker",
            lease_seconds=60,
            kind="CANDIDATE_PACKAGE_BUILD",
        )
        assert job is not None
        assert job.kind == "CANDIDATE_PACKAGE_BUILD"
        assert job.lease_owner is not None
        return JobLease(job.id, job.lease_owner, job.attempt)


@pytest.mark.parametrize("broken_archive", ["missing", "corrupt"])
def test_candidate_package_child_rebuilds_same_reserved_package(
    settings: Settings,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    broken_archive: str,
) -> None:
    engine = _file_engine(tmp_path)
    try:
        candidate_id, _input_id = _assembled_candidate(engine)
        with Session(engine) as session:
            build = prepare_candidate_package_build(session, candidate_id)  # phase one only
            session.commit()
        archive_path = settings.package_root / build.relative_path
        archive_path.parent.mkdir(parents=True)
        if broken_archive == "corrupt":
            archive_path.write_bytes(b"not a zip")

        lease = _claim_candidate_job(engine)
        monkeypatch.setattr(candidate_package_build, "create_database_engine", lambda _: engine)
        candidate_package_build.run_candidate_package_build(settings, lease)

        with Session(engine) as session:
            package = session.scalar(
                select(CandidatePackage).where(CandidatePackage.candidate_id == candidate_id)
            )
            assert package is not None
            assert package.id == build.package_id
            assert package.revision == 1
            assert package.state == "AVAILABLE"
            assert package.payload == {}
            assert package.manifest_json == build.manifest
            assert package.relative_path == build.relative_path
            assert prepare_candidate_package_build(session, candidate_id) == build
            assert session.scalar(
                select(Job).where(
                    Job.kind == "CANDIDATE_PACKAGE_BUILD",
                    Job.resource_type == "portfolio_candidate",
                    Job.resource_id == candidate_id,
                )
            ) is not None
        verify_candidate_package_archive(archive_path, build)
    finally:
        engine.dispose()


def test_unfinished_job_repairs_available_package_with_same_identity(
    settings: Settings,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    engine = _file_engine(tmp_path)
    try:
        candidate_id, _input_id = _assembled_candidate(engine)
        with Session(engine) as session:
            build = prepare_candidate_package_build(session, candidate_id)
            session.commit()
        write_candidate_package_archive(settings, build)
        with Session(engine) as session:
            available = finalize_candidate_package_build(session, build)
            available_id, available_revision = available.id, available.revision
            session.commit()
        archive_path = settings.package_root / build.relative_path
        archive_path.write_bytes(b"corrupt after phase-two before outer job completion")

        lease = _claim_candidate_job(engine)
        monkeypatch.setattr(candidate_package_build, "create_database_engine", lambda _: engine)
        candidate_package_build.run_candidate_package_build(settings, lease)

        with Session(engine) as session:
            package = session.get(CandidatePackage, available_id)
            assert package is not None
            assert package.state == "AVAILABLE"
            assert package.revision == available_revision
        verify_candidate_package_archive(archive_path, build)
    finally:
        engine.dispose()


def test_candidate_package_job_rejects_nonempty_payload_before_facts(
    settings: Settings,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    engine = _file_engine(tmp_path)
    try:
        with Session(engine) as session, session.begin():
            job = Job(
                kind="CANDIDATE_PACKAGE_BUILD",
                resource_type="portfolio_candidate",
                resource_id=uuid4(),
                payload={"candidate_id": "forbidden"},
            )
            session.add(job)
        lease = _claim_candidate_job(engine)
        monkeypatch.setattr(candidate_package_build, "create_database_engine", lambda _: engine)
        with pytest.raises(QfError, match="CANDIDATE_PACKAGE_BUILD_PAYLOAD_FORBIDDEN"):
            candidate_package_build.run_candidate_package_build(settings, lease)
        with Session(engine) as session:
            assert session.scalar(select(CandidatePackage)) is None
    finally:
        engine.dispose()


def test_candidate_package_job_rejects_wrong_resource_before_facts(
    settings: Settings,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    engine = _file_engine(tmp_path)
    try:
        candidate_id, _input_id = _assembled_candidate(engine)
        with Session(engine) as session:
            job = Job(
                kind="CANDIDATE_PACKAGE_BUILD",
                resource_type="candidate_package",
                resource_id=candidate_id,
                state="LEASED",
                payload={},
                attempt=1,
                lease_owner="wrong-resource-worker",
                lease_expires_at=datetime.now(UTC) + timedelta(seconds=60),
            )
            session.add(job)
            session.commit()
            lease = JobLease(job.id, "wrong-resource-worker", 1)
        monkeypatch.setattr(candidate_package_build, "create_database_engine", lambda _: engine)
        with pytest.raises(QfError, match="CANDIDATE_PACKAGE_BUILD_RESOURCE_INVALID"):
            candidate_package_build.run_candidate_package_build(settings, lease)
        with Session(engine) as session:
            assert session.scalar(
                select(CandidatePackage).where(CandidatePackage.candidate_id == candidate_id)
            ) is None
    finally:
        engine.dispose()


def test_candidate_package_reclaimed_between_archive_and_finalize_stays_building(
    settings: Settings,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    engine = _file_engine(tmp_path)
    try:
        candidate_id, _input_id = _assembled_candidate(engine)
        lease = _claim_candidate_job(engine)
        write_archive = candidate_package_build.write_candidate_package_archive

        def reclaim_after_archive(current_settings: Settings, build: CandidatePackageBuild) -> None:
            write_archive(current_settings, build)
            with Session(engine) as session, session.begin():
                job = session.get(Job, lease.job_id)
                assert job is not None
                job.lease_expires_at = datetime.now(UTC) - timedelta(seconds=1)
                assert release_expired_leases(session) == 1
                replacement = claim_next_job(
                    session,
                    owner="replacement-candidate-package-worker",
                    lease_seconds=60,
                    kind="CANDIDATE_PACKAGE_BUILD",
                )
                assert replacement is not None
                assert replacement.id == lease.job_id
                assert replacement.attempt == lease.attempt + 1

        monkeypatch.setattr(candidate_package_build, "create_database_engine", lambda _: engine)
        monkeypatch.setattr(
            candidate_package_build,
            "write_candidate_package_archive",
            reclaim_after_archive,
        )
        with pytest.raises(QfError, match="JOB_LEASE_LOST"):
            candidate_package_build.run_candidate_package_build(settings, lease)
        with Session(engine) as session:
            package = session.scalar(
                select(CandidatePackage).where(CandidatePackage.candidate_id == candidate_id)
            )
            assert package is not None
            assert package.state == "BUILDING"
            job = session.get(Job, lease.job_id)
            assert job is not None
            assert job.state == "LEASED"
            assert job.attempt == lease.attempt + 1
            assert job.lease_owner == "replacement-candidate-package-worker"
    finally:
        engine.dispose()


def test_candidate_package_verifier_rejects_extra_archive_file(
    settings: Settings,
    tmp_path: Path,
) -> None:
    engine = _file_engine(tmp_path)
    try:
        candidate_id, _input_id = _assembled_candidate(engine)
        with Session(engine) as session:
            build = prepare_candidate_package_build(session, candidate_id)
            session.commit()
        write_candidate_package_archive(settings, build)
        archive_path = settings.package_root / build.relative_path
        with ZipFile(archive_path, "a") as archive:
            archive.writestr("unexpected.txt", "not part of the target-only contract")
        with pytest.raises(QfError, match="CANDIDATE_PACKAGE_CONFORMANCE_FAILED"):
            verify_candidate_package_archive(archive_path, build)
    finally:
        engine.dispose()


@pytest.mark.parametrize("path_kind", ("final", "staging"))
def test_candidate_package_refuses_symlink_recovery_paths(
    settings: Settings,
    tmp_path: Path,
    path_kind: str,
) -> None:
    engine = _file_engine(tmp_path)
    try:
        candidate_id, _input_id = _assembled_candidate(engine)
        with Session(engine) as session:
            build = prepare_candidate_package_build(session, candidate_id)
            session.commit()
        sentinel_root = tmp_path / "must-not-delete"
        sentinel_root.mkdir()
        sentinel = sentinel_root / "sentinel"
        sentinel.write_text("keep")
        package_root = settings.package_root
        poisoned = (
            package_root / str(build.package_id)
            if path_kind == "final"
            else package_root / "staging" / str(build.package_id)
        )
        poisoned.parent.mkdir(parents=True, exist_ok=True)
        poisoned.symlink_to(sentinel_root, target_is_directory=True)

        with pytest.raises(QfError, match="CANDIDATE_PACKAGE_PATH_INVALID"):
            write_candidate_package_archive(settings, build)
        assert sentinel.read_text() == "keep"
    finally:
        engine.dispose()


def test_candidate_package_filesystem_lock_rejects_symlink_path(
    settings: Settings,
    tmp_path: Path,
) -> None:
    sentinel_root = tmp_path / "must-not-lock-outside"
    sentinel_root.mkdir()
    sentinel = sentinel_root / "sentinel"
    sentinel.write_text("keep")
    lock_root = settings.package_root / ".candidate-package-build-locks"
    lock_root.symlink_to(sentinel_root, target_is_directory=True)

    with pytest.raises(QfError, match="CANDIDATE_PACKAGE_PATH_INVALID"):
        with candidate_package_filesystem_lock(settings, uuid4()):
            pytest.fail("symlinked lock path must never be acquired")
    assert sentinel.read_text() == "keep"


def test_typed_available_package_downloads_through_handoff_route(
    settings: Settings,
    tmp_path: Path,
) -> None:
    engine = _file_engine(tmp_path)
    try:
        candidate_id, _input_id = _assembled_candidate(engine)
        with Session(engine) as session:
            build = prepare_candidate_package_build(session, candidate_id)
            session.commit()
        write_candidate_package_archive(settings, build)
        with Session(engine) as session:
            package = finalize_candidate_package_build(session, build)
            package_id, package_revision = package.id, package.revision
            session.commit()
        downstream_id = uuid4()
        issued = issue_service_token(settings, downstream_id)
        with Session(engine) as session:
            downstream = DownstreamSystem(
                id=downstream_id,
                name=f"Typed package consumer {downstream_id.hex[:8]}",
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
            session.add(downstream)
            session.flush()
            approval = ApprovalSnapshot(
                candidate_id=candidate_id,
                candidate_package_id=package_id,
                candidate_package_revision=package_revision,
                purpose="PAPER",
                state="APPROVED",
                downstream_system_id=downstream.id,
                valid_until=datetime.now(UTC) + timedelta(days=1),
                human_report={},
                evidence_summary={},
                capital_context={},
                risk_summary={},
                cost_summary={},
                capacity_summary={},
                changes_summary={},
            )
            session.add(approval)
            session.flush()
            handoff = HandoffOffer(
                approval_id=approval.id,
                candidate_package_id=package_id,
                candidate_id=candidate_id,
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
            session.commit()

        client = TestClient(create_app(settings=settings, engine=engine))
        response = client.get(
            f"/api/v1/handoffs/{handoff_id}/package",
            headers={"Authorization": f"Bearer {issued.token}"},
        )
        assert response.status_code == 200, response.text
        assert response.headers["content-type"] == "application/zip"
        with ZipFile(BytesIO(response.content)) as archive:
            assert set(archive.namelist()) == {"manifest.json", "validation/target-portfolio-frame.json"}
    finally:
        engine.dispose()


def test_candidate_package_active_job_uniqueness_rejects_live_sibling(
    tmp_path: Path,
) -> None:
    engine = _file_engine(tmp_path)
    try:
        candidate_id, _input_id = _assembled_candidate(engine)
        with Session(engine) as session:
            build = prepare_candidate_package_build(session, candidate_id)
            session.commit()
        lease = _claim_candidate_job(engine)
        assert lease.attempt == 1
        with Session(engine) as session:
            session.add(
                Job(
                    kind="CANDIDATE_PACKAGE_BUILD",
                    resource_type="portfolio_candidate",
                    resource_id=candidate_id,
                    payload={},
                )
            )
            with pytest.raises(IntegrityError):
                session.flush()
            session.rollback()
        with Session(engine) as session:
            package = session.get(CandidatePackage, build.package_id)
            assert package is not None and package.state == "BUILDING"
            assert len(
                list(
                    session.scalars(
                        select(Job).where(
                            Job.kind == "CANDIDATE_PACKAGE_BUILD",
                            Job.resource_type == "portfolio_candidate",
                            Job.resource_id == candidate_id,
                            Job.state.in_(("READY", "LEASED")),
                        )
                    )
                )
            ) == 1
    finally:
        engine.dispose()
