# Contributing to QuaZonai

Contributions that make research results easier to trust, reproduce and inspect are welcome: reproducible bug reports, focused fixes, better examples and clearer documentation all help. The project is maintained by [@zhengui666](https://github.com/zhengui666); no response-time or release schedule is promised.

## Choose a change

Search [existing issues](https://github.com/zhengui666/QuaZonai/issues) and [pull requests](https://github.com/zhengui666/QuaZonai/pulls) before starting. Small documentation fixes and focused bug fixes can go straight to a PR. Discuss substantial features, new dependencies and architecture changes with the maintainer first. An owner's existing explicit authorization does not need to be repeated per file or command.

Use the [bug or feature form](https://github.com/zhengui666/QuaZonai/issues/new/choose); include the commit, expected/actual result, smallest reproducer and sanitized diagnostics. Never post credentials, private market data, wallet material or hidden model reasoning. For private reports, use the owner's [profile contact route](https://github.com/zhengui666), initially without sensitive details. Keep discussion respectful and specific; review the change, not its author. Maintainers may moderate abuse.

## Set up a checkout

Fork the repository, clone your fork, add the upstream below and create a branch from current main. If you already have a checkout, inspect its worktrees and preserve unrelated modifications before changing branches.

```sh
git remote add upstream https://github.com/zhengui666/QuaZonai.git
git fetch upstream main
git switch -c codex/my-change upstream/main
```

Backend development targets Linux x86_64 with a C toolchain, make and rustup. Use the exact compiler in [rust-toolchain.toml](rust-toolchain.toml). The frontend requires Node.js ≥22.12; CI uses Node.js 24. Install pinned dependencies from the repository root:

```sh
rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy
rustup run 1.98.1 rustc -Vv
npm ci --prefix apps/web --ignore-scripts --no-audit --no-fund
npm ci --prefix runtimes/codex --ignore-scripts --no-audit --no-fund
rustup run 1.98.1 cargo install --locked lychee --version 0.24.2
```

`make` explicitly selects the pinned Rust compiler, even if a distribution Cargo appears first in PATH. Do not lower the compiler version when a dependency fails. First check the actual toolchain, locked dependency and platform. Native OCI tests additionally need Docker and `wasm32-unknown-unknown`; use the [Runtime guide](runtimes/native/README.md). The [cold archive target](apps/runtime/tests/native_restore.rs) also requires GNU tar/chown and noninteractive sudo for its own disposable mixed-owner files. Run Cargo and the Runtime as an ordinary user, not root; only the selected archive/ownership commands are elevated. Store/HTTP tests need a **disposable PostgreSQL 18 + PGMQ 1.10.0 instance**, native Codex and OS prerequisites from [CLI](CLI.md#开发测试) and the [existing CI setup](.github/workflows/ci.yml). Do not point tests at a user database.

For the frontend:

```sh
npm --prefix apps/web run dev
```

This development UI uses the configured loopback API proxy. The credential-free UI preview is instead `make demo-preview`; see [README](README.md#quickstart). Real service setup, PUBLIC_URL and identity initialization belong to [OPERATIONS](OPERATIONS.md#首次启动认证服务).

## Find the right layer

Start with [the architecture guide](docs/architecture.md) and the relevant [DESIGN](DESIGN.md) section. The design owns product behavior; update it before changing a contract. [AGENTS](AGENTS.md) owns development rules and [.opensdlc/project.md](.opensdlc/project.md) routes contributors and agents to actual commands and sources.

- Put wire types in `contracts`, QZ decisions in `domain`, atomic persistence in `store`, and orchestration/transport in the existing application entrypoints. Do not add a service, repository abstraction or compatibility wrapper without a real boundary.
- Prefer existing implementations, the standard library, platform features and installed Rust dependencies. Keep upstream algorithms, Codex sessions and OCI lifecycle in their native owners.
- Rust uses rustfmt and Clippy; TypeScript uses the existing types, tests and official Ant Design components. Follow local naming and layout; keep new behavior close to its tests. `.editorconfig` supplies editor defaults, not a replacement formatter.
- Never hand-edit `contracts/generated/` or `apps/web/src/generated/`. Run the native generator for the changed contract as shown in [CLI](CLI.md#原生组件与合同验证) or [Runtime](runtimes/native/README.md), then `npm --prefix apps/web run generate`. Commit source and generated changes together; review the diff.
- New dependencies need a concrete capability, license/version evidence and an existing-owner check; reuse research belongs in [docs/research/reuse.md](docs/research/reuse.md). Preserve LICENSE/NOTICE. Legacy code can be removed; user data and backups cannot.

## Verify the change

Run commands at the repository root. Start with the affected behavior, then exercise the boundaries it crosses. The table chooses checks; it does not waive any applicable CI.

| Change | Local entry | Failure means / next action |
| --- | --- | --- |
| Markdown, runbook or CLI help | `make check-docs` | Broken file/anchor or help command: fix its source, not the check's exclusions |
| Package ownership | `make check-architecture` | Forbidden direct dependency: move behavior to its owner or obtain a DESIGN change |
| Rust logic | `make check-unit` plus the affected package test | Formatting, lint or behavior regression: fix it and rerun the affected checks |
| Transactions, worker, HTTP or identity | `make check-store`, `make check-http`, or `make check` with disposable prerequisites | Persistence/transport failure: reproduce against the real native components |
| Web or API contracts | `make check-web` | Generated drift, type, test or build failure: regenerate from source and fix behavior |
| Browser, preview or PWA | Install Chromium below, then `npm --prefix apps/web run test:e2e` after `make check-web` | UI/accessibility/offline regression: inspect the actual browser at all three viewports |
| Real hosted API/Worker user services and restart | `CADDY_BIN=/path/to/caddy npm --prefix apps/web run test:e2e:native` with the [Web workflow prerequisites](.github/workflows/web.yml) | Real gateway/authentication/persistence failure: inspect sanitized results and clean only test resources |
| Native computation / OCI | `make native OUTPUT=/tmp/quazonai-native-example` with a new output path; [Runtime tests](runtimes/native/README.md) for OCI | A scientific or isolation failure: retain diagnostics; do not substitute a fixture for acceptance |

```sh
cd apps/web
node node_modules/@playwright/test/cli.js install chromium
```

The native browser flow additionally needs an ordinary Linux user with a running systemd user manager, its actual `XDG_RUNTIME_DIR`, cgroup v2, `systemctl`/`journalctl`, and the built `target/debug/server`, production `apps/web/dist`, an explicitly supplied disposable loopback PostgreSQL administrator (`QUAZONAI_WEB_TEST_ADMIN_URL`), `psql`, and native Caddy (2.11.4 in CI; set `CADDY_BIN` to its path or install it on PATH). It copies the reviewed binary/build and both shipped `deploy/systemd/` units into its private release fixture. Runtime-only native links and path/environment-file drop-ins run the real API and Worker with their unchanged restart/stop policy. The test verifies their actual executable, subcommand, user, configuration and native properties, then observes an idle Worker's automatic restart after one unit-targeted SIGKILL. The actual `deploy/Caddyfile` stays online while the API unit stops normally and starts a new process; the second browser phase must retain the original session/project/command receipt. It does not use Vite preview, seed application rows, reinstall state or re-enroll. Cleanup stops only its own units, checks they are empty, and removes their runtime links before dropping its database and private state. Unverified shutdown or cleanup fails and retains private state; no failure screenshots are uploaded. Public TLS, host boot, active-job recovery and real research still require separate acceptance.

`check-unit` excludes Store/Server tests and is not the full suite. Architecture checks inspect normal/build dependencies, not transitive code or test-only helpers. Links/help/schema checks cannot execute account-dependent runbooks. State unrun checks and reasons; a skipped prerequisite never becomes a pass.

For a bug, establish the failing behavior before the fix and keep a focused regression check. Reuse the repository's tests; do not introduce a framework for a single assertion. After modifying source, rerun relevant validation on that source. Do not delete assertions or loosen expectations simply to turn CI green.

## Submit and maintain

Use one `.opensdlc/tasks/<task-id>/task.md` for each nontrivial task: intent, specification, plan, actual verification, review, delivery and handoff. Continue this delivery in the existing [personal-production task](.opensdlc/tasks/personal-production/task.md); other work reuses its own relevant task instead of writing into the old onboarding record. Small corrections can keep the record short. New task prose/IDs use English unless `.opensdlc/config.json` explicitly selects `zh-CN`; preserve existing document languages. Do not generate empty release/incident files.

Open a PR against `main` using the [existing template](.github/PULL_REQUEST_TEMPLATE.md). Describe the concrete problem, resulting behavior, relevant tests, contract/docs changes and remaining limits. Link the task and issue; do not automatically close broader unfinished scope. Authors using AI remain responsible for understanding and verifying every submitted change. GitHub Codex reviews; it is not the product-code author.

Follow the [review policy](.opensdlc/review.md): address actionable findings, rerun checks, and request review for every new Head. Passing CI alone is not approval. The owner's current scoped authorization permits merge only after explicit clean Codex review, all applicable CI and resolved review threads. Product deployment has its own authorization and recovery requirements in [operations](.opensdlc/operations.md).
