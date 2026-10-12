# QuaZonai project context

<a id="purpose"></a>
## Purpose and architecture

A single-user extensible unattended factor-research foundation: original goal/data → AI proposals and native computation → validation feedback → recorded iteration/stop. Official Pi Durable owns execution; NautilusTrader owns native data/indicators and optional simulation/accounting. Rust preserves scientific transactions, OCI recovery and history; portfolios and target-only delivery are optional, never real broker orders. See [architecture](architecture.md).

<a id="commands"></a>
## Development and checks

Use a branch from current remote `dev` for development and merge back to `dev`; preserve unrelated worktrees. Formal version/tag/default-branch strategy is unchanged. Use [rust-toolchain.toml](../rust-toolchain.toml) and locked Cargo/npm dependencies. The research host needs Node 22.19+ with built-in SQLite; the Web package/CI defines its own Node requirements.

```sh
npm ci --prefix apps/research --ignore-scripts --no-audit --no-fund
npm ci --prefix apps/web --ignore-scripts --no-audit --no-fund
```

| Change | Check at repository root | Actual scope/prerequisite |
| --- | --- | --- |
| Dependency boundaries and official sources | `make check-architecture` | Native Cargo graph and repository component manifests; no vendored/forked implementation |
| Durable host/extensions and default factor science | `make check-research` | TypeScript, official SQLite/HTTP/replay/SIGKILL/abort/children, then real Rust factor calculations and registered Node plugin integration; matching `job` and research npm dependencies |
| Routine rules/components | `make check-unit` | All-target/all-feature compile, lib/bin link and classified rules/local components; no broad PG/native acceptance claim |
| Original portfolio release regression | `make check-release-regression` | Actual unchanged release case once in Store and once in Server against disposable PostgreSQL/PGMQ |
| Routine project validation | `make check` | `check-unit`, durable/native factor checks and original real-PG stack cases |
| Full Rust/scientific/history/HTTP regression | `make check-regression` | All targets, doctests and full enabled suite; disposable PostgreSQL18/PGMQ1.10.0, portable CLI and isolated filesystem prerequisites from [Rust regression](../.github/workflows/rust-regression.yml) |
| Store or HTTP | `make check-store` / `make check-http` | Original isolated database; no Codex executable or test feature |
| Web/generated API | `make check-web` | Source-derived types/validator closure without drift, tests, production build and existing modular build measurement |
| CLI and service Skill | `make check-cli` | Actual native help, packaged Skill and schema/command contracts |
| Markdown/CLI docs | `make check-docs` | Offline local link/anchor checks plus native CLI checks; `LYCHEE` selects the installed checker |
| README or installation guide | `make check-links`; `node --test deploy/install.test.mjs` | Both link and actual Bash example/helper semantics; not service installation |
| Shell manager and package helpers | `node --test deploy/docker/manage_shell.test.mjs`; `python3 -B -m unittest discover -s deploy/docker -p '*_test.py'`; `python3 -B -m unittest discover -s deploy -p '*_test.py'` | Disposable files/command boundaries only; not real installed state |
| Browser/PWA | `CADDY_BIN=/path/to/caddy npm --prefix apps/web run test:e2e` | Native API/Worker, disposable PG, Caddy, Chromium and user services per [Web workflow](../.github/workflows/web.yml) |
| Source acquisition/Worker/browser | [Native data browser](../.github/workflows/native-data-browser.yml) | Original synthetic clocks through real source tools, native preparation/registration/OCI/Worker and browser; no market/PIT/quality claim |
| Scientific image execution/cancel/restore | [Native Runtime](../.github/workflows/native-runtime.yml) | This checkout's built native image and actual Docker/OCI recovery tests; version/doctor is insufficient |
| Real install/upgrade/backup/recovery | [Container action](../.github/actions/container/action.yml) | Actual prebuilt app/native images on isolated installation, not Python-only manager checks |
| Portable CLI release assets | `cargo test --locked -p quazonai-cli`; [CLI workflow](../.github/workflows/cli.yml) | Native supported Windows/macOS/Linux binary/package execution |

[CI](../.github/workflows/ci.yml) runs Rust rules/build, durable types/protocol/native factor integration, original selected real-PG/history/target regressions and Web/generated checks. [Rust regression](../.github/workflows/rust-regression.yml) retains the full scientific/Store/HTTP suite and is still called at the release revision before version image production. Applicable browser/container/native workflows are focused real-component acceptance, not claims made by routine compilation.

The existing dependency-only Rust cache excludes workspace outputs, installed binaries and incremental state. Do not delete unrelated local caches to make a check fit. Resource-constrained builds can lower jobs/debug output while keeping the actual selected tests. Missing prerequisites, ignored tests, absent outcomes or old-Head runs are not passes.

### Generate contracts

Edit original Rust DTOs/handlers, then export affected compact sorted snapshots:

```sh
cargo run --locked -q -p contracts --example generate > contracts/generated/domain-v1.openapi.json
cargo run --locked -q -p server -- openapi > contracts/generated/api-v2.openapi.json
cargo run --locked -q -p runtime -- openapi > contracts/generated/runtime-v1.openapi.json
npm --prefix apps/web run generate
```

Commit source and generated changes together. Native generation and byte-for-byte drift checks remain authoritative; never hand-edit generated clients. The portable CLI embeds the same snapshots and supports `quazonai openapi --schema NAME`, `--domain` and `--list-schemas` offline. Native durable views stay open JSON so adding an Extension does not require another core task-state schema.

`npm --prefix apps/web run dev` proxies a real API, not a substitute backend. The [research guide](../apps/research/README.md) documents the local Node host, provider/extension registration and scientific isolation. Models in deterministic tests use official Faux Provider; native calculations are real, but synthetic catalogs and scripted responses do not establish market quality, profitability or model quality.

<a id="runtime-image-build"></a>
## Image production

[Container action](../.github/actions/container/action.yml) builds matching application/research and scientific images; [version publishing](../.github/workflows/release-version.yml) tests those assets, preserves formal release strategy and packages the installed Bash manager. Scientific image assembly remains [runtimes/native](../runtimes/native); source tools remain [runtimes/data](../runtimes/data). Installation does not compile and does not depend on a host Codex/Node/Rust.

<a id="conventions"></a>
## Conventions and owners

Follow [AGENTS](../AGENTS.md) and source boundaries. Keep regression assertions at the actual scientific/transaction/recovery boundary; removing an execution engine must not erase historical data or unrelated scientific coverage. Third-party public APIs only; preserve licenses, migrations, backups and source facts. QZ does not maintain another Agent scheduler/session database, indicator, matching or account engine.

[@zhengui666](https://github.com/zhengui666) owns the goal and service. The task's explicit acceptance governs its final checks/delivery; a read-only review or a merged PR is not unperformed production deployment or account authorization.

<a id="sources"></a>
## Canonical sources

[Architecture](architecture.md) · [Rust contracts](../crates/contracts/src) · [Store/migrations](../crates/store/src) · [operations](operations.md) · [README](../README.md) · [deployment](../deploy/docker/README.md) · [review](review.md).

<a id="native"></a>
## Native entrypoints

[AGENTS.md](../AGENTS.md) is the coding context. [Service Skill](../skills/quazonai/SKILL.md) operates an installed service, not development or the retired Mission engine. Functional workflows remain at [.github/workflows](../.github/workflows), PR context at [.github/PULL_REQUEST_TEMPLATE.md](../.github/PULL_REQUEST_TEMPLATE.md), task evidence under `tasks/<task-id>/task.md`, and independent Agent evaluation sources under [evals](evals/suite.md). New task IDs/prose default to English when `.opensdlc/config.json` is absent; existing document language/IDs stay unchanged.
