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

Local integrated checks passed: 89 Docker release/upgrade tests, 9 complete asset
packaging tests, and 11 installer/documentation checks. The CLI author verified
Linux native tests, Clippy, architecture and existing server CLI transport/help/
Skill checks. Windows/macOS execution and complete container release acceptance
remain pending native GitHub CI.

All workflow definitions pass actionlint 1.7.12. Dev publication never writes a
shared Codex version/latest tag: it reuses an existing verified published version,
or publishes its tested image under the unique application release tag. This
avoids contention with default-branch publishers without changing live main.

<a id="review"></a>
## Review

Pending PR and final-Head CI/Codex review.

<a id="delivery"></a>
## Delivery

Pending implementation, merge and automatic release verification.
