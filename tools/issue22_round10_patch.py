from __future__ import annotations

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected one match, found {count}: {old[:120]!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


def replace_between(path: str, start: str, end: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    start_index = text.find(start)
    if start_index < 0:
        raise RuntimeError(f"{path}: start marker not found: {start!r}")
    end_index = text.find(end, start_index)
    if end_index < 0:
        raise RuntimeError(f"{path}: end marker not found: {end!r}")
    target.write_text(text[:start_index] + new + text[end_index:], encoding="utf-8")


# ---------------------------------------------------------------------------
# Immutable Gateway/catalog release identity.
# ---------------------------------------------------------------------------
replace_once(
    "backend/src/db/domain_models.py",
    "    catalog_uri: Mapped[str | None] = mapped_column(Text, nullable=True)\n    nautilus_data_type: Mapped[str | None] = mapped_column(String(100), nullable=True)\n",
    "    catalog_uri: Mapped[str | None] = mapped_column(Text, nullable=True)\n"
    "    gateway_instance_id: Mapped[UUID | None] = mapped_column(Uuid, nullable=True)\n"
    "    catalog_release_id: Mapped[UUID | None] = mapped_column(Uuid, nullable=True)\n"
    "    nautilus_data_type: Mapped[str | None] = mapped_column(String(100), nullable=True)\n",
)

replace_once(
    "backend/src/quant_runtime/contracts.py",
    "    runtime_version: str\n    catalog_kind: Literal[\"PARQUET_DATA_CATALOG\"] = \"PARQUET_DATA_CATALOG\"\n",
    "    runtime_version: str\n    gateway_instance_id: UUID\n    catalog_kind: Literal[\"PARQUET_DATA_CATALOG\"] = \"PARQUET_DATA_CATALOG\"\n",
)
replace_once(
    "backend/src/quant_runtime/contracts.py",
    "    catalog_uri: str\n    nautilus_data_type: str\n",
    "    catalog_uri: str\n    gateway_instance_id: UUID\n    catalog_release_id: UUID\n    nautilus_data_type: str\n",
)
replace_once(
    "backend/src/quant_runtime/contracts.py",
    "    catalog_uri: str | None = None\n    valid: bool\n",
    "    catalog_uri: str | None = None\n    gateway_instance_id: UUID | None = None\n    catalog_release_id: UUID | None = None\n    valid: bool\n",
)

engine_path = "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py"
replace_between(
    engine_path,
    "def _source_bundle_sandbox_command(\n",
    "\n\nclass NautilusGatewayEngine:",
    '''def _source_bundle_sandbox_command(\n    *, operation: str, workspace: Path, data_root: Path\n) -> list[str]:\n    """Build a fail-closed Bubblewrap command for authored Python.\n\n    Start from a read-only host root so the interpreter and its native Nautilus\n    dependencies remain executable on normal Linux distributions and hosted CI.\n    Then mask the Gateway data root and common host-secret locations, bind only\n    the disposable operation workspace back at /sandbox, and unshare networking.\n    """\n    if sys.platform != "linux":\n        raise GatewayContractError("SOURCE_BUNDLE execution requires Linux OS isolation")\n    configured = os.getenv("NAUTILUS_GATEWAY_OS_SANDBOX", "bwrap").strip() or "bwrap"\n    sandbox = shutil.which(configured)\n    if sandbox is None:\n        raise GatewayContractError("SOURCE_BUNDLE OS sandbox is unavailable")\n    gateway_source = Path(__file__).resolve().parents[1]\n    command = [\n        sandbox,\n        "--die-with-parent",\n        "--new-session",\n        "--unshare-user",\n        "--unshare-ipc",\n        "--unshare-pid",\n        "--unshare-net",\n        "--unshare-uts",\n        "--ro-bind", "/", "/",\n        "--bind", str(workspace), "/sandbox",\n        "--ro-bind", str(gateway_source), "/gateway-src",\n        "--proc", "/proc",\n        "--dev", "/dev",\n    ]\n    masked: set[str] = set()\n    for candidate in (Path("/tmp"), Path("/run"), Path("/root"), data_root.resolve()):\n        resolved = str(candidate.resolve())\n        if candidate.exists() and resolved not in {"/", str(workspace.resolve())} and resolved not in masked:\n            command.extend(["--tmpfs", resolved])\n            masked.add(resolved)\n    home = Path.home().resolve()\n    executable = Path(sys.executable).resolve()\n    if home.exists() and home != Path("/") and not executable.is_relative_to(home):\n        resolved_home = str(home)\n        if resolved_home not in masked:\n            command.extend(["--tmpfs", resolved_home])\n    command.extend([\n        "--chdir", "/sandbox",\n        "--setenv", "QUAZONAI_NAUTILUS_ISOLATED_CHILD", "1",\n        "--setenv", "PYTHONDONTWRITEBYTECODE", "1",\n        sys.executable, "-I", "-c",\n        (\n            "import sys; sys.path.insert(0, '/gateway-src'); "\n            "from quazonai_nautilus_gateway.isolated_runner import main; main()"\n        ),\n        operation, "/sandbox/runtime", "/sandbox/input.json",\n    ])\n    return command\n''',
)
replace_once(
    engine_path,
    "        self._data_root = data_root.resolve()\n        self._catalog_root = self._data_root / \"catalogs\"\n",
    "        self._data_root = data_root.resolve()\n"
    "        self._gateway_instance_path = self._data_root / \".gateway-instance-id\"\n"
    "        self._catalog_root = self._data_root / \"catalogs\"\n",
)
replace_once(
    engine_path,
    "        self._run_root.mkdir(parents=True, exist_ok=True)\n\n    def capabilities(self) -> dict[str, Any]:\n",
    '''        self._run_root.mkdir(parents=True, exist_ok=True)\n        self._gateway_instance_id = self._load_or_create_gateway_instance_id()\n\n    def _load_or_create_gateway_instance_id(self) -> UUID:\n        path = self._gateway_instance_path\n        if path.exists() or path.is_symlink():\n            if path.is_symlink() or not path.is_file():\n                raise GatewayContractError("gateway instance identity path is invalid")\n            try:\n                return UUID(path.read_text(encoding="ascii").strip())\n            except (OSError, UnicodeError, ValueError) as exc:\n                raise GatewayContractError("gateway instance identity is invalid") from exc\n        candidate = uuid4()\n        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | getattr(os, "O_NOFOLLOW", 0)\n        try:\n            fd = os.open(path, flags, 0o600)\n        except FileExistsError:\n            try:\n                return UUID(path.read_text(encoding="ascii").strip())\n            except (OSError, UnicodeError, ValueError) as exc:\n                raise GatewayContractError("gateway instance identity is invalid") from exc\n        try:\n            os.write(fd, f"{candidate}\\n".encode("ascii"))\n            os.fsync(fd)\n        finally:\n            os.close(fd)\n        return candidate\n\n    def capabilities(self) -> dict[str, Any]:\n''',
)
replace_once(
    engine_path,
    '            "runtime_version": nautilus_version,\n            "catalog_kind": "PARQUET_DATA_CATALOG",\n',
    '            "runtime_version": nautilus_version,\n            "gateway_instance_id": str(self._gateway_instance_id),\n            "catalog_kind": "PARQUET_DATA_CATALOG",\n',
)
replace_once(
    engine_path,
    '            "catalog_uri": f"nautilus-catalog://{manifest[\'catalog_key\']}",\n            "nautilus_data_type": manifest["nautilus_data_type"],\n',
    '            "catalog_uri": f"nautilus-catalog://{manifest[\'catalog_key\']}",\n            "gateway_instance_id": UUID(str(manifest["gateway_instance_id"])),\n            "catalog_release_id": UUID(str(manifest["catalog_release_id"])),\n            "nautilus_data_type": manifest["nautilus_data_type"],\n',
)
replace_once(
    engine_path,
    '        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))\n        return self._manifest_result(manifest)\n',
    '''        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))\n        try:\n            storage_id = UUID(hex=catalog_path.name)\n        except ValueError as exc:\n            raise GatewayContractError("catalog storage identity is invalid") from exc\n        if (\n            str(manifest.get("gateway_instance_id")) != str(self._gateway_instance_id)\n            or str(manifest.get("catalog_release_id")) != str(storage_id)\n        ):\n            raise GatewayContractError("catalog release identity does not match this Gateway")\n        return self._manifest_result(manifest)\n''',
)
replace_once(
    engine_path,
    '                    "runtime_version": nautilus_version,\n                    "catalog_key": request.catalog_key,\n',
    '                    "runtime_version": nautilus_version,\n                    "gateway_instance_id": str(self._gateway_instance_id),\n                    "catalog_release_id": str(storage_id),\n                    "catalog_key": request.catalog_key,\n',
)
replace_once(
    engine_path,
    '        catalog_path = self._catalog_path(request.catalog_key)\n        manifest_path = catalog_path / "quazonai-catalog-manifest.json"\n        findings: list[dict[str, Any]] = []\n',
    '''        catalog_path = self._catalog_path(request.catalog_key)\n        record = self._find_catalog_record(self._load_catalog_registry(), request.catalog_key)\n        if record is None or record.state != "READY":\n            raise GatewayContractError("selected catalog is unavailable")\n        manifest_path = catalog_path / "quazonai-catalog-manifest.json"\n        findings: list[dict[str, Any]] = []\n''',
)
replace_once(
    engine_path,
    '        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))\n        catalog = ParquetDataCatalog(path=str(catalog_path))\n',
    '''        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))\n        if str(manifest.get("gateway_instance_id")) != str(self._gateway_instance_id):\n            findings.append({"code": "GATEWAY_INSTANCE_ID_MISMATCH"})\n        if str(manifest.get("catalog_release_id")) != str(record.storage_id):\n            findings.append({"code": "CATALOG_RELEASE_ID_MISMATCH"})\n        catalog = ParquetDataCatalog(path=str(catalog_path))\n''',
)
replace_once(
    engine_path,
    '            "catalog_uri": f"nautilus-catalog://{request.catalog_key}",\n            "valid": not findings and bool(instruments) and bool(ticks),\n',
    '            "catalog_uri": f"nautilus-catalog://{request.catalog_key}",\n            "gateway_instance_id": str(self._gateway_instance_id),\n            "catalog_release_id": str(record.storage_id),\n            "valid": not findings and bool(instruments) and bool(ticks),\n',
)
replace_once(
    engine_path,
    '                _source_bundle_sandbox_command(operation=operation, workspace=workspace),\n',
    '                _source_bundle_sandbox_command(\n                    operation=operation, workspace=workspace, data_root=self._data_root\n                ),\n',
)

# ---------------------------------------------------------------------------
# Persist and enforce Gateway/catalog release identity in Core.
# ---------------------------------------------------------------------------
replace_once(
    "backend/src/api/domain.py",
    "    partition: str\n    created_at: str\n",
    "    partition: str\n    gateway_instance_id: UUID | None = None\n    catalog_release_id: UUID | None = None\n    created_at: str\n",
)
replace_once(
    "backend/src/api/domain.py",
    "        partition=item.partition,\n        created_at=_iso(item.created_at) or \"\",\n",
    "        partition=item.partition,\n        gateway_instance_id=item.gateway_instance_id,\n        catalog_release_id=item.catalog_release_id,\n        created_at=_iso(item.created_at) or \"\",\n",
)
replace_once(
    "backend/src/api/domain.py",
    "            or validated.available_time_end != ingested.available_time_end\n        ):\n",
    "            or validated.available_time_end != ingested.available_time_end\n"
    "            or validated.gateway_instance_id != ingested.gateway_instance_id\n"
    "            or validated.catalog_release_id != ingested.catalog_release_id\n"
    "        ):\n",
)
replace_once(
    "backend/src/api/domain.py",
    "                    or existing.row_count != ingested.row_count\n                ):\n",
    "                    or existing.row_count != ingested.row_count\n"
    "                    or existing.gateway_instance_id != ingested.gateway_instance_id\n"
    "                    or existing.catalog_release_id != ingested.catalog_release_id\n"
    "                ):\n",
)
replace_once(
    "backend/src/api/domain.py",
    "                    catalog_uri=ingested.catalog_uri,\n                    nautilus_data_type=ingested.nautilus_data_type,\n",
    "                    catalog_uri=ingested.catalog_uri,\n"
    "                    gateway_instance_id=ingested.gateway_instance_id,\n"
    "                    catalog_release_id=ingested.catalog_release_id,\n"
    "                    nautilus_data_type=ingested.nautilus_data_type,\n",
)
replace_once(
    "backend/src/api/domain.py",
    "                partition=item.partition,\n                created_at=_iso(item.created_at) or \"\",\n",
    "                partition=item.partition,\n                gateway_instance_id=item.gateway_instance_id,\n                catalog_release_id=item.catalog_release_id,\n                created_at=_iso(item.created_at) or \"\",\n",
)

replace_once(
    "backend/src/runners/sealed_worker.py",
    "        validation.catalog_uri, validation.nautilus_data_type, validation.schema_revision,\n",
    "        validation.catalog_uri, validation.gateway_instance_id, validation.catalog_release_id,\n"
    "        validation.nautilus_data_type, validation.schema_revision,\n",
)
replace_once(
    "backend/src/runners/sealed_worker.py",
    "                or existing.row_count != validation.row_count\n            ):\n",
    "                or existing.row_count != validation.row_count\n"
    "                or existing.gateway_instance_id != validation.gateway_instance_id\n"
    "                or existing.catalog_release_id != validation.catalog_release_id\n"
    "            ):\n",
)
replace_once(
    "backend/src/runners/sealed_worker.py",
    "                catalog_uri=validation.catalog_uri,\n                nautilus_data_type=validation.nautilus_data_type,\n",
    "                catalog_uri=validation.catalog_uri,\n"
    "                gateway_instance_id=validation.gateway_instance_id,\n"
    "                catalog_release_id=validation.catalog_release_id,\n"
    "                nautilus_data_type=validation.nautilus_data_type,\n",
)

replace_once(
    "backend/src/quant_runtime/ledger.py",
    "    BacktestEvidence,\n    BacktestExperimentRequest,\n",
    "    BacktestEvidence,\n    BacktestExperimentRequest,\n    CatalogValidationRequest,\n",
)
replace_once(
    "backend/src/quant_runtime/ledger.py",
    "        try:\n            config = RemoteNautilusConfig.from_env(sealed=sealed)\n            with NautilusQuantRuntime(config) as runtime:\n                if sealed:\n",
    '''        try:\n            with self._factory() as session:\n                immutable_dataset = session.get(DatasetRevision, request.dataset_revision_id)\n                if immutable_dataset is None:\n                    raise QfError("DATASET_REVISION_NOT_FOUND", "Dataset Revision does not exist.", 404)\n                expected_gateway_instance_id = immutable_dataset.gateway_instance_id\n                expected_catalog_release_id = immutable_dataset.catalog_release_id\n            if expected_gateway_instance_id is None or expected_catalog_release_id is None:\n                raise QfError(\n                    "NAUTILUS_CATALOG_IDENTITY_MISSING",\n                    "Dataset Revision is not bound to an immutable Gateway catalog release.",\n                    422,\n                )\n            config = RemoteNautilusConfig.from_env(sealed=sealed)\n            with NautilusQuantRuntime(config) as runtime:\n                validation_request = CatalogValidationRequest(\n                    catalog_key=request.catalog_key,\n                    instrument_ids=request.instrument_ids,\n                    nautilus_data_type="QuoteTick",\n                )\n                remote_catalog = (\n                    runtime.validate_sealed_catalog(validation_request)\n                    if sealed\n                    else runtime.validate_catalog(validation_request)\n                )\n                if (\n                    not remote_catalog.valid\n                    or remote_catalog.gateway_instance_id != expected_gateway_instance_id\n                    or remote_catalog.catalog_release_id != expected_catalog_release_id\n                ):\n                    raise QfError(\n                        "NAUTILUS_CATALOG_IDENTITY_MISMATCH",\n                        "Remote Gateway catalog release no longer matches the immutable Dataset Revision.",\n                        409,\n                    )\n                if sealed:\n''',
)

# ---------------------------------------------------------------------------
# Global idempotency for sealed catalog registration.
# ---------------------------------------------------------------------------
registration_path = "backend/src/api/research_runtime.py"
replace_once(
    registration_path,
    '    normalized: dict[str, object] = {\n        "data_source_id": str(payload.data_source_id),\n',
    '    normalized: dict[str, object] = {\n        "universe_version_id": str(universe_version_id),\n        "data_source_id": str(payload.data_source_id),\n',
)
old_jobs_block = '''        jobs = list(session.scalars(\n            select(Job)\n            .where(\n                Job.kind == SEALED_DATASET_REGISTRATION_JOB_KIND,\n                Job.resource_id == universe_version_id,\n                Job.state.in_(["READY", "LEASED", "SUCCEEDED"]),\n            )\n            .order_by(Job.created_at.desc())\n        ))\n        for existing in jobs:\n            existing_payload = dict(existing.payload or {})\n            if existing_payload.get("idempotency_key") != key:\n                continue\n            if any(existing_payload.get(name) != value for name, value in normalized.items()):\n                raise QfError("IDEMPOTENCY_KEY_REUSED", "The sealed registration key belongs to another request.", 409)\n            return SealedDatasetRegistrationJobResult(\n                job_id=existing.id, universe_version_id=universe_version_id, state=existing.state\n            )\n        job = enqueue_job(\n            session,\n            kind=SEALED_DATASET_REGISTRATION_JOB_KIND,\n            resource_type="MARKET_UNIVERSE_VERSION",\n            resource_id=universe_version_id,\n            payload=normalized,\n        )\n        return SealedDatasetRegistrationJobResult(\n            job_id=job.id, universe_version_id=universe_version_id, state=job.state\n        )\n'''
new_jobs_block = '''        operation = "SEALED_DATASET_REGISTRATION"\n        receipt = session.execute(\n            select(PublicMutationReceipt)\n            .where(PublicMutationReceipt.idempotency_key == key)\n            .with_for_update()\n        ).scalar_one_or_none()\n        if receipt is None:\n            receipt = PublicMutationReceipt(\n                idempotency_key=key,\n                operation_name=operation,\n                normalized_request=normalized,\n                response_json={"state": "CLAIMING"},\n                status_code=202,\n                created_at=datetime.now(UTC),\n            )\n            try:\n                with session.begin_nested():\n                    session.add(receipt)\n                    session.flush()\n            except IntegrityError as exc:\n                if receipt in session:\n                    session.expunge(receipt)\n                session.expire_all()\n                receipt = session.execute(\n                    select(PublicMutationReceipt)\n                    .where(PublicMutationReceipt.idempotency_key == key)\n                    .with_for_update()\n                ).scalar_one_or_none()\n                if receipt is None:\n                    raise QfError(\n                        "IDEMPOTENCY_RECEIPT_CONFLICT",\n                        "Sealed registration receipt could not be resolved after a concurrent request.",\n                        409,\n                    ) from exc\n            else:\n                job = enqueue_job(\n                    session,\n                    kind=SEALED_DATASET_REGISTRATION_JOB_KIND,\n                    resource_type="MARKET_UNIVERSE_VERSION",\n                    resource_id=universe_version_id,\n                    payload={**normalized, "idempotency_key": key},\n                )\n                receipt.response_json = {\n                    "job_id": str(job.id),\n                    "universe_version_id": str(universe_version_id),\n                }\n                return SealedDatasetRegistrationJobResult(\n                    job_id=job.id, universe_version_id=universe_version_id, state=job.state\n                )\n        if receipt.operation_name != operation or receipt.normalized_request != normalized:\n            raise QfError(\n                "IDEMPOTENCY_KEY_REUSED",\n                "The idempotency key belongs to a different public mutation.",\n                409,\n            )\n        try:\n            job_id = UUID(str((receipt.response_json or {})["job_id"]))\n        except (KeyError, TypeError, ValueError) as exc:\n            raise QfError(\n                "IDEMPOTENCY_RECEIPT_INVALID",\n                "Sealed registration receipt lost its durable job identity.",\n                500,\n            ) from exc\n        job = session.get(Job, job_id)\n        if job is None or job.kind != SEALED_DATASET_REGISTRATION_JOB_KIND:\n            raise QfError(\n                "IDEMPOTENCY_RECEIPT_INVALID",\n                "Sealed registration receipt points to a missing job.",\n                500,\n            )\n        return SealedDatasetRegistrationJobResult(\n            job_id=job.id, universe_version_id=universe_version_id, state=job.state\n        )\n'''
replace_once(registration_path, old_jobs_block, new_jobs_block)

# ---------------------------------------------------------------------------
# Do not consume portfolio holdout evidence until Paper configuration is ready.
# ---------------------------------------------------------------------------
promotion_path = "backend/src/quant_runtime/promotion.py"
insert_marker = '''def _select_portfolio_sealed_dataset(\n    session: Session,\n'''
helper = '''def _ready_candidate_downstream(\n    session: Session, *, environment_type: str, universe: str | None\n) -> DownstreamSystem | None:\n    downstreams = list(\n        session.scalars(\n            select(DownstreamSystem)\n            .where(\n                DownstreamSystem.environment_type == environment_type,\n                DownstreamSystem.enabled.is_(True),\n                DownstreamSystem.preflight_state == "READY",\n                DownstreamSystem.package_contract_version == "2",\n            )\n            .order_by(DownstreamSystem.name, DownstreamSystem.id)\n        )\n    )\n    return next(\n        (\n            downstream\n            for downstream in downstreams\n            if not downstream.compatibility or universe in downstream.compatibility\n        ),\n        None,\n    )\n\n\n'''
replace_once(promotion_path, insert_marker, helper + insert_marker)
replace_once(
    promotion_path,
    '''        constraints = _validate_mandate_before_simulation(mandate, alpha)\n        simulation_request = request.model_copy(\n''',
    '''        constraints = _validate_mandate_before_simulation(mandate, alpha)\n        if _ready_candidate_downstream(\n            session, environment_type="PAPER", universe=alpha.universe\n        ) is None:\n            raise QfError(\n                "PAPER_DOWNSTREAM_NOT_READY",\n                "A ready compatible Candidate Bundle v2 Paper downstream is required before simulation.",\n                409,\n            )\n        simulation_request = request.model_copy(\n''',
)
replace_once(
    promotion_path,
    '''        source_dataset = session.get(DatasetRevision, source.dataset_revision_id)\n        if source_dataset is None:\n            raise QfError("DATASET_REVISION_NOT_FOUND", "Portfolio source Dataset Revision is missing.", 500)\n        portfolio_sealed_dataset = _select_portfolio_sealed_dataset(\n''',
    '''        source_dataset = session.get(DatasetRevision, source.dataset_revision_id)\n        if source_dataset is None:\n            raise QfError("DATASET_REVISION_NOT_FOUND", "Portfolio source Dataset Revision is missing.", 500)\n        paper_downstream = _ready_candidate_downstream(\n            session, environment_type="PAPER", universe=alpha.universe\n        )\n        if paper_downstream is None:\n            raise QfError(\n                "PAPER_DOWNSTREAM_NOT_READY",\n                "A ready compatible Candidate Bundle v2 Paper downstream is required before consuming sealed evidence.",\n                409,\n            )\n        paper_downstream_id = paper_downstream.id\n        portfolio_sealed_dataset = _select_portfolio_sealed_dataset(\n''',
)
old_downstream_lookup = '''        downstreams = list(\n            session.scalars(\n                select(DownstreamSystem)\n                .where(\n                    DownstreamSystem.environment_type == "PAPER",\n                    DownstreamSystem.enabled.is_(True),\n                    DownstreamSystem.preflight_state == "READY",\n                    DownstreamSystem.package_contract_version == "2",\n                )\n                .order_by(DownstreamSystem.name, DownstreamSystem.id)\n            )\n        )\n        paper_downstream = next(\n            (\n                downstream\n                for downstream in downstreams\n                if not downstream.compatibility\n                or persisted_alpha.universe in downstream.compatibility\n            ),\n            None,\n        )\n        if paper_downstream is None:\n            raise QfError(\n                "PAPER_DOWNSTREAM_NOT_READY",\n                "No ready Candidate Bundle v2 Paper downstream matches the selected Alpha.",\n                409,\n            )\n'''
new_downstream_lookup = '''        paper_downstream = session.get(DownstreamSystem, paper_downstream_id)\n        if paper_downstream is None:\n            raise QfError(\n                "PAPER_DOWNSTREAM_DISAPPEARED",\n                "The Paper downstream frozen before sealed evaluation no longer exists.",\n                500,\n            )\n'''
replace_once(promotion_path, old_downstream_lookup, new_downstream_lookup)

# ---------------------------------------------------------------------------
# Explicit Paper -> Live approval transition after valid Forward Evidence.
# ---------------------------------------------------------------------------
domain_path = "backend/src/api/domain.py"
replace_once(
    domain_path,
    '''class ApprovalRejectInput(StrictModel):\n    reason_code: str = Field(min_length=1, max_length=100)\n    note: str | None = Field(default=None, max_length=4000)\n    expected_state: str\n\n\nclass HandoffView(StrictModel):\n''',
    '''class ApprovalRejectInput(StrictModel):\n    reason_code: str = Field(min_length=1, max_length=100)\n    note: str | None = Field(default=None, max_length=4000)\n    expected_state: str\n\n\nclass LivePromotionInput(StrictModel):\n    downstream_system_id: UUID\n    expected_handoff_state: str = "FEEDBACK_COMPLETE"\n\n\nclass HandoffView(StrictModel):\n''',
)
live_route_marker = '''\n\n@router.get("/data-sources", response_model=list[DataSourceView])\ndef list_data_sources(request: Request) -> list[DataSourceView]:\n'''
live_route = '''\n\n@router.post(\n    "/handoffs/{handoff_id}/promote-live",\n    response_model=ApprovalView,\n    status_code=201,\n)\ndef promote_paper_handoff_to_live(\n    handoff_id: UUID,\n    payload: LivePromotionInput,\n    request: Request,\n    idempotency_key: str | None = Header(default=None, alias="Idempotency-Key"),\n) -> dict[str, Any]:\n    """Create a human-gated LIVE Approval only from complete valid Paper evidence."""\n    factory = request.app.state.session_factory\n    with factory() as session, session.begin():\n\n        def action() -> dict[str, Any]:\n            handoff = session.execute(\n                select(HandoffOffer).where(HandoffOffer.id == handoff_id).with_for_update()\n            ).scalar_one_or_none()\n            if handoff is None:\n                raise QfError("HANDOFF_NOT_FOUND", "Paper Handoff was not found.", 404)\n            if handoff.purpose != "PAPER" or handoff.state != payload.expected_handoff_state or handoff.state != "FEEDBACK_COMPLETE":\n                raise QfError(\n                    "LIVE_PROMOTION_FORWARD_EVIDENCE_REQUIRED",\n                    "Live promotion requires a completed Paper Handoff with valid Forward Evidence.",\n                    409,\n                )\n            episode = session.scalar(\n                select(ForwardEvidenceEpisode)\n                .where(\n                    ForwardEvidenceEpisode.handoff_id == handoff.id,\n                    ForwardEvidenceEpisode.state == "FEEDBACK_COMPLETE",\n                )\n                .order_by(ForwardEvidenceEpisode.created_at.desc(), ForwardEvidenceEpisode.id.desc())\n                .limit(1)\n            )\n            if episode is None:\n                raise QfError(\n                    "LIVE_PROMOTION_FORWARD_EVIDENCE_REQUIRED",\n                    "Live promotion requires an immutable completed Forward Evidence episode.",\n                    409,\n                )\n            forward = dict(episode.evidence or {})\n            degradation_state = str(forward.get("degradation_state", "")).strip().upper()\n            if forward.get("degraded") is True or degradation_state in {"DEGRADED", "FAILED"}:\n                raise QfError(\n                    "LIVE_PROMOTION_DEGRADED",\n                    "Degraded Paper evidence cannot be promoted to Live.",\n                    422,\n                )\n            candidate = session.get(PortfolioCandidate, handoff.candidate_id)\n            paper_approval = session.get(ApprovalSnapshot, handoff.approval_id)\n            downstream = session.get(DownstreamSystem, payload.downstream_system_id)\n            if candidate is None or paper_approval is None:\n                raise QfError("LIVE_PROMOTION_LINEAGE_MISSING", "Paper promotion lineage is incomplete.", 500)\n            if (\n                downstream is None\n                or downstream.environment_type != "LIVE"\n                or not downstream.enabled\n                or downstream.preflight_state != "READY"\n                or downstream.package_contract_version != "2"\n                or downstream.service_token_ciphertext is None\n            ):\n                raise QfError(\n                    "LIVE_DOWNSTREAM_NOT_READY",\n                    "Live promotion requires a ready authenticated Candidate Bundle v2 downstream.",\n                    409,\n                )\n            universes = (\n                [str(value) for value in candidate.universe_set_json]\n                if isinstance(candidate.universe_set_json, list)\n                else []\n            )\n            if downstream.compatibility and not any(\n                universe in downstream.compatibility for universe in universes\n            ):\n                raise QfError(\n                    "LIVE_DOWNSTREAM_INCOMPATIBLE",\n                    "Live downstream does not support the Candidate universe.",\n                    409,\n                )\n            for member in candidate.members or []:\n                raw_alpha_id = member.get("alpha_qualification_id") if isinstance(member, dict) else None\n                try:\n                    alpha_id = UUID(str(raw_alpha_id))\n                except (TypeError, ValueError) as exc:\n                    raise QfError("LIVE_PROMOTION_LINEAGE_MISSING", "Candidate lost Alpha lineage.", 500) from exc\n                alpha = session.get(AlphaQualification, alpha_id)\n                if alpha is None or alpha.state != "ACTIVE" or alpha.degradation_state != "HEALTHY":\n                    raise QfError(\n                        "LIVE_PROMOTION_DEGRADED",\n                        "Candidate Alpha is no longer healthy enough for Live promotion.",\n                        422,\n                    )\n            duplicate = session.scalar(\n                select(ApprovalSnapshot)\n                .where(\n                    ApprovalSnapshot.candidate_id == candidate.id,\n                    ApprovalSnapshot.purpose == "LIVE",\n                    ApprovalSnapshot.state.in_(["PENDING", "APPROVED"]),\n                )\n                .order_by(ApprovalSnapshot.created_at.desc())\n                .limit(1)\n            )\n            if duplicate is not None:\n                raise QfError(\n                    "LIVE_APPROVAL_ALREADY_EXISTS",\n                    "Candidate already has an active Live Approval.",\n                    409,\n                    {"approval_id": str(duplicate.id)},\n                )\n            now = _now()\n            approval = ApprovalSnapshot(\n                candidate_id=candidate.id,\n                purpose="LIVE",\n                state="PENDING",\n                downstream_system_id=downstream.id,\n                valid_until=now + timedelta(days=7),\n                recommendation_rationale=(\n                    "Paper Handoff completed its frozen Forward Evidence contract without degradation; "\n                    "Live remains subject to an explicit human Approval and Candidate Bundle conformance."\n                ),\n                human_report={\n                    "summary": "Forward-evidence-gated Live promotion is ready for human review.",\n                    "paper_handoff_id": str(handoff.id),\n                    "forward_evidence_episode_id": str(episode.id),\n                },\n                evidence_summary={\n                    **(paper_approval.evidence_summary or {}),\n                    "paper_handoff_id": str(handoff.id),\n                    "forward_evidence_episode_id": str(episode.id),\n                    "observation_start": episode.observation_start.isoformat(),\n                    "observation_end": episode.observation_end.isoformat(),\n                    "sample_size": episode.sample_size,\n                },\n                capital_context=dict(paper_approval.capital_context or {}),\n                risk_summary=dict(paper_approval.risk_summary or {}),\n                cost_summary=dict(paper_approval.cost_summary or {}),\n                capacity_summary=dict(paper_approval.capacity_summary or {}),\n                changes_summary={\n                    **(paper_approval.changes_summary or {}),\n                    "live_promotion": {\n                        "paper_handoff_id": str(handoff.id),\n                        "forward_evidence_episode_id": str(episode.id),\n                        "created_at": now.isoformat(),\n                    },\n                },\n            )\n            session.add(approval)\n            session.flush()\n            _event(\n                session,\n                "LIVE_APPROVAL_CREATED",\n                "APPROVAL",\n                approval.id,\n                {\n                    "candidate_id": str(candidate.id),\n                    "paper_handoff_id": str(handoff.id),\n                    "forward_evidence_episode_id": str(episode.id),\n                },\n                actor_kind="SYSTEM",\n            )\n            return _approval_view(session, approval).model_dump(mode="json")\n\n        return _idempotent(\n            session,\n            idempotency_key,\n            f"handoff.promote-live:{handoff_id}",\n            payload,\n            action,\n            status_code=201,\n        )\n'''
replace_once(domain_path, live_route_marker, live_route + live_route_marker)

# ---------------------------------------------------------------------------
# Update client mocks and real Gateway integration assertions for identity.
# ---------------------------------------------------------------------------
replace_once(
    "backend/tests/unit/test_quant_runtime_client.py",
    '                "runtime_version": "9.9.9",\n                "catalog_kind": "PARQUET_DATA_CATALOG",\n',
    '                "runtime_version": "9.9.9",\n                "gateway_instance_id": str(uuid4()),\n                "catalog_kind": "PARQUET_DATA_CATALOG",\n',
)
replace_once(
    "nautilus_runtime/tests/test_real_backtest.py",
    '    assert first["catalog_uri"] == "nautilus-catalog://integration-fx-quotes"\n    assert first["row_count"] == 720\n',
    '    assert first["catalog_uri"] == "nautilus-catalog://integration-fx-quotes"\n'
    '    assert UUID(str(first["gateway_instance_id"]))\n'
    '    assert UUID(str(first["catalog_release_id"]))\n'
    '    assert first["row_count"] == 720\n',
)
replace_once(
    "nautilus_runtime/tests/test_real_backtest.py",
    '    assert validated["valid"] is True, validated["findings"]\n    assert validated["row_count"] == 720\n',
    '    assert validated["valid"] is True, validated["findings"]\n'
    '    assert validated["gateway_instance_id"] == first["gateway_instance_id"]\n'
    '    assert validated["catalog_release_id"] == first["catalog_release_id"]\n'
    '    assert validated["row_count"] == 720\n',
)

# The patch is intentionally textual and fail-closed. Any upstream drift causes an error
# before a commit rather than silently applying to the wrong architecture.
