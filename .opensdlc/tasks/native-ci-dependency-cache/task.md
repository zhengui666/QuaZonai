# Native CI dependency cache pilot

## Scope and baseline

Bounded performance pilot from `dev` at
`b66263507f50d39013daee5537d3ef8cacf10f71`, following the owner's native-only
design, implementation and independent-review instruction. Codex remains an
existing runtime under acceptance test, not an author or reviewer. No local
`.agents/skills` exists. PR link and final-head hosted evidence are pending.

Two green final-head Container jobs identified the host release compilation as
the largest measured cost:

| Job | Total | Host Job release build | Application Docker build | Smoke | Cargo downloads |
| --- | --- | --- | --- | --- | --- |
| [PR 137](https://github.com/zhengui666/QuaZonai/actions/runs/36752114659/job/110013345508) | 42m54s | 27m21s | 11m35s | 1m49s | 4.65s |
| [PR 138](https://github.com/zhengui666/QuaZonai/actions/runs/36752854934/job/110015594015) | 32m03s | 19m31s | 8m24s | 1m43s | 4.59s |

Totals include setup and cleanup. Both heads passed all twelve jobs. Runner
variability makes these bottleneck observations, not an A/B speedup measurement.

## Design

The shared [container action](../../../.github/actions/container/action.yml)
adds `cache-native-dependencies`, default `false`. Only the existing Container
workflow opts in on dev pushes or dev-targeted PRs. Publishing, version release
and Dev image callers remain uncached, including the latter's `source: source`
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
save boundaries; priming/clean durations are logged separately. Require 5 GiB
free before restore (up to 2 GiB archive, 2 GiB extraction and 1 GiB reserve).
Skip save if empty, larger than 2 GiB uncompressed, or if free space is less than
the archive size plus 1 GiB. No storage quota, retention or billing setting changes.
Cache service failures are optional; actual build/acceptance failures remain fatal.

## Verification and outstanding evidence

- Fourteen focused Python policy/key/layout/pruning/order/size tests pass, covering dev/fork gates,
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
