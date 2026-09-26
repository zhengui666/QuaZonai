# Simplify service interfaces and complete delivery

<a id="task"></a>
## Task

Implement the owner's attached CLI/MCP/Skill simplification recommendations and explicit instruction to complete and merge everything. Preserve all three interfaces and their distinct roles. The preceding password/device-login change was merged as [PR 123](https://github.com/zhengui666/QuaZonai/pull/123), main `5f7ead5f`; this task starts there. Delivery: [PR 124](https://github.com/zhengui666/QuaZonai/pull/124). Production deployment is not requested.

<a id="intent"></a>
## Intent

Reduce repeated HTTP implementation, model-supplied fixed identifiers, redundant previews and inconsistent command names. The attachment explicitly treats legacy import and direct Job commands as conditional deletion candidates, not proven dead code. Evaluate their real consumers and retain necessary behavior. Existing password/device login already removes repeated connection flags and per-operation grants for owner devices; scoped Mission/Reviewer/Downstream credentials retain their authority boundaries.

<a id="spec"></a>
## Requirements and design

- Keep one portable operational Skill, one service CLI and four bound Mission MCP tools. Runtime, Worker, Job and deployment retain their different responsibilities.
- Extract a small shared Rust HTTP module from existing CLI/MCP code: origin validation, sensitive bearer headers, native client setup, bounded responses and strict JSON/Problem parsing. Keep interface-specific deadlines, TLS inputs, output and authorization checks at their adapters. Do not use subprocess delegation, new dependencies or an SDK project. Never feed saved owner credentials to MCP.
- `research.get_brief` and `run.get` use the launcher's binding without model-supplied IDs. Proposal fields fixed by the Mission are filled by its adapter; native HTTP contracts and validation remain authoritative. Keep current Attempt, expiry, project/Cycle/Brief and response identity checks.
- Retain `--preview`, but use it for complex new requests, user-requested inspection, destructive operations or human review. Authorized routine operations may execute directly with original inputs, one idempotency key and real receipts. Condense repetitive Skill rules without merging its six focused references.
- Expose `quazonai` through the existing executable/install path without adding a second implementation. Reuse the saved connection and explicit overrides. Group weights under `forward weights submit` and messages under `forward messages submit`, preserving both functions and a deliberate compatibility path for existing scripts.
- Audit historical import and direct Job computation consumers. Delete only capabilities proven unused and outside supported compatibility; preserve database upgrades, backup/recovery, historical reads and scientific calculations. Record concrete retention/deletion evidence.
- Verify actual model behavior for the changed Skill with a small controlled baseline/candidate comparison and disposable inputs. Keep model evaluations separate from deterministic checks and read-only review.

<a id="plan"></a>
## Implementation plan

1. Parent owns shared HTTP extraction in `apps/server/src/client/{mod,session}.rs`, `mcp/client.rs` and the new shared module; preserve existing transport/security regression cases and add only missing behavioral coverage.
2. Isolated MCP work owns `mcp/{mod,requests}.rs`, its direct tests and native call fixtures: parameter-free bound reads and fixed proposal inputs, without changing shared transport.
3. Isolated CLI/Skill work owns command grouping, canonical executable installation, portable Skill wording and corresponding CLI/deployment checks. Coordinate MCP schema examples after the interface is settled.
4. A read-only audit traces legacy import and direct Job command consumers while implementation proceeds. Parent integrates isolated commits and records evidence.
5. Run focused native transport/MCP/CLI checks with a disposable database, deployment help/links and relevant model-driven cases; then applicable full CI and independent read-only review. Fix findings and recheck the exact final Head before the authorized merge. Verify merged main and resulting checks.

## Retained compatibility evidence

- Historical import remains a user-facing Settings workflow (`apps/web/src/pages/settings.tsx`, `apps/web/src/components/migrations.tsx`) and native CLI/API contract (`apps/server/src/migrations.rs`). `export-historical-rows` feeds its registered CSV exports; `export-historical-artifacts` preserves explicitly selected public files and the Sealed review boundary. `historical_import_http.rs` exercises export/import/download through real processes and HTTP. Source inspection is also reused by row export. These consumers do not establish that old installations are obsolete, so all historical commands and migrations remain.
- Direct Job `allocate` is called by the real Codex/native-science-thread acceptance path. Forecast, Alpha validation, Sealed evaluation, simulation and portfolio-study command tests cover native processes, malformed files, isolation and scientific results. The managed `job execute` path calls those same scientific implementations. Removing the direct wrappers would discard current process-boundary acceptance, so they remain. `verify-native` was already absent; no implementation remains to delete.
- Normal database migration, backup and cold recovery remain separate supported operations. Scoped grants, independent Reviewer and Downstream authority are retained; stale help implying every owner CLI write requires a one-shot grant was corrected.

<a id="verification"></a>
## Verification

- 95 local Rust test executions passed: 78 focused library/CLI/login/MCP checks plus 17 native-binary, official Codex and feature-enabled MCP-authoring checks. The latter includes an actual official Codex process owning MCP dispatch and resuming its original Thread against a controlled provider; it is distinct from model inference.
- Server Clippy passed with all targets and `native-codex`, warnings denied. Workspace formatting and diff whitespace checks passed.
- Deployment Python checks: 86 passed. Deployment documentation Node checks: 4 passed. Local Markdown check: 143 checked, 134 OK, 9 configured exclusions, zero errors.
- [Actual model comparison](../../evals/runs/service-interface-simplification.md): four paired baseline/candidate cases. Both create exactly one authorized project, stop after revocation, refuse owner fallback for a bound Mission, and retain explicitly requested preview. The candidate removes the routine preview; final real API read finds exactly the two intended projects. This is a bounded observation, not a general pass-rate claim.
- Full hosted final-Head gates and post-merge verification remain pending. Prior authentication checks belong to PR 123 and do not replace this task's checks.

<a id="review"></a>
## Review

Independent read-only reviewer `/root/interface_review` inspected the integrated CLI/MCP/Skill/packaging change and shared HTTP implementation and found no actionable behavioral/security regression. It did not execute tests. Hosted current-Head review and CI remain pending.

<a id="delivery"></a>
## Delivery

PR 123 is merged; this simplification is in progress. The owner's explicit endpoint is implementation and merge, with no production deployment.
