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

These are scoped deliveries. None waives the remaining original product contract. [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62) is the requirement source; its closed metadata is not evidence that every acceptance case ran.

## Executable coverage map

The entries below identify maintained tests, not automatic passes. SQLx cases require disposable PostgreSQL18/PGMQ and the prerequisites in [CLI](../../CLI.md#开发测试). Native OCI and protected account tests retain their own dependencies.

| Contract family | Existing entrypoints | Important limit |
|---|---|---|
| T01, T14–T21: science, allocation and simulation | [Job tests](../../apps/job/tests), [OCI tests](../../apps/runtime/tests/native_oci.rs) | Controlled catalogs/golden cases do not establish real-market provenance, PIT or expected returns. |
| T02: credential-free demonstration | `make demo-preview`, [complete flow](../../apps/web/tests/demo-complete.spec.ts), [preview](../../apps/web/tests/demo-preview.spec.ts), [history](../../apps/web/tests/demo-history.spec.ts), [receipt tests](../../apps/web/src/demo-flow.test.ts) | In-memory synthetic interactions; historical samples are not outputs of a new Cycle. |
| T03–T09: native profiles/protocol/tools/Reviewer | [Codex](../../apps/server/tests/codex_native.rs), [MCP](../../apps/server/tests/mcp_authoring.rs), [Mission Worker](../../apps/server/tests/mission_worker.rs) | Controlled responses are not protected real-account inference. |
| T10–T13, T15: budget/evidence/eligibility | [Store tests](../../crates/store/tests), [review](../../crates/store/tests/research_review.rs), [publication](../../crates/store/tests/evaluation_publication.rs) | Fixture REAL/PIT declarations are not independently verified authorized data. |
| Delivery/Paper/Forward/Wake | [Portfolio/delivery](../../apps/server/tests/portfolio_study_http.rs), [Forward](../../apps/server/tests/forward_automation.rs), [downstream](../../apps/server/tests/downstream_http.rs) | No broker authority; controlled downstream responses are not production delivery. |
| T40: individual recovery and cancellation | [Journal](../../apps/runtime/tests/journal.rs), [OCI](../../apps/runtime/tests/native_oci.rs), [cold/ownership](../../apps/runtime/tests/native_restore.rs), [Run lifecycle](../../crates/store/tests/run_lifecycle.rs), [access recovery](../../apps/server/tests/recovery_access.rs) | Separate checkpoints do not by themselves prove coordinated recovery. |
| T40: joint control and Runtime checkpoint | [Native joint target](../../apps/runtime/tests/native_control_restore.rs), [shared strict archive helper](../../apps/runtime/tests/support/archive.rs), [runbook and command](../runtime-recovery.md#joint-control-and-runtime-checkpoint) | Known terminal remote/unsettled control with quiescent writers, same host/paths; no active-job snapshot, cross-host or production RPO/RTO claim. |
| T39: historical migration | [Source inspection](../../apps/server/tests/historical_source.rs), [rows](../../apps/server/tests/historical_import_http.rs), [artifacts](../../apps/server/tests/historical_artifacts.rs) | Selected synthetic schemas are not the owner's full snapshot. |
| UI/PWA and real entry | [Browser suites](../../apps/web/tests), [hosted restart runner](../../apps/web/scripts/native-browser.mjs) | Real TOTP/project/receipt restart checks do not cover the entire business workflow. |
| T41: documentation and command contracts | `make check-docs`, [CLI help](../../apps/server/tests/client_help.rs), [CI](../../.github/workflows/ci.yml), [Web CI](../../.github/workflows/web.yml) | Links/help/schema validation does not execute account-dependent runbooks. |

Use DESIGN's complete case definitions when collecting final acceptance; this grouping neither deletes nor narrows a case.

## Synthetic Demo evidence

The [README quickstart](../../README.md#quickstart) launches the existing Vite preview. [Isolated configuration](../../apps/web/playwright.demo.config.ts) runs `npm --prefix apps/web run test:demo`; [three-viewport configuration](../../apps/web/playwright.config.ts) separately runs `npm --prefix apps/web run test:e2e`. Web CI runs both before the real hosted API/browser phases.

The preview forks/freezes a Brief with matching inputs, explicitly activates its project, selects synthetic profiles and verifies the exact new Cycle/Run identity. That Cycle has no experiments or qualified candidates. Existing historical selection/Alpha/portfolio/Release views and the original DEMO Package download are separate; approval and production Claim remain unavailable. PR #85 and subsequent accepted checks cover this scoped presentation, not account inference, market computation or the owner's deployment.

## Native recovery evidence

The [access suite](../../apps/server/tests/recovery_access.rs) restores real PostgreSQL/private state and invalidates old access; [graph recovery](../../apps/server/tests/support/graph_recovery.rs) compares persisted relationships and original Package/Forward bytes. The [cold target](../../apps/runtime/tests/native_restore.rs) independently restores actual SQLite/WAL, original manifests/bytes/tombstones and container identity, then compiles from restored inputs. The same target's metadata control demonstrates numeric-owner preservation. These remain required alongside the new joint test.

[PR #89](https://github.com/zhengui666/QuaZonai/pull/89) owns the joint test's current implementation and exact-Head results. It uses native Parquet input and real Runtime metadata/output, keeps the control Attempt `SENT_UNKNOWN`, stops all relevant writers, and restores the database, control artifacts/secrets, Runtime/config and catalog as one checkpoint. The actual Worker must defer without publication/ACK while an original parameter file is withheld, then adopt the same Attempt/spec and original bytes once after that same file returns and the real lease expires. A distinct new validation requires current Runtime capabilities and reads restored data; it does not replace the original result. Provenance remains FIXTURE/PIT UNVERIFIED, and trusted Store login setup is not actual TOTP/account evidence.

At initial formal Head `2654790cc3e086dbcc41a06ae789d02d8292b5d2`, [Runtime35297222018](https://github.com/zhengui666/QuaZonai/actions/runs/35297222018) passed the existing17OCI+2restore cases but the new joint test failed during synchronous catalog preparation, before checkpointing. The corrected execution context and subsequent final-Head results must be checked in #89; compilation or test existence is not successful joint recovery. No old pass is reused for new source.

<a id="acceptance"></a>
## Unfinished product acceptance

| Contract | Remaining acceptance |
|---|---|
| T02 | Reconcile the complete demonstration requirement against accepted scoped interactions, keeping historical samples separate from new Cycle output; no substitution for T07/T08/T42. |
| T07/T08 | Dedicated operator-controlled native account login, real model→scientific Job/Evaluation→same persistent Thread consumption on an accepted commit. Keep credentials private. |
| T39 | Owner-selected full legacy snapshot and artifacts, precision/relationship mapping and explicit treatment of unproven old qualification. |
| T40 | Obtain the actual accepted joint-checkpoint result in #89 in addition to individual restore coverage. Separately complete active-job/fault and relevant checkpoint scenarios and measure the intended production recovery objective; the quiescent fixture alone does not establish them. |
| T41 | Execute remaining environment-dependent README/CLI/Skill/runbooks on the accepted deployment. Real loopback Caddy/API restart does not prove public TLS or systemd boot. |
| T42 | Fresh Web and CLI instance through real research, independent qualification, at least two Alphas, portfolio evaluation, target package, Paper/Forward and applicable policy behavior using authorized data. |

Unavailable account/data/snapshot inputs are unverified requirements, never skipped passes. GitHub records #62 as closed at2026-09-17T09:30:38Z near #85's merge; [the event](https://api.github.com/repos/zhengui666/QuaZonai/issues/events/31308423680) contains no additional test evidence or scope waiver. Current-source complete acceptance and post-merge verification remain required before declaring production readiness.
