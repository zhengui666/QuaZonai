# Issue #62 implementation and acceptance evidence

[DESIGN](../../DESIGN.md) owns W0–W8/T01–T42 and the owner's acceptance amendments. This is a source/evidence index, not a second specification or a permanent readiness certificate. PR discussions and Git history retain failed iterations. Every subsequent commit requires its own applicable checks.

## Merged baselines

| Delivery | Accepted scope and evidence owner |
|---|---|
| [PR #63](https://github.com/zhengui666/QuaZonai/pull/63), `fae61dd9e130613ef63e1ddaaaefddca5e55b2bb` | Native research implementation baseline. Exact [CI35097394993](https://github.com/zhengui666/QuaZonai/actions/runs/35097394993), [Web35097395076](https://github.com/zhengui666/QuaZonai/actions/runs/35097395076), [Runtime35097394991](https://github.com/zhengui666/QuaZonai/actions/runs/35097394991) and [CodeQL35097394922](https://github.com/zhengui666/QuaZonai/actions/runs/35097394922) remain historical evidence, not full acceptance. |
| [PR #85](https://github.com/zhengui666/QuaZonai/pull/85), `ff8bee7f8bc4557d309b709edad6166dde1c03b3` | Native hosting, root recovery and synthetic interactive preview; accepted/post-merge evidence and limitations remain in the PR. |
| [PR #86](https://github.com/zhengui666/QuaZonai/pull/86), `45033f46f87c7b12a85fd42292f5c164a4578468` | Isolated same-host cold restore; accepted d98e504 passed17OCI+1cold in [35210864380](https://github.com/zhengui666/QuaZonai/actions/runs/35210864380). Not a joint database/Runtime checkpoint. |
| [PR #87](https://github.com/zhengui666/QuaZonai/pull/87), `e426fc3e0a3f2c784609d004533c9faab7440b54` | SQLite/SQLx reliability and Mission stack repair. The later post-merge archive failure remains recorded, not overwritten by earlier green checks. |
| [PR #88](https://github.com/zhengui666/QuaZonai/pull/88), `08c0af7ddb8a62375959a74cf24dad851113d017` | Real Caddy/packaged API restart, original receipt, mixed-owner archive control and observed Mission deadline. Final de9fb879 passed681Store/Server,17OCI+2restore and both hosted browser phases. |
| [PR #89](https://github.com/zhengui666/QuaZonai/pull/89), `35a1625aa7a3349fc0d3f3c384a79b6d3b3e4699` | Joint PostgreSQL/control/Runtime/catalog restoration through the real Worker, original Attempt/bytes/ACK and new native work from restored data. |
| [PR #90](https://github.com/zhengui666/QuaZonai/pull/90), `6754d04166a22b3f0a6ac671e7dc44e47ca4a155` | Source-lock inventory. [Post-merge evidence](https://github.com/zhengui666/QuaZonai/pull/90#issuecomment-5725755490) verifies all five workflows,681Store/Server,17OCI+2cold/ownership+1joint and both hosted phases. Not binary/image inventory or full license clearance. |
| [PR #91](https://github.com/zhengui666/QuaZonai/pull/91), `1368e1254b3b2ace4777709e07e9bf305291e096` | Owner-authorized GitHub Actions execution and dedicated-account waiver recorded in DESIGN0.4. Waiver is NOT_RUN, not a test pass. |

These are scoped deliveries. The [2026-09-18 owner amendment](../../DESIGN.md#acceptance-scope), not an unrelated merge or Issue closed flag, changes acceptance scope.

## Executable coverage map

These are maintained entrypoints, not automatic passes. SQLx tests need disposable PostgreSQL18/PGMQ and [CLI prerequisites](../../CLI.md#开发测试); native OCI and credential-free App Server tests retain their native dependencies.

| Contract family | Existing entrypoints | Important limit |
|---|---|---|
| T01, T14–T21: science/allocation/simulation | [Job](../../apps/job/tests), [OCI](../../apps/runtime/tests/native_oci.rs) | Controlled catalogs/golden cases do not prove real-market provenance, PIT or returns. |
| T02: credential-free demonstration | `make demo-preview`, [complete](../../apps/web/tests/demo-complete.spec.ts), [preview](../../apps/web/tests/demo-preview.spec.ts), [history](../../apps/web/tests/demo-history.spec.ts), [receipts](../../apps/web/src/demo-flow.test.ts) | Synthetic historical samples are not new Cycle outputs or qualified research. |
| T03–T09: native profiles/protocol/tools/Reviewer | [Codex](../../apps/server/tests/codex_native.rs), [MCP](../../apps/server/tests/mcp_authoring.rs), [Mission](../../apps/server/tests/mission_worker.rs) | Only the dedicated-account portions are waived. T06 model-directory, T09 independent Reviewer and non-account tool/business checks remain. |
| T08: connected native scientific feedback | [Research target](../../apps/runtime/tests/native_research_loop.rs), [catalog](../../apps/runtime/tests/support/scientific_catalog.rs), [connected scenario](../../apps/runtime/tests/support/scientific_feedback.rs) | Actual computation and original Thread with a controlled Provider; not live model reasoning, independent Reviewer completion or full T42. Current exact-Head execution is recorded in PR #92. |
| T10–T13, T15: budgets/evidence/eligibility | [Store](../../crates/store/tests), [review](../../crates/store/tests/research_review.rs), [publication](../../crates/store/tests/evaluation_publication.rs) | Fixture REAL/PIT declarations are not independently verified authorized market data. |
| Delivery/Paper/Forward/Wake | [Portfolio/delivery](../../apps/server/tests/portfolio_study_http.rs), [Forward](../../apps/server/tests/forward_automation.rs), [downstream](../../apps/server/tests/downstream_http.rs) | No broker authority; controlled counterparts are not production delivery. |
| T40: individual recovery | [Journal](../../apps/runtime/tests/journal.rs), [OCI](../../apps/runtime/tests/native_oci.rs), [cold/ownership](../../apps/runtime/tests/native_restore.rs), [Run lifecycle](../../crates/store/tests/run_lifecycle.rs), [access](../../apps/server/tests/recovery_access.rs) | Individual checkpoints do not by themselves prove coordinated recovery. |
| T40: joint checkpoint | [Joint target](../../apps/runtime/tests/native_control_restore.rs), [archive](../../apps/runtime/tests/support/archive.rs), [runbook](../runtime-recovery.md#joint-control-and-runtime-checkpoint) | Known terminal remote/unsettled control, same host/paths and quiescent writers; not active-job snapshot or production RPO/RTO. |
| T39: migration | [Source](../../apps/server/tests/historical_source.rs), [rows](../../apps/server/tests/historical_import_http.rs), [artifacts](../../apps/server/tests/historical_artifacts.rs) | Selected synthetic schemas are not the owner's complete legacy snapshot. |
| UI/PWA/real entry | [Browser suites](../../apps/web/tests), [hosted restart](../../apps/web/scripts/native-browser.mjs) | Real TOTP/project/receipt persistence is not the complete business workflow. |
| T41: commands/documentation | `make check-docs`, [CLI help](../../apps/server/tests/client_help.rs), [CI](../../.github/workflows/ci.yml), [Web](../../.github/workflows/web.yml) | Links/help/schema do not execute every non-account runbook; account-only examples are waived. |

Use the complete DESIGN cases with the explicit owner amendment. This index grants no additional waiver.

## Synthetic Demo evidence

[README](../../README.md#quickstart) starts the preview. [Isolated Demo configuration](../../apps/web/playwright.demo.config.ts) runs `npm --prefix apps/web run test:demo`; [three-viewport configuration](../../apps/web/playwright.config.ts) separately runs `npm --prefix apps/web run test:e2e`. Web CI runs both before actual hosted API/browser phases.

The preview forks/freezes a matching-input Brief, explicitly activates its project, selects synthetic profiles and verifies new Cycle/Run identity. That Cycle has no experiments or qualification. Historical selection/Alpha/portfolio/Release and original DEMO Package download are separate; approval and production Claim remain unavailable. This verifies presentation, not model research or an owner deployment.

## Native recovery evidence

The [access suite](../../apps/server/tests/recovery_access.rs) restores PostgreSQL/private state and invalidates access; [graph recovery](../../apps/server/tests/support/graph_recovery.rs) compares original relationships/Package/Forward bytes. The [cold target](../../apps/runtime/tests/native_restore.rs) preserves SQLite/WAL, original manifests/bytes/tombstones/container identity and compiles from restored inputs. Its numeric-owner control remains required.

PR #89's joint case restores database, control artifacts/secrets, Runtime/config and catalog while the original control Attempt remains SENT_UNKNOWN. The actual Worker defers without publication/ACK when an original parameter is withheld, then adopts the same Attempt/spec and original bytes once after returning that file and expiring the real lease. A distinct new task uses fresh capabilities and restored data; provenance stays FIXTURE/PIT UNVERIFIED. Trusted Store identity setup is not actual TOTP/account evidence.

The initial catalog-context failure and correction remain in #89. Accepted2236a0f and merged35a1625 completed the case. Main6754d041 re-executed17OCI+2cold/ownership+1joint in [Runtime35309713311](https://github.com/zhengui666/QuaZonai/actions/runs/35309713311), artifact10533217399. Later source requires new checks; quiescent recovery is not a full production recovery objective.

## Connected native research evidence

[PR #92](https://github.com/zhengui666/QuaZonai/pull/92) owns the current exact-Head execution, independent review and delivery status. The scenario connects real Parquet, official App Server discovery/actual MCP proposal, production Worker/Runtime computation, persisted Evaluation and bounded feedback to the original Thread. Only Provider responses are scripted under the account waiver. There are no precomputed scientific scores or forced qualifications.

Recorded checkpoints:

| Head | Executed result |
|---|---|
| `dd8f63a8454b16993851a5140ab7c90830d99b79` | Runtime35347533422/artifact10549140604 passed standalone original-source compilation plus17OCI,2cold/ownership and1joint. Not complete feedback. |
| `61d68ed96ab1703477a19930b6514f42423d0e58` | [Runtime35375341178](https://github.com/zhengui666/QuaZonai/actions/runs/35375341178), artifact10560362278:3science passes/1failure at actual MCP proposal. The subsequent per-tool native approval correction required fresh execution. |
| `00ad66cb2e594e82870badc1a214bffd637709c8` | [Runtime35411144350](https://github.com/zhengui666/QuaZonai/actions/runs/35411144350), artifact10573533366: real proposal/compile/forecast/Validation and feedback preparation executed;3science passes/1failure at consume-feedback due to Store::Conflict. The test reopened a native process under its still-current credential fence. |

The [redelivery correction](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5738454941) follows the production queue/lease contract: wait for actual visibility and lease expiry, read the same message again, then use a new owner while preserving Attempt, Thread and feedback reservation. No timestamps, budgets or credential rules are relaxed. Require actual final-Head evidence in #92 for this correction; its source and this description are not a passed result. Existing exact scientific-point/metric/manifest/summary and zero-qualification assertions remain.

<a id="acceptance"></a>
## Acceptance status and remaining work

The [owner authorization](https://github.com/zhengui666/QuaZonai/issues/62#issuecomment-5725878884) and [DESIGN0.4](../../DESIGN.md#acceptance-scope) authorize Actions execution without CodexPro. `COMPLETED_BY_OWNER_WAIVER` closes the specified requirement with `NOT_RUN`; it is not PASSED and contributes no test count. Do not reopen account items without new owner authorization.

| Contract / portion | Delivery status and evidence boundary |
|---|---|
| T07 dedicated live subscription account | **Completed by owner waiver — NOT_RUN.** No actual login/logout/paid inference is claimed; product account functionality and credential-free protocol checks remain. |
| Dedicated-account portions of T03–T05/T08/T42 | **Completed by owner waiver — NOT_RUN.** Not a waiver of whole T08/T42, SYSTEM/CUSTOM configuration, model directory or user workflow. |
| T02 | Accepted scoped one-command Demo, history/non-delivery and three-viewport checks remain synthetic, not scientific/T42 acceptance. |
| T08 non-account connected loop | **Exact-Head execution and delivery tracked in PR #92.** The last recorded checkpoint above failed at feedback consumption; the redelivery repair requires actual execution. Only a complete connected result, all applicable CI and clean review can close its declared portion. Provider responses stay controlled and FIXTURE remains unqualified. |
| T39 | **Remaining:** owner-selected legacy snapshot/artifacts, precision/relationships and old qualification handling. No snapshot waiver was given. |
| T40 | Individual/joint recovery is verified on stated baselines. Reconcile remaining original disk-full/dependency-offline/version/no-blind-replay cases; do not infer a new cross-host backup platform requirement from every test limitation. |
| T41 | Non-account runbook/deployment coverage remains partial. Native help/Demo/loopback Caddy restart do not prove public TLS/systemd boot. Do not require the waived account example or CodexPro. |
| T42 | **Non-account complete business flow remains:** fresh Web/CLI research, independent evaluation, at least two Alphas, portfolio/target/Paper/Forward and applicable policy behavior using original interfaces. Account execution is waived, not scientific validity, data provenance, authorization or complete entrypoint coverage. |

Product readiness, TOTP, data provenance and Demo restrictions do not change. An empty profile cannot run genuine model research. Other missing data/snapshot/deployment evidence remains unverified, not a skipped pass. #62's old closure metadata does not replace complete amended-contract acceptance and final-Head/post-merge verification.
