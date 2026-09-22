# Service-operating Agent skill

## Intent and baseline

Owner request: research excellent system-operation skills, particularly Lark CLI; define what the skill conveys and what native tools encapsulate; implement a skill for an Agent operating QuaZonai rather than developing it. Tracking: [Issue #102](https://github.com/zhengui666/QuaZonai/issues/102). Baseline: main `cd4b3e356b768a245358beb5e2b9c6e883f72d3d`; isolated branch `feat/service-agent-skill`. No language configuration exists at `.opensdlc/config.json`; English is the default.

## Design and boundaries

[Research](../../../docs/research/service-agent-skills.md) records primary sources, adopted mechanisms and rejected alternatives. The skill is portable, task-first and progressively loaded. Reuse the installed Rust CLI/native schemas and existing bound MCP; no new dependency, HTTP client, authentication flow or business engine. Add machine identity discovery, focused schema discovery and an offline redacted request preview. Keep contributor governance outside the runtime pack.

Preserve the existing human/Reviewer/Downstream separation, native Mission binding, immutable provenance, budgets, cancellation reconciliation and target-only delivery. Do not change production services/data or unrelated local work. Follow DESIGN's Actions-only execution and paid-account waiver; waived is NOT_RUN, not PASSED.

## Implementation plan

1. Replace the contributor-oriented entry with operational routing and self-contained references; document installation for the human provisioner.
2. Extend existing native entrypoints, not another client. Use the native DTO parser for preview and native-generated OpenAPI for schema closure; omit bodies/keys/grants from preview.
3. Add binary-level discovery/preview and portable-pack regressions plus an actual HTTP identity regression. Keep behavior evaluation separate from deterministic checks.
4. Run the narrow checks and applicable native/server/docs CI; resolve read-only review findings on the exact Head before merging. Preserve unexecuted scope honestly.

## Verification record

Implementation in progress. No passing build/test/review or product deployment is claimed by this initial entry. Actual commands, exact Head and outcomes will be recorded after execution. Behavioral scenarios are specified in [service-agent evaluations](../../evals/service-agent.md), initially NOT_RUN. This task does not close Issue #62 or certify the entire product.
