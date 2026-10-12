# TypeScript research workbench

<a id="task"></a>
## Task

**Task ID:** typescript-research-workbench

Owner request: deeply refactor remote `dev` with parallel team execution. Rewrite non-scientific Rust in TypeScript, shorten CI/CD, reduce the frontend to research, strategy portfolios/backtest curves, live/paper observation, settings and extensions, refactor CLI/Skill, and tune official Pi Durable for unattended operation. Tests must be short and concentrate on the idea-to-downstream core flow.

Endpoint: reviewed and verified PR merged into remote `dev`. This does not authorize production installation, paid inference or broker orders. Baseline is `66353559`; worktree `.pi/worktrees/ts-platform`, branch `refactor/ts-platform`. Original-workspace changes and other worktrees are outside this task.

<a id="intent"></a>
## Intent

Replace the expensive Rust service/worker/runtime/CLI stack and sprawling console with a compact TypeScript application around the existing official durable host and native scientific kernel. Reuse official libraries rather than rebuilding transport, agents, scheduling, numerical routines, matching or accounting.

Preserve user data, applied migrations, backups, licenses, original durable conversations, research/trial facts, PIT and held-out isolation, budgets and request identities. A computed result is not independent evidence or permission to trade. The live page is target-only downstream/account observation; QZ does not execute financial orders.

<a id="spec"></a>
## Requirements and design

- TypeScript owns HTTP/authentication, business facts, scientific process orchestration, target receipts, observation, configuration, CLI and extension loading. Official Pi Durable owns task execution, children, replay, recovery, cancellation, usage and conversations. No second task scheduler or execution database.
- Rust is limited to the existing `job` scientific kernel and its required contracts/domain validation and numerical routines, using official NautilusTrader, Clarabel, Arrow and Wasmi. Retire the Rust server, store, integrations, runtime and CLI after current callers switch. Applied SQL migrations and historical database contents remain; historical access uses a read-only TypeScript PostgreSQL adapter.
- A same-origin `/api/v3` TypeScript service serves the five-page React/official Ant Design console. Source wire DTOs live in `packages/contracts/src/index.ts`; scientific wire contracts remain at their native Rust source. Old generated snapshots are not manually rewritten.
- Frontend: warm ivory/terracotta visual system, generous spacing, modern typography, usable mobile navigation. Only names, fields, status, metrics, tables and actual curves; no explanatory paragraphs, raw JSON editors, JSON viewers or fake sample data in production. Authentication remains usable.
- Research submissions preserve original request IDs and inputs. Configured datasets supply original native inputs without browser JSON editing. Portfolio/backtest projections must come from real native shared-account equity, not combined alpha curves or assumed returns. Unsupported adapters/absent evidence fail explicitly.
- Target delivery and live/paper observation retain original external identities, expiry and acknowledgements. Missing balances/metrics stay null. Scientific execution cannot silently become live trading or a qualification.
- Harness tuning uses public pinned upstream APIs for concurrency, timeouts, model/tool retries and compaction where available. Startup resumes the same durable storage; graceful shutdown suspends, rather than inventing cancellation. Unsafe external operations do not repeat after uncertain execution.
- Routine CI runs concise TypeScript/harness/core-flow and browser checks. Scientific change checks and image production reuse dependency/build caches and avoid duplicate cold compiles. Heavy retired route/console suites are removed with their deleted behavior, not skipped to conceal a regression.

<a id="plan"></a>
## Implementation plan

1. Read current remote `dev`, active architecture/review/operations, native scientific CLI and installed upstream capabilities. Three independent read-only explorations cover migration boundaries, CI costs and durable/CLI reuse.
2. Fix shared DTOs and file ownership, then parallel implementation:
   - Service/scientific flow: `apps/service` and native TypeScript adapters; authenticated API, persistent facts, original research projections, portfolio/backtest and target/account observation.
   - Harness/CLI/Skill: `apps/research`, TypeScript `apps/cli`, `skills/quazonai`; native durable tuning, unattended recovery and concise commands.
   - UI: `apps/web`; replace retired pages and tests with the five-page workbench and a few interaction/browser checks.
   - Delivery: `.github`, `deploy`, image/runtime scripts; Node packaging, scientific cache reuse and short CI.
   - Integrator: shared contracts, Cargo membership/pruning, documentation, integration checks and final delivery. Shared paths are not edited concurrently.
3. Verify the actual official durable host and native science with a short idea → recorded trials → independent result → shared-account backtest → portfolio → downstream receipt/observation flow, plus restart/replay/cancel and empty/error UI states. Controlled model/downstream protocols establish deterministic behavior only, not paid-model quality, source qualification or real account performance.
4. Run affected types/builds and concise tests, inspect real browser renders, perform one independent review, fix concrete findings and rerun affected checks. Verify the exact remote Head CI/review and merge into `dev`.

<a id="verification"></a>
## Verification

Environment: Node 26.7.0 and actual Node 22.19.0, Rust 1.98.1, Docker 29.7.2, authenticated GitHub CLI. Task-owned temporary services and logs are under session scratch. Shared scientific compiler caches are retained; no production/account/provider operation is authorized.

<a id="delivery"></a>
## Delivery

Not yet delivered. Production deployment and a literal 24-hour observation are not performed by a code refactor; report the actual unattended recovery tests and observation window.

### Inspected reuse and baseline

- Remote `dev` includes PR #191; the original local `dev` was 241 commits behind and remains untouched. This task starts from remote `66353559`, not the old local tree.
- Hosted baseline at that revision: Rust CI 35m11s, PG 25m35s, dev release 3h41m53s. Scientific/application/operator image compilation totals 87m54s; duplicate service compilations are the primary optimization target.
- Exact official Durable/pi-ai/Chord 1.1.0 public APIs were inspected. Four offline Faux/Harness probes verified sole versus mixed tool-round termination, transient retry and quota-no-retry. They do not establish paid-model quality or a 24-hour run. No public global numeric concurrency/cron API exists in that pinned version.
- Native job stdin/stdout, shared-account simulation/strategy composition, Arrow equity, original auth verifiers, SQL ledgers and installer callers were inspected before implementation. Arbitrary factor scores require an explicit strategy policy, not invented expected returns.
- A first targeted build-output cleanup aborted because the old local workspace lacks `quazonai-cli`; it removed nothing. The corrected Cargo cleanup removed only retired server/store/runtime/integrations outputs (63.7 GiB logical), leaving approximately 19 GiB available. No source, scientific cache, user data, backup or license was removed by that cleanup.
Parallel implementation and the bounded service/native follow-ups are complete. Final exact-Head independent review, hosted CI/native-container acceptance and remote merge remain pending.

### Local implementation evidence (not final-Head acceptance)

- Parent reran research and CLI types/tests after adding the optional original native submission DTO: 18/18 Harness checks in 2.64s and 3/3 CLI checks in 0.66s, no skips. SIGKILL admission repair, safe replay, unsafe uncertainty and committed failure remain exercised.
- Parent reran Web types, one unit check, production build and three Chromium interaction checks after adding explicit decimal strategy weight fields: passed; build 9.24s, browser 30.3s. Both desktop/mobile behavior, missing values, original request retry and no raw JSON remain checked. Vite reports large official vendor/chart chunks; it is a warning, not a build failure.
- Native cleanup owner: three scientific crates; dependency graph 836 → 739 packages without new upstream versions/sources; 88 selected job checks and 199 contract/domain checks passed. All retained all-feature/all-target code compiles. This does not claim full live/Sandbox/OCI acceptance.
- Parent compared all 102 PostgreSQL migrations and four relocated Runtime SQLite migrations against baseline bytes. Licenses and historical generated snapshots are unchanged. Current changed documentation file links passed a local path check; no completed repository-wide link/license audit is claimed.
- A temporary actual Node 22.19 service serves production Web and `/api/v3`; initial setup/session works. Browser filling required actual React input events. Rebuilding Web during the session invalidated an old lazy asset URL; refresh reads the new build. Full real-provider/data/account browser acceptance remains unperformed.
- Service follow-up repaired the outer application admission window under original IDs/inputs without the current dataset registry, and projected authoritative original native submission status. Five focused service checks passed, including exactly one resumed provider call across two restarts.
- Legacy Rust installation automatic cutover is unsupported and explicitly rejected before changing original state. New installation/backup acceptance is not proof of a legacy migration. Paid-model quality, real account operation and literal 24-hour observation remain unperformed.
- Native follow-up passed two focused `blend-targets` CLI cases and the existing legacy blend case. It reuses native BigDecimal target/cash blending, strict original alignment and minimum member expiry; no curve blending or legacy identity fabrication. Cargo/local crate version is now 3.0.0-dev.1 without upstream dependency changes.
- Parent rebuilt the matching four core executables once with the final scientific feature set in 17.57s; output is task-owned scratch. Retained unused native hooks still emit warnings.
- Deployment owner passed Node 22.19 TS/deployment helpers, portable Linux CLI install, service-only container restart/update/stopped SQLite+PG restore and isolated Wasm compilation. Full native container smoke did not pass locally: the supplied debug artifacts require newer glibc than pinned bookworm. CI builds on Ubuntu 22.04; parent enabled retention of exact tested PR outputs to complete matching image acceptance without another cold compilation. No source-matched container pass is claimed yet.
- Parent fixed browser artifact output paths and reran actionlint (without optional shellcheck/pyflakes integrations) and whole-tree whitespace checks: passed. Remote dev still equals baseline 66353559. No task commit/push/PR/review/merge exists yet.
- Parent replaced the test-only joint final evaluator with the production `factor_finalize_group` tool in the existing heldout bundle. It atomically records all original selections before one shared exposure, validates original iteration criteria, retains original owned native children/results and returns no final tuning feedback. Legacy single finalization remains unchanged. No exposure ledger reset or extra submission is used by core acceptance.
- The first production-path attempt exposed a transaction-proxy cloning defect; JSON serialization of original JSON values corrected it and temporary diagnostics were removed. The unchanged single core scenario then passed on actual Node 22.19 with matching job 3: two original API research submissions → production independent joint evaluation → native exact `blend-targets` → one fee-affected Nautilus shared account → original target receipt/null account observation, 1/1 in 1.98s, no skips. Both positive/negative selected Wasm sources contribute; four controlled provider calls, one downstream POST, target 0.0625/cash 0.9375 and original minimum expiry.
- Parent reran `make check-ts check-web` after the production adapter: service 5, Harness 18, CLI 3, Web unit 1, all types and production Web build passed; final Web build 9.76s. Deployment helpers passed 10 checks plus two packaging/two release cases; one archive-dependent installer case skipped in this combined run had been executed separately by the deployment owner. Hosted matching container acceptance is still pending, not relabeled from the service-only diagnostic.
