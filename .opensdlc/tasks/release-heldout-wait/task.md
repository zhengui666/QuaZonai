# Wait for terminal independent evaluation during release checks

<a id="task"></a>
## Task

**Task ID:** release-heldout-wait

**Source:** Dev release [40612600882](https://github.com/zhengui666/QuaZonai/actions/runs/40612600882) at `35c2d1af` failed the existing `deploy/docker/smoke.py:89` final-result assertion, printing `Stop before heldout`. Restoring the release path is necessary for the owner's ongoing Hyperliquid research delivery.

**Endpoint:** Fix the existing polling race and deliver through review and CI to `dev`. Preserve the required final `COMPUTED` result and actual independent assessment; do not change provider behavior, native science, installed services or user data.

<a id="intent"></a>
## Intent

`finalizeFactor` records `STARTED` and the selection reason before awaiting the independent native task, then records `COMPUTED` or `FAILED`. The release poller instead treated every non-null final record as terminal and immediately asserted `COMPUTED`. A normal in-progress result therefore reproduces the release assertion. The historic log printed only the reason, so it does not establish that the native final status was `STOPPED`.

The provider in both the failed source revision and current `dev` calls `factor_finalize`; no packaging or tool-name change is needed for this verified checker race.

<a id="spec"></a>
## Requirements and design

Keep polling while the final record is absent or `STARTED`, within the existing poll limit. Continue rejecting terminal `STOPPED` and `FAILED` immediately. Retain the `COMPUTED` assertion and include the actual final status in its diagnostic alongside the reason, avoiding the prior ambiguity.

<a id="verification"></a>
## Verification

Before the change, importing the existing Python module and returning `STARTED` followed by `COMPUTED` reproduced `AssertionError: Stop before heldout` on the first response. After the change, regressions in the existing `release_test.py` cover that transition and immediate failure for both terminal noncomputed statuses. No new fixture files or release checks were introduced.

`python3 -B -m unittest discover -s deploy/docker -p release_test.py`: 10 tests passed. `git diff --check` passed. No manual Docker release check or native compilation was run for this Python-only fix; the existing applicable CI and subsequent actual release remain pending.

<a id="handoff"></a>
## Handoff

Managed worktree `/home/zzy/.codex/worktrees/release-heldout-protocol/QuaZonai`, branch `fix/release-heldout-wait`, based on remote `dev` at `0e2bce91e7e3b2779014b46fc6a8a9baae9da7bf`. The original checkout, live containers and full raw archive were not changed. This release fix does not consume factor trial budget or establish investment performance.
