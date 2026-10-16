# Bound the scientific Mission scenario's test-harness stack

## Exact failure and baseline

This follow-up starts from the complete 1606-file maintenance head
4e746d8c471997528b4cc3bb5821698e15ceb307. It preserves the preceding native
fixture heap boundary, release gates and all other maintenance fixes.

Run 37596191488, Store job 112709308070, actually executed
controlled_real_declaration_registers_original_reviewed_qualification once and
aborted with SIGABRT/stack overflow. The process began at 09:02:18.722 UTC,
reached the original session-table notices at 09:02:20.283, and overflowed at
09:03:20.392. This is later than the original fixture-construction failure.

The unchanged scenario deliberately waits for its real 60-second runtime probe
to expire before invoking the first Mission worker pipeline. The new failure
time is consistent with that boundary; it is not a stack backtrace proving one
specific native child function. The preceding five pure tests and reduced
fixture ELF frames did not establish that the real Mission test was fixed.

## Minimal same-task boundary

The compiled baseline scenario poll reserves 609288 bytes. Its state is also
embedded into the SQLx case/harness: the original case poll reserves 52856,
run_test 80456, run_test_with_pool poll 63672, and the Runtime block_on wrappers
55144 bytes each. These are compiler stack reservations, not measured runtime
high-water marks or a sum of mutually exclusive branches.

The private settled_scientific_protocol helper becomes a synchronous thin
Pin<Box<impl Future>> constructor. Its entire original async body is only renamed
to settled_scientific_protocol_inner. All original input values, awaited calls,
60-second expiry, timeouts, cancellation ownership and assertions remain in the
same order and task. The constructor returns before the scenario is polled.
This keeps the large scenario state out of its callers' embedded futures.

No production Worker, Store, native client, model, schema, thread stack setting,
thread creation, timeout, expected value or ignored-test flag changes. The
original 26 target cases and prior fixture-boundary regression remain. One new
non-polling test checks the scenario's thin heap-owned entry; it never constructs
a database pool or starts a native process.

## Required acceptance

- Compile with the original locked/offline, unoptimized, debug=0, incremental=0,
  jobs=2 settings and compare actual ELF poll prologues
- Run the new scenario boundary, old fixture boundary and four original pure
  Responses cases; list and preserve all original target cases
- Preserve the previous failed real-PG evidence and both native ELF snapshots
- Execute the unchanged original controlled_real case and applicable complete
  native target in final-head disposable PG18/PGMQ/native-Codex CI

The local environment has no such real PG/native service. Until the exact CI
case passes, this remains a narrow stack-reduction candidate, not a completed
runtime fix.

## Executed native evidence (2026-10-07)

The final target compiled/linked under the same flags. Six pure tests passed:
the new scenario-boundary test, the previous fixture-boundary test and the four
unchanged Responses tests. The target lists 28 cases, preserving the previous
27 plus the new regression; the exact original controlled_real selector lists
one case. Listing is not execution: real PG/Mission cases executed locally = 0.
RUST_MIN_STACK is unset. The negative Responses test's caught panic output is
expected and its test/command exit successfully.

Actual final ELF prologue comparisons (bytes reserved by immediate subtraction):

| Function | Baseline | Candidate |
| --- | ---: | ---: |
| Original controlled_real case poll | 52856 | 88 |
| SQLx run_test | 80456 | 36808 |
| SQLx run_test_with_pool poll | 63672 | 10904 |
| Runtime block_on wrapper | 55144 | 11496 |
| Runtime block_on_inner wrapper | 55144 | 11496 |
| Scientific scenario body poll (renamed inner) | 609288 | 609288 |
| Worker::process_mission_message poll | 109688 | 109688 |
| Worker::drive_mission poll | 359624 | 359624 |
| MissionLauncher::open_with_monitor poll | 82472 | 82472 |
| MissionLauncher::open_inner poll | 224072 | 224072 |

The synchronous scenario constructor reserves 26424 bytes while constructing the
boxed future, then returns before the body is polled. The comparison confirms
removal of the enclosing case/harness state-copy overhead. It does not reduce
the scenario body's own scratch frame or alter the production Mission pipeline.
Probe loops are counted once; pushes, callees and dynamic stack usage are not
included. No sum is presented as a measured runtime high-water mark.

Both ELF snapshots, original failure, exact commands/exits, actual test lists and
objdump prologues are retained with the candidate evidence. Final independent
source/evidence review passed with no outstanding P1/P2 findings. The unchanged
real-PG CI case remains the acceptance condition for resolving the stack-overflow
bug; the review does not establish that unexecuted runtime result.
