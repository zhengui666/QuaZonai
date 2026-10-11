# Partial managed-capital exits

Use only the installed CLI and its current authorized identity. This is a separate
version-1 account-control workflow, not a target-package version or Alpha. QZ does
not hold venue credentials, submit withdrawals or promise zero loss. A supported
external native owner can cancel selected opening work and submit authorized
price-bounded reductions. A reservation prevents that owner's reinvestment; it is
not a wallet lock against the user or another program.

Current execution capability supports only `AMOUNT`. New `PORTFOLIO_SCOPE`
previews are unsupported and the CLI rejects them before sending. The wire
contract still represents historical portfolio scopes: list/show retain their
stream/release IDs, multiple reduction legs and evidence for read-only inspection.
Their presence is not executable capability; never convert them to an amount
scope or start/resume an unsupported historical preview.

## Resolve and preview

1. Resolve the exact project, account source, native account and environment using
   `quazonai client forward accounts sources PROJECT_ID` and
   `quazonai client forward accounts current PROJECT_ID SOURCE_ID`. Read the native
   evidence identity/time; a recent heartbeat does not refresh financial evidence.
   PAPER/Sandbox is simulation. A displayed LIVE account alone establishes no exit
   capability. Do not substitute a different account or broaden the requested scope.
2. Inspect `quazonai client forward accounts exits preview --help` and
   `quazonai openapi --schema CapitalExitPreviewRequestV1` as needed. Keep amounts,
   quantities and revisions as exact decimal strings, never binary floating point.
3. Prepare only `AMOUNT` with the exact requested amount/currency. Alphas share
   net account holdings; they are not
   independent withdrawable lots. Select `CASH_ONLY`, or obtain the user's explicit
   `BOUNDED_LIMIT` price floors, maximum quantities, deadline, maximum incremental
   execution cost and original reference evidence. Never choose or widen these
   financial limits for the user. Missing required facts remain unavailable.
4. Send the typed JSON on stdin with one retained key:
   `quazonai client --idempotency-key ORIGINAL_PREVIEW_KEY forward accounts exits preview --project-id PROJECT_ID`.
   This performs a server evidence preview, not execution. The separate generic
   `--preview` flag validates local syntax/transport and sends no server request;
   it cannot establish capability, fresh evidence or a cash-release plan.
5. Read the immutable plan and its SUPPORTED/UNSUPPORTED/BLOCKED capability/reasons.
   Show requested amount, verified idle cash, required reductions, native cash and
   equity, managed capital before/after, proposed opening-order cancellations,
   retained protective orders, costs, pre-existing unrealized PnL, remaining risk,
   source evidence and expiry. Unknown is not zero. Existing PnL is not incremental
   execution cost; the cost cap is not insurance against all realized losses.

## Approval and execution

Before start, obtain explicit authorization for the exact account/environment,
amount/currency/scope, immutable plan and any quantity/price/cost/deadline bounds.
Start/resume can activate native reduction orders. The deadline only pauses and
preserves capital; there is no automatic market/forced-exit fallback. Do not start
an unsupported or blocked plan. Any changed or expired plan needs refresh and
approval of material changes, never silent acceptance of a replacement.

Use `CapitalExitStartV1` with the returned preview ID, plan artifact ID, original
observation ID and account-control revision. Retain the body/key before invoking:
`quazonai client --idempotency-key ORIGINAL_START_KEY forward accounts exits start --project-id PROJECT_ID`.
A 202 response means the service accepted the request, not that native reductions
or cancellation completed. Start and resume pass the original preview ID to the
server's admission rules; the CLI performs no extra GET and never rewrites its
scope. Follow its original intent ID:

- `quazonai client forward accounts exits list --project-id PROJECT_ID`
- `quazonai client forward accounts exits show INTENT_ID`

Preserve returned cursors. On timeout, disconnect or incompatible response, follow
[recovery](recovery.md): query the original operation and replay the unchanged body
with the same key if needed. Never use a new key or change the limits to disguise
an uncertain outcome. A source/session/project alias cannot create a fresh budget.

## Pause, cancel, continue and manual withdrawal

Each command below takes `CapitalExitActionV1` JSON on stdin and its own retained
idempotency key. The top-level action tag must match the selected command.

- `quazonai client forward accounts exits pause INTENT_ID`: `PAUSE` requests that
  the native owner stop new reductions and resolve/cancel this exit's unfilled
  orders; its reserve remains. 202 is acceptance, not proof the orders stopped
- `quazonai client forward accounts exits cancel INTENT_ID`: `CANCEL` cancels
  future exit work; CANCELLED_RESERVED retains capital. Filled trades remain;
  cancellation does not buy back assets, undo fills or restore reinvestment.
  202 is acceptance; original native cancellation evidence is still required
- `quazonai client forward accounts exits resume INTENT_ID`: `RESUME` requires a
  valid refreshed preview for the same remaining exit and expected revision. It
  continues approved reductions, never new additions or buybacks. 202 is acceptance,
  not completion, and does not relax the approved price, quantity, cost or deadline
- `quazonai client forward accounts exits reconcile INTENT_ID`:
  `RECONCILE_WITHDRAWAL` reports the user's own amount/currency and optional original
  transfer reference for verification. It neither initiates a transfer nor proves
  the report. Wait for matching original native cash-movement evidence; a balance
  drop alone could be fees, PnL or another transfer. Partial withdrawal retains the
  remaining reserve and must not subtract the amount twice from observed cash

`PAUSED`, even with an empty `reason_codes`, does not prove all native orders have
stopped. A `FenceApplied` report can clear pending reasons while the exit remains
paused; it proves the reported strategy gate, not order cancellation. Keep the
funds reserved and inspect the original owner command, fence coverage and order
evidence separately. `capital_exit_owner_command_pending` means the owner
instruction still needs native application evidence; `capital_exit_owner_fence_pending`
means the original native gate still needs coverage evidence. Cancellation remains
pending until the original exit-owned orders have native closure/cancellation
evidence; an accepted command or cleared reason is insufficient.
`capital_exit_deadline_pause_evidence_pending` means the bounded deadline has
elapsed while pause evidence is still missing. It never extends the deadline or
permits a market/forced-exit fallback. Unknown or late native effects remain
explicit; do not assume a pause/cancel request, heartbeat or local process exit
has stopped them.

Immediately before the user withdraws manually, refresh the original intent's
available-cash evidence and have them check the venue's current withdrawal screen.
GET is read-only: it does not manufacture fresh native proof. Missing/expired proof
is UNVERIFIED/STALE even if a previous amount was valid. SIMULATED cash is always
reported as simulated available cash, never money verified for real withdrawal.
No withdrawal command, address entry, direct venue API, credentials, fallback
shell execution or autonomous limit change belongs in this procedure.

## Receipt

Return original intent ID, current state/last phase/reasons, requested amount,
reserved amount, released cash, verified withdrawable amount (or unavailable),
reconciled amount, original evidence time/expiry, PAPER/Sandbox or LIVE label, and
the next required action. Partial, blocked and unknown outcomes stay explicit.
Do not say preview, ACK, order acceptance, a cash snapshot or 202 means withdrawal
completed. If remaining trading is FENCED_PENDING_TARGET, disclose that it cannot
resume additions until a separately qualified compatible target exists. Live
acceptance and native business-path testing are distinct from simulated release.
