# Fix mission-worker notification order

<a id="task"></a>
## Task

**Task ID:** fix-mission-worker-notification-order

**Original request / incident:** [CI run 36138597212, job 108082595412](https://github.com/zhengui666/QuaZonai/actions/runs/36138597212/job/108082595412).

**Agreed endpoint and merge / release scope:** Correct the failing regression assertion; open a PR into `dev`, merge only after current-head checks and review pass, then tag the merged dev revision.

<a id="intent"></a>
## Intent

**Problem, affected users/systems and desired outcome:** The `mission_worker` integration test assumes a fixed order between buffered token-usage and failed-turn notifications. CI observed the token-limit event before the persisted terminal observation, so the assertion failed.

**Scope, constraints and non-goals:** Test and task record only. Preserve assertions that over-budget partial usage requests cancellation and remains unsettled; do not change runtime behavior.

<a id="spec"></a>
## Requirements and design

**Expected behavior and observable acceptance:** In `failed_native_turns_late_partial_usage_still_closes_spending`, over-budget partial usage records the exact token-limit event and cancellation without settling usage; the actual terminal is failed or cancelled. The asynchronous Usage and TurnCompleted notifications have no required relative persistence timestamp.

**Relevant interface, data, UX, permissions, errors and compatibility:** The driver may receive Usage and TurnCompleted together or separately; it persists the token-limit event and native terminal without a cross-event ordering contract.

<a id="plan"></a>
## Implementation plan

**Inspected flow, callers and existing reuse points:** Traced `apps/server/tests/mission_worker.rs::token_limit`, `apps/server/src/worker/mission/turn.rs`, and `crates/store/src/lifecycle/mission.rs::observe_mission_token_limit`.

**Ordered changes, exact paths and checks:** Correct the timestamp expectation in the existing regression test; run the CI job and review the exact PR head.

**Risks, alternatives and engineering decision:** The first fix reversed the original timestamp comparison. Current-head Codex review showed that same-batch notification draining can persist the terminal first, so the relative timestamp assertion is invalid; keep the behavioral assertions instead.

<a id="verification"></a>
## Verification

**Implementation and environment actually checked:** Initial failing GitHub Actions run at commit `2ec8284bf689172582b8eed0db44002e0ddfeec9`.

**Commands / direct observations, actual results and native report links:** Initial `store-postgres` run failed only in `mission_worker`; exact assertion failure was `terminal.observed_at <= event.occurred_at` at line 2120. Other test binaries completed successfully.

**Defect reproduction before and after, where applicable:** Before: assertion failed on the recorded CI head. After: pending current-head CI.

**Failures, unrun checks and remaining coverage:** Post-fix CI and PR review pending.

<a id="review"></a>
## Review

**PR / reviewer and review scope:** [PR #119](https://github.com/zhengui666/QuaZonai/pull/119), current-head Code and Security review.

**Findings, resolutions, re-review and current CI:** Initial Codex review found that reversing the timestamp assertion still flakes when both notifications are drained together. The timestamp comparison is removed; behavioral assertions remain. Re-review and CI for the new head are pending.

**Outstanding findings and actual human approval / pending decision:** None known; current-head gates are pending.

<a id="delivery"></a>
## Delivery

**Actual PR / merge / deployment entry and observed state:** Pending.

**Release record, when a release is in scope:** No release; a dev tag is in scope.

**Unfinished work and next action:** Complete local check, push, PR review and CI, merge to `dev`, tag, and verify remote refs.

<a id="handoff"></a>
## Handoff

**Worktree / branch / uncommitted changes:** `/home/zzy/projects/QuaZonai/.ai-bridge/ci-36138597212`, branch `codex/fix-ci-36138597212` from `origin/dev`.

**Key decisions and current blockers:** Preserve the user's separate dirty worktree. PR merge and tag have not been performed.

**External actions to reconcile before retrying:** Query live PR head, checks and review before merging.

**Next action, resume commands and needed access:** Continue in this worktree; GitHub access is available.
