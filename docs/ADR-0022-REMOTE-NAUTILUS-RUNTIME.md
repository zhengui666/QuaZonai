# ADR-0022: Remote Nautilus-first quant runtime

- **Status:** Accepted
- **Issue:** #22
- **Validated runtime:** `nautilus_trader==1.231.0`
- **Protocol:** QuaZonai Quant Runtime HTTP v1

## Decision

QuaZonai remains the research control plane. A separately deployed and authenticated NautilusTrader Gateway is the canonical owner of market-data catalogs, instrument definitions, backtest/exchange/account models, orders, fills, positions, PnL and performance statistics. The Core Python environment does not install or import NautilusTrader.

The user's NautilusTrader instance is remote. The repository therefore contains a deployable Gateway under `nautilus_runtime/`, but Core compose does not start it. Research and sealed-evaluation Gateways have independent URL/token configuration. Paper and Live are also independent runtimes and reuse the Candidate Bundle's exact strategy wheel/config; broker adapters and credentials are injected only into those downstream runtimes.

## Security and authority

A Research Mission Codex process can only write schema-validated strategy source and experiment contracts in its isolated worktree. The parent worker retains the database and Gateway service token, validates Dataset Revision governance, submits the experiment and returns structured evidence. It never passes Gateway or broker credentials to Codex.

QuaZonai does not expose live order, fill, position, account or TradingNode control APIs. Sealed evaluation returns controlled aggregate disclosure only; raw sealed transaction evidence remains in the isolated runtime.

## Evidence and promotion

Every attempted experiment is a Search Ledger entry, including failures. Alpha Qualification and Portfolio Candidate promotion reference successful entries. Candidate Bundle v2 packages the same strategy artifact, the pinned runtime requirement, configuration, validation fixture, canonical transaction evidence and lineage. It replaces the former QuaZonai-owned four-wheel micro-runtime.

## Verification

CI performs a targeted AST ownership check in Core and a separate Python 3.12 job that installs the exact Nautilus pin, writes deterministic quote data to `ParquetDataCatalog`, executes a real `BacktestNode`, checks orders/fills/positions/PnL/statistics, verifies sealed disclosure, and exercises Gateway authentication.
