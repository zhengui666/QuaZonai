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

<a id="scientific-runtime"></a>
## Scientific Runtime

The application bundle does not provision the scientific gateway, job image or catalogs. Scientific jobs run in the native Docker image; the gateway is a host process that owns its journal and Docker connection. Use the matching [image build](project.md#runtime-image-build) and gateway binary from the application's source revision.

Use a Linux x86_64 Runtime owner with local Docker Engine/cgroup v2 and the gateway's native ABI libraries. Choose absolute, owner-managed paths and write a private `runtime.json` matching [RuntimeConfig](../apps/runtime/src/config.rs). Replace every example path and image placeholder before use:

```json
{
  "schema_version": 1,
  "state_dir": "/srv/quazonai-runtime/state",
  "credential_file": "/srv/quazonai-runtime/runtime-credential",
  "docker_socket": "/var/run/docker.sock",
  "bind": "127.0.0.1:8790",
  "images": [
    {"job_kind": "DATA_VALIDATE", "image_ref": "REPLACE_WITH_NATIVE_IMAGE_ID"},
    {"job_kind": "ALPHA_EVALUATE", "image_ref": "REPLACE_WITH_NATIVE_IMAGE_ID"},
    {"job_kind": "PORTFOLIO_BUILD", "image_ref": "REPLACE_WITH_NATIVE_IMAGE_ID"},
    {"job_kind": "PORTFOLIO_SIMULATE", "image_ref": "REPLACE_WITH_NATIVE_IMAGE_ID"}
  ],
  "catalogs": [],
  "max_cpu": 2,
  "max_memory_mib": 4096,
  "max_wall_seconds": 3600,
  "max_output_bytes": 67108864,
  "max_parallel_jobs": 2,
  "max_pending_jobs": 64,
  "storage_quota_bytes": 10737418240
}
```

The state parent belongs to the Runtime owner; the state directory is mode 0700. Provision a mode-0600 regular credential file for that owner containing 32–8192 printable, non-whitespace ASCII bytes, optionally ending with one newline. Bind the same credential through the control plane's Runtime settings; do not put it in the JSON, URL, command arguments, repository or logs. Keep existing credentials and state when restarting or upgrading.

Each catalog registration is `{ "root": "/absolute/immutable/catalog", "metadata_file": "/absolute/catalog-metadata.json" }`. The metadata must satisfy [RuntimeCatalogMetadataV1](../crates/contracts/src/catalogs.rs), including the original snapshot/version, partition, instruments, historical membership, quality and availability provenance. Roots must not overlap each other or the state, credential or Docker socket. Empty catalogs permit configuration inspection, not a research dataset; missing data remains missing.

From the matching build directory, check and start the gateway using the edited configuration:

```sh
target/release/runtime doctor --config /absolute/runtime.json
target/release/runtime serve --config /absolute/runtime.json
```

`runtime doctor` checks the real Docker/image/resource prerequisites. `runtime serve` uses the original state directory; its existing-task status remains available during a Docker outage, but new execution cannot succeed without Docker. Use the same owner, binary/configuration and paths when supervising or restarting it. Neither command replaces execution/cancellation/restore tests.

The gateway accepts only a loopback listener. Expose it through an existing same-host trusted HTTPS reverse proxy whose origin is reachable from both the Docker API container and host Worker. Their `runtime_targets` entries contain the exact `origin` and allowed `addresses` (`IP:port`), as defined in [RuntimeTarget](../apps/server/src/runtime_transport.rs); register that endpoint and matching credential with the service, then probe readiness before importing/using catalogs. Inside the API container, `127.0.0.1` is not the host, and the gateway has no Unix-socket HTTP listener. Do not substitute a fabricated socket or disable certificate validation. After configuration changes, probe the new capabilities; existing jobs retain their original launch and remote identity.

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
