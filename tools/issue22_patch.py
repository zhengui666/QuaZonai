from pathlib import Path

def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")

def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")

def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    if old not in text:
        raise SystemExit(f"pattern missing in {path}: {old[:180]!r}")
    write(path, text.replace(old, new, 1))

ledger = "backend/src/quant_runtime/ledger.py"
old_execute_prefix = '''        with self._factory() as session, session.begin():
            existing = session.get(SearchLedgerEntry, request.experiment_id)
            if existing is not None:
                if not _same_identity(
                    existing,
                    mission_id=mission_id,
                    program_id=program_id,
                    branch_id=branch_id,
                    parent_entry_id=parent_entry_id,
                    request=request,
                    sealed=sealed,
                ):
                    raise QfError(
                        "EXPERIMENT_ID_REUSED",
                        "Experiment id is already bound to a different immutable contract or lineage.",
                        409,
                    )
                if existing.state == "RUNNING":
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

            dataset = session.get(DatasetRevision, request.dataset_revision_id)
            self._validate_dataset(dataset, request=request, sealed=sealed)
            if mission_id is not None:
                mission = session.get(ResearchMission, mission_id)
                if mission is None or mission.program_id != program_id:
                    raise QfError(
                        "MISSION_NOT_FOUND",
                        "Experiment Mission does not exist in the requested Program.",
                        404,
                    )

            if parent_entry_id is not None:
                parent = session.execute(
                    select(SearchLedgerEntry)
                    .where(SearchLedgerEntry.id == parent_entry_id)
                    .with_for_update()
                ).scalar_one_or_none()
                program_lineage = _evidence_program_lineage(session, program_id)
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
                    if exposure is not None:
                        raise QfError(
                            "SEALED_EXPOSURE_ALREADY_CONSUMED",
                            "This source experiment already has a sealed evaluation exposure.",
                            409,
                            {"sealed_experiment_id": str(exposure.id)},
                        )

            entry = SearchLedgerEntry(
                id=request.experiment_id,
                program_id=program_id,
                branch_id=branch_id,
                mission_id=mission_id,
                dataset_revision_id=request.dataset_revision_id,
                parent_entry_id=parent_entry_id,
                mode=ExperimentMode.SEALED.value if sealed else request.mode.value,
                state="RUNNING",
                runtime_name="NAUTILUS_TRADER",
                runtime_version=None,
                request_json=_normalized_request(request),
                evidence_json={},
                disclosure_json={},
                started_at=_now(),
                finished_at=None,
                failure_code=None,
                failure_message=None,
            )
            session.add(entry)
'''
new_execute_prefix = '''        recovered_existing = False
        with self._factory() as session, session.begin():
            existing = session.get(SearchLedgerEntry, request.experiment_id)
            if existing is not None:
                if not _same_identity(
                    existing,
                    mission_id=mission_id,
                    program_id=program_id,
                    branch_id=branch_id,
                    parent_entry_id=parent_entry_id,
                    request=request,
                    sealed=sealed,
                ):
                    raise QfError(
                        "EXPERIMENT_ID_REUSED",
                        "Experiment id is already bound to a different immutable contract or lineage.",
                        409,
                    )
                if existing.state != "RUNNING":
                    session.expunge(existing)
                    return existing
                stale_before = _now() - timedelta(minutes=35)
                if existing.started_at is None or existing.started_at >= stale_before:
                    raise QfError(
                        "EXPERIMENT_ALREADY_RUNNING",
                        "The exact experiment is already running.",
                        409,
                    )
                dataset = session.get(DatasetRevision, request.dataset_revision_id)
                self._validate_dataset(dataset, request=request, sealed=sealed)
                existing.started_at = _now()
                existing.finished_at = None
                existing.runtime_version = None
                existing.remote_run_id = None
                existing.evidence_json = {}
                existing.disclosure_json = {}
                existing.failure_code = None
                existing.failure_message = None
                recovered_existing = True
                session.flush()

            if not recovered_existing:
                dataset = session.get(DatasetRevision, request.dataset_revision_id)
                self._validate_dataset(dataset, request=request, sealed=sealed)
                if mission_id is not None:
                    mission = session.get(ResearchMission, mission_id)
                    if mission is None or mission.program_id != program_id:
                        raise QfError(
                            "MISSION_NOT_FOUND",
                            "Experiment Mission does not exist in the requested Program.",
                            404,
                        )

                if parent_entry_id is not None:
                    parent = session.execute(
                        select(SearchLedgerEntry)
                        .where(SearchLedgerEntry.id == parent_entry_id)
                        .with_for_update()
                    ).scalar_one_or_none()
                    program_lineage = _evidence_program_lineage(session, program_id)
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
                        if exposure is not None:
                            raise QfError(
                                "SEALED_EXPOSURE_ALREADY_CONSUMED",
                                "This source experiment already has a sealed evaluation exposure.",
                                409,
                                {"sealed_experiment_id": str(exposure.id)},
                            )

                entry = SearchLedgerEntry(
                    id=request.experiment_id,
                    program_id=program_id,
                    branch_id=branch_id,
                    mission_id=mission_id,
                    dataset_revision_id=request.dataset_revision_id,
                    parent_entry_id=parent_entry_id,
                    mode=ExperimentMode.SEALED.value if sealed else request.mode.value,
                    state="RUNNING",
                    runtime_name="NAUTILUS_TRADER",
                    runtime_version=None,
                    request_json=_normalized_request(request),
                    evidence_json={},
                    disclosure_json={},
                    started_at=_now(),
                    finished_at=None,
                    failure_code=None,
                    failure_message=None,
                )
                session.add(entry)
'''
replace_once(ledger, old_execute_prefix, new_execute_prefix)

promotion = "backend/src/quant_runtime/promotion.py"
replace_once(promotion, '''    DatasetRevision,
    DownstreamSystem,
''', '''    DatasetRevision,
    DegradationFollowup,
    DownstreamSystem,
''')
replace_once(promotion, "def qualify_alpha(\n", '''def _qualification_contract(
    *,
    sealed_dataset_revision_id: UUID,
    name: str | None,
    role: str,
) -> dict[str, Any]:
    return {
        "sealed_dataset_revision_id": str(sealed_dataset_revision_id),
        "requested_name": name,
        "role": role,
    }


def _assert_qualification_replay(
    existing: AlphaQualification,
    contract: dict[str, Any],
) -> None:
    if (existing.metrics or {}).get("qualification_contract") != contract:
        raise QfError(
            "ALPHA_QUALIFICATION_CONTRACT_REUSED",
            "Existing Alpha Qualification is bound to a different immutable qualification request.",
            409,
        )


def _has_degradation_observation(session: Session, alpha_id: UUID) -> bool:
    return session.scalar(
        select(DegradationFollowup.id)
        .where(DegradationFollowup.alpha_qualification_id == alpha_id)
        .limit(1)
    ) is not None


def qualify_alpha(
''')
replace_once(promotion, '''    """Run independent sealed evaluation and promote the real Discovery evidence."""
    with factory() as session:
''', '''    """Run independent sealed evaluation and promote the real Discovery evidence."""
    if role not in _RECOGNIZED_ALPHA_ROLES:
        raise QfError(
            "ALPHA_ROLE_INVALID",
            "Alpha Qualification role is not recognized by the governed V1 contract.",
            422,
            {"role": role},
        )
    qualification_contract = _qualification_contract(
        sealed_dataset_revision_id=sealed_dataset_revision_id,
        name=name,
        role=role,
    )
    with factory() as session:
''')
replace_once(promotion, '''        if existing is not None:
            session.expunge(existing)
            return existing
        source = session.get(SearchLedgerEntry, source_experiment_id)
''', '''        if existing is not None:
            _assert_qualification_replay(existing, qualification_contract)
            session.expunge(existing)
            return existing
        source = session.get(SearchLedgerEntry, source_experiment_id)
''')
# Replace the second identical replay block after sealed execution.
replace_once(promotion, '''        if existing is not None:
            session.expunge(existing)
            return existing
        quality_tier = str(disclosure.get("quality_tier", ""))
''', '''        if existing is not None:
            _assert_qualification_replay(existing, qualification_contract)
            session.expunge(existing)
            return existing
        quality_tier = str(disclosure.get("quality_tier", ""))
''')
replace_once(promotion, '''                "strategy_artifact": strategy_artifact,
            },
''', '''                "strategy_artifact": strategy_artifact,
                "qualification_contract": qualification_contract,
            },
''')
replace_once(promotion, '''    if len(alphas) != len(set(alpha_ids)):
        raise QfError(
            "ALPHA_NOT_PORTFOLIO_READY",
            "Every selected Alpha must be active, healthy, and qualified.",
            422,
        )
''', '''    if len(alphas) != len(set(alpha_ids)) or any(
        _has_degradation_observation(session, item.id) for item in alphas
    ):
        raise QfError(
            "ALPHA_NOT_PORTFOLIO_READY",
            "Every selected Alpha must be active, qualified, and free of degradation observations.",
            422,
        )
''')
replace_once(promotion, '''            persisted_alpha is None
            or persisted_alpha.state != "ACTIVE"
            or persisted_alpha.degradation_state != "HEALTHY"
        ):
''', '''            persisted_alpha is None
            or persisted_alpha.state != "ACTIVE"
            or persisted_alpha.degradation_state != "HEALTHY"
            or _has_degradation_observation(session, alpha_id)
        ):
''')

degradation = "backend/src/quant_runtime/degradation.py"
replace_once(degradation, '''                dependencies=[],
''', '''                dependencies=[str(source.id), str(episode.id)],
''')
replace_once(degradation, '''            followup.job_id = job.id
            alpha.degradation_state = "DEGRADED"
            alpha.metrics = {
                **(alpha.metrics or {}),
                "degradation_followup_episode_id": str(episode.id),
                "degradation_followup_mission_id": str(mission.id),
                "degradation_followup_job_id": str(job.id),
                "latest_forward_evidence": evidence,
            }
            append_event(
''', '''            followup.job_id = job.id
            append_event(
''')

workspace = "backend/src/quant_runtime/workspace.py"
replace_once(workspace, '''    DatasetRevision,
    GovernedDataSource,
''', '''    DatasetRevision,
    DegradationFollowup,
    ForwardEvidenceEpisode,
    GovernedDataSource,
''')
replace_once(workspace, '''    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    try:
''', '''    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    degradation_context: dict[str, Any] | None = None
    try:
''')
needle = '''                datasets.append(
                    {
                        "dataset_revision_id": str(revision.id),
                        "universe_version_id": str(revision.universe_version_id),
                        "data_domains": sorted(domains),
                        "catalog_key": catalog_key,
                        "catalog_uri": catalog_uri,
                        "provider_name": revision.provider_name,
                        "source_license": revision.source_license,
                        "nautilus_data_type": revision.nautilus_data_type,
                        "instrument_scope": revision.instrument_scope,
                        "event_start": revision.event_start,
                        "event_end": revision.event_end,
                        "available_start": revision.available_start,
                        "available_end": revision.available_end,
                        "row_count": revision.row_count,
                        "schema_revision": revision.schema_revision,
                        "quality_result": revision.quality_result,
                        "point_in_time_result": revision.point_in_time_result,
                    }
                )
'''
addition = needle + '''            if mission.type == "ALPHA_DEGRADATION_RESEARCH":
                followup = session.scalar(
                    select(DegradationFollowup).where(DegradationFollowup.mission_id == mission.id)
                )
                if followup is None:
                    raise QfError(
                        "DEGRADATION_CONTEXT_MISSING",
                        "Degradation Mission has no immutable follow-up record.",
                        500,
                    )
                source_entry = session.get(SearchLedgerEntry, followup.source_experiment_id)
                episode = session.get(
                    ForwardEvidenceEpisode,
                    followup.forward_evidence_episode_id,
                )
                if (
                    source_entry is None
                    or source_entry.program_id != program.id
                    or episode is None
                ):
                    raise QfError(
                        "DEGRADATION_CONTEXT_MISSING",
                        "Degradation Mission source evidence is incomplete.",
                        500,
                    )
                try:
                    source_request = BacktestExperimentRequest.model_validate(source_entry.request_json)
                except ValueError as exc:
                    raise QfError(
                        "DEGRADATION_SOURCE_CONTRACT_INVALID",
                        "Degradation source experiment contract is invalid.",
                        500,
                    ) from exc
                degradation_context = {
                    "policy": "READ_ONLY_DEGRADATION_EVIDENCE_V1",
                    "alpha_qualification_id": str(followup.alpha_qualification_id),
                    "source_experiment_id": str(source_entry.id),
                    "strategy_artifact": source_request.strategy.model_dump(mode="json"),
                    "discovery_evidence": source_entry.evidence_json,
                    "forward_evidence": {
                        "episode_id": str(episode.id),
                        "observation_start": episode.observation_start,
                        "observation_end": episode.observation_end,
                        "sample_size": episode.sample_size,
                        "evidence": episode.evidence,
                    },
                }
'''
replace_once(workspace, needle, addition)
replace_once(workspace, '''    (workspace / "DATASETS.json").write_text(
''', '''    if degradation_context is not None:
        (workspace / "DEGRADATION_CONTEXT.json").write_text(
            json.dumps(
                degradation_context,
                ensure_ascii=False,
                indent=2,
                default=str,
            ),
            encoding="utf-8",
        )

    (workspace / "DATASETS.json").write_text(
''')
replace_once(workspace, '''7. Do not invent fills, PnL, positions, statistics, or DatasetRevision metadata. The parent
   worker executes accepted contracts and writes canonical results to `evidence/*.json`.
8. The exact successful `StrategyArtifact` is immutable research lineage and is what a
''', '''7. For degradation-triggered Missions, read `DEGRADATION_CONTEXT.json` when present. It is a
   read-only projection of the exact source StrategyArtifact, Discovery evidence, and triggering
   Forward Evidence episode; do not alter or reinterpret it as sealed raw evidence.
8. Do not invent fills, PnL, positions, statistics, or DatasetRevision metadata. The parent
   worker executes accepted contracts and writes canonical results to `evidence/*.json`.
9. The exact successful `StrategyArtifact` is immutable research lineage and is what a
''')

mission_runner = "backend/src/runners/research_missions.py"
replace_once(mission_runner, '''                        "Read MISSION.md, DATASETS.json, EXPERIMENT_CONTRACT.schema.json, and "
                        "NAUTILUS_EXPERIMENTS.md before making quantitative claims. Use only governed Discovery "
''', '''                        "Read MISSION.md, DATASETS.json, EXPERIMENT_CONTRACT.schema.json, and "
                        "NAUTILUS_EXPERIMENTS.md before making quantitative claims. If DEGRADATION_CONTEXT.json "
                        "exists, read it as immutable prior Strategy/Discovery/Forward Evidence context. Use only governed Discovery "
''')

degradation_test = "backend/tests/integration/test_degradation_feedback.py"
replace_once(degradation_test, '''        assert alpha.degradation_state == "DEGRADED"
        assert alpha.metrics["degradation_followup_episode_id"] == str(episode_id)
        assert mission.state == "READY"
''', '''        assert alpha.degradation_state == "HEALTHY"
        assert "degradation_followup_episode_id" not in alpha.metrics
        assert mission.state == "READY"
        assert mission.dependencies == [str(alpha.source_experiment_id), str(episode_id)]
''')

promotion_test = "backend/tests/integration/test_nautilus_promotion.py"
replace_once(promotion_test, '''    assert alpha.metrics["strategy_artifact"]["artifact_id"] == _STRATEGY.artifact_id
''', '''    assert alpha.metrics["strategy_artifact"]["artifact_id"] == _STRATEGY.artifact_id
    assert alpha.metrics["qualification_contract"] == {
        "sealed_dataset_revision_id": str(sealed_dataset_id),
        "requested_name": "Qualified remote Nautilus alpha",
        "role": "PRIMARY_ALPHA",
    }
''')

print("stage2b issue22 immutable evidence closure applied")
