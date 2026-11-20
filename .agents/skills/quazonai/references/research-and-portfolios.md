# Native research, parameter grids, and evidence assessment

## Natural-language research

Read `quazonai datasets`, `quazonai research models`, and `quazonai schema show ResearchStart` first. Use real ready datasets and models; do not invent directories, coverage, or fees.

```sh
quazonai research start --goal "$goal" --dataset "$dataset_id" --model "$provider:$model_id" --request-id "$request_id"
quazonai research get "$research_id"
quazonai research diagnostics "$research_id"
quazonai research wait "$research_id" --timeout 60
```

`research_id` is the accepted receipt's conversationId. Use `--file` for complex requests without mixing it with simple parameters. Select reasoning levels and service tiers from the actual model list. Public literature queries may include only content the user authorizes for external disclosure.

Submit custom strategies through `research start` or terminal-state `research resume`, explaining goals, strategy logic, parameters, and data scope to QuaZonai. Strategy families are not restricted to a built-in name list. The internal Pi Durable research agent follows its own skill to author, build, bind, and run Nautilus strategies. External agents must not directly edit strategy source or call internal tools under this operating skill. If a source workspace is needed, first confirm a usable product environment is configured; otherwise report the blocker rather than claiming execution.

Pre-registered entries such as EMA_CROSS, GRID_MARKET_MAKER, ORDER_BOOK_IMBALANCE, and HURST_VPIN_DIRECTIONAL are conveniences, not a strategy whitelist. External operation submits goals and accurate parameters, tracks receipts, and reads evidence.

Official DataEngine aggregates Hurst/VPIN INTERNAL BARs from original TradeTicks; EXTERNAL BARs require the original complete BarType. If external non-time BARs are only for strategy consumption, explicitly configure native venue.bar_execution=false and supply original Quote/Trade data for execution. Omission retains the official default true; glue does not silently disable it. Research/account factory registration and capabilities are independent; research configuration does not expand account execution capabilities.

To start research directly against an existing local Nautilus catalog, pass its absolute path as seen by the QuaZonai service instead of a dataset ID:

```sh
quazonai research start --goal "$goal" --catalog "$catalog_path" --model "$provider:$model_id" --request-id "$request_id"
```

This uses the catalog in place. It does not discover, import, or register data. Use a catalog the operator has already made available under the service data directory to its local process runner. For a saved JSON request, use the top-level `catalog` field instead of `datasetId` and `additionalDatasetIds` and retain that request and its ID.

Inspect actual data types and coverage first. Data preparation does not establish profitability. Preserve missing or un-warmed statistics as unavailable and retain official metric names, currencies, and units. Backtests do not start accounts or turn historical target points into live strategies.

## Read the process and native results

`research get` reads the current summary, task identities, and assessments. Use `research messages` for user goals, continuation instructions, and agent replies, passing `nextCursor` unchanged to read earlier pages. Messages within each page are chronological, while pagination moves backward from the latest page. An empty page may still have nextCursor; continue. A summary is not the entire research process.

```sh
quazonai research messages "$research_id"
quazonai research messages "$research_id" --cursor "$next_cursor"
quazonai research native-runs "$research_id" --result-task-id "$result_task_id"
quazonai research native-runs "$research_id" --result-task-id "$result_task_id" --offset "$next_offset"
```

Obtain `result_task_id` from current details at `final.nativeStrategyResult.resultTaskId`; preserve it while following that result's `nextOffset`. native-runs reads native run metadata without loading entire large market/order tables. Use native-backtests/evidence below for per-group requests and results. If results change, reread details rather than splicing two results into one run. Historical tasks remain readable by original taskId.

## Continue and recover the same research

Retain conversationId and submissionId. `watch`/`wait` timeout only detaches the client. To stop, use `research stop ID` and read back state. After `diagnostics` confirms a terminal submission:

```sh
quazonai research resume "$research_id" --submission-id "$terminal_submission_id" --instruction "$reply"
```

Use instruction to answer a question, redirect research, or continue ended/stopped research. Do not recreate running work. Preserve the new receipt identities. The web research page displays the same Pi messages and supports replies; the CLI has no separate chat-send command.

## Parameter grids and independent native backtests

Submit new grids through the same `research start` or terminal-submission `research resume`, not retired `parameter-scan options/preview/create/surface`. State the installed strategy/factory, explicit candidate values for each parameter, training/validation windows, costs, and purpose in goal/instruction. Do not require users to handwrite tool JSON or invent substantive assumptions.

QuaZonai's `native_parameter_grid` produces Cartesian combinations from a complete native request and discrete axis values. Paths are arrays of existing field keys; axes cannot overlap and the last axis varies fastest. Original JSON values and decimal strings are preserved. There is no numerical range arithmetic, automatic splitting, or optimization. The tool pages `requests`, `totalCandidates`, and `nextOffset`; keep the same input while reading required pages, rather than treating the first page as all candidates. This is an internal research agent tool, not a same-named CLI command or Nautilus native optimizer.

Declare all candidates, purposes, and independence assumptions before numerical execution. Validation research first records complete original requests with `native_strategy_logic_review`. Then pass the generated complete requests to `native_parameter_batch`: `configured=false` uses official strategy requests; `true` uses installed Rust factory requests. Pi Durable sequentially orchestrates independent child tasks; Nautilus executes each backtest and supplies native statistics. Failure stops later groups. UNKNOWN retains the original task for reconciliation, not resubmission or identity replacement. Supported entrypoints also depend on the bound runner's actual capabilities; an application update does not automatically provide new factories or import capabilities.

```sh
quazonai research native-backtests "$research_id"
# When nextCursor is non-null, pass it unchanged for the next page.
quazonai research native-backtests "$research_id" --cursor "$next_cursor"
quazonai research evidence "$research_id" "$task_id"
```

Lists are paginated in original task order, including failed, unfinished, and historical runs within the same research. Use returned task_id values to inspect each input.request, state, and original Nautilus result. Original task results do not recalculate rankings or synthesize portfolio returns. The web results-and-assessments section offers the same pagination and original JSON links. After a batch, the default display is the last group, not the best group or a portfolio. Read the original results before requesting an assessment. Keep missing evidence unavailable; parameter search does not establish independent OOS validation or live trading permission.

Legacy `parameter-scan list/get/stop/trials/watch/wait` only reads or explicitly stops historical scans. Retained contracts do not mean old generation, scoring, or ranking execution chains remain callable.

## Official equity statistics and return basis

Inspect actual `equity_performance` source, status/reason, currency, snapshot coverage, number of return days, and frequency. It reads official Nautilus portfolio_returns and invokes official CAGR/Sharpe at both 252 and 365 periods. Without equity evidence it remains unavailable; position returns are not substituted as equity returns. Canonical default statistics may use position returns, so metric names alone do not establish daily equity returns. Official cross-day carry-forward is not an actual observation each day, and annualizing a short sample is not realized annual profit.

When declaring assessment metrics through the research agent, specify `metric_source:"EQUITY_PERFORMANCE"`, `annualization_days:252` or `365`, and the corresponding `native_key`. Legacy WF tools or historical contracts are not current automatic splitting capabilities. Declare native requests explicitly for every validation window. Equity-market and around-the-clock frequencies are calculation assumptions and must match user goals and reporting conventions.

## Overfitting checks and AI synthesis

Ask the research agent for pre-execution logic assessment, native validation of explicit candidates/windows, neighboring parameter checks, and cost sensitivity, then read actual results. Do not claim walk-forward/CPCV without corresponding requests; there is no automatic fold splitting or independent CLI optimization command. Nautilus performs backtests and statistics. Logic analysis and synthesized star ratings are model judgments, not official Nautilus scores.

Logic pre-assessment precedes the corresponding numerical run and declares candidates, windows, and optimization objectives in advance. Full-period sensitivity is not a new out-of-sample test; do not claim preregistration after seeing results. Inspect task provenance, state, windows, fees, missing evidence, and limitations. Neither COMPLETED nor stars alone establish successful validation. Insufficient evidence leaves stars unavailable. AI stars are not profit probabilities, independent audits, or trading authorization.

## Existing records

`portfolio list/get` retains historical read-only results. The CLI no longer creates factors, portfolio builds, or capacity scores. Removed `portfolio replay-ticks` / `portfolio tick-replay` routes cannot create or query replays. Use official native strategy execution in the same `research`, without reactivating the retired factor pipeline.

## Explicitly authorized implementation source review

Source review does not automatically scan local files, download URLs, or read credentials. External agents register owner-authorized source text for a specified model in the same research through the CLI:

```sh
quazonai research implementation-sources "$research_id" --file source-material.json
```

`source-material.json`:

```json
{"requestId":"source-material-1","shareWithResearchModel":true,"model":{"provider":"ACTUAL_PROVIDER","modelId":"CURRENT_RESEARCH_MODEL"},"files":[{"id":"strategy","name":"strategy.rs","revision":"OWNER_DECLARED_VERSION","content":"SOURCE_TEXT_AUTHORIZED_FOR_THIS_MODEL"}]}
```

This explicitly authorizes sharing these texts with the research's specified model. Do not include passwords, secrets, private account details, or unauthorized files. The response contains a manifest only; it does not start review, compilation, backtesting, or trading. Original material under a requestId cannot be replaced. Material binds the then-current native research request and model; configuration changes require corresponding newly registered material. Registration does not replace the internal research agent's implementation or assessment.

Next, instruct the original research to select this material through `native_strategy_logic_review`'s `sourceRequestId`. The Pi child conversation pages it with `native_assessment_read_source`, reporting actual files and UTF-16 character ranges read. Unsupplied dependencies, unread ranges, and source-to-build correspondence remain unknown. Owner-supplied version labels are not build evidence. `SCOPED_SOURCE_LOGIC_REVIEW` means authorized excerpts were read, not that the entire implementation was reviewed. Without material, assessment remains configuration-logic review. Preserve old Tasks, reports, and original requests; do not recalculate or authorize trading.

## New datasets and a per-research runner

For an additional trusted Rust runner, first read the current revision with `native-runner status`. Submit one original request with `native-runner import --file`, then confirm completion and configuredRunFactory capability with `native-runner build-status REQUEST_ID`. Import executes trusted code's capabilities; it is not data/strategy acceptance and does not select the global runner.

For new datasets, obtain the returned `revision` through `datasets registry`, then append a service-prepared DatasetConfig file with `datasets register --file`. An ID cannot overwrite any existing or reserved dataset. Retain the original requestId; for unknown outcomes, query/replay the original request rather than changing IDs. Registration does not start research, acquire data, or alter old sources.

After acceptance, a new `research start --file` may explicitly bind a completed build with `runnerBuildRequestId` and current `expectedRunnerRevision`, without changing the global default. Check environment changes or revision conflicts first rather than automatically refreshing and resubmitting. Existing research and same-requestId replay retain the original executor. Dataset/runner readiness is not live trading permission.

Research that already executed may request a post-execution source/logic review by resuming the same research after settlement, specifying the registered sourceRequestId and explicit POST_EXECUTION timing for the internal agent. Read the independent review records in research get and their original evidence links. Post-execution records do not replace prior pre-execution ratings, retroactively create a preregistered protocol, or rerun historical tasks. Source-review child agents do not automatically read historical backtest results; distinguish source conclusions from execution evidence.

Owner `marketImport:true` optionally enables the next formal build; saving configuration does not change old Builds. The existing native importer requires explicit owner high-precision. Builds preserve the same official dependency source/version/precision; do not fabricate capabilities to enable it. Have the internal agent follow its skill to prepare owner features/official lock, create a new Build, and bind new research using the original requestId, without changing the global runner. The internal agent then imports authorized training files through the existing `native_market_import` contract and reads the actual report before dataset registration. Omitted `modality` defaults to `TRADE_TICK`. `BOOK_SNAPSHOT` stores complete order-book snapshots as native Clear plus Add for every level in `OrderBookDelta`, not source incremental updates. Prices and quantities are not depth-truncated. Book import encodes the supplied instrument's price/size precision and strictly verifies each converted value equals its source; nonzero excess precision makes import incomplete and rejects success, without rounding or silently dropping markets. Definitions are input scenarios, not proof of historical applicability. TRADE_TICK import also uses instrument precision and rejects any value loss; trade rounding is not supported. Source `order_count` is not a delta field. Read `parquet-zstd` using actual manifest mappings; choose event/receive clocks explicitly, with an explicit maximum delay for receive-clock mode. External agents operate QuaZonai, read reports, and confirm registration, rather than running the native importer directly.

Without a registered catalog, have the internal agent call the existing import Task through `native_runner_market_import` using the successful Build's original requestId, read the actual report, write a descriptor in the research workspace, register it with `native_dataset_register`, then use `native_research_start_bound`. These are internal tools, not CLI commands. Externally, deliver authorized goals through research resume and read original task results. Do not fabricate dataset placeholders or change the global runner to bypass preparation.

Ask the internal agent to use formal `native_runner_build`, optionally with `optimizationLevel: 0 | 1 | 2 | 3`. It controls compilation optimization for this new build only; omission preserves existing behavior and does not modify owner configuration or old Builds. A different optimization level requires a new requestId; original-requestId replay retains its original parameters. This is not an existing-executable import parameter and does not automatically select a runner, rebind research, or start a backtest.

### Multiple registered catalogs

New research start JSON keeps `datasetId` as the primary dataset and may add `additionalDatasetIds: ["ANOTHER_REGISTERED_DATASET_ID"]`. The service resolves and retains each dataset's original catalog and identity without copying market data or broadening access to a parent directory. Same-requestId retries retain original bindings. This supports multi-catalog inputs to internal configured Nautilus factories/configured parameter batches only; old research and ordinary single-catalog calls remain unchanged. External agents submit real registered IDs and research requirements. QuaZonai internally uses brief, catalog inspection with the specific bound datasetId, and native backtest tools, without external agents authoring or executing strategies.

The native owner environment may explicitly set `backtestCompletion:true`. The internal strategy module supplies `finalize_after_node_run`, enabled through the existing build and new-research binding flow. Omitted/false retains old factories. This setting is neither proof that an old Build supports it nor a determination of early stopping or natural EOF. The registered callback runs only after native execution returns successfully with matching results; internal code must still inspect original completion state. External agents must not use on_stop or manually written files to fabricate final research material.

Lossy book-quantity transformation requires explicit authorization; strict lossless conversion remains the default. Declare `quantity_rounding: {"method":"DECIMAL_HALF_UP","instrument_ids":["EXPLICIT_NATIVE_INSTRUMENT_ID"]}` only in an authorized manifest `book_snapshots` stream. List actual authorized markets, not wildcards. It rounds positive quantities for those markets to the supplied instrument's size_precision using Decimal half-up (exact ties upward). Prices and other markets remain strict. It changes neither raw data nor static definitions and does not establish historical applicability. TRADE_TICK does not accept this policy.

The report's `book_quantity_rounding` lists processed_levels, changed_levels, changed_snapshot_groups, rounded_to_zero_levels, target_size_precisions, and exact string signed_quantity_error, absolute_quantity_error, maximum_absolute_quantity_error per native market. Counts describe processed input levels, including subsequently failed snapshots, not trade volume, successful output, or profit. Rounded-to-zero levels are not silently removed: original complete-snapshot conversion rejects that group with NONPOSITIVE_BOOK_SIZE, import becomes INCOMPLETE, and report/partial output is retained without an atomic rollback guarantee. Only after checking zero rounded-to-zero levels and the actual report may you claim all levels were retained; a policy declaration is not a result.

Data-provider plugin loading, configuration, enablement, and refresh are retired. Register an existing native catalog descriptor; registration does not download, convert, copy, or replay market data. Existing data, dataset IDs, historical bindings, and registration receipts are preserved. Old provider configuration files are no longer reloaded.

Use `datasets select --file` with `{ "datasetIds": ["registered-id"] }` to choose current references for new research. This filters the current list only: it does not merge/delete data or change existing research bindings. Original and archived references remain readable. `datasetIds: null` restores the default list. `datasets registry` returns current, available, and explicitly selected IDs. Verify that a truly integrated native catalog has been registered before selecting it; hiding old entries is not a data merge.
