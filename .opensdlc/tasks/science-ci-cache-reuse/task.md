# Reuse the dev science cache before merging

## Problem and evidence

The science job selected debug for pull requests and release for pushes. GitHub
allows a pull request to restore its base branch's cache, but the different
profile keys prevented that reuse. A pull request's merge-ref cache also cannot
warm another pull request or the subsequent dev push.

- [PR 274 science job](https://github.com/zhengui666/QuaZonai/actions/runs/41967874248/job/127049042710)
  used `ubuntu-22-debug`, reported `No cache found`, and compiled Arrow and
  Nautilus dependencies.
- [Dev science job](https://github.com/zhengui666/QuaZonai/actions/runs/41970812010/job/127058410531)
  restored the full `ubuntu-22-release` key (1,251,286,080 compressed bytes).
  Its subsequent compilation was limited to workspace crates. Rebuilding these
  is intentional with `cache-workspace-crates: false`, not a cache miss.

## Change

Use release for the science job on every event. PR builds, tests and native
artifacts now use the same profile as the dev push and can restore its existing
dependency cache. Keep compiler/environment/lockfile invalidation, standard and
high-precision builds, source compilation and all existing scientific checks.
Do not broaden cache keys or treat a hit as test evidence. Local development
and separate manual/full-regression workflows retain their existing profiles.

## Acceptance

Run the existing deployment/workflow checks and inspect the exact PR Head's
hosted science log for the release cache key, successful restoration and actual
dependency compilation reuse. Require all applicable CI and independent clean
final-Head review before merging into dev. Verify the resulting dev push CI.
Cache eviction or changed dependency/toolchain inputs can still cause legitimate
cold builds; no timing threshold or unconditional hit guarantee is introduced.
