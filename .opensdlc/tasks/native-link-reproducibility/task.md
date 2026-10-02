# Stabilize native application links

## Scope and evidence

Producer prerequisite for [PR 152](https://github.com/zhengui666/QuaZonai/pull/152),
based on dev `183fe2c4bfec6a04bece3a550cc705217ab391d3`. The separate producer PR
and exact-Head review/CI are pending. This changes the Docker producer recipe;
it does not establish a reproducible build, a source-level ThinLTO fix or a
five-minute CI result.

The existing same-source [A/A diagnostic run](https://github.com/zhengui666/QuaZonai/actions/runs/36962973323)
at harness `cbe9bd11746b2de17f9573d2afb5134fbf44cea3` reproduced both historical
server/runtime hashes. Complete byte partitions differed only in static string
tables and GNU build IDs before post-link stripping, and only in build IDs
afterward. Static symbol-table bytes, addresses and sizes matched. The bounded
symbol-name samples differ in LLVM local-promotion module-hash suffixes; the
upstream input responsible for that module-hash drift remains unknown.

## Recipe and identity

The [recipe](../../../deploy/docker/native-build.sh) retains one combined
`cargo build --locked --release -p server -p runtime` invocation and adds
`profile.release.package.server.strip="symbols"` and
`profile.release.package.runtime.strip="symbols"` through `--config`.
[Cargo's versioned documentation](https://github.com/rust-lang/cargo/blob/797e8a9bc/src/doc/src/reference/config.md#command-line-overrides)
supports these command-line package-profile overrides.
The [profile implementation](https://github.com/rust-lang/cargo/blob/797e8a9bc/src/cargo/core/profiles.rs)
matches overrides by package and includes `strip` in profile identity.
The two entry packages therefore acquire new Cargo metadata identities;
[Cargo metadata](https://github.com/rust-lang/cargo/blob/797e8a9bc/src/cargo/core/compiler/build_runner/compilation_files.rs)
also influences symbol names. Old/new executable-byte equality is not assumed.
Dependency profiles and the combined feature graph are retained; splitting the
build into per-binary `cargo rustc` calls would change feature unification.

[Rust 1.98.1](https://github.com/rust-lang/rust/blob/1.98.1/compiler/rustc_codegen_ssa/src/back/linker.rs)
maps symbol stripping to GNU `--strip-all`. In the observed LLVM revision,
[LLD section creation](https://github.com/llvm/llvm-project/blob/52ed14fcd56afc30f9cccd8ca8ce237c2eef7e04/lld/ELF/SyntheticSections.cpp)
omits static `.symtab`/`.strtab` with that option while dynamic symbols and unwind
sections have separate construction paths.
[LLD output writing](https://github.com/llvm/llvm-project/blob/52ed14fcd56afc30f9cccd8ca8ce237c2eef7e04/lld/ELF/Writer.cpp)
then computes its build ID from the output. This is the supported mechanism to
exclude discarded static names before build-ID calculation, not an observed
successful build of this candidate.

The operator command, installation paths/modes and final `strip` calls remain.
No global Cargo profile, upstream source, target, optimization, scientific rule,
license, CLI contract, build-ID option or comparison gate changes. The existing
[native identity](../native-build-inputs/task.md) hashes the recipe's bytes;
both native input and recipe digests change and invalidate old producer evidence.
The final image retains the genuine linker build ID and whole-ELF SHA-256 checks.

## Validation and merge conditions

Local command-boundary tests execute the shell recipe with intercepted commands,
checking argument quoting, the single combined build, package-only overrides,
installation and rejection of stale outputs after build failure. Projection
tests check both digests when either strip setting changes. Together with
existing cache-probe/cost tests, 32 focused tests pass. These use no Rust/Docker
build and cannot establish ELF or installed behavior.

Before merge, a bounded hosted A/A must use this exact source and pinned recipe
in two independent target caches, record actual completed native build commands,
and compare the original whole server/runtime ELFs including genuine build IDs.
For each real ELF before and after final stripping, retain bounded `readelf -W`
section, program-header, dynamic-symbol, dynamic-tag and note evidence. Stream
`readelf -W --debug-dump=frames`, retaining its status, hash and bounded excerpts
within the diagnostic budget. Require nonempty `.dynsym`/`.dynstr`, unchanged
loader-required symbol names/bindings/visibility/versions and `DT_NEEDED`
dependencies compared with the old producer. Unwind sections `.eh_frame`,
`.eh_frame_hdr` and `.gcc_except_table` must remain present; frame decoding must
succeed, and `PT_GNU_EH_FRAME` must still cover `.eh_frame_hdr`. Old/new code
addresses may change with Cargo metadata. Static `.symtab`/`.strtab` must already
be absent before final stripping, and the nonempty build ID must survive that
step unchanged. Require A1/A2 whole-ELF equality separately before and after
stripping, rather than accepting equal parsed summaries. Record current
input/recipe digests and preserve original producer metrics. Cache hits cannot
substitute for independent compilation evidence.

All applicable final-Head CI remains required, including installed CLI/help,
container installation/update/recovery and scientific/OCI/native-data-browser
checks, plus independent native review. Only the actual merged dev SHA may
become PR 152's new fixed B, with both B/C using the same producer recipe and
unchanged whole-ELF gates and cost limits. Historical diagnostics and benchmarks
remain historical. Hosted A/A, live ELF inspection and those acceptance checks
have not run for this local candidate.
