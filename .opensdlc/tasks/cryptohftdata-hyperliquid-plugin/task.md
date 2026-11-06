# CryptoHFTData Hyperliquid source plugin

<a id="task"></a>
## Task

Import the downloaded CryptoHFTData archive into QuaZonai, replace other Hyperliquid source plugins, and merge into remote `dev`. The owner authorized implementation and merge in this chat. Work starts from `0b45ad59` in `codex/cryptohftdata-plugin`; unrelated working trees stay intact.

<a id="intent"></a>
## Intent

Use the existing free published files in place. The completed local selection contains 3,368,701 objects / 279,118,890,859 bytes covering UTC dates 2026-02-12 through 2026-10-13. It contains spot/perpetual orderbook snapshots and perpetual mark-price observations. No live collection or paid source is needed.

<a id="spec"></a>
## Requirements and design

The sole built-in Hyperliquid source becomes `qz.data.cryptohftdata`. Loading registers the completed archive without copying or enumerating millions of files. Existing historical dataset records remain readable, while the old source is absent from new selections. Raw books and marks do not claim BAR/TRADE_TICK research readiness.

The operator plugin selects explicit markets, symbols and UTC dates and reuses `market-history` to write actual native book/quote/price records. Adapt the existing importer for flat snapshots, separate event/receive timestamp units and outer Zstandard files. Preserve original prices, sizes and clocks; no generated trades, instrument parameters or permissions.

<a id="plan"></a>
## Implementation plan

1. Replace the old service provider and its registration/tests; use existing source persistence and Settings UI.
2. Add an offline Python source capability and bounded selection manifest referencing original files. Remove the obsolete HF Hyperliquid profile; retain generic Hub tools.
3. Extend the existing native reader and focused parser tests, keeping nested-book input compatibility.
4. Update operator image payload and source guides. Run affected TypeScript, source, packaging and native checks, then independent final-Head review and hosted CI before merging into `dev`.
5. Register the real local archive using the delivered plugin. Native conversion uses bounded selections because the remaining local disk cannot hold a second complete archive.

<a id="verification"></a>
## Verification

The real completed archive loaded with the exact counts and exclusive end 2026-10-14. Original files remain outside Git and unchanged. All 241 Python source tests, 10 focused service/source tests, service typecheck, seven workflow tests, Cargo formatting/locked metadata and diff checks passed. The complete TypeScript suite is running; native parser tests await hosted compilation to avoid another cold build on the space-constrained host.

<a id="review"></a>
## Review and delivery

Final-Head independent review, GitHub Codex read-only review and applicable hosted CI are required before merging into remote `dev`. Local service registration will use the delivered provider; the archive is loaded as books/context, without a claim of BAR/TradeTick research support.
