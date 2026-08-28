from __future__ import annotations

# Regression coverage for the final Issue 22 Codex findings.
from datetime import UTC, datetime, timedelta
from uuid import uuid4

import pytest

from db.models import DatasetRevision, ResearchProgram
from errors import QfError
from quant_runtime.degradation import _is_explicit_degradation
from quant_runtime.promotion import (
    _discovery_quality_score,
    _evidence_program_lineage_ids,
    _select_sealed_dataset,
)
from quant_runtime.workspace import WorkspaceExperimentActivity


def _revision(*, partition: str, catalog_uri: str, start: datetime, end: datetime, universe_id):
    return DatasetRevision(
        id=uuid4(),
        partition=partition,
        quality_state="VALID",
        point_in_time_state="VALID",
        universe_version_id=universe_id,
        instrument_scope=["EUR/USD.SIM"],
        catalog_uri=catalog_uri,
        event_start=start,
        event_end=end,
        created_at=datetime.now(UTC),
    )


class _DatasetSession:
    def __init__(self, dataset: DatasetRevision) -> None:
        self.dataset = dataset

    def get(self, model, identity):
        del model
        return self.dataset if identity == self.dataset.id else None


def test_discovery_quality_does_not_reward_absolute_pnl_scale() -> None:
    evidence = {
        "fills": [{"trade_id": "T-1"}],
        "statistics": {"returns": {}, "general": {"Profit Factor": 1.25}},
        "pnl": {"USD": {"PnL (total)": 10.0}},
    }
    scaled = {**evidence, "pnl": {"USD": {"PnL (total)": 10_000_000.0}}}
    score, model = _discovery_quality_score(evidence, search_attempt_count=5)
    scaled_score, scaled_model = _discovery_quality_score(scaled, search_attempt_count=5)
    assert scaled_score == score
    assert model["model"] == "DISCOVERY_PUBLIC_PERFORMANCE_V2"
    assert model["absolute_pnl_used_for_scoring"] is False
    assert scaled_model["absolute_pnl_used_for_scoring"] is False


def test_sealed_dataset_must_use_distinct_catalog_and_nonoverlapping_time() -> None:
    universe_id = uuid4()
    start = datetime(2024, 1, 1, tzinfo=UTC)
    source = _revision(
        partition="DISCOVERY",
        catalog_uri="nautilus-catalog://discovery",
        start=start,
        end=start + timedelta(days=10),
        universe_id=universe_id,
    )
    same_catalog = _revision(
        partition="SEALED",
        catalog_uri=source.catalog_uri or "",
        start=start + timedelta(days=10),
        end=start + timedelta(days=20),
        universe_id=universe_id,
    )
    with pytest.raises(QfError) as same_error:
        _select_sealed_dataset(_DatasetSession(same_catalog), source, same_catalog.id)
    assert same_error.value.code == "SEALED_DATASET_NOT_INDEPENDENT"

    overlap = _revision(
        partition="SEALED",
        catalog_uri="nautilus-catalog://sealed",
        start=start + timedelta(days=5),
        end=start + timedelta(days=15),
        universe_id=universe_id,
    )
    with pytest.raises(QfError) as overlap_error:
        _select_sealed_dataset(_DatasetSession(overlap), source, overlap.id)
    assert overlap_error.value.code == "SEALED_DATASET_TIME_OVERLAP"

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


def test_evidence_lineage_includes_inherited_programs() -> None:
    charter_id = uuid4()
    root = ResearchProgram(
        id=uuid4(),
        charter_id=charter_id,
        title="root",
        state="ACTIVE",
        evidence_inherited_from_program_id=None,
    )
    child = ResearchProgram(
        id=uuid4(),
        charter_id=charter_id,
        title="child",
        state="ACTIVE",
        evidence_inherited_from_program_id=root.id,
    )
    programs = {root.id: root, child.id: child}

    class _ProgramSession:
        def get(self, model, identity):
            del model
            return programs.get(identity)

    assert _evidence_program_lineage_ids(_ProgramSession(), child.id) == {child.id, root.id}


def test_degradation_state_normalization_and_rejected_activity_contract() -> None:
    assert _is_explicit_degradation({"degradation_state": "  degraded  "}) is True
    activity = WorkspaceExperimentActivity()
    assert activity.has_activity is False
    activity.rejected_contract_count += 1
    assert activity.has_activity is True
