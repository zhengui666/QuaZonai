# Show Tick Economic Replays in Portfolio Backtests

<a id="task"></a>
## Task

**Task ID:** tick-economic-replay-portfolio-page.

**Original request:** Show tick economic replays admitted through the CLI or service on the Portfolio Backtest page, with truthful progress.

**Endpoint and scope:** The owner accepted this proposal and instructed development. Implement the dedicated read path, Durable Task progress and read-only portfolio-page section against `dev`, with applicable verification and independent review. No production deployment or real-account operation is requested.

<a id="intent"></a>
## Intent

The `/portfolios` page lists standard strategy portfolios, so users cannot find an admitted tick economic replay there or tell whether it is progressing. Add a distinct, read-only, page-level replay collection. A replay has no `portfolioId`; neither its placement nor its links may imply membership in a standard portfolio.

Preserve standard portfolio creation, curves, weights and multi-portfolio behavior. Starting, retrying execution, stopping, deleting, trading, data preparation and factor research are out of scope. Retrying a failed read is in scope.

<a id="baseline"></a>
## Verified dev baseline

Source inspection uses remote `dev`, commit `868cf52d03f37ff869e9fc38abff7fb8a18b5136`.

| Source | Existing behavior and gap |
| --- | --- |
| [Portfolio page](../../../apps/web/src/portfolio.tsx), [router](../../../apps/web/src/router.ts) | `/portfolios` is an existing hash-router destination. Its list reads `PortfolioView[]` from `/portfolios`; detail uses the existing metric, progress and chart components. Neither renders tick economic replays. |
| [Replay contracts](../../../packages/contracts/src/economic-tick.ts) | `TickEconomicReplayView` already carries replay/dataset/factor or weighted-member identity, `taskId`, status, qualification, result and error. It has no `portfolioId`, creation time or progress projection; active views do not expose the execution model. |
| [Replay service and Task](../../../apps/service/src/economic-tick.ts) | `TickEconomicReplays` persists the original request and view. `POST /api/v3/tick-economic-replays` admits a replay; `GET /api/v3/tick-economic-replays/:id` reads its stored view. There is no GET collection. The existing official Durable Task has a `compute` phase and updates replay status, but does not publish the requested task progress. |
| [CLI](../../../apps/cli/src/commands.ts), [service guide](../../../apps/service/README.md) | Replay admission and lookup by ID already use the service. A new admission path is unnecessary. |

This refines the supplied context: replay contracts and service detail reads already exist on `dev`; the missing capability is the page collection and complete read/progress projection. Installed-service behavior was not exercised during this proposal.

<a id="spec"></a>
## Requirements and design

1. **Discovery and separation.** An admitted tick economic replay appears in its own section on `/portfolios`, remains findable after reload and is visible even when no standard portfolios exist. Replay loading, empty or error states do not hide standard portfolio creation or results; standard portfolio state does not gate replay discovery.
2. **Readable identity and context.** Each replay exposes replay ID, dataset, single factor or weighted members, effective execution model, creation time, current status and reported scientific qualification as readable fields. Preserve exact weights and original identities. Resolve execution metadata from the persisted original request/result and authoritative Task metadata. For older records, show unavailable metadata explicitly when it cannot be recovered; never substitute the read time as creation time or guess an execution model.
3. **Dedicated typed reads.** Add the smallest dedicated collection contract and enrich/reuse the existing replay detail contract. The page can list persisted replays and load a selected replay's result. Read the existing `TickEconomicReplays` records and their `taskId`; do not reconstruct records from CLI receipts, logs or process state, overload `PortfolioView`, or create a second execution record. Apply the existing owner-authenticated read boundary. A missing detail remains a clear 404/read error.
4. **Truthful Task progress.** Queued/running views expose the current phase and last update from the replay's existing Durable Task. Its worker publishes real phase transitions through the official Task's public API; service reads project those facts. Use `phase`, `updatedAt` and optional `completed`/`total`. A determinate percentage requires meaningful counters for the displayed phase, with `total > 0` and `0 <= completed <= total`; otherwise show the phase and an indeterminate indicator. Phase-local counters must not be labeled overall replay completion. Do not estimate progress from elapsed time. Missing task/progress data is unavailable, never synthetic 0%, success or failure.
5. **Refresh without execution.** Refresh known active replay data about every five seconds while the section is visible and provide manual refresh for collection and selected detail. Stop automatic detail polling for terminal replays; collection refresh must not repeatedly fetch their result payloads. When no active replay is known, manual refresh or revisiting/reloading the page discovers later CLI/service admissions. Reads and read retries never submit, restart, stop or otherwise change a replay. Failed refresh retains visibly stale last-known data and timestamp, with a retryable error; a delayed response for another selection cannot replace the selected replay.
6. **Results and terminal states.** `COMPUTED` shows available metrics and the original replay curve using the page's existing `Metrics`, `Exact`, `Timestamp` and `EquityPlot` presentation. Preserve each metric's partition, regime, coverage and units; do not combine rows into an invented aggregate. Curve points come from `result.days[].equity_usdt` at `day_start_ns`, use USDT and the original `request.scenario.initial_usdt`, and retain null gaps and `unknown_reason`. Do not substitute dataset currency, USDC funding balances or a new calculation for the recorded USDT curve. Missing results/metrics remain unavailable. `FAILED` and `STOPPED` show the returned error when present, including `tick_economic_local_stop_remote_receipt_unsettled`; local stop does not establish remote cancellation. Loading, empty and recoverable read errors remain clear.
7. **Qualification remains independent.** Display the replay's reported `scientificQualification`, including `UNQUALIFIED_EXPLORATORY`, separately from execution status. `COMPUTED`, a complete progress count or a displayed return never becomes scientific qualification, profitability or target-delivery permission. Preserve existing source, PIT, Sealed, independent-evaluation, trial-ledger, usage, receipt and duplicate-execution boundaries.

The minimal future read surface is GET collection at `/api/v3/tick-economic-replays` plus the existing GET detail at `/api/v3/tick-economic-replays/:id`. Collection rows need identity, context, status, qualification and progress; full native result payloads belong to selected detail. Wire changes belong in [the source contracts](../../../packages/contracts/src/economic-tick.ts), orchestration/projection in [the existing service](../../../apps/service/src/economic-tick.ts), and rendering in [the portfolio page](../../../apps/web/src/portfolio.tsx). Reuse official Durable capabilities and existing UI components; do not hand-edit generated files or add a scheduler.

<a id="verification"></a>
## Acceptance evidence for a future implementation

- With disposable real service/Durable storage, admit a replay through the existing CLI/service path, discover it with zero standard portfolios, observe actual worker progress, load its terminal result, and find the same ID after page reload and service restart. Listing, selecting and refreshing must not create another Task or change replay inputs/status. Controlled UI fixtures alone do not establish this integration.
- Cover single-factor and weighted-member identities; queued/running phases with and without meaningful counters; missing/stale task progress; terminal polling stop; manual discovery after an empty/all-terminal collection; failed reads and recovery; and rapid selection changes.
- Verify `COMPUTED` metrics by partition/regime and original USDT curve values, null gaps, unavailable metadata for older persisted records, `FAILED`/`STOPPED` errors and unchanged exploratory qualification. Inspect desktop/mobile rendering and regress existing portfolio creation, weights, curves and multiple portfolios.
- Run checks appropriate to the eventual contract/service/worker/Web changes from [project commands](../../project.md#commands), followed by the exact-Head CI and read-only review required by [review policy](../../review.md). Native execution evidence is required if native behavior changes; UI/transport checks never establish scientific quality or profitability.

<a id="delivery"></a>
## Proposal review and delivery

Implemented the dedicated owner-authenticated replay collection and enriched detail reads, official Durable Task checkpoint progress, admission time for new Tasks and a separate read-only page section. Legacy missing metadata remains unavailable. Original result money, gaps, metric scopes and qualification remain intact; no scientific kernel behavior changed.

Independent cross-review of backend and Web changes found no actionable issues. Local verification: Service and Research type checks pass; Service 402 tests pass with 29 prerequisite-dependent native checks skipped; Research 275 tests and CLI 15 tests pass; Web types, 52 unit tests and production build pass. Three focused Chromium scenarios pass, including desktop/mobile accessibility, polling, stale/error recovery and original result semantics. The complete Chromium browser suite passes all 74 scenarios, including the disposable real HTTP/Durable regression and all three new replay scenarios. New replay browser checks use controlled transport; the service integration test independently verifies real Durable and remote-worker protocol behavior. Neither constitutes scientific performance evidence.

Chromium download was denied by the environment network policy; browser tests use the existing `/usr/bin/chromium`. `make check-links` remains unavailable because `lychee` is absent. Final-Head hosted CI, GitHub Codex review, merge and deployment are not yet verified. No production service or real account was operated.
