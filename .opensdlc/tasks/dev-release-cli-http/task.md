# Prebuilt dev releases and portable HTTP CLI

<a id="task"></a>
## Task

**Task ID:** dev-release-cli-http

The owner requested HTTP CLI access, automatic timestamped releases after remote
`dev` changes, complete prebuilt Docker/CLI delivery, and tag-specific one-line
installation/update instructions. The delivery endpoint is a reviewed, green PR
merged into `dev`, followed by verification of its automatic published release.

<a id="intent"></a>
## Intent

Installation must consume existing binaries/images with no local compilation or
image build. Preserve installation identity, credentials, data, migration and
recovery behavior. The existing Linux x86_64 Docker/systemd deployment remains
the cluster host; native remote CLI clients support Linux x86_64, macOS Intel and
Apple Silicon, and Windows x86_64.

<a id="spec"></a>
## Requirements and design

- Explicit HTTP URLs work for CLI login, saved sessions and commands, without
  changing unrelated machine/Runtime HTTPS transport rules or following redirects.
  Remote HTTP uses an explicitly configured `CLI_HTTP_ORIGIN`, native marker and
  owner-device credential; machine credentials and browser/bootstrap routes retain
  the original public-origin boundary.
- Every `dev` push receives an immutable `v<core>-dev.<UTC timestamp>.<run ID>` tag
  identifying that push's exact source. Retries reuse the tag. Publication waits
  for all applicable CI for that source and invokes the existing reusable release
  workflow directly, so GitHub token-created tags need no recursive push trigger.
- Publish all required image digests and all four native CLI archives, checksums,
  installers and generated README instructions with the exact release tag.
  Missing assets or a failed platform prevent a complete Release.
- Linux one-line installation/update handles the Docker deployment and CLI;
  macOS/Windows install the CLI for a remote service. Existing production state
  on the development host is not part of acceptance testing.

<a id="plan"></a>
## Implementation plan

1. Reuse the existing client command implementation in a portable Rust binary;
   verify HTTP, profile permissions, offline help/schema and native platforms.
2. Extend `deploy/docker/release.py` and the release workflows for exact-source
   dev tags, native assets and complete publication. Keep the manual arbitrary
   branch image channel and its isolated publisher unchanged.
3. Add prebuilt-only installers and generate tag-bound release documentation;
   retain the existing deployment manager and recovery checks.
4. Run focused Rust, Python and installer checks, native CI and GitHub Codex
   review on the final PR Head, merge into dev, then inspect the resulting tag,
   package digests, all assets and clean-runner installation result.

The portable CLI and installer work use separate worktrees under the OpenSDLC
parallel-work rule. The parent integrates them and owns release workflow edits.
The root README links the generated Release README; tag-specific commands are
rendered in release assets, the deployment bundle and Release notes without
creating a recursive documentation commit on dev.

<a id="verification"></a>
## Verification

Local integrated checks passed: 91 Docker release/upgrade tests, 10 complete asset
packaging tests, and 12 installer/documentation checks. The CLI author verified
Linux native tests, Clippy, architecture and existing server CLI transport/help/
Skill checks. Native Windows/macOS/Linux CI compiled, tested, packaged and ran
the CLI. Native installer failures exposed Bash 3.2 empty-array handling and
PowerShell null-string conversion; both are corrected and await final-Head CI.
System Bash, Bash 3.2 and a symlinked temporary directory pass all 12 Unix checks.

Real non-loopback HTTP through the shipped Caddy routing configuration passes
interactive password login, saved identity, project writes/reads and reused login.
All 8 auth HTTP and 5 client login tests pass, including negative browser/cookie/
origin checks. Native CLI and affected server targets pass Clippy with warnings
denied. Compose retains the HTTP override across old/new bundle directories.
The follow-up review narrowed remote HTTP to owner-device credentials. The server
rejects a genuinely issued machine token even with the CLI marker; the shared CLI
connection rejects that combination before network access. All 10 portable CLI
checks and 13 authentication/login checks pass after this correction.

A real Linux CLI archive was installed and updated in a disposable home, with
exact binary/checksum/version and offline contract verification. Four disposable
Docker images exercised export, gzip archive load, identity readback and package
validation without touching the running installation. Full release-image cold
installation remains a post-merge release gate.

All workflow definitions pass actionlint 1.7.12. Dev publication never writes a
shared Codex version/latest tag: it reuses an existing verified published version,
or publishes its tested image under the unique application release tag. This
avoids contention with default-branch publishers without changing live main.

<a id="review"></a>
## Review

[PR #132](https://github.com/zhengui666/QuaZonai/pull/132) targets dev.
Independent local review identified and corrected missing portable-CI triggers,
shared Codex tag races, incomplete Release detection and main/dev duplicate
publication. GitHub Codex also identified server rejection of remote HTTP login
and out-of-order dev release selection; both are corrected with regression checks.
Final Head `fef1a8e1d47b8178dc7c68c849d5209448a65d5e` passed all 12 checks
and received an explicit clean Codex review. All review threads were resolved.

<a id="delivery"></a>
## Delivery

PR #132 was merged into dev as `d6cc9bbbe442a53e636ab682d6e5017a1ec77edb`.
Its automatic workflow created `v2.0.0-dev.20260928083602.36398316655` and
all six source workflows passed. Web browser acceptance passed an unchanged-SHA
rerun after a UI timeout; both pre-restart and post-restart receipts passed.

Actual publication exposed a test fixture inheriting `RELEASE_BRANCH=dev` while
its temporary Git repository intentionally contains only `origin/main`. The
fixture now explicitly selects main, and the shared container check runs all
release tests in dev context as well. This reproduces the publisher environment
in ordinary CI without changing production ancestry validation. PR #133 passed
all 13 checks and an explicit clean Codex review, then merged as
`cc371918cbbbc46320b47a676ce7a89e05c7343d`. Its automatic tag is
`v2.0.0-dev.20260928100635.36407687164`.

Inspection of the real uploaded Windows binary then found an external
`VCRUNTIME140.dll` dependency. Native runner execution alone could miss this on
machines with Visual Studio installed. Publication was cancelled before any
Release was exposed. The Windows build now statically links the C runtime and
native Windows acceptance checks the actual PE imports for VC runtime DLLs.
This follow-up must pass review and CI; complete release publication remains
the final delivery gate.
