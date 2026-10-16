# Locate the later native Mission stack failure

## Exact observed run and source

Baseline: full maintenance head 729b9cb. Run 37612372735, Store job 112762461015,
executed the original controlled_real_declaration_registers_original_reviewed_qualification
once. It began at 11:25:21.871 UTC, initialized sessions at 11:25:23.292,
logged Store rejections at 11:27:00.857 and 11:27:33.068, and overflowed at
11:27:33.611 (131.74 seconds). This is later than the previously identified
fixture/initial-driver failures. No close/fence failure was recorded this time.

The test deliberately injects two earlier Database errors: validation publication
and independent-reviewer admission. Their existing Worker error conversion emits
the same masked warning. Independent source review traced both expected paths;
the old log lacks stage markers, so the two observed warnings are not themselves
proof of a new Store bug or exact attribution to those injections.

After the second expected rejection, the original test drops its trigger, awaits
two competing ACKs in one tokio::join!, verifies the one original selection and
Reviewer, and starts the independent native-review pipeline. The last warning
and overflow are 0.543 seconds apart. Timing alone does not select one of these
paths as the actual failing stack.

## Minimal fixed-stage evidence

Add only six literal stderr markers in the original private test scenario:

1. validation-publication-rejection-observed
2. reviewer-admission-rejection-observed
3. concurrent-ack-start
4. concurrent-ack-complete
5. independent-review-start
6. independent-review-complete

Markers contain no IDs, SQL error detail, native response, file path, environment
or credential. A rejection marker follows the original expected-error assertion;
it does not claim a particular SQLSTATE. Completion markers follow the original
success unwraps. The original triggers, rollback checks, two-ACK concurrency,
assertions, calls, timeouts and task ownership are unchanged. Removing exactly
these six lines reconstructs the original test file byte-for-byte.

No production code, stack size, allocation boundary, model, migration or test
count changes in this candidate. Do not use it as a completed stack fix.

## Executed local native checks

The unmodified complete baseline was first verified by direct file/byte equality,
then freshly linked under the recorded locked/offline, unoptimized, debug=0,
incremental=0, jobs=2 settings. Verbose compiler commands identify the exact
candidate crate roots. Its ELF was preserved separately before adding markers;
the older cached v3 ELF is not substituted for this baseline.

The diagnostic source was then linked under the same flags. The two existing
heap-boundary tests and four existing Responses tests passed (six final-source
pure tests). The earlier baseline-only run of one of these tests is recorded
separately, not counted twice. The unchanged target lists 28 cases; the exact
original integration selector lists one. No local PG/Mission case ran.

Actual ELF immediate stack reservations, baseline to marked source, in bytes:

- scientific scenario body: 609288 to 609384 (96 bytes of diagnostic overhead)
- original case poll: 88 to 88
- Store::acknowledge_run: 47832 to 47832
- mission::admit_role: 23208 to 23208
- selection::freeze: 15416 to 15416
- Worker::drive_review: 155064 to 155064
- Worker::drive_mission: 359624 to 359624
- Worker::process_mission_message: 109688 to 109688
- MissionLauncher::open_with_monitor / open_inner: 82472 / 224072, unchanged

These prologue measurements exclude pushes, callees and dynamic stack use. They
are not a measured high-water mark or a sum of mutually exclusive branches.
Neither adjacent ACK source placement nor the deeper Reviewer chain alone proves
where the real run overflowed. The next unchanged CI case must supply the last
completed/entered stage before choosing a narrow follow-up boundary.

Current status: native diagnostic build and pure tests complete; independent
final source/evidence review passed with no outstanding P1/P2 findings. The
original real-PG stage trace and runtime stack-failure resolution remain pending.
