from pathlib import Path

def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")

def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")

def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    if old not in text:
        raise SystemExit(f"pattern missing in {path}: {old[:120]!r}")
    write(path, text.replace(old, new, 1))

def replace_all(path: str, old: str, new: str) -> None:
    text = read(path)
    if old not in text:
        raise SystemExit(f"pattern missing in {path}: {old[:120]!r}")
    write(path, text.replace(old, new))

engine = "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py"
replace_once(engine, "import base64\n", "import base64\nimport fcntl\n")
replace_all(engine, "catalog.query_quote_ticks(identifiers=scope)", "catalog.quote_ticks(instrument_ids=scope)")
replace_once(
    engine,
    '''        try:
            lock_fd = os.open(lock_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        except FileExistsError as exc:
            raise GatewayContractError("catalog ingest is already in progress") from exc
        staging_path: Path | None = None
        try:
''',
    '''        lock_fd = os.open(lock_path, os.O_CREAT | os.O_RDWR, 0o600)
        try:
            try:
                fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as exc:
                raise GatewayContractError("catalog ingest is already in progress") from exc
            staging_path: Path | None = None
''',
)
replace_once(
    engine,
    '''        finally:
            os.close(lock_fd)
            lock_path.unlink(missing_ok=True)
            if staging_path is not None and staging_path.exists():
                shutil.rmtree(staging_path, ignore_errors=True)
''',
    '''        finally:
            try:
                fcntl.flock(lock_fd, fcntl.LOCK_UN)
            finally:
                os.close(lock_fd)
            if staging_path is not None and staging_path.exists():
                shutil.rmtree(staging_path, ignore_errors=True)
''',
)

real_test = "nautilus_runtime/tests/test_real_backtest.py"
replace_once(
    real_test,
    '''    assert disclosure["passed"] is False
    assert disclosure["quality_score"] == 0.0
    assert disclosure["policy_checks"]["positive_total_pnl"] is False
''',
    '''    assert disclosure["passed"] is False
    assert disclosure["quality_tier"] == "REJECTED"
    assert "TOTAL_PNL_POLICY_FAILED" in disclosure["reason_codes"]
    assert disclosure["policy_checks"]["positive_total_pnl"] is False
    assert "quality_score" not in disclosure
''',
)

jobs = "backend/src/jobs.py"
replace_once(
    jobs,
    "def complete_job(session: Session, job: Job) -> None:\n",
    '''def renew_job_lease(
    session: Session,
    *,
    job_id: UUID,
    owner: str,
    lease_seconds: int,
    now: datetime | None = None,
) -> bool:
    current = now or datetime.now(UTC)
    result = cast(
        CursorResult[Any],
        session.execute(
            update(Job)
            .where(
                Job.id == job_id,
                Job.state == "LEASED",
                Job.lease_owner == owner,
            )
            .values(
                lease_expires_at=current + timedelta(seconds=lease_seconds),
                updated_at=current,
            )
        ),
    )
    return int(result.rowcount or 0) == 1


def complete_job(session: Session, job: Job) -> None:
''',
)

sealed_worker = "backend/src/runners/sealed_worker.py"
replace_once(
    sealed_worker,
    '''import socket
import time
from collections.abc import Sequence
''',
    '''import socket
import threading
import time
from collections.abc import Sequence
from contextlib import contextmanager
''',
)
replace_once(
    sealed_worker,
    "from jobs import claim_next_job, complete_job, fail_job, release_expired_leases\n",
    '''from jobs import (
    claim_next_job,
    complete_job,
    fail_job,
    release_expired_leases,
    renew_job_lease,
)
''',
)
replace_once(
    sealed_worker,
    "def run_once(\n",
    '''@contextmanager
def _lease_heartbeat(
    settings: Settings,
    *,
    owner: str,
    job_id: UUID,
    factory: SessionFactory,
):
    stop = threading.Event()
    lost = threading.Event()
    interval = max(1.0, min(float(settings.job_lease_seconds) / 3.0, 15.0))

    def heartbeat() -> None:
        while not stop.wait(interval):
            try:
                with factory.begin() as session:
                    renewed = renew_job_lease(
                        session,
                        job_id=job_id,
                        owner=owner,
                        lease_seconds=settings.job_lease_seconds,
                    )
                if not renewed:
                    lost.set()
                    return
            except Exception:
                lost.set()
                LOGGER.exception("sealed job lease heartbeat failed", extra={"job_id": str(job_id)})
                return

    thread = threading.Thread(target=heartbeat, name=f"sealed-lease-{job_id}", daemon=True)
    thread.start()
    try:
        yield lost
    finally:
        stop.set()
        thread.join(timeout=max(2.0, interval + 1.0))


def run_once(
''',
)
replace_once(
    sealed_worker,
    '''    try:
        result = _execute_qualification(factory, job)
    except Exception as exc:  # noqa: BLE001 - durable privileged job boundary
''',
    '''    try:
        with _lease_heartbeat(
            settings,
            owner=owner,
            job_id=job.id,
            factory=factory,
        ) as lease_lost:
            result = _execute_qualification(factory, job)
        if lease_lost.is_set():
            LOGGER.error("sealed job lease ownership was lost", extra={"job_id": str(job.id)})
            return True, settings.job_poll_seconds
    except Exception as exc:  # noqa: BLE001 - durable privileged job boundary
''',
)
replace_once(
    sealed_worker,
    '''            if current is not None:
                fail_job(session, current, str(exc)[-4000:])
''',
    '''            if (
                current is not None
                and current.state == "LEASED"
                and current.lease_owner == owner
            ):
                fail_job(session, current, str(exc)[-4000:])
''',
)
replace_once(
    sealed_worker,
    '''        if current is not None:
            current.payload = {**dict(current.payload or {}), "result": result}
''',
    '''        if (
            current is not None
            and current.state == "LEASED"
            and current.lease_owner == owner
        ):
            current.payload = {**dict(current.payload or {}), "result": result}
''',
)

ledger = "backend/src/quant_runtime/ledger.py"
replace_once(ledger, "from datetime import UTC, datetime\n", "from datetime import UTC, datetime, timedelta\n")
replace_once(ledger, "from db.models import DatasetRevision, ResearchMission, SearchLedgerEntry\n", "from db.models import DatasetRevision, ResearchMission, ResearchProgram, SearchLedgerEntry\n")
replace_once(
    ledger,
    "class ExperimentCoordinator:\n",
    '''def _evidence_program_lineage(session: Session, program_id: UUID) -> set[UUID]:
    lineage: set[UUID] = set()
    current_id: UUID | None = program_id
    while current_id is not None:
        if current_id in lineage:
            raise QfError(
                "RESEARCH_PROGRAM_EVIDENCE_LINEAGE_CYCLE",
                "Research Program evidence inheritance contains a cycle.",
                500,
            )
        lineage.add(current_id)
        program = session.get(ResearchProgram, current_id)
        if program is None:
            raise QfError(
                "RESEARCH_PROGRAM_NOT_FOUND",
                "Research Program in evidence lineage does not exist.",
                404,
            )
        current_id = program.evidence_inherited_from_program_id
    return lineage


class ExperimentCoordinator:
''',
)
replace_once(
    ledger,
    '''                if existing.state == "RUNNING":
                    raise QfError(
                        "EXPERIMENT_ALREADY_RUNNING",
                        "The exact experiment is already running.",
                        409,
                    )
                session.expunge(existing)
                return existing
''',
    '''                if existing.state == "RUNNING":
                    stale_before = _now() - timedelta(minutes=35)
                    if existing.started_at is None or existing.started_at >= stale_before:
                        raise QfError(
                            "EXPERIMENT_ALREADY_RUNNING",
                            "The exact experiment is already running.",
                            409,
                        )
                    existing.state = "FAILED"
                    existing.finished_at = _now()
                    existing.failure_code = "EXPERIMENT_ATTEMPT_ABANDONED"
                    existing.failure_message = (
                        "The prior RUNNING attempt exceeded the remote-runtime recovery window."
                    )
                    session.flush()
                session.expunge(existing)
                return existing
''',
)
replace_once(
    ledger,
    '''                if parent is None or parent.program_id != program_id:
                    raise QfError(
                        "EXPERIMENT_PARENT_INVALID",
                        "Experiment parent does not exist in the requested Program.",
                        422,
                    )
                if sealed:
                    exposure = session.scalar(
                        select(SearchLedgerEntry).where(
                            SearchLedgerEntry.parent_entry_id == parent_entry_id,
                            SearchLedgerEntry.mode == ExperimentMode.SEALED.value,
                            SearchLedgerEntry.state.in_(["RUNNING", "SUCCEEDED"]),
                        )
                    )
''',
    '''                program_lineage = _evidence_program_lineage(session, program_id)
                if parent is None or parent.program_id not in program_lineage:
                    raise QfError(
                        "EXPERIMENT_PARENT_INVALID",
                        "Experiment parent is outside the requested Program evidence lineage.",
                        422,
                    )
                if sealed:
                    exposure = session.scalar(
                        select(SearchLedgerEntry).where(
                            SearchLedgerEntry.program_id.in_(program_lineage),
                            SearchLedgerEntry.dataset_revision_id == request.dataset_revision_id,
                            SearchLedgerEntry.mode == ExperimentMode.SEALED.value,
                            SearchLedgerEntry.state.in_(["RUNNING", "SUCCEEDED"]),
                        )
                    )
''',
)

domain_models = "backend/src/db/domain_models.py"
replace_once(domain_models, "        DateTime(timezone=True), default=datetime.utcnow, nullable=False\n", "        DateTime(timezone=True), default=lambda: datetime.now(UTC), nullable=False\n")

promotion = "backend/src/quant_runtime/promotion.py"
replace_once(promotion, '    "min_capacity_ratio",\n', "")

guards = "backend/tests/integration/test_issue22_final_guards.py"
replace_once(guards, "from quant_runtime.promotion import _validate_mandate_after_simulation\n", "from quant_runtime.promotion import _mandate_constraints, _validate_mandate_after_simulation\n")
replace_once(
    guards,
    "def test_negative_capacity_cannot_satisfy_positive_mandate_floor() -> None:\n",
    '''def test_capacity_constraint_is_rejected_before_remote_simulation() -> None:
    mandate = SimpleNamespace(spec_json={"constraints": {"min_capacity_ratio": 0.5}})
    with pytest.raises(QfError) as raised:
        _mandate_constraints(mandate)
    assert raised.value.code == "PORTFOLIO_MANDATE_CONSTRAINT_UNSUPPORTED"


def test_negative_capacity_cannot_satisfy_positive_mandate_floor() -> None:
''',
)

jobs_test = "backend/tests/unit/test_jobs.py"
replace_once(jobs_test, "from jobs import claim_next_job, enqueue_job, release_expired_leases\n", "from jobs import claim_next_job, enqueue_job, release_expired_leases, renew_job_lease\n")
replace_once(
    jobs_test,
    "\n\ndef test_claim_next_job_respects_explicit_worker_capabilities(\n",
    '''


def test_job_lease_renewal_requires_current_owner(engine) -> None:
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        created = enqueue_job(
            session,
            kind="SEALED_ALPHA_QUALIFICATION",
            resource_type="SEARCH_LEDGER_ENTRY",
            resource_id=uuid4(),
        )
        job_id = created.id
    now = datetime.now(UTC)
    with factory.begin() as session:
        claimed = claim_next_job(session, owner="sealed-a", lease_seconds=10, now=now)
        assert claimed is not None
    with factory.begin() as session:
        assert renew_job_lease(
            session,
            job_id=job_id,
            owner="sealed-a",
            lease_seconds=60,
            now=now + timedelta(seconds=5),
        ) is True
        assert renew_job_lease(
            session,
            job_id=job_id,
            owner="sealed-b",
            lease_seconds=60,
            now=now + timedelta(seconds=5),
        ) is False
    with factory() as session:
        job = session.get(Job, job_id)
        assert job is not None
        assert job.lease_owner == "sealed-a"
        assert job.lease_expires_at is not None
        assert job.lease_expires_at >= now + timedelta(seconds=65)


def test_claim_next_job_respects_explicit_worker_capabilities(
''',
)

print("stage1 issue22 closure applied")
