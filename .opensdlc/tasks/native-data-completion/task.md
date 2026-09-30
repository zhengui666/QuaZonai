# Native data completion

## Goal and scope

Build on the real frozen-input admission in PR142. An explicitly synthetic,
three-candle acquisition must go through the actual `ingest-candles` and
`catalog-prepare` executables, an actual configured Runtime and immutable Job
image, real API registration/freeze/admission, the shipped Worker, actual OCI
`DATA_VALIDATE`, persisted artifacts, and browser readback/download. No terminal
Run or output row may be seeded. This is fixture execution evidence, not a live
provider capture, historical permission, PIT verification, or qualification.

Base: merged [PR142](https://github.com/zhengui666/QuaZonai/pull/142), dev commit
`3f9da76e`. Design/development/review use native tools; no Codex inference is
part of this bounded data acceptance path.

## Implementation plan

- Share the existing synthetic acquisition writer with the native candle tests;
  keep one actual provider descriptor and exact raw/derived/hash relationship.
- A test-only Job example creates three real native candles and then invokes the
  original catalog preparation command. Metadata comes from its measured output,
  never the authored calendar/TLS admission fixture. Original events are
  60/120/180 seconds and receipt time is 2024-01-02T00:00:01Z for all three rows.
- Add an explicit native execution mode to the existing private browser harness.
  Keep ordinary Web and TLS admission acceptance intact. The execution mode
  supplies an actual loopback Runtime, pinned image, catalog and credential via
  existing deployment configuration, with development HTTP explicitly selected.
- Retain original browser auth/restart/idle Worker checks. Freeze/admit/replay with
  Worker stopped, verify the queued checkpoint, then start the same shipped
  Worker. Wait for actual publication and read/download current-attempt artifacts.
- Restart API and Runtime and confirm original identities and bytes, without a
  second attempt/container. Use SQL only for read-only cardinality assertions.
- Native requirements are mandatory; absent Docker, image, cgroup, binaries or
  source output fail the explicit scenario instead of becoming skipped success.

## Evidence contract

Require exactly one frozen input, Run, native task and attempt for the successful
scenario; preserve the original QUEUED receipt. Inspect the actual selected
catalog/image/selection and OCI exit zero. Artifact metadata must bind project,
Run and active attempt, and the downloaded bytes must equal the Runtime outputs.
Verify three measured rows and the original clocks/notionals. FIXTURE/UNVERIFIED
and no qualifications must remain after success and restart.

The local metadata's dataset identifier and registered DatasetRevision identifier
are deliberately different: production admission maps the Job parameters to the
registered identity without rewriting the source metadata. Compare this mapping
and measured meaning rather than pretending the initial and fresh quality JSON
are byte-identical; checked_at is expected to differ.

Targeted negatives: corrupt the native file before execution and require failure
without a fresh quality result; missing/interrupted prerequisites fail; pending
refresh does not POST again; cancelled navigation does not finish a download.
Cleanup only the invocation's recorded container IDs/labels, before owned state
is removed. Do not add a production delay to manufacture active cancellation.

A read-only host catalog mount does not establish a content-addressed immutable
snapshot against all same-shape host rewrites. Do not broaden that claim.

## Implemented boundaries

- `apps/job/examples/browser_catalog_fixture.rs` executes the real plugin and
  native preparation, then reads the prepared Parquet back and checks each
  original candle, receipt clock, instrument definition and metadata hash.
  Its test-only partition manifest comes from the pinned native catalog's BAR
  discovery and verified series filter, requiring exactly one regular owned
  file. Corruption consumes that descriptor, checks canonical root containment,
  kind, series, size and SHA256, rejects symlinks/shared hard links, and records
  actual before/after hashes. It never guesses directory names or selects
  instrument definitions. This assumes stable harness-owned directories, not a
  hostile-filesystem sandbox.
- `QUAZONAI_WEB_DATA_MODE=native-execution` explicitly enables the actual Runtime
  path in the existing browser harness. The default remains TLS admission only.
  The original auth, native idle-Worker restart, API restart, accessibility and
  PWA scenarios remain required before data execution.
- The shipped Worker is stopped during freeze/admission/lost-ACK replay. Read-only
  PostgreSQL checks establish one queued Run and no attempt. Successful execution
  requires one frozen input, native task, attempt, container, native output and
  archived queue message, plus the original queued receipt. Docker inspection
  verifies the actual image, read-only catalog mount, network isolation and exit.
- Both current-attempt `qz.data_quality` and `qz.job_result` are downloaded through
  the UI and compared byte-for-byte with actual Runtime responses. API/Runtime
  restart repeats the downloads and receipt replay with unchanged identities.
- A separate second command reuses the frozen input after its owned native
  Parquet is corrupted. It must fail in OCI, publish no quality artifact, and
  leave successful evidence and FIXTURE/UNVERIFIED status unchanged. Counts for
  the successful and negative Runs remain explicitly separate.
- Container cleanup starts only after the owned Worker and Runtime stop. A
  read-only SQLite connection resolves exact invocation/run/attempt/image labels;
  every candidate is preflighted before any removal. Uncertain ownership retains
  state and the database. No global Docker cleanup is used.
- The [native data browser workflow](../../../.github/workflows/native-data-browser.yml)
  runs on every PR and main/dev push with the default native Rust stack; it never
  enables native-science or invokes model inference. It is an exact-source
  prerequisite in the release selector and Dev waiter, including the main
  workflow-completion trigger. Missing or unsuccessful runs cannot publish.

## Verification status

Local focused checks on 2026-09-30:

- 9 native candle tests passed after fixture extraction; scoped Job Clippy with
  warnings denied, example build and workspace formatting passed.
- Actual plugin conversion, native preparation and prepared Parquet readback
  passed: three events at 60/120/180 seconds, receipt 1704153601000000000 ns,
  close 42000.99, volume 0.10000001 and last-bar USD notional 4200.1.
- 555 Web unit tests, TypeScript and production/PWA build passed.
- 23 cleanup/control-flow tests passed, including missing image, interrupted
  startup, ownership mismatches and retention on an unreadable journal. Scripted
  observations here are unit evidence, not real Docker/systemd acceptance.
- 68 release helper tests passed, including exact-source data gate absence,
  pending/failing/cancelled/skipped states and Dev wait behavior. The broader
  93-test deployment suite had 92 passes and one environment-blocked existing
  Unix-socket fixture; no permission workaround or unrelated test change was made.

The current cloud executor cannot run the required loopback/Docker/Chromium
scenario. Full hosted browser/Worker/OCI success, independent native review and
all applicable final-Head CI remain required before merge; no account, market,
PIT or qualification result is claimed. Hosted CI records exact source, build,
image and per-phase timings and total elapsed seconds in its sanitized evidence.
The added cost is one serial Ubuntu job with a 45-minute ceiling, no paid model
inference and no broad image matrix. Actual hosted duration/cost is not yet
measured; elapsed runtime is not a billing estimate.

Follow-up evidence on 2026-09-30:

- Hosted [run 36790023072](https://github.com/zhengui666/QuaZonai/actions/runs/36790023072)
  at `fdeaab69c659d622cbf390aad1ce695e817df6c7` passed actual preparation,
  freeze/admission, Worker/OCI success, exact artifact download/retry checks and
  API/Runtime restart with the same Run/container. It then stopped in corruption
  setup because the hardcoded `/bar/` path matched no native partition; no
  corruption or negative Run was attempted on that Head.
- The descriptor correction passed 50 Node ownership/control-flow tests,
  scoped Rust formatting, warnings-denied example Clippy and the example build.
  An actual local native fixture discovered one 2,214-byte BAR partition with
  the original three rows/clocks. The real helper changed only this partition to
  the existing 42-byte corrupt payload; native preparation/readback succeeded
  before and failed afterward. The 6,643-byte instrument Parquet stayed
  byte-identical. No Docker, loopback or browser acceptance was run locally.
- The mandatory failed-OCI/no-quality-artifact proof and all other final-Head
  hosted checks remain required. Local byte/readback checks do not establish
  that end-to-end negative result.
