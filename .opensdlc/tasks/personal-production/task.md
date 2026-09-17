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
| Synthetic preview | Matching-input Brief freeze, explicit activation/profile selection, non-qualifying synthetic Cycle/Run, separate two-Alpha/portfolio/Release history and original-package download | Presentation is not native inference, scientific qualification or production acceptance; final-Head evidence is required for T02 |

A live process is not readiness. Empty integration allowlists or missing Codex bindings are not research readiness. QZ does not own broker credentials, orders, positions, NAV or downstream trading controls. Native domain state machines, generated contracts and qualification rules are unchanged. The preview does not connect to real accounts, a database or a Runtime and cannot approve or deliver its DEMO package.

<a id="plan"></a>
## Implementation and reuse

Inspected baseline: `0b9a47aa117175e9e204974b3b6942a427787fe1`. Sources inspected include AGENTS, DESIGN ownership/layout, current OpenSDLC records, README, architecture navigation, CONTRIBUTING, server CLI/router/shutdown/state creation, web main/App/Vite/PWA/Playwright, Codex native process resources, and existing Web/Store CI. Native shutdown is reused, not claimed as a new implementation.

Hosting/recovery paths: `apps/web/src/{App,AppErrorBoundary,main}.tsx`, `apps/web/tests/error-boundary.spec.ts`, `deploy/Caddyfile`, `deploy/systemd/quazonai-{api,worker}.service`, `deploy/quazonai.env.example`, `deploy/proxy.test.mjs`, `.github/workflows/deployment.yml`, `docs/user-guide.md`, README and this task.

The existing inner `RenderBoundary` is consolidated at the actual React root, not duplicated. Root and nested-authentication fault fixtures exercise recovery; native React `onCaughtError` emits a fixed diagnostic only. The original boundary change did not have a recorded runtime pre-fix failure. Later failures in the nested test exposed an incomplete auth module replacement: App's other imports also need uuidPattern and other native exports. The test now forwards all native exports through a dedicated query that bypasses interception, overriding only AuthBoundary. There is no new production fault switch, and all recovery/cancellation/no-mutation/no-private-output assertions remain.

The native Mission implementation requires a user manager, actual XDG runtime directory and scopes under `/user.slice/`. API/Worker are **user units**, installed in `/etc/systemd/user` and enabled only for `quazonai`, with linger for boot/logout independence. Configuration is administrator-owned `root:quazonai` 0640 so the unprivileged manager can read it. The guide uses native `systemctl --machine=quazonai@.host --user`; no shell wrapper or alternate process supervisor is added. CI reuses the Store workflow's user-manager preparation and exercises a user-service → limited user-scope → prlimit launch without model credentials.

Gateway behavior preserves Host/Origin and backend path/status/body, keeps API/health outside SPA fallback, leaves missing build files as 404, and retains native SSE streaming without configured command retries. Serve only built public assets, never checkout or state. Services do not migrate, initialize keys or synthesize readiness; `init-state` remains new-directory-only. Data, credentials, worktrees and licenses remain untouched.

Preview paths: `apps/web/demo/project-editor.ts`, `apps/web/src/demo-{editor,flow}.test.ts`, `apps/web/tests/demo-{complete,history,preview}.spec.ts`, both Playwright configurations, `vite.demo.config.ts`, package/TypeScript configuration and `.github/workflows/web.yml`. The isolated complete-Demo configuration prevents its mutable scene from contaminating the existing three-viewport history suite.

### Exact-version, receipt and consistency repairs

The initial [field-level plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5708172724) records the `e278832...` baseline. The [review/CI continuation plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5709034237) records the findings on `cf11b7be1fad4f62270d777700dd33f3dae842ec` and the actual failed browser checks.

`demo-complete.spec.ts` scopes startup to the newly authored Brief row, not the first global button. Response waiters precede clicks. Assertions bind the freeze response's Brief ID to the startup dialog, request and returned Cycle, project and Run, and require one startup request. It opens the exact new Run and the separate historical Cycle's selection, retaining Alpha/portfolio/Release navigation. Normal-flow 4xx/5xx responses are failures.

The Release drawer renders its DEMO warning, not Package JSON limitations. The test checks that warning and disabled approval button, then uses the real browser download action to inspect the original Package's identity and synthetic/no-delivery limitations. Historical samples are not outputs generated by a new Cycle.

The existing `replay` helper precedes mutable freeze/start state checks, after path/schema/key validation, following the native Store's command ordering. An exact retry returns its original receipt; changed content/path under the same operation/key conflicts; fresh requests retain validation. Freeze also requires exact nonempty dataset sets for DISCOVERY/VALIDATION/SEALED from the selected InputSet members and Brief bindings. Failed freezes change neither draft nor project and publish no receipt. No Rust or generated schema is edited.

The Cycle Map includes the original history, lists it once and allocates max ordinal plus one. A newly simulated Cycle returns `NO_SUPPORTED_CANDIDATE`, zero experiments and only `VIEW_BRIEF`/`VIEW_RUNS`; it cannot advertise a nonexistent selection. Its Run deadline derives from its queued time and frozen wall budget. The static selection and original records remain unchanged.

The earlier proposed rebinding of the original Brief was rejected after reading `records.ts`: its context already exists and points to historical Runtime 220/revision 1, now disabled at revision 2. It remains readable but does not inherit Runtime 2980. The quickstart explicitly forks a new Brief for the interactive path. Runtime lists retain both records, reuse existing pagination and report the new configuration as `NOT_CHECKED` with no available jobs or probe. Unit regressions cover receipt reuse, exact bindings, no side effects, unique ordinals, deadlines, historical identity and honest readiness. The banner and README explain these same boundaries.

### Static headers and draft-validation correction

The [additional plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5709232741) follows inspection of both summary comments and actual inline threads. The original static response policy in `apps/web/Caddyfile` was missing from the new deployment entry. A native snippet now applies its same five header values to existing asset/navigation/fallback handlers, without altering API/health headers or adding a new security mechanism. The existing real-Caddy suite compares actual static response values against the established file, covers GET/HEAD/PWA/plain files/missing assets and verifies that a distinct upstream CSP/cache policy passes through unchanged. Existing status/body/single-dispatch/SSE checks remain.

The new ALL-bindings regression originally expected an empty draft to save; this was a test-authoring error, not a production bug. `bindingListError` already requires 1–64 bindings. The corrected test explicitly rejects empty POST/PATCH drafts and checks unchanged project, Brief and list state. Three separate nonempty missing-role cases still prove that freeze fails without side effects and succeeds after restoring matching bindings. Neither the schema nor production validation is loosened; README distinguishes these stages.

<a id="verification"></a>
## Verification

All results below belong to their stated commits; none approves a later Head.

Historical prior-session sandbox checks recorded `node --check deploy/proxy.test.mjs`, workflow YAML parsing and whitespace checks as passed. Revised user-unit definitions passed `systemd-analyze --user verify` with a temporary runtime directory and `/usr/bin/true` executable stand-in. No QZ service or owner-host manager was started. Those checks were not rerun locally in this continuation.

| Exact commit | Observed evidence | Boundary |
| --- | --- | --- |
| `82c02b8f6efd620fa85d24cef0f7106fd4fb9d2d` | [Hosting 35169054480](https://github.com/zhengui666/QuaZonai/actions/runs/35169054480), job `105036524174`: actual Caddy 2.11.4, 8 passed, none failed/skipped | Earlier unit syntax, synthetic routing peer |
| `6335a2373233df643c748b8e40433042fa911849` | [Hosting 35169523518](https://github.com/zhengui666/QuaZonai/actions/runs/35169523518) passed; [Web 35169523541](https://github.com/zhengui666/QuaZonai/actions/runs/35169523541) had reached browser checks at that observation | Historical partial observation |
| `e278832e89c557208cc66a3f48408f50f14150d2` | [Web 35173462519](https://github.com/zhengui666/QuaZonai/actions/runs/35173462519), job `105050663835`: Rust/OpenAPI, generation, types, 517 Vitest, 5 Node tests and build passed; Demo failed on ambiguous Cycle button | Later browser stages skipped |
| `0cc9f5a51091f340ce3bd28cbe788fe50d6956ad` | [Web 35179601047](https://github.com/zhengui666/QuaZonai/actions/runs/35179601047), job `105068709837`: exact-version startup reached Release details; package-only text was incorrectly expected in DOM | Demo failed; later stages skipped |
| `37308b87d728f4f6437267e3cbfdd66b97d624ab` | [Web 35179717849](https://github.com/zhengui666/QuaZonai/actions/runs/35179717849), job `105069667017`: TypeScript passed, 518 Vitest passed and six new receipt tests failed with 422 before replay | Recorded pre-repair regression |
| `cf11b7be1fad4f62270d777700dd33f3dae842ec` | [Web 35180454929](https://github.com/zhengui666/QuaZonai/actions/runs/35180454929), job `105071255371`: types, 524 Vitest, 5 Node tests, build and complete Demo passed; three viewports 414 passed / 6 failed | Outdated banner and incomplete nested-auth fault fixture each failed at three widths; real-API acceptance skipped. Independent review also identified consistency/documentation and static-header defects |
| `cf11b7be1fad4f62270d777700dd33f3dae842ec` | [Native Runtime 35180454966](https://github.com/zhengui666/QuaZonai/actions/runs/35180454966), job `105072256454`: nalgebra 0.33.3 download failed with repeated HTTP 502 | Dependency transport failure; do not downgrade or remove checks. CI 35180454722, CodeQL 35180454681 and hosting 35180454817 succeeded for this Head only |
| `b60b45fa3fd4346805296bdd84196f005e5ed568` | [Web 35185171980](https://github.com/zhengui666/QuaZonai/actions/runs/35185171980), job `105085579101`: Rust/OpenAPI, generation and TypeScript passed; **530 passed / 1 failed of 531 Vitest tests** | Empty-bindings test incorrectly expected 201 instead of the existing 422 draft rejection. Other 13 flow cases passed; subsequent Node/build/browser stages skipped. This led to the test correction above |

Require the final published Head's full Web contract/unit/build/Demo/three-viewport/PWA/native-browser checks and every other applicable CI workflow. Read actual failures and fix the source or test fault; do not weaken assertions, increase timeouts to hide defects, or count queued/skipped/cancelled/absent checks as passes. Final verification and review evidence are recorded in PR #85 without treating historical green results as approval.

The routing peer is synthetic; user-manager probes use `/usr/bin/true`, not QZ/Codex/research. Unit syntax does not start QZ. Rendering fixtures and Demo receipts do not execute research. Earlier CodexPro calls recorded HTTP 502; current plugin search and tool discovery exposed no matching workspace actions. No workspace/handoff was opened in this continuation. The assistant authored through GitHub file/Git-object tools and used existing GitHub CI; no project shell or local Codex execution was invoked. No owner-host deployment, real-account research, target delivery, restart or restore was observed.

<a id="review"></a>
## Review

[PR #85](https://github.com/zhengui666/QuaZonai/pull/85) owns independent review and CI results. The prior clean response on `6335a23732` and review of `933c545` do not cover the Demo changes. Inspect the union of summary findings and all inline threads, including the static-header and pre-repair evidence findings. Source/docs repairs above require fresh final-Head review, not author self-approval. GitHub Codex remains read-only.

<a id="delivery"></a>
## Delivery

PR #85 exists on `codex/personal-production-20260917`. Delivery requires: (1) a PR; (2) its scope completed, every applicable final-Head CI check passing, all findings resolved and explicit clean `@codex review`; (3) only then merge and verify main. **Never ask GitHub Codex to fix, implement, edit, commit or push.** An unreturned review or old result is not clean approval.

PR metadata, checks, review threads and the eventual merge commit are the delivery evidence. This task does not assert a merge, owner-host deployment or whole-product acceptance. The owner's full request and Issue #62 remain open; do not close them for this scoped slice.

<a id="handoff"></a>
## Handoff and remaining work

Reconcile actual PR Head, CI and independent review first; repair rather than narrow assertions. Use the updated T02 executable/evidence map without treating preview interactions as native model execution. Complete the coverage index's real authorized research, independent evaluation, portfolio/target delivery, legacy-snapshot migration and disposable restore/restart acceptance. Continue source/manual cleanup after tracing callers and preserving contracts. Recover CodexPro access before claiming owner-host execution. Never reset a dirty checkout or other worktrees, expose credentials or substitute fixtures for missing real evidence.
