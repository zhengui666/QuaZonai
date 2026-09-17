# Personal production

<a id="task"></a>
## Task

Task ID: `personal-production`. Source: the owner's 2026-09-17 request to turn QuaZonai into a genuinely usable personal project across code, architecture, documentation, deployment and UX, followed by the explicit goal to close [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

[PR #85](https://github.com/zhengui666/QuaZonai/pull/85) contains the current implementation slice. Its merge alone does not satisfy the complete W0–W8/T01–T42 contract. [DESIGN](../../../DESIGN.md) owns that contract; [the acceptance index](../../../docs/architecture/issue-62-execution.md#acceptance) owns remaining coverage. This task is the local continuation entry, not a duplicate of the PR's full activity log.

<a id="intent"></a>
## Intent

Provide a persistent real-service entry, recoverable UI failures and understandable installation/operations without rebuilding existing engines. Retain the single-user Rust/official Ant Design/target-only boundaries. Clean obsolete product-facing development narration and duplicate active records, not user data, credentials, licenses, migration history or verifiable evidence.

Reuse [maintenance](../production-maintenance/task.md) and [onboarding](../open-source-foundation/task.md). The existing task ID and English language remain unchanged. `DEVELOPMENT.md` is absent at the inspected revision; [CONTRIBUTING](../../../CONTRIBUTING.md) is the development entry.

<a id="spec"></a>
## Specification and ownership

| Area | Current implementation | Limit |
|---|---|---|
| Web root | One React error boundary, fixed diagnostic, keyboard-accessible confirmed reload | No replay/cancellation of submitted work; not network or script-download recovery |
| Hosting | Native Caddy routing and existing static headers; systemd user units with actual user manager/linger | No custom proxy, supervisor, migration-on-start or owner-host deployment claim |
| Installation | Checked source revision; new traversable release directory; first selection refuses existing paths | Build/copy is not activation, TLS or real research acceptance |
| Preview | Explicit synthetic Brief freeze/start with input bindings, revisions, CAS, receipt replay and UTC quota | No model or experiments; historical Alpha/portfolio/Release samples are not new Cycle outputs |
| Historical records | Original frozen contexts and disabled Runtime remain immutable/readable | New configurations do not retroactively grant admission or qualification |
| Documents | Real-hosting-first README, one user guide, canonical evidence index | Keep limitations and source contracts; put full execution history in GitHub, not parallel ledgers |

No native Rust domain, generated schema, database migration or scientific qualification is changed by this slice. QZ does not own broker credentials, real orders, positions, NAV or downstream trading controls.

<a id="plan"></a>
## Implementation and verification plan

Original baseline: `0b9a47aa117175e9e204974b3b6942a427787fe1`. Current continuation baseline: `bb943de1ca02a5d812329344cf43c2d222012899`. Check actual PR Head before publishing; preserve others' changes.

Implementation paths are `apps/web/src/{App,AppErrorBoundary,main}.tsx`, existing preview adapter and browser/unit suites, `deploy/`, `.github/workflows/deployment.yml`, README and `docs/user-guide.md`. Native shutdown, error contracts, platform primitives and generated validators are reused. The nested-auth fault test retains the real module's exports and overrides only the rendering target.

Detailed field-level plans and rationale are authoritative on Issue #62: [preview/CI repairs](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5709034237), [existing headers and authoring validation](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5709232741), [revisions/CAS/quota](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5709361547), and [release installation](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5711421608).

The release-installation correction targets review comments 4033677137, 4033677143 and 4033677147. The guide checks tracked/staged/untracked changes before building and HEAD/worktree after building; it refuses rather than deletes work. Explicit `mkdir -m 0755` preserves traversal under umask 077, and first selection uses non-overwriting `ln -sT`. `deploy/install.test.mjs` executes the guide snippets against real Git/GNU coreutils in disposable paths, reproduces the previous bad semantics and tests successful and rejected cases. Compilation and privilege elevation are explicitly substituted fixtures. Existing full native builds, Caddy, user-manager and browser verification remain required.

Use the original freeze/Cycle command receipts before mutable state, revision and quota checks. New preview Cycles remain non-qualifying with zero experiments, honest actions, unique ordinals and coherent deadlines. Empty bindings fail authoring; nonempty missing-purpose bindings fail freezing. Original contexts and static evidence never change. The remaining cooldown interpretation is recorded in [thread 4033522584](https://github.com/zhengui666/QuaZonai/pull/85#discussion_r4033522584); resolve against DESIGN and native manual/Wake paths, not by inventing a preview-only rule.

<a id="verification"></a>
## Verification evidence

Full commands, logs, review replies and per-turn execution summaries live in [PR #85](https://github.com/zhengui666/QuaZonai/pull/85). Preserve these concrete regression observations:

| Exact revision | Observed result |
|---|---|
| `37308b87d728f4f6437267e3cbfdd66b97d624ab` | [Web 35179717849](https://github.com/zhengui666/QuaZonai/actions/runs/35179717849), job 105069667017: TypeScript passed; **518 Vitest passed / 6 failed** because mutable state guards returned 422 before original receipt replay. |
| `cf11b7be1fad4f62270d777700dd33f3dae842ec` | [Web 35180454929](https://github.com/zhengui666/QuaZonai/actions/runs/35180454929): 524 Vitest, 5 Node and complete Demo passed; three viewports **414 passed / 6 failed** (old banner and incomplete auth-module fixture). Real-API browser phase skipped, not passed. |
| `b60b45fa3fd4346805296bdd84196f005e5ed568` | [Web 35185171980](https://github.com/zhengui666/QuaZonai/actions/runs/35185171980), job 105085579101: **530 passed / 1 failed of 531 Vitest**. The test incorrectly expected an empty-binding draft to be accepted; existing 422 behavior was correct. The test was reshaped without weakening validation. |
| `6808e394585ca0d063442dd4234f8d8559a06719` | [Web 35185968555](https://github.com/zhengui666/QuaZonai/actions/runs/35185968555) completed all stages successfully; [hosting 35185968521](https://github.com/zhengui666/QuaZonai/actions/runs/35185968521) passed 8 real-Caddy cases plus user-manager/unit prerequisites. |
| `bb943de1ca02a5d812329344cf43c2d222012899` | [CI 35187058702](https://github.com/zhengui666/QuaZonai/actions/runs/35187058702), [Web 35187058684](https://github.com/zhengui666/QuaZonai/actions/runs/35187058684), [Native Runtime 35187058727](https://github.com/zhengui666/QuaZonai/actions/runs/35187058727), [hosting 35187058675](https://github.com/zhengui666/QuaZonai/actions/runs/35187058675), [CodeQL 35187058678](https://github.com/zhengui666/QuaZonai/actions/runs/35187058678) all returned success. Subsequent review found the three installation-command defects above. |

These are historical exact-Head results, not approval of later changes. A prior nalgebra download HTTP 502 was a dependency transport failure; no pinned version or test was weakened. No execution of the new installation regression has been claimed at its authorship. Inspect its actual new-Head job and the full applicable CI before declaring success.

Preview records, controlled Caddy peers, syntax stand-ins and process probes do not prove native account inference, real market validity or a production installation. Full Web stages, including three viewports/PWA and real API/TOTP/idempotency/database acceptance, remain required. Missing, failed, cancelled, skipped or old-Head checks do not pass.

<a id="review"></a>
## Independent review

Inspect all summary and inline findings, not only the latest clean summary. Repairs require fresh read-only `@codex review` for the final Head. The original clean responses did not invalidate later findings. Resolve threads with their source and verification evidence; do not count author inspection as independent approval.

<a id="delivery"></a>
## Delivery boundary

1. Publish the PR and complete its declared scope.
2. Require every applicable final-Head CI check to pass, every actionable review finding to be resolved and explicit clean Codex feedback for that Head.
3. Only then mark ready, merge and verify main, recording the actual merge commit.
4. **Never ask GitHub Codex to fix, implement, edit, commit or push.** It is an independent read-only reviewer.

PR metadata owns the current draft/open/merged state; this file does not invent it. Issue #62 remains open until the full accepted contract is demonstrated, not merely this slice.

<a id="handoff"></a>
## Continuation

From the owner's latest instruction, every user turn is summarized in PR comments with its actual actions/results; [Turn 01 starts here](https://github.com/zhengui666/QuaZonai/pull/85#issuecomment-5711370866). Do not duplicate those logs into product documentation.

CodexPro directory search returned no plugin and workspace-tool discovery exposed no matching action in this continuation; earlier sessions recorded upstream 502s. GitHub reads/writes and CI are available. No owner-host workspace, local executor, deployment, real-account research, target delivery, migration or restore was observed. The web assistant authors changes; no coding is delegated to local or GitHub Codex, and no project shell is directly invoked.

After this PR, continue the canonical index's genuine T07/T08 account/model, T39 owner snapshot, T40 full restore/fault rehearsal, T41 runbook and T42 research-to-delivery acceptance. Reuse the real application/CLI/Runtime and existing test fixtures, but never substitute fixtures for the required external evidence. Preserve user work, state and credentials throughout.
