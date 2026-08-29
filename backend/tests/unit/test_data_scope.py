from __future__ import annotations

from types import SimpleNamespace

from quant_runtime.data_scope import dataset_revision_domains, market_scope_matches_universe


def test_quote_revision_uses_semantic_domains_not_raw_columns() -> None:
    revision = SimpleNamespace(nautilus_data_type="QuoteTick")
    source = SimpleNamespace(
        public_config={"data_domains": ["quotes", "market_data"]},
        fields=["event_time", "available_time", "bid_price", "ask_price"],
    )

    domains = dataset_revision_domains(revision, source)

    assert domains == {"quotes", "market_data"}
    assert "bid_price" not in domains
    assert "event_time" not in domains


def test_inferred_market_scope_alias_matches_concrete_universe() -> None:
    universe = SimpleNamespace(name="US Equities", universe_key="US_EQUITIES")

    assert market_scope_matches_universe("US Equities", universe)
    assert market_scope_matches_universe("US equities", universe)
    assert not market_scope_matches_universe("Crypto Spot", universe)
