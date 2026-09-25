# Fix mission-worker notification order

<a id="task"></a>
## Task

**Task ID:** fix-mission-worker-notification-order

**Original request / incident:** [CI run 36138597212, job 108082595412](https://github.com/zhengui666/QuaZonai/actions/runs/36138597212/job/108082595412).

**Agreed endpoint and merge / release scope:** Correct the failing regression assertion; open a PR into `dev`, merge only after current-head checks and review pass, then tag the merged dev revision.

<a id="intent"></a>
## Intent

**Problem, affected users/systems and desired outcome:** The `mission_worker` integration test assumes a fixed order between buffered token-usage and failed-turn notifications. CI observed the token-limit event before the persisted terminal observation, so the assertion failed.

**Scope, constraints and non-goals:** Regression test and the Store/Postgres CI job timeout only. Preserve assertions that over-budget partial usage requests cancellation and remains unsettled; do not change runtime behavior.

<a id="spec"></a>
## Requirements and design

**Expected behavior and observable acceptance:** In `failed_native_turns_late_partial_usage_still_closes_spending`, over-budget partial usage records the exact token-limit event and cancellation without settling usage; the actual terminal is failed or cancelled. The asynchronous Usage and TurnCompleted notifications have no required relative persistence timestamp. The full Store/Postgres suite must finish within its CI job timeout.

**Relevant interface, data, UX, permissions, errors and compatibility:** The driver may receive Usage and TurnCompleted together or separately; it persists the token-limit event and native terminal without a cross-event ordering contract.

<a id="plan"></a>
## Implementation plan

**Inspected flow, callers and existing reuse points:** Traced `apps/server/tests/mission_worker.rs::token_limit`, `apps/server/src/worker/mission/turn.rs`, and `crates/store/src/lifecycle/mission.rs::observe_mission_token_limit`.

**Ordered changes, exact paths and checks:** Remove the invalid timestamp-order assertion in `apps/server/tests/mission_worker.rs`; raise only the `store-postgres` job timeout in `.github/workflows/ci.yml` to 45 minutes after the full test job exceeded 30 minutes; rerun current-head CI and review.

**Risks, alternatives and engineering decision:** Current-head Codex review showed that same-batch notification draining can persist the terminal first, so the relative timestamp assertion is invalid; keep the behavioral assertions instead. The Store job timeout expired while tests were still passing, so extend its time budget without skipping tests or changing parallelism.

<a id="verification"></a>
## Verification

**Implementation and environment actually checked:** Initial failing GitHub Actions run at commit `2ec8284bf689172582b8eed0db44002e0ddfeec9`; follow-up run 36146290381 at head `028cccd4090beafee82043a92a2f93776a6bd531`.

**Commands / direct observations, actual results and native report links:** Initial `store-postgres` run failed in `mission_worker`; the timestamp-order assertion failed at line 2120. On the follow-up head, the corrected `mission_worker` test passed; the job was cancelled at its 30-minute timeout while later Store tests were still running. Other current-head checks passed. The 45-minute job rerun is pending.

**Defect reproduction before and after, where applicable:** Before: assertion failed on the recorded CI head. After: the target test passed on `028cccd`; the full suite did not finish before the old timeout. Verification with the extended timeout is pending.

**Failures, unrun checks and remaining coverage:** Post-fix CI and PR review pending.

<a id="review"></a>
## Review

**PR / reviewer and review scope:** [PR #119](https://github.com/zhengui666/QuaZonai/pull/119), current-head Code and Security review.

**Findings, resolutions, re-review and current CI:** Initial Codex review found that reversing the timestamp assertion still flakes when both notifications are drained together. The timestamp comparison was removed; behavioral assertions remain. Code and Security review passed on `028cccd`; the outdated thread is resolved. Review and CI for the timeout-adjustment head are pending.

**Outstanding findings and actual human approval / pending decision:** None known; current-head gates are pending.

<a id="delivery"></a>
## Delivery

**Actual PR / merge / deployment entry and observed state:** Pending.

**Release record, when a release is in scope:** No release; a dev tag is in scope.

**Unfinished work and next action:** Push the timeout adjustment, pass current-head review and CI, merge to `dev`, tag, and verify remote refs.

<a id="handoff"></a>
## Handoff

**Worktree / branch / uncommitted changes:** `/home/zzy/projects/QuaZonai/.ai-bridge/ci-36138597212`, branch `codex/fix-ci-36138597212` from `origin/dev`.

**Key decisions and current blockers:** Preserve the user's separate dirty worktree. PR merge and tag have not been performed.

**External actions to reconcile before retrying:** Query live PR head, checks and review before merging.

**Next action, resume commands and needed access:** Continue in this worktree; GitHub access is available.
