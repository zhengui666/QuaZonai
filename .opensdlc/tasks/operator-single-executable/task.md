# Single installed operator executable

## Scope

Approved native packaging batch from dev
`bfa3cfc625a752fdb554d9136b9b2eb4412c6f24`. Preserve the existing installed
`catalog-prepare` and `polymarket-history` command contracts through regular-file
launchers. Each launcher executes the fixed packaged `source-tools` path, a
literal selector and quoted original arguments. The shared first-party modules
retain standalone Cargo binaries with their existing feature requirements; the
combined binary requires both optional features. The catalog branch stays
synchronous and the history branch retains its Tokio runtime.

Build and strip one operator ELF. Do not change third-party implementations,
locked dependencies, compiler/profile flags, the default scientific job/cache,
host-extracted service binaries, updater bundle members/schema, source mount
and ownership guards, or installed native override rejection. Docker compiler
input/cache and producer-identity restructuring are a separate batch.

## Acceptance

The native history unit suite moves with its source into feature-gated library
modules. Exact CLI comparisons cover help/version, all history subcommands,
catalog root/ingest modes, invalid arguments and runtime failures, including
paths with spaces, Unicode, shell metacharacters and non-UTF-8 bytes. Native
integration tests execute the new dispatcher and verify original Parquet
artifacts. Existing native/scientific/deployment/registry checks remain required.

Installed and no-checkout cold-install smoke executes both regular launchers
with no Cargo or rustc available. Its bounded synthetic history import writes
three original BARs and an explicitly unqualified report, then catalog
preparation reads the native records/definitions and verifies the resulting
catalog again before publishing fixture metadata. It preserves original source
bytes, checks output-reuse failure, and reuses owned-container reconciliation
before removing temporary mounts. All fixture clocks/fees/prices are synthetic;
no provider data is fetched or qualified by this smoke.

Cost reports distinguish real ELF files from launcher bytes, include the complete
payload, and retain actual image-save/gzip accounting with the existing 2 GB cap.
Admission requires measured old-full versus candidate-full stripped ELF, payload,
image and gzip bytes alongside the same-source no-operator baseline. Report cold
and warm timings with their actual cache/resource conditions; candidate-first
baseline timing and `--no-cache` alone do not prove cold-cache performance.

The initial design also proposed a fresh-builder GHA-layer-cache replay and
actual linker-process telemetry. The reviewed scope was narrowed on 2026-10-01:
cache replay and producer identity belong to the separate cache batch, and
linker-process telemetry is deferred. Their report fields remain explicitly
unmeasured; this batch makes no linker-count reduction claim. Its cold timing
includes base-image transfers into the disposable builder's separate content
store, even though host Docker prerequisite pulls occur before the timer.
The required single-ELF/payload/full-image/gzip measurements, both installed
data paths and honest cold/same-builder-warm total timings are retained.

## Evidence status

Local locked/offline Cargo checks and focused Python checks are recorded with
the staged delivery. Local disk is too constrained for release linking and
Docker is unavailable. New-dispatcher execution, installed container acceptance,
release byte measurements and cold/warm timings remain UNRUN until hosted jobs
produce evidence for the reviewed head. No size or speed gain is claimed.
Codex Agent evaluation remains UNRUN and is not needed to substitute for native
packaging tests. Independent final-head review and all applicable CI are required
before dev merge; no live Coinbase acquisition is part of this batch.

The bounded local checks also validated the synthetic three-BAR history fixture
with already-existing legacy history/catalog executables. Import remained
`UNPROVEN`/`UNVERIFIED`, preparation remained `FIXTURE`/`UNVERIFIED`, and original
native definitions matched. This checks fixture construction only; those older
executables do not establish the changed dispatcher's or packaged image's result.
