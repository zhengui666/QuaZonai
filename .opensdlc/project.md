# QuaZonai project context

<a id="purpose"></a>
## Purpose and architecture

Single-user unattended quantitative research: idea → recorded scientific trials and independent evaluation → optional shared-account strategy/portfolio backtest → target-only delivery and account observation. TypeScript owns HTTP, configuration, orchestration, CLI and Web. Official Pi Durable owns execution and recovery; NautilusTrader, Clarabel, Arrow and the retained Rust kernel own necessary computation. PostgreSQL history and applied migrations remain. See [architecture](architecture.md).

<a id="commands"></a>
## Development and checks

Develop from current remote `dev` in an isolated branch/worktree. Preserve unrelated changes. Node 22.19+ supplies native type stripping and SQLite; use the pinned [Rust toolchain](../rust-toolchain.toml) and lockfiles.

```sh
npm ci --prefix apps/service --no-audit --no-fund
npm ci --prefix apps/research --ignore-scripts --no-audit --no-fund
npm ci --prefix apps/cli --ignore-scripts --no-audit --no-fund
npm ci --prefix apps/web --ignore-scripts --no-audit --no-fund
```

| Change | Command at repository root | Actual scope |
| --- | --- | --- |
| Service, Harness, CLI and Skill | `make check-ts` | Types and concise HTTP/core, durable recovery and client checks; official upstream packages |
| PM decision contracts | `node --experimental-strip-types --test apps/service/test/pm-contract-acceptance.test.ts` | Exact metric identity, empirical comparison, unavailable evidence, funds, nonmonotonic capacity and operating candidate semantics; synthetic counterexamples, not performance evidence |
| PM source revisions and resource reads | `node --experimental-strip-types --test apps/service/test/pm-source.test.ts apps/service/test/pm-routes.test.ts` | Actual file digests, append/correction history and immutable paths; real HTTP authentication, typed resource identity, durable decisions and restart |
| PM original account source and outcomes | `node --experimental-strip-types --test apps/service/test/pm-account.test.ts apps/service/test/pm-outcomes.test.ts` | Original native identity, balances, availability, zero-order proof, derived NAV, retirement, exact cash-flow/execution events and retry conflict semantics; complete cash-flow coverage before account-only adjusted changes |
| PM native methods | `cargo test --locked -p job --features hyperliquid-offline --test ensemble_fit --test capital_curve --test observed_spot -- --test-threads=1` | Training-only calibration/combination, original native replay at tested capital points and observed cash-spot valuation/HOLD/rebalance |
| PM observed account adapter | `QZ_PM_OBSERVED_FIXTURE_DIR=/path/to/export QZ_JOB_BINARY=/path/to/matching/job node --experimental-strip-types --test apps/service/test/pm-account-native.test.ts` | Three actual CLI consumers using original exported catalog, balances and study objects; missing fixture/binary skips and is not a passed native check |
| PM observed account service | `QZ_PM_OBSERVED_FIXTURE_DIR=/path/to/export QZ_JOB_BINARY=/path/to/matching/job node --experimental-strip-types --test apps/service/test/pm-account-http.test.ts` | Multi-day native FIT, matched original-inventory HOLD/rebalance, owner-only evidence and Durable restart through actual HTTP; requires the `science/source.json` fixture and its original archived receipt |
| PM execution source bounds | `node --experimental-strip-types --test apps/service/test/pm-forward.test.ts` | Exact account/source identity, missing cost components, complete inventory, entry/exit limits and original deadlines; source-bound checks alone do not authorize a target |
| PM current model oracle | `QZ_PM_FORWARD_FIXTURE_DIR=/path/to/export QZ_JOB_BINARY=/path/to/matching/job node --experimental-strip-types --test apps/service/test/pm-forward.native.test.ts` | Actual forward CLI with independent scalar forecast/covariance/cost checks, current marks, original snapshot and TTL, rounded trade accounting, and blocked targets; controlled software evidence, not live qualification |
| Web | `make check-web` | Types, small unit check, production build |
| Original native capital-exit owner | `cargo test --locked -p job --features native-sandbox-test --test native_capital_exit -- --test-threads=1` | Existing native owner phases, old-target fencing, reservation, withdrawability and reconciliation; not a real-money operation |
| Workbench browser behavior | `npm --prefix apps/web run test:e2e` | Build Web first; install Web, Service and Research dependencies plus Playwright Chromium. Includes production assets on disposable real HTTP/Durable SQLite, controlled failure/race cases, themes, mobile and axe checks |
| Public source helpers | `make check-source` | Shared HF transport/cache, explicit source selection, offline handoff and failure/recovery fixtures |
| Installation/package helpers | `make check-deploy` | Source helper tests, shell syntax and disposable package/manager checks; not an installed service |
| Routine TypeScript work | `make check` | TS, Web and deployment helpers |
| Scientific contracts/kernel | `make check-science` | Retained native rules, factors, simulation, allocation, isolation and account adapters |
| Remote scientific protocol | `node --experimental-strip-types --test apps/nautilus/test/*.test.ts` | Actual HTTP and controlled child processes, receipts, restart and cancellation; not scientific evidence |
| Connected native core flow | `make check-core QZ_JOB_BINARY=/absolute/path/to/job` | Matching `catalog-prepare` and `browser_catalog_fixture` beside job; actual native factor/heldout, selected strategy, shared-account curve and controlled downstream protocol |
| Full retained science, manual/formal release | `make check-native-full` | Clippy and all retained native tests; not the routine control-plane suite |
| Current Markdown | `make check-links` | Offline links using `.lychee.toml`; historical task records retain their original context |

Build the matching scientific executables once with `bash deploy/docker/native-build.sh debug /absolute/artifact/directory` (or `release`). `CARGO_TARGET_DIR` can reuse the existing cache; `PROFILE=release` selects matching tests. Do not delete unrelated caches or user data. A missing prerequisite, ignored test, old binary or old-Head CI is not a pass.

[CI](../.github/workflows/ci.yml) runs TypeScript, research-workspace Web and scoped native/core checks. Scientific binaries are built once and passed to the connected test. Push runs retain the exact tested release output for image production. [Rust regression](../.github/workflows/rust-regression.yml) is the manual/formal-release full retained-science check, not the retired Server/Store suite.

### Local service

Copy [service.example.json](../apps/service/service.example.json) to a private configuration, then set explicit original database, remote `nautilus.backtest` endpoint, datasets, provider modules and extensions as needed:

```sh
QZ_CONFIG=/absolute/private/service.json npm --prefix apps/service start
npm --prefix apps/web run dev
node --experimental-strip-types apps/cli/src/main.ts --help
```

Vite proxies the real API at port 8080. The application opens the official durable host programmatically; do not also start the standalone research host against the same SQLite database. The [research guide](../apps/research/README.md) describes public Harness tuning, provider registration, bounded continuation and recovery.

### Quantitative PM workflow

The PM workspace adds owner goals, registered data, funding and one scientific trial budget to the original research pipeline. It records planned selection candidates separately from training operations, learns factor combinations from matured training evidence, compares actual native Validation account statistics, and evaluates capital configurations against a matched cash baseline. See the [implementation task](tasks/pm-adaptive-platform/task.md), [reviewed contracts](tasks/quant-pipeline-integrity/design/contracts.md) and [architecture boundary](architecture.md#pm-platform).

```sh
quazonai pm capabilities
quazonai pm start --file MANDATE.json
quazonai pm list
quazonai pm get MANDATE_ID
quazonai pm decision MANDATE_ID
quazonai pm history MANDATE_ID
quazonai pm recompute MANDATE_ID --file ORIGINAL_COMMAND.json
```

`MANDATE.json` uses `PmMandateStart`: original `requestId`, `goal`, configured `datasetId`, `funding`, `model`, `maxTrials`, and optional explicit `thinkingLevel`, `serviceTier`, `businessPreference` / `operatingAuthority`. Reasoning effort and speed inherit matching default-model settings when omitted, are validated against model capabilities, and stay frozen for the mandate’s research campaigns and retries. Recompute and retirement use the original `PmCommand` body with `requestId` and `expectedRevision`; an uncertain response is reconciled with the original identity. `quazonai schema show PmMandateStart` exposes the source wire definition. Service admission binds configured dataset snapshots and their provenance; users do not upload replacement performance metrics through these commands.

Pure methods are in [decision.ts](../packages/contracts/src/decision.ts), DTOs and cross-field guards in [PM contracts](../packages/contracts/src/pm.ts), durable business transitions in [service PM](../apps/service/src/pm-platform.ts), research ledgers in [research PM](../apps/research/src/pm-campaign.ts), and required numerical work in the native kernel. `PmMandateView.comparisons` contains operating configuration comparisons, separate from research candidate comparisons. Full paths, evidence and unsupported behavior are part of the public result.

The owner-authenticated [PM routes](../apps/service/src/pm-routes.ts) expose `/api/v3/capabilities`, `/mandates/:id/decision`, `/mandates/:id/decisions`, `/cycles/:id`, `/capacity-estimates/:id`, `/campaigns/:id` (including `/trials` and `/decision`), `/strategies/:id/versions` and `/strategies/:id/assessments`. All paths use the `/api/v3` prefix. `/pm-evidence/:id` returns the original evidence reference and stored payload to the owner; independent observations are never passed back as selection feedback. Missing resources return 404, and a resource of another entity type cannot stand in for the requested identity.

An ACCOUNT mandate can consume original, continuous native multi-currency CASH balances plus explicit zero open orders when the declared native spot policy is supported. [Observed-account adapters](../apps/service/src/pm-account-native.ts) value inventory using marks at the original economic time, retain derived NAV separately from reported equity, and compare HOLD with recomputed allocations from the same original balances. Original snapshot receipt and later capital reservations fence availability. Locked balances, debt, margin, unknown orders and incomplete source histories remain explicit gaps. Each paired replay leg consumes the shared scientific budget. The short native fixture establishes inventory/order/cost behavior; complete daily evidence requires the separate multi-day source, not extrapolation from a short path.

Account capability envelopes and their first receiver clocks are retained per transport binding, while actual reservations, control revisions and exit obligations remain shared by physical account. The existing observation refresh reads `/account`, `/capabilities` and `/capital-exits`, preserving original receipt reconciliation and supporting account-only sources without inventing a portfolio. A failed or unreadable alias becomes unknown for that binding; it cannot erase another binding's actual source state or borrow its capital permission. Current refresh facts do not replace the research campaign's frozen starting-account evidence. Verify these source/HTTP boundaries with `cd apps/service && node --experimental-strip-types --test test/pm-account-refresh.test.ts test/pm-account.test.ts test/pm-target-basis.test.ts`; these are transport and persistence checks, not market evidence.

The ACCOUNT service acceptance uses explicitly archived, already-received software source facts. Its separate current-receipt check rejects using a snapshot first received today in an earlier historical decision; it does not change the production clock or establish broker/PIT provenance. It checks the explicit no-leverage owner statement through the frozen policy, actual native CASH borrowing flag, nonnegative published balances and original optimizer constraints. Original native statistics independently verify the displayed objectives, and every candidate binds the same original snapshot. A selected HOLD keeps the original inventory and has no new strategy allocation: its derived NAV remains in `accountBasis.valuation` and the comparison scope, while `capital` stays null. A genuine tie retains complete native evidence and derived NAV without inventing a winner or independent qualification. A selected rebalance must retain the completed original independent task and owner evidence; an exception is not a passed evaluation.

The [current forward consumer](../apps/service/src/pm-forward.ts) applies an independently evaluated frozen method to the original account inventory and current cutoff-available marks. It records modeled return per original label horizon, variance, linear cost and method objective separately from historical daily metrics. Continuous optimizer weights and post-rounding trade estimates remain separate; the operating comparison uses the rounded model. A fixed operational inference is not a new search trial, and retry never moves the original target deadline. [Execution source evidence](../packages/contracts/src/pm-forward.ts) binds the original account/source/control revision, environment, all cost components and actual entry/exit bounds. Its readiness scope is explicitly limited to source execution bounds; native rules, PIT provenance, independent qualification and the downstream atomic allocation fence remain separate requirements.

A research winner is not investment authorization. Historical configurations retain the same original NAV; actual interval/currency/cost/state must match their cash or HOLD baseline. The registered frozen paired independent assessment can promote research to SHADOW only, without statistical-significance or live-authorization claims. Unknown evidence is not zero; empirical frontiers are not statistical significance; untested capital gaps are not feasible regions. Fixed method risk aversion is not a learned owner preference. PM target admission and transport consume an already qualified current-account decision and reconcile the original downstream receipt; ACK is not a fill. Historical selection or independent economic evidence alone does not create a deliverable current target. Applicable checks establish their stated software/native behavior, not profitability, market/PIT quality, a live deployment or unattended elapsed time.

### Source contracts

Current API DTOs live in [packages/contracts](../packages/contracts/src/index.ts), imported directly by Service, Web and CLI. Scientific wire rules remain in [Rust contracts](../crates/contracts/src); edit source before running any source-derived export. `contracts/generated/` and `apps/web/src/generated/` are retained historical snapshots and are not the new HTTP API. Never hand-edit them or invoke removed server/runtime generators. `quazonai schema show NAME` exposes the CLI's supported schema views without opening service storage.

Deterministic tests use official Faux Provider and controlled downstream protocols. Browser HTTP tests use an explicitly TEST_ONLY scientific protocol subprocess; they establish HTTP/storage/interaction recovery, not scientific evidence. `check-core` and retained scientific CI execute the matching real kernel. Synthetic catalogs do not establish market/PIT quality, paid-model quality, profitability, real-account results or a literal 24-hour run. Record those separately if authorized.

<a id="runtime-image-build"></a>
## Image production

[Container action](../.github/actions/container/action.yml) keeps the Node application free of native engines and reuses exact tested native outputs for the separately deployed Nautilus worker image; normal npm/buildx/Rust dependency caches avoid duplicate compilation. [Version publishing](../.github/workflows/release-version.yml) and [dev publishing](../.github/workflows/dev-image.yml) preserve the release strategy and run disposable installation/restart/backup acceptance. Source tools remain at [runtimes/data](../runtimes/data); native image assembly at [runtimes/native](../runtimes/native).

CLI archives carry ordinary JS, official dependencies and the official platform Node runtime, not a rewritten runtime or SEA implementation. Installation does not compile and requires no host Node/Rust/Python/Worker. Legacy Rust installation cutover is explicitly unsupported until original authentication, queues and state can be reconciled; new bundles never initialize over that state. See [deployment](../deploy/docker/README.md).

<a id="conventions"></a>
## Conventions and owners

Follow [AGENTS](../AGENTS.md), source ownership and official public APIs. Preserve historical data, applied migrations, backups, original request/trial identities, scientific budgets and license notices. QZ does not own a second scheduler, numerical solver, matching or accounting engine. Tests stay short and concentrate on core behavior and recovery.

[@zhengui666](https://github.com/zhengui666) owns the goal and service. Final applicable CI and independent review must cover the delivered Head before merging into remote `dev`; review and merge are not production deployment or account authorization.

<a id="sources"></a>
## Canonical sources

[Architecture](architecture.md) · [TS contracts](../packages/contracts/src/index.ts) · [Service](../apps/service) · [Rust science](../apps/job) · [Migrations](../migrations) · [operations](operations.md) · [README](../README.md) · [deployment](../deploy/docker/README.md) · [review](review.md).

<a id="native"></a>
## Native entrypoints

[AGENTS.md](../AGENTS.md) is the coding context. [Service Skill](../skills/quazonai/SKILL.md) operates an installed service, not development or the retired Mission engine. Workflows remain at [.github/workflows](../.github/workflows), PR context at [.github/PULL_REQUEST_TEMPLATE.md](../.github/PULL_REQUEST_TEMPLATE.md), task evidence under `tasks/<task-id>/task.md`, and independent Agent evaluations under [evals](evals/suite.md). New task IDs/prose default to English when `.opensdlc/config.json` is absent.
