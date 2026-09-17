# Personal production

<a id="task"></a>
## Task

**Task ID:** `personal-production`.

**Source:** the owner's 2026-09-17 request to make QuaZonai genuinely usable as a personal production project across code, architecture, documentation, deployment and UX, using CodexPro and OpenSDLC.

**Endpoint:** implemented behavior, real acceptance of the complete research/delivery/recovery workflow, latest-Head CI and explicit independent clean review, then authorized delivery. [PR #85](https://github.com/zhengui666/QuaZonai/pull/85) is an implementation slice, not completion of the whole request or [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

<a id="intent"></a>
## Intent

A personal installation needs a persistent real-service entry, understandable operations and recoverable failures. Keep the existing single-user, Rust, official Ant Design and target-only model. Clean product-facing development narration without deleting limitations, migration history, licenses, user data, Git history or acceptance evidence.

Reuse the existing [maintenance](../production-maintenance/task.md) and [onboarding](../open-source-foundation/task.md) work. [DESIGN](../../../DESIGN.md) remains the product contract; the [existing evidence index](../../../docs/architecture/issue-62-execution.md) owns whole-product coverage. No repository language override was present; this task and guide use OpenSDLC's English default while existing Chinese documents retain Chinese. `DEVELOPMENT.md` is absent at the inspected Head; [CONTRIBUTING](../../../CONTRIBUTING.md) is the current development entry.

<a id="spec"></a>
## Requirements and design

| Area | Implemented slice | Remaining whole-request responsibility |
| --- | --- | --- |
| Code | Consolidate rendering recovery at the real root, covering App hooks and descendants; require confirmation and avoid printing exception payloads | Audit and resolve remaining domain/Worker/runtime gaps through their actual call chains |
| Architecture | Reuse Caddy and systemd user services around existing API/Worker/PostgreSQL/PGMQ/Codex/Runtime owners | Complete the original end-to-end product contract and real integrations |
| Documentation | Prioritize real hosting, separate synthetic preview, add one hosting/recovery guide, remove product-facing development narration | Audit active manuals and historical annotations without losing contracts or evidence |
| Deployment | Native user units, readable private configuration, lingering user manager, same-origin gateway and native regressions | Real selected-host TLS, restart, persistence, backup and restore acceptance |
| UX | Generic error state, keyboard-operable reload confirmation, no replay/cancellation claim, root/descendant faults at three widths; truthful product-status footer | Full real research/evaluation/portfolio/target-delivery and mobile acceptance |
| Synthetic preview | In-memory Brief freeze, explicit project activation and profile selection, synthetic Cycle receipt, existing two-Alpha/portfolio/Release history and original-package download | This presentation is not native model execution, scientific qualification or production acceptance; T02 coverage requires its own evidence mapping |

A live process is not readiness. Empty integration allowlists or missing Codex bindings are not research readiness. QZ does not own broker credentials, orders, positions, NAV or downstream trading controls. Native domain state machines, generated contracts and qualification rules are unchanged. The preview does not connect to real accounts, a database or a Runtime and cannot approve or deliver its DEMO package.

<a id="plan"></a>
## Implementation and reuse

Inspected baseline: `0b9a47aa117175e9e204974b3b6942a427787fe1`. Sources inspected include AGENTS, DESIGN ownership/layout, current OpenSDLC records, README, architecture navigation, CONTRIBUTING, server CLI/router/shutdown/state creation, web main/App/Vite/PWA/Playwright, Codex native process resources, and existing Web/Store CI. Native shutdown is reused, not claimed as a new implementation.

Hosting/recovery paths: `apps/web/src/{App,AppErrorBoundary,main}.tsx`, `apps/web/tests/error-boundary.spec.ts`, `deploy/Caddyfile`, `deploy/systemd/quazonai-{api,worker}.service`, `deploy/quazonai.env.example`, `deploy/proxy.test.mjs`, `.github/workflows/deployment.yml`, `docs/user-guide.md`, README and this task.

The existing inner `RenderBoundary` is consolidated at the actual React root, not duplicated. Both root and nested-authentication fault fixtures exercise recovery; native React `onCaughtError` emits a fixed diagnostic only. The unchanged base and fault fixture preserve the pre-fix version; a runtime pre-fix browser failure was not recorded for that change.

The native Mission implementation requires a user manager, actual XDG runtime directory and scopes under `/user.slice/`. API/Worker are **user units**, installed in `/etc/systemd/user` and enabled only for `quazonai`, with linger for boot/logout independence. Configuration is administrator-owned `root:quazonai` 0640 so the unprivileged manager can read it. The guide uses native `systemctl --machine=quazonai@.host --user`; no shell wrapper or alternate process supervisor is added. CI reuses the Store workflow's user-manager preparation and exercises a user-service → limited user-scope → prlimit launch without model credentials.

Gateway behavior preserves Host/Origin and backend path/status/body, keeps API/health outside SPA fallback, leaves missing build files as 404, and retains native SSE streaming without configured command retries. Serve only built public assets, never checkout or state. Services do not migrate, initialize keys or synthesize readiness; `init-state` remains new-directory-only. Data, credentials, worktrees and licenses remain untouched.

Preview paths: `apps/web/demo/project-editor.ts`, `apps/web/src/demo-{editor,flow}.test.ts`, `apps/web/tests/demo-{complete,history}.spec.ts`, both Playwright configurations, `vite.demo.config.ts`, package/TypeScript configuration and `.github/workflows/web.yml`. The isolated complete-Demo configuration prevents its mutable scene from contaminating the existing three-viewport history suite.

### Continuation: exact-version and receipt regressions

The [field-level plan on Issue #62](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5708172724) records the inspected baseline `e278832e89c557208cc66a3f48408f50f14150d2` and repair boundaries.

`demo-complete.spec.ts` scopes Cycle startup to the row containing the newly authored hypothesis. Both frozen Brief versions remain visible; choosing the first global button would conceal a wrong-version launch. Response waiters are registered before clicks. Assertions bind the freeze response's `resource.brief.id` to the startup dialog, request `brief_id`, returned `cycle.brief_id`, project and `run.cycle_id`, then locate the exact returned Cycle row and require one startup request. Existing Alpha/portfolio/Release navigation is retained.

The Release drawer renders its own DEMO warning, not the package JSON's `limitations` array. The test checks the actual visible warning and disabled approval button, then uses the real browser download action to inspect the original package's Release/project identity and both synthetic/no-delivery limitations. Historical sample outputs are not represented as outputs generated by the newly created synthetic Cycle.

`project-editor.ts` reuses the existing `replay` helper before mutable Brief/project/runtime checks for freeze/start, after path, request-schema and key validation. This matches the existing native Store command ordering: an exact retry returns the original receipt; a changed path/body under the same operation/key returns `IDEMPOTENCY_CONFLICT`; a fresh request still undergoes all existing state/reference/revision guards. No native Rust or generated schema is edited. Seven focused `demo-flow.test.ts` regressions cover freeze replay, changed revision/context/path, Cycle replay after pause/archive without additional Cycles/Runs, and malformed requests/keys.

<a id="verification"></a>
## Verification

All results below belong to their stated commits; none approves a later Head.

Historical prior-session sandbox checks recorded `node --check deploy/proxy.test.mjs`, workflow YAML parsing and whitespace checks as passed. Revised user-unit definitions passed `systemd-analyze --user verify` with a temporary runtime directory and `/usr/bin/true` executable stand-in for path resolution. No QZ service or owner-host manager was started. These checks were not rerun locally in this continuation.

| Exact commit | Observed evidence | Boundary |
| --- | --- | --- |
| `82c02b8f6efd620fa85d24cef0f7106fd4fb9d2d` | [Personal hosting run 35169054480](https://github.com/zhengui666/QuaZonai/actions/runs/35169054480), job `105036524174`: actual Caddy 2.11.4, 8 passed, no failures/skips | Earlier unit syntax; synthetic routing peer, not product research |
| `6335a2373233df643c748b8e40433042fa911849` | [Hosting run 35169523518](https://github.com/zhengui666/QuaZonai/actions/runs/35169523518) passed; [Web run 35169523541](https://github.com/zhengui666/QuaZonai/actions/runs/35169523541) had reached browser checks at the earlier observation | Historical partial observation, not latest-Head approval |
| `e278832e89c557208cc66a3f48408f50f14150d2` | [Web run 35173462519](https://github.com/zhengui666/QuaZonai/actions/runs/35173462519), job `105050663835`: Rust/OpenAPI, client generation, TypeScript, 517 Vitest tests, 5 Node tests and static build passed; Demo failed on ambiguous Cycle button | Three-viewport/PWA and native browser acceptance were skipped, not passed; the old union-type defect was already fixed |
| `0cc9f5a51091f340ce3bd28cbe788fe50d6956ad` | [Web run 35179601047](https://github.com/zhengui666/QuaZonai/actions/runs/35179601047), job `105068709837`: the new freeze/start identity assertions and navigation reached Release details; failure was the package-only limitations string incorrectly expected in the DOM | Final Demo assertion failed; later acceptance stages skipped. This led to separate UI-warning and downloaded-package checks |
| `37308b87d728f4f6437267e3cbfdd66b97d624ab` | [Web run 35179717849](https://github.com/zhengui666/QuaZonai/actions/runs/35179717849) is the pre-repair revision containing seven new receipt tests | Its final result and the subsequent repair Head must be read from CI; test authorship alone is not execution evidence |

The routing peer is synthetic; user-manager probes use `/usr/bin/true`, not QZ/Codex/research. Unit syntax does not start QZ. Rendering fixtures and Demo receipts do not execute research. Require the actual final-Head Web contract/unit/build/Demo/three-viewport/PWA/native-browser stages, all other applicable CI and independent review. Queued, skipped, cancelled, absent or unavailable results are not passes. Do not weaken assertions or change timeouts to hide failures.

Prior-session CodexPro calls recorded HTTP 502. In this continuation, plugin search returned no QuaZonai CodexPro and tool discovery exposed no matching workspace actions; no current workspace or handoff was opened. The assistant authored changes through connected GitHub file/Git-object tools and used existing GitHub CI; no project shell or local Codex execution was invoked. No owner-host deployment, real-account research, target delivery, restart or restore was observed.

<a id="review"></a>
## Review

[PR #85](https://github.com/zhengui666/QuaZonai/pull/85) owns independent review and CI results. Codex returned no major issues for `6335a23732` in [comment 5706918345](https://github.com/zhengui666/QuaZonai/pull/85#issuecomment-5706918345); the review summary later names `933c545`. Neither covers the Demo/receipt continuation. Request a fresh review on the final published Head. The reviewer remains read-only; no implementation delegation or author self-approval.

<a id="delivery"></a>
## Delivery

PR #85 exists on `codex/personal-production-20260917`. Delivery requires: (1) a PR; (2) its declared scope completed, every applicable final-Head CI check passing, all findings resolved and explicit clean `@codex review`; (3) only then merge and verify main. **Never ask GitHub Codex to fix, implement, edit, commit or push.** An unreturned review or old result is not clean approval.

The PR is still draft at this record. No merge, owner-host deployment or whole-product acceptance is claimed. The owner's full request and Issue #62 remain open; do not close them for this scoped slice.

<a id="handoff"></a>
## Handoff and remaining work

Reconcile actual PR Head, pending/failed CI and independent review first; fix defects rather than narrowing assertions. Map T02's synthetic-only acceptance without presenting it as native model execution. Complete the existing coverage index's real authorized research, independent evaluation, portfolio/target delivery, legacy-snapshot migration and disposable restore/restart acceptance. Continue source/document cleanup after tracing callers and preserving contracts. Recover CodexPro access before claiming any owner-host execution. Never reset a dirty checkout or other worktrees, paste credentials into chat/logs, or replace missing real evidence with fixtures.
