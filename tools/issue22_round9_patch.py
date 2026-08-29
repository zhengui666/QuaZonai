from __future__ import annotations

from pathlib import Path
import re


def load(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def save(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    text = load(path)
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, got {count}: {old[:120]!r}")
    save(path, text.replace(old, new, 1))


def replace_regex(path: str, pattern: str, new: str) -> None:
    text = load(path)
    updated, count = re.subn(pattern, new, text, count=1, flags=re.S)
    if count != 1:
        raise SystemExit(f"{path}: regex expected one match, got {count}: {pattern[:120]!r}")
    save(path, updated)


BRANCH = "feat/issue-22-remote-nautilus-runtime"

# ---------------------------------------------------------------------------
# 1) Portfolio promotion: research phase is separated from sealed finalization.
# ---------------------------------------------------------------------------
p = "backend/src/quant_runtime/promotion.py"
replace_once(p, "from uuid import UUID, uuid4\n", "from uuid import UUID\n")
marker = "@dataclass(frozen=True, slots=True)\nclass CandidatePromotion:\n"
preparation = '''@dataclass(frozen=True, slots=True)\nclass PortfolioSimulationPreparation:\n    simulation_experiment_id: UUID\n    selected_alpha_id: UUID\n\n\ndef prepare_portfolio_simulation(\n    factory: sessionmaker[Session],\n    *,\n    portfolio_program_id: UUID,\n    alpha_ids: list[UUID],\n    simulation_experiment_id: UUID,\n) -> PortfolioSimulationPreparation:\n    \"\"\"Run only the research-visible transaction simulation before sealed finalization.\n\n    This function deliberately never performs a SEALED call. The API process may use\n    only the ordinary research runtime credential; the independently deployed sealed\n    worker later reuses this exact immutable simulation id and performs the second\n    holdout run with its sealed-only credential.\n    \"\"\"\n    requested_alpha_ids = _canonical_alpha_ids(alpha_ids)\n    if not requested_alpha_ids:\n        raise QfError(\"ALPHA_SELECTION_EMPTY\", \"At least one qualified Alpha is required.\", 422)\n    with factory() as session:\n        portfolio_program = session.get(PortfolioProgram, portfolio_program_id)\n        if portfolio_program is None:\n            raise QfError(\"PORTFOLIO_PROGRAM_NOT_FOUND\", \"Portfolio Program does not exist.\", 404)\n        mandate = _bound_mandate(session, portfolio_program)\n        alpha, source, request = _load_portfolio_source(\n            session, requested_alpha_ids, mandate\n        )\n        constraints = _validate_mandate_before_simulation(mandate, alpha)\n        simulation_request = request.model_copy(\n            update={\n                \"experiment_id\": simulation_experiment_id,\n                \"mode\": ExperimentMode.PORTFOLIO,\n                \"tags\": {\n                    **request.tags,\n                    \"portfolio_program_id\": str(portfolio_program_id),\n                    \"alpha_qualification_id\": str(alpha.id),\n                    \"optimizer\": \"MAX_SEARCH_ADJUSTED_QUALITY_V1\",\n                    \"allocation_policy\": \"DERIVE_FROM_EXECUTED_NOTIONAL_V1\",\n                },\n            }\n        )\n        source_program_id = source.program_id\n        source_branch_id = source.branch_id\n        alpha_id = alpha.id\n\n    simulation = ExperimentCoordinator(factory).execute(\n        mission_id=None,\n        program_id=source_program_id,\n        branch_id=source_branch_id,\n        request=simulation_request,\n        sealed=False,\n    )\n    simulation_evidence = _require_real_transaction_evidence(simulation)\n    _validate_mandate_after_simulation(constraints, simulation_evidence)\n    return PortfolioSimulationPreparation(\n        simulation_experiment_id=simulation.id,\n        selected_alpha_id=alpha_id,\n    )\n\n\n'''
replace_once(p, marker, preparation + marker)

# ---------------------------------------------------------------------------
# 2) Public API: queue the second SEALED run/finalization to the sealed worker.
# ---------------------------------------------------------------------------
p = "backend/src/api/research_runtime.py"
replace_once(
    p,
    "from quant_runtime.promotion import simulate_portfolio_candidate\n",
    "from quant_runtime.promotion import prepare_portfolio_simulation\n",
)
replace_once(
    p,
    'SEALED_DATASET_REGISTRATION_JOB_KIND = "SEALED_DATASET_REGISTRATION"\n',
    'SEALED_DATASET_REGISTRATION_JOB_KIND = "SEALED_DATASET_REGISTRATION"\n'
    'SEALED_PORTFOLIO_PROMOTION_JOB_KIND = "SEALED_PORTFOLIO_PROMOTION"\n',
)
old_model = '''class PortfolioSimulationResult(StrictModel):\n    candidate_id: UUID\n    approval_id: UUID\n    simulation_experiment_id: UUID\n    selected_alpha_id: UUID\n'''
new_model = '''class PortfolioSimulationResult(StrictModel):\n    job_id: UUID\n    state: str\n    simulation_experiment_id: UUID\n    portfolio_sealed_experiment_id: UUID\n    selected_alpha_id: UUID\n    candidate_id: UUID | None = None\n    approval_id: UUID | None = None\n'''
replace_once(p, old_model, new_model)
replace_once(
    p,
    '''    if existing.status_code == 200:\n        return existing, False, experiment_id, None\n    if existing.status_code != _SIMULATION_PENDING_STATUS:\n''',
    '''    if existing.status_code in {200, 202}:\n        return existing, False, experiment_id, portfolio_sealed_experiment_id\n    if existing.status_code != _SIMULATION_PENDING_STATUS:\n''',
)
# Enforce the governed source's declared Universe scope both before enqueue and later in worker.
replace_once(
    p,
    '''        if source is None or source.state != "ACTIVE" or source.preflight_state != "READY":\n            raise QfError("DATA_SOURCE_NOT_READY", "Sealed registration requires an active ready Data Source.", 409)\n        jobs = list(session.scalars(\n''',
    '''        if source is None or source.state != "ACTIVE" or source.preflight_state != "READY":\n            raise QfError("DATA_SOURCE_NOT_READY", "Sealed registration requires an active ready Data Source.", 409)\n        source_scope = {str(value) for value in (source.universe_scope or []) if str(value)}\n        if source_scope and universe.name not in source_scope:\n            raise QfError(\n                "DATA_SOURCE_UNIVERSE_SCOPE_MISMATCH",\n                "Sealed registration Data Source is not governed for this Universe Version.",\n                422,\n                {"universe": universe.name},\n            )\n        jobs = list(session.scalars(\n''',
)
endpoint_pattern = r'@router\.post\(\n    "/portfolio-programs/\{portfolio_program_id\}/simulate-candidate",[\s\S]*\Z'
endpoint = '''@router.post(\n    "/portfolio-programs/{portfolio_program_id}/simulate-candidate",\n    response_model=PortfolioSimulationResult,\n    status_code=202,\n)\ndef simulate_candidate(\n    portfolio_program_id: UUID,\n    payload: PortfolioSimulationInput,\n    request: Request,\n    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),\n) -> PortfolioSimulationResult:\n    key = (idempotency_key or "").strip()\n    if not key or len(key) > 200:\n        raise QfError(\n            "IDEMPOTENCY_KEY_REQUIRED",\n            "Candidate simulation requires a 1..200 character Idempotency-Key.",\n            422,\n        )\n    normalized: dict[str, object] = {\n        "portfolio_program_id": str(portfolio_program_id),\n        "alpha_ids": sorted(str(value) for value in payload.alpha_ids),\n    }\n    operation = f"SIMULATE_NAUTILUS_CANDIDATE:{portfolio_program_id}"\n    factory = request.app.state.session_factory\n    with factory.begin() as session:\n        receipt, claimed, experiment_id, portfolio_sealed_experiment_id = _claim_simulation_receipt(\n            session,\n            key=key,\n            operation=operation,\n            normalized=normalized,\n        )\n        if not claimed:\n            return PortfolioSimulationResult.model_validate(receipt.response_json)\n\n    if portfolio_sealed_experiment_id is None:\n        raise QfError(\n            "IDEMPOTENCY_RECEIPT_INVALID",\n            "Candidate simulation receipt is missing its portfolio sealed identity.",\n            500,\n        )\n    try:\n        prepared = prepare_portfolio_simulation(\n            factory,\n            portfolio_program_id=portfolio_program_id,\n            alpha_ids=payload.alpha_ids,\n            simulation_experiment_id=experiment_id,\n        )\n    except Exception as exc:\n        _mark_simulation_retryable(\n            factory,\n            key=key,\n            operation=operation,\n            normalized=normalized,\n            experiment_id=experiment_id,\n            exc=exc,\n        )\n        raise\n\n    with factory.begin() as session:\n        receipt = session.execute(\n            select(PublicMutationReceipt)\n            .where(PublicMutationReceipt.idempotency_key == key)\n            .with_for_update()\n        ).scalar_one_or_none()\n        if receipt is None or not _simulation_receipt_matches(\n            receipt, operation=operation, normalized=normalized\n        ):\n            raise QfError(\n                "IDEMPOTENCY_RECEIPT_CONFLICT",\n                "Candidate simulation receipt changed before sealed finalization was queued.",\n                409,\n            )\n        if receipt.status_code == 200 or receipt.status_code == 202:\n            return PortfolioSimulationResult.model_validate(receipt.response_json)\n        receipt_simulation_id, receipt_sealed_id = _pending_simulation_experiment_ids(\n            receipt, require_portfolio_sealed=True\n        )\n        if (\n            receipt_simulation_id != experiment_id\n            or receipt_sealed_id != portfolio_sealed_experiment_id\n        ):\n            raise QfError(\n                "IDEMPOTENCY_RECEIPT_CONFLICT",\n                "Candidate simulation receipt changed experiment identity.",\n                409,\n            )\n        job = enqueue_job(\n            session,\n            kind=SEALED_PORTFOLIO_PROMOTION_JOB_KIND,\n            resource_type="PORTFOLIO_PROGRAM",\n            resource_id=portfolio_program_id,\n            payload={\n                "alpha_ids": normalized["alpha_ids"],\n                "simulation_experiment_id": str(experiment_id),\n                "portfolio_sealed_experiment_id": str(portfolio_sealed_experiment_id),\n                "idempotency_key": key,\n                "idempotency_operation": operation,\n                "idempotency_normalized": normalized,\n            },\n        )\n        response = PortfolioSimulationResult(\n            job_id=job.id,\n            state=job.state,\n            simulation_experiment_id=prepared.simulation_experiment_id,\n            portfolio_sealed_experiment_id=portfolio_sealed_experiment_id,\n            selected_alpha_id=prepared.selected_alpha_id,\n        )\n        receipt.response_json = response.model_dump(mode="json")\n        receipt.status_code = 202\n        session.flush()\n        return response\n'''
replace_regex(p, endpoint_pattern, endpoint)

# ---------------------------------------------------------------------------
# 3) Sealed worker: own portfolio holdout/finalization and retry uncertain transport.
# ---------------------------------------------------------------------------
p = "backend/src/runners/sealed_worker.py"
replace_once(p, "import argparse\n", "import argparse\n\nimport httpx\n")
replace_once(
    p,
    "from db.models import DatasetRevision, GovernedDataSource, Job, MarketUniverseVersion\n",
    "from db.models import (\n    DatasetRevision,\n    GovernedDataSource,\n    Job,\n    MarketUniverseVersion,\n    PublicMutationReceipt,\n)\n",
)
replace_once(
    p,
    "    renew_job_lease,\n)",
    "    renew_job_lease,\n    retry_job,\n)",
)
replace_once(
    p,
    "from quant_runtime.promotion import qualify_alpha\n",
    "from quant_runtime.promotion import qualify_alpha, simulate_portfolio_candidate\n",
)
replace_once(
    p,
    'SEALED_DATASET_REGISTRATION_JOB_KIND = "SEALED_DATASET_REGISTRATION"\nSEALED_JOB_KINDS = {SEALED_JOB_KIND, SEALED_DATASET_REGISTRATION_JOB_KIND}\n',
    'SEALED_DATASET_REGISTRATION_JOB_KIND = "SEALED_DATASET_REGISTRATION"\n'
    'SEALED_PORTFOLIO_PROMOTION_JOB_KIND = "SEALED_PORTFOLIO_PROMOTION"\n'
    'SEALED_JOB_KINDS = {\n'
    '    SEALED_JOB_KIND,\n'
    '    SEALED_DATASET_REGISTRATION_JOB_KIND,\n'
    '    SEALED_PORTFOLIO_PROMOTION_JOB_KIND,\n'
    '}\n',
)
insert_before_registration = '''\n\ndef _execute_portfolio_promotion(factory: SessionFactory, job: Job) -> dict[str, str]:\n    payload = dict(job.payload or {})\n    simulation_experiment_id = _uuid_payload(job, "simulation_experiment_id")\n    portfolio_sealed_experiment_id = _uuid_payload(job, "portfolio_sealed_experiment_id")\n    alpha_ids_raw = payload.get("alpha_ids", [])\n    if not isinstance(alpha_ids_raw, list):\n        raise RuntimeError("sealed portfolio job alpha_ids must be a list")\n    try:\n        alpha_ids = [UUID(str(value)) for value in alpha_ids_raw]\n    except ValueError as exc:\n        raise RuntimeError("sealed portfolio job alpha_ids must contain UUIDs") from exc\n    result = simulate_portfolio_candidate(\n        factory,\n        portfolio_program_id=job.resource_id,\n        alpha_ids=alpha_ids,\n        simulation_experiment_id=simulation_experiment_id,\n        portfolio_sealed_experiment_id=portfolio_sealed_experiment_id,\n    )\n    response = {\n        "job_id": str(job.id),\n        "state": "SUCCEEDED",\n        "candidate_id": str(result.candidate_id),\n        "approval_id": str(result.approval_id),\n        "simulation_experiment_id": str(result.simulation_experiment_id),\n        "portfolio_sealed_experiment_id": str(portfolio_sealed_experiment_id),\n        "selected_alpha_id": str(result.selected_alpha_id),\n    }\n    key = str(payload.get("idempotency_key") or "")\n    operation = str(payload.get("idempotency_operation") or "")\n    normalized = payload.get("idempotency_normalized")\n    if not key or not operation or not isinstance(normalized, dict):\n        raise RuntimeError("sealed portfolio job lost its public idempotency receipt identity")\n    with factory.begin() as session:\n        receipt = session.execute(\n            select(PublicMutationReceipt)\n            .where(PublicMutationReceipt.idempotency_key == key)\n            .with_for_update()\n        ).scalar_one_or_none()\n        if receipt is None:\n            raise RuntimeError("sealed portfolio job public idempotency receipt is missing")\n        if receipt.operation_name != operation or receipt.normalized_request != normalized:\n            raise RuntimeError("sealed portfolio job public idempotency receipt identity changed")\n        persisted_simulation = str((receipt.response_json or {}).get("simulation_experiment_id") or "")\n        persisted_sealed = str((receipt.response_json or {}).get("portfolio_sealed_experiment_id") or "")\n        if persisted_simulation != str(simulation_experiment_id) or persisted_sealed != str(portfolio_sealed_experiment_id):\n            raise RuntimeError("sealed portfolio job experiment identity changed")\n        receipt.response_json = response\n        receipt.status_code = 200\n        session.flush()\n    return response\n'''
replace_once(
    p,
    "\ndef _execute_sealed_dataset_registration(\n",
    insert_before_registration + "\n\ndef _execute_sealed_dataset_registration(\n",
)
replace_once(
    p,
    '''        if source is None or source.state != "ACTIVE" or source.preflight_state != "READY":\n            raise RuntimeError("sealed registration Data Source is no longer ready")\n        existing = session.scalar(\n''',
    '''        if source is None or source.state != "ACTIVE" or source.preflight_state != "READY":\n            raise RuntimeError("sealed registration Data Source is no longer ready")\n        source_scope = {str(value) for value in (source.universe_scope or []) if str(value)}\n        if source_scope and universe.name not in source_scope:\n            raise RuntimeError("sealed registration Data Source no longer covers this Universe")\n        existing = session.scalar(\n''',
)
replace_once(
    p,
    '''            if job.kind == SEALED_JOB_KIND:\n                result = _execute_qualification(factory, job)\n            elif job.kind == SEALED_DATASET_REGISTRATION_JOB_KIND:\n                result = _execute_sealed_dataset_registration(factory, job)\n            else:\n''',
    '''            if job.kind == SEALED_JOB_KIND:\n                result = _execute_qualification(factory, job)\n            elif job.kind == SEALED_DATASET_REGISTRATION_JOB_KIND:\n                result = _execute_sealed_dataset_registration(factory, job)\n            elif job.kind == SEALED_PORTFOLIO_PROMOTION_JOB_KIND:\n                result = _execute_portfolio_promotion(factory, job)\n            else:\n''',
)
replace_once(
    p,
    '''    except Exception as exc:  # noqa: BLE001 - durable privileged job boundary\n        with factory.begin() as session:\n''',
    '''    except httpx.TransportError as exc:\n        with factory.begin() as session:\n            current = session.get(Job, job.id)\n            if (\n                current is not None\n                and current.state == "LEASED"\n                and current.lease_owner == owner\n            ):\n                retry_job(\n                    session,\n                    current,\n                    "sealed remote result is transport-uncertain; retrying the same durable experiment",\n                    delay_seconds=min(max(settings.job_poll_seconds, 1.0), 30.0),\n                )\n                append_event(\n                    session,\n                    kind="SEALED_JOB_RETRYABLE",\n                    aggregate_type="job",\n                    aggregate_id=current.id,\n                    payload={\n                        "error_code": type(exc).__name__,\n                        "experiment_id": str((current.payload or {}).get("sealed_experiment_id") or (current.payload or {}).get("portfolio_sealed_experiment_id") or ""),\n                    },\n                )\n        LOGGER.warning("sealed job remote result is uncertain; durable retry retained", extra={"job_id": str(job.id)})\n        return True, settings.job_poll_seconds\n    except Exception as exc:  # noqa: BLE001 - durable privileged job boundary\n        with factory.begin() as session:\n''',
)

# ---------------------------------------------------------------------------
# 4) Mission workspace: privileged parent writes are symlink-safe and atomic.
# ---------------------------------------------------------------------------
p = "backend/src/quant_runtime/workspace.py"
replace_once(p, "from uuid import UUID\n", "from uuid import UUID, uuid4\n")
helper_marker = '_DEGRADATION_REASON_CODE = re.compile(r"[A-Z0-9][A-Z0-9_:-]{0,79}")\n\n\n'
helper = '''_DEGRADATION_REASON_CODE = re.compile(r"[A-Z0-9][A-Z0-9_:-]{0,79}")\n\n\ndef write_parent_owned_workspace_file(path: Path, content: str) -> None:\n    \"\"\"Atomically replace a fixed parent-owned file without following Mission links.\"\"\"\n    parent = path.parent.resolve(strict=True)\n    directory_flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0)\n    directory_fd = os.open(parent, directory_flags)\n    temporary_name = f".{path.name}.parent-{uuid4().hex}.tmp"\n    try:\n        try:\n            current = os.stat(path.name, dir_fd=directory_fd, follow_symlinks=False)\n        except FileNotFoundError:\n            current = None\n        if current is not None and not stat.S_ISREG(current.st_mode):\n            raise QfError(\n                "MISSION_WORKSPACE_PATH_UNSAFE",\n                "Parent-owned Mission workspace files must be regular files, not links.",\n                422,\n                {"path": path.name},\n            )\n        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)\n        fd = os.open(temporary_name, flags, 0o600, dir_fd=directory_fd)\n        try:\n            payload = content.encode("utf-8")\n            offset = 0\n            while offset < len(payload):\n                offset += os.write(fd, payload[offset:])\n            os.fsync(fd)\n        finally:\n            os.close(fd)\n        os.replace(\n            temporary_name,\n            path.name,\n            src_dir_fd=directory_fd,\n            dst_dir_fd=directory_fd,\n        )\n        os.fsync(directory_fd)\n    finally:\n        try:\n            os.unlink(temporary_name, dir_fd=directory_fd)\n        except FileNotFoundError:\n            pass\n        os.close(directory_fd)\n\n\ndef _ensure_parent_owned_directory(path: Path) -> None:\n    try:\n        info = os.lstat(path)\n    except FileNotFoundError:\n        path.mkdir(mode=0o700, parents=False)\n        info = os.lstat(path)\n    if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode):\n        raise QfError(\n            "MISSION_WORKSPACE_PATH_UNSAFE",\n            "Parent-owned Mission workspace directory must not be a link.",\n            422,\n            {"path": path.name},\n        )\n\n\n'''
replace_once(p, helper_marker, helper)
replace_once(
    p,
    '''        (workspace / "DEGRADATION_CONTEXT.json").write_text(\n            json.dumps(\n                degradation_context,\n                ensure_ascii=False,\n                indent=2,\n                default=str,\n            ),\n            encoding="utf-8",\n        )\n''',
    '''        write_parent_owned_workspace_file(\n            workspace / "DEGRADATION_CONTEXT.json",\n            json.dumps(\n                degradation_context,\n                ensure_ascii=False,\n                indent=2,\n                default=str,\n            ),\n        )\n''',
)
replace_once(
    p,
    '''    (workspace / "DATASETS.json").write_text(\n        json.dumps(\n            {"policy": "GOVERNED_DISCOVERY_ONLY", "datasets": datasets},\n            ensure_ascii=False,\n            indent=2,\n            default=str,\n        ),\n        encoding="utf-8",\n    )\n''',
    '''    write_parent_owned_workspace_file(\n        workspace / "DATASETS.json",\n        json.dumps(\n            {"policy": "GOVERNED_DISCOVERY_ONLY", "datasets": datasets},\n            ensure_ascii=False,\n            indent=2,\n            default=str,\n        ),\n    )\n''',
)
replace_once(
    p,
    '''    (workspace / "EXPERIMENT_CONTRACT.schema.json").write_text(\n        json.dumps(\n            BacktestExperimentRequest.model_json_schema(),\n            ensure_ascii=False,\n            indent=2,\n        ),\n        encoding="utf-8",\n    )\n    experiments_root = workspace / "experiments"\n    experiments_root.mkdir(parents=True, exist_ok=True)\n''',
    '''    write_parent_owned_workspace_file(\n        workspace / "EXPERIMENT_CONTRACT.schema.json",\n        json.dumps(\n            BacktestExperimentRequest.model_json_schema(),\n            ensure_ascii=False,\n            indent=2,\n        ),\n    )\n    experiments_root = workspace / "experiments"\n    _ensure_parent_owned_directory(experiments_root)\n''',
)
replace_once(
    p,
    '    (workspace / "NAUTILUS_EXPERIMENTS.md").write_text(\n        """# Governed Nautilus experiment interface\n',
    '    write_parent_owned_workspace_file(\n        workspace / "NAUTILUS_EXPERIMENTS.md",\n        """# Governed Nautilus experiment interface\n',
)
replace_once(
    p,
    '''`RESULT.md`; do not fabricate a backtest.\n""",\n        encoding="utf-8",\n    )\n''',
    '''`RESULT.md`; do not fabricate a backtest.\n""",\n    )\n''',
)

p = "backend/src/runners/research_missions.py"
replace_once(
    p,
    "from quant_runtime.workspace import execute_workspace_experiments, prepare_experiment_workspace\n",
    "from quant_runtime.workspace import (\n    execute_workspace_experiments,\n    prepare_experiment_workspace,\n    write_parent_owned_workspace_file,\n)\n",
)
replace_once(
    p,
    '    (workspace / "MISSION.md").write_text(context, encoding="utf-8")\n',
    '    write_parent_owned_workspace_file(workspace / "MISSION.md", context)\n',
)

# ---------------------------------------------------------------------------
# 5) Gateway receipt locking: per-experiment long lock, global registry short lock.
# ---------------------------------------------------------------------------
p = "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py"
pattern = r'    def _idempotent_run\(\n[\s\S]*?\n    def run_backtest_idempotent\('
new_idempotent = '''    def _idempotent_run(\n        self,\n        operation: str,\n        request: BacktestExperimentRequest,\n        runner: Callable[[BacktestExperimentRequest], dict[str, Any]],\n    ) -> dict[str, Any]:\n        canonical_request = request.model_dump(mode="json")\n        receipt_key = str(request.experiment_id)\n        experiment_lock_path = self._run_root / f"{request.experiment_id.hex}.lock"\n        experiment_lock_fd = os.open(experiment_lock_path, os.O_CREAT | os.O_RDWR, 0o600)\n\n        def registry_lock() -> int:\n            fd = os.open(self._run_lock_path, os.O_CREAT | os.O_RDWR, 0o600)\n            fcntl.flock(fd, fcntl.LOCK_EX)\n            return fd\n\n        def registry_unlock(fd: int) -> None:\n            try:\n                fcntl.flock(fd, fcntl.LOCK_UN)\n            finally:\n                os.close(fd)\n\n        try:\n            # Only identical experiment ids serialize for the duration of BacktestNode.\n            # The global receipt registry lock is held only while reading/writing JSON.\n            fcntl.flock(experiment_lock_fd, fcntl.LOCK_EX)\n            global_fd = registry_lock()\n            try:\n                receipts = self._load_run_receipts()\n                receipt = receipts.get(receipt_key)\n                if receipt is not None:\n                    if (\n                        receipt.get("operation") != operation\n                        or receipt.get("request") != canonical_request\n                    ):\n                        raise GatewayContractError(\n                            "experiment id is already bound to another immutable backtest contract"\n                        )\n                    if receipt.get("state") == "FAILED":\n                        raise GatewayContractError(\n                            "the immutable backtest contract previously reached a terminal failure"\n                        )\n                    result = receipt.get("result")\n                    if receipt.get("state") == "SUCCEEDED" and isinstance(result, dict):\n                        return result\n                    if receipt.get("state") not in {"RUNNING", "SUCCEEDED"}:\n                        raise GatewayContractError("stored run receipt has an invalid state")\n                receipts[receipt_key] = {\n                    "operation": operation,\n                    "request": canonical_request,\n                    "state": "RUNNING",\n                    "started_at": _utc_now().isoformat(),\n                }\n                self._write_run_receipts(receipts)\n            finally:\n                registry_unlock(global_fd)\n\n            try:\n                result = _jsonable(runner(request))\n                if not isinstance(result, dict):\n                    raise GatewayContractError("backtest terminal result is invalid")\n            except GatewayContractError:\n                global_fd = registry_lock()\n                try:\n                    receipts = self._load_run_receipts()\n                    receipts[receipt_key] = {\n                        "operation": operation,\n                        "request": canonical_request,\n                        "state": "FAILED",\n                        "failure_code": "CONTRACT_INVALID",\n                        "completed_at": _utc_now().isoformat(),\n                    }\n                    self._write_run_receipts(receipts)\n                finally:\n                    registry_unlock(global_fd)\n                raise\n\n            global_fd = registry_lock()\n            try:\n                receipts = self._load_run_receipts()\n                receipt = receipts.get(receipt_key)\n                if receipt is not None and (\n                    receipt.get("operation") != operation\n                    or receipt.get("request") != canonical_request\n                ):\n                    raise GatewayContractError("stored run receipt identity changed during execution")\n                receipts[receipt_key] = {\n                    "operation": operation,\n                    "request": canonical_request,\n                    "state": "SUCCEEDED",\n                    "result": result,\n                    "completed_at": _utc_now().isoformat(),\n                }\n                self._write_run_receipts(receipts)\n            finally:\n                registry_unlock(global_fd)\n            return result\n        finally:\n            try:\n                fcntl.flock(experiment_lock_fd, fcntl.LOCK_UN)\n            finally:\n                os.close(experiment_lock_fd)\n\n    def run_backtest_idempotent('''
replace_regex(p, pattern, new_idempotent)

# ---------------------------------------------------------------------------
# 6) Forward feedback: evidence must lie inside the real post-accept observation window.
# ---------------------------------------------------------------------------
p = "backend/src/api/domain.py"
replace_once(
    p,
    '''def _validate_complete_feedback(\n    handoff: HandoffOffer, payload: FeedbackInput\n) -> tuple[datetime, datetime, int]:\n''',
    '''def _validate_complete_feedback(\n    handoff: HandoffOffer, payload: FeedbackInput, *, received_at: datetime\n) -> tuple[datetime, datetime, int]:\n''',
)
replace_once(
    p,
    '''    start = payload.observation_start\n    end = payload.observation_end\n    if end <= start:\n        problems.append("observation_end must be after observation_start")\n''',
    '''    start = payload.observation_start\n    end = payload.observation_end\n    if start.tzinfo is None or start.utcoffset() is None:\n        problems.append("observation_start must be timezone-aware")\n    if end.tzinfo is None or end.utcoffset() is None:\n        problems.append("observation_end must be timezone-aware")\n    accepted_at = handoff.accepted_at\n    if accepted_at is None:\n        problems.append("Handoff must be accepted before complete forward feedback")\n    else:\n        if accepted_at.tzinfo is None or accepted_at.utcoffset() is None:\n            accepted_at = accepted_at.replace(tzinfo=UTC)\n        if start.tzinfo is not None and start.utcoffset() is not None and start < accepted_at:\n            problems.append("observation_start cannot predate downstream acceptance")\n    if received_at.tzinfo is None or received_at.utcoffset() is None:\n        raise QfError("FEEDBACK_CONTRACT_INVALID", "Feedback receipt timestamp must be timezone-aware.", 500)\n    if end.tzinfo is not None and end.utcoffset() is not None and end > received_at:\n        problems.append("observation_end cannot be in the future")\n    if (\n        start.tzinfo is not None\n        and start.utcoffset() is not None\n        and end.tzinfo is not None\n        and end.utcoffset() is not None\n        and end <= start\n    ):\n        problems.append("observation_end must be after observation_start")\n''',
)
replace_once(
    p,
    '''            if payload.state == "FEEDBACK_COMPLETE":\n                start, end, sample_size = _validate_complete_feedback(handoff, payload)\n                episode = ForwardEvidenceEpisode(\n''',
    '''            if payload.state == "FEEDBACK_COMPLETE":\n                received_at = _now()\n                start, end, sample_size = _validate_complete_feedback(\n                    handoff, payload, received_at=received_at\n                )\n                episode = ForwardEvidenceEpisode(\n''',
)
replace_once(p, "                    created_at=_now(),\n", "                    created_at=received_at,\n")

# ---------------------------------------------------------------------------
# 7) CI Linux sandbox profile, lint/stale test, and operations documentation.
# ---------------------------------------------------------------------------
p = ".github/workflows/ci.yml"
replace_once(
    p,
    '''      - name: Install OS capability sandbox\n        run: |\n          sudo apt-get update\n          sudo apt-get install -y bubblewrap\n          sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0 || true\n''',
    '''      - name: Install OS capability sandbox\n        run: |\n          sudo apt-get update\n          sudo apt-get install -y bubblewrap apparmor-profiles apparmor-utils\n          if [ -f /usr/share/apparmor/extra-profiles/bwrap-userns-restrict ]; then\n            sudo install -m 0644 /usr/share/apparmor/extra-profiles/bwrap-userns-restrict /etc/apparmor.d/bwrap-userns-restrict\n            sudo apparmor_parser -r /etc/apparmor.d/bwrap-userns-restrict\n          else\n            sudo tee /etc/apparmor.d/bwrap >/dev/null <<'APPARMOR'\n          abi <abi/4.0>,\n          include <tunables/global>\n          profile bwrap /usr/bin/bwrap flags=(unconfined) {\n            userns,\n            include if exists <local/bwrap>\n          }\n          APPARMOR\n            sudo apparmor_parser -r /etc/apparmor.d/bwrap\n          fi\n          bwrap --ro-bind / / --proc /proc --dev /dev --unshare-user --unshare-ipc --unshare-pid --unshare-net -- true\n''',
)

p = "nautilus_runtime/tests/test_gateway_api.py"
text = load(p)
text = text.replace('with pytest.raises(ValidationError, match="attribute \'sys\'"):', 'with pytest.raises(ValidationError):')
save(p, text)

# Keep the ordinary endpoint idempotency test aligned with async sealed-worker finalization.
p = "backend/tests/integration/test_issue22_final_guards.py"
replace_once(
    p,
    "    monkeypatch.setattr(research_runtime, \"simulate_portfolio_candidate\", fake_simulation)\n",
    "    monkeypatch.setattr(research_runtime, \"prepare_portfolio_simulation\", fake_simulation)\n",
)
replace_once(
    p,
    '''        return SimpleNamespace(\n            candidate_id=candidate_id,\n            approval_id=approval_id,\n            simulation_experiment_id=simulation_experiment_id,\n            selected_alpha_id=selected_alpha_id,\n        )\n''',
    '''        return SimpleNamespace(\n            simulation_experiment_id=simulation_experiment_id,\n            selected_alpha_id=selected_alpha_id,\n        )\n''',
)
replace_once(p, "    assert first.status_code == 200, first.text\n", "    assert first.status_code == 202, first.text\n")
replace_once(p, "    assert replay.status_code == 200, replay.text\n", "    assert replay.status_code == 202, replay.text\n")
replace_once(p, "    assert replay.json() == first.json()\n", "    assert replay.json() == first.json()\n    assert first.json()[\"state\"] == \"READY\"\n    assert first.json()[\"job_id\"]\n")
# The fake preparation no longer receives the sealed id; preserve call tuple shape for existing assertions.
replace_once(
    p,
    "        calls.append((portfolio_program_id, tuple(alpha_ids), simulation_experiment_id, portfolio_sealed_experiment_id))\n",
    "        calls.append((portfolio_program_id, tuple(alpha_ids), simulation_experiment_id, portfolio_sealed_experiment_id))\n",
)
# fake signature may accept the old optional arg harmlessly; API calls without it, so index 3 is None now.
replace_once(p, "    assert calls[0][3] is not None\n", "    assert calls[0][3] is None\n")

# Add targeted security/feedback regression tests in a new file.
Path("backend/tests/unit/test_issue22_codex_round9.py").write_text('''from __future__ import annotations\n\nfrom datetime import UTC, datetime, timedelta\nfrom pathlib import Path\nfrom types import SimpleNamespace\n\nimport pytest\n\nfrom api.domain import FeedbackInput, _validate_complete_feedback\nfrom errors import QfError\nfrom quant_runtime.workspace import write_parent_owned_workspace_file\n\n\ndef test_parent_owned_workspace_write_refuses_symlink(tmp_path: Path) -> None:\n    target = tmp_path / "outside.txt"\n    target.write_text("outside", encoding="utf-8")\n    link = tmp_path / "DATASETS.json"\n    link.symlink_to(target)\n    with pytest.raises(QfError) as raised:\n        write_parent_owned_workspace_file(link, "{}")\n    assert raised.value.code == "MISSION_WORKSPACE_PATH_UNSAFE"\n    assert target.read_text(encoding="utf-8") == "outside"\n\n\ndef test_complete_feedback_must_be_post_accept_and_not_future() -> None:\n    accepted = datetime(2026, 8, 29, 1, 0, tzinfo=UTC)\n    handoff = SimpleNamespace(\n        accepted_at=accepted,\n        feedback_contract_snapshot={\n            "minimum_observation_duration_seconds": 60,\n            "minimum_valid_sample_size": 1,\n            "required_fields": ["return"],\n        },\n    )\n    before_accept = FeedbackInput(\n        state="FEEDBACK_COMPLETE",\n        observation_start=accepted - timedelta(minutes=5),\n        observation_end=accepted + timedelta(minutes=5),\n        sample_size=2,\n        evidence={"return": 0.01},\n    )\n    with pytest.raises(QfError):\n        _validate_complete_feedback(\n            handoff, before_accept, received_at=accepted + timedelta(minutes=10)\n        )\n    future = FeedbackInput(\n        state="FEEDBACK_COMPLETE",\n        observation_start=accepted,\n        observation_end=accepted + timedelta(minutes=20),\n        sample_size=2,\n        evidence={"return": 0.01},\n    )\n    with pytest.raises(QfError):\n        _validate_complete_feedback(\n            handoff, future, received_at=accepted + timedelta(minutes=10)\n        )\n''', encoding="utf-8")

# Update the existing end-to-end feedback fixture to represent an actual elapsed forward window.
p = "backend/tests/integration/test_domain_api.py"
needle = '''    start = datetime.now(UTC) - timedelta(minutes=10)\n    feedback = client.post(\n'''
replacement = '''    start = datetime.now(UTC) - timedelta(minutes=10)\n    factory = create_session_factory(engine)\n    with factory() as session, session.begin():\n        persisted_handoff = session.get(HandoffOffer, UUID(handoff["id"]))\n        assert persisted_handoff is not None\n        persisted_handoff.accepted_at = start - timedelta(minutes=1)\n    feedback = client.post(\n'''
replace_once(p, needle, replacement)
# Ensure HandoffOffer is imported in the integration test's db.models tuple.
replace_once(p, "    GovernedDataSource,\n    Job,\n", "    GovernedDataSource,\n    HandoffOffer,\n    Job,\n")

p = "OPERATIONS.md"
text = load(p)
if "### SOURCE_BUNDLE OS sandbox prerequisite" not in text:
    text += '''\n\n### SOURCE_BUNDLE OS sandbox prerequisite\n\nRemote Nautilus Gateway hosts that execute `SOURCE_BUNDLE` artifacts must be Linux hosts with\nBubblewrap available. On Ubuntu 24.04, keep the system-wide AppArmor unprivileged-user-namespace\nrestriction enabled and load the `bwrap-userns-restrict` AppArmor profile (from\n`apparmor-profiles`) before starting the Gateway. QuaZonai fails closed when the OS sandbox cannot\ncreate its isolated user/network/pid namespaces; do not work around this by granting the Gateway\nDocker socket access or by exposing sealed catalogs to the ordinary research runtime.\n'''
    save(p, text)

print("round9 patch applied")
