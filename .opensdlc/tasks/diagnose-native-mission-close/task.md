# Observe the failed native Mission close without changing its result

## Original evidence and scope

Baseline: complete maintenance head 16727aad. Real-PG run 37601964521, Store job
112728233695, executed controlled_real_declaration_registers_original_reviewed_qualification
once. After 99.46 seconds it failed normally at mission_worker.rs:666 with
Codex("CLOSE_MISSION", Unavailable), rather than another stack-overflow abort.
The preceding stack reductions did not establish that the whole Mission passed.

Worker::drive_mission evaluates its original protocol result before propagating
the close result. This error identifies Client::close as the exposed failure;
it does not establish that the experiment advanced or the queue was acknowledged.
The turn driver can also return an unresolved outcome without an error.

The first native session requires ResourceAccount, so the original host path is
barrier, freeze/sample, final-CPU checkpoint, exact cgroup kill, native stop/fence
confirmation, closed checkpoint and mark_closed. Client then shuts down the
wire and confirms/kills the original launcher child. Existing errors collapse to
Unavailable. The previous service diagnostics were cfg(test) and restricted to
their own fixtures; an integration target links the ordinary server library and
therefore could not expose those internal failure stages.

## Minimal diagnostics

Use the existing tracing facility in the ordinary library. The private typed
Phase enum selects only compile-time static strings; the NativeFailure classifier
is exhaustive and emits only existing error-class names. Its Rejected payload is
not emitted. The event contains exactly phase and failure_class. No external
error text, stdout, resource/unit identity, path, environment, credential, account
data, request content or new diagnostic configuration is recorded.

Only failed operations log. The helper returns the original NativeFailure.
Client close, host resource close and account checkpoint call sites preserve their
original returns, operation ordering, bounded waits, ownership and cancellation.
The original pause().await.and_then(cpu_nanoseconds).ok() still short-circuits
and yields None on failure: logging does not invent zero CPU or final accounting.
mark_closed remains after successful native stop and committed closed checkpoint.
Legacy scope release still requires its original disappearance confirmation.

The service.rs author owns the complementary thirteen barrier/stop phase call
sites in a separate patch. Integrate both patches before compilation. This patch
does not modify service.rs or the queued-launcher diagnostic, and adds no new
manager, retry, supervisor, ledger, successful fallback or weakened fence.

## Verification and unfinished result

Two pure helper tests check all existing error classes, unchanged returned
values, exact emitted-field whitelist, rejection-payload exclusion and fixed
unique phase names. Native formatting and source review are local checks only;
Cargo execution must occur after the service patch is combined in the one shared
build window. The original real-PG/native Mission case must then run again to
identify the actual failing close stage. No diagnostic-only change counts as a
fix for that failure or as a successful close, CPU settlement or completed run.

Current status: final independent static review passed with no outstanding P1/P2.
The combined native build, two pure tests and real-PG diagnostic rerun remain
pending. Static review is not a successful close or a runtime repair result.
