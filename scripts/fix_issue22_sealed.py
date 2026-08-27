from __future__ import annotations

import re
from pathlib import Path


path = Path("backend/src/runners/quant_experiments.py")
text = path.read_text(encoding="utf-8")
quality = '''    if dataset.quality_state != "VALID" or dataset.point_in_time_state != "VALID":
        raise QfError(
            "DATASET_NOT_RESEARCH_READY",
            "The Dataset Revision has not passed quality and point-in-time validation.",
            422,
        )
'''
quality_new = quality + '''    if dataset.partition != "DISCOVERY":
        raise QfError(
            "DISCOVERY_DATASET_REQUIRED",
            "Mission experiments must begin with a Discovery Dataset Revision.",
            422,
            {"partition": dataset.partition},
        )
'''
if quality not in text:
    raise RuntimeError("discovery dataset validation baseline not found")
text = text.replace(quality, quality_new, 1)

start = text.index("def _create_sealed_experiment(")
end = text.index("\ndef _promote_from_sealed(", start)
replacement = '''def _create_sealed_experiment(
    session: Session,
    discovery: QuantExperiment,
) -> QuantExperiment | None:
    existing = session.scalar(
        select(QuantExperiment).where(
            QuantExperiment.parent_experiment_id == discovery.id,
            QuantExperiment.zone == "SEALED",
        )
    )
    if existing is not None:
        return existing
    contract = ExperimentContract.model_validate(discovery.contract_json)
    candidates = session.execute(
        select(NautilusCatalogBinding, DatasetRevision)
        .join(DatasetRevision, DatasetRevision.id == NautilusCatalogBinding.dataset_revision_id)
        .where(
            DatasetRevision.partition == "SEALED",
            DatasetRevision.quality_state == "VALID",
            DatasetRevision.point_in_time_state == "VALID",
            NautilusCatalogBinding.runtime_version == PINNED_NAUTILUS_VERSION,
        )
        .order_by(DatasetRevision.created_at.desc())
    ).all()
    sealed_binding: NautilusCatalogBinding | None = None
    sealed_dataset: DatasetRevision | None = None
    requested = set(contract.catalog.instrument_ids)
    for binding, dataset in candidates:
        if requested.issubset(set(binding.instrument_scope)):
            sealed_binding = binding
            sealed_dataset = dataset
            break
    if sealed_binding is None or sealed_dataset is None:
        _ledger(
            session,
            experiment=discovery,
            outcome="SEALED_DATA_BLOCKED",
            evidence_summary={"reason": "NO_INDEPENDENT_SEALED_CATALOG"},
        )
        append_event(
            session,
            kind="SEALED_QUANT_EXPERIMENT_BLOCKED",
            aggregate_type="RESEARCH_PROGRAM",
            aggregate_id=discovery.program_id,
            payload={
                "discovery_experiment_id": str(discovery.id),
                "reason": "NO_INDEPENDENT_SEALED_CATALOG",
            },
        )
        return None
    sealed_contract = contract.model_copy(
        update={
            "run_id": uuid4(),
            "catalog": contract.catalog.model_copy(
                update={
                    "dataset_revision_id": sealed_dataset.id,
                    "catalog_uri": sealed_binding.catalog_uri,
                    "nautilus_data_type": sealed_binding.nautilus_data_type,
                    "partition": "SEALED",
                }
            ),
        }
    )
    sealed = QuantExperiment(
        parent_experiment_id=discovery.id,
        mission_id=discovery.mission_id,
        program_id=discovery.program_id,
        dataset_revision_id=sealed_dataset.id,
        zone="SEALED",
        state="READY",
        runtime_name="NAUTILUS_TRADER",
        runtime_version=PINNED_NAUTILUS_VERSION,
        strategy_artifact=dict(discovery.strategy_artifact),
        contract_json=sealed_contract.model_dump(mode="json"),
    )
    session.add(sealed)
    session.flush()
    _ledger(session, experiment=sealed, outcome="QUEUED")
    _enqueue(session, kind="NAUTILUS_SEALED_BACKTEST", resource_id=sealed.id)
    append_event(
        session,
        kind="SEALED_QUANT_EXPERIMENT_QUEUED",
        aggregate_type="RESEARCH_PROGRAM",
        aggregate_id=sealed.program_id,
        payload={
            "discovery_experiment_id": str(discovery.id),
            "sealed_experiment_id": str(sealed.id),
            "sealed_dataset_revision_id": str(sealed_dataset.id),
        },
    )
    return sealed
'''
text = text[:start] + replacement + text[end:]
path.write_text(text, encoding="utf-8")

path = Path("backend/tests/integration/test_quant_experiment_pipeline.py")
text = path.read_text(encoding="utf-8")
text = text.replace(
    '''        dataset = DatasetRevision(
            universe_name="FX",
            revision_no=1,
            schema_version="quote-v1",
            quality_state="VALID",
            point_in_time_state="VALID",
            partition="DISCOVERY",
            created_at=datetime.now(UTC),
        )
''',
    '''        dataset = DatasetRevision(
            universe_name="FX",
            revision_no=1,
            schema_version="quote-v1",
            quality_state="VALID",
            point_in_time_state="VALID",
            partition="DISCOVERY",
            created_at=datetime.now(UTC),
        )
        sealed_dataset = DatasetRevision(
            universe_name="FX",
            revision_no=2,
            schema_version="quote-v1",
            quality_state="VALID",
            point_in_time_state="VALID",
            partition="SEALED",
            created_at=datetime.now(UTC),
        )
''',
    1,
)
text = text.replace(
    "        session.add_all([mission, dataset, downstream])\n",
    "        session.add_all([mission, dataset, sealed_dataset, downstream])\n",
    1,
)
needle = '''        session.add(binding)
        session.flush()
        return mission.id, dataset.id
'''
replacement = '''        sealed_binding = NautilusCatalogBinding(
            dataset_revision_id=sealed_dataset.id,
            provider="integration-sealed-fixture",
            source_license="TEST",
            catalog_uri="catalog://sealed-eurusd",
            nautilus_data_type="QuoteTick",
            instrument_scope=["EUR/USD.SIM"],
            event_time_range={},
            available_time_range={},
            schema_revision="quote-v1",
            quality_result={"state": "VALID"},
            point_in_time_result={"state": "VALID"},
            runtime_name="NAUTILUS_TRADER",
            runtime_version="1.231.0",
            ingested_at=datetime.now(UTC),
        )
        session.add_all([binding, sealed_binding])
        session.flush()
        return mission.id, dataset.id
'''
if needle not in text:
    raise RuntimeError("pipeline binding baseline not found")
text = text.replace(needle, replacement, 1)
text = text.replace(
    '''        assert sealed is not None
        sealed_id = sealed.id
''',
    '''        assert sealed is not None
        assert sealed.dataset_revision_id != dataset_id
        assert sealed.contract_json["catalog"]["catalog_uri"] == "catalog://sealed-eurusd"
        sealed_id = sealed.id
''',
    1,
)
path.write_text(text, encoding="utf-8")

path = Path("backend/tests/integration/test_remote_nautilus_runtime.py")
text = path.read_text(encoding="utf-8")
old_ingest = '''        ingested = client.post(
            "/v1/catalogs/ingest",
            json={
                "catalog_key": key,
                "dataset_revision_id": str(dataset_id),
                "provider": "CI normalized quote fixture",
                "source_license": "CI_TEST",
                "instrument": "EUR/USD",
                "quotes": quotes,
                "schema_revision": "quote-v1",
                "partition": "DISCOVERY",
            },
            headers={"Idempotency-Key": key},
        )
        ingested.raise_for_status()
        metadata = ingested.json()
'''
new_ingest = old_ingest + '''        sealed_dataset_id = uuid4()
        sealed_key = f"issue22-sealed-{sealed_dataset_id.hex}"
        sealed_ingested = client.post(
            "/v1/catalogs/ingest",
            json={
                "catalog_key": sealed_key,
                "dataset_revision_id": str(sealed_dataset_id),
                "provider": "CI independent sealed quote fixture",
                "source_license": "CI_TEST",
                "instrument": "EUR/USD",
                "quotes": quotes,
                "schema_revision": "quote-v1",
                "partition": "SEALED",
            },
            headers={"Idempotency-Key": sealed_key},
        )
        sealed_ingested.raise_for_status()
        sealed_metadata = sealed_ingested.json()
'''
if old_ingest not in text:
    raise RuntimeError("remote integration ingestion baseline not found")
text = text.replace(old_ingest, new_ingest, 1)
old_sealed = '''        sealed = runtime.run_sealed_backtest(contract)
        assert sealed.partition == "SEALED"
'''
new_sealed = '''        sealed_catalog = CatalogReference(
            dataset_revision_id=sealed_dataset_id,
            catalog_uri=sealed_metadata["catalog_uri"],
            nautilus_data_type=sealed_metadata["nautilus_data_type"],
            instrument_ids=sealed_metadata["instrument_scope"],
            partition="SEALED",
            start_time=quotes[0]["timestamp"],
            end_time=quotes[-1]["timestamp"],
        )
        sealed_contract = contract.model_copy(
            update={"run_id": uuid4(), "catalog": sealed_catalog}
        )
        sealed = runtime.run_sealed_backtest(sealed_contract)
        assert sealed.catalog_uri != discovery.catalog_uri
        assert sealed.partition == "SEALED"
'''
if old_sealed not in text:
    raise RuntimeError("remote integration sealed baseline not found")
text = text.replace(old_sealed, new_sealed, 1)
path.write_text(text, encoding="utf-8")
