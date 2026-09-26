# Simplify service interfaces and complete delivery

<a id="task"></a>
## Task

Implement the owner's attached CLI/MCP/Skill simplification recommendations and explicit instruction to complete and merge everything. Preserve all three interfaces and their distinct roles. The preceding password/device-login change was merged as [PR 123](https://github.com/zhengui666/QuaZonai/pull/123), main `5f7ead5f`; this task starts there. Production deployment is not requested.

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

<a id="verification"></a>
## Verification

Not run for this task yet. Prior authentication checks belong to PR 123 and do not establish this refactor's behavior.

<a id="review"></a>
## Review

Pending integrated independent review, hosted current-Head review and CI.

<a id="delivery"></a>
## Delivery

PR 123 is merged; this simplification is in progress. The owner's explicit endpoint is implementation and merge, with no production deployment.
