"""Build and verify target-only Packages from immutable relational Candidates."""

from __future__ import annotations

import json
import math
import os
import shutil
import zipfile
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any, Iterator, NoReturn
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from db.models import (
    CandidatePackage,
    Job,
    PortfolioAssemblyInput,
    PortfolioAssemblyInputMember,
    PortfolioCandidate,
    PortfolioCandidateMember,
)
from errors import QfError
from jobs import enqueue_job
from settings import Settings

try:  # Linux workers use the standard-library flock; unavailable platforms fail closed.
    import fcntl
except ImportError:  # pragma: no cover - production workers run in Linux containers
    fcntl = None  # type: ignore[assignment]


_CANDIDATE_BUNDLE_CONTRACT_VERSION = "1"
_TARGET_FRAME_PATH = "validation/target-portfolio-frame.json"
_ARCHIVE_NAMES = frozenset({"manifest.json", _TARGET_FRAME_PATH})
_TARGET_FRAME_FIELDS = frozenset(
    {
        "schema_version",
        "portfolio_candidate_id",
        "portfolio_state",
        "universe_version_id",
        "as_of_time",
        "effective_from",
        "effective_until",
        "rows",
    }
)
_TARGET_FRAME_ROW_FIELDS = frozenset({"instrument_id", "target_weight", "confidence"})
_FORBIDDEN_SECRET_FIELDS = {
    "api_key",
    "apikey",
    "private_key",
    "secret_key",
    "service_token",
    "access_token",
    "refresh_token",
    "account_password",
    "wallet_seed",
    "broker_url",
}
_FORBIDDEN_EXECUTION_FIELDS = {
    "account",
    "account_id",
    "broker",
    "execution",
    "execution_retry",
    "fill",
    "fills",
    "heartbeat",
    "limit",
    "limit_price",
    "order",
    "order_id",
    "order_type",
    "orders",
    "position",
    "positions",
    "recovery",
    "side",
    "stop",
    "stop_price",
    "tif",
    "time_in_force",
    "venue",
}


@dataclass(frozen=True, slots=True)
class CandidatePackageBuild:
    """The only filesystem inputs, reconstructed from locked Core relations."""

    candidate_id: UUID
    package_id: UUID
    revision: int
    relative_path: str
    manifest: dict[str, Any]
    target_frame: dict[str, Any]


def _conflict(code: str, message: str) -> QfError:
    return QfError(code, message, 409)


def _locked(statement: Any, *, lock: bool) -> Any:
    return statement.with_for_update() if lock else statement


def _json_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True).encode("utf-8")


def _write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(_json_bytes(value))


def _stored_utc(value: object) -> datetime | None:
    if not isinstance(value, datetime):
        return None
    return value.replace(tzinfo=UTC) if value.tzinfo is None else value.astimezone(UTC)


def _finite_decimal(value: object, field: str) -> Decimal:
    if not isinstance(value, Decimal) or not value.is_finite():
        raise _conflict("CANDIDATE_PACKAGE_SOURCE_INVALID", f"Candidate {field} is not finite.")
    return value


def _reject_forbidden_fields(value: object, path: str = "$") -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = str(key).casefold()
            if normalized in _FORBIDDEN_SECRET_FIELDS or normalized.endswith("_secret"):
                raise QfError(
                    "CANDIDATE_PACKAGE_CONTAINS_SECRET",
                    "Target Portfolio Frame contains a forbidden credential field.",
                    422,
                    {"path": f"{path}.{key}"},
                )
            if normalized in _FORBIDDEN_EXECUTION_FIELDS:
                raise QfError(
                    "CANDIDATE_PACKAGE_CONTAINS_EXECUTION_FIELD",
                    "Target Portfolio Frame must not contain execution fields.",
                    422,
                    {"path": f"{path}.{key}"},
                )
            _reject_forbidden_fields(item, f"{path}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _reject_forbidden_fields(item, f"{path}[{index}]")


def _target_frame(
    candidate: PortfolioCandidate,
    input_row: PortfolioAssemblyInput,
    input_members: list[PortfolioAssemblyInputMember],
    candidate_members: list[PortfolioCandidateMember],
) -> dict[str, Any]:
    if (
        candidate.state != "ASSEMBLED"
        or candidate.assembly_input_id is None
        or candidate.universe_version_id is None
        or input_row.state != "ASSEMBLED"
        or input_row.outcome_code != "OPTIMAL"
    ):
        raise _conflict(
            "CANDIDATE_PACKAGE_TRUSTED_SOURCE_REQUIRED",
            "Candidate Package requires an assembled relational Candidate and Input.",
        )
    if not input_members or len(input_members) != len(candidate_members):
        raise _conflict(
            "CANDIDATE_PACKAGE_MEMBERS_INVALID",
            "Candidate Package requires the exact relational Candidate member set.",
        )
    if tuple(member.axis_index for member in input_members) != tuple(range(len(input_members))):
        raise _conflict(
            "CANDIDATE_PACKAGE_MEMBERS_INVALID",
            "Candidate Package Input member axes are incomplete.",
        )
    weights = {member.alpha_qualification_id: member for member in candidate_members}
    expected_ids = tuple(member.alpha_qualification_id for member in input_members)
    if (
        len(weights) != len(candidate_members)
        or set(weights) != set(expected_ids)
        or any(member.role != "PRIMARY_ALPHA" for member in candidate_members)
    ):
        raise _conflict(
            "CANDIDATE_PACKAGE_MEMBERS_INVALID",
            "Candidate Package members do not match the assembled Input.",
        )
    as_of_time = _stored_utc(input_row.as_of_time)
    effective_from = _stored_utc(input_row.effective_from)
    effective_until = _stored_utc(input_row.effective_until)
    if (
        as_of_time is None
        or effective_from is None
        or effective_from < as_of_time
        or (effective_until is not None and effective_until < effective_from)
    ):
        raise _conflict(
            "CANDIDATE_PACKAGE_SOURCE_INVALID",
            "Candidate Package Input timestamps are invalid.",
        )
    rows: list[dict[str, Any]] = []
    total_weight = Decimal()
    instruments: set[str] = set()
    for member in input_members:
        candidate_member = weights[member.alpha_qualification_id]
        weight = _finite_decimal(candidate_member.target_weight, "target weight")
        confidence = _finite_decimal(member.confidence, "confidence")
        instrument_id = member.instrument_id.strip() if isinstance(member.instrument_id, str) else ""
        if (
            not instrument_id
            or instrument_id in instruments
            or weight < 0
            or weight > 1
            or confidence < 0
            or confidence > 1
        ):
            raise _conflict(
                "CANDIDATE_PACKAGE_MEMBERS_INVALID",
                "Candidate Package targets must be unique finite long-only rows.",
            )
        instruments.add(instrument_id)
        total_weight += weight
        rows.append(
            {
                "instrument_id": instrument_id,
                "target_weight": float(weight),
                "confidence": float(confidence),
            }
        )
    if not math.isclose(float(total_weight), 1.0, rel_tol=0.0, abs_tol=1e-6):
        raise _conflict(
            "CANDIDATE_PACKAGE_MEMBERS_INVALID",
            "Candidate Package target weights must sum to one.",
        )
    frame = {
        "schema_version": "1",
        "portfolio_candidate_id": str(candidate.id),
        "portfolio_state": "ASSEMBLED",
        "universe_version_id": str(candidate.universe_version_id),
        "as_of_time": as_of_time.isoformat(),
        "effective_from": effective_from.isoformat(),
        "effective_until": effective_until.isoformat() if effective_until is not None else None,
        "rows": rows,
    }
    _reject_forbidden_fields(frame)
    return frame


def _trusted_candidate_frame(
    session: Session,
    candidate_id: UUID,
    *,
    lock: bool,
) -> tuple[PortfolioCandidate, dict[str, Any]]:
    candidate = session.scalar(
        _locked(select(PortfolioCandidate).where(PortfolioCandidate.id == candidate_id), lock=lock)
    )
    if candidate is None:
        raise QfError("CANDIDATE_NOT_FOUND", "Portfolio Candidate was not found.", 404)
    if (
        candidate.state != "ASSEMBLED"
        or candidate.assembly_input_id is None
        or candidate.candidate_family_id is None
        or candidate.mandate_version_id is None
        or candidate.capital_context_version_id is None
        or candidate.universe_version_id is None
    ):
        raise _conflict(
            "CANDIDATE_PACKAGE_TRUSTED_SOURCE_REQUIRED",
            "Candidate Package requires an assembled relational Candidate.",
        )
    input_row = session.scalar(
        _locked(
            select(PortfolioAssemblyInput).where(
                PortfolioAssemblyInput.id == candidate.assembly_input_id,
                PortfolioAssemblyInput.portfolio_program_id == candidate.portfolio_program_id,
                PortfolioAssemblyInput.mandate_version_id == candidate.mandate_version_id,
                PortfolioAssemblyInput.capital_context_version_id
                == candidate.capital_context_version_id,
                PortfolioAssemblyInput.universe_version_id == candidate.universe_version_id,
            ),
            lock=lock,
        )
    )
    if input_row is None:
        raise _conflict(
            "CANDIDATE_PACKAGE_LINEAGE_INVALID",
            "Candidate Package Input lineage is not exact.",
        )
    input_members = list(
        session.scalars(
            _locked(
                select(PortfolioAssemblyInputMember)
                .where(PortfolioAssemblyInputMember.input_id == input_row.id)
                .order_by(PortfolioAssemblyInputMember.axis_index),
                lock=lock,
            )
        )
    )
    candidate_members = list(
        session.scalars(
            _locked(
                select(PortfolioCandidateMember)
                .where(PortfolioCandidateMember.candidate_id == candidate.id)
                .order_by(PortfolioCandidateMember.alpha_qualification_id),
                lock=lock,
            )
        )
    )
    return candidate, _target_frame(candidate, input_row, input_members, candidate_members)


def _relative_path(package_id: UUID) -> str:
    return (Path(str(package_id)) / "candidate-package.zip").as_posix()


def _build(
    candidate: PortfolioCandidate,
    target_frame: dict[str, Any],
    *,
    package_id: UUID,
    revision: int,
) -> CandidatePackageBuild:
    if revision != 1:
        raise _conflict(
            "CANDIDATE_PACKAGE_REVISION_INVALID",
            "Candidate Package V1 only supports revision one.",
        )
    manifest = {
        "candidate_bundle_contract_version": _CANDIDATE_BUNDLE_CONTRACT_VERSION,
        "package_kind": "TARGET_PORTFOLIO_FRAME",
        "candidate_id": str(candidate.id),
        "candidate_package_id": str(package_id),
        "candidate_package_revision": revision,
        "target_portfolio_frame": _TARGET_FRAME_PATH,
    }
    _reject_forbidden_fields(manifest)
    return CandidatePackageBuild(
        candidate_id=candidate.id,
        package_id=package_id,
        revision=revision,
        relative_path=_relative_path(package_id),
        manifest=manifest,
        target_frame=target_frame,
    )


def _reserved_package(
    session: Session,
    candidate: PortfolioCandidate,
    target_frame: dict[str, Any],
) -> tuple[CandidatePackage, CandidatePackageBuild]:
    packages = list(
        session.scalars(
            select(CandidatePackage)
            .where(CandidatePackage.candidate_id == candidate.id)
            .with_for_update()
        )
    )
    if len(packages) > 1:
        raise _conflict(
            "CANDIDATE_PACKAGE_V1_CONFLICT",
            "Candidate Package V1 permits exactly one Package for an assembled Candidate.",
        )
    if not packages:
        package = CandidatePackage(
            id=uuid4(),
            candidate_id=candidate.id,
            revision=1,
            contract_version=_CANDIDATE_BUNDLE_CONTRACT_VERSION,
            state="BUILDING",
            manifest_json={},
            relative_path="",
            payload={},
            created_at=datetime.now(UTC),
        )
        build = _build(candidate, target_frame, package_id=package.id, revision=package.revision)
        package.manifest_json = build.manifest
        package.relative_path = build.relative_path
        session.add(package)
        session.flush()
        return package, build

    package = packages[0]
    build = _build(candidate, target_frame, package_id=package.id, revision=package.revision)
    if (
        package.contract_version != _CANDIDATE_BUNDLE_CONTRACT_VERSION
        or package.state not in {"BUILDING", "AVAILABLE"}
        or package.manifest_json != build.manifest
        or package.relative_path != build.relative_path
        or package.payload != {}
    ):
        raise _conflict(
            "CANDIDATE_PACKAGE_V1_CONFLICT",
            "Candidate has an incompatible immutable Package fact.",
        )
    return package, build


def _active_build_jobs(session: Session, candidate_id: UUID, *, lock: bool) -> list[Job]:
    statement = select(Job).where(
        Job.kind == "CANDIDATE_PACKAGE_BUILD",
        Job.resource_type == "portfolio_candidate",
        Job.resource_id == candidate_id,
        Job.state.in_(("READY", "LEASED")),
    )
    return list(session.scalars(_locked(statement, lock=lock)))


def _ensure_single_build_job(session: Session, candidate_id: UUID) -> None:
    # The Candidate row is already locked by the caller.  Do not take a Job
    # row lock here: workers lock Job then Candidate, and the partial unique
    # index is sufficient to reject any non-cooperating duplicate enqueue.
    jobs = _active_build_jobs(session, candidate_id, lock=False)
    if len(jobs) > 1 or (jobs and jobs[0].payload != {}):
        raise _conflict(
            "CANDIDATE_PACKAGE_JOB_CONFLICT",
            "Candidate Package has an invalid active build job.",
        )
    if not jobs:
        enqueue_job(
            session,
            kind="CANDIDATE_PACKAGE_BUILD",
            resource_type="portfolio_candidate",
            resource_id=candidate_id,
            payload={},
        )


def _require_single_build_job(session: Session, candidate_id: UUID) -> None:
    jobs = _active_build_jobs(session, candidate_id, lock=True)
    if len(jobs) != 1 or jobs[0].payload != {}:
        raise _conflict(
            "CANDIDATE_PACKAGE_JOB_CONFLICT",
            "Candidate Package build has no exclusive empty-payload job.",
        )


def _reserve_candidate_package_build(session: Session, candidate_id: UUID) -> CandidatePackage:
    """Worker-only phase-one reservation of the deterministic V1 Package."""
    if not isinstance(candidate_id, UUID):
        raise QfError("CANDIDATE_PACKAGE_SOURCE_INVALID", "Candidate ID is invalid.", 422)
    candidate, target_frame = _trusted_candidate_frame(session, candidate_id, lock=True)
    package, _build_context = _reserved_package(session, candidate, target_frame)
    return package


def enqueue_candidate_package_build(session: Session, candidate_id: UUID) -> None:
    """Atomically queue the Candidate resource; the fenced child owns Package reservation."""
    if not isinstance(candidate_id, UUID):
        raise QfError("CANDIDATE_PACKAGE_SOURCE_INVALID", "Candidate ID is invalid.", 422)
    candidate, target_frame = _trusted_candidate_frame(session, candidate_id, lock=True)
    packages = list(
        session.scalars(
            select(CandidatePackage)
            .where(CandidatePackage.candidate_id == candidate.id)
            .with_for_update()
        )
    )
    if len(packages) > 1:
        raise _conflict(
            "CANDIDATE_PACKAGE_V1_CONFLICT",
            "Candidate Package V1 permits exactly one Package for an assembled Candidate.",
        )
    if packages:
        package = packages[0]
        expected = _build(candidate, target_frame, package_id=package.id, revision=package.revision)
        if (
            package.contract_version != _CANDIDATE_BUNDLE_CONTRACT_VERSION
            or package.state not in {"BUILDING", "AVAILABLE"}
            or package.manifest_json != expected.manifest
            or package.relative_path != expected.relative_path
            or package.payload != {}
        ):
            raise _conflict(
                "CANDIDATE_PACKAGE_V1_CONFLICT",
                "Candidate has an incompatible immutable Package fact.",
            )
        if package.state == "AVAILABLE":
            return
    _ensure_single_build_job(session, candidate.id)


def prepare_candidate_package_build(session: Session, candidate_id: UUID) -> CandidatePackageBuild:
    """Lock/revalidate the Candidate and reserve its Package before filesystem work."""
    _require_single_build_job(session, candidate_id)
    package = _reserve_candidate_package_build(session, candidate_id)
    candidate, target_frame = _trusted_candidate_frame(session, candidate_id, lock=True)
    build = _build(candidate, target_frame, package_id=package.id, revision=package.revision)
    if (
        package.manifest_json != build.manifest
        or package.relative_path != build.relative_path
        or package.state not in {"BUILDING", "AVAILABLE"}
    ):
        raise _conflict(
            "CANDIDATE_PACKAGE_V1_CONFLICT",
            "Candidate Package reservation no longer matches its relational source.",
        )
    return build


def _validate_target_frame(value: object) -> None:
    if not isinstance(value, dict) or set(value) != _TARGET_FRAME_FIELDS:
        raise QfError(
            "CANDIDATE_PACKAGE_CONFORMANCE_FAILED",
            "Candidate Package target frame does not use the supported schema.",
            422,
        )
    if value.get("schema_version") != "1" or value.get("portfolio_state") != "ASSEMBLED":
        raise QfError(
            "CANDIDATE_PACKAGE_CONFORMANCE_FAILED",
            "Candidate Package target frame is not an assembled V1 frame.",
            422,
        )
    rows = value.get("rows")
    if not isinstance(rows, list) or not rows:
        raise QfError(
            "CANDIDATE_PACKAGE_CONFORMANCE_FAILED",
            "Candidate Package target frame has no target rows.",
            422,
        )
    instruments: set[str] = set()
    total_weight = 0.0
    for row in rows:
        if not isinstance(row, dict) or set(row) != _TARGET_FRAME_ROW_FIELDS:
            raise QfError(
                "CANDIDATE_PACKAGE_CONFORMANCE_FAILED",
                "Candidate Package target frame rows are invalid.",
                422,
            )
        instrument = row.get("instrument_id")
        weight = row.get("target_weight")
        confidence = row.get("confidence")
        if (
            not isinstance(instrument, str)
            or not instrument.strip()
            or instrument in instruments
            or isinstance(weight, bool)
            or isinstance(confidence, bool)
            or not isinstance(weight, int | float)
            or not isinstance(confidence, int | float)
            or not math.isfinite(float(weight))
            or not math.isfinite(float(confidence))
            or not 0 <= float(weight) <= 1
            or not 0 <= float(confidence) <= 1
        ):
            raise QfError(
                "CANDIDATE_PACKAGE_CONFORMANCE_FAILED",
                "Candidate Package target rows must be unique finite long-only values.",
                422,
            )
        instruments.add(instrument)
        total_weight += float(weight)
    if not math.isclose(total_weight, 1.0, rel_tol=0.0, abs_tol=1e-6):
        raise QfError(
            "CANDIDATE_PACKAGE_CONFORMANCE_FAILED",
            "Candidate Package target weights must sum to one.",
            422,
        )
    _reject_forbidden_fields(value)


def verify_candidate_package_archive(path: Path, build: CandidatePackageBuild) -> None:
    """Small trusted reference-fixture conformance check; no remote runtime involved."""
    try:
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            if len(names) != len(_ARCHIVE_NAMES) or set(names) != _ARCHIVE_NAMES:
                raise QfError(
                    "CANDIDATE_PACKAGE_CONFORMANCE_FAILED",
                    "Candidate Package archive contains unsupported files.",
                    422,
                )
            manifest = json.loads(archive.read("manifest.json"))
            target_frame = json.loads(archive.read(_TARGET_FRAME_PATH))
    except (OSError, zipfile.BadZipFile, json.JSONDecodeError, KeyError) as error:
        raise QfError(
            "CANDIDATE_PACKAGE_CONFORMANCE_FAILED",
            "Candidate Package archive cannot be read as the trusted target-only fixture.",
            422,
        ) from error
    _validate_target_frame(target_frame)
    if manifest != build.manifest or target_frame != build.target_frame:
        raise QfError(
            "CANDIDATE_PACKAGE_CONFORMANCE_FAILED",
            "Candidate Package archive does not match its immutable relational source.",
            422,
        )


def _package_paths(settings: Settings, package_id: UUID) -> tuple[Path, Path, Path]:
    root = settings.package_root.resolve()
    staging_root = root / "staging"
    staging = staging_root / str(package_id)
    final_root = root / str(package_id)
    if staging_root.is_symlink() or staging.is_symlink() or final_root.is_symlink():
        raise QfError("CANDIDATE_PACKAGE_PATH_INVALID", "Candidate Package path is invalid.", 500)
    resolved_staging_root = staging_root.resolve()
    resolved_staging = staging.resolve()
    resolved_final_root = final_root.resolve()
    if (
        not resolved_staging_root.is_relative_to(root)
        or not resolved_staging.is_relative_to(resolved_staging_root)
        or not resolved_final_root.is_relative_to(root)
    ):
        raise QfError("CANDIDATE_PACKAGE_PATH_INVALID", "Candidate Package path is invalid.", 500)
    return staging, final_root, final_root / "candidate-package.zip"


@contextmanager
def candidate_package_filesystem_lock(settings: Settings, package_id: UUID) -> Iterator[None]:
    """Serialize one deterministic Package path across fenced worker attempts."""
    if fcntl is None:
        raise QfError(
            "CANDIDATE_PACKAGE_LOCK_UNAVAILABLE",
            "Candidate Package build requires a local filesystem lock.",
            500,
        )
    root = settings.package_root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    flags = os.O_RDONLY | os.O_DIRECTORY
    nofollow = getattr(os, "O_NOFOLLOW", 0)
    root_fd = os.open(root, flags)
    lock_dir_fd: int | None = None
    lock_fd: int | None = None
    try:
        try:
            os.mkdir(".candidate-package-build-locks", mode=0o700, dir_fd=root_fd)
        except FileExistsError:
            pass
        try:
            lock_dir_fd = os.open(
                ".candidate-package-build-locks",
                flags | nofollow,
                dir_fd=root_fd,
            )
            lock_fd = os.open(
                f"{package_id}.lock",
                os.O_RDWR | os.O_CREAT | nofollow,
                0o600,
                dir_fd=lock_dir_fd,
            )
        except OSError as error:
            raise QfError(
                "CANDIDATE_PACKAGE_PATH_INVALID",
                "Candidate Package lock path is invalid.",
                500,
            ) from error
        fcntl.flock(lock_fd, fcntl.LOCK_EX)
        yield
    finally:
        if lock_fd is not None:
            try:
                fcntl.flock(lock_fd, fcntl.LOCK_UN)
            finally:
                os.close(lock_fd)
        if lock_dir_fd is not None:
            os.close(lock_dir_fd)
        os.close(root_fd)


def write_candidate_package_archive(settings: Settings, build: CandidatePackageBuild) -> None:
    """Atomically promote a locally verified archive, or verify a prior promotion."""
    staging, final_root, archive_path = _package_paths(settings, build.package_id)
    if final_root.exists():
        if not final_root.is_dir():
            raise QfError("CANDIDATE_PACKAGE_PATH_INVALID", "Candidate Package root is invalid.", 500)
        try:
            verify_candidate_package_archive(archive_path, build)
            return
        except QfError as error:
            if error.code != "CANDIDATE_PACKAGE_CONFORMANCE_FAILED":
                raise
            # This UUID-derived resolved directory belongs to the one active
            # candidate-resource job reserved in phase one. A crash can leave
            # a partial archive, which must rebuild under the same identity.
            if final_root.is_symlink():
                raise QfError(
                    "CANDIDATE_PACKAGE_PATH_INVALID", "Candidate Package path is invalid.", 500
                ) from error
            shutil.rmtree(final_root)
    staging.parent.mkdir(parents=True, exist_ok=True)
    if staging.exists():
        if not staging.is_dir():
            raise QfError("CANDIDATE_PACKAGE_PATH_INVALID", "Candidate Package staging root is invalid.", 500)
        if staging.is_symlink():
            raise QfError("CANDIDATE_PACKAGE_PATH_INVALID", "Candidate Package staging root is invalid.", 500)
        shutil.rmtree(staging)
    staging.mkdir()
    try:
        _write_json(staging / "manifest.json", build.manifest)
        _write_json(staging / _TARGET_FRAME_PATH, build.target_frame)
        staged_archive = staging / "candidate-package.zip"
        with zipfile.ZipFile(staged_archive, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.write(staging / "manifest.json", "manifest.json")
            archive.write(staging / _TARGET_FRAME_PATH, _TARGET_FRAME_PATH)
        verify_candidate_package_archive(staged_archive, build)
        final_root.parent.mkdir(parents=True, exist_ok=True)
        try:
            os.replace(staging, final_root)
        except OSError:
            if not final_root.is_dir():
                raise
            verify_candidate_package_archive(archive_path, build)
    finally:
        if staging.exists():
            if staging.is_symlink():
                raise QfError("CANDIDATE_PACKAGE_PATH_INVALID", "Candidate Package staging root is invalid.", 500)
            shutil.rmtree(staging)
    verify_candidate_package_archive(archive_path, build)


def finalize_candidate_package_build(session: Session, build: CandidatePackageBuild) -> CandidatePackage:
    """Flip only the same verified BUILDING row to AVAILABLE in fenced phase two."""
    candidate, target_frame = _trusted_candidate_frame(session, build.candidate_id, lock=True)
    package = session.scalar(
        select(CandidatePackage)
        .where(CandidatePackage.id == build.package_id, CandidatePackage.candidate_id == candidate.id)
        .with_for_update()
    )
    expected = _build(candidate, target_frame, package_id=build.package_id, revision=build.revision)
    if (
        package is None
        or expected != build
        or package.revision != build.revision
        or package.contract_version != _CANDIDATE_BUNDLE_CONTRACT_VERSION
        or package.manifest_json != build.manifest
        or package.relative_path != build.relative_path
        or package.payload != {}
    ):
        raise _conflict(
            "CANDIDATE_PACKAGE_V1_CONFLICT",
            "Candidate Package cannot be finalized from a changed relational source.",
        )
    if package.state == "AVAILABLE":
        from promotion_service import enqueue_p2p_promotion_job_for_candidate

        enqueue_p2p_promotion_job_for_candidate(session, candidate.id)
        return package
    if package.state != "BUILDING":
        raise _conflict(
            "CANDIDATE_PACKAGE_STATE_CONFLICT",
            "Candidate Package is not reserved for a build.",
        )
    package.state = "AVAILABLE"
    session.flush()
    from promotion_service import enqueue_p2p_promotion_job_for_candidate

    enqueue_p2p_promotion_job_for_candidate(session, candidate.id)
    return package


def is_trusted_candidate_package(session: Session, package: CandidatePackage) -> bool:
    """Structural eligibility guard for any current Approval-bound Package."""
    if (
        package.state != "AVAILABLE"
        or package.contract_version != _CANDIDATE_BUNDLE_CONTRACT_VERSION
        or package.revision != 1
        or package.payload != {}
    ):
        return False
    try:
        candidate, target_frame = _trusted_candidate_frame(session, package.candidate_id, lock=False)
        expected = _build(candidate, target_frame, package_id=package.id, revision=package.revision)
    except QfError:
        return False
    return package.manifest_json == expected.manifest and package.relative_path == expected.relative_path


def build_candidate_package(*_: Any, **__: Any) -> NoReturn:
    """Legacy raw-frame writer is intentionally retired; use the fenced worker."""
    raise _conflict(
        "CANDIDATE_PACKAGE_TRUSTED_BUILD_REQUIRED",
        "Candidate Packages are built only from a relational assembled Candidate by CANDIDATE_PACKAGE_BUILD.",
    )


def create_candidate_package(*_: Any, **__: Any) -> NoReturn:
    """Legacy direct Package creation is intentionally retired."""
    raise _conflict(
        "CANDIDATE_PACKAGE_TRUSTED_BUILD_REQUIRED",
        "Candidate Packages are reserved only by the trusted Candidate assembly path.",
    )


def resolve_package_archive(settings: Settings, relative_path: str) -> Path:
    root = settings.package_root.resolve()
    candidate = (root / relative_path).resolve()
    if not candidate.is_relative_to(root):
        raise QfError("CANDIDATE_PACKAGE_PATH_INVALID", "Candidate Package path is invalid.", 500)
    if not candidate.is_file():
        raise QfError("CANDIDATE_PACKAGE_MISSING", "Candidate Package artifact is missing.", 500)
    return candidate
