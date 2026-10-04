# Native CI dependency cache pilot

## Scope and baseline

Bounded performance pilot from `dev` at
`b66263507f50d39013daee5537d3ef8cacf10f71`, following the owner's native-only
design, implementation and independent-review instruction. Codex remains an
existing runtime under acceptance test, not an author or reviewer. No local
`.agents/skills` exists. Delivery is tracked in [PR 140](https://github.com/zhengui666/QuaZonai/pull/140); final-head checks remain mandatory.

Two green final-head Container jobs identified the host release compilation as
the largest measured cost:

| Job | Total | Host Job release build | Application Docker build | Smoke | Cargo downloads |
| --- | --- | --- | --- | --- | --- |
| [PR 137](https://github.com/zhengui666/QuaZonai/actions/runs/36752114659/job/110013345508) | 42m54s | 27m21s | 11m35s | 1m49s | 4.65s |
| [PR 138](https://github.com/zhengui666/QuaZonai/actions/runs/36752854934/job/110015594015) | 32m03s | 19m31s | 8m24s | 1m43s | 4.59s |

Totals include setup and cleanup. Both heads passed all twelve jobs. Runner
variability makes these bottleneck observations, not an A/B speedup measurement.

## Design

The initial shared [container action](../../../.github/actions/container/action.yml)
pilot added `cache-native-dependencies`, default `false`. Only the existing Container
workflow opted in on dev pushes or dev-targeted PRs. Publishing, version release
and Dev image callers initially remained uncached, including the latter's `source: source`
checkout. The helper derives paths from the source working directory and accepts
both repository-root and nested checkouts. `contents: read` stays unchanged.

Use official [actions/cache v6.1.0](https://github.com/actions/cache/releases/tag/v6.1.0),
pinned to `55cc8345863c7cc4c66a329aec7e433d2d1c52a9`, with separate restore/save
actions. The only cached path is this checkout's `target/release`, after removing
all workspace outputs. Cargo home, registry/configuration/credentials, application
images, acceptance results and workspace executables are not cached by this pilot.
The existing Docker build cache is unchanged.

One exact versioned Job/default-features/release key hashes runner OS/architecture,
exact runner image identity, pinned Rust and observed compiler/tool identities,
compiler/profile environment, Cargo home location, Cargo configuration, `Cargo.lock`, every tracked
workspace `Cargo.toml` and `rust-toolchain.toml`. Missing inputs or custom target
layouts skip caching. There are no fallback restore keys; nonexact or unconfirmed
restores are discarded. A same-repository dev PR or dev push may save; fork PRs
only restore, under GitHub's native branch cache access rules.

After restore, a fresh runner first needs registry index metadata for Cargo to
select workspace packages. `cargo metadata --no-deps` does not provide that data.
The pinned [Cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html)
`cargo clean --locked --release --workspace --dry-run` primes native
metadata resolution without deleting outputs or acquiring extra crate sources.
It is bounded to 120 seconds. The actual removal then uses pinned
`cargo clean --locked --offline --release --workspace`, also bounded. Any priming
or pruning failure discards this disposable release tree and builds cold. This
is not a network-free warm path: Cargo still acquires official registry metadata
and normal build sources using its existing configuration and trust model.

The exact-head assertion remains first. The unchanged
`cargo build --locked --release -p job`, Rust 1.98.1, two Cargo build jobs,
release debug level zero, image assembly and every downstream acceptance check
still execute. After successful build/image assembly, repeat offline workspace
clean, then explicitly save dependencies before the existing release-tree
removal. Saving does not claim acceptance; independent Docker/Codex builds and
installation/update/recovery checks still follow on every run.

`du` and `df` log uncompressed bytes and available disk at restore, pruning and
save boundaries; priming/clean durations are logged separately. Require 7 GiB
free before restore (up to 3 GiB archive, 3 GiB extraction and 1 GiB reserve).
Skip save if empty, larger than 3 GiB uncompressed, or if free space is less than
the archive size plus 1 GiB. No storage quota, retention or billing setting changes.
Cache service failures are optional; actual build/acceptance failures remain fatal.

## Verification and outstanding evidence

- Seventeen focused Python policy/key/layout/pruning/order/size tests pass, covering dev/fork gates,
  exact restore handling, locked input and toolchain/profile invalidation,
  root/nested/spaced paths, empty inputs, source-code-only key stability,
  no caching credentials, unsupported target layouts and cold fallback
- Pinned Cargo 1.98.1 / Rust 1.98.1 tiny two-member workspace with a workspace
  build script and official strsim 0.10.0: clean removed all 31 workspace-owned
  release files and retained all seven dependency files with identical bytes,
  mtimes and hashes. Edited members/build script rebuilt; strsim remained Fresh
- Empty-home dry-run metadata priming took about six seconds in the tiny fixture,
  fetched index metadata only, and left all 38 artifact files unchanged. The
  following offline clean removed the 31 workspace files and preserved the seven
  dependency files. Cargo.lock SHA-256 stayed identical before, after dry-run and
  after offline clean (`d651e25360f1e84916895121219b4d1ef2158b6eebe03c4c0270bb24b4885c0b`)
- Fresh Cargo home at the same absolute path, retained dependency target and actual
  crate redownload also preserved Fresh dependency artifacts. Moving only the
  checkout/target path worked; changing Cargo home path invalidated reuse. This
  pilot relies on the standard hosted runner Cargo home layout
- Deployment packaging tests pass in default and `RELEASE_BRANCH=dev` modes
  (10 each); native-files Node tests pass (5)
- Docker deployment tests in both modes are blocked only by the existing
  localhost socket fixture's executor `EPERM`, including the permitted escalation;
  other tests pass. Hosted execution is still required
- All workflow/action YAML parses and shell snippets pass syntax checks;
  `git diff --check` passes. These are not a hosted workflow execution

Independent review, final-head hosted CI (including Dev image for the shared
action change), bounded-cache size evidence, and equal-check cold/warm hosted
measurements remain required before merge or any speedup claim. A miss, oversized
cache, metadata failure or external dependency rebuild is measured honestly and
must not weaken checks to make the pilot appear faster. The main performance
hypothesis is avoiding repeated third-party compilation, not reducing downloads.

## First hosted cold observation and bounded adjustment

Head `db2043bec423a3460fd906b863bccfde1d918234` passed all applicable
checks, including the additional Dev image job. The
[Container cold job](https://github.com/zhengui666/QuaZonai/actions/runs/36764703938/job/110055822340)
missed its exact cache key. Host Job release compilation took **21m01s**; native
offline workspace pruning took **0.264s** and removed 33 workspace files.
The retained dependency tree measured **2,435,668,032 bytes** (about 2.27 GiB),
with **85,863,993,344 available bytes** before saving. The initial 2 GiB guard
correctly skipped saving. No compressed archive, transfer size or warm reuse was
measured in that run, so it establishes no speedup.

The pilot ceiling is now a bounded **3 GiB uncompressed**, with **7 GiB** free
required before restore and the unchanged archive-plus-1-GiB reserve before
saving. The exact dependency key, native pruning, source boundaries, save
authority, cold fallback and all acceptance checks are unchanged. This does not
change repository storage limits, retention, billing or permissions. Two added
unit checks cover the measured tree and exact restore-headroom boundary.

The adjusted final Head still requires all applicable hosted checks, an actual
compressed archive/transfer-size observation, and an actual warm restore/build
measurement before any performance claim or merge. Docker's independent cache
and runner variability must not be attributed to this host-dependency pilot.

## Real warm result and native target collision

At head `9427b0a6`, the [cold job](https://github.com/zhengui666/QuaZonai/actions/runs/36770681869/job/110075979248)
compiled the host Job in17m49s and saved625,466,672 compressed bytes from a
2,435,668,032-byte dependency tree, with85,794,074,624 free bytes before save.
The [same-head warm job](https://github.com/zhengui666/QuaZonai/actions/runs/36770681869/job/110089143365)
restored that exact archive/key, then used2.420s metadata priming and0.315s native
pruning. Host build was16m56s. Compilation entries fell519→57, but that small
elapsed difference is not a reliable speedup claim; all13 checks passed and the
pilot remained draft while the expensive rebuilds were investigated.

An isolated Rust1.98.1 native reproduction confirmed that the auto-discovered
Runtime test target `http` collides with registry crate `http`. Cargo workspace
clean removed the external crate's `.rmeta` and `.d` files even when Runtime was
never built. QuaZonai's actual manifest, pointed only at the6.7-MB disposable
target, removed exactly those two files; the next build reported missing metadata
and rebuilt `http` plus its dependent. This follows Cargo's
[target-name matching](https://doc.rust-lang.org/stable/nightly-rustc/src/cargo/ops/cargo_clean.rs.html#358-420).
All54 external hosted warm recompiles are in the reverse-http dependency closure;
the other3 are intentionally cleaned workspace crates. The original hosted
fingerprint logs/archive were not inspected, so its exact causal attribution
remains an inference supported by the actual-workspace reproduction.

The narrow correction explicitly names that same unchanged test file
`runtime_http` in the Runtime manifest. Official Cargo metadata sees exactly one
target, the original test executes in the isolated control, and external `http`
stays Fresh after the identical clean. Inventory of164 workspace targets found
no other dependency-name collision; every package compiled by the cold run had
its cached manifest checked, including custom library names. No tracked command
references require migration, no test is disabled and native pruning stays intact.

The existing manifest hash automatically selects a fresh cache key, bypassing
the damaged immutable archive. A focused declaration regression protects this
mapping. Final-head native CI, independent delta review and a new cold/warm
measurement remained required at that point; the subsequent hosted observation follows.

## Post-correction hosted reuse

The corrected key's [cold host build](https://github.com/zhengui666/QuaZonai/actions/runs/36778373578/job/110101928467)
at `c92edbe03e34c51c5ef41c5f1e243e414c4f2f12` took **26m03s** and
saved **626,021,544 compressed bytes** from **2,437,374,242 dependency bytes**,
with **85,792,276,480 available bytes** before save. Its later application-packaging
stage was cancelled by a newer branch head; it is not an acceptance pass.

At integrated head `c057cfac4f2018b0892b39a496b971967d1a69ff`, the
[completed Container job](https://github.com/zhengui666/QuaZonai/actions/runs/36782360530/job/110115934310)
restored that exact archive/key. Restore took about **12.7s**, metadata priming
**2.869s**, and native pruning **0.214s**. The host Job release build then took
**1m37s** and compiled only the intentionally pruned `contracts`, `domain` and
`job` workspace crates. All external dependencies, including `http`, DataFusion
and Nautilus, were reused. The compared Job/contracts/domain source, locked
manifests and toolchain inputs are identical across these two heads; the same
versioned key includes their manifest/toolchain configuration.

This establishes a useful reduction in the observed host compilation stage,
not a whole-CI A/B result or a guaranteed speedup on every runner. The independent
application Docker build still took **9m33s**. The corrected cold job was cancelled
after the host/cache stages, while the integrated warm Container job completed
its install/update/recovery checks. Other exact-head checks remain independent;
in particular, the Web retry regression must be fixed before this PR can merge.

## Browser gate recovery

A separate hosted upload regression at PR 145 head `895ad435` established that
the official loading icon contributed `loading` to a retry button's accessible
name after pending ended. This branch receives the same application-wide
supported AntD `button.loadingIcon` configuration with a decorative hidden icon.
Visible text, busy state, mutation guards and exact-role browser clicks remain.
Data-validation pending/error/retry regressions and native AntD rendering checks
cover the common behavior. The previous PR 140 timeout's identical cause is not
proven by a passing diagnostic run. At `3c8023d`, the [complete Web job](https://github.com/zhengui666/QuaZonai/actions/runs/36787717263/job/110133022791)
passed. Its [bounded diagnostic artifact](https://github.com/zhengui666/QuaZonai/actions/runs/36787717263/artifacts/11131165277)
observed an enabled, nonbusy retry with one unhidden loading icon and zero exact
role matches; the accessible name became valid 38 ms later, and the whole case
finished in 3,873 ms. This establishes transient name contamination, not the
earlier persistent timeout. Temporary count/phase logging is removed after
retaining this evidence; useful accessible-name assertions remain.
No retry timeout, acceptance check or scientific rule is loosened.

## Dev-release caller reuse, 2026-10-01

The [version publisher](../../../.github/workflows/release-version.yml) now opts
into the same bounded dependency cache only when its branch input is `dev`, the
caller event is `push`, and its ref is `refs/heads/dev`. Main, tag and manual
release callers and the separate Dev image workflow remain excluded. The
default remains false. The existing dev-push Container workflow can seed the
same branch-scoped cache before the exact-source release gate succeeds.

Both callers use the same root checkout, pinned compiler, default Job features,
release profile and shared action. No helper, key, path, permission, save policy,
3 GiB ceiling, 7 GiB restore headroom, native workspace pruning or cold fallback
changes. Every build and acceptance stage still runs; a cache hit is not a
source or test result. GitHub preserves the caller's event/ref context in
[reusable workflows](https://docs.github.com/en/actions/reference/workflows-and-actions/reusing-workflow-configurations#github-context).

At source `bfa3cfc`, the successful [dev Container job](https://github.com/zhengui666/QuaZonai/actions/runs/36811437274/job/110207133814)
restored **626,025,650 compressed bytes** and retained **2,437,374,242 dependency
bytes** after pruning. It rebuilt only `contracts`, `domain` and `job` in
**1m53s**. The same-source [uncached release](https://github.com/zhengui666/QuaZonai/actions/runs/36811438022/job/110217444967)
compiled the host Job in **25m36s**. Their runner image revisions differ
(`20260920.314.1` and `20260927.320.1`), so the unchanged exact key would miss
between this particular pair. These observations motivate reuse; they do not
measure the new caller or establish a controlled release speedup. Its independent
application build took **40.403s** with existing BuildKit hits; the stored operator
producer duration is not a fresh compilation measurement.

The caller regression keeps Container's dev/fork policy and the Dev image
exclusion, and checks all three release opt-in conditions. Final-head review,
the complete applicable CI and deployment tests remain required. The publisher
is push-only, so this changed caller cannot be executed by a PR check: an actual
authorized dev release must still demonstrate the exact key/matched key, cache
bytes and headroom, fresh first-party compilation, source-bound native OCI,
normal smoke, published image/bundle readback and fresh no-checkout cold install.
Until those observations exist, release cache reuse is **unverified**. Record
miss/cold fallback honestly, and keep cache overhead, host compilation, application
build and whole-release duration separate. Do not widen keys to manufacture hits.
