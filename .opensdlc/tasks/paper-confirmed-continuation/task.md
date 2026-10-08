# Continue acknowledged Paper control work without idle delays

## Evidence and boundary

Follow-up to [PR #179](https://github.com/zhengui666/QuaZonai/pull/179).
Actual merge source `3ef0269e2e4337ac13087a325d516d45444a2316` failed the two
pause/resume and cancelled/resume bridges in
[Rust CI 37768534815](https://github.com/zhengui666/QuaZonai/actions/runs/37768534815).
Its 1,727 source paths, bytes and executable modes matched the reviewed source.
The prior first-Availability priority fix did execute: both traces contain a fresh
same-command sequence-15 `WITHDRAWABILITY_OBSERVED` POST with 850 pUSD. The
12-second fixture wait expires 132 ms and 109 ms after those POSTs respectively;
no corresponding response is retained. This is not proof of acceptance or rejection.
Passing PR CI retained case results but no successful HTTP traces.

The resumed chain still performs Claim, Fence, original observations, native
REDUCING, full historical assessment discovery, native WAITING_EVIDENCE and
Availability. It unconditionally sleeps 250 ms between confirmed intermediate
steps as well as actual polling/retries. Observed response medians were about
551/558 ms; needless intermediate sleeps consume time inside the same control
chain. Historical discovery filtering is deliberately outside this patch.

The nested native owner-control report's FENCING state is the original retained
same-epoch authority snapshot. It is intentionally not overwritten by later
lifecycle projections. Current binding, command, epoch, source observation,
sequence, acknowledged fence, native freshness and availability evidence remain
validated; the nested historical state is not the outer availability outcome.

## Scheduling change

Only a successful ACK in the current step permits immediate continuation:

- Claim, which next requires a fresh intent read and the corresponding Fence
- First Fence for the exact command
- Original observation binding, once until another accepted non-observation outcome
- First native WAITING_EVIDENCE transition for the exact command

Every continuation still enters the original step function, which replays an
unknown pending write first or reads the current intent before new native actions.
New Pause/Cancel commands therefore retain claim/fence priority. Observations,
all historical discovery pages and all native/store validation remain unchanged.

Idle turns, assessments, REDUCING/repeated progress, terminal stop progress,
Availability, consecutive observation-only ACKs and every error retain 250 ms
pacing. New timestamps, sequences or updated-at fields do not grant continuation.
No business request is rejected or evidence discarded to impose a burst limit.
A normal continuation ends at a paced native progress, assessment, Availability
or error; an observation-only stream cannot keep renewing immediate execution.

The 12-second bridge wait, all policy deadlines and native freshness windows are
unchanged. No production mock, authentication shortcut or order route is added.

## Validation

The scheduling regression uses the actual Poller run loop and transport with a
synthetic loopback/native-inbox protocol fixture. Every HTTP request waits 550 ms.
It retains both irrelevant history pages and original observation refresh, then
requires the full Resume control chain, Availability ACK consumption and the next
fresh intent request within the unchanged 12-second budget. This is scheduling
coverage, not a real account, native fill or PostgreSQL acceptance result.

Separate cases cover inserted Pause and Cancel instructions, repeated native
WAITING_EVIDENCE with changed timestamps/sequences, consecutive observation-only
ACKs, and existing native/definite/unknown failure recovery with exact pending
path/key/body replay. The original six official-native Paper cases and real-PG
bridge definitions remain intact.

Real PostgreSQL execution at the exact repair/integration head is still required.
The absence of the failed Availability response cannot be repaired by inference
from synthetic tests. Full Rust regression remains required for publication.
