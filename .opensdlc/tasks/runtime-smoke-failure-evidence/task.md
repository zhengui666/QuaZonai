# Retain safe Runtime smoke failure evidence

## Failure and scope

The Container check for [PR #154](https://github.com/zhengui666/QuaZonai/pull/154)
at `b8401840ed1a144a863bc4266ee227ef88507d5e` reached a failed native compilation
with `has_result: true`. The [failed job](https://github.com/zhengui666/QuaZonai/actions/runs/36830792188/job/110266674837)
asserted success before retrieving that result. Cleanup then removed the owned
Runtime state and gateway log. No retained artifact establishes the underlying
failure code; resource, engine, input and output failures remain unproven.

The Runtime, scientific job and smoke sources are identical to the passing
`5503dd0d` and `9276b84e` revisions. That comparison does not explain this failed
execution or make its result a pass.

## Behavior and boundaries

[The smoke harness](../../../deploy/docker/smoke.py) now reads an available
result once before its original success assertion. It uses the same authenticated
request with the existing 15-second timeout and 5 MiB read bound, verifies the
run, attempt, external job identity and state, and prints only allowlisted
failure class/code values to the retained job log. It never prints result bodies,
artifact references, messages, credentials, native stderr or exception details.
An unavailable or mismatched result leaves the original failure in place.

The result contract has no process exit status or signal. Its final CPU and peak
memory counters are null, so they cannot distinguish native process failures.
The existing [SQLite journal](../../../apps/runtime/src/supervisor.rs) is located
at the owned temporary Runtime's `state/journal.sqlite`; the
[native-exit table](../../../apps/runtime/migrations/0002_materializations_and_native_exits.sql)
already records the original process exit code and OOM observation.

The diagnostic captures only this journal and its live WAL through no-follow
directory/file descriptors, with an 8 MiB bound per regular, single-link file.
It rejects changes during capture and never retries. A private temporary copy
is queried read-only with WAL support, a bounded query and no source SHM access
or mutation. The query requires regular strict tables, the exact locally owned
run/attempt/external identity, terminal phase and matching container/start
identity. Only an actual integer `exit_code` and a validated SQLite 0/1
`oom_killed` (emitted as a JSON boolean) are added to the log. Missing, unsafe,
unreadable, malformed or unmatched evidence is explicitly unavailable; it is
never defaulted to zero or false. No other journal field is logged.

The scientific request, limits, deadlines, ownership, polling and cleanup remain
unchanged. This is an evidence repair, not a claim to fix the unexplained native
failure. No production Runtime or paused packaging comparator is changed.

## Verification

[Fourteen focused tests](../../../deploy/docker/smoke_test.py) pass, including failure
retrieval before process/container cleanup, original assertion preservation when
retrieval fails, result identity matching and closed-code output. They exercise
live WAL from the actual first-party migrations, unchanged source DB/WAL/SHM,
strict scalar handling, unavailable/mismatched records, missing/corrupt/oversized
files, symlink/FIFO/hardlink rejection and read/race failures. Before the journal
extension, deployment unit discovery ran 138 tests: 137 passed; the pre-existing
legacy Codex install test could not create an AF_UNIX socket in this executor.
That blocked test was not retried. No local Docker smoke or hosted rerun was
executed. Independent native review and applicable CI on the exact published
head remain required under [review](../../review.md).
