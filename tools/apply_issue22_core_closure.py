from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def write(relative: str, content: str) -> None:
    (ROOT / relative).write_text(content, encoding="utf-8")


def replace_once(relative: str, old: str, new: str) -> None:
    content = read(relative)
    count = content.count(old)
    if count != 1:
        raise RuntimeError(
            f"{relative}: expected one replacement, found {count}: {old[:120]!r}"
        )
    write(relative, content.replace(old, new, 1))


def replace_all(relative: str, old: str, new: str, *, minimum: int = 1) -> None:
    content = read(relative)
    count = content.count(old)
    if count < minimum:
        raise RuntimeError(
            f"{relative}: expected at least {minimum} replacements, found {count}: {old!r}"
        )
    write(relative, content.replace(old, new))


# Concrete Research Charter horizons and the governed browser-test dataset.
replace_once(
    "backend/src/api/domain.py",
    '''def _infer_horizon(idea: str) -> str:
    match = re.search(r"\\b(\\d+)\\s*(minute|min|hour|hr|day|d|h|m)s?\\b", idea.lower())
    if not match:
        return "System inferred"
    number, unit = match.groups()
    return f"{number}{unit[0].upper()}"
''',
    '''def _infer_horizon(idea: str) -> str:
    match = re.search(r"\\b(\\d+)\\s*(minute|min|hour|hr|day|d|h|m)s?\\b", idea.lower())
    if not match:
        # A frozen Charter must never carry an unresolved sentinel into
        # qualification. Daily is the explicit V1 default when the Idea does
        # not state a horizon; users can state another concrete horizon.
        return "1D"
    number, unit = match.groups()
    return f"{number}{unit[0].upper()}"
''',
)
replace_once(
    "backend/src/api/domain.py",
    'objective=f"Test the Charter hypothesis within {preview.market_scope}.",',
    'objective=f"Test the Charter hypothesis within {market_scope}.",',
)

replace_once(
    "backend/tests/frontend_e2e_seed.py",
    '''UNIVERSE_ID = UUID("10000000-0000-0000-0000-000000000001")
MANDATE_ID = UUID("20000000-0000-0000-0000-000000000001")
''',
    '''UNIVERSE_ID = UUID("10000000-0000-0000-0000-000000000001")
DATA_SOURCE_ID = UUID("10000000-0000-0000-0000-000000000002")
DISCOVERY_DATASET_ID = UUID("10000000-0000-0000-0000-000000000003")
MANDATE_ID = UUID("20000000-0000-0000-0000-000000000001")
''',
)
replace_all("backend/tests/frontend_e2e_seed.py", "EUR/USD.SIM", "AAPL.XNAS", minimum=5)
replace_once(
    "backend/tests/frontend_e2e_seed.py",
    '''            "dataset_revision_ids": [],
''',
    '''            "dataset_revision_ids": [str(DISCOVERY_DATASET_ID)],
''',
)
replace_once(
    "backend/tests/frontend_e2e_seed.py",
    '''            "risk_config": {"bypass": False},
''',
    '''            "risk_config": {},
''',
)
replace_once(
    "backend/tests/frontend_e2e_seed.py",
    '''        mandate = PortfolioMandate(
''',
    '''        data_source = GovernedDataSource(
            id=DATA_SOURCE_ID,
            name="Seeded executable PIT quotes",
            provider="CI generated fixture",
            state="ACTIVE",
            universe_scope=["US Equities"],
            fields=[
                "timestamp",
                "available_at",
                "bid_price",
                "ask_price",
                "volume",
            ],
            update_cadence="STATIC_FIXTURE",
            preflight_state="READY",
            public_config={"data_domains": ["quotes", "market_data"]},
        )
        discovery = DatasetRevision(
            id=DISCOVERY_DATASET_ID,
            data_source_id=DATA_SOURCE_ID,
            universe_version_id=UNIVERSE_ID,
            universe_name="US Equities",
            revision_no=1,
            schema_version="nautilus.quote_tick.v2",
            event_start=now - timedelta(days=30),
            event_end=now - timedelta(days=1),
            available_start=now - timedelta(days=30) + timedelta(seconds=2),
            available_end=now - timedelta(days=1) + timedelta(seconds=2),
            row_count=360,
            quality_state="VALID",
            point_in_time_state="VALID",
            partition="DISCOVERY",
            created_at=now,
            provider_name="CI generated fixture",
            source_license="CC0-1.0",
            catalog_uri="nautilus-catalog://frontend-e2e",
            nautilus_data_type="QuoteTick",
            instrument_scope=["AAPL.XNAS"],
            schema_revision="nautilus.quote_tick.v2",
            quality_result={"state": "VALID", "sorted": True},
            point_in_time_result={
                "state": "VALID",
                "replay_order": "TS_INIT",
                "event_time_preserved": True,
                "availability_time_preserved": True,
            },
            ingested_at=now,
        )
        mandate = PortfolioMandate(
''',
)
replace_once(
    "backend/tests/frontend_e2e_seed.py",
    '''        session.add_all([universe, mandate, paper, live, portfolio_program])
''',
    '''        session.add_all(
            [universe, data_source, discovery, mandate, paper, live, portfolio_program]
        )
''',
)

replace_once(
    "frontend/e2e/smoke.spec.ts",
    '''  await expect(page.getByText(/post-earnings drift in liquid US equities/i)).toBeVisible();
''',
    '''  await expect(
    page
      .getByText('Test post-earnings drift in liquid US equities after realistic costs.', {
        exact: true,
      })
      .last(),
  ).toBeVisible();
''',
)
replace_once(
    "frontend/e2e/smoke.spec.ts",
    '''test('Flow 3: create datasource -> readiness update', async ({ page }) => {
  await page.goto('/admin');
  const researchReady = page.locator('.qz-kpi').filter({ hasText: 'Research ready' });
  await expect(researchReady).toContainText('NO');
''',
    '''test('Flow 3: register an additional datasource while readiness stays healthy', async ({ page }) => {
  await page.goto('/admin');
  const researchReady = page.locator('.qz-kpi').filter({ hasText: 'Research ready' });
  await expect(researchReady).toContainText('YES');
''',
)
replace_once(
    "frontend/e2e/smoke.spec.ts",
    "  await dialog.getByLabel('Name').fill('Primary PIT Data');\n",
    "  await dialog.getByLabel('Name').fill('Supplemental PIT Data');\n",
)
replace_once(
    "frontend/e2e/smoke.spec.ts",
    "  await expect(page.getByText('Primary PIT Data')).toBeVisible();\n",
    "  await expect(page.getByText('Supplemental PIT Data')).toBeVisible();\n",
)


# Protocol parity: timezone-aware inputs and no silently ignored config.
replace_once(
    "backend/src/quant_runtime/contracts.py",
    '''class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class ExperimentMode(StrEnum):
''',
    '''class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


def _require_aware_datetime(value: datetime, *, field_name: str) -> None:
    if value.tzinfo is None or value.utcoffset() is None:
        raise ValueError(f"{field_name} must be timezone-aware")


class ExperimentMode(StrEnum):
''',
)
replace_once(
    "backend/src/quant_runtime/contracts.py",
    '''    @model_validator(mode="after")
    def validate_availability(self) -> QuoteRow:
        if self.available_at < self.timestamp:
            raise ValueError("available_at cannot precede the market event timestamp")
        return self
''',
    '''    @model_validator(mode="after")
    def validate_availability(self) -> QuoteRow:
        _require_aware_datetime(self.timestamp, field_name="timestamp")
        _require_aware_datetime(self.available_at, field_name="available_at")
        if self.available_at < self.timestamp:
            raise ValueError("available_at cannot precede the market event timestamp")
        return self
''',
)
replace_once(
    "backend/src/quant_runtime/contracts.py",
    '''    risk_config: dict[str, Any] = Field(default_factory=dict)
    tags: dict[str, str] = Field(default_factory=dict)


class OrderEvidence(StrictModel):
''',
    '''    risk_config: dict[str, Any] = Field(default_factory=dict)
    tags: dict[str, str] = Field(default_factory=dict)

    @model_validator(mode="after")
    def validate_v1_configuration(self) -> BacktestExperimentRequest:
        if self.start_time is not None:
            _require_aware_datetime(self.start_time, field_name="start_time")
        if self.end_time is not None:
            _require_aware_datetime(self.end_time, field_name="end_time")
        if (
            self.start_time is not None
            and self.end_time is not None
            and self.start_time >= self.end_time
        ):
            raise ValueError("start_time must precede end_time")
        if self.data_config:
            raise ValueError(
                "data_config is reserved until protocol v1 explicitly applies its fields; "
                "use the top-level catalog/instrument/time contract instead"
            )
        if self.risk_config:
            raise ValueError(
                "risk_config is reserved until protocol v1 explicitly applies a Nautilus "
                "RiskEngine configuration"
            )
        return self


class OrderEvidence(StrictModel):
''',
)


# Candidate Bundle lineage must preserve production identifiers and all assets.
replace_once(
    "backend/src/candidate_bundles.py",
    '''def _member_payload(candidate: Any) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for index, member in enumerate(list(_value(candidate, "members", []) or [])):
        rows.append(
            {
                "alpha_id": str(_value(member, "alpha_id", "")) or None,
                "instrument_id": str(
                    _first(
                        member,
                        ("instrument_id", "symbol", "asset", "id"),
                        f"member-{index + 1}",
                    )
                ),
                "target_weight": str(_first(member, ("target_weight", "weight"), "0")),
            }
        )
    return sorted(rows, key=lambda row: (row["instrument_id"], row["alpha_id"] or ""))
''',
    '''def _member_payload(candidate: Any) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
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
        for instrument_id in normalized_instruments:
            rows.append(
                {
                    "alpha_qualification_id": str(alpha_id),
                    "instrument_id": instrument_id,
                    "target_weight": target_weight,
                }
            )
    return sorted(
        rows,
        key=lambda row: (
            row["instrument_id"],
            row["alpha_qualification_id"],
        ),
    )
''',
)
replace_once(
    "backend/tests/unit/test_candidate_bundles.py",
    '''        "validation/expected-orders.json",
        "validation/expected-positions.json",
''',
    '''        "validation/expected-orders.json",
        "validation/expected-fills.json",
        "validation/expected-positions.json",
''',
)
candidate_tests = read("backend/tests/unit/test_candidate_bundles.py")
if "test_bundle_preserves_production_alpha_lineage_and_every_instrument" not in candidate_tests:
    candidate_tests = candidate_tests.rstrip() + '''


def test_bundle_preserves_production_alpha_lineage_and_every_instrument() -> None:
    candidate = _candidate()
    alpha_id = uuid4()
    candidate.members = [
        {
            "alpha_qualification_id": str(alpha_id),
            "instrument_ids": ["AAPL.XNAS", "MSFT.XNAS"],
            "target_weight": 1.0,
        }
    ]  # type: ignore[assignment]
    built = build_candidate_bundle(object(), candidate=candidate)
    assert built.manifest["target_weights"] == [
        {
            "alpha_qualification_id": str(alpha_id),
            "instrument_id": "AAPL.XNAS",
            "target_weight": "1.0",
        },
        {
            "alpha_qualification_id": str(alpha_id),
            "instrument_id": "MSFT.XNAS",
            "target_weight": "1.0",
        },
    ]
'''
    write("backend/tests/unit/test_candidate_bundles.py", candidate_tests + "\n")


# Promotion gates: concrete horizon, sealed bounds, roles, identity, materiality.
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''CATALOG_URI_PREFIX = "nautilus-catalog://"


def _now() -> datetime:
''',
    '''CATALOG_URI_PREFIX = "nautilus-catalog://"
_UNRESOLVED_HORIZONS = {
    "",
    "system inferred",
    "system_inferred",
    "unknown",
    "unspecified",
    "tbd",
}
_RECOGNIZED_ALPHA_ROLES = {
    "PRIMARY_ALPHA",
    "DIVERSIFIER_ALPHA",
    "HEDGE_ALPHA",
    "REGIME_SIGNAL",
    "RISK_MODULATOR",
    "SHADOW_ALPHA",
}
_PROMOTABLE_ALPHA_ROLES = _RECOGNIZED_ALPHA_ROLES - {"SHADOW_ALPHA"}


def _now() -> datetime:
''',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''    evidence = entry.evidence_json or {}
    missing = [
''',
    '''    evidence = entry.evidence_json or {}
    if str(evidence.get("experiment_id", "")) != str(entry.id):
        raise QfError(
            "NAUTILUS_EVIDENCE_IDENTITY_MISMATCH",
            "Nautilus evidence is not bound to its Search Ledger experiment id.",
            422,
        )
    if str(evidence.get("mode", "")) != entry.mode:
        raise QfError(
            "NAUTILUS_EVIDENCE_MODE_MISMATCH",
            "Nautilus evidence mode does not match its Search Ledger entry.",
            422,
        )
    missing = [
''',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''        source_horizon = (charter.prediction_horizon or "").strip()
        if not source_horizon:
            raise QfError(
                "ALPHA_HORIZON_MISSING",
                "Alpha Qualification requires a frozen non-empty prediction horizon.",
                422,
            )
''',
    '''        source_horizon = (charter.prediction_horizon or "").strip()
        if source_horizon.casefold().replace("-", "_") in _UNRESOLVED_HORIZONS:
            raise QfError(
                "ALPHA_HORIZON_UNRESOLVED",
                "Alpha Qualification requires a concrete frozen prediction horizon.",
                422,
            )
''',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''    return dataset


def qualify_alpha(
''',
    '''    return dataset


def _sealed_dataset_bounds(dataset: DatasetRevision) -> tuple[datetime, datetime]:
    start = dataset.event_start
    end = dataset.event_end
    if start is None or end is None:
        raise QfError(
            "SEALED_DATASET_TIME_RANGE_MISSING",
            "Sealed evaluation requires explicit immutable event-time bounds.",
            422,
        )
    if (
        start.tzinfo is None
        or start.utcoffset() is None
        or end.tzinfo is None
        or end.utcoffset() is None
        or start >= end
    ):
        raise QfError(
            "SEALED_DATASET_TIME_RANGE_INVALID",
            "Sealed Dataset Revision has invalid event-time bounds.",
            422,
        )
    return start, end


def qualify_alpha(
''',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''        sealed_dataset = _select_sealed_dataset(session, source_dataset, sealed_dataset_revision_id)
        sealed_request = source_request.model_copy(
            update={
                "experiment_id": uuid4(),
                "mode": ExperimentMode.SEALED,
                "dataset_revision_id": sealed_dataset.id,
                "catalog_key": _catalog_key(sealed_dataset),
            }
        )
''',
    '''        sealed_dataset = _select_sealed_dataset(session, source_dataset, sealed_dataset_revision_id)
        sealed_start, sealed_end = _sealed_dataset_bounds(sealed_dataset)
        sealed_request = source_request.model_copy(
            update={
                "experiment_id": uuid4(),
                "mode": ExperimentMode.SEALED,
                "dataset_revision_id": sealed_dataset.id,
                "catalog_key": _catalog_key(sealed_dataset),
                "start_time": sealed_start,
                "end_time": sealed_end,
            }
        )
''',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''def _alpha_score(alpha: AlphaQualification) -> float:
    try:
        return float((alpha.metrics or {}).get("search_adjusted_quality", 0.0))
    except (TypeError, ValueError):
        return 0.0


def _load_portfolio_source(
''',
    '''def _alpha_score(alpha: AlphaQualification) -> float:
    try:
        return float((alpha.metrics or {}).get("search_adjusted_quality", 0.0))
    except (TypeError, ValueError):
        return 0.0


def _canonical_alpha_ids(alpha_ids: list[UUID]) -> list[UUID]:
    return sorted(set(alpha_ids), key=str)


def _candidate_quality(candidate: PortfolioCandidate) -> float:
    raw = (candidate.metrics or {}).get("search_adjusted_quality")
    try:
        value = float(raw)
    except (TypeError, ValueError) as exc:
        raise QfError(
            "CANDIDATE_BASELINE_EVIDENCE_MISSING",
            "Current Candidate lacks a numeric search-adjusted quality baseline.",
            422,
        ) from exc
    if value != value or value in {float("inf"), float("-inf")}:
        raise QfError(
            "CANDIDATE_BASELINE_EVIDENCE_INVALID",
            "Current Candidate quality baseline must be finite.",
            422,
        )
    return value


def _material_improvement_delta(mandate: PortfolioMandate) -> float:
    spec = mandate.spec_json or {}
    configured = spec.get("material_improvement_gate", {})
    if configured is None:
        configured = {}
    if not isinstance(configured, dict):
        raise QfError(
            "MATERIAL_IMPROVEMENT_POLICY_INVALID",
            "material_improvement_gate must be an object.",
            422,
        )
    raw = configured.get(
        "min_search_adjusted_quality_delta",
        spec.get("min_search_adjusted_quality_delta", 0.0),
    )
    delta = _number(raw, key="min_search_adjusted_quality_delta")
    if delta < 0:
        raise QfError(
            "MATERIAL_IMPROVEMENT_POLICY_INVALID",
            "Material Improvement delta cannot be negative.",
            422,
        )
    return delta


def _require_material_improvement(
    current: PortfolioCandidate | None,
    *,
    proposed_quality: float,
    mandate: PortfolioMandate,
) -> None:
    if current is None:
        return
    baseline = _candidate_quality(current)
    minimum_delta = _material_improvement_delta(mandate)
    required = baseline + minimum_delta
    if proposed_quality <= required:
        raise QfError(
            "CANDIDATE_NOT_MATERIALLY_IMPROVED",
            "Simulation evidence does not materially improve the current Candidate.",
            422,
            {
                "current_quality": baseline,
                "proposed_quality": proposed_quality,
                "minimum_delta": minimum_delta,
                "required_quality_exclusive": required,
            },
        )


def _load_portfolio_source(
''',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''    if len(alphas) != len(set(alpha_ids)):
        raise QfError(
            "ALPHA_NOT_PORTFOLIO_READY",
            "Every selected Alpha must be active, healthy, and qualified.",
            422,
        )
    selected = max(alphas, key=lambda item: (_alpha_score(item), str(item.id)))
''',
    '''    if len(alphas) != len(set(alpha_ids)):
        raise QfError(
            "ALPHA_NOT_PORTFOLIO_READY",
            "Every selected Alpha must be active, healthy, and qualified.",
            422,
        )
    invalid_roles = sorted(
        {item.role for item in alphas if item.role not in _RECOGNIZED_ALPHA_ROLES}
    )
    if invalid_roles:
        raise QfError(
            "ALPHA_ROLE_INVALID",
            "Portfolio promotion encountered an unrecognized Alpha role.",
            422,
            {"roles": invalid_roles},
        )
    if any(item.role == "SHADOW_ALPHA" for item in alphas):
        raise QfError(
            "SHADOW_ALPHA_NOT_HANDOFF_ELIGIBLE",
            "Shadow Alpha cannot directly form a promotable Handoff Candidate.",
            422,
        )
    if any(item.role not in _PROMOTABLE_ALPHA_ROLES for item in alphas):
        raise QfError(
            "ALPHA_ROLE_NOT_PROMOTABLE",
            "Selected Alpha role is not eligible for Candidate promotion.",
            422,
        )
    selected = max(alphas, key=lambda item: (_alpha_score(item), str(item.id)))
''',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''    """Optimize Alpha selection, run a real PORTFOLIO simulation, and freeze approval facts."""
    with factory() as session:
''',
    '''    """Optimize Alpha selection, run a real PORTFOLIO simulation, and freeze approval facts."""
    requested_alpha_ids = _canonical_alpha_ids(alpha_ids)
    if not requested_alpha_ids:
        raise QfError("ALPHA_SELECTION_EMPTY", "At least one qualified Alpha is required.", 422)
    with factory() as session:
''',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''        if simulation_experiment_id is not None:
            existing_candidate = session.scalar(
                select(PortfolioCandidate).where(
                    PortfolioCandidate.simulation_experiment_id == simulation_experiment_id
                )
            )
            if existing_candidate is not None:
                existing_approval = session.scalar(
                    select(ApprovalSnapshot)
                    .where(ApprovalSnapshot.candidate_id == existing_candidate.id)
                    .order_by(ApprovalSnapshot.created_at.desc())
                    .limit(1)
                )
                if existing_approval is None:
                    raise QfError(
                        "CANDIDATE_APPROVAL_MISSING",
                        "Idempotent Candidate exists without its Approval Snapshot.",
                        500,
                    )
                selected = existing_candidate.members[0] if existing_candidate.members else {}
                try:
                    selected_alpha_id = UUID(str(selected.get("alpha_qualification_id")))
                except (TypeError, ValueError) as exc:
                    raise QfError(
                        "CANDIDATE_ALPHA_LINEAGE_MISSING",
                        "Idempotent Candidate lost its selected Alpha lineage.",
                        500,
                    ) from exc
                return CandidatePromotion(
                    candidate_id=existing_candidate.id,
                    approval_id=existing_approval.id,
                    simulation_experiment_id=simulation_experiment_id,
                    selected_alpha_id=selected_alpha_id,
                )
        alpha, source, request = _load_portfolio_source(session, alpha_ids)
''',
    '''        if simulation_experiment_id is not None:
            existing_candidate = session.scalar(
                select(PortfolioCandidate).where(
                    PortfolioCandidate.simulation_experiment_id == simulation_experiment_id
                )
            )
            if existing_candidate is not None:
                optimizer = (existing_candidate.metrics or {}).get("optimizer", {})
                stored_ids: list[UUID] = []
                for raw_id in optimizer.get("considered_alpha_ids", []):
                    try:
                        stored_ids.append(UUID(str(raw_id)))
                    except (TypeError, ValueError) as exc:
                        raise QfError(
                            "CANDIDATE_ALPHA_LINEAGE_MISSING",
                            "Idempotent Candidate lost its considered Alpha lineage.",
                            500,
                        ) from exc
                if (
                    existing_candidate.portfolio_program_id != portfolio_program_id
                    or _canonical_alpha_ids(stored_ids) != requested_alpha_ids
                ):
                    raise QfError(
                        "CANDIDATE_SIMULATION_IDENTITY_MISMATCH",
                        "Simulation id is bound to a different Portfolio/Alpha contract.",
                        409,
                    )
                existing_approval = session.scalar(
                    select(ApprovalSnapshot)
                    .where(ApprovalSnapshot.candidate_id == existing_candidate.id)
                    .order_by(ApprovalSnapshot.created_at.desc())
                    .limit(1)
                )
                if existing_approval is None:
                    raise QfError(
                        "CANDIDATE_APPROVAL_MISSING",
                        "Idempotent Candidate exists without its Approval Snapshot.",
                        500,
                    )
                selected = existing_candidate.members[0] if existing_candidate.members else {}
                try:
                    selected_alpha_id = UUID(str(selected.get("alpha_qualification_id")))
                except (TypeError, ValueError) as exc:
                    raise QfError(
                        "CANDIDATE_ALPHA_LINEAGE_MISSING",
                        "Idempotent Candidate lost its selected Alpha lineage.",
                        500,
                    ) from exc
                return CandidatePromotion(
                    candidate_id=existing_candidate.id,
                    approval_id=existing_approval.id,
                    simulation_experiment_id=simulation_experiment_id,
                    selected_alpha_id=selected_alpha_id,
                )
        alpha, source, request = _load_portfolio_source(session, requested_alpha_ids)
''',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''        _validate_mandate_after_simulation(current_constraints, simulation_evidence)
        request_json = BacktestExperimentRequest.model_validate(simulation.request_json)
''',
    '''        _validate_mandate_after_simulation(current_constraints, simulation_evidence)
        current_candidate = (
            session.get(PortfolioCandidate, portfolio_program.current_candidate_id)
            if portfolio_program.current_candidate_id is not None
            else None
        )
        _require_material_improvement(
            current_candidate,
            proposed_quality=alpha_quality,
            mandate=persisted_mandate,
        )
        request_json = BacktestExperimentRequest.model_validate(simulation.request_json)
''',
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    '''                    "considered_alpha_ids": [str(item) for item in alpha_ids],
''',
    '''                    "considered_alpha_ids": [str(item) for item in requested_alpha_ids],
''',
)

# Bind remote results to the immutable Search Ledger request before persistence.
replace_once(
    "backend/src/quant_runtime/ledger.py",
    '''                else:
                    result = runtime.run_backtest(request)
        except Exception as exc:
''',
    '''                else:
                    result = runtime.run_backtest(request)
            expected_mode = ExperimentMode.SEALED if sealed else request.mode
            if result.experiment_id != request.experiment_id or result.mode != expected_mode:
                raise QfError(
                    "NAUTILUS_RUNTIME_RESULT_IDENTITY_MISMATCH",
                    "Remote result does not match the immutable experiment id and mode.",
                    502,
                    {
                        "expected_experiment_id": str(request.experiment_id),
                        "received_experiment_id": str(result.experiment_id),
                        "expected_mode": expected_mode.value,
                        "received_mode": str(result.mode),
                    },
                )
        except Exception as exc:
''',
)

# Focused regression tests for unresolved horizons and Shadow Alpha.
replace_once(
    "backend/tests/integration/test_nautilus_promotion.py",
    '''from sqlalchemy import Engine, select

from db.models import (
''',
    '''import pytest
from sqlalchemy import Engine, select

from db.models import (
''',
)
replace_once(
    "backend/tests/integration/test_nautilus_promotion.py",
    '''from db.session import create_session_factory
''',
    '''from db.session import create_session_factory
from errors import QfError
''',
)
promotion_tests = read("backend/tests/integration/test_nautilus_promotion.py")
if "test_unresolved_horizon_cannot_qualify" not in promotion_tests:
    promotion_tests = promotion_tests.rstrip() + '''


def test_unresolved_horizon_cannot_qualify(
    engine: Engine,
    monkeypatch,
) -> None:
    factory = create_session_factory(engine)
    source_id, sealed_dataset_id, _ = _seed(engine)
    with factory() as session, session.begin():
        source = session.get(SearchLedgerEntry, source_id)
        assert source is not None
        program = session.get(ResearchProgram, source.program_id)
        assert program is not None
        charter = session.get(ResearchCharter, program.charter_id)
        assert charter is not None
        charter.prediction_horizon = "System inferred"

    def should_not_execute(*args, **kwargs):
        raise AssertionError("sealed runtime must not run for unresolved horizons")

    monkeypatch.setattr(ExperimentCoordinator, "execute", should_not_execute)
    with pytest.raises(QfError) as raised:
        qualify_alpha(
            factory,
            source_experiment_id=source_id,
            sealed_dataset_revision_id=sealed_dataset_id,
        )
    assert raised.value.code == "ALPHA_HORIZON_UNRESOLVED"


def test_shadow_alpha_cannot_form_candidate(engine: Engine) -> None:
    factory = create_session_factory(engine)
    source_id, _, portfolio_program_id = _seed(engine)
    with factory() as session, session.begin():
        source = session.get(SearchLedgerEntry, source_id)
        assert source is not None
        dataset = session.get(DatasetRevision, source.dataset_revision_id)
        assert dataset is not None
        alpha = AlphaQualification(
            program_id=source.program_id,
            universe_version_id=dataset.universe_version_id,
            universe="FX",
            horizon="1D",
            role="SHADOW_ALPHA",
            state="ACTIVE",
            name="shadow",
            degradation_state="HEALTHY",
            metrics={
                "search_adjusted_quality": 0.8,
                "sealed_disclosure": {"passed": True},
            },
            lineage=[],
            scope_json={},
            created_at=datetime.now(UTC),
            source_experiment_id=source.id,
        )
        session.add(alpha)
        session.flush()
        alpha_id = alpha.id
    with pytest.raises(QfError) as raised:
        simulate_portfolio_candidate(
            factory,
            portfolio_program_id=portfolio_program_id,
            alpha_ids=[alpha_id],
        )
    assert raised.value.code == "SHADOW_ALPHA_NOT_HANDOFF_ELIGIBLE"
'''
    write("backend/tests/integration/test_nautilus_promotion.py", promotion_tests + "\n")


# Remove the contradictory pre-Issue-22 package contract from the fact source.
design = read("DESIGN.md")
design = design.replace(
    "当前状态：**目标方案已锁定；现有代码仍包含旧 Nautilus 执行控制路径，尚未 conforming / release-ready**",
    "当前状态：**远程 Nautilus-first 边界是当前实现基线；完成状态以最终 CI 与独立审查为准**",
)
old_section = '''## 24. Candidate Package

V1 标准格式：

```text
candidate-package/
  manifest.json
  schemas/
  runtime/
    feature_pipeline.whl
    alpha_model.whl
    calibration.whl
    portfolio_policy.whl
  fixtures/
    input.arrow
    expected_alpha.arrow
    expected_portfolio.arrow
  evidence/
    approval-summary.json
  lineage.json
```

Python Reference Runtime 是正式参考实现：

```text
canonical input
→ Feature Pipeline
→ Alpha
→ Calibration
→ Portfolio Policy
→ TargetPortfolioFrame
```

它不连接行情源、broker 或 wallet，不提交订单。

Package 禁止包含：broker URL、API key、private key、account ID、order type、TIF、order id、recovery、heartbeat 或 execution retry。

QZ 不为 Package 创建应用级 hash/checksum/fingerprint。完整性与兼容性依赖：显式 artifact ID/version、文件名/长度、wheel/package metadata、schema validation、Reference Fixture 执行结果与 contract version。
'''
new_section = '''## 24. Candidate Bundle v2

Issue 22 起，唯一标准交付物是 Nautilus-native `Candidate Bundle v2`。旧的自定义
Feature/Alpha/Calibration/Portfolio 四轮 Package 合同已经废止，不再是实现或验收依据。

```text
candidate-bundle/
  manifest.json
  requirements.lock
  strategy/
    strategy.whl
    strategy-config.json
    actor-config.json
  data/
    requirements.json
    instrument-scope.json
    custom-data-schemas/
  runtime/
    nautilus-version.json
    backtest-run-config.json
    venue-config.json
    risk-config.json
    live-node-template.json
  validation/
    fixture-catalog/
    expected-orders.json
    expected-fills.json
    expected-positions.json
    expected-statistics.json
  evidence/
    discovery-summary.json
    sealed-summary.json
    robustness-summary.json
    portfolio-simulation.json
  lineage.json
```

Bundle 冻结研究、Sealed Evaluation 与 Portfolio simulation 使用的同一
`strategy.whl`、配置、Dataset/Alpha/Experiment lineage 和真实验证证据。独立 Paper/Live
Runtime 复用该 wheel 与配置；broker adapter、账户、凭据和任何可执行订单指令只属于下游
Runtime，绝不进入 Bundle 或 QuaZonai Core。

QZ 不为 Bundle 创建应用级 hash/checksum/fingerprint。兼容性依赖显式 UUID/version、
wheel metadata、schema validation、隔离的 conformance 执行结果与 contract version。
'''
if old_section not in design:
    raise RuntimeError("DESIGN.md: old Candidate Package section not found")
design = design.replace(old_section, new_section, 1)
design = design.replace(
    "截至本基线，仓库代码仍主要实现旧的 QZ+Nautilus execution control-plane。它与本文冲突，不能称为 conforming。",
    "当前实现以本节的远程 Nautilus-first 研究控制平面为基线；旧 execution control-plane 不再保留兼容路径。",
)
for old, new in (
    ("CandidatePackage", "CandidateBundle"),
    ("candidate_packages", "candidate_bundles"),
    ("candidate_package_id", "candidate_bundle_id"),
    ("Candidate Package", "Candidate Bundle"),
    ("candidate-package", "candidate-bundle"),
):
    design = design.replace(old, new)
write("DESIGN.md", design)

for relative in (
    "AGENTS.md",
    "README.md",
    "OPERATIONS.md",
    "CLI.md",
    "docs/ADR-0022-REMOTE-NAUTILUS-RUNTIME.md",
):
    content = read(relative)
    for old, new in (
        ("CandidatePackage", "CandidateBundle"),
        ("candidate_packages", "candidate_bundles"),
        ("candidate_package_id", "candidate_bundle_id"),
        ("Candidate Package", "Candidate Bundle"),
        ("candidate-package", "candidate-bundle"),
    ):
        content = content.replace(old, new)
    write(relative, content)

agents = read("AGENTS.md")
agents = agents.replace(
    "- Candidate Bundle 只输出 `TargetPortfolioFrame`，不输出订单；",
    "- Candidate Bundle 可包含历史/验证用订单、成交与持仓证据，但不得包含可执行订单指令、broker adapter 或凭据；",
)
write("AGENTS.md", agents)
