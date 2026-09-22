# Contributing to QuaZonai

Contributions that make research results easier to trust, reproduce and inspect are welcome: reproducible bug reports, focused fixes, better examples and clearer documentation all help. The project is maintained by [@zhengui666](https://github.com/zhengui666); no response-time or release schedule is promised.

## Choose a change

Work from the owner's concrete request or an existing Issue. No separate proposal, approval ledger or mandatory form is needed. Never post credentials, private data or hidden model reasoning.

## Set up a checkout

Create a branch from current main; a fork is optional. If you already have a checkout, inspect its worktrees and preserve unrelated modifications before changing branches.

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

This development UI proxies the real loopback Rust API. Configure PUBLIC_URL for the browser origin and start the real dependencies described in [OPERATIONS](OPERATIONS.md#首次启动认证服务). There is no separate preview backend or automatic sample data.

## Find the right layer

Start with the relevant [DESIGN](DESIGN.md) section. The design owns product behavior; update it before changing a contract. [AGENTS](AGENTS.md) owns development rules and [.opensdlc/project.md](.opensdlc/project.md) routes contributors and agents to actual commands and sources.

- Put wire types in `contracts`, QZ decisions in `domain`, atomic persistence in `store`, and orchestration/transport in the existing application entrypoints. Do not add a service, repository abstraction or compatibility wrapper without a real boundary.
- Prefer existing implementations, the standard library, platform features and installed Rust dependencies. Keep upstream algorithms, Codex sessions and OCI lifecycle in their native owners.
- Rust uses rustfmt and Clippy; TypeScript uses the existing types, tests and official Ant Design components. Follow local naming and layout; keep new behavior close to its tests. `.editorconfig` supplies editor defaults, not a replacement formatter.
- Never hand-edit `contracts/generated/` or `apps/web/src/generated/`. Run the native generator for the changed contract as shown in [CLI](CLI.md#原生组件与合同验证) or [Runtime](runtimes/native/README.md), then `npm --prefix apps/web run generate`. Commit source and generated changes together; review the diff.
- New dependencies need a concrete capability, license/version evidence and an existing-owner check; reuse research belongs in the relevant [DESIGN](DESIGN.md) section. Preserve LICENSE/NOTICE. Legacy code can be removed; user data and backups cannot.

## Verify the change

Run commands at the repository root. Start with the affected behavior, then exercise the boundaries it crosses. The table chooses checks; it does not waive any applicable CI.

| Change | Local entry | Failure means / next action |
| --- | --- | --- |
| Markdown, runbook or CLI help | `make check-docs` | Broken file/anchor or help command: fix its source, not the check's exclusions |
| Package ownership | `make check-architecture` | Forbidden direct dependency: move behavior to its owner or obtain a DESIGN change |
| Rust logic | `make check-unit` plus the affected package test | Formatting, lint or behavior regression: fix it and rerun the affected checks |
| Transactions, worker, HTTP or identity | `make check-store`, `make check-http`, or `make check` with disposable prerequisites | Persistence/transport failure: reproduce against the real native components |
| Web or API contracts | `make check-web` | Generated drift, type, test or build failure: regenerate from source and fix behavior |
| Browser, PWA, real API/Worker, gateway and restart | `CADDY_BIN=/path/to/caddy npm --prefix apps/web run test:e2e` with the [Web workflow prerequisites](.github/workflows/web.yml) | Real gateway/authentication/persistence failure: inspect sanitized results and clean only test resources |
| Native computation / OCI | `make native OUTPUT=/tmp/quazonai-native-example` with a new output path; [Runtime tests](runtimes/native/README.md) for OCI | A scientific or isolation failure: retain diagnostics; do not substitute a fixture for acceptance |

```sh
cd apps/web
node node_modules/@playwright/test/cli.js install chromium
```

The native browser checks require the actual systemd user manager, disposable PostgreSQL, Caddy and built server/web artifacts. Use the exact [Web workflow prerequisites](.github/workflows/web.yml), not a second setup copied here. The harness runs real API/Worker/gateway services and checks three viewports, themes, accessibility, PWA updates, restart, session and receipt persistence.

`check-unit` excludes Store/Server tests and is not the full suite. Architecture checks inspect normal/build dependencies, not transitive code or test-only helpers. Links/help/schema checks cannot execute account-dependent runbooks. State unrun checks and reasons; a skipped prerequisite never becomes a pass.

For a bug, establish the failing behavior before the fix and keep a focused regression check. Reuse the repository's tests; do not introduce a framework for a single assertion. After modifying source, rerun relevant validation on that source. Do not delete assertions or loosen expectations simply to turn CI green.

## Submit and maintain

Use one `.opensdlc/tasks/<task-id>/task.md` for each nontrivial task: intent, specification, plan, actual verification, review, delivery and handoff. Reuse the relevant task record; do not append new work to an unrelated historical task. Small corrections can keep the record short. New task prose/IDs use English unless `.opensdlc/config.json` explicitly selects `zh-CN`; preserve existing document languages. Do not generate empty release/incident files.

Open a PR against `main` using the [existing template](.github/PULL_REQUEST_TEMPLATE.md). Describe the concrete problem, resulting behavior, relevant tests, contract/docs changes and remaining limits. Link the task and issue; do not automatically close broader unfinished scope. Authors using AI remain responsible for understanding and verifying every submitted change. GitHub Codex reviews; it is not the product-code author.

Follow the [review policy](.opensdlc/review.md): address actionable findings, rerun checks, and request review for every new Head. Passing CI alone is not approval. The owner's current scoped authorization permits merge only after explicit clean Codex review, all applicable CI and resolved review threads. Product deployment has its own authorization and recovery requirements in [operations](.opensdlc/operations.md).
