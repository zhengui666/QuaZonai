# Native Runtime cold recovery

## Scope and prerequisites

This procedure restores a **quiescent Runtime checkpoint on the same host, at the same absolute state path**, with its original Docker identities still available. It uses GNU tar and SQLite's existing recovery, not a new backup service. The [Runtime guide](../runtimes/native/README.md) owns installation, configuration and native task semantics; [OPERATIONS](../OPERATIONS.md) owns the control-plane database, keys and access cutover.

Do not use a Runtime-only checkpoint to roll the whole application back independently. The database, artifact stores, Runtime journal and external work must agree on the recovery point. A task sent after the checkpoint can still exist in Docker even though the older journal does not contain it. That case requires explicit reconciliation, not resubmission or a new task ID. Cross-host relocation, active-job filesystem snapshots and complete production RPO/RTO are outside this cold procedure.

Before archiving, stop new submissions from the control plane, resolve every outstanding dispatch, and wait for the selected Runtime's native jobs to finish or reach confirmed cancellation. A stopped gateway, request timeout or unknown submission is not proof that a job stopped. Verify the original identities through the control-plane records and native Docker inspection. Then stop the Runtime service using its actual supervisor and verify that it has exited. Do not archive a directory while the gateway, an active job or another process is still writing it.

Keep the reviewed executable, exact image identity, Docker state, immutable catalog versions and external configuration available. The credential file is outside `state_dir`; protect and restore it through its existing private configuration procedure, never by pasting it into a command, Issue or log. The commands below run as the owner of the state directory. They do not install software, change service accounts, stop jobs or validate your chosen recovery point.

## What must remain together

| State | Recovery requirement |
|---|---|
| `journal.sqlite` and any `journal.sqlite-wal` / `journal.sqlite-shm` | Archive the complete stopped directory. Do not exclude or manually delete the WAL; acknowledged transactions may still be there. |
| Native input/output bytes and manifests | Preserve the original stored identities and bytes, not merely their metadata or a newly computed replacement. |
| Job directories and stored launch information | Retain the original absolute paths and the checkpoint's filesystem contents. Do not edit old launches to use a different directory. |
| Runtime instance, run/attempt and container identities | Keep them unchanged. Never create a new instance or repeat START to make old work appear recovered. |
| Cancellation tombstones and barriers | Preserve them even when there is no successful job or result. Late requests must remain unable to run. |
| External configuration, credentials, pinned images and catalogs | These are not automatically included in `state_dir`; retain their correct versions and permissions separately. |

SQLite documents the WAL as part of persistent database state: separating it from the database can lose committed transactions. See [SQLite WAL](https://www.sqlite.org/wal.html). The examples use [GNU tar's native comparison](https://www.gnu.org/software/tar/manual/tar.html) before reopening the restored database; SQLite may legitimately checkpoint or change WAL files after startup.

## Create a stopped checkpoint

Select actual, canonical paths for your installation. The example backup parent must already exist, be owned by the operator and remain **outside** the state tree and web root. This is not permission to create, overwrite or delete an unrelated installation.

```sh
set -eu
umask 077
state=/srv/quazonai-runtime/state
backup_parent=/srv/quazonai-runtime/backups
test -d "$state" && test ! -L "$state"
test -d "$backup_parent" && test ! -L "$backup_parent"
checkpoint=$(mktemp -d "$backup_parent/runtime-XXXXXXXX")
tar --create --file "$checkpoint/state.tar" --directory "$state" .
tar --compare --file "$checkpoint/state.tar" --directory "$state"
```

Keep the resulting checkpoint path locally together with its actual source revision, checkpoint time and coordinated control-plane recovery point. Native tar comparison verifies the archive against the stopped source; it does not prove that the application can restart. A failed command leaves the partial archive for inspection and must not be recorded as a successful backup. Preserve the previous usable checkpoint.

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
tar --extract --preserve-permissions --file "$checkpoint/state.tar" --directory "$state"
tar --compare --file "$checkpoint/state.tar" --directory "$state"
```

Replace both `REPLACE_WITH_...` values with inspected paths first. The retained directory must be a distinct unused sibling, not a child of the source or backup directory. A conflicting destination or failed extraction is a stop condition: do not delete either copy to force success. Do not start two Runtime processes from the retained and restored copies. Their instance identity is intentionally identical.

For an actual rollback to an older checkpoint, the retained current directory can differ from the archive. Do not merge their individual SQLite/WAL files or object trees. Leave both copies intact and resolve the selected recovery point as a whole.

## Verify before resuming submissions

Start the reviewed Runtime with its existing configuration and original state path, leaving control-plane submissions stopped. A listening port or a successful `doctor` probe does not establish restoration.

Read representative **original** job identities through the existing Runtime protocol. Confirm their run/attempt IDs, status, timestamps, result manifest and output bytes. Repeat a known original request only with its exact original identity and payload; it must return the same record, not create another container or restart the old one. Verify that a known cancelled identity stays closed. Missing images, catalogs, bytes, journal state or native container identity require investigation; never substitute a fabricated success or delete a tombstone.

After historical checks and control-plane recovery reconciliation, explicitly authorize one bounded new task using the restored input references. Verify actual native execution and output retrieval. Then resume the intended services through their normal startup procedure. Keep the checkpoint and retained copy until the operator's existing retention policy permits removal.

If the restored Runtime cannot start, stop it, retain its diagnostics privately, and preserve both state trees. Selecting the retained original again is a separate deliberate recovery choice, not an automatic repair. Do not regenerate `instance_id`, change old JobSpecs or replay unknown START operations.

## Executable regression and its limits

[The cold-restore target](../apps/runtime/tests/native_restore.rs) reuses the existing native OCI fixture. It compiles real Rust to Wasm, creates a pre-submit cancellation, stops the gateway after native work terminates, archives a nonempty WAL checkpoint, moves the original directory aside and starts a new process from restored files at the original path. It checks original status/manifest/output bytes, immutable replay and conflict behavior, cancellation, unchanged native container identity and a new compile using restored inputs without reupload. It compares the retained original again after recovery.

Use the build environment and pinned image described in the [Runtime guide](../runtimes/native/README.md#验证). Both targets are explicit in the existing Native Runtime CI; absent Docker/image prerequisites fail rather than skip:

```sh
QUAZONAI_NATIVE_JOB_IMAGE='sha256:ACTUAL_NATIVE_IMAGE_ID' \
QUAZONAI_DOCKER_SOCKET=/var/run/docker.sock \
rustup run 1.98.1 cargo test --locked -p runtime --features native-oci \
  --test native_restore -- --test-threads=1 --nocapture
```

The output's `cold_restore_elapsed_ms` measures only this disposable fixture's restore and historical verification, not production recovery time or data-loss tolerance. CI must actually pass on the accepted revision before counting this regression as evidence. The owner's installation, coordinated database/Runtime restoration, active-job fault rehearsal, real accounts and full [T40/T42 acceptance](architecture/issue-62-execution.md#acceptance) still require their own evidence.
