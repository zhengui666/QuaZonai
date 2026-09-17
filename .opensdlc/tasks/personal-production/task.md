# Personal production

<a id="task"></a>
## Task

**Task ID:** `personal-production`.

**Source:** the owner's 2026-09-17 request to turn the demonstration into a genuinely usable personal project across code, architecture, documentation, deployment and user experience, using CodexPro and OpenSDLC.

**Endpoint:** implemented behavior, actual acceptance of the complete research/delivery/recovery workflow, latest-Head CI and explicit independent clean review, then authorized delivery. A deployment template or a scoped maintenance PR does not complete this endpoint or [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

<a id="intent"></a>
## Intent

A personal installation must have a real, persistent entry point, understandable operations and recoverable failures. Keep the existing single-user, Rust, official Ant Design and target-only ownership model. Remove obsolete product-facing development narration without removing limitations, migration history, licenses, user data or the evidence required to verify delivery.

Reuse the completed [maintenance](../production-maintenance/task.md) and [onboarding](../open-source-foundation/task.md) work rather than recreating their policies. [DESIGN](../../../DESIGN.md) remains the product contract; [the existing evidence index](../../../docs/architecture/issue-62-execution.md) owns whole-product coverage.

<a id="spec"></a>
## Requirements and design

The first implementation slice is a maintainable personal-hosting boundary, not a new business engine:

- **Code:** protect the actual React root against rendering failures; recovery must not silently replay mutations, expose exception contents or imply that reloading cancels server work.
- **Architecture:** retain the current API, Worker, PostgreSQL/PGMQ, Codex and separate Runtime responsibilities. Reuse native Caddy for static files and same-origin reverse proxying, and systemd for process supervision. Do not add a custom proxy, orchestration DSL, migration-on-start behavior or microservices.
- **Documentation:** put real hosting before synthetic preview, with one operator path and links to authoritative configuration, research and recovery contracts. Keep the preview explicitly synthetic.
- **Deployment:** provide actual API/Worker service definitions, operator-owned environment examples and a gateway configuration. Preserve Host/Origin, API status/body, SSE streaming and missing-asset errors. Never turn an API failure into the SPA shell. Keep the built web directory separate from private state, credentials and the source checkout.
- **Experience:** render a readable, keyboard-accessible recovery state at desktop/tablet/mobile widths; require confirmation before a full reload discards unsubmitted UI state. Already submitted operations can remain active or have unknown outcomes.

This packaging instantiates existing contracts; it does not alter qualification, approval, identity or research state machines. A live process is not readiness, and neither is research acceptance. Public TLS issuance, actual host restart, persistent data, real Codex/data/Runtime research, target delivery and backup restoration still require their respective real environments.

<a id="plan"></a>
## Implementation plan

Inspected baseline: `0b9a47aa117175e9e204974b3b6942a427787fe1`; source tree `17e57ac411091002c10cd0f1d625b7a92d280179`. Read AGENTS, DESIGN ownership/layout, existing OpenSDLC tasks, README, architecture navigation, server CLI/router/shutdown/state creation, web entry/Vite/PWA/Playwright and the existing Web workflow. The API already has native graceful shutdown; reuse it rather than claim a new fix.

1. Add `apps/web/src/AppErrorBoundary.tsx`, wrap `apps/web/src/main.tsx`, and add a real-browser rendering-fault regression to the existing three-viewport suite without production test hooks or new dependencies.
2. Add `deploy/Caddyfile`, `deploy/systemd/quazonai-api.service`, `deploy/systemd/quazonai-worker.service` and `deploy/quazonai.env.example`. State initialization remains an explicit, new-directory-only command; migrations continue to use a separate privileged identity.
3. Add `deploy/proxy.test.mjs` that runs real Caddy against an explicitly synthetic local HTTP peer and static directory. Test routing, Host/Origin, error preservation, missing assets, methods and streaming. Use Node's existing test runner. Add native deployment validation in `.github/workflows/deployment.yml`; do not deploy or inject production secrets.
4. Add `docs/user-guide.md` for personal hosting and recovery; simplify README navigation and remove its product-facing development-research narration while retaining canonical research records and truthful acceptance limits.
5. Run available checks, inspect exact-Head CI and request read-only GitHub Codex review. Fix actual findings. Keep this branch unmerged if required checks or review are unavailable. Do not substitute successful fixtures for the owner's complete production requirement.
6. Continue with the existing evidence index's genuine whole-product gaps: full authorized research and independent evaluation, portfolio/target delivery, operational recovery and persistent real-host/browser acceptance. Audit large domain/UI modules and obsolete active documents with their call chains before deleting or restructuring them.

Highest risks: treating a template as a tested installation, masking API failures with HTML, losing in-memory unresolved-request data on reload, and erasing valid historical evidence while cleaning presentation. Do not reset the original dirty checkout or other worktrees.

<a id="verification"></a>
## Verification

At plan creation, no implementation checks have run for this branch. CodexPro's direct workspace entry, stable wrapper and configuration read returned upstream HTTP 502; no local workspace contents or local execution are claimed. GitHub reads and the isolated branch creation succeeded. Earlier PR results are baseline history only.

The deployment fixture is a routing test, not real API/database/market acceptance. systemd parsing is not a started service. Browser rendering-fault tests are not real research acceptance. Record exact commands and outcomes as they occur; queued, unavailable, failed or skipped checks are not passes.

<a id="review"></a>
## Review

Independent review and latest-Head CI are pending. GitHub will own their authoritative results. GitHub Codex may review only; it must not implement, commit or alter this branch. No author self-review counts as independent approval.

<a id="delivery"></a>
## Delivery

No merge, deployment or production acceptance has occurred in this task. The complete owner request remains open even when this first implementation slice is published.

<a id="handoff"></a>
## Handoff

Work branch: `codex/personal-production-20260917`, isolated from main and the inaccessible local checkout. Reconcile its actual Head, PR, CI and review before retrying writes or continuing. Recover CodexPro access before claiming any verification on the owner's host. Do not request or transmit account credentials in chat, logs or repository files.
