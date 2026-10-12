# Durable factor research host

Official Pi Durable **1.1.0**, pi-ai **1.1.0** and Chord **1.1.0** own conversations, submissions, task execution, children, replay, cancellation, usage and compaction. QuaZonai adds original research/trial/selection/exposure facts and small native scientific adapters. There is no second task database, Agent loop, cron or imported internal scheduler.

## Host integration

`openResearch({database,extensions?,modules?,providerModules?,models?,storage?,settings?,thinkingLevel?})` returns `harness/models/registry/start/list/view/watch/stop/tasks/usage/availableModels/close`, plus `submission` and `submissionFor`. The `/api/v3` service opens this same database once and uses the host programmatically. Only one process may own its native SQLite and sidecars. Close suspends work; reopen resumes the same identities.

`start(requestId,input)` retains the original `{goal,model:{provider,modelId},data,policy}` and rejects changed input under the same ID. Startup reconciles committed Requests against public `tx.submissionByRequest` and repairs only missing submissions through `Conversation.submit`, using the original goal/ID. Native done/unanswered submissions and recorded stopped/final studies are never repeated. No raw `Tx.createSubmission` admission is used.

`view` includes native view, Study and the original native `submission` record. Public service projections should expose only submission ID/status, scientific facts and effective agent metadata—not hidden reasoning or raw private transcript. `tasks(cursor?)` paginates native stored records; inspection/usage remain upstream observations, not mirrored task status. `watch` is a disposable native watch; detachment does not abort work.

Close stops new admissions, drains admitted starts, detaches host watchers, then invokes `Harness.close`. Explicit `stop` records owner stop intent and invokes `Conversation.abort(...,{background:true})` for owned work. Neither shutdown nor a local process exit proves a remote provider/downstream cancellation.

## Native tuning

These are owner-configurable public `HarnessOptions.settings` starting values, **not** performance guarantees:

| Setting | Default / unit |
| --- | --- |
| `stream.timeoutMs` | 120000 milliseconds per supported model request |
| `stream.maxRetries` | 0 SDK retries; avoid multiplying durable attempts |
| `retry` | enabled, six retries, exponential base 2000 ms, native delay cap 60000 ms |
| `compaction` | official defaults: enabled; reserve 16384, recent 20000, background 32768 tokens |
| `progress.partialIntervalMs/outputIntervalMs` | 250 milliseconds |
| `contextRetentionMs` | 600000 milliseconds |
| tool execution | native parallel reads; scientific mutation/finalization tools make their round sequential |

Load the same live host settings/modules on restart; settings themselves are not persisted. Model and effective thinking are persisted through native `configure`/`pi.agent`. The owner default is `medium`, explicitly clamped by public pi-ai capabilities (nonreasoning models resolve to `off`). Availability reports supported/effective levels, not reasoning content.

Quota/billing/auth failures remain final; there is no model/provider fallback. A stream timeout is not a task/24-hour deadline and provider support varies. Pi Durable 1.1.0 has **no public global numeric concurrency limit**; sequential is per round, not a limit across conversations.

The research-specific native `GenerationTask.onYield` hook continues only studies lacking a final record, under the original trial budget/criteria. Conversation-scoped decisions/counters survive successor generations and crashes. Two consecutive yields without new trials stop with `no_progress_continuation_limit`; exhaustion or absent budget records a stop reason. This is not an unconditional self-loop or a per-generation budget reset.

Native `control.terminate` ends a round only when every tool returns it. Final tools should be called alone. The research `ToolTask.afterTool` hook marks subsequent successful read/error results terminal once final is recorded; scientific adapters refuse additional computation with terminal feedback. A read preceding final in a mixed round can still cause one more model request. Upstream engine semantics are unchanged.

## Models and extensions

Official `builtinModels` uses only configured host environment and trusted provider modules. It **does not** reuse Pi Desktop credentials automatically. No arbitrary credentials files are searched/read, secrets returned, or paid discovery probes run. Trusted `providerModules` default-export native pi-ai Providers; `models` can be supplied directly. Boot refresh is offline metadata-only.

Trusted `modules` default-export official `defineExtension` bundles. Research/brief/finish are in `plugins/research.ts`; `factor-wasm.ts` registers the owned factor child, `heldout-wasm.ts` selects and assesses once, and optional catalog/volume/validation-compare modules use the same native facts. `factor_finalize_group` in the same heldout bundle assesses two or more original recorded selections together; every selection must be made before any final exposure, include the caller and use the same sealed criteria. It uses one shared exposure, retains each original native child/result and returns no tuning feedback. A previously finalized window cannot be reopened to add members. Missing adapters/data remain explicit failures. Keep task names/versions compatible when changing installed extensions so old work can resume.

## Scientific isolation

The Wasm adapter uses a prepared `data.catalog` and original native request, with optional `data.sealed` for independent final evaluation. Preserve original licensing, source/PIT and availability facts; an existing directory establishes none of them. Iteration truncates the native selection at validation end, including labels. Shared half-open causal partitions use canonical decimal-string boundaries satisfying `start < train < validation <= end`.

Trials are recorded before computation; FAILED stays FAILED after recovery. Original proposals, comparison and selection persist. Session-wide exposure is keyed by actual native BAR identity/windows: renamed datasets, copied catalogs, new roots or forks cannot make exposed samples unseen. Final metrics/failure are owner-visible, never model-facing tuning feedback.

WABT compiles model WAT; an owned official Task runs the real `job evaluate-factor --summary-only`. Omitted/null fuel becomes finite `"10000000"`; explicit budgets stay explicit and results echo effective request/consumed fuel. Process timeout is **120 seconds**, memory bound **2048 MiB**, configurable through positive `QZ_FACTOR_TIMEOUT_SECONDS` / `QZ_FACTOR_MEMORY_MB`. Provider credentials are not inherited; official environment cleanup joins local processes. Use `QZ_SCRATCH_DIR` for native temporary files (tests use `PI_SCRATCH_DIR`). Missing values stay null and computation is not qualification/profitability.

## CLI and verification

Installed operations use the [CLI](../cli/README.md) and [service Skill](../../skills/quazonai/SKILL.md). The old Rust client/bridge commands are not this workbench's API. The optional `src/main.ts` is a private loopback-only host, not the public service; `research.example.json` documents native settings.

```sh
npm --prefix apps/research run typecheck
npm --prefix apps/research test
```

The short suite exercises actual official SQLite/Faux/HTTP/children, SIGKILL safe/unsafe/committed-failure and admission-gap recovery, persisted provider backoff/quota/auth, bounded yield continuation, mixed-final/read behavior and actual watcher detach. `apps/job/tests/factors.rs` launches `test/native-science.ts` with real mixed-asset native catalogs and the real kernel to verify IC, exposure/final isolation and finite fuel. Faux controls model protocol only: these checks prove neither model judgment, real-market/account quality, profitability nor 24-hour unattended acceptance.
