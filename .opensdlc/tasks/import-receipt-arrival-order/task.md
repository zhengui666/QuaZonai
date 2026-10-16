# Verify import receipts across response arrival order

## Problem and evidence

Dev source `3a86582c1119558be62a53c34ba1c67a78ab93ea` passed 71 of
72 synthetic browser tests. The [Web job](https://github.com/zhengui666/QuaZonai/actions/runs/36843886386/job/110309263931)
failed while expecting the report-list receipt banner after releasing a held
import response and immediately returning to the migrations tab. Its native API
stage did not run; the exact-source release gate correctly stopped publication.

The test did not order response completion against component remount.
`MigrationManagement` restores its editor if the retained request is still
submitted at mount. A success arriving afterward displays the receipt in that
editor; a success already received at mount appears in the list banner. The
unconditional banner assertion assumes one arrival order. This is a source-based
explanation of the failure, not evidence that a production receipt was lost.
Raw browser diagnostics were not retained by this job's privacy policy.

## Bounded change

Replace the ambiguous sequence with two controlled cases using the same held
synthetic response and failed report-list reads:

- Complete the import while the editor is mounted, observe the exact receipt,
  return to the report list, then navigate away and back. Require the retained
  banner and receipt identifier.
- Keep the request pending across navigation and return. Observe the restored
  pending editor and disabled retry action before releasing the response. Require
  the exact receipt there, then explicitly return to the list and require its
  banner and identifier.

Both cases require one request with an idempotency key and the exact original
body, retain active Settings work until explicit receipt dismissal, and verify
that dismissal clears the receipt and active work. A finally block releases the
held response after failures. There is no automatic retry, timeout increase,
product-state change, new test hook, or raw diagnostic publication.

## Validation and delivery

Type checking and browser discovery are local checks; they do not execute these
browser assertions. The known local socket restriction means actual execution
must come from the existing hosted Web gate. Require independent native review,
all applicable final-head checks, and exact dev integration before merge. Follow
the merged source through its full release and cold-install gates afterward.

This closes a timing gap in the [Settings work tests](../settings-autosave/task.md)
while preserving [browser-leave protection](../settings-beforeunload/task.md).
It does not establish a cause or fix for the separate historical native compiler
smoke failure described in [runtime evidence](../runtime-smoke-failure-evidence/task.md).
