"""Shared governed-data scope rules for immutable Research Charters and Missions."""

from __future__ import annotations

from db.models import DatasetRevision, GovernedDataSource, MarketUniverseVersion

_DATA_TYPE_DOMAINS: dict[str, set[str]] = {
    "quotetick": {"quotes", "market_data"},
    "tradetick": {"trades", "market_data"},
    "bar": {"bars", "market_data"},
    "orderbookdelta": {"order_book", "market_data"},
    "orderbookdeltas": {"order_book", "market_data"},
}

_MARKET_SCOPE_ALIASES: dict[str, set[str]] = {
    "us_equities": {"us_equities", "us_equity", "equities", "equity", "stocks", "stock"},
    "crypto_spot": {"crypto_spot", "crypto", "cryptocurrency", "digital_assets"},
    "us_options": {"us_options", "options", "option"},
    "futures": {"futures", "future"},
    "fx": {"fx", "foreign_exchange", "forex"},
}


def normalize_data_domain(value: object) -> str:
    return str(value).strip().casefold().replace("-", "_").replace(" ", "_")


def dataset_revision_domains(
    revision: DatasetRevision,
    source: GovernedDataSource | None,
) -> set[str]:
    """Return semantic data domains; source field/column names are not domains."""
    result = set(_DATA_TYPE_DOMAINS.get(normalize_data_domain(revision.nautilus_data_type), set()))
    if source is not None:
        config = source.public_config or {}
        for key in ("data_domain", "domain"):
            if config.get(key):
                result.add(normalize_data_domain(config[key]))
        configured = config.get("data_domains")
        if isinstance(configured, list):
            result.update(normalize_data_domain(value) for value in configured)
    return {value for value in result if value}


def market_scope_matches_universe(
    market_scope: str | list[str] | None,
    universe: MarketUniverseVersion,
) -> bool:
    """Match the inferred Idea scope to a concrete active Universe Version."""
    requested = market_scope if isinstance(market_scope, list) else [market_scope]
    tokens = {normalize_data_domain(value) for value in requested if value}
    if not tokens or tokens == {"system_inferred"}:
        return True
    universe_tokens = {
        normalize_data_domain(universe.name),
        normalize_data_domain(universe.universe_key),
    }
    expanded: set[str] = set()
    for token in tokens:
        expanded.add(token)
        expanded.update(_MARKET_SCOPE_ALIASES.get(token, set()))
    for canonical, aliases in _MARKET_SCOPE_ALIASES.items():
        if universe_tokens.intersection(aliases | {canonical}):
            universe_tokens.update(aliases)
            universe_tokens.add(canonical)
    return bool(expanded.intersection(universe_tokens))
