from __future__ import annotations

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"expected one match in {path}, found {count}: {old[:100]!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


# ---------------------------------------------------------------------------
# 1. SOURCE_BUNDLE execution: Linux kernel namespace/mount isolation via bwrap.
# ---------------------------------------------------------------------------
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/models.py",
    '    "__subclasses__",\n}\n',
    '    "__subclasses__",\n'
    '    "environ",\n'
    '    "fork",\n'
    '    "modules",\n'
    '    "popen",\n'
    '    "socket",\n'
    '    "spawn",\n'
    '    "sys",\n'
    '    "system",\n'
    '}\n',
)

replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    'def _candidate_strategy_wheel_path(candidate_id: UUID) -> str:\n'
    '    return (\n'
    '        "strategy/quazonai_candidate_strategy-"\n'
    '        f"0.0.{candidate_id.int}-py3-none-any.whl"\n'
    '    )\n\n\nclass NautilusGatewayEngine:\n',
    'def _candidate_strategy_wheel_path(candidate_id: UUID) -> str:\n'
    '    return (\n'
    '        "strategy/quazonai_candidate_strategy-"\n'
    '        f"0.0.{candidate_id.int}-py3-none-any.whl"\n'
    '    )\n\n\n'
    'def _source_bundle_sandbox_command(\n'
    '    *, operation: str, workspace: Path\n'
    ') -> list[str]:\n'
    '    """Build a fail-closed Bubblewrap command for authored Python.\n\n'
    '    The child receives only the one disposable operation workspace, trusted\n'
    '    Python/runtime libraries, an empty network namespace and fresh /proc,/dev,/tmp.\n'
    '    It cannot see the Gateway data root, sibling sealed catalogs, service secrets,\n'
    '    host home directories, or host network namespace.\n'
    '    """\n'
    '    if sys.platform != "linux":\n'
    '        raise GatewayContractError("SOURCE_BUNDLE execution requires Linux OS isolation")\n'
    '    configured = os.getenv("NAUTILUS_GATEWAY_OS_SANDBOX", "bwrap").strip() or "bwrap"\n'
    '    sandbox = shutil.which(configured)\n'
    '    if sandbox is None:\n'
    '        raise GatewayContractError("SOURCE_BUNDLE OS sandbox is unavailable")\n'
    '    command = [\n'
    '        sandbox,\n'
    '        "--die-with-parent",\n'
    '        "--new-session",\n'
    '        "--unshare-all",\n'
    '        "--proc", "/proc",\n'
    '        "--dev", "/dev",\n'
    '        "--tmpfs", "/tmp",\n'
    '        "--bind", str(workspace), "/sandbox",\n'
    '        "--chdir", "/sandbox",\n'
    '    ]\n'
    '    seen: set[str] = set()\n'
    '    for candidate in (\n'
    '        Path("/usr"), Path("/lib"), Path("/lib64"), Path("/etc"),\n'
    '        Path(sys.base_prefix), Path(sys.prefix),\n'
    '    ):\n'
    '        resolved = str(candidate.resolve())\n'
    '        if candidate.exists() and resolved not in seen:\n'
    '            command.extend(["--ro-bind", resolved, resolved])\n'
    '            seen.add(resolved)\n'
    '    gateway_source = Path(__file__).resolve().parents[2]\n'
    '    command.extend(["--ro-bind", str(gateway_source), "/gateway-src"])\n'
    '    command.extend([\n'
    '        "--setenv", "QUAZONAI_NAUTILUS_ISOLATED_CHILD", "1",\n'
    '        "--setenv", "PYTHONDONTWRITEBYTECODE", "1",\n'
    '        sys.executable, "-I", "-c",\n'
    '        (\n'
    '            "import sys; sys.path.insert(0, \'/gateway-src\'); "\n'
    '            "from quazonai_nautilus_gateway.isolated_runner import main; main()"\n'
    '        ),\n'
    '        operation, "/sandbox/runtime", "/sandbox/input.json",\n'
    '    ])\n'
    '    return command\n\n\nclass NautilusGatewayEngine:\n',
)

replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '            completed = subprocess.run(\n'
    '                [\n'
    '                    sys.executable,\n'
    '                    "-I",\n'
    '                    "-m",\n'
    '                    "quazonai_nautilus_gateway.isolated_runner",\n'
    '                    operation,\n'
    '                    str(child_root),\n'
    '                    str(input_path),\n'
    '                ],\n',
    '            completed = subprocess.run(\n'
    '                _source_bundle_sandbox_command(operation=operation, workspace=workspace),\n',
)

# ---------------------------------------------------------------------------
# 2. Rich catalog validation metadata + sealed-only registration route.
# ---------------------------------------------------------------------------
replace_once(
    "backend/src/quant_runtime/contracts.py",
    'class CatalogValidationResult(StrictModel):\n'
    '    protocol_version: str\n'
    '    runtime_version: str\n'
    '    catalog_key: str\n'
    '    valid: bool\n'
    '    instrument_scope: list[str]\n'
    '    row_count: int = Field(ge=0)\n'
    '    event_time_start: datetime | None = None\n'
    '    event_time_end: datetime | None = None\n'
    '    available_time_start: datetime | None = None\n'
    '    available_time_end: datetime | None = None\n'
    '    findings: list[dict[str, Any]] = Field(default_factory=list)\n',
    'class CatalogValidationResult(StrictModel):\n'
    '    protocol_version: str\n'
    '    runtime_version: str\n'
    '    catalog_key: str\n'
    '    catalog_uri: str | None = None\n'
    '    valid: bool\n'
    '    nautilus_data_type: str | None = None\n'
    '    instrument_scope: list[str]\n'
    '    row_count: int = Field(ge=0)\n'
    '    event_time_start: datetime | None = None\n'
    '    event_time_end: datetime | None = None\n'
    '    available_time_start: datetime | None = None\n'
    '    available_time_end: datetime | None = None\n'
    '    schema_revision: str | None = None\n'
    '    quality_result: dict[str, Any] = Field(default_factory=dict)\n'
    '    point_in_time_result: dict[str, Any] = Field(default_factory=dict)\n'
    '    ingested_at: datetime | None = None\n'
    '    findings: list[dict[str, Any]] = Field(default_factory=list)\n',
)

replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '        return {\n'
    '            "protocol_version": PROTOCOL_VERSION,\n'
    '            "runtime_version": nautilus_version,\n'
    '            "catalog_key": request.catalog_key,\n'
    '            "valid": not findings and bool(instruments) and bool(ticks),\n'
    '            "instrument_scope": scope,\n',
    '        return {\n'
    '            "protocol_version": PROTOCOL_VERSION,\n'
    '            "runtime_version": nautilus_version,\n'
    '            "catalog_key": request.catalog_key,\n'
    '            "catalog_uri": f"nautilus-catalog://{request.catalog_key}",\n'
    '            "valid": not findings and bool(instruments) and bool(ticks),\n'
    '            "nautilus_data_type": manifest.get("nautilus_data_type"),\n'
    '            "instrument_scope": scope,\n',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '            "available_time_end": actual_available_end,\n'
    '            "findings": findings,\n'
    '        }\n\n    @contextmanager\n',
    '            "available_time_end": actual_available_end,\n'
    '            "schema_revision": manifest.get("schema_revision"),\n'
    '            "quality_result": manifest.get("quality_result", {}),\n'
    '            "point_in_time_result": manifest.get("point_in_time_result", {}),\n'
    '            "ingested_at": _parse_time(manifest["ingested_at"]),\n'
    '            "findings": findings,\n'
    '        }\n\n    @contextmanager\n',
)

replace_once(
    "backend/src/quant_runtime/client.py",
    '    def validate_catalog(self, request: CatalogValidationRequest) -> CatalogValidationResult: ...\n\n'
    '    def run_backtest',
    '    def validate_catalog(self, request: CatalogValidationRequest) -> CatalogValidationResult: ...\n\n'
    '    def validate_sealed_catalog(\n'
    '        self, request: CatalogValidationRequest\n'
    '    ) -> CatalogValidationResult: ...\n\n'
    '    def run_backtest',
)
replace_once(
    "backend/src/quant_runtime/client.py",
    '    def run_backtest(self, request: BacktestExperimentRequest) -> BacktestEvidence:\n',
    '    def validate_sealed_catalog(\n'
    '        self, request: CatalogValidationRequest\n'
    '    ) -> CatalogValidationResult:\n'
    '        response = self._client.post(\n'
    '            "v1/sealed-catalogs/validate",\n'
    '            json=request.model_dump(mode="json"),\n'
    '            headers={"Idempotency-Key": str(request.request_id)},\n'
    '        )\n'
    '        return self._parse(response, CatalogValidationResult)\n\n'
    '    def run_backtest(self, request: BacktestExperimentRequest) -> BacktestEvidence:\n',
)

replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/app.py",
    '    @app.post("/v1/backtests", dependencies=[Depends(_authorize)])\n',
    '    @app.post("/v1/sealed-catalogs/validate", dependencies=[Depends(_authorize)])\n'
    '    def validate_sealed_catalog(request: CatalogValidationRequest) -> dict[str, Any]:\n'
    '        _require_role(gateway_role, "SEALED")\n'
    '        return engine().validate_catalog(request)\n\n'
    '    @app.post("/v1/backtests", dependencies=[Depends(_authorize)])\n',
)

# Local-only sealed host provisioning command: raw rows never traverse Core/API/DB.
Path("nautilus_runtime/src/quazonai_nautilus_gateway/provision.py").write_text(
    '''"""Local sealed-host catalog provisioning; never exposed over HTTP."""\n\n'
    'from __future__ import annotations\n\n'
    'import argparse\nimport json\nimport os\nfrom pathlib import Path\nfrom typing import Sequence\n\n'
    'from quazonai_nautilus_gateway.engine import NautilusGatewayEngine\n'
    'from quazonai_nautilus_gateway.models import CatalogIngestRequest\n\n'
    'def main(argv: Sequence[str] | None = None) -> int:\n'
    '    parser = argparse.ArgumentParser(description="Provision a sealed catalog locally on the remote Gateway host")\n'
    '    parser.add_argument("--input", type=Path, required=True)\n'
    '    parser.add_argument("--data-root", type=Path, default=None)\n'
    '    args = parser.parse_args(argv)\n'
    '    role = os.getenv("NAUTILUS_GATEWAY_ROLE", "").strip().upper()\n'
    '    if role != "SEALED":\n'
    '        raise SystemExit("NAUTILUS_GATEWAY_ROLE=SEALED is required")\n'
    '    root = args.data_root or Path(os.getenv("NAUTILUS_GATEWAY_DATA_ROOT", "/tmp/quazonai-nautilus"))\n'
    '    request = CatalogIngestRequest.model_validate(json.loads(args.input.read_text(encoding="utf-8")))\n'
    '    result = NautilusGatewayEngine(root).ingest(request)\n'
    '    print(json.dumps({"catalog_key": result["catalog_key"], "catalog_uri": result["catalog_uri"]}))\n'
    '    return 0\n\n'
    'if __name__ == "__main__":\n    raise SystemExit(main())\n'''.replace("'\n    '", ""),
    encoding="utf-8",
)
replace_once(
    "nautilus_runtime/pyproject.toml",
    'quazonai-nautilus-gateway = "quazonai_nautilus_gateway.app:run"\n',
    'quazonai-nautilus-gateway = "quazonai_nautilus_gateway.app:run"\n'
    'quazonai-nautilus-sealed-provision = "quazonai_nautilus_gateway.provision:main"\n',
)

# ---------------------------------------------------------------------------
# 3. Durable sealed Alpha experiment id + sealed Dataset registration jobs.
# ---------------------------------------------------------------------------
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '    sealed_dataset_revision_id: UUID,\n'
    '    name: str | None = None,\n',
    '    sealed_dataset_revision_id: UUID,\n'
    '    sealed_experiment_id: UUID,\n'
    '    name: str | None = None,\n',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '                "experiment_id": uuid4(),\n'
    '                "mode": ExperimentMode.SEALED,\n',
    '                "experiment_id": sealed_experiment_id,\n'
    '                "mode": ExperimentMode.SEALED,\n',
)

replace_once(
    "backend/src/api/research_runtime.py",
    'from db.models import DatasetRevision, Job, PublicMutationReceipt, SearchLedgerEntry\n',
    'from db.models import (\n'
    '    DatasetRevision,\n'
    '    GovernedDataSource,\n'
    '    Job,\n'
    '    MarketUniverseVersion,\n'
    '    PublicMutationReceipt,\n'
    '    SearchLedgerEntry,\n'
    ')\n',
)
replace_once(
    "backend/src/api/research_runtime.py",
    'SEALED_JOB_KIND = "SEALED_ALPHA_QUALIFICATION"\n',
    'SEALED_JOB_KIND = "SEALED_ALPHA_QUALIFICATION"\n'
    'SEALED_DATASET_REGISTRATION_JOB_KIND = "SEALED_DATASET_REGISTRATION"\n',
)
replace_once(
    "backend/src/api/research_runtime.py",
    'class PortfolioSimulationInput(StrictModel):\n',
    'class SealedDatasetRegistrationInput(StrictModel):\n'
    '    data_source_id: UUID\n'
    '    catalog_key: str = Field(min_length=1, max_length=128, pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$")\n'
    '    source_license: str | None = Field(default=None, max_length=500)\n'
    '    expected_instrument_ids: list[str] = Field(default_factory=list)\n\n'
    'class SealedDatasetRegistrationJobResult(StrictModel):\n'
    '    job_id: UUID\n'
    '    universe_version_id: UUID\n'
    '    state: str\n\n\n'
    'class PortfolioSimulationInput(StrictModel):\n',
)

replace_once(
    "backend/src/api/research_runtime.py",
    '        job = enqueue_job(\n'
    '            session,\n'
    '            kind=SEALED_JOB_KIND,\n'
    '            resource_type="SEARCH_LEDGER_ENTRY",\n'
    '            resource_id=source_experiment_id,\n'
    '            payload=requested,\n'
    '        )\n',
    '        job = enqueue_job(\n'
    '            session,\n'
    '            kind=SEALED_JOB_KIND,\n'
    '            resource_type="SEARCH_LEDGER_ENTRY",\n'
    '            resource_id=source_experiment_id,\n'
    '            payload={**requested, "sealed_experiment_id": str(uuid4())},\n'
    '        )\n',
)

registration_endpoint = '''\n\n@router.post(\n    "/market-universe-versions/{universe_version_id}/sealed-dataset-revisions/register",\n    response_model=SealedDatasetRegistrationJobResult,\n    status_code=202,\n)\ndef register_sealed_dataset(\n    universe_version_id: UUID,\n    payload: SealedDatasetRegistrationInput,\n    request: Request,\n    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),\n) -> SealedDatasetRegistrationJobResult:\n    """Queue metadata-only registration in the credential-isolated sealed worker."""\n    key = (idempotency_key or "").strip()\n    if not key or len(key) > 200:\n        raise QfError(\n            "IDEMPOTENCY_KEY_REQUIRED",\n            "Sealed Dataset registration requires a 1..200 character Idempotency-Key.",\n            422,\n        )\n    expected = list(dict.fromkeys(item.strip() for item in payload.expected_instrument_ids if item.strip()))\n    normalized = {\n        "data_source_id": str(payload.data_source_id),\n        "catalog_key": payload.catalog_key,\n        "source_license": payload.source_license,\n        "expected_instrument_ids": expected,\n        "idempotency_key": key,\n    }\n    factory = request.app.state.session_factory\n    with factory() as session, session.begin():\n        universe = session.execute(\n            select(MarketUniverseVersion)\n            .where(MarketUniverseVersion.id == universe_version_id)\n            .with_for_update()\n        ).scalar_one_or_none()\n        source = session.execute(\n            select(GovernedDataSource)\n            .where(GovernedDataSource.id == payload.data_source_id)\n            .with_for_update()\n        ).scalar_one_or_none()\n        if universe is None or universe.state != "ACTIVE":\n            raise QfError("UNIVERSE_VERSION_NOT_ACTIVE", "Sealed registration requires an active Universe Version.", 409)\n        if source is None or source.state != "ACTIVE" or source.preflight_state != "READY":\n            raise QfError("DATA_SOURCE_NOT_READY", "Sealed registration requires an active ready Data Source.", 409)\n        jobs = list(session.scalars(\n            select(Job)\n            .where(\n                Job.kind == SEALED_DATASET_REGISTRATION_JOB_KIND,\n                Job.resource_id == universe_version_id,\n                Job.state.in_(["READY", "LEASED", "SUCCEEDED"]),\n            )\n            .order_by(Job.created_at.desc())\n        ))\n        for existing in jobs:\n            existing_payload = dict(existing.payload or {})\n            if existing_payload.get("idempotency_key") != key:\n                continue\n            if any(existing_payload.get(name) != value for name, value in normalized.items()):\n                raise QfError("IDEMPOTENCY_KEY_REUSED", "The sealed registration key belongs to another request.", 409)\n            return SealedDatasetRegistrationJobResult(\n                job_id=existing.id, universe_version_id=universe_version_id, state=existing.state\n            )\n        job = enqueue_job(\n            session,\n            kind=SEALED_DATASET_REGISTRATION_JOB_KIND,\n            resource_type="MARKET_UNIVERSE_VERSION",\n            resource_id=universe_version_id,\n            payload=normalized,\n        )\n        return SealedDatasetRegistrationJobResult(\n            job_id=job.id, universe_version_id=universe_version_id, state=job.state\n        )\n'''
replace_once(
    "backend/src/api/research_runtime.py",
    '\n\n@router.post(\n    "/portfolio-programs/{portfolio_program_id}/simulate-candidate",\n',
    registration_endpoint + '\n\n@router.post(\n    "/portfolio-programs/{portfolio_program_id}/simulate-candidate",\n',
)

# Durable portfolio simulation + portfolio-sealed experiment IDs in the same receipt.
replace_once(
    "backend/src/api/research_runtime.py",
    'def _pending_simulation_experiment_id(receipt: PublicMutationReceipt) -> UUID:\n'
    '    try:\n'
    '        return UUID(str((receipt.response_json or {})["simulation_experiment_id"]))\n'
    '    except (KeyError, TypeError, ValueError) as exc:\n'
    '        raise QfError(\n'
    '            "IDEMPOTENCY_RECEIPT_INVALID",\n'
    '            "Candidate simulation idempotency receipt is missing its experiment identity.",\n'
    '            500,\n'
    '        ) from exc\n',
    'def _pending_simulation_experiment_ids(\n'
    '    receipt: PublicMutationReceipt, *, require_portfolio_sealed: bool\n'
    ') -> tuple[UUID, UUID | None]:\n'
    '    try:\n'
    '        simulation_id = UUID(str((receipt.response_json or {})["simulation_experiment_id"]))\n'
    '        raw_sealed = (receipt.response_json or {}).get("portfolio_sealed_experiment_id")\n'
    '        sealed_id = UUID(str(raw_sealed)) if raw_sealed is not None else None\n'
    '    except (KeyError, TypeError, ValueError) as exc:\n'
    '        raise QfError(\n'
    '            "IDEMPOTENCY_RECEIPT_INVALID",\n'
    '            "Candidate simulation receipt is missing a durable experiment identity.",\n'
    '            500,\n'
    '        ) from exc\n'
    '    if require_portfolio_sealed and sealed_id is None:\n'
    '        raise QfError(\n'
    '            "IDEMPOTENCY_RECEIPT_INVALID",\n'
    '            "Candidate simulation receipt is missing its portfolio sealed identity.",\n'
    '            500,\n'
    '        )\n'
    '    return simulation_id, sealed_id\n',
)
replace_once(
    "backend/src/api/research_runtime.py",
    ') -> tuple[PublicMutationReceipt, bool, UUID]:\n',
    ') -> tuple[PublicMutationReceipt, bool, UUID, UUID | None]:\n',
)
replace_once(
    "backend/src/api/research_runtime.py",
    '        experiment_id = uuid4()\n'
    '        now = datetime.now(UTC)\n',
    '        experiment_id = uuid4()\n'
    '        portfolio_sealed_experiment_id = uuid4()\n'
    '        now = datetime.now(UTC)\n',
)
replace_once(
    "backend/src/api/research_runtime.py",
    '                "simulation_experiment_id": str(experiment_id),\n'
    '                "attempt_started_at": now.isoformat(),\n',
    '                "simulation_experiment_id": str(experiment_id),\n'
    '                "portfolio_sealed_experiment_id": str(portfolio_sealed_experiment_id),\n'
    '                "attempt_started_at": now.isoformat(),\n',
)
replace_once(
    "backend/src/api/research_runtime.py",
    '            return receipt, True, experiment_id\n\n'
    '    if not _simulation_receipt_matches',
    '            return receipt, True, experiment_id, portfolio_sealed_experiment_id\n\n'
    '    if not _simulation_receipt_matches',
)
replace_once(
    "backend/src/api/research_runtime.py",
    '    experiment_id = _pending_simulation_experiment_id(existing)\n'
    '    if existing.status_code == 200:\n'
    '        return existing, False, experiment_id\n',
    '    experiment_id, portfolio_sealed_experiment_id = _pending_simulation_experiment_ids(\n'
    '        existing, require_portfolio_sealed=existing.status_code != 200\n'
    '    )\n'
    '    if existing.status_code == 200:\n'
    '        return existing, False, experiment_id, None\n',
)
replace_once(
    "backend/src/api/research_runtime.py",
    '        "simulation_experiment_id": str(experiment_id),\n'
    '        "attempt_started_at": now.isoformat(),\n'
    '    }\n'
    '    existing.status_code = _SIMULATION_PENDING_STATUS\n'
    '    session.flush()\n'
    '    return existing, True, experiment_id\n',
    '        "simulation_experiment_id": str(experiment_id),\n'
    '        "portfolio_sealed_experiment_id": str(portfolio_sealed_experiment_id),\n'
    '        "attempt_started_at": now.isoformat(),\n'
    '    }\n'
    '    existing.status_code = _SIMULATION_PENDING_STATUS\n'
    '    session.flush()\n'
    '    return existing, True, experiment_id, portfolio_sealed_experiment_id\n',
)
replace_once(
    "backend/src/api/research_runtime.py",
    '        if _pending_simulation_experiment_id(receipt) != experiment_id:\n',
    '        receipt_simulation_id, _ = _pending_simulation_experiment_ids(\n'
    '            receipt, require_portfolio_sealed=True\n'
    '        )\n'
    '        if receipt_simulation_id != experiment_id:\n',
)
replace_once(
    "backend/src/api/research_runtime.py",
    '        receipt, claimed, experiment_id = _claim_simulation_receipt(\n',
    '        receipt, claimed, experiment_id, portfolio_sealed_experiment_id = _claim_simulation_receipt(\n',
)
replace_once(
    "backend/src/api/research_runtime.py",
    '        result = simulate_portfolio_candidate(\n'
    '            factory,\n'
    '            portfolio_program_id=portfolio_program_id,\n'
    '            alpha_ids=payload.alpha_ids,\n'
    '            simulation_experiment_id=experiment_id,\n'
    '        )\n',
    '        if portfolio_sealed_experiment_id is None:\n'
    '            raise QfError("IDEMPOTENCY_RECEIPT_INVALID", "Missing portfolio sealed experiment id.", 500)\n'
    '        result = simulate_portfolio_candidate(\n'
    '            factory,\n'
    '            portfolio_program_id=portfolio_program_id,\n'
    '            alpha_ids=payload.alpha_ids,\n'
    '            simulation_experiment_id=experiment_id,\n'
    '            portfolio_sealed_experiment_id=portfolio_sealed_experiment_id,\n'
    '        )\n',
)

# Sealed worker dispatch + registration persistence.
replace_once(
    "backend/src/runners/sealed_worker.py",
    'from uuid import UUID\n\nfrom db.models import Job\n',
    'from uuid import UUID\n\nfrom sqlalchemy import func, select\n\n'
    'from db.models import DatasetRevision, GovernedDataSource, Job, MarketUniverseVersion\n',
)
replace_once(
    "backend/src/runners/sealed_worker.py",
    'from quant_runtime.client import RemoteNautilusConfig\n',
    'from quant_runtime.client import NautilusQuantRuntime, RemoteNautilusConfig\n'
    'from quant_runtime.contracts import CatalogValidationRequest\n',
)
replace_once(
    "backend/src/runners/sealed_worker.py",
    'SEALED_JOB_KIND = "SEALED_ALPHA_QUALIFICATION"\n',
    'SEALED_JOB_KIND = "SEALED_ALPHA_QUALIFICATION"\n'
    'SEALED_DATASET_REGISTRATION_JOB_KIND = "SEALED_DATASET_REGISTRATION"\n'
    'SEALED_JOB_KINDS = {SEALED_JOB_KIND, SEALED_DATASET_REGISTRATION_JOB_KIND}\n',
)
replace_once(
    "backend/src/runners/sealed_worker.py",
    '    sealed_dataset_revision_id = _uuid_payload(job, "sealed_dataset_revision_id")\n',
    '    sealed_dataset_revision_id = _uuid_payload(job, "sealed_dataset_revision_id")\n'
    '    sealed_experiment_id = _uuid_payload(job, "sealed_experiment_id")\n',
)
replace_once(
    "backend/src/runners/sealed_worker.py",
    '        sealed_dataset_revision_id=sealed_dataset_revision_id,\n'
    '        name=name,\n',
    '        sealed_dataset_revision_id=sealed_dataset_revision_id,\n'
    '        sealed_experiment_id=sealed_experiment_id,\n'
    '        name=name,\n',
)

registration_worker = '''\n\ndef _execute_sealed_dataset_registration(\n    factory: SessionFactory, job: Job\n) -> dict[str, str]:\n    payload = dict(job.payload or {})\n    universe_version_id = job.resource_id\n    data_source_id = _uuid_payload(job, "data_source_id")\n    catalog_key = str(payload.get("catalog_key") or "")\n    expected = [str(value) for value in payload.get("expected_instrument_ids", [])]\n    if not catalog_key:\n        raise RuntimeError("sealed registration catalog_key is required")\n    with NautilusQuantRuntime(RemoteNautilusConfig.from_env(sealed=True)) as runtime:\n        validation = runtime.validate_sealed_catalog(\n            CatalogValidationRequest(\n                catalog_key=catalog_key,\n                instrument_ids=expected,\n                nautilus_data_type="QuoteTick",\n            )\n        )\n    if not validation.valid:\n        raise RuntimeError("sealed catalog failed remote validation")\n    required = (\n        validation.catalog_uri, validation.nautilus_data_type, validation.schema_revision,\n        validation.event_time_start, validation.event_time_end,\n        validation.available_time_start, validation.available_time_end, validation.ingested_at,\n    )\n    if any(value is None for value in required):\n        raise RuntimeError("sealed catalog validation omitted immutable governance metadata")\n    if str(validation.quality_result.get("state", "")).upper() != "VALID":\n        raise RuntimeError("sealed catalog quality governance is not VALID")\n    if str(validation.point_in_time_result.get("state", "")).upper() != "VALID":\n        raise RuntimeError("sealed catalog point-in-time governance is not VALID")\n\n    with factory.begin() as session:\n        universe = session.execute(\n            select(MarketUniverseVersion)\n            .where(MarketUniverseVersion.id == universe_version_id)\n            .with_for_update()\n        ).scalar_one_or_none()\n        source = session.execute(\n            select(GovernedDataSource)\n            .where(GovernedDataSource.id == data_source_id)\n            .with_for_update()\n        ).scalar_one_or_none()\n        if universe is None or universe.state != "ACTIVE":\n            raise RuntimeError("sealed registration Universe Version is no longer active")\n        if source is None or source.state != "ACTIVE" or source.preflight_state != "READY":\n            raise RuntimeError("sealed registration Data Source is no longer ready")\n        existing = session.scalar(\n            select(DatasetRevision).where(DatasetRevision.catalog_uri == validation.catalog_uri)\n        )\n        if existing is not None:\n            if (\n                existing.partition != "SEALED"\n                or existing.universe_version_id != universe.id\n                or existing.data_source_id != source.id\n                or existing.instrument_scope != validation.instrument_scope\n                or existing.row_count != validation.row_count\n            ):\n                raise RuntimeError("sealed catalog URI is already bound to different governance facts")\n            revision = existing\n        else:\n            revision_no = int(\n                session.scalar(\n                    select(func.max(DatasetRevision.revision_no)).where(\n                        DatasetRevision.data_source_id == source.id,\n                        DatasetRevision.universe_version_id == universe.id,\n                        DatasetRevision.partition == "SEALED",\n                    )\n                )\n                or 0\n            ) + 1\n            revision = DatasetRevision(\n                data_source_id=source.id,\n                universe_version_id=universe.id,\n                universe_name=universe.name,\n                revision_no=revision_no,\n                schema_version=validation.schema_revision,\n                event_start=validation.event_time_start,\n                event_end=validation.event_time_end,\n                available_start=validation.available_time_start,\n                available_end=validation.available_time_end,\n                row_count=validation.row_count,\n                quality_state="VALID",\n                point_in_time_state="VALID",\n                partition="SEALED",\n                provider_name=source.provider or source.name,\n                source_license=(str(payload.get("source_license")) if payload.get("source_license") else None),\n                catalog_uri=validation.catalog_uri,\n                nautilus_data_type=validation.nautilus_data_type,\n                instrument_scope=validation.instrument_scope,\n                schema_revision=validation.schema_revision,\n                quality_result=validation.quality_result,\n                point_in_time_result=validation.point_in_time_result,\n                ingested_at=validation.ingested_at,\n                created_at=validation.ingested_at,\n            )\n            session.add(revision)\n            session.flush()\n            append_event(\n                session,\n                kind="SEALED_DATASET_REVISION_REGISTERED",\n                aggregate_type="dataset_revision",\n                aggregate_id=revision.id,\n                payload={\n                    "universe_version_id": str(universe.id),\n                    "data_source_id": str(source.id),\n                    "catalog_key": catalog_key,\n                },\n            )\n        return {\n            "dataset_revision_id": str(revision.id),\n            "universe_version_id": str(universe.id),\n            "state": "REGISTERED",\n        }\n'''
replace_once(
    "backend/src/runners/sealed_worker.py",
    '\n\n@contextmanager\ndef _lease_heartbeat',
    registration_worker + '\n\n@contextmanager\ndef _lease_heartbeat',
)
replace_once(
    "backend/src/runners/sealed_worker.py",
    '            kinds={SEALED_JOB_KIND},\n',
    '            kinds=SEALED_JOB_KINDS,\n',
)
replace_once(
    "backend/src/runners/sealed_worker.py",
    '            result = _execute_qualification(factory, job)\n',
    '            if job.kind == SEALED_JOB_KIND:\n'
    '                result = _execute_qualification(factory, job)\n'
    '            elif job.kind == SEALED_DATASET_REGISTRATION_JOB_KIND:\n'
    '                result = _execute_sealed_dataset_registration(factory, job)\n'
    '            else:\n'
    '                raise RuntimeError(f"unsupported sealed job kind: {job.kind}")\n',
)

# ---------------------------------------------------------------------------
# 4. Independent portfolio sealed pass + one actionable pending approval.
# ---------------------------------------------------------------------------
portfolio_helpers = '''\n\ndef _pending_program_approval_id(session: Session, program_id: UUID) -> UUID | None:\n    return session.scalar(\n        select(ApprovalSnapshot.id)\n        .join(PortfolioCandidate, ApprovalSnapshot.candidate_id == PortfolioCandidate.id)\n        .where(\n            PortfolioCandidate.portfolio_program_id == program_id,\n            ApprovalSnapshot.state == "PENDING",\n        )\n        .order_by(ApprovalSnapshot.created_at)\n        .limit(1)\n    )\n\n\ndef _select_portfolio_sealed_dataset(\n    session: Session,\n    *,\n    alpha: AlphaQualification,\n    source_dataset: DatasetRevision,\n    source_request: BacktestExperimentRequest,\n    program_id: UUID,\n) -> DatasetRevision:\n    qualification = (alpha.metrics or {}).get("qualification_contract", {})\n    try:\n        qualification_sealed_id = UUID(str(qualification["sealed_dataset_revision_id"]))\n    except (KeyError, TypeError, ValueError) as exc:\n        raise QfError(\n            "ALPHA_SEALED_LINEAGE_MISSING",\n            "Qualified Alpha lost its sealed Dataset lineage.",\n            500,\n        ) from exc\n    qualification_sealed = session.get(DatasetRevision, qualification_sealed_id)\n    if qualification_sealed is None:\n        raise QfError("DATASET_REVISION_NOT_FOUND", "Alpha sealed Dataset Revision is missing.", 500)\n    lineage_ids = _evidence_program_lineage_ids(session, program_id)\n    consumed = set(\n        session.scalars(\n            select(SearchLedgerEntry.dataset_revision_id).where(\n                SearchLedgerEntry.program_id.in_(lineage_ids),\n                SearchLedgerEntry.mode == ExperimentMode.SEALED.value,\n                SearchLedgerEntry.state.in_(["RUNNING", "SUCCEEDED"]),\n            )\n        )\n    )\n    candidates = list(\n        session.scalars(\n            select(DatasetRevision)\n            .where(\n                DatasetRevision.partition == "SEALED",\n                DatasetRevision.quality_state == "VALID",\n                DatasetRevision.point_in_time_state == "VALID",\n                DatasetRevision.universe_version_id == source_dataset.universe_version_id,\n            )\n            .order_by(DatasetRevision.event_start, DatasetRevision.id)\n        )\n    )\n    q_start, q_end = _sealed_dataset_bounds(qualification_sealed)\n    for candidate in candidates:\n        if candidate.id in consumed or candidate.id == qualification_sealed_id:\n            continue\n        try:\n            _select_sealed_dataset(session, source_dataset, candidate.id)\n            c_start, c_end = _sealed_dataset_bounds(candidate)\n        except QfError:\n            continue\n        if q_start <= c_end and c_start <= q_end:\n            continue\n        if not set(source_request.instrument_ids).issubset(set(candidate.instrument_scope or [])):\n            continue\n        return candidate\n    raise QfError(\n        "PORTFOLIO_SEALED_DATASET_UNAVAILABLE",\n        "Candidate promotion requires a second independent governed SEALED episode.",\n        422,\n    )\n'''
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '\n\n@dataclass(frozen=True, slots=True)\nclass CandidatePromotion:',
    portfolio_helpers + '\n\n@dataclass(frozen=True, slots=True)\nclass CandidatePromotion:',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '    alpha_ids: list[UUID],\n'
    '    simulation_experiment_id: UUID | None = None,\n',
    '    alpha_ids: list[UUID],\n'
    '    simulation_experiment_id: UUID,\n'
    '    portfolio_sealed_experiment_id: UUID,\n',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '        experiment_id = simulation_experiment_id or uuid4()\n'
    '        if simulation_experiment_id is not None:\n',
    '        experiment_id = simulation_experiment_id\n'
    '        if simulation_experiment_id is not None:\n',
)
# Before leaving the first read transaction, bind the second independent sealed episode.
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '        mandate_constraints = _validate_mandate_before_simulation(mandate, alpha)\n'
    '        mandate_id = mandate.id\n',
    '        mandate_constraints = _validate_mandate_before_simulation(mandate, alpha)\n'
    '        source_dataset = session.get(DatasetRevision, source.dataset_revision_id)\n'
    '        if source_dataset is None:\n'
    '            raise QfError("DATASET_REVISION_NOT_FOUND", "Portfolio source Dataset Revision is missing.", 500)\n'
    '        portfolio_sealed_dataset = _select_portfolio_sealed_dataset(\n'
    '            session,\n'
    '            alpha=alpha,\n'
    '            source_dataset=source_dataset,\n'
    '            source_request=request,\n'
    '            program_id=source.program_id,\n'
    '        )\n'
    '        portfolio_sealed_start, portfolio_sealed_end = _sealed_dataset_bounds(portfolio_sealed_dataset)\n'
    '        portfolio_sealed_dataset_id = portfolio_sealed_dataset.id\n'
    '        portfolio_sealed_catalog_key = _catalog_key(portfolio_sealed_dataset)\n'
    '        mandate_id = mandate.id\n',
)
# Run independent portfolio sealed disclosure after public transaction-level simulation.
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '    simulation_evidence = _require_real_transaction_evidence(simulation)\n'
    '    _validate_mandate_after_simulation(mandate_constraints, simulation_evidence)\n\n'
    '    with factory() as session, session.begin():\n',
    '    simulation_evidence = _require_real_transaction_evidence(simulation)\n'
    '    _validate_mandate_after_simulation(mandate_constraints, simulation_evidence)\n'
    '    portfolio_sealed_request = simulation_request.model_copy(\n'
    '        update={\n'
    '            "experiment_id": portfolio_sealed_experiment_id,\n'
    '            "mode": ExperimentMode.SEALED,\n'
    '            "dataset_revision_id": portfolio_sealed_dataset_id,\n'
    '            "catalog_key": portfolio_sealed_catalog_key,\n'
    '            "start_time": portfolio_sealed_start,\n'
    '            "end_time": portfolio_sealed_end,\n'
    '            "tags": {**simulation_request.tags, "evaluation_stage": "PORTFOLIO_SEALED"},\n'
    '        }\n'
    '    )\n'
    '    portfolio_sealed = ExperimentCoordinator(factory).execute(\n'
    '        mission_id=None,\n'
    '        program_id=source_program_id,\n'
    '        branch_id=source_branch_id,\n'
    '        request=portfolio_sealed_request,\n'
    '        sealed=True,\n'
    '        parent_entry_id=simulation.id,\n'
    '    )\n'
    '    if portfolio_sealed.evidence_json:\n'
    '        raise QfError("SEALED_RAW_EVIDENCE_PERSISTED", "Portfolio sealed evaluation crossed the disclosure boundary.", 500)\n'
    '    portfolio_sealed_disclosure = dict(portfolio_sealed.disclosure_json or {})\n'
    '    if not portfolio_sealed_disclosure.get("passed"):\n'
    '        raise QfError(\n'
    '            "PORTFOLIO_SEALED_EVALUATION_FAILED",\n'
    '            "Constructed portfolio did not pass independent sealed evaluation.",\n'
    '            422,\n'
    '            {"sealed_experiment_id": str(portfolio_sealed.id)},\n'
    '        )\n\n'
    '    with factory() as session, session.begin():\n',
)
# Program lock must serialize actionable recommendations.
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '        current_candidate = (\n'
    '            session.get(PortfolioCandidate, portfolio_program.current_candidate_id)\n',
    '        pending_approval_id = _pending_program_approval_id(session, portfolio_program.id)\n'
    '        if pending_approval_id is not None:\n'
    '            raise QfError(\n'
    '                "PORTFOLIO_APPROVAL_PENDING",\n'
    '                "Portfolio Program already has an actionable pending recommendation.",\n'
    '                409,\n'
    '                {"approval_id": str(pending_approval_id)},\n'
    '            )\n'
    '        current_candidate = (\n'
    '            session.get(PortfolioCandidate, portfolio_program.current_candidate_id)\n',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '                    "sealed_summary": sealed_summary,\n'
    '                    "robustness_summary": {\n',
    '                    "sealed_summary": sealed_summary,\n'
    '                    "portfolio_sealed_disclosure": portfolio_sealed_disclosure,\n'
    '                    "portfolio_sealed_experiment_id": str(portfolio_sealed.id),\n'
    '                    "portfolio_sealed_dataset_revision_id": str(portfolio_sealed.dataset_revision_id),\n'
    '                    "robustness_summary": {\n',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '                "remote_run_id": simulation.remote_run_id,\n'
    '            },\n',
    '                "remote_run_id": simulation.remote_run_id,\n'
    '                "portfolio_sealed_experiment_id": str(portfolio_sealed.id),\n'
    '                "portfolio_sealed_passed": True,\n'
    '            },\n',
)

# ---------------------------------------------------------------------------
# 5. Tests and CI for OS sandbox, durable IDs, second sealed episode/registration.
# ---------------------------------------------------------------------------
replace_once(
    "backend/tests/integration/test_nautilus_promotion.py",
    '        sealed_dataset_revision_id=sealed_dataset_id,\n'
    '        name="Qualified remote Nautilus alpha",\n',
    '        sealed_dataset_revision_id=sealed_dataset_id,\n'
    '        sealed_experiment_id=uuid4(),\n'
    '        name="Qualified remote Nautilus alpha",\n',
)
replace_once(
    "backend/tests/integration/test_nautilus_promotion.py",
    '            sealed_dataset_revision_id=sealed_dataset_id,\n'
    '        )\n',
    '            sealed_dataset_revision_id=sealed_dataset_id,\n'
    '            sealed_experiment_id=uuid4(),\n'
    '        )\n',
)
# Seed a second non-overlapping sealed episode before actual Candidate simulation.
replace_once(
    "backend/tests/integration/test_nautilus_promotion.py",
    '    promoted = simulate_portfolio_candidate(\n'
    '        factory,\n'
    '        portfolio_program_id=portfolio_program_id,\n'
    '        alpha_ids=[alpha.id],\n'
    '    )\n',
    '    with factory() as session, session.begin():\n'
    '        first_sealed = session.get(DatasetRevision, sealed_dataset_id)\n'
    '        assert first_sealed is not None\n'
    '        second_start = datetime.now(UTC) - timedelta(days=9)\n'
    '        session.add(\n'
    '            DatasetRevision(\n'
    '                universe_version_id=first_sealed.universe_version_id,\n'
    '                universe_name=first_sealed.universe_name,\n'
    '                revision_no=3,\n'
    '                event_start=second_start,\n'
    '                event_end=second_start + timedelta(days=5),\n'
    '                available_start=second_start + timedelta(seconds=2),\n'
    '                available_end=second_start + timedelta(days=5, seconds=2),\n'
    '                row_count=360,\n'
    '                quality_state="VALID",\n'
    '                point_in_time_state="VALID",\n'
    '                partition="SEALED",\n'
    '                created_at=datetime.now(UTC),\n'
    '                provider_name="CI fixture provider",\n'
    '                source_license="CC0-1.0",\n'
    '                catalog_uri="nautilus-catalog://promotion-portfolio-sealed",\n'
    '                nautilus_data_type="QuoteTick",\n'
    '                instrument_scope=["EUR/USD.SIM"],\n'
    '                schema_revision="quote-v2",\n'
    '                quality_result={"state": "VALID"},\n'
    '                point_in_time_result={"state": "VALID"},\n'
    '                ingested_at=datetime.now(UTC),\n'
    '            )\n'
    '        )\n'
    '    promoted = simulate_portfolio_candidate(\n'
    '        factory,\n'
    '        portfolio_program_id=portfolio_program_id,\n'
    '        alpha_ids=[alpha.id],\n'
    '        simulation_experiment_id=uuid4(),\n'
    '        portfolio_sealed_experiment_id=uuid4(),\n'
    '    )\n',
)
replace_once(
    "backend/tests/integration/test_nautilus_promotion.py",
    '            portfolio_program_id=portfolio_program_id,\n'
    '            alpha_ids=[alpha_id],\n'
    '        )\n',
    '            portfolio_program_id=portfolio_program_id,\n'
    '            alpha_ids=[alpha_id],\n'
    '            simulation_experiment_id=uuid4(),\n'
    '            portfolio_sealed_experiment_id=uuid4(),\n'
    '        )\n',
)

# Endpoint fake accepts new argument.
replace_once(
    "backend/tests/integration/test_issue22_final_guards.py",
    '        simulation_experiment_id=None,\n'
    '    ):\n'
    '        calls.append((portfolio_program_id, tuple(alpha_ids), simulation_experiment_id))\n',
    '        simulation_experiment_id=None,\n'
    '        portfolio_sealed_experiment_id=None,\n'
    '    ):\n'
    '        calls.append((portfolio_program_id, tuple(alpha_ids), simulation_experiment_id, portfolio_sealed_experiment_id))\n',
)
replace_once(
    "backend/tests/integration/test_issue22_final_guards.py",
    '    assert calls[0][2] is not None\n',
    '    assert calls[0][2] is not None\n'
    '    assert calls[0][3] is not None\n',
)

# Sealed worker qualification unit test payloads need durable id where present.
for test_path in [
    "backend/tests/integration/test_issue22_readiness_v2.py",
    "backend/tests/unit/test_issue22_codex_round5.py",
    "backend/tests/unit/test_jobs.py",
]:
    path = Path(test_path)
    if path.exists():
        text = path.read_text(encoding="utf-8")
        needle = '"sealed_dataset_revision_id": str('
        if needle in text and '"sealed_experiment_id"' not in text:
            # Best-effort broad fixture addition after each sealed dataset line.
            lines = text.splitlines(keepends=True)
            output: list[str] = []
            for line in lines:
                output.append(line)
                if needle in line:
                    indent = line[: len(line) - len(line.lstrip())]
                    output.append(f'{indent}"sealed_experiment_id": str(uuid4()),\n')
            path.write_text("".join(output), encoding="utf-8")

# Gateway source validator regression and sandbox requirement.
with Path("nautilus_runtime/tests/test_gateway_api.py").open("a", encoding="utf-8") as stream:
    stream.write('''\n\ndef test_source_bundle_rejects_module_object_escape() -> None:\n    with pytest.raises(ValueError, match="attribute 'sys'"):\n        StrategyArtifact(\n            artifact_id="escape",\n            kind="SOURCE_BUNDLE",\n            strategy_path="evil:S",\n            config_path="evil:C",\n            source_files={"evil.py": "import dataclasses\\ndataclasses.sys.modules['os'].system('id')\\n"},\n        )\n''')

replace_once(
    ".github/workflows/ci.yml",
    '      - name: Install pinned remote runtime\n'
    '        run: python -m pip install -e \'nautilus_runtime[dev]\'\n',
    '      - name: Install OS capability sandbox\n'
    '        run: |\n'
    '          sudo apt-get update\n'
    '          sudo apt-get install -y bubblewrap\n'
    '          sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0 || true\n'
    '          bwrap --unshare-all --ro-bind /usr /usr --ro-bind /lib /lib --ro-bind /lib64 /lib64 --proc /proc --dev /dev /usr/bin/true\n'
    '      - name: Install pinned remote runtime\n'
    '        run: python -m pip install -e \'nautilus_runtime[dev]\'\n',
)

# Operations contract: remote Linux gateway must provide the kernel sandbox; sealed raw
# data are provisioned locally on that remote instance then metadata-registered by worker.
with Path("OPERATIONS.md").open("a", encoding="utf-8") as stream:
    stream.write('''\n\n### Remote Nautilus SOURCE_BUNDLE OS isolation\n\nThe remote Nautilus Gateway is a Linux-only execution boundary for Mission-authored `SOURCE_BUNDLE` code. Install `bubblewrap` (`bwrap`) and permit unprivileged user/mount/network namespaces for the Gateway service account. The Gateway fails closed when `bwrap` is unavailable. Each authored strategy runs with an empty network namespace and a mount namespace containing only trusted Python/runtime libraries plus the single disposable operation workspace; the Gateway data root, sibling catalogs, service environment and host home are not mounted. The Python AST gate remains defense-in-depth, not the isolation boundary.\n\n### Sealed catalog provisioning and registration\n\nSealed observations never transit QuaZonai API, Codex workspaces, ordinary workers or Core job payloads. On the independently deployed SEALED Nautilus host, set `NAUTILUS_GATEWAY_ROLE=SEALED` and provision the typed `CatalogIngestRequest` from a local protected file with `quazonai-nautilus-sealed-provision --input /secure/release.json`. Core then queues metadata-only registration with `POST /api/v1/market-universe-versions/{universe_version_id}/sealed-dataset-revisions/register` (Idempotency-Key required). Only `sealed-evaluator` possesses the sealed Gateway credential; it calls the sealed-only catalog validation route and freezes the validated DatasetRevision metadata as `SEALED`. No sealed QuoteTick row is persisted in Core. Candidate promotion requires a second, non-overlapping sealed revision beyond the Alpha qualification episode and performs an independent portfolio-level sealed disclosure before creating a Paper Approval.\n''')
