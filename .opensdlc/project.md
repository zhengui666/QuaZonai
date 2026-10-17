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
| Web | `make check-web` | Types, small unit check, production build |
| Workbench browser behavior | `npm --prefix apps/web run test:e2e` | Build Web first; install Web, Service and Research dependencies plus Playwright Chromium. Includes production assets on disposable real HTTP/Durable SQLite, controlled failure/race cases, themes, mobile and axe checks |
| Public source helpers | `make check-source` | Shared HF transport/cache, explicit source selection, offline handoff and failure/recovery fixtures |
| Installation/package helpers | `make check-deploy` | Source helper tests, shell syntax and disposable package/manager checks; not an installed service |
| Routine TypeScript work | `make check` | TS, Web and deployment helpers |
| Scientific contracts/kernel | `make check-science` | Retained native rules, factors, simulation, allocation, isolation and account adapters |
| Connected native core flow | `make check-core QZ_JOB_BINARY=/absolute/path/to/job` | Matching `catalog-prepare` and `browser_catalog_fixture` beside job; actual native factor/heldout, selected strategy, shared-account curve and controlled downstream protocol |
| Full retained science, manual/formal release | `make check-native-full` | Clippy and all retained native tests; not the routine control-plane suite |
| Current Markdown | `make check-links` | Offline links using `.lychee.toml`; historical task records retain their original context |

Build the matching scientific executables once with `bash deploy/docker/native-build.sh debug /absolute/artifact/directory` (or `release`). `CARGO_TARGET_DIR` can reuse the existing cache; `PROFILE=release` selects matching tests. Do not delete unrelated caches or user data. A missing prerequisite, ignored test, old binary or old-Head CI is not a pass.

[CI](../.github/workflows/ci.yml) runs TypeScript, six-workspace Web and scoped native/core checks. Scientific binaries are built once and passed to the connected test. Push runs retain the exact tested release output for image production. [Rust regression](../.github/workflows/rust-regression.yml) is the manual/formal-release full retained-science check, not the retired Server/Store suite.

### Local service

Copy [service.example.json](../apps/service/service.example.json) to a private configuration, then set explicit original database, native job, datasets, provider modules and extensions as needed:

```sh
QZ_CONFIG=/absolute/private/service.json npm --prefix apps/service start
npm --prefix apps/web run dev
node --experimental-strip-types apps/cli/src/main.ts --help
```

Vite proxies the real API at port 8080. The application opens the official durable host programmatically; do not also start the standalone research host against the same SQLite database. The [research guide](../apps/research/README.md) describes public Harness tuning, provider registration, bounded continuation and recovery.

### Source contracts

Current API DTOs live in [packages/contracts](../packages/contracts/src/index.ts), imported directly by Service, Web and CLI. Scientific wire rules remain in [Rust contracts](../crates/contracts/src); edit source before running any source-derived export. `contracts/generated/` and `apps/web/src/generated/` are retained historical snapshots and are not the new HTTP API. Never hand-edit them or invoke removed server/runtime generators. `quazonai schema show NAME` exposes the CLI's supported schema views without opening service storage.

Deterministic tests use official Faux Provider and controlled downstream protocols. Browser HTTP tests use an explicitly TEST_ONLY scientific protocol subprocess; they establish HTTP/storage/interaction recovery, not scientific evidence. `check-core` and retained scientific CI execute the matching real kernel. Synthetic catalogs do not establish market/PIT quality, paid-model quality, profitability, real-account results or a literal 24-hour run. Record those separately if authorized.

<a id="runtime-image-build"></a>
## Image production

[Container action](../.github/actions/container/action.yml) reuses exact tested native outputs for the Node application and scientific image; normal npm/buildx/Rust dependency caches avoid duplicate compilation. [Version publishing](../.github/workflows/release-version.yml) and [dev publishing](../.github/workflows/dev-image.yml) preserve the release strategy and run disposable installation/restart/backup acceptance. Source tools remain at [runtimes/data](../runtimes/data); native image assembly at [runtimes/native](../runtimes/native).

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
