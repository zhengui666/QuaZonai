# Publish development images before merge

<a id="intent"></a>
## Intent

The owner requested a branch from the latest main, a dev image channel for
unmerged development branches, and a reviewed PR merged into main. Base:
`8d1f09b031d83fad72a367da653504b37335593a`; branch: `codex/dev-image-channel`.
The existing primary worktree and installed services remain outside this task.

<a id="spec"></a>
## Requirements and design

A manually dispatched workflow accepts only a same-repository branch name under
`refs/heads/`; tags, arbitrary SHAs and synthetic PR refs are not inputs. Checkout
resolves it once. A read-only build uses the workflow's trusted container action
against a separate source checkout. A separate default-branch `workflow_run` publisher accepts only successful manual
builds dispatched from main, then accepts the exact run/attempt artifact,
checks the image ID, OCI labels and run-specific dev tag, and pushes the tested
image as `ghcr.io/zhengui666/quazonai:dev-<full-sha>-<run-id>-<build-attempt>`.
Rebuilding creates a new tag; retrying only the publisher reuses the original
artifact and tag. Sources need existing container build/deployment files, but not
the dev workflow/action. The publisher never executes source code or the image.
The versioned release gate is unchanged; no deployment bundle is produced.

Contracts: [DESIGN](../../../DESIGN.md#container-release). Usage:
[OPERATIONS](../../../OPERATIONS.md#dev-image). Dispatch follows
[GitHub's workflow documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow).

<a id="plan"></a>
## Implementation plan

1. Add `.github/workflows/dev-image.yml` and `dev-image-publish.yml`, reusing pinned
   actions and the container composite action. Add their paths to Container PR selection.
2. Run the new read-only build on workflow/action PR changes, exercising a separate
   checkout and OCI identity checks. Document dispatch and dev artifacts in DESIGN, OPERATIONS and README. Keep
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

<a id="review"></a>
## Review remediation

PR #118 review on `101960c` identified package-write exposure to selected code and
synthetic fork PR refs. The updated workflow isolates build from publisher, loads
the action from the workflow checkout, constrains publisher tags, and narrows input
to repository branches. PR runs exercise build/export with no publishing permission.
New-head CI and clean re-review are required; earlier checks are not reused.

The additional review on `d1f2ae36` found that dispatch could select an untrusted
workflow revision, and a job retry could rerun its build dependency. Publishing now
lives in a separate default-branch `workflow_run` definition restricted to successful
manual main runs of the exact build workflow. Its own retry has no build dependency;
it reuses the original event's artifact and derives tags from that build ID/attempt.
See [GitHub's native event semantics](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#workflow_run).
