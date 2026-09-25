# Engineering operations

<a id="controls"></a>
## Action boundaries

[AGENTS](../AGENTS.md) defines authorship and executor roles. Tests use disposable services, accounts with no real credentials, and task-owned directories. Do not attach production data or change an existing installation during normal CI. The owner authorizes releases, deployment, account login and data recovery separately from a code merge.

<a id="delivery"></a>
## Delivery

Create a branch from current main, run the relevant [checks](project.md#commands), open a PR and satisfy [review](review.md). Use the PR's exact final Head for checks/review; the native Issue/PR/Actions record owns the result. A source revert does not downgrade a database.

<a id="container-release"></a>
### Versioned images

[release.yml](../.github/workflows/release.yml) observes main/tag pushes and check completion. A release tag is `vMAJOR.MINOR.PATCH[-prerelease]`, without leading zeros, build metadata or a floating alias. Its dereferenced commit must be an ancestor of current main; squash/rebase does not move an old tag.

Release requires successful current-source CI, Web console, Native Runtime, Polymarket history and Container workflows. The release job builds and validates the image, pushes that same image to GHCR, and publishes the manifest/deployment bundle before completing the draft Release. It does not rebuild a different image after testing, overwrite a complete version or publish `latest`. A main push without a version tag is not a release.

Push the chosen immutable tag only within release authorization. Reconcile an existing tag/draft before retrying. Verify the published revision, image digest, package visibility and `quazonai-deploy.tar.gz`; repository visibility alone does not set GHCR visibility. Deployment uses the [versioned bundle](../deploy/docker/README.md), not a developer checkout.

<a id="dev-image"></a>
### Development images

Run **Dev image** on workflow ref `main`, setting `source_branch` to an existing branch in this repository. The source branch needs container build/deployment files but need not contain the workflow. It is resolved to a fixed SHA; tags, arbitrary SHAs and fork PR refs are not inputs.

```sh
gh workflow run dev-image.yml --repo zhengui666/QuaZonai --ref main \
  -f source_branch=your-development-branch
```

The read-only build uses the workflow's [container action](../.github/actions/container/action.yml) and actual install/update/recovery checks. [Dev image publish](../.github/workflows/dev-image-publish.yml) is loaded from the default branch, accepts the successful manual build from main, validates the archive/image identity, and publishes `ghcr.io/zhengui666/quazonai:dev-<full-sha>-<run-id>-<build-attempt>`. The publisher never executes development source.

Use the digest in the publisher's successful Summary. A publisher retry reuses the same build artifact/tag; rerunning the build resolves the branch again. Build artifacts expire after seven days. Both workflows must succeed. This channel creates no Release or deployment bundle and never switches an installation; dev tags are not inputs to `update.sh`.

<a id="recovery"></a>
## Recovery

Application update/retry/backup procedures are maintained in the [deployment bundle](../deploy/docker/README.md#recovery). Phase and identity invariants are in [architecture](architecture.md#container-release). Application updates do not upgrade PostgreSQL or downgrade schemas.

For a cold restore, stop admissions and the original Worker, reconcile actual remote tasks, then stop every writer. Preserve the matching database, application artifacts, `master.key`, private installation manifest, original image/configuration, Codex home and independent Runtime journals/catalogs. Restore original absolute paths and ownership. Never replace an original backup with a partly migrated database or start an old executable against a newer schema.

After restoring the control database and ending old transactions, the migration owner runs the matching server's `recover-access --recovery-id UUIDv7`. Save one recovery ID for the operation; unknown outcomes replay that ID, while a different restore gets a new one. This cuts over access, not data or external execution. Preserve original ciphertext/key material, reissue required machine connections, and reconcile old remote identities before resuming work. Native checks: [recovery_access](../apps/server/tests/recovery_access.rs) and [native_control_restore](../apps/runtime/tests/native_control_restore.rs).

Runtime cold backups include the complete stopped state directory, SQLite/WAL, manifests, objects, cancellation records and instance identity. Do not copy a live directory as a consistent backup. [native_restore](../apps/runtime/tests/native_restore.rs) checks archive/ownership behavior with disposable files. A Runtime-only restore is not a coordinated control-plane restore.

<a id="observe"></a>
## Observe and diagnose

Use the installed manager's `status`, Run/Attempt snapshots, safe event streams and the exact failing CI job. Keep resource IDs, original requests/keys and returned receipts; exclude credentials and private model/data content.

| Observation | Action |
| --- | --- |
| HTTP 202, submission timeout or lost acknowledgement | Read the original Run/receipt; replay only the original authorized request when required |
| Cancellation pending or remote identity unknown | Reconcile the original remote task; do not infer termination from local exit or issue another Run |
| `preparing` update failure | Retry the same target; preserve identity and original processors if Runs were admitted |
| `migrating` / `starting` marker | Keep the recovery point and candidate; follow the same-target recovery path, not an old executable |
| Missing model/data/Runtime capability | Repair the actual prerequisite; no fabricated output or fallback source |
| Failed check | Inspect that Head's job/step, fix the owning source, rerun affected checks and review the new Head |

No unattended production monitor is installed by these documents. Actual automation is defined in [workflows](../.github/workflows); the service owner triages operational faults and opens a task for a reproducible fix.

<a id="metrics"></a>
## Measurements

Report observed duration, workload, profile, source revision and outcome. Keep execution success, evidence validity, review, merge and deployment separate. A microbenchmark does not establish a whole-system speedup; a disposable smoke test does not exercise a real account or market-data entitlement.
