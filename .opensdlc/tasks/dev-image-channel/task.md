# Publish development images before merge

<a id="intent"></a>
## Intent

The owner requested a branch from the latest main, a dev image channel for
unmerged development branches, and a reviewed PR merged into main. Base:
`8d1f09b031d83fad72a367da653504b37335593a`; branch: `codex/dev-image-channel`.
The existing primary worktree and installed services remain outside this task.

<a id="spec"></a>
## Requirements and design

A manually dispatched workflow accepts a same-repository branch, tag or full SHA.
Checkout resolves it once; the existing container action builds and tests that
exact source. The tested image is published as
`ghcr.io/zhengui666/quazonai:dev-<full-sha>-<run-id>-<attempt>` and read back by
digest. There is no floating alias, Git tag, GitHub Release, deployment bundle
or production deployment. Sources must contain the existing container action
and deployment files. The versioned release gate is unchanged.

Contracts: [DESIGN](../../../DESIGN.md#container-release). Usage:
[OPERATIONS](../../../OPERATIONS.md#dev-image). Dispatch follows
[GitHub's workflow documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow).

<a id="plan"></a>
## Implementation plan

1. Add `.github/workflows/dev-image.yml`, reusing pinned actions and the container
   composite action. Add its path to Container PR selection.
2. Document dispatch and dev artifacts in DESIGN, OPERATIONS and README. Keep
   `release.py` and versioned deployment manifests unchanged.
3. Obtain current-head CI and explicit clean Codex review, then merge. Once the
   workflow exists on main, dispatch an unmerged source commit and verify the
   published tag, revision and digest. Record native evidence in the PR.

<a id="verification"></a>
## Verification

Local YAML parsing, Bash syntax checks for every workflow script, actual metadata
generation from the checkout with distinct retry numbers, and `git diff --check`
passed. These checks do not claim a container build or registry publication.
Current-head CI and registry publication are pending. Native builds and real
container lifecycle checks run in GitHub Actions; no local database or user service
is used for acceptance. The PR owns review, CI, merge and publication evidence.
