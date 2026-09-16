# Issue #62 implementation and acceptance evidence

[DESIGN](../../DESIGN.md) owns the complete product contract and W0–W8/T01–T42 acceptance. This file is a navigation and evidence index, not another specification or a permanent release-ready certificate. Historical implementation notes remain in Git history; new checks belong to their exact commit and CI run.

## Merged baseline

[PR #63](https://github.com/zhengui666/QuaZonai/pull/63) merged on 2026-09-16 as `fae61dd9e130613ef63e1ddaaaefddca5e55b2bb`. The following successful runs are evidence for that commit only:

| Run | Proven scope |
|---|---|
| [CI](https://github.com/zhengui666/QuaZonai/actions/runs/35097394993) | Rust contracts/scientific fixtures, native Codex with controlled responses, PostgreSQL/PGMQ and Store/Server regressions |
| [Web console](https://github.com/zhengui666/QuaZonai/actions/runs/35097395076) | Generated client, types/unit/build, three viewports/PWA and disposable real API/database browser acceptance |
| [Native Runtime](https://github.com/zhengui666/QuaZonai/actions/runs/35097394991) | Real OCI lifecycle, compile, recovery and isolation regressions |
| [CodeQL](https://github.com/zhengui666/QuaZonai/actions/runs/35097394922) | Configured analyses of that source commit |

A follow-up commit must pass its own checks. The merge did not complete the remaining product acceptance or close [Issue #62](https://github.com/zhengui666/QuaZonai/issues/62).

## Executable coverage map

These are maintained test entrypoints, not claims that every full-product scenario has passed. SQLx suites require the disposable instance described in [CLI](../../CLI.md#开发测试); native OCI and Codex tests require their documented prerequisites.

| Contract family | Existing executable entrypoints | Important limit |
|---|---|---|
| T01, T14–T21: native science, allocation and simulation | [job tests](../../apps/job/tests), [OCI tests](../../apps/runtime/tests/native_oci.rs) | Controlled catalogs and golden cases do not prove real-market provenance, PIT or return expectations |
| T03–T09: native profiles, protocol, tools and Reviewer | [Codex](../../apps/server/tests/codex_native.rs), [MCP](../../apps/server/tests/mcp_authoring.rs), [Mission Worker](../../apps/server/tests/mission_worker.rs) | Controlled model responses are not protected real-account inference |
| T10–T13, T15: budgets, evidence and eligibility | [Store tests](../../crates/store/tests), [research review](../../crates/store/tests/research_review.rs), [publication](../../crates/store/tests/evaluation_publication.rs) | Protocol fixtures and REAL/PIT declarations are not independently verified authorized market data |
| Delivery, approval, Paper/Live, Forward and Wake | [portfolio/delivery](../../apps/server/tests/portfolio_study_http.rs), [Forward automation](../../apps/server/tests/forward_automation.rs), [downstream](../../apps/server/tests/downstream_http.rs) | No real broker authority or execution; downstream fixtures do not prove a production target delivery |
| Recovery, cancellation and identity boundaries | [runtime journal](../../apps/runtime/tests/journal.rs), [native OCI](../../apps/runtime/tests/native_oci.rs), [Run lifecycle](../../crates/store/tests/run_lifecycle.rs), [recovery access](../../apps/server/tests/recovery_access.rs) | Individual restart/archive tests do not establish complete restore procedures or RPO/RTO |
| Historical migration | [source inspection](../../apps/server/tests/historical_source.rs), [historical rows](../../apps/server/tests/historical_import_http.rs), [artifacts](../../apps/server/tests/historical_artifacts.rs) | Selected synthetic old schemas and public artifacts are not the owner's complete legacy snapshot |
| UI/PWA and real browser entry | [browser suites](../../apps/web/tests), [native browser runner](../../apps/web/scripts/native-browser.mjs) | Real authentication/project smoke is separate from the entire research-to-delivery workflow |
| T41: documentation and command contracts | `make check-docs`, [CLI help](../../apps/server/tests/client_help.rs), generated OpenAPI/client checks in [CI](../../.github/workflows/ci.yml) and [Web CI](../../.github/workflows/web.yml) | Links/help/schema checks do not execute account-, data- or database-dependent runbook examples |

Use the full T01–T42 definitions in DESIGN when collecting final acceptance; the groups above do not replace or remove any case.

<a id="acceptance"></a>
## Unfinished product acceptance

| Contract | Missing acceptance evidence / next action |
|---|---|
| T02 | Complete interactive credential-free Demo. The current synthetic preview supports project/Brief editing and historical views, but refuses Brief freezing, research execution and production claims. Exercise the complete demonstrable workflow in a browser without adding production bypasses. |
| T07/T08 | Operator login in a dedicated native profile, then real model → scientific Job/Evaluation → consumption in the same persistent Thread. Keep account material private and use a reviewed fixed commit. |
| T39 | Owner-selected legacy snapshot plus its artifacts, full mapping/precision/relationship report and explicit handling of unproven old qualifications. Existing projection import tests are insufficient. |
| T40 | Complete fresh-process backup/restore and fault rehearsal with database, private state, historical artifacts and Runtime reconciliation; measure the intended recovery objective. Existing native archive tests cover only part of this. |
| T41 | Execute the remaining environment-dependent README/CLI/Skill/runbook examples against the accepted deployment; record commands, outcomes and prerequisites. |
| T42 | Fresh instance through both Web and CLI: real research, independent qualification, at least two Alphas, portfolio evaluation, target package, Paper/Forward and applicable policy-driven behavior using authorized data. |

Unavailable account/data/backup inputs remain explicit gaps, not skipped passes. Follow-up maintenance may merge within its stated reviewed scope, but Issue #62 stays open until the complete contract, current-source evidence and post-merge verification are satisfied. No production deployment or full-product acceptance is implied by this index.
