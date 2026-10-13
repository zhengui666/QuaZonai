# Linux-only release and test support

<a id="task"></a>
## Task

**Task ID:** linux-only-release

**Source:** Owner-approved goal on 2026-10-13: use current remote `dev` and support only Linux x86_64 for releases, installation and tests, including the standalone CLI.

**Endpoint:** Merge the verified change into remote `dev`; no manual formal release or existing-service operation.

<a id="intent"></a>
## Intent

Stop maintaining Windows/macOS distribution, installer and test paths. Preserve the existing Linux stack, CLI-only installation, original client configuration and private permissions. Browser clients remain platform-independent. Preserve unrelated worktrees, user changes, data, migrations, backups, upstream dependencies and license files.

<a id="spec"></a>
## Requirements and design

The release chain requires one Linux CLI archive, `install.sh`, the existing deployment bundle/manifest/README and three image archives. Remove the non-Linux CLI runners and PowerShell entrypoints, use tar and Linux XDG/HOME permissions directly, and document Linux-only support in current product/developer/Agent references. Do not scrub upstream lockfiles or historical task records.

Acceptance: `make check`, `make check-links`, a real official Node 22.19.0 Linux CLI package with extracted smoke checks and non-skipped installer checks, clean independent review and all applicable final-Head CI before merge.

<a id="plan"></a>
## Implementation plan

Baseline: `origin/dev` at `bcbef01a2d2ef2129d96a70d3389f239fd4575c2`, isolated `chore/linux-only` worktree. The retained service/science CI already uses Linux.

1. Simplify `.github/workflows/cli.yml`, exact CLI artifact consumption in `release-version.yml`, `deploy/package.py`, `deploy/install.sh` and `apps/cli/src/profile.ts`; remove the two PowerShell files.
2. Keep package/installer/profile regression checks at those boundaries and update current documentation.
3. Run named Linux checks and genuine bundled-Node CLI tests; review the exact final Head and merge through a PR after applicable CI.

<a id="verification"></a>
## Verification

Checked on Linux x86_64 with official Node 22.19.0 and lychee 0.24.2. The official runtime was reused from a cached upstream distribution after a network download failed. Temporary runtime distributions, packages and logs stay in the session scratch directory, not an existing installation.

| Check | Observed result |
| --- | --- |
| `make check` with the real CLI archive/version enabled | Passed: 5 service, 18 research, 4 CLI, 1 Web, 12 deployment/native-file, 3 package and 2 release tests; no skipped tests; TypeScript checks and Web production build passed |
| `deploy/package.py pack-cli` and `smoke-cli`, official Linux Node 22.19.0 | Passed: extracted version/help and current/native schema discovery |
| `QZ_TEST_CLI_ARCHIVE=... QZ_TEST_CLI_VERSION=... node --test deploy/install.test.mjs` | Both tests passed, no skips: repeat CLI-only installation without host Node/npm, original XDG configuration and 0700/0600 permissions, version/help/schema and Node license retained |
| Linux asset/catalog contract | Passed: only eight Linux/deployment assets required; an incomplete newer release is not selected |
| Changed current Markdown and this task through lychee | Passed, zero link errors |
| `make check-links` | Failed with 60 broken file/anchor links; the unmodified `bcbef01a` baseline fails with the same 60 errors |
| Owned release/install/CLI platform-reference scan and `git diff --check` | Passed: no remaining non-Linux executable branches or deleted-file callers |

**Blocker:** Full-repository links point to Rust entrypoints, old Web tests and deployment anchors removed by the preceding TypeScript refactor. Fixing those unrelated historical/current documents is outside the approved platform-only scope. No checker exclusions, placeholder files or rewritten historical records were added to turn the failure into a pass. The mandatory full link-check acceptance remains unmet.

<a id="review"></a>
## Review

Independent read-only review and final-Head hosted CI pending; use the current [review policy](../../review.md). A clean review or green hosted jobs will not replace the failed mandatory `make check-links` result.

<a id="delivery"></a>
## Delivery

PR pending. Keep the change unmerged while full-repository link acceptance is blocked. The root checkout and other ongoing worktrees remain untouched; no formal release or existing-service operation is performed.
