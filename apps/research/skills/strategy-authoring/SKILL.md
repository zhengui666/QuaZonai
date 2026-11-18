---
name: strategy-authoring
description: Write, build, and run Nautilus-compatible Rust strategies as an internal Pi Durable research agent. Defines interfaces, not permitted strategy categories.
---

# Rust Strategy Authoring Contract

**This contract defines interfaces and execution, not which strategies you may write.** Any strategy supported by the configured native Nautilus capabilities may be implemented. The research objective determines strategy type, trading logic, parameters, instrument count, and frequency; there is no whitelist of preregistered strategy names. The data, orders, and complete example below illustrate interfaces, not a mandatory template. Combine native Strategy, Actor, Controller, and their public capabilities as needed. What can actually execute depends on the configured data, adapters, and version.

This reference applies to an **explicitly configured, trusted owner Rust crate**, not dynamic Python, compilation in arbitrary directories, or changes to QuaZonai/Nautilus. First read `native_runner_preparation_status` and the owner's Cargo.toml, src/lib.rs, and existing factory. The Rust APIs shown here correspond to Nautilus `634e09456c949f488b2bfcf6b6897b33cbf218d7`. If the environment uses another version, verify its signatures rather than copying develop documentation. Use the same official revision and compatible features for every Nautilus crate to avoid incompatible duplicate types. Do not modify upstream or implement a replacement matching, indicator, or return-calculation engine.

## Your responsibilities

Turn the accepted research objective into strategy source, builds, native execution, and evidence-based evaluation. Work in the workspace bound to the current research using its native task tools. Retain original task IDs for builds, executions, and reports. Deliver conclusions, the actual scope of verification, limitations, and actionable next steps in the research reply. If necessary environment configuration is missing, report the specific requirement to the operator rather than changing product configuration or pretending execution succeeded.

Internal logic assessment uses registered implementation-source materials and `native_assessment_read_source`. Data synthesis assessment uses the original task's evidence manifest and paginated `native_assessment_read_exposure`. Base judgments on what was actually read, not on treating the parent research summary as independent verification. Registering materials and reading native results do not restrict strategy categories or trading logic.

For result presentation, see [Native result charts and evidence](references/native-result-charts.md). The same guide is included in the internal research context. Web charts read original task tables; their display is not proof that an assessment agent read the evidence. There is no `render_chart` tool.

## 1. Owner and launcher responsibilities

The owner is a library crate with a configurable factory module. For example, selecting `research` requires `pub mod research;` in src/lib.rs. The configured module must expose these exact signatures:

```rust
pub fn config(catalog: &std::path::Path, name: &str, config: &serde_json::Value)
    -> anyhow::Result<nautilus_backtest::config::BacktestRunConfig>;
pub fn install(engine: &mut nautilus_backtest::engine::BacktestEngine,
    name: &str, config: &serde_json::Value) -> anyhow::Result<()>;
```

`name` selects an owner implementation; `config` parses and retains explicit parameters. The config function returns a **native BacktestRunConfig**, not ConfiguredNativeRun. The QZ-generated launcher supplies that wrapper and an install closure calling the owner's install function. Do not handwrite or replace the generated launcher/main, or assume the current global runner has been updated.

The exact execution order is: factory config → native BacktestNode construction → node.build → retrieve the matching BacktestEngine → owner install → node.run → native results. The owner must not create a second node, call run, or implement its own replay loop. Register with `engine.add_strategy(strategy)?` (not a Box) or `engine.add_actor(actor)?`; use `engine.add_instrument(instrument)?` and `engine.add_data(data, None, true, true)?` when needed. Do not trade from config or present another process as this execution.

## 2. Writing a Strategy

The strategy struct contains `core: StrategyCore`. Construct it with `StrategyCore::new(StrategyConfig { strategy_id: Some(...), order_id_tag: Some(...), ..Default::default() })`. Use `nautilus_trading::nautilus_strategy!(Type)`, then implement lifecycle/data callbacks through `impl nautilus_common::actor::DataActor for Type`. Give each instance a unique strategy ID and order ID tag.

These are common callbacks, not an exhaustive list of supported data. Bars, order books, timers, and other native APIs are available in the configured version as appropriate.

| Data | Subscription in on_start | Callback signature (returns anyhow::Result<()>) |
| --- | --- | --- |
| TradeTick | `self.subscribe_trades(instrument_id, None, None)` | `fn on_trade(&mut self, tick: &TradeTick)` |
| QuoteTick | `self.subscribe_quotes(instrument_id, None, None)` | `fn on_quote(&mut self, tick: &QuoteTick)` |
| CustomData | `self.subscribe_data(data_type.clone(), None, None)` | `fn on_data(&mut self, data: &CustomData)` |

These are not Python's `on_trade_tick` / `subscribe_trade_ticks`. Use the version's official indicators and update methods after checking their signatures. The market order below illustrates a call signature, not an order-type restriction. Use native APIs for limit/stop orders, amendments, cancellations, and other operations. This example requires `use nautilus_trading::strategy::Strategy;`:

```rust
let order = self.order().market(
    instrument_id, side, quantity,
    None, // time_in_force
    None, // reduce_only
    None, // quote_quantity
    None, // exec_algorithm_id
    None, // exec_algorithm_params
    None, // tags
    None, // client_order_id
);
self.submit_order(order, None, None, None)?;
```

Use the actual instrument's precision and minimum quantity. Side, quantity, and timing come from the requested strategy, not this snippet. Reuse native Strategy/Cache/Portfolio for order callbacks, cancellations, positions, funding, fees, and statistics. Signal-only observation may use `DataActorCore` + `nautilus_actor!`, but callback counts are not trading returns. Registering a Strategy that never orders does not produce profitability evidence.

## 3. Backtest configuration and data

- BacktestRunConfig needs at least one actual venue with explicit OMS/account/book types, starting_balances, and fee_model. Observation-only runs must also specify the venue and fees.
- `BacktestVenueConfig::builder().fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()))` accepts **FeeModelAny**. Do not copy the handle conversion used by lower-level SimulatedVenueConfig. Zero fees are only appropriate for an explicitly declared no-order integration check. Return research requires sourced fee assumptions and verification of actual fill charges; do not assume zero costs.
- Keep start/end nanoseconds, prices, quantities, and amounts as exact strings/native types. Use native DataConfig/catalog queries and windows supported by actual coverage. A query range does not prove every file record falls inside it. Distinguish ts_event from ts_init; do not infer real event order from tid sorting.
- Ordinary market data is loaded through native BacktestDataConfig. This version does not accept Custom through that path. Use the public install hook to load registered CustomData for replay by the same Nautilus engine. This is bounded in-memory loading, not unlimited streaming. Read only approved training catalogs/windows.
- The configured path currently accepts file catalogs only and checks the real config.data paths against bound roots. Owner-side queries must use the same bound catalogs and approved scope, not out-of-sample data.
- `dispose_on_completion(false)` retains the canonical/equity results read by QZ. Results unavailable with true do not mean zero returns.

### Chainticks account-fill conventions

This section applies only when this source is selected. Other strategies need not use Chainticks or restrict themselves to aggressive-fill analysis.

The configured launcher must enable the corresponding source import and call `job::chainticks_fill::register_observation_type()` before the factory. Use the registered ChainticksFillObservation, not a new Rust type with the same name. Dynamic access uses `data.data.to_json()?`; actual fields include `source_row_zero_based` and `raw_envelope` (event and block), not guessed row/index/fill fields.

Account fills **are not a complete public trade feed**. Ordinary aggressive-fill selection must consume `classification == "taker_fill_observation"`, not merely crossed=true. Preserve the meanings of maker_fill_observation, special_event_observation, unknown_fill_observation, and unsupported_fill_observation. Maker fills are passive executions and may support appropriate research; special/unknown/unsupported events are not ordinary aggressive fills. tid=0 alone is not a deletion rule. Distinguish source-row identity, replay, both account sides, and trade identity; retain original rows. Special events such as Spot Dust Conversion may have crossed=true. Account fills alone do not establish order-book state, slippage, or executable returns.

## 4. Complete, compilable no-order owner example

The packaged [assets/research.rs](assets/research.rs) is a complete module, not pseudocode; the internal research agent also receives its full text. It uses a native Strategy to consume registered CustomData and verify loaded/received counts without ordering or fabricating returns. It validates lifecycle and integration only. It does not require real strategies to use CustomData, this source, no-order logic, or the example's names/parameters. Replace the implementation completely when needed to implement any native-compatible strategy serving the objective.

The owner Cargo dependencies need anyhow, serde_json, and the same pinned revision of nautilus-backtest (streaming), nautilus-common, nautilus-core, nautilus-execution, nautilus-model, nautilus-persistence, and nautilus-trading. Preserve the environment's features and locked dependencies; do not copy a lockfile from another version. Example configuration:

```json
{"schema_version":1,"name":"custom-callback-check","config":{"catalog":"ACTUAL_REGISTERED_TRAINING_CATALOG","start_ns":"ACTUAL_TRAINING_START_NS","end_ns":"ACTUAL_TRAINING_END_NS"}}
```

Replace these resource placeholders with real evidence before submission and check request fields against this version's schema. The example does not register a dataset or create a catalog. Do not use out-of-sample data for integration checks.

## 5. Edit → build → bind → verify

The internal research agent uses official read/write/edit tools in the explicit workspace, not a shell. Use the actual tools:

1. `native_runner_preparation_status {}`: read the real owner/factory, revision, and available environment.
2. Edit owner source. After changing Cargo.toml dependencies, call `native_runner_build {requestId, expectedRevision, prepareOwnerLock:true}` so official Cargo prepares the lock offline. Usually omit that flag when dependencies are unchanged. Never hand-edit Cargo.lock or concurrently edit/compile the same owner from another research task.
3. `native_runner_build_status {requestId}`: follow the original build's status and preflight/build diagnostics until actual success. Running/checkpoint-unknown is not terminal; do not resubmit a terminal UNKNOWN under a new ID. Correcting a compile error requires a new build while retaining the old failure.
4. Call `native_research_start_bound` with the completed build's requestId, current configuration revision, and actual dataset/model. It creates **a new research explicitly bound to the new binary**. It does not update an old research executor and does not require a global native_runner_select change.
5. In that research, read the brief and actual request. First record complete planned requests, windows, and assumptions with `native_strategy_logic_review`; then set factory/name/config using `native_configured_configure {request: complete native request}` and execute the exact configuration with native_configured_backtest. Declare windows/parameters/assumptions first. A successful build is not successful replay. Inspect original task state, stderr, callbacks, native results, orders/fills, and fee evidence. A no-order example demonstrates integration only.

Use the actual internal tools and bound capabilities described above. Do not invent capabilities or automatically resubmit an original UNKNOWN execution.

## 6. Backtest factories are not account strategies

This factory builds BacktestRunConfig only; it does not authorize or start paper/live execution. Keep this research workflow separate from account operation. Do not add wallet access, account connections, or live orders to the integration example or infer trading permission from backtest success.

For source/logic review after an existing execution, use the same native_strategy_logic_review with explicit `reviewTiming:"POST_EXECUTION"` and an authorized sourceRequestId. This creates a new independent review Task. It does not rerun the backtest, replace earlier pre-execution logic ratings/reports, or retroactively satisfy a pre-execution protocol. The child still reads only the specified source materials; it does not automatically receive historical backtests or the parent transcript. Read the original review and execution evidence separately when combining them. Omitting reviewTiming still requests pre-execution review, which rejects requests with an existing execution.

When a research workspace is configured, a synthesis assessment child may use `native_assessment_read_report {taskId, offset, limit}` to read request.config.report from an original native Task in its assessment manifest. This reuses official Pi read line pagination and does not execute a backtest. The report is the current file at the path declared by that Task, not automatically an immutable native result or proof of its producer. Read the required pages, retain truncation warnings, and disclose unread material. A parent summary is not independent reading.

### Formal import of existing market data

Check preparation status for an owner environment with `marketImport:true`; if it is missing, report the required configuration to the operator. The next native_runner_build compiles the existing market-history importer entrypoint; existing research/builds remain unchanged. This entrypoint uses high precision: the owner's official dependency graph must explicitly select the same precision, or the build reports rust_market_import_requires_high_precision_owner. It never silently changes strategy precision. Select official high precision through the owner's own Cargo feature, prepare the lock using official Cargo, create a new Build, and read its actual capabilities. Do not rewrite old capability receipts or substitute a global runner switch for binding.

With a registered catalog, a newly bound research can use native_market_import. Without a catalog, do not borrow an unrelated dataset or create a placeholder research. Follow the formal preparation sequence:

1. In the current ordinary research, call `native_runner_market_import {runnerBuildRequestId: "original requestId of a successful Build", expectedRunnerRevision: current instance configuration revision, request: complete NativeMarketImportRequestV1}`. At minimum, the request is `{schema_version:1, manifest_path:"authorized absolute manifest path", instruments_path:"absolute native definitions path", output_path:"new output directory"}`. Optional start_seconds/end_seconds are strings. path_prefixes is an array of relative directory prefixes, each ending in `/` (for example `trades/2026-04-14/`), not individual filenames. Omit it or use an empty array when all manifest files are authorized; do not guess prefixes. Select individual files through the manifest's explicit files list. Optional modality defaults to TRADE_TICK; use BOOK_SNAPSHOT for full book snapshots, not claims of source incremental updates. The manifest can declare parquet-zstd; map fields from actual source columns. Each snapshot becomes native OrderBookDelta Clear + Add for every level, with no depth truncation. By default, book prices/sizes use the supplied instrument precision and must remain exactly equal after conversion. Nonzero excess digits cause an incomplete import rather than rounding or silently skipping markets. The definition is an input scenario, not proof of historical validity. TRADE_TICK retains native encoding precision. Source order_count is not a native delta field; do not claim it is retained. `received_time:true` selects by receipt time and requires an explicit maximum_receive_delay_ns string; otherwise selection uses event time. Do not mix clocks or invent timestamps. This native contract does not support bars/resume. The tool reuses the original native import Task and resolves its target from the successful Build only; it does not change the current research's old execution binding.
2. Read the original import result's catalog_path, report_path, and the required report content. For large results, follow the returned instructions and use paginated native_assessment_read_exposure. Acceptance of a tool call is not successful import.
3. Use official write to place a dataset-descriptor JSON object or object array in the configured research source workspace. The existing contract is below; replace every placeholder with actual evidence:

```json
{"id":"NEW_DATASET_ID","name":"ACTUAL_DESCRIPTION","assetClass":"CRYPTO","currency":"ACTUAL_QUOTE_CURRENCY","source":"ACTUAL_SOURCE","start":null,"end":null,"ready":true,"nativeModality":"TRADE_TICK","data":{"catalog":"catalog_path returned by the original import"},"policy":{},"preparation":{"archiveLoaded":true,"nativePrepared":true,"scientificQualification":"NOT_ASSESSED"}}
```

The TRADE_TICK nativeModality above is a trade example. Omit this optional field for book snapshot descriptors: the current registration contract accepts only BAR/TRADE_TICK, so do not mislabel books or insert unsupported BOOK_SNAPSHOT. The actual book catalog type is OrderBookDelta, queried/replayed with native selectors. Import-request modality and descriptor nativeModality are different contracts.

id/name/assetClass/currency/source must be nonempty strings. start/end are explicit date-time strings or null; do not guess coverage. ready is a boolean and may be true only when the data is actually usable. data is an object; relative catalog paths resolve against the descriptor file's directory, though an actual absolute path is preferable. Preparation is not research qualification. Disclose conflicts, missing coverage, late definitions, and other report limitations; do not fabricate returns or admission. Do not include credentials or unauthorized data in the descriptor.

4. Call `native_dataset_registration_status {}` for the **data-source** revision, which differs from the instance configuration revision. Then call `native_dataset_register {requestId:"stable ID for this registration", expectedRevision: data-source revision, path:"absolute descriptor path inside the workspace"}`. This reuses the existing append-only registry and validates real workspace paths, including symlinks. Replaying the original request returns its receipt without overwriting an old dataset.
5. Call native_research_start_bound with the actual registered datasetId, successful Build's original requestId, current instance configuration revision, and actual model. Import, registration, and binding are distinct facts. Do not change the global runner or rebind old research.

Retain original requests, outputs, and failure records; do not resubmit UNKNOWN. Late definitions must retain their actual ts_init and be disclosed as a static scenario, never backdated into historical definitions. Import success is not data admission, valid out-of-sample evidence, or a completed strategy backtest.

### Catalog queries and import receipt linkage

native_strategy_catalog data_types uses native NautilusDataType names such as `["TradeTick"]`, not dataset modality names such as `["TRADE_TICK"]`. Omit the filter to discover actual selectors when needed, then reuse returned names exactly and paginate with next_cursor. Query-planning failure does not mean data is missing or replay occurred; do not recast an error as success.

Import tools return the original import taskId for linkage and paginated reading, both for small results and large-result prompts. catalog_path/report_path and the real manifest/report remain direct preparation evidence. Task IDs are not mandatory strategy parameters. Do not add unnecessary ID requirements to an observer or general full-market workflow. For older small results without an ID, ask the operator to retrieve the original Task linkage rather than rerunning import.

Native simulated venues for perpetuals/futures must use an instrument-compatible AccountType::Margin. An observer that never orders still cannot use Cash: the pinned upstream rejects loading perpetuals/futures into a cash account in add_instrument. Configure equities/spot according to their actual account semantics; this is not a Margin-only restriction on all strategies. Simulated account types, balances, and leverage do not imply a live account connection or trading permission.

A formal native_runner_build request may include `optimizationLevel: 0 | 1 | 2 | 3`, controlling only the new build's compiler optimization. Omission preserves prior behavior and changes neither owner configuration nor old Builds. A different optimization level requires a new requestId; replaying an original requestId must retain its original parameters. This is not an executable-import option and does not automatically select a runner, rebind research, or start a backtest.

### Formal binding of multiple registered catalogs

When a configured native backtest needs catalogs from multiple registered datasets, such as separate books and trades, use datasetId for the primary dataset and optional additionalDatasetIds for the others in a new research. native_research_start_bound retains each original root and registration identity. It does not broaden access to a parent directory, copy data, or permit bypass reads from install. Old research bindings do not change.

Read the primary/additional dataset identities and catalogs in native_strategy_brief.catalogBindings first. native_strategy_catalog defaults to the primary catalog; pass an exact datasetId to inspect another catalog already bound to that research, and read the required pages. The configured factory still receives the original primary root. Explicit config may use bound additional paths to construct multiple official BacktestDataConfig entries. Only native_configured_backtest and configured parameter batches use additional roots; the single-strategy adapter still uses the primary catalog. Binding does not prove complete data, historical rules, independent out-of-sample evaluation, or scientific qualification. Assessment materials must explain each source's actual use and limitations.

QZ does not automatically deduplicate or choose between overlapping records for the same instrument/type across catalogs. Preserve Nautilus's native multi-stream behavior and disclose selection, temporal overlaps, and equal-timestamp ordering. Total bound rows are not the number of records actually consumed.

Multi-root execution requires a new runner with actual capabilities.research.multiCatalogBindings=true. An old build lacking that field does not support it. Rebuild with current formal glue and create a new research binding; do not edit old capabilities or research. Single-root execution does not require this additional capability.

### Explicit native post-run completion checks

To inspect and write final materials after native node.run() returns, check that the owner environment enables backtestCompletion:true. If needed, report that configuration requirement to the operator. Provide this function in the same backtestFactory module:

```rust
pub fn finalize_after_node_run(
    node: &mut nautilus_backtest::node::BacktestNode,
    config_id: &str,
    results: &[nautilus_backtest::result::BacktestResult],
    name: &str,
    config: &serde_json::Value,
) -> anyhow::Result<()>;
```

Keep the original config/install factory contract. A new generated build retains parameters and registers the optional callback. Omitted/false preserves the old interface without requiring existing modules to add a function. The callback requires dispose_on_completion=false and runs only when the original node.run() succeeds and returns a native result matching the configuration. Upstream may already have called clear_data, but the original node/engine/actor remain inspectable. Do not claim this occurs before clear_data.

A successful native result alone does not prove natural EOF. The owner must check the original canonical outcome and its own processing state, distinguishing normal completion, early stop, and missing materials. Neither on_stop nor Ok alone proves EOF. A callback error retains failure and performs cleanup; do not claim success. Saving the option does not change old Builds or existing research: build again formally and create a new binding. This interface adds no orders, bypass market reads, or scoring logic.

### Explicit book-quantity rounding

Lossy book-quantity transformation requires explicit authorization; strict lossless conversion remains the default. Declare `quantity_rounding: {"method":"DECIMAL_HALF_UP","instrument_ids":["EXACT_NATIVE_INSTRUMENT_ID"]}` only in an authorized manifest book_snapshots stream. List the actual authorized markets, without wildcards. Only positive quantities for those instruments are rounded to the supplied instrument's size_precision using Decimal half-up (ties round upward). Prices and other markets remain strict. This changes neither raw data nor static definitions and does not establish historical definition validity. TRADE_TICK does not accept this policy.

The book_quantity_rounding report lists processed_levels, changed_levels, changed_snapshot_groups, rounded_to_zero_levels, target_size_precisions, and exact string totals signed_quantity_error, absolute_quantity_error, and maximum_absolute_quantity_error by native market. These are processed input levels, including snapshots later rejected, not traded volume, successful output, or returns. Zero-rounded levels are not silently deleted: the original full-snapshot conversion rejects the group with NONPOSITIVE_BOOK_SIZE, the import is INCOMPLETE, and reports/partial output remain. There is no atomic rollback guarantee. Check the actual report and confirm that no levels rounded to zero before claiming every level was preserved; a policy declaration is not a result.
