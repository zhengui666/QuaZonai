# Issue #62 implementation and acceptance evidence

[DESIGN](../../DESIGN.md) owns the complete product contract and W0–W8/T01–T42 acceptance. This file is a navigation and evidence index, not another specification or a permanent release-ready certificate. Historical implementation notes remain in Git history; new checks belong to their exact commit and CI run.

## Merged baselines

[PR #63](https://github.com/zhengui666/QuaZonai/pull/63) merged on 2026-09-16 as `fae61dd9e130613ef63e1ddaaaefddca5e55b2bb`. The following successful runs are evidence for that commit only:

| Run | Proven scope |
|---|---|
| [CI](https://github.com/zhengui666/QuaZonai/actions/runs/35097394993) | Rust contracts/scientific fixtures, native Codex with controlled responses, PostgreSQL/PGMQ and Store/Server regressions |
| [Web console](https://github.com/zhengui666/QuaZonai/actions/runs/35097395076) | Generated client, types/unit/build, three viewports/PWA and disposable real API/database browser acceptance |
| [Native Runtime](https://github.com/zhengui666/QuaZonai/actions/runs/35097394991) | Real OCI lifecycle, compile, recovery and isolation regressions |
| [CodeQL](https://github.com/zhengui666/QuaZonai/actions/runs/35097394922) | Configured analyses of that source commit |

[PR #85](https://github.com/zhengui666/QuaZonai/pull/85) subsequently merged on 2026-09-17 as `ff8bee7f8bc4557d309b709edad6166dde1c03b3`. Its accepted Head was `fbacfc63791259f899adab122bad814cbe66c428`; native hosting, root-render recovery, installation commands and the explicitly synthetic interactive preview were the declared scope. Post-merge [CI](https://github.com/zhengui666/QuaZonai/actions/runs/35205505863), [Web](https://github.com/zhengui666/QuaZonai/actions/runs/35205505883), [Native Runtime](https://github.com/zhengui666/QuaZonai/actions/runs/35205505910), [hosting](https://github.com/zhengui666/QuaZonai/actions/runs/35205505881) and [CodeQL](https://github.com/zhengui666/QuaZonai/actions/runs/35205505868) returned success on that merge commit. These do not verify later cold-restore additions.

A follow-up commit must pass its own checks. Neither merge completed the remaining product acceptance or closed [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

## Executable coverage map

These are maintained test entrypoints, not claims that every full-product scenario has passed. SQLx suites require the disposable instance described in [CLI](../../CLI.md#开发测试); native OCI and Codex tests require their documented prerequisites.

| Contract family | Existing executable entrypoints | Important limit |
|---|---|---|
| T01, T14–T21: native science, allocation and simulation | [job tests](../../apps/job/tests), [OCI tests](../../apps/runtime/tests/native_oci.rs) | Controlled catalogs and golden cases do not prove real-market provenance, PIT or return expectations |
| T02: credential-free interactive demonstration | `make demo-preview`, [complete browser flow](../../apps/web/tests/demo-complete.spec.ts), [preview/history suites](../../apps/web/tests/demo-preview.spec.ts), [receipt and consistency tests](../../apps/web/src/demo-flow.test.ts) | In-memory synthetic records only; historical Alpha/portfolio/Release samples are not outputs of the newly started Cycle |
| T03–T09: native profiles, protocol, tools and Reviewer | [Codex](../../apps/server/tests/codex_native.rs), [MCP](../../apps/server/tests/mcp_authoring.rs), [Mission Worker](../../apps/server/tests/mission_worker.rs) | Controlled model responses are not protected real-account inference |
| T10–T13, T15: budgets, evidence and eligibility | [Store tests](../../crates/store/tests), [research review](../../crates/store/tests/research_review.rs), [publication](../../crates/store/tests/evaluation_publication.rs) | Protocol fixtures and REAL/PIT declarations are not independently verified authorized market data |
| Delivery, approval, Paper/Live, Forward and Wake | [portfolio/delivery](../../apps/server/tests/portfolio_study_http.rs), [Forward automation](../../apps/server/tests/forward_automation.rs), [downstream](../../apps/server/tests/downstream_http.rs) | No real broker authority or execution; downstream fixtures do not prove a production target delivery |
| Recovery, cancellation and identity boundaries | [runtime journal](../../apps/runtime/tests/journal.rs), [native OCI](../../apps/runtime/tests/native_oci.rs), [cold archive restore](../../apps/runtime/tests/native_restore.rs), [Run lifecycle](../../crates/store/tests/run_lifecycle.rs), [recovery access](../../apps/server/tests/recovery_access.rs) | Individual restart/archive tests do not establish complete coordinated restoration or production RPO/RTO |
| Historical migration | [source inspection](../../apps/server/tests/historical_source.rs), [historical rows](../../apps/server/tests/historical_import_http.rs), [artifacts](../../apps/server/tests/historical_artifacts.rs) | Selected synthetic old schemas and public artifacts are not the owner's complete legacy snapshot |
| UI/PWA and real browser entry | [browser suites](../../apps/web/tests), [native browser runner](../../apps/web/scripts/native-browser.mjs) | Real authentication/project smoke is separate from the entire research-to-delivery workflow |
| T41: documentation and command contracts | `make check-docs`, [CLI help](../../apps/server/tests/client_help.rs), generated OpenAPI/client checks in [CI](../../.github/workflows/ci.yml) and [Web CI](../../.github/workflows/web.yml) | Links/help/schema checks do not execute account-, data- or database-dependent runbook examples |

Use the full T01–T42 definitions in DESIGN when collecting final acceptance; the groups above do not replace or remove any case.

## Synthetic Demo evidence

The [README quickstart](../../README.md#quickstart) starts the existing Vite preview with `make demo-preview`. The [isolated configuration](../../apps/web/playwright.demo.config.ts) runs `npm --prefix apps/web run test:demo`; the [three-viewport configuration](../../apps/web/playwright.config.ts) separately runs `npm --prefix apps/web run test:e2e`, including [historical qualification and portfolio views](../../apps/web/tests/demo-history.spec.ts). The [Web workflow](../../.github/workflows/web.yml) installs the documented prerequisites and runs both before real-API browser acceptance.

The interactive flow forks and freezes a Brief with three matching input bindings, explicitly activates the project, selects two synthetic profiles, and checks the exact new Cycle/Run links. The new Cycle has no experiments or qualified candidates. Its original Run is readable, while only the historical Cycle advertises a selection snapshot. Historical Alpha/portfolio/Release views and the original DEMO Package download remain separate; the warning, disabled approval and refusal of production Claim preserve the no-delivery boundary. Original frozen history is not rebound to a newer Runtime.

PR #85's accepted and post-merge Web checks cover these scoped interactions, three viewports/PWA and disposable real-API browser acceptance. The original failed checks and their repairs remain in the [task](../../.opensdlc/tasks/personal-production/task.md) and PR discussion; they are not hidden by the later green result. No account inference, market computation or owner-host deployment is established by this preview.

## Native recovery evidence

The existing [access recovery suite](../../apps/server/tests/recovery_access.rs) performs real PostgreSQL dump/restore, private-state archive restoration and fresh-server access cutover. [Graph recovery](../../apps/server/tests/support/graph_recovery.rs) compares persisted application relationships and original Package/Forward bytes. These existing entrypoints are reused, not replaced by another backup framework.

[Native cold restore](../../apps/runtime/tests/native_restore.rs) adds an explicit `native-oci` target for an isolated same-host Runtime checkpoint: real compilation/output retrieval, pre-submit cancellation, whole stopped SQLite/WAL state archive, original-directory retention, new-process restoration, original result/cancellation replay and another native compile using restored inputs. Its [runbook](../runtime-recovery.md) states the same-path, no-active-writers and coordinated-checkpoint prerequisites. The existing Native Runtime workflow runs both the original OCI suite and the cold-restore target; the latter's first actual result must be recorded in its delivery PR. Test authorship is not a passed restore.

A terminal-job Runtime checkpoint does not demonstrate an active-job snapshot, cross-host relocation, complete database/Runtime consistency or the owner's actual recovery objective. Keep those T40 obligations explicit.

<a id="acceptance"></a>
## Unfinished product acceptance

| Contract | Missing acceptance evidence / next action |
|---|---|
| T02 | The scoped interactive preview is merged through PR #85. Check its coverage against the full demonstration contract, keeping the independent historical samples separate from new Cycle outputs; it does not replace T07/T08/T42. |
| T07/T08 | Operator login in a dedicated native profile, then real model → scientific Job/Evaluation → consumption in the same persistent Thread. Keep account material private and use a reviewed fixed commit. |
| T39 | Owner-selected legacy snapshot plus its artifacts, full mapping/precision/relationship report and explicit handling of unproven old qualifications. Existing projection import tests are insufficient. |
| T40 | Execute and review the new Runtime cold-restore target, then complete coordinated fresh-process backup/restore and active-job fault rehearsal with database, private state, historical artifacts and Runtime reconciliation; measure the intended production recovery objective. Individual native tests remain partial evidence. |
| T41 | Execute the remaining environment-dependent README/CLI/Skill/runbook examples against the accepted deployment; record commands, outcomes and prerequisites. |
| T42 | Fresh instance through both Web and CLI: real research, independent qualification, at least two Alphas, portfolio evaluation, target package, Paper/Forward and applicable policy-driven behavior using authorized data. |

Unavailable account/data/backup inputs remain explicit gaps, not skipped passes. Follow-up maintenance may merge within its stated reviewed scope, but Issue #62 stays open until the complete contract, current-source evidence and post-merge verification are satisfied. No production deployment or full-product acceptance is implied by this index.
