# Split the native Mission test at the observed Reviewer boundary

## Scope and observed failure

Base: PR 177 commit `941cdbd15d24b2fc2d93527807487613e0742ea2`.
Its real PostgreSQL/native Mission run `37617286166`, job `112778592152`,
artifact `11481235879` reached both expected rejection markers, completed the
original concurrent ACKs, and printed `independent-review-start` before SIGABRT.
It never printed `independent-review-complete`. Earlier fixture, shutdown and
store-rejection hypotheses do not substitute for this observed stage.

Only `apps/server/tests/mission_worker.rs` and this task record change. The
private test scenario is split into Research and Review/Sealed phases; no
production source, protocol, SQL, migration, dependency or workflow changes.

## Implementation and retained behavior

Both phases have synchronous thin `Pin<Box<impl Future>>` constructors. A thin
async orchestrator awaits Research, moves its original state into Review, then
awaits Review on the same task. No task or thread is spawned and no stack size,
timeout, resource budget or acceptance condition is increased or removed.

`ScientificReviewContext` moves the existing pool, origin, declared origin,
Fixture, experiment, Worker, named `_stop` watch Sender, Receiver, evaluation,
derived alpha, cycle, selection trials, facts, original messages and token count.
The Fixture's TempDir and service handles and the Sender remain alive through
the original final Sealed assertions. Review only reborrows `messages[0]`; it
does not reread the queue, create another Worker/Fixture or replace a Run/session.

The Research body is the original 19,454 bytes before the first Review marker;
the Review body is the original 13,142-byte suffix plus its unchanged final
newline. Concatenating those bodies
and undoing the wrappers/context/new test reconstructs the entire original test
file byte for byte. All six fixed markers, the two intentional database failures,
their rollback assertions, concurrent `tokio::join!` ACKs, real probe expiry,
150-second timeouts, lease-loss checks, independent Reviewer, Sealed replay and
original qualification/valid-until assertions remain intact.

## Native validation

Compiled with the existing lockfile, offline dependencies, Rust 1.98.1,
unoptimized test profile, debug information disabled, incremental compilation
disabled, two build jobs, and no `RUST_MIN_STACK` override. Verbose compile
output records this candidate's actual source paths. No local PostgreSQL,
native manager, real account or venue order was used in this validation.

- New nonpolling thin-phase regression: 1 passed
- Existing thin-fixture and whole-scenario regressions: 2 passed
- Original Responses pure regressions: 4 passed
- Total actually executed: 7 passed, 0 failed, 0 ignored
- Full test target: 29 listed, comprising the original 28 plus the new test
- Original controlled-real Mission selector: exactly 1 listed, not executed

The original negative Responses fixture emits its expected caught panic while
the test succeeds; it is not a failure of this candidate.

Same-flags compiler-emitted stack prologues, in bytes:

| Function | Exact 941 baseline | Candidate |
| --- | ---: | ---: |
| Whole-scenario poll / new orchestrator poll | 609384 | 5432 |
| New Research poll | absent | 455800 |
| New Review/Sealed poll | absent | 159048 |
| Scenario constructor | 26424 | 56 |
| Worker drive_review poll | 155064 | 155064 |
| Worker drive_mission poll | 359624 | 359624 |
| Worker process_mission_message poll | 109688 | 109688 |
| MissionLauncher open_inner poll | 224072 | 224072 |
| Store acknowledge_run poll | 47832 | 47832 |

These are disassembled immediate stack allocations, excluding pushes, callees
and runtime high-water usage. Research and Review polls are sequential, not
simultaneously added to a claimed stack total. The exact baseline and candidate
ELFs and full prologues are retained outside the source tree. The production
Worker/Store frame sizes are unchanged. This supports the focused removal of
the monolithic test frame at the observed Review stage; it does not prove the
real Mission no longer overflows.

## Review and remaining acceptance

Independent final source, patch and evidence review passed with no P1/P2
findings. The reviewer independently reconstructed the original full test bytes,
checked the complete two-path tree difference and verified the actual logs and
ELF prologues. The final ordinary-stack PostgreSQL/native Mission case must still run
in CI at the published head. A listed test, nonpolling size regression, or static
frame reduction is not that acceptance. Merge remains blocked on the real case
and the repository's other applicable final-head checks.
