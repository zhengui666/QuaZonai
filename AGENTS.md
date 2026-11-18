# QuaZonai Developer Guide

## Project purpose and primary principles

QuaZonai is a single-user quantitative research workbench built around **NautilusTrader and official Pi Durable integration**. It connects ideas, agent research, explicit native strategy backtests, target delivery, and account observation. Its value lies in business integration, research experience, and presenting results, not reimplementing underlying frameworks.

**Avoid reinventing existing capabilities: inspect official NautilusTrader and Pi Durable functionality first, reuse it directly wherever possible, and add only the thinnest business integration.** Write project code only when existing public interfaces genuinely cannot meet the specific need. Do not build alternatives because upstream capabilities are unfamiliar. Use official packages or pinned official upstream versions through public APIs, without copying upstream implementations or maintaining patches or forks.

- NautilusTrader owns market-data catalogs, native indicators, backtesting, matching, shared-account simulation, and native trading models. Preserve its formats, precision, and public interfaces. Do not modify, trim, or replace upstream features or behavior, or write another backtest engine, matcher, or indicator framework. Describe capabilities not yet wired into QuaZonai as integration scope, not limitations of Nautilus itself.
- Pi Durable owns agent Conversation, Submission, Task, Doc, persistence, recovery, replay, child tasks, waiting, cancellation, and tool loops. Reuse its Harness, Extension, and Storage; do not create another agent scheduler, task state machine, task database, or recovery protocol.
- Reuse official pi-ai for model/provider calls, Chord for concurrency and context, and Arrow for columnar data. Legacy capital forward, custom Clarabel allocation, and WASM execution chains are retired and must not return as research or account execution entrypoints. Retain historical contracts, diagnostics, and original capital records. Do not reimplement existing library capabilities.
- Find existing equivalent entrypoints, contracts, and call chains before adding features, and extend them in place. Solve local needs with local changes rather than parallel systems, generic platforms, or multiple forwarding layers.

## Current architecture: preserve it

The following are architectural constraints of the current implementation. **Do not change the architecture without approval. Before implementing a necessary change, explain the exact change, necessity, impact, and migration to the project owner and obtain explicit approval for that architectural change.** General feature, repair, cleanup, or merge authorization is not architectural approval. Do not change the architecture first and explain later, or edit this document to exempt yourself.

### Layers and directories

| Path | Responsibility and boundary |
| --- | --- |
| `packages/contracts/src/` | Shared TypeScript business types, HTTP/CLI/Web DTOs, and rules; one contract across entrypoints. |
| `apps/service/src/` | Fastify HTTP `/api/v3`, instance/model configuration, owner sessions, business facts, research admission, science adaptation, portfolios, target receipts, and account observation. Business orchestration belongs here. |
| `apps/research/src/` | Official Pi Durable host, research documents and ledgers, tool calls, continuation/recovery, literature, and native results. `host.ts` opens official storage and Harness. |
| `apps/research/plugins/` | Research/native-metric tools through official Extension; reuse the host, not a custom runtime. |
| `apps/nautilus/src/` | Existing remote Nautilus HTTP adapter connecting science processes, catalogs, and explicitly configured native account hosts. Account/science execution reuse existing containers. |
| `apps/job/src/` | Rust native entrypoints for science and official LiveNode account hosting. Orders, fills, positions, and account recovery use Nautilus public interfaces; business control stays in TypeScript. |
| `crates/contracts/src/` | Native science Wire types, Arrow types, and generatable science contracts. |
| `crates/domain/src/` | Pure rules, numeric logic, and result validation required for science, not HTTP service or agent scheduling. |
| `apps/cli/src/` | Client, commands, and contract reads for the same HTTP API; never directly open or modify service storage. |
| `apps/web/src/` | React, TypeScript, official Ant Design, and ECharts for interaction/display, not background research scheduling. |
| `runtimes/data/` | Python data acquisition/source adaptation feeding native science, not a rewritten Nautilus engine. |
| `contracts/generated/` | Machine-consumed contracts generated from Rust science definitions, not handwritten developer documentation; do not edit manually. |
| `deploy/`, `runtimes/native/` | Installation, containers, CLI distribution, and native image builds. |
| `.github/` | Retained CI, build, and release configuration. |

The Rust workspace retains only `apps/job`, `crates/contracts`, and `crates/domain`. Use TypeScript for the non-scientific control plane. Do not reintroduce a Rust Server, Store, Runtime gateway, or Rust CLI as another control plane.

### Calls and data flow

```text
Web / CLI
    → TypeScript Service / api/v3
        → Research Host / official Pi Durable / pi-ai
            → remote Nautilus HTTP adapter
                → Rust job / NautilusTrader / Arrow
        → independent paper/live account connections: target receipts and observation
```

Current research directly invokes explicit native strategy backtests, not custom factor generation, WASM fitting, independent scoring, or PM BUILD/STUDY admission. Backtests are historical computations, not PASS, out-of-sample validity, or automatic trading permission. Target delivery and account observation are separate business facts: completed research does not establish delivery, and a receipt does not establish a fill.

- `apps/service/src/service.ts` assembles the service. `facts.ts`, `pm-facts.ts`, and related modules express business facts through official Durable Doc; do not add a separate facts database.
- `apps/research/src/host.ts` assembles official Harness and SQLite Storage. Research, tools, and science child tasks retain original Conversation/Submission/Task identities.
- `apps/service/src/science.ts`, `apps/research/src/native-execution.ts`, and `apps/nautilus/src/` connect the existing science chain; keep adaptation thin and direct.
- Research/backtesting uses the `nautilus.backtest` connection. Paper/live use their own instance connections, accounts, and credentials. Configure each purpose independently without silent fallback between them.
- Startup reopens original Durable storage and lets the official runtime recover original Tasks. A browser disconnect does not cancel research; local process exit does not establish remote termination.
- Configuration, credentials, research state, native results, and existing data persist with the installation directory. Never conceal upgrade failure by clearing data or changing object identity. Retired science workflows have no compatibility execution. Preserve and verify existing account facts, capital records, and pending orders without restoring deleted scientific algorithms.
- QZ itself neither holds brokerage accounts nor directly sends financial orders; it delivers targets and reads account facts through configured downstream interfaces.
- Capital exits reuse the account downstream and containers above, with official LiveNode, Strategy, RiskEngine, ExecutionEngine, Cache, and Hyperliquid adapters. QZ persists exit intentions, reservations, and commands through existing Pi Durable. The user performs external transfers/withdrawals, reconciled against platform records. Shared TypeScript validates capital contracts independently of the science Worker.

### Contracts and numeric meaning

Public TypeScript contracts belong in `packages/contracts/src/`; native science contracts belong in `crates/contracts/src/`. Web and CLI consume the same service/contracts, not separate business implementations. Generate contracts from source definitions rather than editing generated output.

Preserve numeric meaning: do not first convert money, decimals, or nanoseconds to imprecise JavaScript Numbers. Do not replace missing values with zero or present failure as success. Keep validation needed by actual business behavior, not defensive layers for hypothetical risks.

## Strictly prohibited development practices

These are explicit prohibitions, not engineering conventions to add by default:

- **Overengineering:** abstractions without a current need, generic frameworks, plugin platforms, duplicate wrappers, layered forwarding, or entire systems reserved for hypothetical future uses.
- **Excessive security/defensiveness:** security layers, permission systems, repeated validation, fallback chains, implicit degradation, and defensive protocols unrelated to actual single-user use. Handle real errors directly and accurately, without stacking mechanisms.
- **SHA256:** do not introduce or promote SHA256/sha256sum checks, content fingerprints, hash manifests, proof chains, or development workflows dependent on such digests. Do not manually rewrite existing dependency-tool lockfiles or upstream release metadata.
- **Smoke workflows:** do not add, run, or require smoke scripts/processes. Validate the specific change and real behavior.
- **Fixtures:** do not add independent fixture systems, fabricated data deliveries, template environments, or acceptance based on fixed fake results instead of real outcomes.
- **Browser tests:** do not add, run, or require Playwright, browser end-to-end tests, screenshot comparisons, or similar browser testing workflows.
- **Gates:** do not add custom approval chains, quality/security gates, formal review sign-offs, or checks without practical benefit. Preserve necessary CI without unilaterally expanding the process.
- **Freezes/baselines:** do not add frozen manifests/snapshots, baseline archives/proofs, comparative evidence bundles, or similar engineering mechanisms.
- Do not restore `.opensdlc`, SKILL, task documents, report directories, or other extra non-code material. Maintain development guidance centrally in this root document. Retain necessary build/runtime configuration, CI, and program-consumed generated contracts.

These rules govern subsequent development and execution. Existing business code may use similarly named fields or historical implementations. Do not alter scientific semantics, persistent formats, or runtime architecture merely to remove keywords. Architectural changes still require explicit prior approval.

## Making changes

1. Start from the concrete user behavior and locate existing entrypoints, contracts, and call chains.
2. Inspect official NautilusTrader/Pi Durable interfaces and installed versions before deciding how much adaptation is needed.
3. Make minimal changes within existing layers, reusing business rules and components rather than adding parallel paths.
4. Run only necessary type checks, compilation, or relevant unit tests based on impact. Do not run prohibited workflows above or substitute quantity for validation value.
5. Deliver behavior changes, actual validation results, and unfinished work. When asked to merge into dev, complete the remote merge and read back its commit state.

## Actual scope of native research and historical scans

- `native_strategy_configure` saves an explicit official strategy request in the same Pi Conversation. `native_strategy_backtest` executes through the original Task, remote receipt, and bound catalog, without another strategy state machine.
- Current single-instrument backtesting wires official EMA_CROSS (QuoteTick), ORDER_BOOK_IMBALANCE (L2), GRID_MARKET_MAKER (QuoteTick), and HURST_VPIN_DIRECTIONAL (official BAR/QuoteTick/TradeTick subscriptions). Grid-specific fields map individually to official GridMarketMakerConfig; instrument_id comes from the original request and optional base is parsed unchanged by official StrategyConfig. Omitted trade_size/expire_time_secs retain official None semantics. Other numbers/switches are explicit, without glue-supplied trading rules. Arbitrary Python strategies, multiple instruments, and all native strategies are not yet wired.
- All Hurst/VPIN-specific fields map to public HurstVpinDirectionalConfig. Upstream performs window validation and indicator/trading algorithms. Official DataEngine aggregates INTERNAL BARs from actual TradeTicks; EXTERNAL BARs require complete BarType identifiers, not instrument IDs. Different BarTypes for one instrument can be read together. External non-time BARs used only as strategy inputs should explicitly set official venue.bar_execution=false and provide actual Quote/Trade execution data. Omission retains official true and its rejection behavior; do not silently alter matching semantics.
- Replay uses original event/availability windows, official accounts/matching/fees, and native statistics. Do not fill missing values with zero or treat a backtest as independent OOS validation. Account execution still allows only existing EMA_CROSS; registering Grid, Hurst/VPIN, or order-book backtests does not enable account execution.
- Custom parameter-grid enumeration, batch selection, and scan execution are retired. Web/API/CLI no longer create, retry, or cancel-and-resubmit scans. Historical scans retain original requests, Tasks, results, and receipts as read-only state. Necessary stop actions address pending child tasks under original identities, without new backtests.
- Retired research scoring, review allocation, ensemble fit/apply/freeze, custom factor tools, and legacy PM science creation have no compatibility execution. Do not reinstall old plugins for historical records. Rust job no longer depends on the WASM interpreter or Clarabel solver. Historical Wire types and rejection tests may retain identifiers without supporting execution. Preserve actual capital/account and source-data code according to its call graph, not keyword deletion.
- Native compatibility means using official configuration, strategy, indicator, data, and report interfaces, not another strategy DSL. Rust `add_strategy<T>` currently requires a concrete compile-time type. Arbitrary Python `ImportableStrategyConfig` dynamic execution is not wired, and native strategy entrypoints do not execute ad hoc user source. Wired strategies are not all Nautilus capabilities; extend public interfaces without changing upstream.

`market-history hyperliquid-funding` still defaults to saving original official-adapter `FundingRateUpdate` records. Explicitly supplying both `--settled-history-jsonl <original-sidecar>` and `--historical-settlement` selects offline post-hoc settlement adaptation with no network request. Preserve original `event_ns`/rate, setting `next_funding_ns`, `ts_event`, and assumed `ts_init` to that same original timestamp without rounding or computing funding payments. Output must be a new directory. A target native catalog may already contain market data but not funding data; this mode neither incrementally mixes nor overwrites funding. The existing funding manifest records explicit scope before writing data, so failure does not leave unlabeled adapted data.

Using that catalog with `native-strategy-backtest` requires explicit `FundingRateUpdate` selection, not import without replay. Formal results retain post-hoc settlement limitation notes. Actual receipt/publication times are unknown, so do not claim strict PIT or no-lookahead certification; using these rates as signals still risks lookahead. Import provides no historical settlement prices. The official engine uses existing marks or book midpoints, not necessarily actual Hyperliquid payments. Read row counts do not establish all settlements; inspect official position funding adjustments and account events. L2 OBI plus funding retains the native book-to-quote pipeline. Combinations with other selectors such as MarkPrice are outside this wiring scope.

## Market-data responsibility boundary

Native market data is authoritative in Nautilus `ParquetDataCatalog`. QuaZonai stores source adaptation, task receipts, and DatasetConfig references, not another market-data format or catalog copies created by registration. External formats such as CHD still require native conversion. Source receipt clocks and reconnection-duplicate semantics cannot be replaced by generic timestamp deduplication.

Streaming adapters retain only bounded buffering, keep records with the same `ts_init` within a file, and segment contiguous precision versions. Upstream `EncodeToRecordBatch` provides encoding metadata; official catalog APIs perform reads/writes. Pinned upstream rejects mixed precision and overlapping file intervals. Do not delete these source segments as if they were a duplicate storage engine.

## Account capital-exit runtime configuration

The existing remote Nautilus entrypoint explicitly configures an account array through `QZ_NAUTILUS_ACCOUNTS`. Each entry contains `id`, `clientId`, `accountId`, `mode`, and native spot `instruments`. Paper additionally requires `startingBalance` and explicit `paperFeeRate`. LIVE uses `accountAddress` for the master address and `network` for mainnet/testnet. Inject signing keys through the account host's `HYPERLIQUID_PK` / `HYPERLIQUID_TESTNET_PK`, never source code or business requests.

Application account URLs point to `/accounts/<id>/` on the existing Nautilus service and use its credentials. `QZ_ACCOUNT_BINARY` optionally selects the native host, defaulting to `account-runtime` beside the science executable. Account business records and official caches reside in `accounts/<id>/` under the existing Nautilus data directory. Preserve it for original account/pending-order recovery. Build the native host with `cargo build --locked -p job --features account-runtime --bin account-runtime`; existing image scripts package it.

New simulation requests must declare native model version `0.69.0`; old `0.68.0` requests are not silently executed as the new version. Account observation accepts only current `0.69.0`, not old versions. Cache version and upstream revision must exactly match the current pin; otherwise reject explicitly without migrating, relabeling, deleting, or clearing original files.

Nautilus is pinned to official `634e09456c949f488b2bfcf6b6897b33cbf218d7` (0.69.0). Accounts may explicitly set `executionProductTypes` to `["SPOT"]`, reusing upstream product-scope restrictions for orders, private reports, and recovery. Omission still includes all products and preserves old recovery visibility. Before narrowing scope, handle out-of-scope open orders and local pending submissions. Out-of-scope orders are not automatically canceled. Balances and scheduled account-wide cancellations remain account-level; product scope is not account isolation. Retain original directories rather than clearing caches for upgrades. Submission recovery explicitly uses official `RetainUnresolved`: unknown submissions remain unresolved; absence from queries is not rejection/completion. Once the upstream recovery budget is exhausted, automatic per-order rechecks stop. Unconfirmed cancels/modifications may recover their previous accepted state and still require verification.

Capital operations use the account page's capital-exit action or the same HTTP API through CLI `capital context/status/preview/command/get`. Paper cash/withdrawable evidence always remains labeled simulated. Verify live spot balances, withdrawal-path availability, and user-completed external withdrawals separately.

## Common entrypoints and commands

Application `package.json` engines and deployment configuration specify Node versions; `rust-toolchain.toml` pins Rust. Respective lockfiles manage dependencies.

| Purpose | Command |
| --- | --- |
| Install one TypeScript application's dependencies | `npm ci --prefix apps/<application>` |
| Check service, research host, or CLI types | `npm --prefix apps/service run typecheck`, replacing service with `research` / `cli` as needed |
| Check Web types | `npm --prefix apps/web run typecheck` |
| Build Web | `npm --prefix apps/web run build` |
| Run one application's unit tests | `npm --prefix apps/<application> test`, selecting only relevant tests |
| Check Rust formatting | `cargo fmt --all -- --check` |
| Check pure science rules | `cargo test --locked -p contracts -p domain --lib --tests` |
| Generate science contracts | `make generate-native-schema` |
| Start the service | `npm --prefix apps/service start` |
| Start the remote Nautilus adapter | `npm --prefix apps/nautilus start` |
| Use the development CLI | `npm --prefix apps/cli start -- --help` |

If an old version cannot settle an exited empty-venue native backtest during upgrade, explicitly use deployment manager `apply-update --resume-exited-backtest CONVERSATION:OWNER:NATIVE:SUBMISSION`. First enter DRAINING through existing maintenance. The manager uses the original CLI/API to confirm no activity outside that research wait chain before normally shutting down the original app. The target release then checks the same maintenance epoch, original process-exit evidence, and unique wait chain. Preserve original tasks, UNKNOWN, submission, data, and external Nautilus. After startup, resume the same maintenance epoch and reconcile through the research native-failure review API. This neither automatically resubmits nor treats exit as cancellation. Other unfinished work still blocks upgrade.

Science requires actual remote instance/data-path configuration. Never substitute fabricated results for native completion. Run checks necessary for the current need rather than every historical Makefile target.

## Native conditional-order preview

The existing account adapter provides `POST /accounts/<id>/orders/preview` with the same client/account identity contract; see `packages/contracts/src/native-order-preview.ts`. It directly constructs STOP_LOSS (`StopMarket`), TAKE_PROFIT (`MarketIfTouched`), or BRACKET through Nautilus `OrderFactory`, then invokes official Hyperliquid validation. Price/quantity must match the configured instrument's native precision/increments. Bracket uses an IOC Limit entry, OTO association, and OUO reducing exit legs, not a custom trigger/order-group engine.

This entrypoint is preview-only. Its temporary independent OrderFactory does not consume account strategy order IDs, write Cache, submit orders, or enable automatic protection. Current official Hyperliquid activates staged children after the entry fill; do not describe it as exchange-atomic OCO. Sandbox simulation is not evidence of live support. Preview checks neither balances nor market acceptance and does not guarantee stop execution prices. Existing targets/capital exits retain their original IOC path without automatically attaching protective orders.

New paper brackets currently return `account_native_paper_bracket_recovery_unverified`; live brackets return `account_native_live_brackets_not_supported`. Both capabilities are unsupported. Complete verification of intermediate recovery windows, including parent fills/child resizing and exit fills/linked cancellations, is unfinished. Do not market preview, protocol ACK, Sandbox, or steady-state restart tests as live protective-order support. Ordinary non-bracket IOC remains unchanged. This is QZ's actual integration boundary, not a modification or limitation of Nautilus itself.

Existing `nativeBrackets`, `observation cancel-protection`, and `POST /api/v3/observations/:id/cancel-protection` do not imply support for submitting new brackets. Query/cancel existing protective orders through actual account bindings and official states; do not fabricate recovery, automatically allow execution, or clear original caches.

## Native performance and optional price benchmarks

Account returns additionally register Nautilus Calmar, 95% VaR, 95% Expected Shortfall, and Ulcer Index. Ordinary accounts retain 252-day annualization; spot daily reports retain 365 days. Uncomputable metrics remain null with reasons, not zero. Observed Spot hold/rebalance comparisons also return native relative statistics against the actual hold account.

Existing strategy/science `simulationSettings` (native request `settings`) may include `price_benchmark`, a complete `NativeBarSelectionV1`: `schema_version`, single-element `bar_types`, decimal-string `event_start_ns` / `event_end_ns` / `decision_cutoff_ns`, and optional `maximum_rows`. Omission means no price benchmark. This is not compatibility for older science requests.

Benchmarks read the same catalog authorized for the task without downloading or generating market prices. The selected window must include closed prices at the first return day's start and every day end; it may begin one bar before the trading window. Only INTERNAL time bars with known right-open semantics are supported. Quote currency must equal report currency, with actual closed prices and complete account returns at every UTC day boundary. Mismatches return `BENCHMARK_*` reasons without currency conversion, forward fill, or automatic instrument substitution. Output explicitly identifies a price-return benchmark excluding fees/funding and using a zero risk-free rate, not an actual buy-and-hold account. Statistics directly use native Nautilus Alpha, Beta, Information Ratio, Tracking Error, Treynor, and upside/downside capture.

Actual benchmark read cutoff is the earlier of configured cutoff and current simulation.selection.decision_cutoff_ns, preventing future-arriving historical bars when reusing strategy parameters across validation/holdout windows. Comparisons must cover all complete UTC days in the current request. Missing first/last/intermediate days, duplicates, or unknown account returns make them unavailable. Partial boundary days are excluded. Do not present a residual tail of the native snapshot ring buffer as a complete window.

## Current dual-path research assessment wiring

`native_strategy_logic_review` creates an official Pi Durable child Task before executing the configured strategy. The child uses independent official pi-ai context to review declared economic mechanisms, lookahead risks, degrees of freedom, parameter rationale, and execution assumptions. Upstream strategies may be explicitly selected with `strategies` and preregistered complete requests. Omission reviews only the registered strategy, without reading the parent conversation/native results or claiming inspection of unsupplied source. Results bind the original strategy request, configuration Task ID, model, and assessment-rule version. Missing models/invalid responses fail honestly. Logic stars are research judgments, not profit probabilities.

`native_strategy_assessment_report` creates another official child Task to read actual logic-review and native-execution Tasks for the same configuration. It reads explicit backtests/native sensitivity facts; historical WF remains historical. Before execution, data state is `NOT_RUN`. Missing pre-execution methodology or required results leaves combined stars `null`, without model advice overriding absent facts. See below for ratings of complete local methods. Historical backtests/exploratory scans are not independent validation. Old reports remain under original Task IDs; changed parameters do not reuse mismatched logic ratings.

This is the first integration slice of a complete automatic dual-path flow, not a finished data-validation product. Ordinary research backtests retain their existing completion behavior. Review tools may run in later Submissions of the same Conversation without new trading gates, approvals, or capital permissions.

Automatic walk-forward fold generation/training selection is retired with no new calls. Historical WF Tasks, requests, and reports remain read-only, and pending execution may recover/cancel through original receipts. Large historical tables still use bounded streaming parsing; retirement is not a reason to read everything into memory.

For new research, Pi declares complete explicit native requests and training/validation purposes before seeing results, then configures and invokes native backtests sequentially. Each execution has explicit windows, accounts, and upstream configuration. QZ does not generate folds, select winners, or recompute statistics. Original event/availability boundaries, fees, and reports remain unchanged.

Pre-execution logic review may declare multiple complete requests in validationMethod.requests. Each subsequent backtest links to the pre-review through original request, catalog, target, and task timing. Sequential configuration Task ID changes do not lose declared coverage; undeclared requests/old WF shapes do not gain coverage. Combined reports retain all actual attempts/failures in this conversation; completeness across conversations is unverified.

The pre-review tool's `validationMethod` records explicit complete requests, purpose, and neighborhood/cost assumptions, not QZ fold/selection parameters. Sensitivity tools invoke native backtests only for explicit scenarios. Original results, failures, and each call retain their Tasks; cancellation/recovery reuse receipts. Same-window sensitivity is not independent OOS validation.

Synthesis reads actual Tasks, compares preregistered candidates/methods with execution, and presents native statistics, explicit windows/sensitivity, trade counts, and recorded trials. With available evidence complete, an independent Pi child agent judges whether it supports 0–5 stars. Logic/data remain separate and combined stars are not their average. Missing material or insufficient evidence in the agent's judgment leaves `null`. Valid ratings are explicitly `RATED_WITH_DISCLOSED_LIMITATIONS`, assessing only this conversation's recorded method, not probability, comprehensive overfitting certification, or a trading gate. Disclose unperformed CPCV, cross-conversation trial review, or independent source-data audits without mechanically forbidding scoped research judgment. Preserve official statistics; IEEE754 hexadecimal decoding is display-only, and missing/nonfinite values are not zero.

Review binding compares the officially normalized request, actual catalog, and native target. Omission versus an officially equivalent null is not a new strategy or a way to bypass prior-results checks. Identical requests on different catalogs/targets are not combined as one evidence set. This records execution provenance, not an independent data audit, immutable snapshot, or proof. Reports retain native `run.outcome`, notes, and row counts. Official `stopped` is an actual stop, not complete validation. Describe sample scope using original windows/actual records, not longer derived-cache date ranges as raw coverage. Glue does not assume USDT/USDC equivalence.

Complete native research uses the same Pi tool loop: `native_strategy_brief` reads registered original requests/provenance, the agent declares candidates/methods and reviews logic before results, then executes declared native windows, neighborhood/cost checks, and synthesis. `native_strategy_backtest` saves actual history without forcibly ending the entire research by default. An independent single backtest may explicitly set `finishResearch: true`. This integration adds no scheduler, strategy DSL, currency-conversion inference, or live orders. Missing original request/catalog/target or required account/fee assumptions still require actual input; do not claim automatic data registration.

Cost/neighborhood evidence must match the actual explicit strategy configurations executed. Strategy A's results do not cover strategy B. Preserve original fees, changed scenarios, and Task IDs; disclose insufficient coverage. An executed configuration cannot be re-registered as if newly preregistered.

Maximizing training Sharpe only compares actual training candidates; it does not establish OOS or annualized objectives. Retain official return basis, per-fold availability windows, row/trade counts, and selection history so the agent can judge short samples and selection bias, rather than treating a high value as stable annualized performance.

Research detail `assessments` directly reads original Pi review Tasks, returning actual states, original reports, and `CURRENT`/`HISTORICAL` configuration labels by comparing native requests, catalog/target, and configuration Task. It does not recompute stars from agent text. CLI `research get` returns the same field unchanged; Web displays three rating paths, unrated state, limitations, and history. Evidence links `/api/v3/research/:id/evidence/:taskId` read only this research's native backtest/WF/review Tasks with owner authentication, without exposing private tool tasks or other research or adding a second report store.

After historical results are saved, official active Task/Submission state still determines stoppability. Do not hide stop because of `COMPUTED`. User stops and later `model_error` after result persistence both permit same-research continuation. Historical results move to original `previousNativeResults`; original Tasks are not rewritten. Before a new submission, inspect all conversation backtest/WF/sensitivity Tasks and remote receipts. For unresolved work, only read original receipts: do not POST again or convert cancellation/unknown into success. New submissions get new stop tasks rather than reusing completed stop commands.

## Explicit fixed-specification research scenarios

Native strategy requests default to instrument definitions available in the catalog at the window start. Explicit `instrument_definition.mode=STATIC_SCENARIO` with an original catalog definition's exact `ts_init_ns` and `ts_event_ns` supplies that version as fixed simulation rules throughout the window to official `BacktestEngine.add_instrument`. It does not alter original definition timestamps, backfill history, change market data/upstream, or mix other specification updates into the scenario.

Each explicit-window result carries actual definition times and `HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED`. Out-of-time tests only describe temporal separation under this assumption, not verified historical specifications or live evidence. Fees, currencies, and funding models remain separate explicit assumptions with no implicit USDT/USDC conversion. Default strict `CATALOG_DEFINITION_AVAILABLE_AT_WINDOW_START` describes catalog availability only, not an independent historical-source audit.

Omitted `instrument_definition` uses strict catalog-time definitions. Only explicit `STATIC_SCENARIO` with exact actual catalog `ts_init_ns`/`ts_event_ns` means assuming that specification throughout the window. Do not modify catalog/data/definition times or relabel future definitions as historical. Every native result retains definition basis. Strict mode establishes availability at window start, not an independent source-accuracy audit.

STATIC_SCENARIO passes unchanged through configuration, each explicit backtest, and cost/neighborhood candidates; native metrics remain unchanged. It may support limited conditional ratings marked `RATED_CONDITIONAL_STATIC_SCENARIO`, with conditional conclusions and historical specifications always disclosed as unverified. It is not actual historical tradability, live evidence, or automatic trading permission. Reports/API/Web and historical scans retain original definition times/scope without mixing strict historical and scenario bases.

Historical scans display original native Tasks/results read-only: stopped remains stopped, not complete validation. They no longer rank or generate heatmaps. Recovery, viewing, and stopping must not rerun old candidates. Preserve original metrics, timestamps, and data basis.

Native result `equity_performance` only reads official PortfolioAnalyzer portfolio_returns (original equity snapshots), directly invoking official CAGR/Sharpe at 252/365 frequencies independently of canonical defaults that may fall back to position returns. If equity is unavailable, retain the reason without position substitution or implicit currency conversion. Display original snapshot UTC-date count, return-day count, unpriced snapshots, and range. Official cross-day carry-forward is not daily observation; short-sample CAGR is extrapolation, not realized annual profit. WF may explicitly select `EQUITY_PERFORMANCE` and frequency; scans use independent equity metric IDs to distinguish same-named canonical values without new trading gates.

Order-book snapshots are not event-by-event deltas. Reports must disclose known depth, sampling frequency, omitted empty books/invalid levels, and carry/checkpoint semantics. Snapshot matching results do not establish actual high-frequency executability.

Native replay uses official `BacktestNode(chunk_size=4096)` inside existing Rust job, not a new service/engine. Catalog adaptation supplies only exact selected original initial definitions, strict-mode subsequent definitions, and window data. Static scenarios do not mix definition updates. Data `start_time` filters availability; node `start` maps only explicit `available_start_ns`, otherwise upstream infers it from the first complete replay event. Canonical fixed `run_config_id` is `native-strategy`; original account/equity/order/time fields remain unchanged.

Group intersecting, shared-endpoint, and transitively overlapping files using public catalog time ranges. Exhaust/release the current group before opening the next. Within a group, use official per-file merging with `optimize_file_loading=false`, not directory optimization: the latter has been observed to interleave cross-file CLEAR/ADD/F_LAST frames sharing `ts_init`. Official node/engine handles F_LAST aggregation, cross-selector order, same-time blocks, matching, and statistics. Memory still depends on the largest overlapping file group, same-time volume, and order history; fixed RSS is not guaranteed.

Before node startup, native strategies reuse bounded file reads for complete counts/frame-tail validation. `data_rows_by_type` describes raw rows observed by that pre-run scan, not consumed replay rows or a consistent catalog snapshot. Existing notes disclose actual reader rows including prefetch and unverified catalog consistency. Parquet supports Latest only; read-only access does not block other importers. Files added after group discovery may or may not be included, and replacement can change contents without row-count changes. Folds/candidates do not guarantee identical contents; equal counts are not identical inputs. Add no freezes, hashes, or trading gates. A startup-then-stop test selected 9 rows, prefetched 6, and replayed 0 frames, distinct measures. Targeted native_strategy integration tests and same-canonical comparisons cover actual upstream strategies, fees, funding same-time ordering, explicit/default starts, and static/historical definitions. Temporary synthetic data is not historical trading/profit evidence.

This sequential independent-process stress test used 648 temporary synthetic native Parquet files, 2653560 delta rows, and 884520 complete frames with official OBI dry-run. Original preload peaked at 648732 KiB and replayed in 35.91 seconds; deferred-group official node including selected-row pre-counting peaked at 160028 KiB and took 69.59 seconds. Complete canonical results differed only in explicit run labels; capital/time fields were retained. In this sample peak memory fell about 75% and time rose about 94%; do not extrapolate to real 27-day data or live effects. Targeted checks used jobs=1/debug=0. Final targeted retries disabled incremental caching due to shared-build disk pressure, not as a new validation workflow.

This science simplification removed custom BAR capacity estimation, account sizing, target blending, capital-curve planning, old target replay, portfolio numeric optimization, and inventory valuation entrypoints. Retain historical contracts and capital records without implying executable old algorithms. UI displays upstream statistics/raw equity without recomputing annualization, returns, or drawdowns. Preserve negative drawdowns, where nearer 0 means less loss. API numeric sorting stays unchanged; UI initially displays descending order.

## Static wiring of user Rust native strategies

User strategies implement official Nautilus `Strategy` in an independent Rust crate, outside QZ's scientific implementation. Requests may use `strategy: { kind: "REGISTERED", name, config }`, interpreted by the user crate's registration function. QZ has no strategy-name whitelist or DSL and does not infer parameters/rewrite algorithms. A default release `job` without registered user strategies rejects explicitly rather than falling back to four examples.

Existing job exposes `native_strategy::registry::StrategyRegistration`, receiving official `BacktestEngine`, requested `InstrumentId`, name, and original configuration to construct the user strategy and invoke official `engine.add_strategy(...)`. The crate's binary calls `job::cli::main_with_registration(Some(register))`, reusing CLI, log isolation, single backtests, and result transport. Every backtest creates a separate native instance. The crate depends on repository `apps/job` and Nautilus crates at exactly the Cargo.lock revision with unified precision features. New strategy types require build/deployment; configuration changes to compiled types do not. This is neither `.so` loading nor a Python environment.

Deployment reuses remote native service `QZ_JOB_BINARY` and `QZ_TICK_JOB_BINARY`. Native strategies use the latter, falling back to the former if unset; point the appropriate path at the new user runner. Existing `runtimes/native/build-native-image.mjs --job-binary /path/to/user-runner --output-dir /outside/new-directory` packages the actual binary and generates its native manifest. If `QZ_NATIVE_MANIFEST` is configured, install the matching new artifact too; an old manifest/new binary mismatch rejects startup. Preserve catalogs, task receipts, account caches, and login data rather than deleting storage to upgrade.

This seam opens Rust strategy types, not all Nautilus data/account use cases. Requests still use existing single `instrument_id`, selectors, and venue contracts. Multi-instrument, other markets, and live lifecycle require separate native integration. Implemented construction APIs do not establish that a particular strategy compiled, deployed, or backtested successfully.

For multiple instruments/venues or complete engine configuration, the crate may implement `native_configured::ConfiguredRunFactory` and call `cli::main_with_native_hooks(single_registration, Some(factory))`. The factory receives the actual bound catalog root, registered name, and original configuration, returning `ConfiguredNativeRun { config: BacktestRunConfig, install }`. User code builds `config` through official typed builders; `install` adds native Strategies to the new official engine. Unknown names must fail, as does a default job without a factory. Typed factories may install official Actors/Controllers first and dynamically create strategies through upstream lifecycle; QZ does not enforce a pre-run strategy count. Return native results without treating empty/no-trade output as strategy validation. Single-strategy REGISTERED still requires immediate Strategy installation; that narrow contract does not limit full configuration.

`native-configured-backtest --catalog ...` takes `{ schema_version: 1, name, config }` in the existing job/remote worker. QZ does not duplicate multi-venue/data/engine schemas or override `DataEngineConfig`, `emit_quotes_from_book`, official streaming chunk size, or matching parameters. Before execution, check every returned catalog's canonical path remains under this run's allowed root; only local file protocol is currently wired. This path check is not an OS sandbox for arbitrary Rust. Factories must not create alternative data sources/algorithms to bypass official APIs.

`native_results` contains original official `BacktestResult` serialization, without reannualization, scoring, winner selection, or aggregation. Official `dispose_on_completion=false` also returns original `canonical_result`; disposal enabled yields explicit `OMITTED_BY_DISPOSE_ON_COMPLETION_CONFIG` without changing the choice. Official `raise_exception=false` may return empty results, not proof of full-window completion or strategy validity. This entrypoint does not automatically generate the single-instrument `equity_performance` wrapper; do not mix report shapes. Existing Pi Tasks/remote receipts own execution state. Rebuilding a node is not recovery from a mid-run checkpoint.

Accept the multi-instrument/multi-venue interface separately from actual strategies/data coverage. It is not LiveNode account registration, does not remove account lifecycle limits, and starts neither paper nor live trading. User strategies' official save/load, order ownership, and account recovery require separate wiring/verification.

## Static Rust wiring of user account strategies

Accounts retain the independent `account-runtime` protocol and `QZ_ACCOUNT_BINARY`. The crate may call `job::account_runtime::run_with_registration(Some(register))` from a current-thread Tokio entrypoint and retain `build-identity` returning `job::build_identity()`. Depend on the same pinned Nautilus revision, `account-runtime` feature, and high precision, not a new service/dynamic source loader.

Account configuration retains `native_strategy: { instrument_id, strategy: { kind: "REGISTERED", name, config } }` with original JSON `config`. Only explicit START accepted by existing controls invokes the static callback. `AccountStrategyRegistrar::add` registers a concrete official Strategy; official Controller registers it, official Cache/DataActor callbacks recover state, then upstream starts it. Missing callbacks, unknown names, or no registered strategy fail without example fallback. New types require building/deploying the account runner; existing-type configuration changes do not. The callback may read all original account instrument IDs, but the host remains wired to Hyperliquid/Sandbox and existing account models, not every multi-venue LiveNode configuration.

RECOVER neither instantiates/loads user signal strategies nor calls their `on_start`. Under the existing exclusive cache lock, first call official `ExecutionEngine.load_cache`. Integrity failure or any emulated order rejects recovery before connection/start, retaining original order files. The official emulator automatically restores such orders without a disabling setting; HALTED is not proof of no automatic cancellation. Ordinary non-emulated recovery, explicit START, and parameter changes still need official Sandbox and independent user-crate compilation verification. Backtest factories or no-market lifecycle acceptance are not actual account/live acceptance.

## Separate application deployment from user native environments

Default dev/release publishes, installs, and updates only the QuaZonai app/CLI, without bundling or automatically managing Nautilus. Installer operations target Compose `app --no-deps` and do not update/start/stop/delete existing Nautilus containers or account/catalog data. Legacy `--local-runtime` is retired. Owners maintain external services independently.

Upstream NautilusTrader is a library, not this project's HTTP API. Connections currently require the QuaZonai worker protocol: research `/v1/capabilities`, `/v1/execute`, and original request receipts; accounts independently use `/accounts/<id>`. Installing upstream or entering a URL does not establish compatibility. Users may provide their own compatible adapter or explicitly build this repository's optional source adapter, not a default release attachment:

```bash
# In a trusted source checkout, the user prepares official Rust tools and Docker.
bash deploy/docker/native-build.sh release /absolute/new-native-artifacts
docker buildx build --load --target nautilus --file deploy/docker/Dockerfile \
  --build-context native=/absolute/new-native-artifacts \
  --tag owner-quazonai-native:local .
```

This only builds a user-managed protocol adapter image; it does not deploy, connect exchanges, or trade. Explicitly configure persistent directories, catalog roots, worker token, and network when running it. Configure backtest/account instances separately in the app; do not borrow backtest credentials for live trading. Custom strategies use the Rust crate interfaces above; the default runner contains no user strategies. Configure account credentials in the user's native environment, not strategy JSON/chat. Account connection, recovery, and actual trading start are separate explicit steps, never side effects of application updates or saving settings.

This deployment change does not mean arbitrary Nautilus environments are integrated. Direct Rust environment integration still needs independent implementation, without requiring a predeployed QZ worker as the final general solution. The source adapter above is one available explicit path, not a Python strategy runtime. Automatic `restore` replaces app-specific `data/` while retaining current Compose overrides. It rejects other Compose services, known native account directories, or failed checks. Native data should live outside the app directory. Compose cannot prove unrelated processes do not share that directory. Mixed data directories need separately scoped recovery; this tool is not a safe restorer for every native dataset.

## Approved owner Rust process integration: source delivery boundary

This directory contains QuaZonai's own Rust integration source, not Nautilus,
a native executable, a compiler, or a ready-to-run trading environment. The
application release does not download or install those components.

Source discovery, source generation, compilation, connection, and strategy
start are different operations. Saving connection settings never performs a
build or starts a node. Manifest discovery uses an explicitly selected installed
Cargo executable with `metadata --no-deps --offline --locked`; it is not a
compile, dependency-resolution, precision, or factory-visibility check.

The owner project must provide a public Rust library and explicitly selected
factory modules. Backtest modules expose `config` returning the official
`BacktestRunConfig`, and `install` receiving the official `BacktestEngine`.
Account modules expose `strategy(name, config, trader_id, account_id, execution_client_id, instrument_id, instruments)` using the official Nautilus identifier types and `&[InstrumentId]`, returning their concrete
official Strategy type. The generated launcher adapts these entry points; the
owner library does not need to depend on QuaZonai. Bin-only/private functions
cannot be reflected into a library or automatically attached to an existing
running process.

Generate into a separate QZ-owned writable directory. Default builds do not edit the owner's
source or lockfile. The explicit research `prepareOwnerLock=true` build option
is an exception for Cargo.lock only: official `cargo metadata --offline` resolves
the configured owner workspace inside `research.codingWorkspace` before the
existing locked preflight. No hand-written lockfile, update-all, download,
compilation or node execution occurs in that preparation step. Compatible locked
dependencies are retained by Cargo; dependency changes and preflight stderr are
visible in `preflight.log`. Do not use it concurrently with another study editing
or building the same owner. Saving/inspecting settings never prepares a lock. A copied lockfile is only a starting point, not proof that
new glue dependencies resolve. Dependencies, upstream source/revision, precision
features and toolchain must be verified against the actual resulting build.
The glue source currently references its tested upstream revision; do not
silently change the user's Nautilus dependency to match it. Incompatible
versions require an explicit compatible integration, not a successful label.
No network dependency download or toolchain installation is implicit.

## Container access is not host access

The default app container only mounts the installation data directory at its
same absolute path. It does not mount `/home`, host processes, Docker sockets,
SSH agents, or arbitrary Cargo installations. A host path entered in a form is
not automatically accessible. Even an accessible Cargo executable needs its
Rust compiler, cached dependencies, linker, and compatible system libraries.
Do not infer that an executable path alone makes this a build environment.

The application itself does not install a toolchain, modify mounts, set up SSH,
or launch a separate adapter server. If the configured environment is not
reachable from the application process, report that specific missing path or
capability. The default image is not claimed to build host projects out of the
box. A validated build/connection is required before enabling runtime actions;
source generation alone remains `SOURCE_ONLY_NOT_BUILT`.

Explicit launcher builds execute owner and dependency build scripts. Cargo's
`--offline`/`--frozen` restrict Cargo dependency resolution; they are not an OS
network sandbox and do not prevent a build script from accessing the network.
Build interruption is an unknown outcome for descendant processes, not proof of
a safe stop. Preserve its directory and diagnostics until reconciled. Compiler
output goes to that build's log rather than a bounded in-memory capture; disk
write or build failures must be surfaced, and truncated diagnostics must not be
presented as complete. Concurrent cache users receive distinct generated binary
target names and isolated final artifact copies, never a shared executable name.

The first launcher builder supports only the selected installed compiler's host
target. It passes that exact target to both metadata filtering and Cargo build,
and reads the target-specific artifact directory. It does not silently follow
inherited cross-compilation settings; cross-compilation is not supported by this
entry point. Target-specific dependency caches may require extra disk space.

The account factory receives the original node/account/client identities and configured instrument IDs only at explicit START. These are routing context, not proof of connectivity, balances or trading permission. Older two-argument factories must update their signature and rebuild; source generation does not prove compilation or compatibility. User configuration is passed unchanged; no identity is injected into its JSON.

### Native backtest execution-model assumptions

`native-strategy-backtest` accepts optional venue fields for the tested Nautilus
0.69.0 revision `634e094`: `latency_model` with `kind: "STATIC"` and decimal-string
`base_latency_ns`, `insert_latency_ns`, `update_latency_ns`, `delete_latency_ns`;
and `fill_model` with `kind: "DEFAULT"`, `prob_fill_on_limit`, `prob_slippage`
(upstream `[0,1]` validation, including its numerical tolerance) and optional
decimal-string `random_seed`.
The official static model adds base latency to each operation delay. Omission
keeps the previous upstream defaults. These are explicit scenario assumptions,
not measured exchange latency or calibrated queue/impact execution. They neither
supply missing order books nor certify historical fees, fills or profitability.
This limited wiring does not claim all upstream fill models or versions are
supported. Account runtime configuration and existing account history are unchanged.

### Explicit Chainticks observation use in owner factories

`NativeRustEnvironment.chainticksFillObservations=true` requires an explicit
`backtestFactory`. The generated launcher compiles the existing
`chainticks-observations` data-type feature and registers that Arrow type before
calling the owner factory's config function. Omission does not enable it; this
selection does not enable account-runtime or force a precision change. Saving a
selection does not rebuild or connect an existing runner.

The pinned upstream BacktestDataConfig does not accept Custom data. A reviewed
owner factory may use the existing install hook, official catalog
`query_custom_data_dynamic`, `BacktestEngine.add_data`, and an official
DataActor/Strategy subscription in oneshot mode. The owner can read the original
fields through `CustomData.data.to_json()` without duplicating the source type.
This loads a bounded selection in memory; it is not a streaming adapter. The
factory still specifies the bound catalog, window and subscription. Source
observations are not market TradeTicks, and do not provide execution liquidity,
deduplication, complete coverage, historical instrument rules or a strategy.

### Explicit research source authoring

The optional service configuration `research.codingWorkspace` names an existing
source-only directory accessible to the service. No default workspace or host
mount is created. It enables official Pi Durable `read`, `write`, and `edit` in
native research conversations, with an official `NodeExecutionEnv`; literature
and assessment conversations do not receive these tools. The directory is shared,
so concurrent studies must use separate source files and preserve each other's
work. Files survive service restart. Do not place credentials or held-out data in
this source workspace. Path checks prevent these tools following outside paths
or symlinks; the workspace is not an operating-system sandbox. Unrestricted
`bash` is not enabled.

Source authoring does not configure a toolchain, compile, rebind a runner, or
connect an account. Set the existing explicit owner Rust environment to the
intended project separately, and use the existing `native_runner_build` and
original build-status task, then per-research runner binding. Cargo build scripts
retain their existing execution semantics; these file-tool boundaries do not
sandbox compiled owner code or establish OOS isolation for arbitrary code. Keep
separate native execution data scope. The container still cannot see unmounted
host toolchains. Never report source-writing verification as a successful build.

### Container-owned Rust compilation

The application image includes the native C linker, pkg-config and OpenSSL
headers needed by its existing Rust builder. This does not install/select an
owner toolchain, enable account execution, or replace the separately configured
Nautilus service. An operator must expose a real, matching Cargo/rustc toolchain
read-only inside the application container and configure its actual Cargo path;
a host-execution wrapper is not that toolchain. Cargo's offline source cache must
be a separate writable, credential-free directory. Do not mount a credentialed
host Cargo home or OOS/account data into a source workspace.

Explicit builds share the QZ-owned `dataDir/native-build-cache` through the
existing product builder and Cargo's own locking, while preserving separate
request directories, owner source/lock files and original Pi task outcomes.
Cache reuse is not a disk-space guarantee. Check free space and measured build
cost before admission; a mounted compiler or successful capabilities inspection
is not proof that a new strategy was compiled or run. The opt-in native CI builds
inside the application container with networking disabled and records image,
cache and output sizes; it does not run market-data research or publish/deploy.

## Explicit review of retained native failures

`POST /api/v3/research/:id/native-failure-review` provides an explicit reconciliation seam only for the original empty-venues configuration rejection of an old `native-configured-backtest`. Requests must include original `nativeTaskId`, `ownerTaskId`, `requestId`, classification `EMPTY_VENUES_REJECTED_BEFORE_ENGINE_BUILD`, and `sourceReview`/`sources` reviewing the original owner factory and rejection call chain. It is not an automatic log classifier. Exit codes alone cannot establish that arbitrary user Rust had no side effects. Callers must inspect original source and execution evidence first, without rerunning the request to “verify” it.

In one official Pi transaction, the service checks original tasks, bindings, saved exit facts, and exact original stderr, then adopts a FAILED result through existing `NativeCalls.adoptedReceipt`. It does not rewrite the native Task's UNKNOWN terminal state, request, stderr, or process facts. Original uncertainty and review provenance remain retained; the original backtest Task settles through its existing reconcile failure path. This produces no backtest result, automatic resubmission/new research, or trading permission. Different exits, termination, diagnostics, commands, or mismatched identities remain UNKNOWN. This interface does not settle unresolved account or data-import executions.

### Model discovery and Ultra collaboration

Authenticated OpenAI/ChatGPT catalogs refresh through official Pi provider hooks;
the latest successful capability contract is retained through `ModelsStore` for
original-task recovery. A discovery failure is not a current availability claim.
An identity-only API-key catalog does not invent capabilities for unknown IDs.

`Ultra` is a requested collaboration mode, not a literal provider effort. The
research intake retains the original request, configures the catalog's supported
`multi_agent_reasoning_effort` (otherwise Max, otherwise the last supported
non-Ultra level), and explicitly enables `qz.ultra-delegation`. Ordinary research
does not receive this extension. The UI distinguishes the requested mode from the
retained official `pi.agent` configuration; neither proves a request succeeded.

`ultra_delegate` uses official foreground task-owned conversations and replay-safe
submission IDs. Independent calls can run in parallel, and parent cancellation
reaches the children. Children receive only the explicit assignment/material,
the same model/actual effort and service tier. They perform analysis, review, and
synthesis without research, source-file, market, or trading tools and without
recursive delegation. The parent owns source retrieval, scientific execution,
file changes, result integration, and final delivery. This is a bounded Pi-native
collaboration mode, not the full Codex agent environment. Child answers are not
new empirical evidence. UI results are projections of original Pi identities,
submissions and ordinary final text, never a second result ledger.
