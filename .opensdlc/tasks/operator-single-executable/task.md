# Single installed operator executable

## Scope

The final packaging comparison uses the actual PR 162 dev merge
`544ee6e042b297c0943decb06aa4ea2ce0b92089` (M) as B. Its tree is identical to
the producer candidate `dc9c8cf30704781615ba6524f50368d0c148d9b3` (P), whose
bounded A/A and compatibility evidence passed as recorded below. That diagnostic
does not qualify packaging. Historical b58 evidence stays historical. The original PR 152 head
`56aad24e0c73b31a9255e37adaa0abcd812a7c42` and the selected exact baseline must both remain
ancestors of the integration commit (C). The prepared integration preserves
the existing diagnostic head `a299344e4be376ec4ed8e7d01e45468c4c3c1fd6`, its
portable-fixture correction, and M as ancestors. M introduces no additional
source changes beyond P; the consolidated operator and its producer remain intact.
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
`native-build.sh`; its server branch is identical to the selected producer baseline. Fixed launchers are copied
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
application-only repetitions were UNRUN at that preparation stage.

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

The subsequent capture head enabled `--prestrip-aa-only`. It ran one
A1/A2 pair at the historical baseline and disabled C even if A/A matched. The
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
444 MiB, with 128 MiB reserved for the released reference and a separate
8 MiB report reserve. That per-repetition allowance
reserves 64 MiB per symbol output and 8 MiB for metadata before allowing the raw
copies. The small artifact in this mode is limited to 8 MiB including its
manifest; original pre-strip ELFs and symbol texts remain in the original large
archive. The existing 35-minute total deadline, 3-minute owned-cleanup reserve,
40 GB admission checks, full-mode 85/95-minute limits and unconditional evidence
uploads remain unchanged. Failed controls stop further repetitions.

Actual linker argv is explicitly unavailable: no logging hooks are added to
the production build. The production helper entrypoint and available producer
tool versions are observations, not proof of which Cargo or linker invocation produced
the files. Upstream LLVM's whole-link-output build-ID mechanism motivates this
capture but does not establish the actual linker's identity or the failure's
cause. That pre-strip A/A run was UNRUN at the original preparation stage. It cannot replace full current-base packaging qualification.

## Fixed-producer repetition preparation

The original pre-strip diagnostic subsequently ran as `36962973323` at
`cbe9bd11746b2de17f9573d2afb5134fbf44cea3`. Its historical evidence remains
separate from the proposed producer correction and does not qualify packaging.

Forensic recipe metadata records the actual production entrypoint
`sh deploy/docker/native-build.sh server`, together with a byte-exact copy of
the selected source's `native-build.sh`. The helper must be a regular file no
larger than 64 KiB, and its copied-byte SHA-256 must equal the native helper
hash from source verification before any build starts. Its original bytes are
allowlisted in the small artifact. The metadata no longer hardcodes Cargo
arguments that could become stale when the producer changes. Capture still
does not instrument or modify the production Dockerfile prefix or helper.

The exact producer candidate P is now
`dc9c8cf30704781615ba6524f50368d0c148d9b3`, with tree
`4ad08b0fb89322e2e02b2a0e64b10b386ebfc5e6` and sole parent
`183fe2c4bfec6a04bece3a550cc705217ab391d3`. The prepared C mechanically
integrates P. Comparator and workflow pins both select P for the bounded A/A;
P is a producer PR candidate, not a dev merge. No corrected-producer hosted
result is claimed before execution. P's standalone layout and
C's consolidated layout must share the identical fixed common native recipe;
positive operator substitutions, source binding, ancestry of P and the prior
packaging head, original whole-ELF equality and diagnostic-only rejection all
remain required. Do not inject new flags into the historical b58 baseline.

After P's applicable CI and fixed-producer A/A evidence pass, merge the producer
change first. Final PR 152 qualification must use that actual dev merge M as B,
integrate M into C, and restore the complete full comparison. An unmerged P,
earlier baseline or successful diagnostic operation cannot substitute for M's
full current-base qualification.

## Bounded dynamic-link and unwind compatibility evidence

The fixed-producer A/A also enables `--verify-producer-compatibility`. Before
compiling, it checks 40 GB headroom and extracts original server/runtime ELFs
from the verified released 183fe image
`ghcr.io/zhengui666/quazonai@sha256:a01c07cc9ce2ce0132d85e8bce28466f8784178b9dbd052119844085d43b76f5`.
The immutable digest, revision label and linux/amd64 platform must match. The
reference application is never started, and no old source is compiled. Only an
owned UUID container is created and removed after identity-checked cleanup;
pre-existing/shared images are not removed. Reference originals have a combined
120 MiB file-size allowance enforced before writes, leaving 8 MiB of its
128 MiB envelope for metadata. The full retained-evidence ceiling stays 1 GiB.

For reference and fixed outputs, bounded GNU dynamic-symbol, dynamic-tag and
version-info text is retained. The comparison resolves version IDs to provider,
version name, flags and hidden/default semantics, and compares loader-required
symbol names, types, binding, visibility, definition kind, object/TLS sizes,
DT_NEEDED order and loader options. Raw dynamic section SHA-256 values remain
observations: old/new addresses and numeric version IDs may legitimately
change. No ELF is transformed and no original whole-ELF gate is weakened.
Unparseable, missing, oversized or erroneous GNU output blocks the diagnostic.

For every fixed ELF before and after final stripping, nonempty `.dynsym`,
`.dynstr`, `.eh_frame`, `.eh_frame_hdr`, `.gcc_except_table` and the GNU build-ID
note must exist. `PT_GNU_EH_FRAME` must describe `.eh_frame_hdr` at its recorded
file offset, address and size. GNU frame decoding must finish with CIE/FDE
records and no warnings or errors. Each generated decoder output is limited to
64 MiB before growth; status, complete-output hash, byte count, record counts,
warnings and a bounded excerpt survive in the small report. Only generated
temporary decoder text is removed afterward. Original ELFs remain untouched.

Static `.symtab`/`.strtab` must already be absent before final stripping. The
nonempty genuine build ID, dynamic/unwind section bytes and decoded loader
semantics must survive that final strip unchanged. A1/A2 whole-file SHA-256 and
length equality remain exact requirements separately before and after strip.
The existing 35-minute absolute deadline, 3-minute cleanup reserve, 40 GB
per-build admission and disabled C remain. All successful diagnostics still
carry `DIAGNOSTIC_ONLY` and fail full packaging qualification; actual fixed-
producer hosted A/A, final-head CI and independent review remain required.

## Portable GNU mutation fixtures

The two parser-mutation regressions now use explicit, complete GNU-style text
with the FUNC import and unversioned GLOBAL export that their mutations require.
Those properties are not guaranteed by a runner's `/usr/bin/true`. The real GNU
loader, frame decoding and native-structure checks remain, as do every parser
rejection assertion and the production compatibility and whole-file A/A gates.
This fixture correction is not native producer or installed-image acceptance.

PR 162 merged into dev as `544ee6e042b297c0943decb06aa4ea2ce0b92089`.
The fixture-only correction preserved the diagnostic's immutable pins and
disabled C. The final integration below moves the workflow to full M/C comparison
without relaxing the diagnostic-only rejection or production checks.

## Final merged-producer comparison preparation

[Fixed-producer run 36969878498](https://github.com/zhengui666/QuaZonai/actions/runs/36969878498)
completed its native diagnostic successfully. The job failed only at the deliberate
packaging-qualification rejection. The retained diagnostic reports complete
producer checks with no reasons, exact server/runtime A/A whole-file equality
before and after stripping, compatible decoded loader semantics, complete
dynamic/unwind/strip checks and successful owned cleanup for both repetitions.
Its small artifact SHA-256 is
`ec4848c602d1087ce530f1066fd6db61dd753332b2307296fe390dcc46ea5ba9`.
P's applicable PR workflows passed, and M has the same exact source tree
`4ad08b0fb89322e2e02b2a0e64b10b386ebfc5e6`. This establishes the prerequisite
to resume the existing full comparison; candidate C was not built by that run.

The workflow now selects its existing `full` branch and pins both source checkout
and comparator to M. The sequential old/candidate measurement is retained from
the prior full packaging workflow. Its 85-minute measurement / 95-minute job,
40 GB admission per variant, independent empty builders, original evidence,
strict whole-ELF equality and measured payload/image/gzip reduction gates stay
unchanged. Diagnostic mode remains non-qualifying if selected again.

This preparation has not executed the final M/C comparison or final-head hosted
CI. Merge remains blocked until the exact integrated C passes the full comparison,
applicable Web, Worker, Runtime, Container and installed source acceptance, with
independent integration review. Neither the earlier A/A success nor local Python
and shell checks establish those final delivery gates or a five-minute CI result.
