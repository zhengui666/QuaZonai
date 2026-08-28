from __future__ import annotations

from datetime import UTC, datetime, timedelta
from types import SimpleNamespace
from uuid import uuid4

import pytest
from sqlalchemy import Engine

from api.domain import (
    CharterView,
    _resolve_frozen_research_scope,
    _target_effective_until,
)
from candidate_bundles import _member_payload
from db.models import DatasetRevision, GovernedDataSource, MarketUniverseVersion
from db.session import create_session_factory
from errors import QfError
from quant_runtime.contracts import BacktestExperimentRequest, ExperimentMode, StrategyArtifact
from quant_runtime.promotion import (
    _approval_simulation_summaries,
    _executed_instrument_weights,
    _mandate_constraints,
)
from runners.research_missions import _open_codex_thread


def test_transport_retry_resumes_persisted_codex_thread() -> None:
    class FakeCodex:
        def __init__(self) -> None:
            self.started = 0
            self.resumed: list[str] = []

        def thread_start(self, **kwargs):
            self.started += 1
            return SimpleNamespace(id="new", kwargs=kwargs)

        def thread_resume(self, thread_id: str, **kwargs):
            self.resumed.append(thread_id)
            return SimpleNamespace(id=thread_id, kwargs=kwargs)

    fake = FakeCodex()
    thread = _open_codex_thread(
        fake, persisted_thread_id="thr-persisted", options={"model": "test"}
    )
    assert thread.id == "thr-persisted"
    assert fake.started == 0
    assert fake.resumed == ["thr-persisted"]


def test_approved_target_validity_uses_decision_and_target_expiry() -> None:
    candidate_id = uuid4()
    alpha_id = uuid4()
    universe_id = uuid4()
    created = datetime(2026, 8, 20, tzinfo=UTC)
    decision = datetime(2026, 8, 28, tzinfo=UTC)
    review_deadline = datetime(2026, 8, 29, tzinfo=UTC)
    target_expiry = datetime(2026, 9, 4, tzinfo=UTC)
    candidate = SimpleNamespace(
        id=candidate_id,
        created_at=created,
        state="READY",
        metrics={"search_adjusted_quality": 0.8},
        members=[{
            "alpha_qualification_id": str(alpha_id),
            "universe_version_id": str(universe_id),
            "instrument_id": "EUR/USD.SIM",
            "target_weight": 1.0,
        }],
    )
    approval = SimpleNamespace(
        state="APPROVED",
        created_at=created,
        updated_at=decision,
        valid_until=review_deadline,
        expires_at=target_expiry,
    )
    row = _member_payload(candidate, approval=approval, runtime={})[0]
    assert row["effective_from"] == decision.isoformat()
    assert row["effective_until"] == target_expiry.isoformat()
    assert row["effective_until"] != review_deadline.isoformat()


def test_target_validity_is_a_separate_downstream_contract() -> None:
    decision = datetime(2026, 8, 28, tzinfo=UTC)
    downstream = SimpleNamespace(public_config={"target_validity_seconds": 3600})
    assert _target_effective_until(downstream, decision) == decision + timedelta(hours=1)


def test_max_cost_bps_is_rejected_before_remote_simulation() -> None:
    nested = SimpleNamespace(spec_json={"constraints": {"max_cost_bps": 5}})
    with pytest.raises(QfError) as nested_error:
        _mandate_constraints(nested)
    assert nested_error.value.code == "PORTFOLIO_MANDATE_CONSTRAINT_UNSUPPORTED"

    top_level = SimpleNamespace(spec_json={"max_cost_bps": 5})
    with pytest.raises(QfError) as top_level_error:
        _mandate_constraints(top_level)
    assert top_level_error.value.code == "PORTFOLIO_MANDATE_CONSTRAINT_UNSUPPORTED"


def test_multi_instrument_weights_come_from_executed_notional() -> None:
    weights = _executed_instrument_weights(
        {
            "fills": [
                {"instrument_id": "A.SIM", "quantity": "10", "price": "100"},
                {"instrument_id": "B.SIM", "quantity": "5", "price": "100"},
            ],
            "positions": [],
        },
        ["A.SIM", "B.SIM"],
    )
    assert weights["A.SIM"] == pytest.approx(2 / 3)
    assert weights["B.SIM"] == pytest.approx(1 / 3)
    assert sum(weights.values()) == pytest.approx(1.0)


def test_short_exposure_sign_is_preserved_in_published_weights() -> None:
    weights = _executed_instrument_weights(
        {
            "fills": [
                {"instrument_id": "A.SIM", "quantity": "10", "price": "100", "side": "BUY"},
                {"instrument_id": "B.SIM", "quantity": "5", "price": "100", "side": "SELL"},
            ],
            "positions": [
                {"instrument_id": "A.SIM", "quantity": "10", "side": "LONG", "closed_at": None},
                {"instrument_id": "B.SIM", "quantity": "5", "side": "SHORT", "closed_at": None},
            ],
        },
        ["A.SIM", "B.SIM"],
    )
    assert weights["A.SIM"] == pytest.approx(2 / 3)
    assert weights["B.SIM"] == pytest.approx(-1 / 3)
    assert sum(abs(value) for value in weights.values()) == pytest.approx(1.0)


def test_approval_freezes_real_simulation_summaries() -> None:
    request = BacktestExperimentRequest(
        experiment_id=uuid4(),
        mode=ExperimentMode.PORTFOLIO,
        dataset_revision_id=uuid4(),
        catalog_key="catalog",
        instrument_ids=["EUR/USD.SIM"],
        strategy=StrategyArtifact(
            artifact_id="a",
            kind="IMPORTABLE",
            strategy_path="m:S",
            config_path="m:C",
        ),
        venue_config={"base_currency": "USD", "starting_balances": ["250000 USD"]},
    )
    summaries = _approval_simulation_summaries(
        constraints={"max_drawdown": 0.2},
        evidence={
            "fills": [{"commission": "2.50 USD"}],
            "statistics": {"returns": {"max_drawdown": -0.08}},
        },
        request=request,
        current_candidate=None,
        proposed_quality=0.82,
    )
    assert summaries["capital_context"]["starting_balances"] == ["250000 USD"]
    assert summaries["risk_summary"]["mandate_limits"] == {"max_drawdown": 0.2}
    assert summaries["cost_summary"]["fill_commissions"] == ["2.50 USD"]
    assert summaries["capacity_summary"]["status"] == "NOT_GOVERNED_IN_V1"
    assert summaries["changes_summary"]["material_improvement_gate"] == "PASSED"


def test_scope_does_not_fallback_when_latest_universe_has_no_dataset(engine: Engine) -> None:
    factory = create_session_factory(engine)
    now = datetime.now(UTC)
    with factory() as session, session.begin():
        source = GovernedDataSource(
            name="FX quotes",
            provider="fixture",
            state="ACTIVE",
            universe_scope=["FX"],
            fields=["event_time", "available_time", "bid_price", "ask_price"],
            update_cadence="daily",
            preflight_state="READY",
            public_config={"data_domains": ["quotes"]},
        )
        v1 = MarketUniverseVersion(
            universe_key="FX", version_no=1, name="FX", state="ACTIVE", spec_json={}, created_at=now
        )
        v2 = MarketUniverseVersion(
            universe_key="FX", version_no=2, name="FX", state="ACTIVE", spec_json={}, created_at=now
        )
        session.add_all([source, v1, v2])
        session.flush()
        session.add(
            DatasetRevision(
                data_source_id=source.id,
                universe_version_id=v1.id,
                universe_name="FX",
                revision_no=1,
                event_start=now - timedelta(days=2),
                event_end=now - timedelta(days=1),
                available_start=now - timedelta(days=2),
                available_end=now - timedelta(days=1),
                row_count=10,
                quality_state="VALID",
                point_in_time_state="VALID",
                partition="DISCOVERY",
                created_at=now,
                catalog_uri="nautilus-catalog://fx-v1",
                nautilus_data_type="QuoteTick",
                instrument_scope=["EUR/USD.SIM"],
            )
        )
    with factory() as session:
        preview = CharterView(
            original_idea_text="Research an FX quote alpha with a daily horizon.",
            market_scope="FX",
            prediction_horizon="1D",
        )
        with pytest.raises(QfError) as raised:
            _resolve_frozen_research_scope(session, preview)
    assert raised.value.code == "RESEARCH_SCOPE_UNAVAILABLE"
