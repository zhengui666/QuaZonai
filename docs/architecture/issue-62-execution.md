# Issue #62 implementation and acceptance evidence

[DESIGN](../../DESIGN.md) owns W0–W8/T01–T42. This is a source and evidence index, not another specification or a permanent readiness certificate. Git history and the linked PRs retain original failed iterations; every later commit requires its own checks.

## Merged baselines

| Delivery | Accepted scope and evidence owner |
|---|---|
| [PR #63](https://github.com/zhengui666/QuaZonai/pull/63), `fae61dd9e130613ef63e1ddaaaefddca5e55b2bb` | Native research implementation baseline. Exact-commit [CI35097394993](https://github.com/zhengui666/QuaZonai/actions/runs/35097394993), [Web35097395076](https://github.com/zhengui666/QuaZonai/actions/runs/35097395076), [Runtime35097394991](https://github.com/zhengui666/QuaZonai/actions/runs/35097394991) and [CodeQL35097394922](https://github.com/zhengui666/QuaZonai/actions/runs/35097394922) remain historical evidence, not full product acceptance. |
| [PR #85](https://github.com/zhengui666/QuaZonai/pull/85), `ff8bee7f8bc4557d309b709edad6166dde1c03b3` | Native hosting, root-render recovery and explicitly synthetic interactive preview. The PR owns accepted and post-merge checks, screenshots, fixes and limitations. |
| [PR #86](https://github.com/zhengui666/QuaZonai/pull/86), `45033f46f87c7b12a85fd42292f5c164a4578468` | Isolated same-host Runtime cold restore; accepted Head d98e504 passed17OCI+1cold restore in [35210864380](https://github.com/zhengui666/QuaZonai/actions/runs/35210864380). Not a common database/Runtime checkpoint. |
| [PR #87](https://github.com/zhengui666/QuaZonai/pull/87), `e426fc3e0a3f2c784609d004533c9faab7440b54` | Native SQLite/SQLx reliability and executed Mission stack repair. Final d302 checks passed; the later post-merge archive failure remains recorded, not overwritten by earlier green results. |
| [PR #88](https://github.com/zhengui666/QuaZonai/pull/88), `08c0af7ddb8a62375959a74cf24dad851113d017` | Actual Caddy/packaged API restart, first command receipt, mixed-owner archive control and observed Mission deadline. Final de9fb879 workflows passed, including681Store/Server,17OCI+2restore and both real hosted browser phases. Scope and exact artifacts/review remain in the PR. |
| [PR #89](https://github.com/zhengui666/QuaZonai/pull/89), `35a1625aa7a3349fc0d3f3c384a79b6d3b3e4699` | Joint PostgreSQL/control/Runtime/catalog restoration through the real Worker, with original Attempt/bytes/ACK preservation and a new native task using restored data. Account login uses a controlled fixture, not a real subscription. |
| [PR #90](https://github.com/zhengui666/QuaZonai/pull/90), `6754d04166a22b3f0a6ac671e7dc44e47ca4a155` | Complete source-lock inventory. [Post-merge evidence](https://github.com/zhengui666/QuaZonai/pull/90#issuecomment-5725755490) separately verifies all five workflows,681Store/Server,17OCI+2cold/ownership+1joint and both hosted browser phases. Not binary/image contents or complete license clearance. |

These are scoped deliveries, not automatic full-product acceptance. The owner's [2026-09-18 amendment](../../DESIGN.md#acceptance-scope) explicitly closes dedicated-account acceptance by waiver; that decision, not a prior merge, changes the scope. [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62) is the requirement source; its closed metadata is not evidence that every acceptance case ran.

## Executable coverage map

The entries below identify maintained tests, not automatic passes. SQLx cases require disposable PostgreSQL18/PGMQ and the prerequisites in [CLI](../../CLI.md#开发测试). Native OCI retains its prerequisites. Dedicated live-account execution is no longer required by the owner-approved scope; credential-free native protocol tests remain required.

| Contract family | Existing entrypoints | Important limit |
|---|---|---|
| T01, T14–T21: science, allocation and simulation | [Job tests](../../apps/job/tests), [OCI tests](../../apps/runtime/tests/native_oci.rs) | Controlled catalogs/golden cases do not establish real-market provenance, PIT or expected returns. |
| T02: credential-free demonstration | `make demo-preview`, [complete flow](../../apps/web/tests/demo-complete.spec.ts), [preview](../../apps/web/tests/demo-preview.spec.ts), [history](../../apps/web/tests/demo-history.spec.ts), [receipt tests](../../apps/web/src/demo-flow.test.ts) | In-memory synthetic interactions; historical samples are not outputs of a new Cycle. |
| T03–T09: native profiles/protocol/tools/Reviewer | [Codex](../../apps/server/tests/codex_native.rs), [MCP](../../apps/server/tests/mcp_authoring.rs), [Mission Worker](../../apps/server/tests/mission_worker.rs) | Controlled responses remain identified as such. Only T07 and the dedicated-account portions of T03–T05/T08 follow the waiver in DESIGN 0.4; T06 model-directory and T09 independent-Reviewer validation remain required, as do non-account Thread/tool/business checks. |
| T10–T13, T15: budget/evidence/eligibility | [Store tests](../../crates/store/tests), [review](../../crates/store/tests/research_review.rs), [publication](../../crates/store/tests/evaluation_publication.rs) | Fixture REAL/PIT declarations are not independently verified authorized data. |
| Delivery/Paper/Forward/Wake | [portfolio/delivery](../../apps/server/tests/portfolio_study_http.rs), [Forward](../../apps/server/tests/forward_automation.rs), [downstream](../../apps/server/tests/downstream_http.rs) | No broker authority; controlled downstream responses are not production delivery. |
| T40: individual recovery and cancellation | [Journal](../../apps/runtime/tests/journal.rs), [OCI](../../apps/runtime/tests/native_oci.rs), [cold/ownership](../../apps/runtime/tests/native_restore.rs), [Run lifecycle](../../crates/store/tests/run_lifecycle.rs), [access recovery](../../apps/server/tests/recovery_access.rs) | Separate checkpoints do not by themselves prove coordinated recovery. |
| T40: joint control and Runtime checkpoint | [Native joint target](../../apps/runtime/tests/native_control_restore.rs), [shared strict archive helper](../../apps/runtime/tests/support/archive.rs), [runbook and command](../runtime-recovery.md#joint-control-and-runtime-checkpoint) | Known terminal remote/unsettled control with quiescent writers, same host/paths; no active-job snapshot, cross-host or production RPO/RTO claim. |
| T39: historical migration | [Source inspection](../../apps/server/tests/historical_source.rs), [rows](../../apps/server/tests/historical_import_http.rs), [artifacts](../../apps/server/tests/historical_artifacts.rs) | Selected synthetic schemas are not the owner's full snapshot. |
| UI/PWA and real entry | [Browser suites](../../apps/web/tests), [hosted restart runner](../../apps/web/scripts/native-browser.mjs) | Real TOTP/project/receipt restart checks do not cover the entire business workflow. |
| T41: documentation and command contracts | `make check-docs`, [CLI help](../../apps/server/tests/client_help.rs), [CI](../../.github/workflows/ci.yml), [Web CI](../../.github/workflows/web.yml) | Links/help/schema validation does not execute every runbook. The optional dedicated-account examples are waived; other commands retain their original coverage requirements. |

Use DESIGN's case definitions with its explicit owner amendment when collecting final acceptance. This index does not independently grant additional waivers.

## Synthetic Demo evidence

The [README quickstart](../../README.md#quickstart) launches the existing Vite preview. [Isolated configuration](../../apps/web/playwright.demo.config.ts) runs `npm --prefix apps/web run test:demo`; [three-viewport configuration](../../apps/web/playwright.config.ts) separately runs `npm --prefix apps/web run test:e2e`. Web CI runs both before the real hosted API/browser phases.

The preview forks/freezes a Brief with matching inputs, explicitly activates its project, selects synthetic profiles and verifies the exact new Cycle/Run identity. That Cycle has no experiments or qualified candidates. Existing historical selection/Alpha/portfolio/Release views and the original DEMO Package download are separate; approval and production Claim remain unavailable. PR #85 and subsequent accepted checks cover this scoped presentation, not account inference, market computation or the owner's deployment.

## Native science and persisted Thread

The explicit [native science target](../../apps/server/tests/native_science_thread.rs) connects
official App Server shell/tool-search dispatch, the real production `job allocate` executable,
Mission MCP `artifact.submit`, HTTP/PostgreSQL/ArtifactStore publication and same-Thread
continuation after a real process restart. Run it with the existing Native Runtime workflow's
Job/Codex/PostgreSQL prerequisites:

```sh
QUAZONAI_NATIVE_JOB_BIN="$(pwd)/target/debug/job" \
rustup run 1.98.1 cargo test --locked -p server --features native-science \
  --test native_science_thread -- --test-threads=1 --nocapture
```

Supply the explicitly isolated `DATABASE_URL` and pinned `CODEX_NATIVE_BIN` as in that workflow,
never a production database or real account. The provider emits controlled decisions, not
scientific return values. The native Job must calculate both the optimal allocation and an
infeasible case; the latter must have no weights. Original report bytes and producer identity
are checked through the real storage interface, and each public fixture conclusion must cite
its corresponding Artifact ID. No report is promoted to qualification or delivery evidence.

At introduction the target is unexecuted; the active PR and exact-Head Actions results must
establish its actual outcome. This focused connection is not the production Worker's complete
experiment/Evaluation chain, real model inference or full T42. Keep those boundaries distinct;
the account-only waiver remains unchanged.

## Native recovery evidence

The [access suite](../../apps/server/tests/recovery_access.rs) restores real PostgreSQL/private state and invalidates old access; [graph recovery](../../apps/server/tests/support/graph_recovery.rs) compares persisted relationships and original Package/Forward bytes. The [cold target](../../apps/runtime/tests/native_restore.rs) independently restores actual SQLite/WAL, original manifests/bytes/tombstones and container identity, then compiles from restored inputs. The same target's metadata control demonstrates numeric-owner preservation. These remain required alongside the new joint test.

[PR #89](https://github.com/zhengui666/QuaZonai/pull/89) owns the joint test's current implementation and exact-Head results. It uses native Parquet input and real Runtime metadata/output, keeps the control Attempt `SENT_UNKNOWN`, stops all relevant writers, and restores the database, control artifacts/secrets, Runtime/config and catalog as one checkpoint. The actual Worker must defer without publication/ACK while an original parameter file is withheld, then adopt the same Attempt/spec and original bytes once after that same file returns and the real lease expires. A distinct new validation requires current Runtime capabilities and reads restored data; it does not replace the original result. Provenance remains FIXTURE/PIT UNVERIFIED, and trusted Store login setup is not actual TOTP/account evidence.

The initial catalog-preparation failure and its blocking-pool correction remain in PR #89. Its accepted2236a0f Head and merged35a1625 completed the joint case. Main6754d041 subsequently re-executed all17OCI+2cold/ownership+1joint cases in [Runtime35309713311](https://github.com/zhengui666/QuaZonai/actions/runs/35309713311), artifact10533217399. This is executed quiescent-checkpoint evidence, not active-job/cross-host or full-product acceptance; later changes still need their own checks.

<a id="acceptance"></a>
## Acceptance status and remaining work

The [owner authorization](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5725878884) and [DESIGN 0.4](../../DESIGN.md#acceptance-scope) take effect on 2026-09-18. Verification uses existing GitHub Actions; CodexPro reconnection is not required. `COMPLETED_BY_OWNER_WAIVER` closes the specified requirement with execution `NOT_RUN`; it is not a passed test, and adds nothing to run/test pass counts. Do not reopen these account items without new owner authorization.

| Contract / portion | Status | Evidence and remaining boundary |
|---|---|---|
| T07 dedicated live subscription account | **Completed by owner waiver — NOT_RUN** | Owner has no dedicated account and explicitly authorized completion without execution. No real login/logout/paid-inference result is claimed. Existing account functionality and credential-free protocol checks remain. |
| Dedicated-account/credential-dependent live-provider portions of T03–T05, T08 and T42 | **Completed by owner waiver — NOT_RUN** | Only the unavailable account execution is closed. Not a waiver of entire T08/T42, native SYSTEM/CUSTOM behavior, model directory or user workflow. |
| T02 credential-free demonstration | Verified scoped UI on the recorded baseline | The accepted one-command Demo, history/non-delivery checks and three-viewports are exercised by Web CI. It remains synthetic; it does not establish real scientific results or the full T42 chain. |
| T08 non-account Agent loop | Partial evidence; new connection requires execution | The [native science/Thread target](../../apps/server/tests/native_science_thread.rs) must execute genuine Job output through native tools and immutable MCP/HTTP reports across a same-Thread restart. Provider decisions and parent records remain fixtures; complete Worker experiment/Evaluation orchestration is a separate remaining boundary. Account invocation stays waived. |
| T39 historical migration | Remaining | Owner-selected real legacy snapshot/artifacts, precision/relationship mapping and treatment of old qualification. No snapshot waiver was given; synthetic source-schema tests are not that evidence. |
| T40 recovery/disk-full/dependency-offline | Joint and individual checkpoints verified; reconcile remaining original cases | Use accepted joint/cold/access/graph/ENOSPC results, then complete missing original admission/version/offline/no-blind-replay evidence. Do not treat every test limitation as a demand for a new cross-host backup platform. |
| T41 documentation and deployment commands | Partial; non-account examples remain | Native help, documented Demo and real loopback Caddy/API restart are exercised. Verify the remaining actual commands/deployment behavior. Do not demand the waived account example or a CodexPro connection. |
| T42 Web/CLI complete business flow | Non-account chain remains | Fresh instances through research, independent evaluation, at least two Alphas, portfolio, target/Paper/Forward and applicable policy behavior using authorized data and original interfaces, without manual SQL. Account invocation is waived, not scientific validity, data provenance, product authorization or full entrypoint coverage. |

Product authentication, profile readiness, application TOTP, data provenance and Demo restrictions do not change. An absent account still cannot run real model research; the acceptance waiver is not a runtime readiness or qualification bypass.

Other missing data/snapshot/deployment evidence remains unverified, never a skipped pass. GitHub's old #62 closure event alone is not acceptance; this new explicit owner decision authorizes only the account portion described above. Complete the rest of the amended contract and final-Head/post-merge verification before declaring the entire product delivered.
