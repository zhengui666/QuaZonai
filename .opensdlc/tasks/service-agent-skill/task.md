# Service-operating Agent skill

## Intent and baseline

Owner request: research excellent system-operation skills, particularly Lark CLI; define what the skill conveys and what native tools encapsulate; implement a skill for an Agent operating QuaZonai rather than developing it. Tracking: [Issue #102](https://github.com/zhengui666/QuaZonai/issues/102). Baseline: main `cd4b3e356b768a245358beb5e2b9c6e883f72d3d`; isolated branch `feat/service-agent-skill`. No language configuration exists at `.opensdlc/config.json`; English is the default.

## Design and boundaries

[Research](../../../docs/research/service-agent-skills.md) records primary sources, adopted mechanisms and rejected alternatives. The skill is portable, task-first and progressively loaded. Reuse the installed Rust CLI/native schemas and existing bound MCP; no new dependency, HTTP client, authentication flow or business engine. Add machine identity discovery, focused schema discovery and an offline redacted request preview. Keep contributor governance outside the runtime pack.

Preserve the existing human/Reviewer/Downstream separation, native Mission binding, immutable provenance, budgets, cancellation reconciliation and target-only delivery. Do not change production services/data or unrelated local work. Follow DESIGN's Actions-only execution and paid-account waiver; waived is NOT_RUN, not PASSED.

## Delivered implementation

[PR #103](https://github.com/zhengui666/QuaZonai/pull/103) contains the portable operational entry and six conditional references, the human installation guide, native identity/schema/preview commands, and regression tests. Existing contributor navigation no longer requires the runtime skill. The existing CLI/MCP transports and all domain authorization/scientific boundaries remain intact; no deployment or host-level skill installation is claimed.

## Acceptance and evidence

The PR's exact-Head Actions checks and independent read-only Codex review are the delivery record. Do not infer a pass from committed tests, a successful formatter, a queued workflow or an older Head. Every source change requires applicable CI on that new Head. Merge only after all applicable checks succeed and the current review has no unresolved findings; verify main after merge.

| Entry | Executable coverage |
| --- | --- |
| `make check-cli` | Existing native help traversal, installed-binary request previews and errors, schema discovery/reference closure, documented command shapes, isolated skill-directory installation and relative links |
| `cargo test --locked -p server --features native-codex` with disposable PostgreSQL/PGMQ | Existing native service regressions plus `client_identity`: real CLI process, HTTP/Bearer authorization, PostgreSQL identity and secret-free output |
| Original Rust/native, Store/database, Web, hosting, documentation and security CI | Applicable integration and regression checks on the exact PR Head; no weakened gate or replacement self-certified status |

On initial Head `3efe44f6bfdc117a0d9b8be3b748805f8a204867`, native compilation correctly rejected an old `Arguments` test initializer missing the added flag. Commit `e42657a4917fb2669933341bb27fda6b89f6f2f9` adds `preview: false` to that existing origin regression without disabling tests or changing production behavior. Subsequent results belong to their actual tested Head in the PR.

Actual model-driven [service-agent evaluations](../../evals/service-agent.md) remain **NOT_RUN**. Deterministic contract tests are not model rollouts. Dedicated paid/native-account acceptance is owner-waived/NOT_RUN, never PASSED. This task does not close Issue #62, certify the entire product or alter production services/data.
