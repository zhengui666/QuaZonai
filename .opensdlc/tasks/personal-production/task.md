# Personal production

<a id="task"></a>
## Task

Task ID: `personal-production`. Source: the owner's 2026-09-17 request to turn QuaZonai into a genuinely usable personal project across code, architecture, documentation, deployment and UX, followed by the explicit goal to close [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

[PR #85](https://github.com/zhengui666/QuaZonai/pull/85) delivered the hosting, render-recovery and interactive-preview slice, merged as `ff8bee7f8bc4557d309b709edad6166dde1c03b3`. Its merge alone does not satisfy W0–W8/T01–T42. [DESIGN](../../../DESIGN.md) owns the contract; [the acceptance index](../../../docs/architecture/issue-62-execution.md#acceptance) owns remaining coverage. This task is the continuation entry, not a duplicate of the PR activity logs.

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
| Native recovery | Explicit cold-restore target using the original Runtime/Docker fixture and GNU tar | New test must actually run; same-host terminal checkpoint is not complete T40 or production RPO/RTO |

No native domain behavior, generated schema, database migration or scientific qualification changes in these slices. QZ does not own broker credentials, real orders, positions, NAV or downstream trading controls.

<a id="plan"></a>
## Implementation and verification plan

Original baseline: `0b9a47aa117175e9e204974b3b6942a427787fe1`. Current continuation baseline: merged main `ff8bee7f8bc4557d309b709edad6166dde1c03b3`. The next branch is `codex/runtime-cold-restore-20260917`; check its actual Head and delivery PR before publishing. Preserve others' changes.

PR #85 paths are `apps/web/src/{App,AppErrorBoundary,main}.tsx`, the existing preview adapter and browser/unit suites, `deploy/`, `.github/workflows/deployment.yml`, README and `docs/user-guide.md`. Native shutdown, error contracts, platform primitives and generated validators are reused. The nested-auth fault test retains the real module's exports and overrides only the rendering target.

Field-level plans and rationale remain on Issue #62: [preview/CI repairs](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5709034237), [existing headers and authoring validation](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5709232741), [revisions/CAS/quota](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5709361547), and [release installation](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5711421608).

The release-installation correction targets review comments 4033677137, 4033677143 and 4033677147. The guide checks tracked/staged/untracked changes before building and HEAD/worktree after building; it refuses rather than deletes work. Explicit `mkdir -m 0755` preserves traversal under umask 077, and first selection uses non-overwriting `ln -sT`. `deploy/install.test.mjs` executes guide snippets against real Git/GNU coreutils in disposable paths, reproduces the previous bad semantics and tests success/rejection. Compilation and privilege elevation are explicitly substituted fixtures. Full native builds, Caddy, user-manager and browser verification remain separate requirements.

The preview reuses original receipts before mutable state, revision and quota checks. New Cycles remain non-qualifying with zero experiments, honest actions, unique ordinals and coherent deadlines. Empty bindings fail authoring; nonempty missing-purpose bindings fail freezing. Original contexts and static evidence never change. The manual/Wake cooldown interpretation and its disposition remain in [thread 4033522584](https://github.com/zhengui666/QuaZonai/pull/85#discussion_r4033522584); do not invent a preview-only rule or imply a waiver of automatic-Wake cooldown.

### Native Runtime cold restoration

The [Turn 02 plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5712651287) extends existing real recovery evidence, rather than claiming database backup is absent. `recovery_access.rs` already executes PostgreSQL/archive/fresh-server recovery, and `support/graph_recovery.rs` preserves full relationships and original artifact bytes.

The new `apps/runtime/tests/native_restore.rs` reuses `support/oci.rs`, real Docker, the reviewed native job image, production Runtime HTTP and GNU tar. It compiles Rust to Wasm, records original output bytes and a pre-submit cancellation, stops the sole writer after work terminates, archives the whole SQLite/WAL directory and retains the original tree. A fresh Runtime opens the extracted copy at the same original path. The test checks exact historical status/manifest/bytes, conflict/replay, cancellation and unchanged container start/identity, then compiles again from restored inputs without reupload. Native instance identity must persist and the retained original must remain unchanged.

`apps/runtime/Cargo.toml` explicitly requires `native-oci` for the new target. The existing Native Runtime workflow builds and lints it, then runs it after the unchanged OCI suite; only the new target uses nocapture to retain its sanitized checkpoint measurements. No new dependency, image builder, backup framework, hash registry or recovery service is introduced. [The runbook](../../../docs/runtime-recovery.md) states the same-host, same-path, no-active-writers prerequisite and separates external configuration/credentials and coordinated control-plane recovery.

This is additional acceptance coverage, not an already reproduced production defect. No pre-repair failure or execution of the new target is asserted at authorship. Actual format, Clippy, execution, CI and review results belong in the delivery PR. Fix observed failures without weakening the test or claiming the owner's host ran it.

<a id="verification"></a>
## Verification evidence

Full commands, logs, review replies and per-turn execution summaries live in [PR #85](https://github.com/zhengui666/QuaZonai/pull/85) and its linked continuation PRs. Preserve these concrete regression observations:

| Exact revision | Observed result |
|---|---|
| `37308b87d728f4f6437267e3cbfdd66b97d624ab` | [Web 35179717849](https://github.com/zhengui666/QuaZonai/actions/runs/35179717849), job 105069667017: TypeScript passed; **518 Vitest passed / 6 failed** because mutable state guards returned 422 before original receipt replay. |
| `cf11b7be1fad4f62270d777700dd33f3dae842ec` | [Web 35180454929](https://github.com/zhengui666/QuaZonai/actions/runs/35180454929): 524 Vitest, 5 Node and complete Demo passed; three viewports **414 passed / 6 failed** (old banner and incomplete auth-module fixture). Real-API browser phase skipped, not passed. |
| `b60b45fa3fd4346805296bdd84196f005e5ed568` | [Web 35185171980](https://github.com/zhengui666/QuaZonai/actions/runs/35185171980), job 105085579101: **530 passed / 1 failed of 531 Vitest**. The test incorrectly expected an empty-binding draft to be accepted; existing 422 behavior was correct. The test was reshaped without weakening validation. |
| `6808e394585ca0d063442dd4234f8d8559a06719` | [Web 35185968555](https://github.com/zhengui666/QuaZonai/actions/runs/35185968555) completed all stages successfully; [hosting 35185968521](https://github.com/zhengui666/QuaZonai/actions/runs/35185968521) passed 8 real-Caddy cases plus user-manager/unit prerequisites. |
| `bb943de1ca02a5d812329344cf43c2d222012899` | [CI 35187058702](https://github.com/zhengui666/QuaZonai/actions/runs/35187058702), [Web 35187058684](https://github.com/zhengui666/QuaZonai/actions/runs/35187058684), [Native Runtime 35187058727](https://github.com/zhengui666/QuaZonai/actions/runs/35187058727), [hosting 35187058675](https://github.com/zhengui666/QuaZonai/actions/runs/35187058675), [CodeQL 35187058678](https://github.com/zhengui666/QuaZonai/actions/runs/35187058678) all returned success. Later review found the installation-command defects above. |
| `ff8bee7f8bc4557d309b709edad6166dde1c03b3` | After PR #85 merged its reviewed `fbacfc6379` Head, main [CI 35205505863](https://github.com/zhengui666/QuaZonai/actions/runs/35205505863), [Web 35205505883](https://github.com/zhengui666/QuaZonai/actions/runs/35205505883), [Native Runtime 35205505910](https://github.com/zhengui666/QuaZonai/actions/runs/35205505910), [hosting 35205505881](https://github.com/zhengui666/QuaZonai/actions/runs/35205505881), and [CodeQL 35205505868](https://github.com/zhengui666/QuaZonai/actions/runs/35205505868) returned success. |

These exact-Head results do not approve later additions. A previous nalgebra HTTP 502 was a dependency transport failure; no pinned version or test was weakened. The new cold-restore target needs its own actual execution and final-Head checks.

Preview records, controlled peers, syntax stand-ins and process probes do not prove native account inference, market validity or a production installation. Full Web stages, including three viewports/PWA and real API/TOTP/idempotency/database acceptance, remain required. Missing, failed, cancelled, skipped or old-Head checks do not pass.

<a id="review"></a>
## Independent review

Inspect all summary and inline findings, not only the latest clean summary. Repairs require fresh read-only `@codex review` for the final Head. The original clean responses did not invalidate later findings. Resolve threads with their source and verification evidence; do not count author inspection as independent approval.

<a id="delivery"></a>
## Delivery boundary

1. Publish the PR and complete its declared scope.
2. Require every applicable final-Head CI check to pass, every actionable review finding to be resolved and explicit clean Codex feedback for that Head.
3. Only then mark ready, merge and verify main, recording the actual merge commit.
4. **Never ask GitHub Codex to fix, implement, edit, commit or push.** It is an independent read-only reviewer.

PR metadata owns draft/open/merged state. Issue #62 remains open until the complete accepted contract is demonstrated, not merely individual maintenance or recovery slices.

<a id="handoff"></a>
## Continuation

Each user turn is summarized in PR comments with actual actions/results; [Turn 01](https://github.com/zhengui666/QuaZonai/pull/85#issuecomment-5711370866) and [Turn 02](https://github.com/zhengui666/QuaZonai/pull/85#issuecomment-5712464409) anchor the continuation. Link the next PR there; do not copy complete activity logs into product documents.

CodexPro plugin search and workspace discovery still returned no matching action in this continuation; earlier sessions recorded HTTP 502. GitHub reads/writes and CI are available. No owner-host workspace, local executor, deployment, real-account research, target delivery, migration or restore was observed. The web assistant authors changes; coding is not delegated and project shell is not directly invoked.

Continue genuine T07/T08 account/model, T39 owner snapshot, T40 coordinated restore/fault rehearsal, T41 runbooks and T42 research-to-delivery acceptance. Native cold-restore CI is only the stated Runtime checkpoint case. Never substitute fixtures for required external evidence, or remove user work/state/credentials to manufacture a clean setup.
