# QuaZonai project context

<a id="purpose"></a>
## Purpose and architecture

A single-user research workbench: Web/CLI/MCP → Rust API → PostgreSQL/PGMQ → Worker → Codex or a scientific Runtime. The output is research evidence and target portfolios, not broker orders. Module ownership and domain invariants are in [architecture](architecture.md).

<a id="commands"></a>
## Development and checks

Work at the repository root on a branch from the requested delivery base (normally main; dev for development releases). Preserve unrelated worktrees. Use the compiler in [rust-toolchain.toml](../rust-toolchain.toml); the [Makefile](../Makefile) selects it explicitly. Dependency versions come from Cargo/npm lockfiles. The frontend Node requirement is in [package.json](../apps/web/package.json); CI setup is authoritative for native prerequisites.

```sh
npm ci --prefix apps/web --ignore-scripts --no-audit --no-fund
npm ci --prefix runtimes/codex --ignore-scripts --no-audit --no-fund
```

| Change | Check from the repository root | Required result / prerequisite |
| --- | --- | --- |
| Markdown and CLI/Skill documentation | `make check-docs` | Links, native help and Skill contracts pass; requires the CI-pinned lychee and Rust dependencies |
| README or Docker deployment guide | `make check-links` and `node --test deploy/install.test.mjs` | Run both: the latter checks Bash examples and native helper help with Node, Bash and Python; it does not install services and is not included in `make check-docs` |
| Markdown links only | `make check-links` | All tracked Markdown, including `.opensdlc`, resolves; this does not run CLI checks |
| Package ownership and upstream sources | `make check-architecture` | Allowed dependency directions and official source packages; Cargo fetches missing locked packages when the cache is cold |
| Portable CLI and release assets | `cargo test --locked -p quazonai-cli` and `python3 -B -m unittest discover -s deploy -p '*_test.py'` | Native Windows/macOS/Linux jobs in the CLI workflow execute the actual binaries; installer checks use `node --test deploy/install.test.mjs` |
| Rust rules and local components | `make check-unit` | All-target/all-feature check, lib/bin build, classified rules and disposable-file component tests; no database/native services; not the full suite |
| Original portfolio-release stack regression | `make check-release-regression` | Unchanged shared case, once in each original Store/Server target; disposable PostgreSQL/PGMQ; both must run and pass |
| Transactions | `make check-store` | Disposable PostgreSQL/PGMQ through `DATABASE_URL` |
| HTTP and Worker | `make check-http` | Disposable database plus native Codex/system prerequisites from [Rust regression](../.github/workflows/rust-regression.yml) |
| Routine Rust validation | `make check` | `check-unit` plus both original real-PG stack cases; no production database |
| Full former Rust regression | `make check-regression` or [Rust regression workflow](../.github/workflows/rust-regression.yml) | Native Codex/CLI/systemd and disposable PostgreSQL/PGMQ prerequisites; manual and mandatory before version image production |
| Web and generated client | `make check-web` | No generated drift; types, tests and production build pass |
| Browser/PWA | `CADDY_BIN=/path/to/caddy npm --prefix apps/web run test:e2e` | Real API, Worker, PostgreSQL, Caddy and systemd user manager; use the Web workflow setup |
| Data acquisition/Worker/browser closure | [Native data browser workflow](../.github/workflows/native-data-browser.yml) | Original synthetic candle clocks, real preparation/registration/Worker/OCI, exact artifact downloads and restart identity; optional manually dispatched acceptance |
| Scientific/OCI boundaries | [Native Runtime workflow](../.github/workflows/native-runtime.yml) | Built native image, Docker/cgroup prerequisites, actual execution/cancellation/restore tests |
| Container installer | [Container action](../.github/actions/container/action.yml) | Real installation, upgrade and recovery against disposable resources |

Routine PR and main/dev push validation runs only [Rust CI](../.github/workflows/ci.yml). It checks the entire workspace with all targets/features, links production lib/bin targets with the existing optional Job/Server feature set, and executes classified rule/local component tests. This retains Contracts/Domain rules located in Cargo integration targets, eight non-PG Store lib tests and default-feature Server rules. CLI HTTP/FIFO/subprocess, compiler/ZIP subprocess, architecture-process, full-filesystem and native system probes are outside this routine group. See [the command plan](../tests/native/rust_unit.sh) for the exact selectors.

The native routine job restores a dependency-only `Swatinem/rust-cache` after installing the pinned toolchain. Its compiler/host/environment and manifest/lock inputs are supplemented by the workflow, root Cargo configuration and both shared test-command scripts. Workspace crate outputs, installed binaries and incremental state are excluded; the action also sets `CARGO_INCREMENTAL=0`. Failure saving is enabled but cancellation/timeout does not guarantee a save. Compare actual same-plan cold/hot restore, compilation and save timings before claiming a benefit.

The separate required `store-postgres` job executes the unchanged `qualified_portfolio::release_freezes_original_package_and_replays_without_republishing` once in Store `experiment_compilations` and once in Server `portfolio_study_http` with `native-codex`. Each target must list exactly one match and actually pass one non-ignored test; either failure fails the job, and Store failure still allows Server to run. This is real PostgreSQL/PGMQ transaction regression, not a pure unit test or the complete Store/Server suite. Its selected case needs no official Codex, npm, portable CLI or systemd setup.

[Rust regression](../.github/workflows/rust-regression.yml) preserves the former complete default Rust CI jobs without the routine skips: all-target build/linking, doctests, scientific/native CLI/Paper/Sandbox/SQLite/system/filesystem components and the broad real-PG Store/Server/native-Codex suite. Run it manually for integration changes; the version publisher calls it directly at the release revision and cannot build/push images before it succeeds. The routine lib/bin build no longer links integration/example executables, though `cargo check --all-targets --all-features` still compiles their code. Broad integration and PG business-chain execution therefore moves out of every PR, and remains mandatory for every version publication.

Frontend, documentation and benchmarks remain optional focused checks. Native Runtime/OCI, Native data browser, Web, Polymarket history and Container workflows remain manually dispatched acceptance workflows. Portable CLI packaging and image/install acceptance remain in the version publisher.

Store/HTTP tests use PostgreSQL 18 with PGMQ 1.10.0. [Rust regression](../.github/workflows/rust-regression.yml) defines the native binaries and system environment needed by the complete former suite; [CI](../.github/workflows/ci.yml) starts only the disposable PG/PGMQ needed by its two required selected cases. [Web console](../.github/workflows/web.yml) defines browser setup, including Chromium. Cold-archive tests may elevate only their disposable ownership/archive operations, never Cargo or the application.

For a contract change, edit Rust DTOs/handlers, export the affected native schema with `cargo run --locked -q -p contracts --example generate`, `cargo run --locked -q -p server -- openapi`, or `cargo run --locked -q -p runtime -- openapi` into the corresponding `contracts/generated/` snapshot, then run `npm --prefix apps/web run generate`. Commit the source and generated diff together. `quazonai openapi --list-schemas` and `quazonai openapi --schema ArtifactCreate` inspect an installed binary offline; they do not query a running server's version. Add `--domain` to inspect native domain DTOs such as `ExperimentEvaluationParametersV1` and `FeatureObservationsV1`; this uses the same Rust export as the contracts generator.

Domain and HTTP exports are sorted compact JSON snapshots with a final newline;
their native values and byte-for-byte drift checks remain authoritative. Response
validators use the modular native Ajv graph and pinned Vite/esbuild transformer,
with exports, aliases and validation errors preserved. Production consumers use
selective scalar facades and asynchronous response validation; the eager facade
is for compatibility. Edit the Rust source or generator rather than generated
artifacts. Generator tests check independent output directories, native CJS/ESM
loading and equivalence to the original response schemas; the production PWA
retains its 2 MiB chunk gate.

`npm --prefix apps/web run dev` is a development UI proxy for a real local API. Browser behavior is verified by the native harness, not a substitute backend. Scientific samples belong to tests, not product entrypoints.

<a id="runtime-image-build"></a>
### Image production

[Container action](../.github/actions/container/action.yml) builds the application, scientific job and Codex images, then installs those already-built artifacts. [Version publishing](../.github/workflows/release-version.yml) pushes the tested images and validates GHCR installation on a fresh runner before publishing the deployment bundle. The job-image assembler lives in [runtimes/native](../runtimes/native); deployment uses the manifest and installed [Runtime entry](operations.md#scientific-runtime).

<a id="conventions"></a>
## Conventions

Follow [AGENTS](../AGENTS.md) and the owning module's existing types. Keep regression checks at the failing boundary; do not weaken assertions to pass CI. New dependencies need a concrete capability and compatible licensing. Numerical performance claims need equal outputs and a recorded workload/profile.

Use one `tasks/<task-id>/task.md` linking the Issue and PR. New task IDs/prose use English unless `.opensdlc/config.json` selects `zh-CN`; existing IDs and document languages stay unchanged. Split a specification only when it needs its own maintained body. Do not copy templates, source schemas or full execution logs into project context.

<a id="owners"></a>
## Responsibilities

[@zhengui666](https://github.com/zhengui666) owns requirements and the service. Authorship/execution boundaries are in [AGENTS](../AGENTS.md); merge criteria are in [review](review.md). Product researchers, evaluators and downstream capabilities are separate execution roles, not additional human users.

<a id="sources"></a>
## Sources

| Subject | Canonical source |
| --- | --- |
| Product behavior and package directions | [architecture](architecture.md) |
| Wire fields and validation | [Rust contracts](../crates/contracts/src), [API OpenAPI](../contracts/generated/api-v2.openapi.json), [Runtime OpenAPI](../contracts/generated/runtime-v1.openapi.json), [domain schema](../contracts/generated/domain-v1.openapi.json) |
| Persistence and migrations | [Store](../crates/store/src), [migrations](../migrations) |
| UI and brand | [web source](../apps/web/src), [product icon](../apps/web/public/icon.svg) |
| User operation | [README](../README.md), [deployment bundle](../deploy/docker/README.md), [service Skill](../skills/quazonai/SKILL.md) |
| Delivery, recovery and measurements | [operations](operations.md), actual Issue/PR/Actions records |

<a id="native"></a>
## Native entrypoints

[AGENTS.md](../AGENTS.md) remains the coding-agent entry. [skills/quazonai/SKILL.md](../skills/quazonai/SKILL.md) is the independently installable service-operation Skill; its [portable-package checks](../apps/server/tests/client_skill.rs) do not require developer documentation. [GitHub workflows](../.github/workflows) and the [PR template](../.github/PULL_REQUEST_TEMPLATE.md) retain their native locations. Instruction reviews use the [evaluation cases](evals/suite.md).
