# Personal production

<a id="task"></a>
## Task

**Task ID:** `personal-production`.

**Source:** the owner's 2026-09-17 request to make QuaZonai genuinely usable as a personal production project across code, architecture, documentation, deployment and UX, using CodexPro and OpenSDLC.

**Endpoint:** implemented behavior, real acceptance of the complete research/delivery/recovery workflow, latest-Head CI and explicit independent clean review, then authorized delivery. [PR #85](https://github.com/zhengui666/QuaZonai/pull/85) is the first implementation slice, not completion of the whole request or [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

<a id="intent"></a>
## Intent

A personal installation needs a persistent real-service entry, understandable operations and recoverable failures. Keep the existing single-user, Rust, official Ant Design and target-only model. Clean product-facing development narration without deleting limitations, migration history, licenses, user data, Git history or acceptance evidence.

Reuse the existing [maintenance](../production-maintenance/task.md) and [onboarding](../open-source-foundation/task.md) work. [DESIGN](../../../DESIGN.md) remains the product contract; the [existing evidence index](../../../docs/architecture/issue-62-execution.md) owns whole-product coverage. No repository language override was present; this new task and guide use OpenSDLC's English default while existing Chinese documents retain Chinese.

<a id="spec"></a>
## Requirements and design

| Area | Implemented slice | Remaining whole-request responsibility |
| --- | --- | --- |
| Code | Consolidate rendering recovery at the real root, covering App hooks and descendants; require confirmation and avoid printing exception payloads | Audit and resolve remaining domain/Worker/runtime gaps through their actual call chains |
| Architecture | Reuse Caddy and systemd user services around existing API/Worker/PostgreSQL/PGMQ/Codex/Runtime owners | Complete the original end-to-end product contract and real integrations |
| Documentation | Prioritize real hosting, separate synthetic preview, add one hosting/recovery guide, remove product-facing development narration | Audit active manuals and historical annotations without losing contracts or evidence |
| Deployment | Native user units, readable private configuration, lingering user manager, same-origin gateway and native regressions | Real selected-host TLS, restart, persistence, backup and restore acceptance |
| UX | Generic error state, keyboard-operable reload confirmation, no replay/cancellation claim, root/descendant faults at three widths; replace Issue-number footer narration with truthful product status | Full real research/evaluation/portfolio/target-delivery and mobile acceptance |

A live process is not readiness. Empty integration allowlists or missing Codex bindings are not research readiness. QZ does not own broker credentials, orders, positions, NAV or downstream trading controls. No domain state machine, generated contract or qualification rule changes in this packaging.

<a id="plan"></a>
## Implementation and reuse

Inspected baseline: `0b9a47aa117175e9e204974b3b6942a427787fe1`. Sources inspected include AGENTS, DESIGN ownership/layout, current OpenSDLC records, README, architecture navigation, CONTRIBUTING, server CLI/router/shutdown/state creation, web main/App/Vite/PWA/Playwright, Codex native process resources, and existing Web/Store CI. Native shutdown is reused, not claimed as a new implementation.

Paths: `apps/web/src/{App,AppErrorBoundary,main}.tsx`, `apps/web/tests/error-boundary.spec.ts`, `deploy/Caddyfile`, `deploy/systemd/quazonai-{api,worker}.service`, `deploy/quazonai.env.example`, `deploy/proxy.test.mjs`, `.github/workflows/deployment.yml`, `docs/user-guide.md`, README and this task.

The existing inner `RenderBoundary` is consolidated at the actual React root, not duplicated. Both root and nested-authentication fault fixtures exercise recovery; native React `onCaughtError` emits a fixed diagnostic only. The unchanged base and fault fixture preserve the pre-fix version, but a runtime pre-fix browser failure has not been observed in this session.

The native Mission implementation requires a user manager, actual XDG runtime directory and scopes under `/user.slice/`. Accordingly, API/Worker are **user units**, installed in `/etc/systemd/user` and enabled only for `quazonai`, with linger for boot/logout independence. Configuration is administrator-owned `root:quazonai` 0640 so the unprivileged manager can read it. The guide uses native `systemctl --machine=quazonai@.host --user`; no shell wrapper or alternate process supervisor is added. CI reuses the Store workflow's user-manager preparation and exercises a user-service → limited user-scope → prlimit launch without model credentials.

Gateway behavior preserves Host/Origin and backend path/status/body, keeps API/health outside SPA fallback, leaves missing build files as 404, and retains native SSE streaming without configured command retries. Serve only built public assets, never checkout or state. Services do not migrate, initialize keys or synthesize readiness; `init-state` remains new-directory-only. Data, credentials, worktrees and licenses remain untouched.

<a id="verification"></a>
## Verification

Actual sandbox checks: `node --check deploy/proxy.test.mjs`, workflow YAML parsing and newline/trailing-whitespace validation passed. Both revised user-unit definitions passed `systemd-analyze --user verify` (exit 0), using a temporary runtime directory and `/usr/bin/true` executable stand-in for path resolution. No service or manager was started in the sandbox; the temporary executable was removed. Native user-manager execution remains a CI/host check, not a claimed sandbox result.

Historical exact-commit result: on `82c02b8f6efd620fa85d24cef0f7106fd4fb9d2d`, [Personal hosting](https://github.com/zhengui666/QuaZonai/actions/runs/35169054480), job `105036524174`, ran actual Caddy 2.11.4: 8 passed, 0 failed/skipped. Cases cover HTML/HEAD, PWA/missing assets, raw request/Host/Origin/idempotency preservation, backend 404/503, observed single mutation dispatch, non-navigation methods, SSE before upstream completion and backend outage. This result used the earlier unit syntax and is not acceptance of later user-service revisions.

On `6335a2373233df643c748b8e40433042fa911849`, [Web run](https://github.com/zhengui666/QuaZonai/actions/runs/35169523541), job `105038887337`, had passed Rust API/OpenAPI, generated validators, TypeScript/unit checks, production static build and Chromium installation at the last observation; the browser stage was still running and real-API browser acceptance pending. Its [hosting run](https://github.com/zhengui666/QuaZonai/actions/runs/35169523518) passed. These historical results do not approve a later Head.

The routing peer is synthetic; user-manager probes use `/usr/bin/true`, not QZ/Codex/research. Unit syntax does not start QZ. Rendering fixtures do not execute research. Latest-Head browser/build, all applicable CI and independent review must be checked through PR #85; queued, skipped or unavailable results are not passes.

CodexPro direct workspace, wrapper, configuration and minimal workspace reads returned HTTP 502. GitHub reads/writes succeeded. No owner-host execution, real-account research, target delivery, restart or restore has been observed.

<a id="review"></a>
## Review

[PR #85](https://github.com/zhengui666/QuaZonai/pull/85) owns independent review and CI results. Codex returned no major issues for `6335a23732` in [comment 5706918345](https://github.com/zhengui666/QuaZonai/pull/85#issuecomment-5706918345). Subsequent user-service corrections require fresh review and validation. The reviewer remains read-only; no implementation delegation or author self-approval. Resolve findings and inspect the actual latest Head before merging.

<a id="delivery"></a>
## Delivery

Draft PR #85 exists on `codex/personal-production-20260917`. No merge, deployment or whole-product acceptance has occurred. The owner's full request and Issue #62 remain open; do not close them for this scoped slice.

<a id="handoff"></a>
## Handoff and remaining work

Reconcile actual PR Head, pending/failed CI and independent review first; fix defects rather than narrowing assertions. Complete the existing coverage index's real authorized research, independent evaluation, portfolio/target delivery and disposable restore/restart acceptance. Continue source/document cleanup after tracing callers and preserving contracts. Recover CodexPro access before claiming any owner-host execution. Never reset the dirty checkout or other worktrees, paste credentials into chat/logs, or replace missing real evidence with fixtures.
