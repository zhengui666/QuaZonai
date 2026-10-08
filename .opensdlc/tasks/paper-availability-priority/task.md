# First Paper availability before assessment discovery

## Source and failure

Follow-up to [PR #178](https://github.com/zhengui666/QuaZonai/pull/178).
The actual merge source at `6d9bef77700ab4c4b4c31eaf6d926d7037d01e18`
failed the pause/resume and cancelled/resume Paper bridges in
[Rust CI 37758394124](https://github.com/zhengui666/QuaZonai/actions/runs/37758394124).
All 1,726 source paths, bytes and executable modes matched the reviewed source.
The preceding [PR CI 37754964070](https://github.com/zhengui666/QuaZonai/actions/runs/37754964070)
passed those cases, but its exported artifacts do not contain successful per-request
traces; only the whole-case durations can be compared.

Both failure traces contain an accepted original native `WAITING_EVIDENCE`
progress report for the resumed command (control epoch 3, account revision 4,
intent revision 14), released cash of 850 pUSD and native free cash of
850.001376 pUSD. Neither contains an Availability submission. The Poller instead
fetches another multi-page history of immutable blocked preview receipts before
Availability. Its first page contains 50 obsolete revision-1/revision-3 requests;
the existing 12-second final wait expires while page two is pending. Native report
freshness and the policy deadline remain valid at failure. This is a scheduling
failure; no assertion is relaxed and no timeout is treated as successful evidence.

## Change

After an acknowledged native `WAITING_EVIDENCE` progress report, give the same
command's first Availability attempt priority over assessment discovery. Restore
discovery before making that attempt, so a native failure or definite HTTP
rejection cannot permanently starve a later Resume preview. An ambiguous write
remains pending and replays its exact original path, key and body before any read.

Keep the existing nonpending intent/claim/fence priorities, observation refresh,
exact-command readiness guard, STOP/CANCEL behavior, policy deadlines, native
freshness, receipt identities and official Nautilus execution unchanged. Do not
add a new page cap, an execution shortcut, a production mock or an order route.

## Verification

Four synthetic protocol scheduling tests execute the real Poller step and
transport against an isolated loopback DTO responder. They cover successful ACK,
native failure before pending evidence, definite 422 rejection, and an unknown
truncated response followed by exact original replay. They assert that later
assessment discovery resumes; they are not native-account or real-PG evidence.
The two existing Poller assessment/replay unit tests remain selected with them.

The unchanged real-PG Paper scenarios additionally assert that the first
same-command Availability attempt occurs before any assessment-discovery GET
following an accepted original native `WAITING_EVIDENCE` report. Existing late
Resume, original engine clock/session, fill/fee, cancellation, source identity and
receipt-replay assertions remain intact, including the 12-second final wait.

Real-PG execution and exact-head remote CI remain required. Local protocol tests
or successful compilation do not replace those gates. Full Rust regression remains
required before image publication; this task does not authorize a release.
