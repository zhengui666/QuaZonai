# Project review policy

<a id="scope"></a>
## Scope and basis

[AGENTS](../AGENTS.md) owns development authority; [DESIGN](../DESIGN.md) owns product contracts. [@zhengui666](https://github.com/zhengui666) is the code/technical owner. Review the task's declared scope, actual implementation and checks; generated output is checked against its source rather than styled by hand.

<a id="focus"></a>
## Review focus

Trace shared callers, ownership and data flow. Check input validation, transaction/immutability boundaries, authorization, budget/evidence association, retry/cancellation and failure recovery. Protect credentials and private data under DESIGN 0.3 without inventing additional security projects or merge gates.

Compare behavior with the task intent/plan and DESIGN. Prefer existing/native capabilities; flag unnecessary layers and legacy compatibility. Check that tests would detect the failure, expectations were not weakened, and docs state only real commands and capabilities. CI format/generated checks do not need duplicate stylistic review; semantics still do.

<a id="severity"></a>
## Severity and response

| Finding | Response |
| --- | --- |
| Incorrect behavior, data/credential exposure, violated product contract, misleading evidence, broken required check | Fix before merge, validate the new Head and request re-review |
| Unclear maintainability or incomplete requirement with concrete impact | Resolve with the code owner; fix or explicitly agree on scope before merge |
| Optional style/preference without behavioral impact | Identify as a suggestion, not a defect; avoid churn already handled by formatters |

<a id="approval"></a>
## Review and approval

The owner's standing [authorization](../AGENTS.md#开发权限) permits a scoped merge after all applicable latest-Head CI succeeds, all review threads are resolved, and `@codex review` explicitly reports no issues on that Head. A reaction, silence, pending/cancelled/skipped/failed run, quota error or old review does not pass. GitHub Codex reviews; the local author fixes. Any new commit restarts the applicable checks and review cycle. Use an expected-Head merge and reread main afterward.

Maintainer decisions are required for changed product requirements, significant architecture, license changes or production release; authors cannot manufacture approval. The current task's explicit CI/review endpoint is recorded in [its intent](tasks/open-source-foundation/task.md#task). No general requirement is inferred that an unrelated product Issue must close before a scoped maintenance PR merges.

**Observed platform state, 2026-09-17:** main's branch-protection API returned `404 Branch not protected`. CODEOWNERS exists but does not itself enforce review. These rules are presently an owner-authorized procedure, not a claimed non-bypassable GitHub protection. Repository permission changes remain the owner's administrative decision; recheck settings before relying on them.

GitHub PR comments, reviews, threads and check runs are the canonical review results. Link them in the task; do not maintain a second approval ledger.

<a id="quality"></a>
## Finding quality

The code owner should review repeated findings and false positives monthly, using existing PR history. This is a manual maintenance recommendation, not an installed scheduler. Add recurring factual mistakes to project context or the evaluation suite; retain useful regression tests and avoid counting generated/style noise as new defects.
