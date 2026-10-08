# Partial managed-capital exits

Use only the installed CLI and its current authorized identity. This is a separate
version-1 account-control workflow, not a target-package version or Alpha. QZ does
not hold venue credentials, submit withdrawals or promise zero loss. A supported
external native owner can cancel selected opening work and submit authorized
price-bounded reductions. A reservation prevents that owner's reinvestment; it is
not a wallet lock against the user or another program.

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
3. Prepare `AMOUNT`, or the exact `PORTFOLIO_SCOPE` stream IDs/release IDs plus the
   requested amount/currency. Alphas share net account holdings; they are not
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
A 202 response means requested. Follow its original intent ID:

- `quazonai client forward accounts exits list --project-id PROJECT_ID`
- `quazonai client forward accounts exits show INTENT_ID`

Preserve returned cursors. On timeout, disconnect or incompatible response, follow
[recovery](recovery.md): query the original operation and replay the unchanged body
with the same key if needed. Never use a new key or change the limits to disguise
an uncertain outcome. A source/session/project alias cannot create a fresh budget.

## Pause, cancel, continue and manual withdrawal

Each command below takes `CapitalExitActionV1` JSON on stdin and its own retained
idempotency key. The top-level action tag must match the selected command.

- `quazonai client forward accounts exits pause INTENT_ID`: `PAUSE` stops new
  reductions, resolves/cancels this exit's unfilled orders and keeps its reserve
- `quazonai client forward accounts exits cancel INTENT_ID`: `CANCEL` cancels
  future exit work; CANCELLED_RESERVED retains capital. Filled trades remain;
  cancellation does not buy back assets, undo fills or restore reinvestment
- `quazonai client forward accounts exits resume INTENT_ID`: `RESUME` requires a
  valid refreshed preview for the same remaining exit and expected revision. It
  continues approved reductions, never new additions or buybacks
- `quazonai client forward accounts exits reconcile INTENT_ID`:
  `RECONCILE_WITHDRAWAL` reports the user's own amount/currency and optional original
  transfer reference for verification. It neither initiates a transfer nor proves
  the report. Wait for matching original native cash-movement evidence; a balance
  drop alone could be fees, PnL or another transfer. Partial withdrawal retains the
  remaining reserve and must not subtract the amount twice from observed cash

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
