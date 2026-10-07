# Optional application execution budgets

Base: dev ca3928bc26358131775ac9c9f92a61ed13b7aca1. No Issue or PR has been published for this candidate.

## Goal

Represent absent application wall-time, cumulative CPU, token and task-output budgets explicitly. The user confirmed all four may be absent. Preserve finite legacy requests, actual observations, memory limits, finite CPU occupancy, cancellation, exact ownership leases, unknown-send reconciliation, native resource failures and scientific eligibility. No maximum integer or far-future date represents absence.

## Contract and execution

- Budget and job wall/CPU/output limits, plus Run/JobSpec/turn deadlines, are nullable. Existing positive values keep their meaning. Actual output byte counts remain mandatory; unknown CPU observations remain null.
- Runtime advertises optional-wall-time/1, optional-cpu-budget/1 and optional-output-budget/1 only after the matching pinned native-image contract is verified. Old images/runtimes cannot silently relax limits. No-wall with finite cumulative CPU is explicitly unavailable until an independently surviving CPU enforcer exists; no-wall with absent CPU uses exact-owner resource recovery.
- Absent wall/CPU/output skips the corresponding task watchdog, CPU ulimit and file-size/task-total budget. Memory, finite CPU rate, network/startup/cleanup timeouts, cancellation, parser/frame/object format bounds and real storage exhaustion still apply.
- Mission resource records identify run, attempt, owner epoch and each physical resource. Recovery verifies current authority and reclaims only the exact stale owner resource. Unknown historical CPU blocks a finite grant; an absent CPU grant preserves unknown accounting rather than manufacturing zero.
- Native credentials remain scoped to the original run/attempt/owner. No-deadline Missions use the current finite worker lease as effective expiry. Revocation, cancellation, stale ownership, expired leases and disabled principals immediately remove authority. Ordinary fixed-expiry issuances are unchanged.
- Token absence propagates through reservation, worker and settlement without MAX sentinels. Actual usage, cost, turn limits, replay and ambiguous-send reconciliation remain intact.

## Explicit relaxed research preset

New drafts or the explicit preset action use 32 experiments, one concurrent scientific run, 64 turns per Mission, 8 repair turns, 1024 MiB memory, 12 cycles/day and zero minimum cycle interval. Application wall/CPU/token/output limits are absent. Applying the preset preserves existing memory, concurrency and cost settings, and never lowers existing larger trial/turn allowances. One core is used without a finite CPU/wall ratio. Native resource admission remains authoritative.

Loading an existing draft does not apply the preset. Frozen policy, Brief and history are never rewritten; actual project changes require a new authorized version. Execution budgets do not weaken minimum observations, Alpha qualification, independent Study, PIT, Sealed isolation, transaction-cost assumptions or return metrics.

The old 120-second allocation failure is retained as historical evidence: Data plus two Missions plus two trials' compile/forecast/validation plus two Sealed evaluations are eleven separately reserved Runs (132 seconds at the former 12-second slice); Build and Study are additional Runs. The nine-second redistribution proposal was superseded and is not included.

## Verification status

Final current slice: Contracts/Domain all-target check and 234 tests passed (40 targets, one existing architecture subprocess case filtered). Store/Server/Runtime all-target/all-feature check passed after the cleanup-only correction. Earlier-head failures and passes remain in the evidence logs, superseded by this run. New tests cover absent versus finite caps, finite-limit bypass rejection, old-runtime refusal, actual output observations, bounded-memory/core-rate behavior, nullable journal reopen/replay/cancellation, token usage and exact recovery identities.

Domain schema is regenerated. Remaining API/Runtime/client snapshots and actual resource recovery validation are pending joint integration. PostgreSQL, systemd/cgroup and Docker tests are authored but not yet executed. Frontend validation is paused by user instruction; source changes are explicitly unverified. No deployment or successful Polymarket research is claimed.

## Terminal recovery integration correction

A committed final turn/summary can precede process cleanup. Recovery must therefore run before the Worker’s complete/advance shortcuts, without starting a new model connection. The candidate now exposes an exact-owner cleanup-only path and refuses Mission terminal commitment while any resource record is open, both in Store and in the database state trigger. Added PostgreSQL cases cover cancellation before send and a final answer followed by ownership takeover; they assert no extra session, turn or native launch. These cases compiled in the final all-target check but were not executed; real PostgreSQL and abrupt-death recovery are still unverified.

## Follow-on application slice

`ForwardEvaluate` currently uses trusted hard-coded CPU 30 seconds / wall 60 seconds / output 1 MiB and migration `202609140068_forward_evaluation_inputs.sql` requires exactly that tuple. It does not inherit research Brief limits. The approved full-application scope includes this path. A follow-on slice must bind limits to the original candidate/research admission through a new migration and preserve its protected-input, fixed operation and parallelism guards. Do not change constants alone or relax its provenance gates. This slice is pending after terminal-resource recovery is verified.


## Reopened independent-review corrections (2026-10-06)

Library v4 remains an immutable prior checkpoint, with its 234 rule tests and compilation evidence scoped only to that head. Current source is reopened and unverified. New confirmed blockers: expired NOT_SENT bootstrap resource prevents takeover; HOST permit/late scope-registration can escape premature absence closure; finite CPU exhaustion blocks existing-turn reconciliation. The expired-claim/terminal route and closed-with-unknown accounting are being corrected. New HOST service fencing and persistent control-only reconciliation bounds are in design/implementation review. No current-head pass or live host recovery is claimed.
