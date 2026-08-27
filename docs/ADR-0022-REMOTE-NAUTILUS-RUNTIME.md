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

## Closure invariants

- A catalog key is immutable after its first successful ingest. Only an exact replay of the complete ingest contract is idempotent; a different source, payload, instrument, provider or license must use a new key. Catalog schema lineage uses explicit versioned identifiers rather than application-defined content digests.
- Mission-supplied source bundles and Candidate Bundle wheels are imported only inside disposable child processes. The child receives only the selected catalog copy, a sanitized environment without Gateway credentials, no stdin, and no external IP network access.
- Promotion requires a concrete frozen prediction horizon, explicit sealed-dataset time bounds, matching experiment identity and mode, a recognized non-shadow Alpha role, replay identity over the complete requested Alpha set, and a passing Material Improvement gate before any Candidate or Approval state is advanced.
- Candidate Bundle lineage preserves canonical `alpha_qualification_id` values and every governed instrument represented by each selected Alpha. Historical orders, fills and positions are validation evidence only; executable order instructions, broker adapters, accounts and credentials remain downstream-owned.

## Verification

CI performs a targeted AST ownership check in Core and a separate Python 3.12 job that installs the exact Nautilus pin, writes deterministic quote data to `ParquetDataCatalog`, verifies immutable catalog replay, executes a real `BacktestNode` in an isolated strategy process, checks orders/fills/positions/PnL/statistics, verifies sealed disclosure, validates Candidate Bundle conformance, and exercises Gateway authentication.
