# Restore GPT-6 discovery from native Codex

<a id="task"></a>
## Task

**Task ID:** fix-native-codex-gpt6-discovery

**Source:** Owner request on 2026-09-23 to branch from current `main`, diagnose and fix local GPT-6 model discovery, open a PR, and merge it after review and CI.

**Endpoint:** Merged PR on `main`, with current-head checks, explicit clean `@codex review`, and post-merge verification. Installing the merged release is a separate production operation.

<a id="intent"></a>
## Intent

The local Codex CLI advertises `gpt-6-astra`, `gpt-6-sol`, and `gpt-6-luna`, while QuaZonai cannot produce a matching available model observation. The owner additionally requires support for future local Codex versions. Preserve native model IDs, efforts, defaults, authentication, and account isolation; do not add model-name exceptions or copy credentials.

<a id="spec"></a>
## Requirements and design

The Codex profile probe must accept future native versions when their actual initialize, account, model, and Thread protocol responses satisfy QuaZonai's bounded contract. It must complete `model/list` pagination and persist exactly the models and capabilities returned. The observed version is recorded, not compared with a build-time constant. A resumed Mission remains bound to the version it originally used. Missing installation, authentication, or incompatible protocol must remain explicit failures. The API and Worker continue to use their own OS user's PATH and CODEX_HOME, as required by [DESIGN](../../../DESIGN.md).

Codex 0.156.1 also replays a prior Turn start notification after Thread resume. The current Mission driver may ignore it only when the Turn belongs to the same persisted Session and already has an immutable settlement receipt; an unknown Turn or a different Thread remains a correlation failure.

Before this task, the source rejected any version except 0.144.4 in the App Server client and domain probe validation. The current host CLI is 0.156.1; the running QuaZonai processes use UID 951, whose PATH and native account state are separate from the desktop user's. Production account and PATH alignment must be checked under UID 951 after deployment.

<a id="plan"></a>
## Implementation plan

1. Compare the official 0.156.1 App Server protocol with the adapter's account, model, Thread, tool, and permission paths; reproduce the existing version failure using the new binary.
2. Remove runtime exact-version gates and propagate the validated observed version through probe records and Mission receipts. Keep resume version binding and bounded protocol validation; never guess capabilities from the version string.
3. Upgrade the pinned CI protocol fixture to 0.156.1 and align its lockfile, assertions, notices, and DESIGN text. Adjust only native test assumptions that changed. Verify no-account handshake, full catalog, default/override Thread behavior, account policy, and fixture-backed Mission flow without paid inference or real credentials.
4. Push one focused PR. Resolve review findings, rerun applicable CI on the final head, request `@codex review` until it explicitly reports no issues, then merge and reread `main`.

The main risk is a future native protocol or sandbox change. QuaZonai accepts newer versions only when the observed protocol passes its existing checks; incompatible responses fail explicitly. Runtime service credentials and existing databases are outside this PR.

<a id="verification"></a>
## Verification

Before the fix, the native server test using the host's official 0.156.1 binary failed at `Client::start` with `NativeFailure::Version` (exit 101). Direct isolated 0.156.1 App Server checks returned all three GPT-6 models and accepted the existing Thread and Mission request shapes without paid inference. This does not prove UID 951's account catalog.

After the fix, `cargo test --locked -p server --features native-codex --test codex_native` passed 6/6 with the official local 0.156.1 binary, including GPT-6 catalog and persisted Thread recovery. `cargo test --locked -p domain --test codex_profiles` passed 6/6; `cargo test --locked -p domain --lib codex::native_version` passed 2/2. The CI fixture lock was regenerated for 0.156.1 and `npm ci --dry-run --prefix runtimes/codex --ignore-scripts --no-audit --no-fund` succeeded. PostgreSQL-backed Mission/Store and final-head CI evidence is recorded in [PR #108](https://github.com/zhengui666/QuaZonai/pull/108).

`cargo test --locked -p store --test missions --no-run` compiled the new database predicate test. An isolated `make check-web` rerun regenerated contracts without a diff, passed TypeScript checking, passed all 528 Vitest cases, and built the production Vite/PWA bundle. The native Runtime and Store fixtures were updated for observed 0.156.1 MCP output, settled-Turn replay, and interrupted Turn snapshots; their PostgreSQL execution is part of PR #108 CI.

<a id="review"></a>
## Review

[PR #108](https://github.com/zhengui666/QuaZonai/pull/108) is the review record. Merge requires explicit clean `@codex review` on its final Head, all review threads resolved, and all applicable CI passing at that Head.

<a id="delivery"></a>
## Delivery

[PR #108](https://github.com/zhengui666/QuaZonai/pull/108) is the delivery record. Merge and post-merge verification follow the final-head gate above. The running service needs a separately authorized release installation before its behavior can change.
