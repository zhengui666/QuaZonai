# QuaZonai TypeScript application

Fastify owns `/api/v3`, owner/device authentication, configuration, portfolio facts and target/account observation. `openResearch()` is called **in process**; the service does not proxy the retired Rust server. Official Pi Durable owns SQLite, tasks, replay, scheduling, shutdown and recovery. New application facts are the session-scoped `qz.application` document in the **original configured research database**.

## Run

Install `apps/research` and this package with their npm locks. Node >=22.19 supports the entry point; validation also runs on Node 26.

```sh
cd apps/service
npm ci
QZ_CONFIG=/absolute/path/service.json QZ_JOB_BINARY=/absolute/path/job \
  node --experimental-strip-types src/main.ts
```

`service.example.json` has no datasets, connections, sample balances or credentials. Relative paths are resolved against the config file. Defaults: `127.0.0.1:8080`, persistent `data/`, `data/research.sqlite`; no QZ task/native/model-discovery deadline. Set `research.database` to the existing SQLite file, **not** a fresh replacement. `research.modules`, `research.providerModules` and native `research.settings` are passed to the official host, except retired stream deadlines. Provider credentials stay in its environment or private `${research.database}.auth/credentials.sqlite` (0700 directory, 0600 file), never research documents. Back up that directory alongside SQLite/sidecars. `passwordHash` optionally supplies an existing Argon2 verifier; otherwise set the owner password through `/session/setup`. Configure `secureCookies` and `publicOrigin` for the deployment origin. `staticDir` optionally serves the built same-origin console.

`createService({config, models?, storage?, extensions?})` (also exported as `buildService`) returns `{app, research, work, close}`; `app.inject()` supports short integration tests. `close()` awaits the host's suspension; it does not invent cancellation or delete facts.

## Research and strategy policy

A dataset is a `DatasetView` plus `data` (original catalog path/native factor request, optional `sealed`) and optional `policy`/`strategy`. Simplified submissions capture that registry entry before any trial. New full submissions require registered data; exact replays retain original inputs even if the old registry entry disappeared. On startup the service repairs outer `qz.application` admissions absent from native `Requests` using their original request IDs, models, budgets and captured inputs, before HTTP readiness. The host alone repairs native admissions; existing native stopped/final/submitted studies are not resubmitted. Changed replay input returns `409 request_id_conflict`; GET never starts research. `ResearchDetail.submission` projects the authoritative original native `{id,status}` or null—only native `done`/`unanswered` settles a wait, not final evidence or empty tasks.

The bounded adapter supports **distinct selected studies on one configured dataset/policy, one instrument, one currency and ONE native shared account**:

- `strategy.kind`: `factor-threshold`; a named server-side policy with `instrumentId`, finite `threshold`, exact `investedWeight`, exact `targetTtlNs`, and the original native `simulationSettings` object.
- Every study must retain that identical pre-admitted policy, a computed selected trial and an independently computed final report with held-out observations. Explicit decimal member weights are required for multiple studies: positive fractions summing exactly to 1, associated with each original research ID. A single study defaults to weight `1`. Original bodies, evidence and weights are captured together for exact replay. Unavailable IC stays unavailable; computation is not qualification.
- For members sharing a sealed window, the packaged heldout extension provides `factor_finalize_group`: select every original recorded trial before any final assessment and assess the group once. A second study can finalize itself together with earlier selections; individual finalization cannot later be reopened to add members. The service and core check use this production tool, not a test-only final evaluator.
- `job forecast` runs EACH selected original Wasm against the captured catalog/native request. The explicit threshold selects its configured weight or cash; scores never become expected returns or direct weights. Multiple aligned decision rows then go to actual `job blend-targets`; native BigDecimal owns blending and the minimum original expiry. Unequal selections, decision lengths/clocks, assets, currencies or admitted policies fail without resampling/fill.
- `job simulate` runs ONCE on those combined targets with one original capital/account. Curves come from canonical shared-account snapshots, never blended alpha equities. Persistent facts retain original source research/trial/model identities, forecasts, member target rows, weights, native blend output and the combined simulation request/report.
- Portfolio/catalog capture and task admission precede execution. `qz.portfolio` and `qz.observation` are official `defineTask` definitions installed before `Harness.open`; there is no detached startup queue.

Supply `simulationSettings` exactly as the kernel requires (`adapter_kind`, official class/version, parameters, fees, leverage and account kind). The classic CurrencyPair adapter supports MARGIN, not single-currency CASH. Unsupported native modes fail without fallback. Feature-policy templates and `COMPOSE_STRATEGY_TARGETS` managed file artifacts are not adapted; no legacy Run/Attempt/artifact identities are fabricated. New multi-study work requires the public `job blend-targets` command. Existing v1 single-evidence task inputs retain their original kernel path and run without it; durable task kinds/versions are unchanged.

Settings GET/PATCH exposes only `{defaultModel,thinkingLevel}`. PATCH rejects all other fields, validates configured model/capability and affects only new research; original admitted inputs replay unchanged. Historical name/trial/timeout settings remain stored but are neither projected nor enforced. `/workspace` supplies application identity and observation connections. `/models` lists actual providers, supported auth methods, active login and available models; `/models/login` starts an original request ID, `/models/login/:id` observes/cancels it, and `/models/login/:id/respond` answers its current prompt ID. `/models/providers/:provider` DELETE logs out through the official API; environment-provided auth may remain configured. All routes require owner authentication. API Key/OAuth values are transient browser inputs and private native credentials, not public DTOs; only safe auth links/events and static failure codes are exposed.

At the Node entry point, resolved config `jobBinary` sets `QZ_JOB_BINARY` before the sole host loads factor extensions, selecting the same kernel for research and portfolios. Programmatic `createService` callers with factor extensions supply that startup environment themselves; native children still receive no provider credentials. Retired factor timeout/memory environment variables are ignored. No native process deadline, memory ulimit or adapter output cap is imposed; explicit scientific budgets and owner cancellation remain. Downstream HTTP headers/body/connect have no QZ deadline. Extension changes report `pending_restart` without hot-swapping admitted tasks. Back up original SQLite/sidecars, its `.auth` directory, `dataDir/catalogs` and `dataDir/extensions.json` together.

## Operator dataset configuration

The [Polymarket v2 source preset](../../runtimes/data/polymarket-v2.md) exports one offline `DatasetConfig` fragment through `export-dataset polymarket-v2-hf` (TypeScript entrypoint: `src/dataset-export.ts`). It consumes actually prepared catalogs and original owner-selected factor requests, optionally a compatible separately prepared Sealed binding and explicit policy/strategy. Native decoding/measurement remains in Rust. Export defaults to `ready=false`; explicit `--ready` is technical owner enablement, not permission, PIT, coverage or scientific qualification.

Review and merge the entry into the existing private `service.json.datasets` array without replacing other configuration or credentials. Export does not call HTTP, edit service configuration, restart the service or submit research. `/api/v3/datasets` lists configured public dataset views and does not expose their `data`/policy/strategy. Prepared paths must exist verbatim wherever the native job runs. The service captures original input **JSON**, not physical catalog bytes: keep owner-managed catalogs unchanged/read-only for replay and independent evaluation. Optional strategy export additionally requires exact instrument/static policy compatibility and primary catalog coverage of the full final request, because the existing portfolio consumer captures the primary binding. Prefix-only primary plus a separate full Sealed catalog is a research-only export; it is not silently enabled for portfolio replay. Keep raw source evidence and Sealed values outside researcher-readable mounts.

## Target-only downstream protocol

A configured connection has `id`, `name`, `mode` (`paper`/`live`), `url`, original `clientId`/`accountId`, and optional `tokenEnv` (environment variable **name**, never a credential in config). The supported downstream adapter exposes:

- `POST <url>/targets`: `{id, clientId, accountId, expiresAt, target}`. `target` is the original immutable native target point, including string nanosecond clocks/expiry, exact target/cash weights and currency. No broker orders are sent.
- `GET <url>/targets/:id`: the original receipt, also used to reconcile an uncertain POST.
- Both receipt responses: `{id, clientId, accountId, expiresAt, status, externalId?, acknowledgedAt?}`; status is `PENDING`, `ACKNOWLEDGED`, `REJECTED`, `EXPIRED` or `REVOKED`. Receipt identity and original expiry must match.
- `GET <url>/account`: `{clientId, accountId, sequence, nativeClockNs, currency, equity, cash, positions}`. Sequence/clock and all money/quantity values are **strings**. Equity/cash/position value/PnL may be null. Positions use the shared `PositionView` fields. The original snapshot is retained; conflicting or out-of-order snapshots fail.

The durable checkpoint moves to reconciliation **before** POST. A suspended/crashed/unknown offer is never POSTed again; only receipt GET can reconcile it. A different request cannot bypass an unsettled delivery. Expiry is never extended. Stopping observation stops local work, **not** an external offer or account; its original receipt remains. Explicit refresh of a stopped observation can reconcile its receipt without restarting account polling. There is no production synthetic balance or automatic broker adapter. A legacy downstream that implements another offer/claim/intake protocol needs its own explicit adapter; unsupported responses remain errors/uncertainty.

Optional `archiveDatabase` uses official `pg`, fixed SELECT projections of applied `app.projects`/`app.runs`, and `BEGIN READ ONLY`; no migrations or new scientific SQL facts are written. Cost stays null when an exact monetary usage fact is unavailable.

## Short checks

```sh
npm run typecheck
npm test
# Existing executables only: no full Rust backend compile.
/absolute/path/browser_catalog_fixture "$PI_SCRATCH_DIR/new-browser-catalog" /absolute/path/catalog-prepare
QZ_JOB_BINARY=/absolute/path/job \
QZ_BROWSER_FIXTURE="$PI_SCRATCH_DIR/new-browser-catalog/prepared.json" npm run test:native
```

Default protocol tests use real official SQLite/Harness, private credential persistence, provider prompt/retry/cancel/shutdown, frozen reasoning, real child cancellation and test-only Faux protocols, including outer-only admission recovery with a removed dataset registry and original receipt/provider-call checks. The ONE separate native check uses job 3.0.0-dev.1, original `browser_catalog_fixture` definitions and a bounded 599-row/three-batch native acquisition. Two original controlled studies select positive/negative Wasm; a test-only joint final evaluator freezes both selections BEFORE one shared sealed exposure, without resetting the exposure ledger or returning final tuning feedback. Actual `forecast` → `blend-targets` → one `simulate` supplies exact target/cash weights, filled positions, original fees/shared-account curve and target receipt/account nulls. The test does not certify historical PIT, quality, profit, real account performance or deployment.
