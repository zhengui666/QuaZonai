# Durable recovery

Base: remote `dev` at `8fe4f9d25dc0a66e32237edf0e9f7db5ad51e227`.

## Acceptance

Preserve committed Agent/business facts, original execution identities and external receipts across process, transport, power and maintenance interruption while storage remains intact. Official Pi Durable remains the execution authority. Long native scientific computations must resume internal state through official runC 1.5.2 / CRIU 4.3 process-tree checkpoints paired with the same generation of writable files; whole-invocation replay is not an acceptable substitute. Restore only on an actually validated Linux execution profile.

## Implementation

- P0: real workload checkpoint/restore and storage-failure evidence; unsupported environments remain explicitly blocked.
- P1: FULL SQLite commits, lifetime ownership, durable file publication, original complete per-step requests.
- P2: asynchronous V2 execution, official worker Tasks, detached observers, persisted cancellation and completion receipts, checkpoint broker.
- P3: managed capacity/data operations and all scientific call sites.
- P4: compatibility, fault injection, independent review, exact-head CI and remote dev integration.

## Current evidence

Implementation in progress. No production deployment, power-loss acceptance, complete native checkpoint acceptance or merge is claimed. Model-provider internal inference is outside local process snapshots; preserve committed contexts and report uncertain usage honestly.

## Qualification blocker

P0 is blocked on this executor: the pinned official tools fail required kernel
feature probes, including in the approved task-owned isolated Docker test.
The complete commands and output are retained in `evidence/p0-environment.json`.
No verified production profile has been generated. True scientific internal
frontier and cache-loss/power-failure acceptance remain required; a mock backend,
controlled subprocess, successful typecheck or final-output equality is not that
proof. Existing V1 executions do not acquire the V2 guarantee retroactively.

Live project-board synchronization is pending: the owner-private service route
was inspected, but the execution network proxy rejected its read with HTTP 403.
No board write was attempted or claimed.

## Implemented source and verification boundary

The reviewable implementation includes official FULL/WAL storage and owner
locking, durable file publication, stable per-step original requests/results,
V2 worker tasks and cancellation fences, the runC/CRIU broker and same-generation
filesystem adapter, managed capacity/source refresh/capital validation, frozen
automatic-sizing rounds, and original-request browser retry behavior.

Independent review found and corrected preparation UUID instability, source
verification incorrectly sharing the science result cache, cancel-before-admit
races, several process-launch/checkpoint publication crash windows, and settings
forms dropping recovery bindings. Controlled tests exercise these contracts;
they do not qualify a native recovery profile.

Still required before full-plan completion: actual native internal-frontier and
power-loss acceptance on a compatible node, retained physical restore/resource
usage accounting, complete upgrade/profile-retention acceptance, and final-head
hosted CI/review. General long-running data transformations are refused unless a
supported recovery adapter is declared; the managed plugin path currently admits
read-only recovery. This task is not production-ready or merge-ready solely from
its local test results.

## Local checks

- `make check-ts`: passed; Service 425 passed / 29 native-prerequisite skips,
  Research 294 passed, Nautilus 27 passed, CLI 15 passed. Skips are not native
  acceptance.
- Web types, 80 unit tests and production build passed. Six focused Chromium
  cases passed, including data-refresh response-loss/reload replay and preserving
  a V2 binding while rotating credentials.
- `make check-deploy` passed with the official checksum-verified Node 22.19.0
  runtime. The initial attempt with the environment's repackaged Node failed
  because its distribution LICENSE was missing; no assertions were relaxed.
- Thirty focused recovery tests also passed on official Node 22.19.0.
- Independent static and controlled-protocol review findings were corrected.
  Real CRIU scientific continuation and power-loss acceptance remain blocked;
  no verified profile was emitted.
