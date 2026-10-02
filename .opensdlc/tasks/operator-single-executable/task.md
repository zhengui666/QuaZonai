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
historical; it is not acceptance for B/C. At that preparation point the final B/C pair was UNRUN and retained
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

## Common-ELF failure and bounded diagnostic retention

The subsequent exact B/C run `36925840314` at candidate
`d4853685809ab948a376eaf7078a8f15c96b925f` failed admission. Its only equality
failures were the common server/runtime ELF hashes; their sizes matched. It
observed 112,954,992 fewer full-image bytes and 40,832,372 fewer actual gzip
bytes, but those observations do not establish an admissible packaging result.
The original binaries and section diagnostics remain in its 105,328,725-byte
artifact. Available file materialization could not retrieve that artifact, so
no section-level cause was established from the historical run.

Keep the original full artifact unchanged. A separate diagnostic collector
copies an exact allowlist of original reports and GNU section/header/size
observations into a new directory. Each file is bounded at 4 MiB and the total
including its manifest at 16 MiB. The manifest records missing files, byte
lengths and SHA-256, and explicitly makes no admission or root-cause claim.
It rejects symlinks, non-regular files, existing output and budget overflow.
ELFs, arbitrary logs and unrelated files are excluded only from this additional
small artifact. The original archive still retains them without truncation.

Comparison failure cannot be hidden: successful collection preserves its exit
status, and collection failure also fails the job. A malformed or absent
section report is explicitly unavailable, not invented as equal.
Section differences are also printed to the job log so a future file transfer
problem does not conceal which original sections differ. No byte normalization,
build-ID removal, dependency patch, hash waiver or scientific test change is
part of this retention fix. Its local tests do not establish hosted equality.

Current dev has subsequently advanced to
`183fe2c4bfec6a04bece3a550cc705217ab391d3`. This diagnostic preparation still
addresses the historical b58/d485 production inputs. Final delivery must
integrate the actual dev baseline and perform a new complete qualification;
neither a local mechanism probe nor a diagnostic-only run qualifies that merge.

## Bounded application-ELF diagnosis (not mergeable)

The current investigative workflow deliberately selects
`application-elf-diagnostic`. Its reports have a separate schema and kind,
permanent `DIAGNOSTIC_ONLY` qualification, `admissible: false`, and a distinct
diagnostic completion status. The full comparator explicitly rejects these
reports, and the workflow comparison step fails even when diagnosis completes.
This investigative head must not merge. Final delivery must restore full mode,
integrate the actual current dev baseline, and obtain a new complete B/C
qualification, installed tests, applicable final-head CI and independent review.

Diagnosis uses the unchanged production Dockerfile with `--target server`, its
original `native-build.sh server` invocation, compiler, flags and native source
closure. It builds no operator, web or full application image. Each sequential
repetition has a distinct, verified-empty owned BuildKit builder, no cache
imports, and a 40 GB precheck. Identity-checked cleanup must complete before
another repetition starts; no global cleanup is permitted.

The smallest initial repetition control is A1/A2 at the same immutable historical
baseline revision and resolved checkout path. Both retain their source path,
revision, tree, native closure, harness and runner identities. Original
`/out/server` and `/out/runtime` bytes are extracted directly from the server
stage, alongside the producer's `/build/.native-*` source digests and GNU ELF
diagnostics. The producer digests must equal the independently verified source
closure. Docker's compilation path remains `/build` for every repetition.

A1/A2 compare both complete original ELF SHA-256 and byte lengths. Any mismatch
ends expensive work immediately with the evidence retained; failed controls or
cleanup also block further builds. Candidate C runs only after exact A/A
equality and enough remaining budget based on observed repetition durations.
The summary and small artifact distinguish AA from BC observations. All
original ELFs stay in the original archive without normalization or truncation.
Missing or malformed observations remain unavailable. No size, speed,
installation or admissible packaging claim follows from diagnostic completion.

One absolute 35-minute deadline starts before workflow setup and covers all
diagnostic repetitions, reserving its last 3 minutes for owned cleanup. The
local comparison/collection step is bounded at 1 minute, the additional small
artifact upload at 1 minute and the original full archive upload at 8 minutes.
The existing full mode retains its 85-minute measurement and 95-minute job
limits, with those evidence steps fitting its 10-minute reserve. The workflow
keeps the same 95-minute outer job bound in either mode.

A separate bounded local leaf probe found that changing `iso_currency` data
ordering changed the dependency rlib hash, while repeated A/A builds and the
dynamic `from_code` caller's original and stripped ELFs, all sections and GNU
build IDs remained identical. This excludes only that local release-style
closure; its linker/system libraries differ from the pinned production Docker
environment. It is neither a production repeatability result nor evidence of
the hosted failure's cause, and no dependency change follows from it. Local
Python checks validate orchestration and evidence handling only; production
application-only repetitions remain UNRUN at this preparation stage.

## Same-source failure and original pre-strip evidence

The subsequent hosted run `36957287977` at
`1fc1e74de2888e0a7a10c07a6247ae1b07f81c3a` completed the diagnostic operation
and correctly failed qualification. A1/A2 used the same b58 source, checkout
path, native input/recipe identity and runner, with distinct verified-empty
builders and completed owned cleanup. Their original whole-ELF hashes differed
and reproduced the two historical B/C identities; candidate C was not started.
Only `.note.gnu.build-id` differed among the 28 sections captured for each
binary, with matching GNU header/layout observations. That section list omitted
`.shstrtab` and did not cover file gaps or original pre-strip symbol/string
tables, so it does not prove all other link-output bytes were identical.

The next investigative head enables `--prestrip-aa-only`. It runs exactly one
A1/A2 pair at the historical baseline and disables C even if A/A matches. The
production Dockerfile remains an exact byte prefix of a separately recorded
diagnostic Dockerfile; no trailing newline is normalized. The original
Dockerfile-specific ignore file is copied unchanged alongside the diagnostic
Dockerfile, retaining the same context and native source identity. The original
production recipe digest, executed Dockerfile digest and appended capture
suffix digest are separate observations and must match between repetitions.

The appended stage derives from the completed production `server` stage and
mounts its `/build/target` cache read-only. It never invokes Cargo or changes
the original command, environment, helper, compiler, linker flags or source.
It retains original `target/release/server` and `runtime` alongside the existing
`/out` binaries, ordered GNU symbol tables and producer tool versions. Original
sizes are checked before copying. Each symbol output has an operating-system
file-size limit before output starts. Missing or oversized originals and symbol
output failure write an explicit blocked capture status; the completed image
still permits retention of the original `/out` evidence before the diagnostic
operation stops. No capture failure can admit the comparison or start A2.

GNU `readelf` offsets and lengths define checked file-backed section ranges,
including `.symtab`, `.strtab`, `.shstrtab` and any debug sections that exist.
Coverage also hashes the ELF/program/section headers and every gap or trailer.
Missing tables, invalid counts, overlapping ranges and ranges beyond the
original file block the observation. The disjoint interval partition covers
every original byte, and its whole-file digest must match the existing stripped
ELF observation. The original binaries are never rewritten or normalized.

The runner also streams the two original ordered symbol texts, without sorting
or interpreting symbols. `diagnostic.json` retains each complete file's original
byte SHA-256 and line count, the total differing-line count, and the first 32
differing original line pairs. Each preview is bounded to 1,024 bytes and has a
byte-preserving base64 prefix, readable escaped text, original line length/hash
and an explicit truncation flag. Missing lines are explicit nulls. Sampling or
output truncation never substitutes for the total count or whole-file hash;
each pair report is bounded to 512 KiB. Unavailable or incomplete comparison is
explicit and blocks diagnostic completion. These results travel in the small
artifact, while the untouched complete symbol texts remain in the large archive.

Extra retained evidence has a 1 GiB ceiling: each repetition reserves at most
508 MiB, with a separate 8 MiB report reserve. That per-repetition allowance
reserves 64 MiB per symbol output and 8 MiB for metadata before allowing the raw
copies. The small artifact in this mode is limited to 8 MiB including its
manifest; original pre-strip ELFs and symbol texts remain in the original large
archive. The existing 35-minute total deadline, 3-minute owned-cleanup reserve,
40 GB admission checks, full-mode 85/95-minute limits and unconditional evidence
uploads remain unchanged. Failed controls stop further repetitions.

Actual linker argv is explicitly unavailable: no logging hooks are added to
the production build. The exact original Cargo command and available producer
tool versions are observations, not proof of which linker invocation produced
the files. Upstream LLVM's whole-link-output build-ID mechanism motivates this
capture but does not establish the actual linker's identity or the failure's
cause. This next pre-strip A/A run remains UNRUN until independently reviewed
and executed. It cannot replace full current-base packaging qualification.
