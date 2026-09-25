# QuaZonai project context

<a id="purpose"></a>
## Purpose and architecture

A single-user research workbench: Web/CLI/MCP → Rust API → PostgreSQL/PGMQ → Worker → Codex or a scientific Runtime. The output is research evidence and target portfolios, not broker orders. Module ownership and domain invariants are in [architecture](architecture.md).

<a id="commands"></a>
## Development and checks

Work at the repository root on a branch from current main. Preserve unrelated worktrees. Use the compiler in [rust-toolchain.toml](../rust-toolchain.toml); the [Makefile](../Makefile) selects it explicitly. Dependency versions come from Cargo/npm lockfiles. The frontend Node requirement is in [package.json](../apps/web/package.json); CI setup is authoritative for native prerequisites.

```sh
npm ci --prefix apps/web --ignore-scripts --no-audit --no-fund
npm ci --prefix runtimes/codex --ignore-scripts --no-audit --no-fund
```

| Change | Check from the repository root | Required result / prerequisite |
| --- | --- | --- |
| Markdown and CLI/Skill documentation | `make check-docs` | Links, native help and Skill contracts pass; requires the CI-pinned lychee and Rust dependencies |
| Markdown links only | `make check-links` | All tracked Markdown, including `.opensdlc`, resolves; this does not run CLI checks |
| Package ownership | `make check-architecture` | Allowed production/build dependency directions |
| Rust logic | `make check-unit` | Formatting, Clippy and non-Store/non-Server tests; not the full suite |
| Transactions | `make check-store` | Disposable PostgreSQL/PGMQ through `DATABASE_URL` |
| HTTP and Worker | `make check-http` | Disposable database plus native Codex/system prerequisites from CI |
| Full Rust suite | `make check` | Real disposable dependencies; no production database |
| Web and generated client | `make check-web` | No generated drift; types, tests and production build pass |
| Browser/PWA | `CADDY_BIN=/path/to/caddy npm --prefix apps/web run test:e2e` | Real API, Worker, PostgreSQL, Caddy and systemd user manager; use the Web workflow setup |
| Scientific/OCI boundaries | [Native Runtime workflow](../.github/workflows/native-runtime.yml) | Built native image, Docker/cgroup prerequisites, actual execution/cancellation/restore tests |
| Container installer | [Container action](../.github/actions/container/action.yml) | Real installation, upgrade and recovery against disposable resources |

Store/HTTP tests use PostgreSQL 18 with PGMQ 1.10.0. [CI](../.github/workflows/ci.yml) defines the native binaries, environment and schema export commands. [Web console](../.github/workflows/web.yml) defines browser setup, including Chromium. Cold-archive tests may elevate only their disposable ownership/archive operations, never Cargo or the application.

For a contract change, edit Rust DTOs/handlers, run the relevant native schema export from CI, then `npm --prefix apps/web run generate`. Commit the source and generated diff together. `server openapi --list-schemas` and `server openapi --schema ArtifactCreate` inspect an installed binary offline; they do not query a running server's version.

`npm --prefix apps/web run dev` is a development UI proxy for a real local API. Browser behavior is verified by the native harness, not a substitute backend. Scientific samples belong to tests, not product entrypoints.

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
