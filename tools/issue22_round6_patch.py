from __future__ import annotations

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path}, got {count}: {old[:100]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


def replace_between(path: str, start: str, end: str, replacement: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    start_index = text.find(start)
    if start_index < 0:
        raise SystemExit(f"start marker missing in {path}: {start!r}")
    end_index = text.find(end, start_index)
    if end_index < 0:
        raise SystemExit(f"end marker missing in {path}: {end!r}")
    file.write_text(text[:start_index] + replacement + text[end_index:], encoding="utf-8")


# 1) Inclusive DatasetRevision event bounds mean touching endpoints are overlap.
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''    if source_start < sealed_end and sealed_start < source_end:
''',
    '''    if source_start <= sealed_end and sealed_start <= source_end:
''',
)

# 2) Preserve exact experiment contracts across transport-uncertain durable job retries.
replace_once(
    "backend/src/jobs.py",
    '''def fail_job(session: Session, job: Job, message: str) -> None:
    job.state = "FAILED"
    job.lease_owner = None
    job.lease_expires_at = None
    job.last_error = message
    session.flush()
''',
    '''def retry_job(
    session: Session,
    job: Job,
    message: str,
    *,
    delay_seconds: float = 0.0,
    now: datetime | None = None,
) -> None:
    current = now or datetime.now(UTC)
    job.state = "READY"
    job.lease_owner = None
    job.lease_expires_at = None
    job.last_error = message
    job.available_at = current + timedelta(seconds=max(0.0, float(delay_seconds)))
    session.flush()


def fail_job(session: Session, job: Job, message: str) -> None:
    job.state = "FAILED"
    job.lease_owner = None
    job.lease_expires_at = None
    job.last_error = message
    session.flush()
''',
)

replace_once(
    "backend/src/runners/finite_worker.py",
    '''from jobs import claim_next_job, complete_job, fail_job, release_expired_leases
''',
    '''from jobs import claim_next_job, complete_job, fail_job, release_expired_leases, retry_job
''',
)
replace_once(
    "backend/src/runners/finite_worker.py",
    '''LOGGER = logging.getLogger("quazonai.finite_worker")
Handler = Callable[[Settings, Job], None]
''',
    '''LOGGER = logging.getLogger("quazonai.finite_worker")
Handler = Callable[[Settings, Job], None]
RETRYABLE_CHILD_EXIT_CODE = 75


class RetryableJobError(RuntimeError):
    """A child preserved its immutable work and asks the durable queue to retry it."""
''',
)
replace_once(
    "backend/src/runners/finite_worker.py",
    '''        except subprocess.CalledProcessError as exc:
            message = (exc.stderr or "").strip()[-2000:]
            raise RuntimeError(
                f"{job.kind} child failed with exit code {exc.returncode}: {message}"
            ) from exc
''',
    '''        except subprocess.CalledProcessError as exc:
            message = (exc.stderr or "").strip()[-2000:]
            if exc.returncode == RETRYABLE_CHILD_EXIT_CODE:
                raise RetryableJobError(
                    f"{job.kind} child requested retry: {message or 'remote result reconciliation pending'}"
                ) from exc
            raise RuntimeError(
                f"{job.kind} child failed with exit code {exc.returncode}: {message}"
            ) from exc
''',
)
replace_once(
    "backend/src/runners/finite_worker.py",
    '''    try:
        if handler is None:
            raise RuntimeError(f"Unsupported job kind: {job.kind}")
        handler(settings, job)
    except Exception as exc:  # noqa: BLE001 - durable job failure boundary
''',
    '''    try:
        if handler is None:
            raise RuntimeError(f"Unsupported job kind: {job.kind}")
        handler(settings, job)
    except RetryableJobError as exc:
        with factory.begin() as session:
            current = session.get(Job, job.id)
            if current is not None:
                retry_job(
                    session,
                    current,
                    str(exc)[-4000:],
                    delay_seconds=max(1.0, settings.job_poll_seconds),
                )
                append_event(
                    session,
                    kind="JOB_RETRY_SCHEDULED",
                    aggregate_type="job",
                    aggregate_id=current.id,
                    payload={"kind": current.kind, "attempt": current.attempt},
                )
        LOGGER.warning("job retry scheduled", extra={"job_id": str(job.id)})
        return True, settings.job_poll_seconds
    except Exception as exc:  # noqa: BLE001 - durable job failure boundary
''',
)

replace_once(
    "backend/src/runners/research_missions.py",
    '''import threading
from collections.abc import Iterator
''',
    '''import threading
from collections.abc import Iterator

import httpx
''',
)
replace_once(
    "backend/src/runners/research_missions.py",
    '''BROKER_ACCEPT_POLL_SECONDS = 0.25
''',
    '''BROKER_ACCEPT_POLL_SECONDS = 0.25
RETRYABLE_MISSION_EXIT_CODE = 75
REMOTE_RESULT_UNCERTAIN = "NAUTILUS_REMOTE_RESULT_UNCERTAIN"


class RetryableMissionError(RuntimeError):
    """The remote result is ambiguous and the same durable Mission must be retried."""
''',
)
replace_once(
    "backend/src/runners/research_missions.py",
    '''    if workspace.exists():
        _git("worktree", "remove", "--force", str(workspace), cwd=repo, check=False)
        shutil.rmtree(workspace, ignore_errors=True)
    _git("branch", "-D", branch, cwd=repo, check=False)
''',
    '''    if workspace.exists():
        # A transport-uncertain runtime result must resume the exact experiment
        # files which generated its immutable experiment id. Do not rebuild the
        # worktree and silently replace that contract on a durable retry.
        return workspace
    _git("branch", "-D", branch, cwd=repo, check=False)
''',
)
# shutil is no longer used after preserving retry worktrees.
replace_once(
    "backend/src/runners/research_missions.py",
    '''import shutil
''',
    '''''',
)
replace_once(
    "backend/src/runners/research_missions.py",
    '''    factory = create_session_factory(engine)
    try:
        with _provider_credential_broker(settings.codex_api_key) as credential_socket:
''',
    '''    factory = create_session_factory(engine)
    try:
        # If a previous attempt lost the HTTP response, the worktree still holds
        # the exact contract. Reconcile it before giving Codex another turn.
        if (workspace / "experiments").exists():
            with factory() as session:
                retry_mission = session.get(ResearchMission, mission_id)
                retry_branch_id = retry_mission.branch_id if retry_mission is not None else None
            if retry_branch_id is None:
                raise QfError(
                    "MISSION_BRANCH_MISSING",
                    "Research Mission has no Branch for experiment reconciliation.",
                    500,
                )
            execute_workspace_experiments(
                settings,
                workspace=workspace,
                mission_id=mission_id,
                program_id=program_id,
                branch_id=retry_branch_id,
                already_executed=set(),
            )
        with _provider_credential_broker(settings.codex_api_key) as credential_socket:
''',
)
replace_once(
    "backend/src/runners/research_missions.py",
    '''    except Exception as exc:
        with factory() as session, session.begin():
''',
    '''    except httpx.TransportError as exc:
        with factory() as session, session.begin():
            mission = session.get(ResearchMission, mission_id)
            if mission is not None and mission.state in {"READY", "RUNNING"}:
                mission.state = "READY"
                mission.finished_at = None
                mission.error_code = REMOTE_RESULT_UNCERTAIN
                mission.summary = (
                    "Remote Nautilus result is transport-uncertain; the exact Mission worktree and "
                    "experiment id are preserved for durable reconciliation."
                )
                mission.attempt += 1
                _event(
                    session,
                    kind="MISSION_RETRY_SCHEDULED",
                    program_id=program_id,
                    mission_id=mission_id,
                    payload={"error_code": REMOTE_RESULT_UNCERTAIN},
                )
        raise RetryableMissionError(
            "remote Nautilus result uncertain; retry the same Mission contract"
        ) from exc
    except Exception as exc:
        with factory() as session, session.begin():
''',
)
replace_once(
    "backend/src/runners/research_missions.py",
    '''    run_mission(settings, UUID(args.job_id))
    return 0
''',
    '''    try:
        run_mission(settings, UUID(args.job_id))
    except RetryableMissionError:
        return RETRYABLE_MISSION_EXIT_CODE
    return 0
''',
)

# 3/6) Candidate Bundle: complete TargetPortfolioFrame rows and collision-free wheel identity.
candidate_path = Path("backend/src/candidate_bundles.py")
text = candidate_path.read_text(encoding="utf-8")
if text.count('version = f"0.0.{candidate_id.int % 1_000_000}"') != 2:
    raise SystemExit("candidate wheel version markers changed")
text = text.replace(
    'version = f"0.0.{candidate_id.int % 1_000_000}"',
    'version = f"0.0.{candidate_id.int}"',
)
start = text.index("def _member_payload(candidate: Any) -> list[dict[str, Any]]:\n")
end = text.index("\ndef _approval_summary", start)
member_function = '''def _member_payload(
    candidate: Any,
    *,
    approval: Any | None = None,
    runtime: Mapping[str, Any] | None = None,
) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    candidate_id = _value(candidate, "id")
    metrics = dict(_value(candidate, "metrics", {}) or {})
    runtime_payload = dict(runtime or {})
    as_of_time = _value(candidate, "created_at")
    effective_from = _first(approval, ("updated_at", "created_at"), as_of_time)
    effective_until = _first(approval, ("valid_until", "expires_at"))
    portfolio_state = _value(candidate, "state")
    default_confidence = metrics.get("search_adjusted_quality")
    default_universe_version_id = runtime_payload.get("universe_version_id")

    for index, member in enumerate(list(_value(candidate, "members", []) or [])):
        alpha_id = _first(member, ("alpha_qualification_id", "alpha_id"))
        if alpha_id in (None, ""):
            raise QfError(
                "CANDIDATE_MEMBER_LINEAGE_MISSING",
                "Every Candidate member must retain its Alpha Qualification id.",
                422,
                {"member_index": index},
            )
        raw_instruments = _value(member, "instrument_ids")
        if raw_instruments is None:
            singular = _first(member, ("instrument_id", "symbol", "asset", "id"))
            instruments = [singular] if singular not in (None, "") else []
        elif isinstance(raw_instruments, (list, tuple, set)):
            instruments = list(raw_instruments)
        else:
            raise QfError(
                "CANDIDATE_MEMBER_INSTRUMENTS_INVALID",
                "Candidate member instrument_ids must be a list.",
                422,
                {"member_index": index},
            )
        normalized_instruments = list(
            dict.fromkeys(
                str(value).strip()
                for value in instruments
                if value is not None and str(value).strip()
            )
        )
        if not normalized_instruments:
            raise QfError(
                "CANDIDATE_MEMBER_INSTRUMENTS_MISSING",
                "Every Candidate member must retain at least one governed instrument.",
                422,
                {"member_index": index},
            )
        target_weight = str(_first(member, ("target_weight", "weight"), "0"))
        universe_version_id = _first(
            member,
            ("universe_version_id",),
            default_universe_version_id,
        )
        confidence = _first(member, ("confidence",), default_confidence)
        for instrument_id in normalized_instruments:
            rows.append(
                _json_safe(
                    {
                        "as_of_time": as_of_time,
                        "effective_from": effective_from,
                        "effective_until": effective_until,
                        "universe_version_id": (
                            str(universe_version_id) if universe_version_id not in (None, "") else None
                        ),
                        "alpha_qualification_id": str(alpha_id),
                        "instrument_id": instrument_id,
                        "target_weight": target_weight,
                        "confidence": confidence,
                        "portfolio_state": portfolio_state,
                        "portfolio_candidate_id": str(candidate_id),
                    }
                )
            )
    return sorted(
        rows,
        key=lambda row: (
            row["instrument_id"],
            row["alpha_qualification_id"],
        ),
    )
'''
text = text[:start] + member_function + text[end:]
text = text.replace(
    '    target_weights = _member_payload(candidate)\n',
    '    target_weights = _member_payload(candidate, approval=approval, runtime=runtime)\n',
    1,
)
text = text.replace(
    '''                "custom_data_schemas": "data/custom-data-schemas/",
''',
    '''                "custom_data_schemas": "data/custom-data-schemas/",
                "target_portfolio_frame": "data/target-portfolio-frame.json",
''',
    1,
)
text = text.replace(
    '''        "data/instrument-scope.json": _json_bytes(instrument_scope),
''',
    '''        "data/instrument-scope.json": _json_bytes(instrument_scope),
        "data/target-portfolio-frame.json": _json_bytes(target_weights),
''',
    1,
)
text = text.replace(
    '''        "data/instrument-scope.json",
        "data/custom-data-schemas/index.json",
''',
    '''        "data/instrument-scope.json",
        "data/target-portfolio-frame.json",
        "data/custom-data-schemas/index.json",
''',
    1,
)
validation_marker = '''                if wheel_path != expected_wheel or wheel_path not in names:
                    findings.append({"code": "STRATEGY_WHEEL_PATH_INVALID"})
'''
validation_replacement = validation_marker + '''                target_rows = manifest.get("target_weights", [])
                target_fields = {
                    "as_of_time",
                    "effective_from",
                    "effective_until",
                    "universe_version_id",
                    "instrument_id",
                    "target_weight",
                    "confidence",
                    "portfolio_state",
                    "portfolio_candidate_id",
                }
                if not isinstance(target_rows, list) or any(
                    not isinstance(row, dict) or not target_fields.issubset(row)
                    for row in target_rows
                ):
                    findings.append({"code": "TARGET_PORTFOLIO_FRAME_INVALID"})
'''
if text.count(validation_marker) != 1:
    raise SystemExit("candidate validation marker changed")
text = text.replace(validation_marker, validation_replacement, 1)
candidate_path.write_text(text, encoding="utf-8")

# Real portfolio promotion freezes Universe Version and confidence into every target row.
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''                    "target_weight": instrument_weight,
                    "universe": alpha_universe,
                    "instrument_id": instrument_id,
''',
    '''                    "target_weight": instrument_weight,
                    "confidence": alpha_quality,
                    "universe": alpha_universe,
                    "universe_version_id": (
                        str(persisted_alpha.universe_version_id)
                        if persisted_alpha.universe_version_id is not None
                        else None
                    ),
                    "instrument_id": instrument_id,
''',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''                    "dataset_revision_ids": [str(simulation.dataset_revision_id)],
                    "alpha_qualification_ids": [str(persisted_alpha.id)],
''',
    '''                    "dataset_revision_ids": [str(simulation.dataset_revision_id)],
                    "alpha_qualification_ids": [str(persisted_alpha.id)],
                    "universe_version_id": (
                        str(persisted_alpha.universe_version_id)
                        if persisted_alpha.universe_version_id is not None
                        else None
                    ),
''',
)

# 4) Degradation dedupe is the unique Alpha × ForwardEvidenceEpisode pair.
degradation = Path("backend/src/quant_runtime/degradation.py")
text = degradation.read_text(encoding="utf-8")
text = text.replace("from sqlalchemy import exists, func, or_, select\n", "from sqlalchemy import func, or_, select\n", 1)
old = '''    handled_episode = exists(
        select(DegradationFollowup.id).where(
            DegradationFollowup.forward_evidence_episode_id == ForwardEvidenceEpisode.id
        )
    )
    episodes = list(
        session.scalars(
            select(ForwardEvidenceEpisode)
            .where(
                ForwardEvidenceEpisode.state == "FEEDBACK_COMPLETE",
                or_(
                    ForwardEvidenceEpisode.evidence["degraded"].as_boolean().is_(True),
                    func.upper(
                        func.trim(
                            ForwardEvidenceEpisode.evidence["degradation_state"].as_string()
                        )
                    ).in_(sorted(_DEGRADATION_STATES)),
                ),
                ~handled_episode,
            )
            .order_by(ForwardEvidenceEpisode.created_at.asc())
        )
    )
'''
new = '''    episodes = list(
        session.scalars(
            select(ForwardEvidenceEpisode)
            .where(
                ForwardEvidenceEpisode.state == "FEEDBACK_COMPLETE",
                or_(
                    ForwardEvidenceEpisode.evidence["degraded"].as_boolean().is_(True),
                    func.upper(
                        func.trim(
                            ForwardEvidenceEpisode.evidence["degradation_state"].as_string()
                        )
                    ).in_(sorted(_DEGRADATION_STATES)),
                ),
            )
            .order_by(ForwardEvidenceEpisode.created_at.asc())
        )
    )
    episode_ids = [episode.id for episode in episodes]
    handled_pairs = (
        set(
            session.execute(
                select(
                    DegradationFollowup.alpha_qualification_id,
                    DegradationFollowup.forward_evidence_episode_id,
                ).where(DegradationFollowup.forward_evidence_episode_id.in_(episode_ids))
            ).all()
        )
        if episode_ids
        else set()
    )
'''
if text.count(old) != 1:
    raise SystemExit("degradation episode filter changed")
text = text.replace(old, new, 1)
text = text.replace(
    '''        for alpha_id in _member_alpha_ids(candidate):
            alpha = session.get(AlphaQualification, alpha_id)
''',
    '''        for alpha_id in _member_alpha_ids(candidate):
            pair = (alpha_id, episode.id)
            if pair in handled_pairs:
                continue
            alpha = session.get(AlphaQualification, alpha_id)
''',
    1,
)
text = text.replace(
    '''            except IntegrityError:
                continue
            branch = ResearchBranch(
''',
    '''            except IntegrityError:
                handled_pairs.add(pair)
                continue
            handled_pairs.add(pair)
            branch = ResearchBranch(
''',
    1,
)
degradation.write_text(text, encoding="utf-8")

# 5) Dataset ingest claims the global receipt before any Universe mutation and binds the full input.
replace_once(
    "backend/src/api/domain.py",
    '''from sqlalchemy import func, select, text
from sqlalchemy.orm import Session
''',
    '''from sqlalchemy import func, select, text
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session
''',
)
new_ingest = '''_DATASET_INGEST_PENDING_STATUS = 102
_DATASET_INGEST_STALE_AFTER = timedelta(minutes=35)


def _dataset_ingest_identity(
    source_id: UUID,
    source: GovernedDataSource,
    payload: DatasetIngestInput,
) -> dict[str, Any]:
    return {
        "source_id": str(source_id),
        "source_governance": {
            "name": source.name,
            "provider": source.provider,
            "state": source.state,
            "preflight_state": source.preflight_state,
            "universe_scope": list(source.universe_scope or []),
            "fields": list(source.fields or []),
            "update_cadence": source.update_cadence,
            "public_config": dict(source.public_config or {}),
        },
        "ingest": payload.model_dump(mode="json", exclude_none=False),
    }


def _dataset_ingest_receipt_matches(
    receipt: PublicMutationReceipt,
    *,
    operation: str,
    identity: dict[str, Any],
) -> bool:
    return receipt.operation_name == operation and receipt.normalized_request == identity


def _claim_dataset_ingest_receipt(
    session: Session,
    *,
    key: str,
    operation: str,
    identity: dict[str, Any],
    request_id: UUID,
) -> tuple[PublicMutationReceipt, bool]:
    existing = session.execute(
        select(PublicMutationReceipt)
        .where(PublicMutationReceipt.idempotency_key == key)
        .with_for_update()
    ).scalar_one_or_none()
    if existing is None:
        now = _now()
        receipt = PublicMutationReceipt(
            idempotency_key=key,
            operation_name=operation,
            normalized_request=identity,
            response_json={
                "state": "RUNNING",
                "request_id": str(request_id),
                "attempt_started_at": now.isoformat(),
            },
            status_code=_DATASET_INGEST_PENDING_STATUS,
            created_at=now,
        )
        try:
            with session.begin_nested():
                session.add(receipt)
                session.flush()
        except IntegrityError as exc:
            if receipt in session:
                session.expunge(receipt)
            session.expire_all()
            existing = session.execute(
                select(PublicMutationReceipt)
                .where(PublicMutationReceipt.idempotency_key == key)
                .with_for_update()
            ).scalar_one_or_none()
            if existing is None:
                raise QfError(
                    "IDEMPOTENCY_RECEIPT_CONFLICT",
                    "Dataset ingest receipt could not be resolved after a concurrent request.",
                    409,
                ) from exc
        else:
            return receipt, True

    if not _dataset_ingest_receipt_matches(existing, operation=operation, identity=identity):
        raise QfError(
            "IDEMPOTENCY_KEY_REUSED",
            "The idempotency key belongs to a different request.",
            409,
        )
    if existing.status_code == 201:
        return existing, False
    if existing.status_code != _DATASET_INGEST_PENDING_STATUS:
        raise QfError(
            "IDEMPOTENCY_RECEIPT_INVALID",
            "Dataset ingest receipt has an unsupported state.",
            500,
            {"status_code": existing.status_code},
        )
    pending = dict(existing.response_json or {})
    if str(pending.get("request_id", "")) != str(request_id):
        raise QfError(
            "IDEMPOTENCY_RECEIPT_INVALID",
            "Dataset ingest receipt lost its immutable remote request id.",
            500,
        )
    state = str(pending.get("state", "")).upper()
    now = _now()
    stale = False
    try:
        started = datetime.fromisoformat(str(pending["attempt_started_at"]))
        stale = (
            started.tzinfo is not None
            and started.utcoffset() is not None
            and started < now - _DATASET_INGEST_STALE_AFTER
        )
    except (KeyError, TypeError, ValueError):
        stale = True
    if state != "RETRYABLE" and not stale:
        raise QfError(
            "DATASET_INGEST_IN_PROGRESS",
            "The exact governed Dataset ingest is already running.",
            409,
            {"request_id": str(request_id)},
        )
    existing.response_json = {
        "state": "RUNNING",
        "request_id": str(request_id),
        "attempt_started_at": now.isoformat(),
    }
    existing.status_code = _DATASET_INGEST_PENDING_STATUS
    session.flush()
    return existing, True


def _mark_dataset_ingest_retryable(
    factory: Any,
    *,
    key: str,
    operation: str,
    identity: dict[str, Any],
    request_id: UUID,
    exc: Exception,
) -> None:
    with factory.begin() as session:
        receipt = session.execute(
            select(PublicMutationReceipt)
            .where(PublicMutationReceipt.idempotency_key == key)
            .with_for_update()
        ).scalar_one_or_none()
        if (
            receipt is None
            or not _dataset_ingest_receipt_matches(
                receipt,
                operation=operation,
                identity=identity,
            )
            or receipt.status_code != _DATASET_INGEST_PENDING_STATUS
        ):
            return
        receipt.response_json = {
            "state": "RETRYABLE",
            "request_id": str(request_id),
            "attempt_started_at": _now().isoformat(),
            "last_failure_code": str(getattr(exc, "code", type(exc).__name__))[:100],
        }
        session.flush()


@router.post(
    "/data-sources/{source_id}/dataset-revisions/ingest",
    response_model=DatasetView,
    status_code=201,
)
def ingest_dataset_revision(
    source_id: UUID,
    payload: DatasetIngestInput,
    request: Request,
    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),
) -> dict[str, Any]:
    key = (idempotency_key or "").strip()
    if not key or len(key) > 200:
        raise QfError(
            "IDEMPOTENCY_KEY_REQUIRED",
            "Dataset ingest requires a 1..200 character Idempotency-Key.",
            422,
        )
    operation = f"dataset-revision.ingest:{source_id}"
    request_id = uuid5(NAMESPACE_URL, f"quazonai:dataset-ingest:{source_id}:{key}")
    factory = request.app.state.session_factory

    # Atomically bind the global idempotency key to every governed ingest input
    # before creating a Universe Version or touching the remote runtime.
    with factory() as session, session.begin():
        source = session.execute(
            select(GovernedDataSource)
            .where(GovernedDataSource.id == source_id)
            .with_for_update()
        ).scalar_one_or_none()
        if (
            source is None
            or source.state != "ACTIVE"
            or source.preflight_state != "READY"
        ):
            raise QfError(
                "DATA_SOURCE_NOT_READY",
                "Dataset ingest requires an active ready governed Data Source.",
                409,
            )
        required_fields = {"event_time", "available_time", "bid_price", "ask_price"}
        if not required_fields.issubset(set(source.fields or [])):
            raise QfError(
                "DATA_SOURCE_FIELDS_INCOMPLETE",
                "QuoteTick ingest requires event/availability and bid/ask fields.",
                422,
                {"required_fields": sorted(required_fields)},
            )
        if source.universe_scope and payload.universe_name not in source.universe_scope:
            raise QfError(
                "DATA_SOURCE_UNIVERSE_SCOPE_MISMATCH",
                "Data Source is not governed for the requested Universe.",
                422,
            )
        identity = _dataset_ingest_identity(source_id, source, payload)
        receipt, claimed = _claim_dataset_ingest_receipt(
            session,
            key=key,
            operation=operation,
            identity=identity,
            request_id=request_id,
        )
        if not claimed:
            return dict(receipt.response_json)

        if session.bind is not None and session.bind.dialect.name == "postgresql":
            session.execute(text("SELECT pg_advisory_xact_lock(:lock_key)"), {"lock_key": 220024})
        latest = session.scalar(
            select(MarketUniverseVersion)
            .where(MarketUniverseVersion.universe_key == payload.universe_key)
            .order_by(MarketUniverseVersion.version_no.desc())
            .limit(1)
            .with_for_update()
        )
        if (
            latest is not None
            and latest.state == "ACTIVE"
            and latest.name == payload.universe_name
            and latest.spec_json == payload.universe_spec
        ):
            universe = latest
        else:
            universe = MarketUniverseVersion(
                universe_key=payload.universe_key,
                version_no=(latest.version_no + 1) if latest is not None else 1,
                name=payload.universe_name,
                state="ACTIVE",
                spec_json=payload.universe_spec,
                created_at=_now(),
            )
            session.add(universe)
            session.flush()
        source_provider = source.provider or source.name
        universe_id = universe.id

    try:
        ingest_request = CatalogIngestRequest(
            request_id=request_id,
            catalog_key=payload.catalog_key,
            provider=source_provider,
            source=payload.source,
            source_license=payload.source_license,
            instruments=payload.instruments,
        )
        ingested, validated = _remote_ingest_and_validate(ingest_request)
        if not validated.valid:
            raise QfError(
                "NAUTILUS_CATALOG_VALIDATION_FAILED",
                "Remote Nautilus catalog did not pass validation.",
                422,
                {"findings": validated.findings},
            )
        if (
            ingested.catalog_key != payload.catalog_key
            or validated.catalog_key != payload.catalog_key
            or sorted(ingested.instrument_scope) != sorted(validated.instrument_scope)
            or ingested.row_count != validated.row_count
            or validated.event_time_start != ingested.event_time_start
            or validated.event_time_end != ingested.event_time_end
            or validated.available_time_start != ingested.available_time_start
            or validated.available_time_end != ingested.available_time_end
        ):
            raise QfError(
                "NAUTILUS_CATALOG_VALIDATION_MISMATCH",
                "Validated catalog facts differ from the immutable ingest result.",
                502,
            )
        quality_result = dict(ingested.quality_result or {})
        point_in_time_result = dict(ingested.point_in_time_result or {})
        if (
            str(quality_result.get("state", "")).upper() != "VALID"
            or str(point_in_time_result.get("state", "")).upper() != "VALID"
        ):
            raise QfError(
                "DATASET_GOVERNANCE_FAILED",
                "Ingested catalog must pass quality and point-in-time governance.",
                422,
            )

        with factory() as session, session.begin():
            if session.bind is not None and session.bind.dialect.name == "postgresql":
                session.execute(text("SELECT pg_advisory_xact_lock(:lock_key)"), {"lock_key": 220027})
            receipt = session.execute(
                select(PublicMutationReceipt)
                .where(PublicMutationReceipt.idempotency_key == key)
                .with_for_update()
            ).scalar_one_or_none()
            source = session.execute(
                select(GovernedDataSource)
                .where(GovernedDataSource.id == source_id)
                .with_for_update()
            ).scalar_one_or_none()
            universe = session.get(MarketUniverseVersion, universe_id)
            if (
                receipt is None
                or not _dataset_ingest_receipt_matches(
                    receipt,
                    operation=operation,
                    identity=identity,
                )
                or receipt.status_code != _DATASET_INGEST_PENDING_STATUS
            ):
                raise QfError(
                    "IDEMPOTENCY_RECEIPT_INVALID",
                    "Dataset ingest receipt changed while the remote runtime was executing.",
                    409,
                )
            if (
                source is None
                or source.state != "ACTIVE"
                or source.preflight_state != "READY"
                or universe is None
                or universe.state != "ACTIVE"
                or _dataset_ingest_identity(source_id, source, payload) != identity
            ):
                raise QfError(
                    "DATASET_GOVERNANCE_CHANGED",
                    "Data Source or governed ingest identity changed while the catalog was ingesting.",
                    409,
                )

            existing = session.scalar(
                select(DatasetRevision).where(DatasetRevision.catalog_uri == ingested.catalog_uri)
            )
            if existing is not None:
                if (
                    existing.data_source_id != source.id
                    or existing.universe_version_id != universe.id
                    or existing.instrument_scope != ingested.instrument_scope
                    or existing.row_count != ingested.row_count
                ):
                    raise QfError(
                        "DATASET_CATALOG_IDENTITY_CONFLICT",
                        "Catalog URI is already bound to different governed facts.",
                        409,
                    )
                result = _dataset_view(existing)
            else:
                revision_no = int(
                    session.scalar(
                        select(func.max(DatasetRevision.revision_no)).where(
                            DatasetRevision.data_source_id == source.id,
                            DatasetRevision.universe_version_id == universe.id,
                            DatasetRevision.partition == "DISCOVERY",
                        )
                    )
                    or 0
                ) + 1
                revision = DatasetRevision(
                    data_source_id=source.id,
                    universe_version_id=universe.id,
                    universe_name=universe.name,
                    revision_no=revision_no,
                    schema_version=ingested.schema_revision,
                    event_start=ingested.event_time_start,
                    event_end=ingested.event_time_end,
                    available_start=ingested.available_time_start,
                    available_end=ingested.available_time_end,
                    row_count=ingested.row_count,
                    quality_state="VALID",
                    point_in_time_state="VALID",
                    partition="DISCOVERY",
                    created_at=_now(),
                    provider_name=source_provider,
                    source_license=payload.source_license,
                    catalog_uri=ingested.catalog_uri,
                    nautilus_data_type=ingested.nautilus_data_type,
                    instrument_scope=ingested.instrument_scope,
                    schema_revision=ingested.schema_revision,
                    quality_result=quality_result,
                    point_in_time_result=point_in_time_result,
                    ingested_at=ingested.ingested_at,
                )
                session.add(revision)
                session.flush()
                _event(
                    session,
                    "DATASET_REVISION_INGESTED",
                    "DATASET_REVISION",
                    revision.id,
                    {
                        "data_source_id": str(source.id),
                        "universe_version_id": str(universe.id),
                        "catalog_uri": revision.catalog_uri,
                        "row_count": revision.row_count,
                    },
                    actor_kind="HUMAN",
                )
                result = _dataset_view(revision)
            receipt.response_json = result
            receipt.status_code = 201
            session.flush()
            return result
    except Exception as exc:
        _mark_dataset_ingest_retryable(
            factory,
            key=key,
            operation=operation,
            identity=identity,
            request_id=request_id,
            exc=exc,
        )
        raise


'''
replace_between(
    "backend/src/api/domain.py",
    '@router.post(\n    "/data-sources/{source_id}/dataset-revisions/ingest",\n',
    '@router.get("/datasets", response_model=list[DatasetView])\n',
    new_ingest,
)

# Focused regressions for round6.
Path("backend/tests/unit/test_issue22_codex_round6.py").write_text('''from __future__ import annotations\n\nfrom datetime import UTC, datetime\nfrom types import SimpleNamespace\nfrom uuid import UUID, uuid4\n\nfrom candidate_bundles import _member_payload, _strategy_wheel_filename\nfrom jobs import retry_job\n\n\ndef test_strategy_wheel_version_uses_collision_free_candidate_identity() -> None:\n    first = UUID(int=1)\n    second = UUID(int=1_000_001)\n    assert _strategy_wheel_filename(first) != _strategy_wheel_filename(second)\n    assert str(first.int) in _strategy_wheel_filename(first)\n    assert str(second.int) in _strategy_wheel_filename(second)\n\n\ndef test_target_portfolio_rows_freeze_canonical_identity_and_validity() -> None:\n    candidate_id = uuid4()\n    universe_version_id = uuid4()\n    alpha_id = uuid4()\n    as_of = datetime(2026, 8, 28, 12, tzinfo=UTC)\n    effective = datetime(2026, 8, 28, 13, tzinfo=UTC)\n    expires = datetime(2026, 9, 4, 13, tzinfo=UTC)\n    candidate = SimpleNamespace(\n        id=candidate_id,\n        created_at=as_of,\n        state="READY",\n        metrics={"search_adjusted_quality": 0.81},\n        members=[\n            {\n                "alpha_qualification_id": str(alpha_id),\n                "universe_version_id": str(universe_version_id),\n                "instrument_id": "EUR/USD.SIM",\n                "target_weight": 1.0,\n            }\n        ],\n    )\n    approval = SimpleNamespace(updated_at=effective, created_at=effective, valid_until=expires)\n    row = _member_payload(candidate, approval=approval, runtime={})[0]\n    assert row["as_of_time"] == as_of.isoformat()\n    assert row["effective_from"] == effective.isoformat()\n    assert row["effective_until"] == expires.isoformat()\n    assert row["universe_version_id"] == str(universe_version_id)\n    assert row["confidence"] == 0.81\n    assert row["portfolio_state"] == "READY"\n    assert row["portfolio_candidate_id"] == str(candidate_id)\n\n\ndef test_retry_job_releases_lease_without_terminal_failure() -> None:\n    job = SimpleNamespace(\n        state="LEASED",\n        lease_owner="worker",\n        lease_expires_at=datetime.now(UTC),\n        last_error=None,\n        available_at=datetime.now(UTC),\n    )\n\n    class Session:\n        def flush(self) -> None:\n            return None\n\n    retry_job(Session(), job, "uncertain remote result", delay_seconds=3)\n    assert job.state == "READY"\n    assert job.lease_owner is None\n    assert job.lease_expires_at is None\n    assert job.last_error == "uncertain remote result"\n''', encoding="utf-8")

# Add endpoint-boundary equality regression to round5 tests.
round5 = Path("backend/tests/unit/test_issue22_codex_round5.py")
text = round5.read_text(encoding="utf-8")
needle = '''    with pytest.raises(QfError) as overlap_error:
        _select_sealed_dataset(_DatasetSession(overlap), source, overlap.id)
    assert overlap_error.value.code == "SEALED_DATASET_TIME_OVERLAP"
'''
replacement = needle + '''
    touching = _revision(
        partition="SEALED",
        catalog_uri="nautilus-catalog://sealed-touching",
        start=source.event_end,
        end=start + timedelta(days=20),
        universe_id=universe_id,
    )
    with pytest.raises(QfError) as touching_error:
        _select_sealed_dataset(_DatasetSession(touching), source, touching.id)
    assert touching_error.value.code == "SEALED_DATASET_TIME_OVERLAP"
'''
if text.count(needle) != 1:
    raise SystemExit("round5 overlap test marker changed")
round5.write_text(text.replace(needle, replacement, 1), encoding="utf-8")

# Temporary patch machinery must not survive in the product branch.
Path("tools/issue22_round6_patch.py").unlink(missing_ok=True)
Path(".github/workflows/issue22-round6-maintenance.yml").unlink(missing_ok=True)
