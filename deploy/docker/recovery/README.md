# Internal scientific checkpoint deployment

This is the deployment adapter for the official runC **1.5.2** and CRIU **4.3**
interfaces. The adapter alone does **not** establish that a host or a scientific
command supports internal checkpoint/restore. A missing, incompatible or altered
verified profile keeps V2 scientific admission unavailable. Existing V1 receipts
remain readable through their original protocol.

## Host and process boundary

Install the pinned official tools on the separately administered Linux science
host. CRIU needs the host kernel, namespace and ptrace capabilities exercised by
the native acceptance suite. The ordinary HTTP worker remains unprivileged; do
not give it a Docker socket or unrestricted privileged container access.

Provision a root-owned private broker configuration from
[`broker.example.json`](broker.example.json). Replace both digests with actual
accepted identities, and use the original immutable scientific image rootfs,
matching Node binary, libraries and native executables. Catalog roots use the
same absolute paths as the frozen application requests. All configured inputs
must be immutable for the lifetime of their pending invocations.

Run the broker from the installed matching source tree:

```sh
node --experimental-strip-types apps/nautilus/src/broker.ts /etc/quazonai-recovery/broker.json
```

The broker exposes a Unix socket only. Give the worker's existing service group
access to its parent directory and socket; the example's decimal mode `432` is
octal `0660`. Keep the configuration, installed binaries, profile and broker
state inaccessible to HTTP clients. Run one broker owner per data directory.
The process lifetime lock rejects a second broker; invocation operation locks
also cover runC helpers after an HTTP worker disconnects.

Set the HTTP worker's `QZ_NAUTILUS_RECOVERY_SOCKET` to that socket. The broker
accepts only fixed profile/prepare/inspect/checkpoint/restore/start/cancel
operations. Host binary paths, rootfs, mounts and privileges come from the local
configuration, never a remote operation.

## Acceptance profile

The production profile is a JSON `VerifiedCheckpointProfile`:

- `schemaVersion: 1`, `profileId`, `verifiedCommands`, `checkpointIntervalMs`;
- `runtime`: exact runC `1.5.2`, CRIU `4.3`, kernel release and CPU digest;
- `rootfsDigest`: the actual `checkpointTreeDigest` of the immutable rootfs;
- `frozenInputs`: each configured catalog root and its actual content digest;
- `evidence`: each verified command and the digest of its successful native
  restore-oracle report.

`profileId` is the canonical JSON SHA-256 of that complete body excluding
`profileId`. The driver checks runtime compatibility and real filesystem bytes;
a manually supplied digest label does not pass validation. Missing acceptance
evidence never becomes a default capability. The explicit acceptance factory
can exercise operations before qualification, but its `profile()` always returns
null and it is never used by the broker.

The required native acceptance must prove intermediate Nautilus account/order
and event continuity, nonzero Clarabel iteration continuation, Wasmi private
memory/global state, Tick reader/label state, and multithreaded tar/zstd process
trees. Final output equality or a `sleep` container restore is insufficient.
Run the required native acceptance executable against the original scientific
fixtures and preserve its evidence; a preflight-only report is not qualification.

## Durable checkpoint sequence

Each invocation gets an isolated OCI container with no network or TTY, immutable
read-only rootfs/catalog mounts, and one writable `/work`. The init shell opens
its own regular-file stdio inside that namespace and then execs the completion
wrapper. The scientific child also uses ordinary input/output files; internal
child pipes remain within the captured tree.

The first wrapper waits at a gate. After its baseline image is durably saved and
the official Task commits generation zero, restore that image and release the
gate. Later checkpoints use runC's default stop-and-dump behavior, followed by
`cp -a --reflink=auto` of the stable `/work`, filesystem sync, immutable manifest
publication and the official Task's generation commit. Only then restore and
continue. No `--leave-running`, incremental chain or lazy restore is used.

The broker deliberately never writes a competing "current task" database or
schedules retries. The official Worker Task owns the generation pointer, timer,
cancel intent and recovery decision. A candidate missing successful dump
evidence is discarded in favor of the previous committed generation. The initial
waiting wrapper may be recreated only before the computation gate ever opens.

Output, progress and completion files are restored as one generation, so future
uncommitted tails cannot be appended to restored old memory. A completion record
binds original request/input/implementation, exit status and output digests.
The HTTP worker re-collects that original record after a restart; it never infers
success from a stopped container. OOM/signal exits restore the previous checkpoint
and retain the original invocation instead of declaring a scientific failure.

## Maintenance and recovery

Keep the application Durable store, the Worker Durable store, this broker's
execution trees, complete checkpoint generations, original rootfs and frozen
catalog inputs. Retain current and previous complete generations until their
official task and result retention permit collection. The adapter does not
automatically delete these recovery points.

After reboot the runC runtime root is boot-specific, so persisted stale PIDs
cannot signal a process from the new boot. Same-boot attachment checks private
container/bundle identity and PID start time, including the launch/restore intent
window. Unknown ownership blocks restoration.

Do not change kernels, CPU compatibility, runC/CRIU or scientific images while
pending snapshots depend on an unverified replacement. Restore using the original
validated configuration. A failure to capture, copy, sync, validate or restore
keeps the original invocation and recovery evidence; it does not permit whole
scientific-operation replay.

For power-loss acceptance use an environment that can actually lose unsynced
filesystem caches. SIGKILL tests exercise process recovery, not the power-loss
durability promise.
