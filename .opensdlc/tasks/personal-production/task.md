# Personal production

<a id="task"></a>
## Task

**Task ID:** `personal-production`.

**Source:** the owner's 2026-09-17 request to make the demo genuinely usable as a personal production project across code, architecture, documentation, deployment and UX, using CodexPro and OpenSDLC.

**Endpoint:** implemented behavior, actual acceptance of the complete research/delivery/recovery workflow, latest-Head CI and explicit independent clean review, then authorized delivery. [PR #85](https://github.com/zhengui666/QuaZonai/pull/85) is the first implementation slice, not completion of the whole request or [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

<a id="intent"></a>
## Intent

A personal installation needs a persistent real-service entry, understandable operations and recoverable failures. Keep the existing single-user, Rust, official Ant Design and target-only model. Clean product-facing development narration without deleting limitations, migration history, licenses, user data, Git history or acceptance evidence.

Reuse the existing [maintenance](../production-maintenance/task.md) and [onboarding](../open-source-foundation/task.md) work. [DESIGN](../../../DESIGN.md) remains the product contract; the [existing evidence index](../../../docs/architecture/issue-62-execution.md) owns whole-product coverage. The repository has no language override; this new task and guide use OpenSDLC's English default while existing Chinese documents retain Chinese.

<a id="spec"></a>
## Requirements and design

| Area | Implemented slice | Remaining whole-request responsibility |
| --- | --- | --- |
| Code | Consolidate the existing rendering boundary at the real root, covering App's own hooks and descendants; require confirmed recovery and avoid printing exception payloads | Audit and resolve remaining domain/Worker/runtime gaps through their actual call chains, not an unverified rewrite |
| Architecture | Reuse Caddy and systemd around the existing API/Worker/PostgreSQL/PGMQ/Codex/Runtime owners | Complete the original end-to-end product contract and real integration acceptance |
| Documentation | Prioritize real hosting, separate synthetic preview, add one hosting/recovery guide, remove product-facing research/development narration | Audit oversized active manuals and historical annotations without losing fields, contracts or real evidence |
| Deployment | Native API/Worker units, private environment example, exclusive same-origin gateway and actual gateway regressions | Real selected-host TLS, process restart, persistent data, backup and restore acceptance |
| UX | Generic error state, keyboard-operable reload confirmation, no replay/cancellation claim, root and descendant browser fault cases at three widths; remove Issue-number narration from the footer while retaining development status | Full real research/evaluation/portfolio/target-delivery and mobile acceptance |

A live process is not readiness. An empty integration allowlist or missing Codex binding is not research readiness. QZ does not own broker credentials, orders, positions, NAV or downstream trading controls. No domain state-machine, generated-contract or qualification change is introduced by this packaging.

<a id="plan"></a>
## Implementation and reuse

Inspected baseline: `0b9a47aa117175e9e204974b3b6942a427787fe1`. Read AGENTS, DESIGN ownership/layout, current OpenSDLC records, README, architecture navigation, CONTRIBUTING, server CLI/router/shutdown/state creation, web main/App/Vite/PWA/Playwright and existing Web CI. Native shutdown already exists and is reused.

`App.tsx` already had an inner `RenderBoundary`; reading only `main.tsx` initially missed it. Remove that duplicate and move its responsibility to `AppErrorBoundary` around the actual root, rather than keep two competing recovery behaviors. Add both root and nested-authentication fault cases; React's native `onCaughtError` emits only a fixed diagnostic, never the exception body. The unchanged base commit and fault fixture preserve the pre-fix version, but a runtime pre-fix failure has not been observed in this session.

Paths: `apps/web/src/{App,AppErrorBoundary,main}.tsx`, `apps/web/tests/error-boundary.spec.ts`, `deploy/Caddyfile`, `deploy/systemd/quazonai-{api,worker}.service`, `deploy/quazonai.env.example`, `deploy/proxy.test.mjs`, `.github/workflows/deployment.yml`, `docs/user-guide.md`, README and this task.

Gateway decisions: preserve original Host/Origin and backend path/status/body, keep API/health routes out of SPA fallback, leave missing build files as 404, retain native SSE streaming and no configured command retries. Serve only built public assets, never the checkout or state. systemd runs native foreground commands and does not migrate, initialize keys or synthesize readiness. `init-state` remains a new-directory-only operation. Existing data, credentials, worktrees and licenses are untouched.

<a id="verification"></a>
## Verification

Actually executed before the first implementation publication:

- `node --check deploy/proxy.test.mjs`: exit 0 in the current sandbox.
- Workflow YAML parsing and newline/trailing-whitespace checks: passed for the 11 initial implementation files.
- Native `systemd-analyze verify` on both actual unit files: exit 0, using a temporary `/usr/bin/true` executable stand-in only for syntax resolution. No service started; the stand-in was removed.

On implementation commit `82c02b8f6efd620fa85d24cef0f7106fd4fb9d2d`, the [Personal hosting run](https://github.com/zhengui666/QuaZonai/actions/runs/35169054480) passed. Its job `105036524174` executed actual Caddy 2.11.4: 8 tests passed, 0 failed/skipped, covering HTML/HEAD, PWA/missing assets, raw request and Host/Origin/idempotency preservation, backend 404/503, no observed mutation replay, non-navigation methods, first SSE event while upstream stayed open, and backend outage. Native systemd parsing and unchanged-source checks passed there too. This is a historical result for that exact commit, not automatic approval of later Heads.

The routing peer is synthetic, not a database or scientific result. systemd syntax is not a started QZ service. Rendering fixtures do not execute research. Browser/TypeScript/build, full applicable repository CI, the subsequent root/descendant consolidation and independent review must be checked against the actual latest PR Head. They are pending unless the linked native results explicitly show otherwise.

CodexPro direct workspace, stable wrapper, configuration and later minimal workspace reads returned HTTP 502. GitHub reads and writes succeeded. No owner-host execution, credentials, real-account research, live delivery, restart or restore has been observed. Do not call a skipped/unavailable prerequisite a pass.

<a id="review"></a>
## Review

[PR #85 review](https://github.com/zhengui666/QuaZonai/pull/85) is authoritative. GitHub Codex was requested read-only for `82c02b8`; it was running at the last observation before consolidation. A new Head requires new review. Do not count author inspection, an old-Head result or a queued review as independent approval. Resolve actionable findings and rerun affected validation before any merge.

<a id="delivery"></a>
## Delivery

Draft PR #85 exists on `codex/personal-production-20260917`. No merge, deployment or whole-product acceptance has occurred. The owner's full request and Issue #62 remain open. Do not close them for this scoped slice.

<a id="handoff"></a>
## Handoff and next work

Reconcile actual PR Head, pending/failed CI and independent review first; fix observed defects rather than narrowing assertions. Then use the existing whole-product coverage index to complete real authorized research, independent evaluation, portfolio/target delivery and disposable restore/restart acceptance. Continue source/document cleanup only after tracing affected callers and preserving their contracts. Recover CodexPro access before claiming any execution on the owner's machine. Never reset the original dirty checkout or another worktree, paste credentials into chat/logs, or replace missing real evidence with fixtures.
