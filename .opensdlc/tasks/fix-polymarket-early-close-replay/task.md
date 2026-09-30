# Close native trading before early Polymarket settlement

<a id="task"></a>
## Task

Task ID: `fix-polymarket-early-close-replay`. Source: the authorized September 30,
2026 early-resolution audit and minimal implementation request. Continue
[PR #136](https://github.com/zhengui666/QuaZonai/pull/136) from head
`5fbcf8c85ed82e9a584670ac71762e3a85ed3fcd`. The endpoint is all applicable CI passing
on the final head, resolved findings and explicit clean GitHub Codex review, then
merge that head into remote `dev` and read back the merge.

<a id="intent"></a>
## Intent

An original Polymarket definition can retain a scheduled expiration later than
the actual resolution. The official native `ContractExpired` close can redeem
positions early, but that close alone does not set the market status to closed.
It can also match resting orders before canceling them. The replay must close
native trading before passing the verified resolution to native settlement.

The motivating R6 market `4948593` retained a scheduled expiration of
`2026-09-30T15:10:00Z` in its 09:44 Gamma receipt. Its two unique captured prints
were at 09:53 and 09:57. The subsequent resolution API reported 11:51:03Z and was
actually received at 12:35:04.226Z. These records expose the lifecycle requirement;
they do not prove an actual MARKET order or a failed QZ simulation.

Local original evidence is under the primary checkout's
`.ai-bridge/polymarket-history/small-contract-continuation/`: the original
`forward-75m-20260930T084044Z/definitions/04-4948593-gamma.body.json` and receipt,
`current-state-20260930T123146Z/resolution-current.body.json` and receipt, and
`resolution-chain-read-20260930T125253Z/current-state-observation.derived.json`.
The independent current public RPC observation at 12:59:37Z used finalized block
`94712021`, hash `0x1a16ac1d30ea5a191c3b0eed848ebcb40350b7a67f8e7e3197c7501bf2f929a3`,
and confirmed current payout fractions `1/1, 0/1` for R6. Historical state queries
were unavailable; current state does not establish historical availability or a
historical resolution transaction proof.

The personal official API research-use evidence is recorded as native OPERATOR
REPORT `01a0f257-64de-7952-8563-e07fa881c961`, with local evidence in
`research-record/personal-api-source-use-20260930T124328Z/report.json`. This patch
creates no corresponding DataSource, DataGrant or Dataset registration. The small
contract evidence remains unregistered, with no scientific qualification, Alpha
or portfolio acceptance.

<a id="spec"></a>
## Requirements and design

Use the already locked official Nautilus 0.63.0 public `InstrumentStatus` API.
For each source-verified `ContractExpired` close selected for replay, place a
derived `Close` status immediately before its own close in the same native event
vector, retaining its exact instrument ID, `ts_event` and `ts_init`. This is a
resolution companion, not a separately received source status. Native settlement
still owns redemption orders, positions, cash and zero trading commission.

Reject any selected close whose receipt time equals a BAR receipt time anywhere
in the selected portfolio. Existing native catalogs do not retain enough
cross-type arrival information to choose which occurred first, and a BAR for
another member can trigger portfolio orders. Do not change timestamps or choose
an undocumented cross-source tie order.

Keep original scheduled expiration, BARs, raw archives, frozen V1 contracts,
dependencies, whole-order floor, collateral and the existing partial-fill fee
bound. There is no general statuses ingestion architecture or new HTTP resolution
mapper. Retain `NATIVE_TARGET_REPLAY_FAILED` while attaching the native generated
order-rejection reason needed to diagnose a market-closed rejection.

<a id="plan"></a>
## Implementation plan

`simulation::run` is the sole caller of `prediction::close_events`.
`prediction::catalog_closes` is shared with `catalog::measure_catalog`, which
operator preparation and managed DATA_VALIDATE both use. Add portfolio-wide
receipt-tie rejection after the shared frozen source inventory checks in
[prediction](../../../apps/job/src/prediction.rs). Add status companions and retain
the internal rejection diagnostic in [simulation](../../../apps/job/src/simulation.rs).
Use one regression in [the existing Polymarket module](../../../apps/job/tests/polymarket.rs)
to check early redemption, a new order after close rejected specifically because
the market is closed, and ambiguous same-member and other-member receipt ties.

The existing fixture catalog, complete frozen settlement group, native Parquet
types and one-job-per-process helper are reused. Synthetic fixture clocks remain
explicitly separate from the motivating real evidence. No upstream source,
service, collector or registered dataset is changed.

<a id="verification"></a>
## Verification

Static inspection confirmed the official resolver's status-before-close sequence
and stable native receipt-time sorting. The new regression must execute on the
final hosted head together with the existing 13 Polymarket tests. Local tests,
builds and installs are not run. The first `cargo fmt --all -- --check` found only
new-code formatting differences; these were corrected. The final format check
and `git diff --check` both exited 0. No observed pre-fix regression run or
current-head CI success is claimed.

<a id="review"></a>
## Review and delivery

The root agent owns independent review, PR #136 publication, final-head hosted CI
and read-only GitHub Codex review. The existing head's green CI and earlier review
requests do not verify this amendment. Merge and delivery remain pending.

<a id="handoff"></a>
## Handoff

Worktree: `/home/zzy/.codex/worktrees/polymarket-partial-fill-fees/QuaZonai`, branch
`codex/fix-polymarket-partial-fill-fees`. The authorized local endpoint is an
uncommitted, reviewable patch with format and diff checks. No local execution or
dataset admission is part of this implementation.
