# Personal production

<a id="task"></a>
## Task

Task ID: `personal-production`. Source: the owner's 2026-09-17 request to make QuaZonai a genuinely usable personal project across code, architecture, documentation, deployment and UX, followed by the explicit goal to complete [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

[DESIGN](../../../DESIGN.md) owns W0–W8/T01–T42. [The acceptance index](../../../docs/architecture/issue-62-execution.md#acceptance) owns remaining coverage. This task is a continuation entry, not a duplicate of the PR activity logs or a production-readiness certificate.

<a id="intent"></a>
## Intent

Provide a persistent real-service entry, recoverable failures and understandable operations without rebuilding existing engines. Retain single-user Rust/official Ant Design/target-only ownership. Remove obsolete product-facing narration and duplicate active records, not user data, credentials, licenses, migration history or verifiable evidence.

Reuse [maintenance](../production-maintenance/task.md) and [onboarding](../open-source-foundation/task.md). The task ID and English language remain unchanged. `DEVELOPMENT.md` is absent at the inspected revision; [CONTRIBUTING](../../../CONTRIBUTING.md) is the development entry.

<a id="spec"></a>
## Scope and ownership

| Area | Delivered or proposed scope | Boundary |
|---|---|---|
| Web root | One React boundary, fixed diagnostic and confirmed keyboard-accessible reload | No replay/cancellation of submitted work; not network/script-download recovery |
| Hosting | Native Caddy with the existing static header policy; systemd user units and actual user manager/linger | No custom proxy, supervisor, migration-on-start or owner-host deployment claim |
| Installation | Checked revision, traversable new release directory, refusal to overwrite an existing first selection | Build/copy is not TLS activation or real research acceptance |
| Preview | Explicit synthetic freeze/start with input matching, revisions, CAS, receipt replay and UTC quota | No model/experiments; historical Alpha/portfolio/Release samples are not new Cycle outputs |
| Runtime restoration | Original Runtime/Docker fixture, stopped complete SQLite/WAL archive and fresh process | Isolated same-host/same-path terminal checkpoint, not complete coordinated T40 or production RPO/RTO |
| SQLite reliability | One SQLx0.9 graph, fixed bundled SQLite and official compatible session adapter | Preventive reliability, not evidence of existing corruption or a new persistence/session framework |

QZ does not own broker credentials, real orders, positions, NAV or downstream trading controls. Original frozen contexts and evidence remain immutable. Dependency upgrades do not grant research qualification.

<a id="plan"></a>
## Implementation and reuse

[PR #85](https://github.com/zhengui666/QuaZonai/pull/85) delivered hosting, root recovery and interactive preview, merged as `ff8bee7f8bc4557d309b709edad6166dde1c03b3`. [PR #86](https://github.com/zhengui666/QuaZonai/pull/86) delivered the native cold-restore case, merged as `45033f46f87c7b12a85fd42292f5c164a4578468`. [PR #87](https://github.com/zhengui666/QuaZonai/pull/87) delivered SQLite/SQLx compatibility and the executed Mission stack repair, merged as `e426fc3e0a3f2c784609d004533c9faab7440b54`. The hosting/restart continuation uses that main baseline and branch `codex/native-hosted-restart-20260917`. Reconcile actual Head before publishing and preserve unrelated work.

The hosting implementation reuses existing shutdown, generated contracts, browser suites and platform services. The nested rendering fixture preserves all original auth exports and replaces only AuthBoundary. The preview retains honest non-qualifying results, immutable history and replay before mutable-state guards. The manual/Wake cooldown interpretation remains in [thread 4033522584](https://github.com/zhengui666/QuaZonai/pull/85#discussion_r4033522584); no preview-only rule or automatic-Wake waiver was introduced.

PR #86's `apps/runtime/tests/native_restore.rs` reuses the native OCI fixture, actual Rust→Wasm output and GNU tar. It stops the writer after terminal work, archives SQLite/WAL, retains the original and restores a copy at the original path. A new Runtime verifies identity/status/manifest/bytes, cancellation/conflict/replay and unchanged container/start identity, then compiles from restored inputs without reupload. The existing workflow and [runbook](../../../docs/runtime-recovery.md) retain this narrow scope; PostgreSQL/access/graph recovery is independent.

### SQLite and SQLx compatibility

[The dependency plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5713291004) follows Cargo's actual rejection of mixed native SQLite bindings. Runtime pins `libsqlite3-sys =0.37.0`, carrying SQLite3.51.3. Existing SQLx consumers use workspace `=0.9.0`. Reuse tower-sessions0.15.0 and the official SQLx adapter at exact upstream revision `d18c9bf76f1d4fb73130dbe5aa643197f14b5d2d`, with Time0.3.47. This is a fixed upstream Git revision, not a published compatible crate release.

Compiler-driven source changes use native `AssertSqlSafe` only at existing dynamic SQL call sites whose fragments are constants, closed choices or already-quoted PostgreSQL/projection identifiers. SQL text, value binds, authority, transaction/lock order and schema remain unchanged. It is an assertion, not a sanitizer; there is no blanket conversion for arbitrary strings. Static SQL stays static. Borrowed Strings use `as_str()`, and two native preparation tests use public `SqlStr::from_static`. Migration remains under the original outer transaction/lock using `run_direct(None, connection, false)` to execute all pending migrations, not skip them.

The existing Runtime journal suite checks `sqlite_version()=3.51.3`, WAL mode, persisted instance identity and integrity through the linked SQLx connection, not the host CLI. The original PostgreSQL migration/session/TOTP/access/graph recovery, native scientific/OCI/cold-restore and browser tests remain required. The source comparison of old/new session schemas and MessagePack serialization does not replace their execution. Rationale and original copied-DDL license attribution remain in [reuse research](../../../docs/research/reuse.md#bundled-sqlite-wal-reliability-and-sqlx-compatibility), [compatibility](../../../docs/architecture/compatibility-matrix.md) and THIRD_PARTY_NOTICES.

The assistant authored exact patches. Temporary isolated CI applied them with native Git, formatted with rustfmt, resolved Cargo.lock and staged unreferenced file blobs without model participation or commit/push/ref updates. The assistant checked the actual artifacts before selecting files for the committed tree. Final source removes `.github/workflows/sqlite-resolution.yml`, `.github/sqlx-compatibility.patch.gz`, `.github/sqlx-borrowed.patch` and `.github/sqlx-docs.patch`. There is no permanent generator or new query framework. Final CI reads committed source with original `--locked` and unchanged-source checks.

### Packaged gateway and restart acceptance

The [field-level plan](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5715833744) closes a specific T41 test gap: the previous real-browser harness used Vite preview while personal installations use `deploy/Caddyfile`. Existing synthetic gateway tests and isolated recovery tests cannot prove that deployed routing and a restarted API preserve the same authenticated project and command receipt.

Extend only `apps/web/scripts/native-browser.mjs`, its existing `native-console.spec.ts`, Web CI, this task and the existing operator/contributor guides. Copy the actually built Rust binary and production `dist` into a private test release, and run native Caddy with the actual deployment route/policy. Only certificate issuance and the administrator listener are disabled for loopback. Reuse the existing process-group lifecycle, credential redaction, disposable PostgreSQL identities and cleanup regressions; do not add a test control HTTP service, new runner, mock API or persistence framework.

Two explicit browser phases run once each without skipped tests. The first preserves the real TOTP enrollment, lost-ACK/idempotency, CSRF and three-viewport checks, then privately saves Playwright's original storageState and ProjectCreate body/receipt. The harness stops the real API, requires its normal exit, observes 502 through the still-running Caddy while the real static shell remains available, and launches a different API PID against the same database, state path and keys. It must report initialized=true without new migration/bootstrap/init-state. The second browser phase loads the original session, reads the unchanged project and replays the exact original request/key with the same receipt and only one project, then tests confirmed logout and denial of further writes. Session files and raw diagnostics stay private and are deleted.

Web CI reuses the hosting workflow's pinned Caddy2.11.4 binary extraction and keeps every original test and timeout. No production Rust, schema, lockfile, Caddy policy or service definition changes. Caddy [validate/run](https://caddyserver.com/docs/command-line) and Playwright [storageState](https://playwright.dev/docs/api/class-browsercontext#browser-context-storage-state) provide the native capabilities. This is same-host loopback routing and clean API restart acceptance, not public TLS, systemd boot, active Worker recovery, coordinated backups or real research.

<a id="verification"></a>
## Verification evidence

The PRs retain full commands, failed iterations, review replies and results. These references preserve concrete provenance; none approves a later Head:

| Exact revision | Observed result |
|---|---|
| `37308b87d728f4f6437267e3cbfdd66b97d624ab` | [Web 35179717849](https://github.com/zhengui666/QuaZonai/actions/runs/35179717849), job105069667017: 518 Vitest passed / 6 failed because mutable-state checks preceded receipt replay. |
| `cf11b7be1fad4f62270d777700dd33f3dae842ec` | [Web 35180454929](https://github.com/zhengui666/QuaZonai/actions/runs/35180454929): unit/build/Demo passed, then three viewports 414 passed / 6 failed; real-API stage skipped. |
| `b60b45fa3fd4346805296bdd84196f005e5ed568` | [Web 35185171980](https://github.com/zhengui666/QuaZonai/actions/runs/35185171980): 530 passed / 1 failed of 531 Vitest. The test wrongly expected an empty-binding draft to save; original 422 behavior was preserved and coverage reshaped. |
| `6808e394585ca0d063442dd4234f8d8559a06719` | [Web 35185968555](https://github.com/zhengui666/QuaZonai/actions/runs/35185968555) passed all stages; [hosting 35185968521](https://github.com/zhengui666/QuaZonai/actions/runs/35185968521) passed 8 real-Caddy cases and native process prerequisites. |
| `ff8bee7f8bc4557d309b709edad6166dde1c03b3` | PR #85 post-merge main CI35205505863, Web35205505883, Runtime35205505910, hosting35205505881 and CodeQL35205505868 returned success; links are retained in the acceptance index. |
| `d98e50444d183548cedeb9af81da62fafd00c05e` | [Runtime 35210864380](https://github.com/zhengui666/QuaZonai/actions/runs/35210864380), artifact10492261134: 17 OCI + 1 cold-restore test passed. The archive contained actual WAL and output bytes. All applicable CI and clean review are recorded in #86; no production RPO/RTO inferred. |
| `06c950976b86290df7b4586f1db9966274589b96` | [Resolution 35215777804](https://github.com/zhengui666/QuaZonai/actions/runs/35215777804), artifact10494646558: native lock generation succeeded; 69 Store SqlSafeStr errors remained. |
| `18a4e656eebd2c6f144ab7cf1388c7396ee1f044` | [Resolution 35218000174](https://github.com/zhengui666/QuaZonai/actions/runs/35218000174): exact patch/format/resolution succeeded; unsupported AssertSqlSafe<&String> stopped compilation. |
| `b966b58d7f53f33484bd46f49eaa261b29d830c6` | [Resolution 35218567705](https://github.com/zhengui666/QuaZonai/actions/runs/35218567705): two static prepare calls required native SqlStr. Subsequent 5572 used a wrong private module path; e6 corrected it to the public re-export. No test was removed. |
| `e6d1899dadf7b1e6861dd50161d3a0b35b8921b6` | [Resolution 35220221096](https://github.com/zhengui666/QuaZonai/actions/runs/35220221096), artifact10496159991: all-workspace/all-target/all-feature cargo check --locked succeeded with no compiler diagnostics, unchanged diff after check. The 45 staged files were checked against actual bytes and authored changes. |
| `ce76c585947bf4875618dda818bd558915aac5f6` | [Resolution 35221232002](https://github.com/zhengui666/QuaZonai/actions/runs/35221232002), artifact10497367175: compilation succeeded again, no diagnostics; same 45 files plus the exact DESIGN pin update. This is generated-source compilation, not final committed-source test execution. |

PR #87's final `d302f5c1d9737d105278815150958b544709c6b2` passed CI35226608307, Web35226608222, Runtime35226608214, hosting35226608265 and CodeQL35226608310. Store artifact10501375712 recorded 681 successful tests with no failures/ignored/filtered, including the formerly aborting Mission, all 18 Mission cases, six migration cases and three access-recovery cases. The corrected pointee-layout finding was resolved with executed evidence and clean review5715140727 before the expected-Head merge. Merge `e426fc3` has no source diff from d302; its push checks remain independently observed in the PR, not inferred from source equality.

At creation of the hosted-restart continuation, its new Caddy/browser phases have not executed; no pre-change runtime failure is claimed for this added acceptance coverage. Require the new Head's actual tests, both browser phase exit codes, original/new process evidence, cleanup and independent review. Missing Caddy is failure, never fallback to Vite. Compilation, preview records, controlled peers and syntax stand-ins do not prove account inference or market validity. Failed, cancelled, skipped, absent or old-Head checks are not passes.

<a id="review"></a>
## Independent review

Inspect all summary and inline findings. Repairs require fresh read-only `@codex review` for final Head; old clean responses do not invalidate later findings. Resolve threads with source and verification evidence. Author inspection is not independent approval.

<a id="delivery"></a>
## Delivery boundary

1. Publish the PR and finish its declared scope.
2. Require every applicable final-Head CI check to pass, all actionable findings resolved and explicit clean Codex feedback for that Head.
3. Only then mark ready, merge and verify main, recording the actual merge commit.
4. **Never ask GitHub Codex to fix, implement, edit, commit or push.** It is an independent read-only reviewer.

PR metadata owns draft/open/merged state. Issue metadata is not acceptance evidence: a fresh API read found #62 closed at 2026-09-17T09:30:38Z. Closure event31308423680 is adjacent to PR #85's merge but states no intent or completion evidence. This task does not infer a deliberate scope waiver or undo that owner-attributed state. The remaining full contract still governs delivery.

<a id="handoff"></a>
## Continuation

Each user turn has public execution summaries in its PR: [Turn 01](https://github.com/zhengui666/QuaZonai/pull/85#issuecomment-5711370866), [Turn 02](https://github.com/zhengui666/QuaZonai/pull/86), [Turn 03](https://github.com/zhengui666/QuaZonai/pull/87#issuecomment-5713645761). Keep activity logs in GitHub, not product manuals.

CodexPro workspace discovery/search exposed no matching action; earlier sessions recorded HTTP502. GitHub file tools and isolated native CI are available. No owner-host workspace, local executor, production installation, paid model account, actual legacy snapshot or coordinated production restore was accessed. The assistant authors code; it is not delegated.

Continue genuine T07/T08 account/model, T39 owner snapshot, T40 coordinated restore/active-job faults, T41 runbooks and T42 research-to-delivery acceptance. Preserve user state and credentials; required external evidence cannot be replaced with fixtures.
