# Single installed operator executable

## Scope

Final native packaging integration uses actual merged dev baseline
`b58ec6d153d5b211b2941bf0d603ea3a3108a68b` (B), including PR 158
archive ingestion and PR 159 native input isolation. The original PR 152 head
`56aad24e0c73b31a9255e37adaa0abcd812a7c42` and B must both remain
ancestors of the final integration commit (C); publication is a fast-forward.
Preserve the existing installed
`catalog-prepare` and `polymarket-history` command contracts through regular-file
launchers. Each launcher executes the fixed packaged `source-tools` path, a
literal selector and quoted original arguments. The shared first-party modules
retain standalone Cargo binaries with their existing feature requirements; the
combined binary requires both optional features. The catalog branch stays
synchronous and the history branch retains its Tokio runtime.

Build and strip one operator ELF. Do not change third-party implementations,
locked dependencies, compiler/profile flags, the default scientific job/cache,
host-extracted service binaries, updater bundle members/schema, source mount
and ownership guards, or installed native override rejection. Preserve the
merged native collector
and its closure. The exact build/install/strip/hash substitution is in
`native-build.sh`; its server branch is unchanged. Fixed launchers are copied
only in the final packaging stage, outside Rust inputs. The unchanged
Dockerignore already admits them through its `!deploy/docker/` parent rule.

## Acceptance

The native history unit suite moves with its source into feature-gated library
modules. Exact CLI comparisons cover help/version, all history subcommands,
catalog root/ingest-candles/ingest-archive-candles modes, invalid arguments and
runtime failures, including
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
The required single-ELF/payload/full-image/gzip measurements, the original two
installed data paths and honest cold/same-builder-warm total timings are retained.
The final integration also retains the installed archive path.

## Historical evidence status

The original PR 152 delivery recorded local locked/offline Cargo checks and
focused Python checks. Local disk is too constrained for release linking and
Docker is unavailable. Hosted evidence is tracked in
[PR 152](https://github.com/zhengui666/QuaZonai/pull/152); current qualification
limits are recorded below. No controlled size or speed gain is claimed.
Codex Agent evaluation remains UNRUN and is not needed to substitute for native
packaging tests. Independent final-head review and all applicable CI are required
before dev merge; no live Coinbase acquisition is part of this batch.

The bounded local checks also validated the synthetic three-BAR history fixture
with already-existing legacy history/catalog executables. Import remained
`UNPROVEN`/`UNVERIFIED`, preparation remained `FIXTURE`/`UNVERIFIED`, and original
native definitions matched. This checks fixture construction only; those older
executables do not establish the changed dispatcher's or packaged image's result.

## Historical controlled comparison repair

At head `8d70f49`, all thirteen ordinary hosted jobs passed, including actual
dispatcher/history, installed source paths, Worker/browser, Runtime and Container
acceptance. The additional cost comparison failed: the two measurement hosts
had different CPU models, runner images, memory totals and Docker/containerd
identity. Original server/runtime ELF sizes matched but their hashes differed;
the cause is unresolved and admission remains blocked. Observed full-image bytes
were 619,747,868 versus 507,052,423; gzip bytes were 237,495,992 versus 196,774,504.
These retained observations are not controlled attribution or a speed claim.

The repaired experiment measures both variants sequentially on one hosted
runner, each with an independently verified empty builder. Its 95-minute job
reserves time for cleanup and evidence; each variant requires at least 40 GB free
or reports blocked. Cleanup removes only owned resources and restores temporary
archive aliases. Reports retain all mismatches, input failures, original ELF
bytes and binutils diagnostics before nonzero exit. Application hash equality
is now mandatory; no compiler flags or bytes are normalized to force equality.

The fixed historical baseline is an experiment for this delivery. Automatic
triggers cover its workflow and comparison helpers; manual execution remains
available. Ordinary production installation/function gates are unchanged. The
actual final repair head must pass a new paired hosted run and all applicable CI;
that run was still **UNRUN** at that repair stage. Independent review reproduced and resolved numeric
overflow reporting and expired-deadline archive-alias cleanup failures. Thirty-one
focused helper tests pass; they do not establish hosted measurement validity.


## Final source and native-input integration

B is the actual dev merge of PR 159, tree
`3bbb75e583f012bc507b25a291bd7ab6fe155724`. Its successful same-builder
input-noise probe establishes warm reuse only; it does not establish a
five-minute whole-CI result. UI, Web workflow splitting and later ripgrep work
are separate batches, not prerequisites for this final native recipe.

The catalog archive, shared Bars and candles modules move from bin to operator
with identical bytes. Classic ZIP changes only its test-relative Bars path.
History helpers retain their original bytes apart from three test-relative
import paths. All archive dependency versions/features, Cargo.lock, third-party
notices/license bytes and checksums remain B's originals. Standalone binaries
and the shared dispatcher call the same feature-gated implementations.

Installed acceptance now executes Coinbase, history and archive paths before
its shared success marker. Archive freeze/verify/convert/preparation still uses
the registered source path and installed catalog launcher, with original hashes,
receipt clocks, unqualified provenance, native definitions/readback and output
reuse rejection. No alternate archive kernel, market or scenario is added.

The current cost/probe helpers explicitly distinguish B's two real operator
ELFs from C's one `source-tools` ELF plus two exact regular launchers. Producer
schema 2 keeps its original input-bound timing and hash meaning; report schema 3
adds explicit layout/current-source identity without promoting historical schema
1 or reinterpreting old schema 2 reports. The final comparator uses one immutable
harness/recipe for both layouts and requires identical common server/runtime
ELFs in full and same-source no-operator images. Unexplained ELF differences
fail after retaining the observations, original binaries and diagnostics.

Historical run `36860149322` compared the pre-archive/pre-input-isolation pair
and passed its controlled size gates: about 112.7 MB full-image and 40.7 MB actual
gzip reduction. It did not establish a cold speedup. That evidence remains
historical; it is not acceptance for B/C. The final B/C pair is UNRUN and retains
one runner, sequential variants, independent empty owned builders, 40 GB
admission per variant, 85-minute measurement/95-minute job limits and owned
cleanup. No cache, hash, size, gzip, provenance or ordinary CI gate is removed.

Local preparation validates mechanical relocation, unchanged locked/licensed
inputs, Python source/smoke/projection helpers and installer behavior. It does
not link Rust, run Docker or qualify installed image behavior. Exact frozen
staged-tree/patch identities and executed checks accompany review. Final C needs
independent native review, all applicable final-head CI, the actual three-path
Container smoke and one final controlled B/C comparison before dev merge. A
changed C or harness invalidates that final-head qualification.
