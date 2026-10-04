# Agent evaluation report foundation

## Task and scope

Deliver the first usable Agent report contract, offline validator, immutable report
publication and in-app dashboard for `dev`. This branch is stacked on
[PR 143](https://github.com/zhengui666/QuaZonai/pull/143), exact base
`d2a155598fb8b6195bea16d6209f81982759b20c`; that dependency is not yet merged
at integration time. The maintained format, reproducible
commands and live-run boundary are in the [framework guide](../../../tests/agent-evaluation/README.md).
The authorship/workflow update is tracked separately in
[PR 137](https://github.com/zhengui666/QuaZonai/pull/137); this batch does not duplicate it.

This foundation does not implement a live model runner, execute held-out tasks,
claim authenticated model availability, or grant independent scientific
qualification. No production evaluation reports, example results or test cases
are inserted. Existing generic REPORT uploads remain compatible.

## Design

- Rust contracts own the report, exact scalar types and pinned ISO cost currency
  schema. Pure domain validation checks outcome consistency, complete assertions
  and evidence, exact observed identities for PASS and FAIL, unique cases and
  disjoint tuning/held-out metadata. Mismatched identities remain BLOCKED
- Reuse artifact upload, idempotency, immutable native storage, project authority,
  revocation and capacity. The typed read retains the capacity permit through
  stream consumption/disconnect and shares backend-unavailable behavior
- The project dashboard uses generated API types and validators with Ant Design.
  It shows requested versus observed settings, four outcomes, provenance hashes,
  exact measurements and explicit unknowns. Lost-acknowledgement retry retains
  the original body/key; dismissing the form does not revoke a stored artifact
- The only committed report is an UNRUN / PROTOCOL_ONLY fixture. Production logic
  does not load fixtures, held-out scenario contents or golden answers
- Native domain/HTTP JSON snapshots use deterministic compact serialization;
  generated response validators use PR 143’s modular native Ajv graph and pinned
  Vite/esbuild transformer. The upload awaits the existing typed GET contract
  through the selective lazy entrypoint, with sequence guards before and after
  validation. A local component-load failure leaves upload disabled and is
  distinct from invalid JSON; no upload or HTTP write is automatically replayed.
  Schema values, exact scalar rules, media dispatch, exports, function reuse and
  snapshotted Ajv errors remain intact. Regenerated artifacts are committed and
  verified against their native sources

## Verification

Run the commands in the framework guide, native schema generation, web generation,
TypeScript, Vitest, production/PWA build, Rust formatting and scoped Clippy.
Regression coverage includes malformed/forged reports, model mismatch as BLOCKED,
precision and null semantics, duplicate cases/split hashes, backend error parity,
capacity held through partial reads/disconnect, artifact auth/revocation, and a
real-native-browser lost-ACK upload/retry/details flow.

The earlier foundation was reviewed and checked on base `5e7f295`, including the
Chrono/typed-read correction below. Its monolithic-validator size measurements
and earlier test totals do not describe this integrated branch. Integration
retains the prior handwritten report/API/domain changes, excludes the old
monolithic generator/test compaction patch, and regenerates every changed web
artifact using the modular generator. PR 141’s data-dialog recovery and current
workflows are retained from the new base.

Integrated local verification is recorded below after final regeneration. The
PostgreSQL/PGMQ HTTP tests and actual browser/PWA acceptance require hosted CI;
local sockets, browser execution and Docker are platform-blocked and were not
retried. A successful build or protocol fixture is not a live evaluation,
browser pass or final-Head CI pass. Exact-Head hosted checks and independent
native review remain required before merge.

On 2026-09-30, an isolated read-only native 0.156.1 catalogue advertised the exact
`gpt-6-luna` / `max` configuration. The default cloud environment's official
`login status` separately confirmed an existing ChatGPT login. App Server still
failed on platform read-only runtime-state writes, including an authorized retry
with temporary documented `sqlite_home` / `log_dir` overrides. No account refresh,
thread, turn or inference ran; no credentials were read or copied and persistent
configuration stayed unchanged. Live evaluation therefore remains BLOCKED on
execution, not inferred to be unauthenticated. Catalogue advertisement and login
status do not establish an observed serving identity or evaluation result.
The separate documented `codex exec --ephemeral --ignore-user-config --sandbox
read-only --model gpt-6-luna` path, with per-process `max` effort and optional
tools/integrations disabled, also failed after 5.2 seconds at read-only in-process
App Server startup, before any thread, events or inference. It produced no subject
output or live evaluation; no further native startup retries are planned.

## Remaining live evaluation work

1. Restore a supported native execution route within the existing account
   authorization. Verify the actual serving model/reasoning identity at run time;
   read-only catalogue advertisement and login status alone do not establish it
2. Implement a bounded external runner with explicit invocation/budget controls,
   public native invocation identities and retained auditable observation bytes
3. Freeze the suite, required assertions, source and data hashes before execution;
   maintain independently held-out scenarios and golden answers outside runtime
   product logic and tuning exposure. Publish reports into an evaluation project
   inaccessible to the subject; ordinary REPORT artifacts retain existing
   project-level RESEARCH visibility and do not create that isolation themselves
4. Require the exact requested policy `gpt-6-luna` / `max`, verify observed identity,
   preserve failures/blocked/unrun cases, and never silently substitute settings
5. Run and review authorized tuning and held-out evaluations, record actual usage,
   tool calls, elapsed time and cost when observed, then publish the real reports
   through the existing artifact route

Publication, current-head CI and merge are coordinated separately. No deployment
or account authorization is implied by this foundation.

## Independent review correction

Independent native review found that Chrono could accept timestamps outside the
generated HTTP date-time validator while the typed route returned raw uploaded
JSON. The route now serializes the parsed native type and admission rejects
extended years and invalid UTC leap-second positions. Original `/content` bytes
remain immutable and can be hashed externally; no server-provided digest is implied. Native, generated-web and database-backed HTTP
regressions cover normalized reads, invalid admission and original download.
The HTTP test binary was compiled locally; actual PostgreSQL execution remains
pending hosted CI, not counted as a local pass.

The original correction passed contracts/domain and web checks. Those historical
results are not a final-Head pass for this stacked integration.

## Integrated verification

On 2026-09-30, local checks passed:

- Native HTTP/domain regeneration exactly preserves the reviewed foundation’s
  parsed schema values; the modular generator owns 350 response files
- All 316 preexisting core modules remain byte-identical; the report adds 14
  schema-owned modules. Complete independent generated/tracking checks pass
- All 41 generator/loader tests pass, including 45,448 native full-error/result
  comparisons and 35,550 route/media comparisons; repeated complete generations
  are byte-identical
- TypeScript and all 556 Vitest tests pass, including asynchronous report
  validation and independent concurrent error snapshots
- Production build and static PWA checks pass: 1,707,770-byte startup closure,
  4,501,724 total static bytes, every lazy JS/CSS chunk precached, and largest
  asset 1,391,622 bytes under the unchanged 2 MiB limit. The eager compatibility
  facade is absent from the production graph. These are build-byte measurements,
  not browser performance claims
- All 173 contracts/domain tests and the offline UNRUN/PROTOCOL_ONLY report
  validator pass
- Both normal-dev and production browser matrices discover the new failed-load
  and cancelled-delayed-upload regressions, plus uncertain-upload replacement
  and exact-retry cases (18 discovered tests across both modes). Discovery is not browser execution

- All 19 scoped server schema/CLI/Skill/artifact tests pass, and the expanded
  database-backed artifact HTTP regression binary compiles without execution
- Rust formatting and all-target Clippy for contracts, domain and server pass
- Final regenerated files match the staged output byte-for-byte, and changed
  documentation’s relative link targets resolve

Actual PostgreSQL/PGMQ and browser acceptance, independent final-Head review and
hosted CI remain required. No live inference, publication or merge is performed
in this integration task.

## Integrated upload recovery correction

Independent native review reproduced a P2 where selecting a different file after
an uncertain upload could replace the original intent without the explicit
close warning. The editor now freezes its submitted body and idempotency key
at the first write, prevents double-submit synchronously, disables replacement,
and rejects programmatic file replacement until explicit close confirmation.
Retries use the original request; no automatic replay is introduced. A receipt
for another project or artifact kind fails closed and keeps the original intent.

The native lost-ACK case now checks the locked picker. New normal-dev/production
cases cover lost ACK and HTTP201 receipt-validator loading failure, exact retry,
continued editing, explicit abandonment and a distinct new intent after remount.
Independent source-level hook execution verified both failure paths, double
submission, guarded replacement, exact body/key retry and foreign-project receipt
rejection. TypeScript, all556 web tests, build, PWA graph checks and18 browser
test discovery entries pass after the fix. Real browser execution remains the
final-head hosted gate; this source-level check is not browser acceptance.
