# Bound the native Mission fixture's debug poll stack

## Source and observed failure

Release run 37582752653 at dev 3018f10b189fb912fea93da20840bc7eada0cdc1
aborted the Server native-Codex mission_worker target with stack overflow in
controlled_real_declaration_registers_original_reviewed_qualification.
This source is the original 1581-file tree shared by merge 3018f and PR176 head
831cabd, not the later Polymarket combination.

The failure occurs about 1.32 seconds after the target starts. The original
scenario requires a genuine 60-second runtime-probe expiry before its first
Mission send, so this evidence points to the early fixture/proposal path rather
than a model response or qualification assertion.

## Native diagnosis and minimal boundary

A temporary non-polling layout probe under the same unoptimized/no-debug native
flags found a 52,360-byte scenario future and a 46,904-byte deepest bootstrap
future. DTOs were small (NativeTaskParametersV1 136 bytes; OperatorCommand 128
bytes), and the Mission production entry was already heap-pinned. These values
alone do not explain or prove a stack fix.

The compiled x86-64 ELF showed materially larger poll scratch frames: the
scenario subtracts 702,536 bytes, transparent selection/trigger async wrappers
96,856 and 96,824 bytes respectively, and the deepest fixture 266,984 bytes. These frames remain
nested during the early awaited fixture chain. The problem is not solved by
merely describing the future object's size.

The change places the deepest native fixture behind a thin Pin<Box<Future>> at
the existing fixture_with_trigger boundary, before its callers embed/poll it.
The constructor does not poll or spawn work. Original input arguments,
26 original target cases, assertions, native services, clocks, authority and
cancellation ownership remain unchanged. No production business model, thread
stack size, RUST_MIN_STACK, new thread or skip is introduced.

## Acceptance

- Compare actual native ELF poll frames before/after using identical compiler flags
- Keep a non-polling regression for the thin heap-owned bootstrap boundary
- Execute the unchanged controlled_real_declaration case and the full original
  Mission target on the real disposable PG18/PGMQ/native-Codex service with the
  ordinary test stack; compilation/layout checks are not runtime acceptance
- Preserve the exact failing log and distinguish temporary diagnostics from the
  final submitted test source

The real PG/Mission case is not yet rerun for this candidate. Source/ELF results
and the original failure are retained separately; do not call this a completed
release fix until the exact-head integration test actually passes.

## Executed native evidence (2026-10-07)

The same locked/offline, jobs=2, incremental=0, dev/test debug=0, unoptimized
compiler configuration was used before and after. The temporary layout probe
has been removed from the submitted source. The final target compiles/links;
the new bootstrap-boundary test passes, and all four pre-existing Responses
pure tests pass. Listing confirms all 26 original tests plus the single new
boundary regression (27 total). The caught panic printed by the negative
native-tool-output test is expected; that test and its command exit successfully.

Final x86-64 ELF prologues show these immediate stack reservations in bytes:

| Poll function | Before | Final |
| --- | ---: | ---: |
| settled_scientific_protocol | 702536 | 608712 |
| original controlled_real case | 104792 | 52568 |
| fixture_with_selection | 96856 | 3016 |
| fixture_with_cost | 96872 | 3048 |
| fixture | 96920 | 3096 |
| fixture_with_trigger (old async poll) | 96824 | absent |
| fixture_with_trigger_and_token_cap | 266984 | 266984 |
| Worker::process_message | 53784 | 53784 |
| Worker::drive_mission | 359624 | 359624 |

The new synchronous allocation helper has a 46952-byte construction frame;
it returns before its child is polled. It is not a replacement nested async
poll frame. Values come from objdump of the actual native binaries, counting
the stack-probe loop once. They exclude pushes, callees and dynamic stack use,
and do not claim an observed runtime stack high-water mark or sum mutually
exclusive runtime branches. Production Worker poll reservations are unchanged.

The exact original PG/Mission test and full native target were not executed
locally: this environment has no disposable PG18/PGMQ/native-Codex service.
They remain required at the integrated CI head with the ordinary thread stack.
There is no stack-size setting, new thread, ignored test, removed assertion,
schema change or production-code change in this patch. Independent final
source/evidence review passed with no outstanding P1/P2 findings. That review
confirms the narrow change and evidence limits, not the unexecuted PG/Mission
runtime result.
