from __future__ import annotations

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one match in {path}: {old[:120]!r}; got {text.count(old)}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


# 1) SEALED gateway must not expose raw catalog validation metadata.
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/app.py",
    '''    @app.post("/v1/catalogs/validate", dependencies=[Depends(_authorize)])
    def validate_catalog(request: CatalogValidationRequest) -> dict[str, Any]:
        return engine().validate_catalog(request)
''',
    '''    @app.post("/v1/catalogs/validate", dependencies=[Depends(_authorize)])
    def validate_catalog(request: CatalogValidationRequest) -> dict[str, Any]:
        _require_role(gateway_role, "RESEARCH")
        return engine().validate_catalog(request)
''',
)

# 2/4/5) Promotion: scale-independent scoring, independent sealed holdout, inherited-search penalty.
promotion = Path("backend/src/quant_runtime/promotion.py")
text = promotion.read_text(encoding="utf-8")
needle = '''def _now() -> datetime:
    return datetime.now(UTC)


def _require_real_transaction_evidence'''
replacement = '''def _now() -> datetime:
    return datetime.now(UTC)


def _evidence_program_lineage_ids(session: Session, program_id: UUID) -> set[UUID]:
    """Return the complete immutable evidence-inheritance lineage for scoring exposure."""
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


def _require_real_transaction_evidence'''
if text.count(needle) != 1:
    raise SystemExit("promotion lineage insertion point changed")
text = text.replace(needle, replacement, 1)

old = '''    if total_return is not None:
        score += 0.20 * math.tanh(total_return / 0.25)
    elif total_pnl is not None:
        score += 0.12 * math.tanh(total_pnl / 10_000.0)
'''
new = '''    if total_return is not None:
        score += 0.20 * math.tanh(total_return / 0.25)
    # Absolute PnL is audit evidence only. It is deliberately not a scoring fallback:
    # account size, trade notional and currency scale are Mission-controlled inputs.
'''
if text.count(old) != 1:
    raise SystemExit("promotion absolute-PnL scoring block changed")
text = text.replace(old, new, 1)
text = text.replace('''        "model": "DISCOVERY_PUBLIC_PERFORMANCE_V1",
''', '''        "model": "DISCOVERY_PUBLIC_PERFORMANCE_V2",
''', 1)
text = text.replace('''        "total_pnl": total_pnl,
        "fill_count": fill_count,
''', '''        "total_pnl": total_pnl,
        "absolute_pnl_used_for_scoring": False,
        "fill_count": fill_count,
''', 1)

old = '''    source_scope = set(source_dataset.instrument_scope or [])
    sealed_scope = set(dataset.instrument_scope or [])
    if not source_scope or not source_scope.issubset(sealed_scope):
        raise QfError(
            "SEALED_DATASET_SCOPE_MISMATCH",
            "Sealed Dataset Revision does not cover the Discovery instrument scope.",
            422,
        )
    return dataset
'''
new = '''    source_scope = set(source_dataset.instrument_scope or [])
    sealed_scope = set(dataset.instrument_scope or [])
    if not source_scope or not source_scope.issubset(sealed_scope):
        raise QfError(
            "SEALED_DATASET_SCOPE_MISMATCH",
            "Sealed Dataset Revision does not cover the Discovery instrument scope.",
            422,
        )
    source_catalog_uri = (source_dataset.catalog_uri or "").strip()
    sealed_catalog_uri = (dataset.catalog_uri or "").strip()
    if source_catalog_uri and sealed_catalog_uri and source_catalog_uri == sealed_catalog_uri:
        raise QfError(
            "SEALED_DATASET_NOT_INDEPENDENT",
            "Sealed evaluation must use a catalog revision independent from Discovery.",
            422,
        )

    source_start = source_dataset.event_start
    source_end = source_dataset.event_end
    sealed_start = dataset.event_start
    sealed_end = dataset.event_end
    bounds = (source_start, source_end, sealed_start, sealed_end)
    if any(value is None for value in bounds):
        raise QfError(
            "SEALED_DATASET_INDEPENDENCE_UNVERIFIABLE",
            "Discovery and Sealed revisions require explicit event-time bounds to prove holdout independence.",
            422,
        )
    assert source_start is not None and source_end is not None
    assert sealed_start is not None and sealed_end is not None
    if any(value.tzinfo is None or value.utcoffset() is None for value in bounds if value is not None):
        raise QfError(
            "SEALED_DATASET_INDEPENDENCE_UNVERIFIABLE",
            "Discovery and Sealed event-time bounds must be timezone-aware.",
            422,
        )
    if source_start >= source_end or sealed_start >= sealed_end:
        raise QfError(
            "SEALED_DATASET_INDEPENDENCE_UNVERIFIABLE",
            "Discovery and Sealed revisions require valid event-time intervals.",
            422,
        )
    if source_start < sealed_end and sealed_start < source_end:
        raise QfError(
            "SEALED_DATASET_TIME_OVERLAP",
            "Sealed evaluation event-time coverage must not overlap Discovery coverage.",
            422,
        )
    return dataset
'''
if text.count(old) != 1:
    raise SystemExit("promotion sealed selector block changed")
text = text.replace(old, new, 1)

old = '''        search_attempt_count = int(
            session.scalar(
                select(func.count())
                .select_from(SearchLedgerEntry)
                .where(
                    SearchLedgerEntry.program_id == source_program_id,
                    SearchLedgerEntry.mode == ExperimentMode.DISCOVERY.value,
                )
            )
            or 1
        )
'''
new = '''        evidence_program_ids = _evidence_program_lineage_ids(session, source_program_id)
        search_attempt_count = int(
            session.scalar(
                select(func.count())
                .select_from(SearchLedgerEntry)
                .where(
                    SearchLedgerEntry.program_id.in_(evidence_program_ids),
                    SearchLedgerEntry.mode == ExperimentMode.DISCOVERY.value,
                )
            )
            or 1
        )
'''
if text.count(old) != 1:
    raise SystemExit("promotion search-attempt count block changed")
text = text.replace(old, new, 1)
promotion.write_text(text, encoding="utf-8")

# 3) Match degradation_state using the same normalization accepted by the helper.
replace_once(
    "backend/src/quant_runtime/degradation.py",
    "from sqlalchemy import exists, or_, select\n",
    "from sqlalchemy import exists, func, or_, select\n",
)
replace_once(
    "backend/src/quant_runtime/degradation.py",
    '''                    ForwardEvidenceEpisode.evidence["degradation_state"]
                    .as_string()
                    .in_(sorted(_DEGRADATION_STATES)),
''',
    '''                    func.upper(
                        func.trim(
                            ForwardEvidenceEpisode.evidence["degradation_state"].as_string()
                        )
                    ).in_(sorted(_DEGRADATION_STATES)),
''',
)

# 6) Preserve rejected malformed-contract activity so the Mission always gets an evidence-review turn.
workspace = Path("backend/src/quant_runtime/workspace.py")
text = workspace.read_text(encoding="utf-8")
needle = '''def execute_workspace_experiments(
    settings: Settings,
'''
insert = '''class WorkspaceExperimentActivity(list[UUID]):
    """List-compatible experiment ids plus contract rejections lacking a usable UUID."""

    rejected_contract_count: int

    def __init__(self) -> None:
        super().__init__()
        self.rejected_contract_count = 0

    @property
    def has_activity(self) -> bool:
        return bool(self) or self.rejected_contract_count > 0


def execute_workspace_experiments(
    settings: Settings,
'''
if text.count(needle) != 1:
    raise SystemExit("workspace execution insertion point changed")
text = text.replace(needle, insert, 1)
text = text.replace(''') -> list[UUID]:
    """Execute new Discovery contracts and write structured evidence for the next Codex turn."""
    contracts_root = _controlled_experiment_root(workspace)
    if contracts_root is None:
        return []
''', ''') -> WorkspaceExperimentActivity:
    """Execute new Discovery contracts and report all evidence/rejection activity."""
    contracts_root = _controlled_experiment_root(workspace)
    if contracts_root is None:
        return WorkspaceExperimentActivity()
''', 1)
text = text.replace('''    executed: list[UUID] = []
''', '''    executed = WorkspaceExperimentActivity()
''', 1)
old = '''                write_workspace_json(
                    workspace,
                    f"evidence/rejected-{path.stem}.json",
                    {
                        "state": "REJECTED",
                        "contract_file": path.name,
                        "failure_code": exc.code,
                        "failure_message": exc.message,
                    },
                )
                continue
'''
new = '''                write_workspace_json(
                    workspace,
                    f"evidence/rejected-{path.stem}.json",
                    {
                        "state": "REJECTED",
                        "contract_file": path.name,
                        "failure_code": exc.code,
                        "failure_message": exc.message,
                    },
                )
                executed.rejected_contract_count += 1
                continue
'''
if text.count(old) != 1:
    raise SystemExit("workspace malformed-contract rejection block changed")
text = text.replace(old, new, 1)
workspace.write_text(text, encoding="utf-8")

replace_once(
    "backend/src/runners/research_missions.py",
    '''                executed_experiment_ids: set[UUID] = set()
                executed_experiment_ids.update(
                    execute_workspace_experiments(
                        settings,
                        workspace=workspace,
                        mission_id=mission.id,
                        program_id=mission.program_id,
                        branch_id=mission.branch_id,
                        already_executed=executed_experiment_ids,
                    )
                )
                if executed_experiment_ids:
''',
    '''                executed_experiment_ids: set[UUID] = set()
                experiment_activity = execute_workspace_experiments(
                    settings,
                    workspace=workspace,
                    mission_id=mission.id,
                    program_id=mission.program_id,
                    branch_id=mission.branch_id,
                    already_executed=executed_experiment_ids,
                )
                executed_experiment_ids.update(experiment_activity)
                if experiment_activity.has_activity:
''',
)

# Gateway regression coverage for the disclosure-only SEALED role.
gateway_test = Path("nautilus_runtime/tests/test_gateway_api.py")
text = gateway_test.read_text(encoding="utf-8")
append = '''\n\ndef test_sealed_gateway_hides_catalog_validation(monkeypatch, tmp_path: Path) -> None:\n    monkeypatch.setenv("NAUTILUS_GATEWAY_ALLOW_ANONYMOUS", "true")\n    monkeypatch.delenv("NAUTILUS_GATEWAY_TOKEN", raising=False)\n    client = TestClient(create_app(data_root=tmp_path, role="SEALED"))\n    response = client.post(\n        "/v1/catalogs/validate",\n        json={\n            "request_id": str(uuid4()),\n            "catalog_key": "sealed-private-catalog",\n            "instrument_ids": [],\n        },\n    )\n    assert response.status_code == 404\n    assert response.json()["detail"] == "operation unavailable on this gateway role"\n'''
if "test_sealed_gateway_hides_catalog_validation" not in text:
    gateway_test.write_text(text.rstrip() + append + "\n", encoding="utf-8")

# Focused Core regressions for this Codex round.
Path("backend/tests/unit/test_issue22_codex_round5.py").write_text('''from __future__ import annotations\n\nfrom datetime import UTC, datetime, timedelta\nfrom uuid import uuid4\n\nimport pytest\n\nfrom db.models import DatasetRevision, ResearchProgram\nfrom errors import QfError\nfrom quant_runtime.degradation import _is_explicit_degradation\nfrom quant_runtime.promotion import (\n    _discovery_quality_score,\n    _evidence_program_lineage_ids,\n    _select_sealed_dataset,\n)\nfrom quant_runtime.workspace import WorkspaceExperimentActivity\n\n\ndef _revision(*, partition: str, catalog_uri: str, start: datetime, end: datetime, universe_id):\n    return DatasetRevision(\n        id=uuid4(),\n        partition=partition,\n        quality_state="VALID",\n        point_in_time_state="VALID",\n        universe_version_id=universe_id,\n        instrument_scope=["EUR/USD.SIM"],\n        catalog_uri=catalog_uri,\n        event_start=start,\n        event_end=end,\n        created_at=datetime.now(UTC),\n    )\n\n\nclass _DatasetSession:\n    def __init__(self, dataset: DatasetRevision) -> None:\n        self.dataset = dataset\n\n    def get(self, model, identity):\n        del model\n        return self.dataset if identity == self.dataset.id else None\n\n\ndef test_discovery_quality_does_not_reward_absolute_pnl_scale() -> None:\n    evidence = {\n        "fills": [{"trade_id": "T-1"}],\n        "statistics": {"returns": {}, "general": {"Profit Factor": 1.25}},\n        "pnl": {"USD": {"PnL (total)": 10.0}},\n    }\n    scaled = {**evidence, "pnl": {"USD": {"PnL (total)": 10_000_000.0}}}\n    score, model = _discovery_quality_score(evidence, search_attempt_count=5)\n    scaled_score, scaled_model = _discovery_quality_score(scaled, search_attempt_count=5)\n    assert scaled_score == score\n    assert model["model"] == "DISCOVERY_PUBLIC_PERFORMANCE_V2"\n    assert model["absolute_pnl_used_for_scoring"] is False\n    assert scaled_model["absolute_pnl_used_for_scoring"] is False\n\n\ndef test_sealed_dataset_must_use_distinct_catalog_and_nonoverlapping_time() -> None:\n    universe_id = uuid4()\n    start = datetime(2024, 1, 1, tzinfo=UTC)\n    source = _revision(\n        partition="DISCOVERY",\n        catalog_uri="nautilus-catalog://discovery",\n        start=start,\n        end=start + timedelta(days=10),\n        universe_id=universe_id,\n    )\n    same_catalog = _revision(\n        partition="SEALED",\n        catalog_uri=source.catalog_uri or "",\n        start=start + timedelta(days=10),\n        end=start + timedelta(days=20),\n        universe_id=universe_id,\n    )\n    with pytest.raises(QfError) as same_error:\n        _select_sealed_dataset(_DatasetSession(same_catalog), source, same_catalog.id)\n    assert same_error.value.code == "SEALED_DATASET_NOT_INDEPENDENT"\n\n    overlap = _revision(\n        partition="SEALED",\n        catalog_uri="nautilus-catalog://sealed",\n        start=start + timedelta(days=5),\n        end=start + timedelta(days=15),\n        universe_id=universe_id,\n    )\n    with pytest.raises(QfError) as overlap_error:\n        _select_sealed_dataset(_DatasetSession(overlap), source, overlap.id)\n    assert overlap_error.value.code == "SEALED_DATASET_TIME_OVERLAP"\n\n\ndef test_evidence_lineage_includes_inherited_programs() -> None:\n    charter_id = uuid4()\n    root = ResearchProgram(\n        id=uuid4(),\n        charter_id=charter_id,\n        title="root",\n        state="ACTIVE",\n        evidence_inherited_from_program_id=None,\n    )\n    child = ResearchProgram(\n        id=uuid4(),\n        charter_id=charter_id,\n        title="child",\n        state="ACTIVE",\n        evidence_inherited_from_program_id=root.id,\n    )\n    programs = {root.id: root, child.id: child}\n\n    class _ProgramSession:\n        def get(self, model, identity):\n            del model\n            return programs.get(identity)\n\n    assert _evidence_program_lineage_ids(_ProgramSession(), child.id) == {child.id, root.id}\n\n\ndef test_degradation_state_normalization_and_rejected_activity_contract() -> None:\n    assert _is_explicit_degradation({"degradation_state": "  degraded  "}) is True\n    activity = WorkspaceExperimentActivity()\n    assert activity.has_activity is False\n    activity.rejected_contract_count += 1\n    assert activity.has_activity is True\n''', encoding="utf-8")

# The patch machinery must not remain in the product branch.
Path("tools/issue22_round5_patch.py").unlink(missing_ok=True)
Path(".github/workflows/issue22-round5-maintenance.yml").unlink(missing_ok=True)
