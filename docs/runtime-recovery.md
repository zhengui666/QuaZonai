# Native recovery checkpoints

## Scope and prerequisites

The standalone procedure below restores a **quiescent Runtime checkpoint on the same host, at the same absolute state path**, with its original Docker identities still available. It uses GNU tar and SQLite's existing recovery, not a new backup service. The [Runtime guide](../runtimes/native/README.md) owns installation, configuration and native task semantics; [OPERATIONS](../OPERATIONS.md) owns the control-plane database, keys and access cutover. The [joint-checkpoint regression](#joint-control-and-runtime-checkpoint) separately exercises a common database, control-state, Runtime and catalog checkpoint.

Do not use a Runtime-only checkpoint to roll the whole application back independently. The database, artifact stores, Runtime journal and external work must agree on the recovery point. A task sent after the checkpoint can still exist in Docker even though the older journal does not contain it. That case requires explicit reconciliation, not resubmission or a new task ID. Cross-host relocation, active-job filesystem snapshots and complete production RPO/RTO are outside this cold procedure.

Before archiving, stop new submissions from the control plane, resolve every outstanding dispatch, and wait for the selected Runtime's native jobs to finish or reach confirmed cancellation. A stopped gateway, request timeout or unknown submission is not proof that a job stopped. Verify the original identities through the control-plane records and native Docker inspection. Then stop the Runtime service using its actual supervisor and verify that it has exited. Do not archive a directory while the gateway, an active job or another process is still writing it.

Keep the reviewed executable, exact image identity, Docker state, immutable catalog versions and external configuration available. The credential file is outside `state_dir`; protect and restore it through its existing private configuration procedure, never by pasting it into a command, Issue or log. Run the examples as the state-directory owner, using administrator privileges **only for the selected GNU tar operations**. These privileges are necessary to read and restore mixed-owner native files, including UID/GID65532 job outputs that can remain until terminal cleanup. Do not run the Runtime as root or recursively change ownership to make a comparison pass. The commands do not install software, stop jobs or validate the chosen recovery point.

## What must remain together

| State | Recovery requirement |
|---|---|
| `journal.sqlite` and any `journal.sqlite-wal` / `journal.sqlite-shm` | Archive the complete stopped directory. Do not exclude or manually delete the WAL; acknowledged transactions may still be there. |
| Native input/output bytes and manifests | Preserve the original stored identities and bytes, not merely their metadata or a newly computed replacement. |
| Job directories and stored launch information | Retain the original absolute paths and filesystem contents, including numeric owner/group and permission bits. Do not edit old launches to use a different directory. |
| Runtime instance, run/attempt and container identities | Keep them unchanged. Never create a new instance or repeat START to make old work appear recovered. |
| Cancellation tombstones and barriers | Preserve them even when there is no successful job or result. Late requests must remain unable to run. |
| External configuration, credentials, pinned images and catalogs | These are not automatically included in `state_dir`; retain their correct versions and permissions separately. |

SQLite documents the WAL as part of persistent database state: separating it from the database can lose committed transactions. See [SQLite WAL](https://www.sqlite.org/wal.html). GNU tar's [comparison](https://www.gnu.org/software/tar/manual/html_node/compare.html) checks ownership as well as contents and permissions. A normal user's extraction defaults to that user's ownership; `--preserve-permissions` alone does not restore another UID/GID. The examples therefore use privileged numeric-owner extraction and compare before reopening SQLite, which may legitimately change or checkpoint WAL. See [GNU tar's ownership options](https://www.gnu.org/software/tar/manual/tar.html).

## Create a stopped checkpoint

Select actual, canonical paths for your installation. The example backup parent must already exist, be owned by the operator and remain **outside** the state tree and web root. This is not permission to create, overwrite or delete an unrelated installation. Use only a locally created, operator-selected checkpoint; do not extract an untrusted archive with administrator privileges.

```sh
set -eu
umask 077
state=/srv/quazonai-runtime/state
backup_parent=/srv/quazonai-runtime/backups
test -d "$state" && test ! -L "$state"
test -d "$backup_parent" && test ! -L "$backup_parent"
checkpoint=$(mktemp -d "$backup_parent/runtime-XXXXXXXX")
: > "$checkpoint/state.tar"
sudo tar --create --numeric-owner --file "$checkpoint/state.tar" --directory "$state" .
sudo tar --compare --numeric-owner --file "$checkpoint/state.tar" --directory "$state"
```

The new private checkpoint directory and precreated archive remain owned by the operator (directory0700, archive0600 under this umask); only native tar uses elevated access to the source. Keep the resulting checkpoint path locally together with its actual source revision, checkpoint time and coordinated control-plane recovery point. Native comparison verifies the archive against the stopped source; it does not prove that the application can restart. A failed command leaves the partial archive for inspection and must not be recorded as a successful backup. Preserve the previous usable checkpoint.

## Restore without overwriting the original

Keep admission and both relevant service processes stopped. Select the checkpoint explicitly; the example below deliberately does not guess the newest directory. Confirm adequate space for the retained original and restored copy. The original state must remain recoverable until the restored application has passed verification.

```sh
set -eu
umask 077
state=/srv/quazonai-runtime/state
checkpoint=/srv/quazonai-runtime/backups/REPLACE_WITH_SELECTED_CHECKPOINT
retained=/srv/quazonai-runtime/REPLACE_WITH_UNUSED_RETAINED_DIRECTORY
test -s "$checkpoint/state.tar"
test -d "$state" && test ! -L "$state"
test ! -e "$retained" && test ! -L "$retained"
mv --no-target-directory --no-clobber -- "$state" "$retained"
mkdir -m 0700 -- "$state"
sudo tar --extract --numeric-owner --same-owner --preserve-permissions \
  --file "$checkpoint/state.tar" --directory "$state"
sudo tar --compare --numeric-owner --file "$checkpoint/state.tar" --directory "$state"
```

Replace both `REPLACE_WITH_...` values with inspected paths first. The retained directory must be a distinct unused sibling, not a child of the source or backup directory. A conflicting destination or failed extraction/comparison is a stop condition: do not delete either copy, suppress tar differences or change owners to force success. Do not start two Runtime processes from the retained and restored copies. Their instance identity is intentionally identical.

For an actual rollback to an older checkpoint, the retained current directory can differ from the archive. Do not merge their individual SQLite/WAL files or object trees. Leave both copies intact and resolve the selected recovery point as a whole.

## Verify before resuming submissions

Start the reviewed Runtime **as its original unprivileged service account** with its existing configuration and state path, leaving control-plane submissions stopped. A listening port or a successful `doctor` probe does not establish restoration.

Read representative **original** job identities through the existing Runtime protocol. Confirm their run/attempt IDs, status, timestamps, result manifest and output bytes. Repeat a known original request only with its exact original identity and payload; it must return the same record, not create another container or restart the old one. Verify that a known cancelled identity stays closed. Missing images, catalogs, bytes, journal state or native container identity require investigation; never substitute a fabricated success or delete a tombstone.

After historical checks and control-plane recovery reconciliation, explicitly authorize one bounded new task using the restored input references. New admission still requires a current native capability probe; restoration does not extend an old probe's validity. Verify actual execution and output retrieval, then resume the intended services through their normal startup procedure. Keep the checkpoint and retained copy until the operator's existing retention policy permits removal.

If the restored Runtime cannot start, stop it, retain its diagnostics privately, and preserve both state trees. Selecting the retained original again is a separate deliberate recovery choice, not an automatic repair. Do not regenerate `instance_id`, change old JobSpecs or replay unknown START operations.

## Executable Runtime regression

[The cold-restore target](../apps/runtime/tests/native_restore.rs) reuses the existing native OCI fixture. It compiles real Rust to Wasm, creates a pre-submit cancellation, stops the gateway after native work terminates, archives a nonempty WAL checkpoint, moves the original directory aside and starts a new process from restored files at the original path. It checks original status/manifest/output bytes, immutable replay and conflict behavior, cancellation, unchanged native container identity and a new compile using restored inputs without reupload. It compares the retained original again after recovery.

The same target contains a separate **metadata-only control**: a test-owned file is assigned65532:65532, the former unprivileged extraction must lose that owner and fail native comparison, and corrected privileged extraction must preserve numeric owner/group, mode and bytes. This is deterministic filesystem coverage, not a synthetic model or extra research result. The shared [archive helper](../apps/runtime/tests/support/archive.rs) retains the same numeric-owner procedure and bounded sanitized diagnostics; unexpected exits and differences fail.

Use the build environment and pinned image described in the [Runtime guide](../runtimes/native/README.md#验证). Run tests as an ordinary user with Docker access, GNU tar/chown and noninteractive `sudo` for their own disposable archive files. Only archive/ownership commands are elevated, never Runtime or Cargo. The existing Native Runtime CI selects these tests explicitly; missing prerequisites fail:

```sh
QUAZONAI_NATIVE_JOB_IMAGE='sha256:ACTUAL_NATIVE_IMAGE_ID' \
QUAZONAI_DOCKER_SOCKET=/var/run/docker.sock \
rustup run 1.98.1 cargo test --locked -p runtime --features native-oci \
  --test native_restore -- --test-threads=1 --nocapture
```

## Joint control and Runtime checkpoint

[The joint target](../apps/runtime/tests/native_control_restore.rs) covers an additional boundary that separate database and Runtime tests cannot establish. A genuine native `DATA_VALIDATE` has finished remotely, while the control-plane Attempt is still `SENT_UNKNOWN`, its original message is unacknowledged and no result has been published. The test confirms the native container is terminal and stops the sole Runtime writer; there is no running control Worker. Only this known quiescent state forms the common checkpoint. It is not permission to snapshot an unknown or running remote task, nor to edit a control row to manufacture that state.

The checkpoint consists of the native PostgreSQL dump, control artifact/secret directory, complete Runtime directory/configuration and actual Parquet catalog, with the original master key retained separately. The test restores a new database and same-path, new-inode copies, preserving originals and numeric ownership. Actual `recover-access` invalidates the old browser authority. The test's trusted Store verified-step input is not a real TOTP or model-account login.

Recovery then uses the **built production Worker CLI**, not a successful-response mock. Withholding an original local parameter file must prevent publication, terminal receipt and ACK even though remote output exists. Returning the same file and allowing the real lease to expire lets a new Worker reconcile the same Attempt/external ID/spec with an advanced owner epoch. It must publish the original raw manifest/output bytes once and archive the original message, without restarting the original container. A separately authorized new task refreshes native capability observations and reads the restored catalog without registering replacement data. Fixture origin/PIT status remain unchanged; no scientific qualification is granted.

In addition to the native image and archive prerequisites above, build `server`, supply a **disposable PostgreSQL18/PGMQ administrator URL**, and provide matching `pg_dump`/`pg_restore` clients. The test creates its own SQLx database, fresh restoration database and application role. Do not supply an installation's database. When using the existing CI PostgreSQL container, `QZ_TEST_PG_CONTAINER` selects its native clients; it must identify that same disposable server. Credentials and archives stay in the private fixture and are not uploaded.

```sh
rustup run 1.98.1 cargo build --locked -p server --bin server
# Set DATABASE_URL locally to the disposable test administrator; never paste a real secret.
QUAZONAI_NATIVE_SERVER_BIN="$(pwd)/target/debug/server" \
QUAZONAI_NATIVE_JOB_IMAGE='sha256:ACTUAL_NATIVE_IMAGE_ID' \
QUAZONAI_DOCKER_SOCKET=/var/run/docker.sock \
rustup run 1.98.1 cargo test --locked -p runtime --features native-oci \
  --test native_control_restore -- --test-threads=1 --nocapture
```

The [existing Native Runtime workflow](../.github/workflows/native-runtime.yml) builds the real server and image, runs the original OCI/cold/ownership suites, then runs this joint case. [PR #89](https://github.com/zhengui666/QuaZonai/pull/89) owns its actual candidate results and review; a listed command or compiled test is not a passing restore. Check the accepted commit and complete result before treating this scenario as evidence.

`cold_restore_elapsed_ms` and `adoption_elapsed_ms` measure only disposable fixture phases, not production recovery time or data-loss tolerance. These scenarios do not prove active-job power-loss snapshots, cross-host relocation, arbitrary earlier rollback, real account/market licensing, the owner's recovery objective or complete [T40/T42 acceptance](architecture/issue-62-execution.md#acceptance). Keep those requirements separate.
