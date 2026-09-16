# Engineering operations

Actual product commands and recovery procedures remain in [OPERATIONS](../OPERATIONS.md), [CLI](../CLI.md) and [Runtime](../runtimes/native/README.md). This page owns development/release coordination, not another operational state machine.

<a id="controls"></a>
## Action boundaries

| Action | Actual mechanism and decision route | If unavailable |
| --- | --- | --- |
| Edit/test a development change | Owner authorization in [AGENTS](../AGENTS.md); isolated checkout and disposable test resources | Preserve others' work, record the missing prerequisite and continue independent checks |
| Merge a PR | [Current-Head review procedure](review.md#approval), GitHub CI/reviews and expected-Head merge | Fix the failure or wait for the actual review; never infer success |
| Product approval or target delivery | Product identity/transaction contracts in DESIGN A7/A8 and their existing tests | Use the real Operator/Reviewer/Downstream route; development permissions grant no product identity |
| Deploy, migrate or restore user data | Release/service owner decision; explicit native commands in OPERATIONS/CLI | Prepare the reviewed artifact/runbook; request only missing consequential authority/input |
| Read account/secret material | Native private stores and scoped product interfaces | Keep secret values out of source, logs, chat and model context |

CI uses native GitHub permissions and test identities. Read [workflow configuration](../.github/workflows) for exact scopes. Main was observed without branch protection on 2026-09-17; prose and CODEOWNERS are not native enforcement. No new host hook or production automation is claimed here.

<a id="delivery"></a>
## Delivery and recovery

Development is authorized within task scope. Ordinary PR CI uses fixtures/disposable databases; it does not receive production account material. There is no repository-configured staging/production deployment pipeline in the current workflows. Production preparation and production execution are separate decisions.

After a scoped merge, verify the merge commit and its main CI. A task records its PR and actual result; a formal release additionally needs a release artifact/environment, owner authorization, changes/migrations, actual rehearsal, observation and rollback evidence. Create `.opensdlc/releases/<release-id>.md` only for such a real release, linking any authoritative GitHub release.

For a documentation/test-only regression, prepare a revert PR through the same review process. For product or data changes, follow [migration transaction boundaries](../OPERATIONS.md#完整迁移命令的提交边界), [Run recovery](../OPERATIONS.md#run-admission--attempt--sse-运维边界) and [private data/key handling](../OPERATIONS.md#数据和密钥). A source revert does not undo a database migration or recover secrets. No production rollback rehearsal or RPO/RTO is asserted by this adoption; remaining restore evidence stays in the [acceptance index](../docs/architecture/issue-62-execution.md#acceptance).

<a id="observe"></a>
## Observe and respond

GitHub `pull_request` and main `push` events run the committed checks; CodeQL also has its existing schedule. A required check with any result other than success deterministically blocks the delivery procedure. This is the first actionable maintenance signal; no model interprets a failure as green. A maintainer reads the failing job, reproduces the narrow check and opens/fixes the associated task.

The service owner is the repository owner. Production liveness, queue/reconciliation and recovery observations follow OPERATIONS; `/health/live` alone does not prove research or delivery readiness. No automatic production alert/repair service is installed here. A request to monitor later requires a real configured trigger and its own authority.

Record a real incident in the existing Issue or `.opensdlc/incidents/<incident-id>.md` only when one occurs. Capture impact, facts, root cause, recovery and the fix task; keep full native logs at their source and sanitize credentials. Add a focused regression and, when agent behavior contributed, a case in the [evaluation suite](evals/suite.md).

<a id="metrics"></a>
## Measures and improvement

Use GitHub PR/check timestamps and findings to inspect time to first passing CI, repeated failures, time to clean review and recurring escaped defects. No synthetic baseline or dashboard is created. The owner can inspect these during monthly review and after an incident; a repeated issue becomes a specific fix or evaluation case, not another standing report.
