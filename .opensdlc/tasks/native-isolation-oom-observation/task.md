# Native isolation OOM observation

## Scope and evidence

A bounded test-harness correction in the authorized
[dev iteration](../dev-iteration-foundation/task.md), based on `4e4a9fb`.
Publication and exact-head independent review/CI remain separate gates.
No production Runtime code, resource limit, privilege or failure classification changes.

[Native Runtime run 36764452268](https://github.com/zhengui666/QuaZonai/actions/runs/36764452268/job/110054963665)
at source `6024c55` reported memory probe exit 137 with `OOMKilled=false` at
19:25:41 UTC on 2026-09-30; polling expired 15 seconds later. The other 17 OCI
tests passed, including the real compiler's `OOMKilled=true` observation and
durable `MEMORY_LIMIT` receipt. Neither exit 137 nor that separate compiler test
proves why this particular probe died.

The pressure binary has been unchanged since
[21305b3](https://github.com/zhengui666/QuaZonai/commit/21305b365e2599510aaaf2877a4e1d525b7cce31).
[b2c2306](https://github.com/zhengui666/QuaZonai/commit/b2c230605e1a4717488e394ca7be1106841bbd60)
added the existing post-exit OOM wait. These paths are unchanged between the
failed source and this task's base. Rust 1.98.1 and Bollard 0.21.1 stay pinned;
Docker comes from the Ubuntu 24.04 hosted runner, with its actual version and
cgroup version already captured by the workflow. No particular daemon version
is inferred from its API version.

## Diagnosis and correction

An asynchronous OOM notification race is a supported hypothesis, not a locally
reproduced root cause. In the upstream
[containerd 1.7.28 start path](https://github.com/containerd/containerd/blob/v1.7.28/runtime/v2/runc/task/service.go#L270-L324),
the process starts before cgroup OOM monitoring is added. Its
[OOM watcher](https://github.com/containerd/containerd/blob/v1.7.28/pkg/oom/v2/v2.go)
consumes an asynchronous cgroup event channel; the
[cgroup reader](https://github.com/containerd/cgroups/blob/v3.0.5/cgroup2/manager.go#L694-L739)
can end when the cgroup disappears. Docker updates `OOMKilled` on the distinct
[OOM event](https://github.com/moby/moby/blob/v28.0.4/daemon/monitor.go), rather
than deriving it from exit 137. Waiting after container exit cannot guarantee
recovery of a lost notification.

The CI-only probe now keeps a low-memory supervisor in the same cgroup. A file
written through the existing read-only input bind releases pressure only after
Docker start returns. A single child performs the original bounded allocation.
The supervisor requires actual SIGKILL and strictly increasing local `max`,
`oom` and `oom_kill` counters, with `memory.max=64 MiB` and `memory.swap.max=0`.
Per the [kernel cgroup v2 contract](https://docs.kernel.org/admin-guide/cgroup-v2.html),
`oom_kill` alone can include other OOM causes, so it is not sufficient evidence.
Missing, malformed, duplicate, stale or decreasing required counters fail closed.

The supervisor writes scalar evidence and stays alive until the harness has
observed both its proof and Docker `OOMKilled=true`, then exits nonzero. This
preserves the original Docker assertion while preventing normal probe teardown
from racing event consumption. Start completion is not claimed to synchronously
acknowledge upstream watcher registration. The original 15-second host bound
remains; child and handshake waits are also bounded, a timed-out child is killed
and reaped, and the host removes the test container on its timeout. No host OOM
score or cgroup setting is changed. The separate production compiler OOM test
is unchanged.

## Verification

Local Rust 1.98.1 standalone probe build with the CI flags, seven test entries
(six regression tests plus their inert child fixture), workspace formatting,
and `git diff --check` pass. Regressions cover parsing, fresh counter deltas,
ordinary exit 137 versus a real signal, killed-child timeout cleanup, and
present/absent handshake gating. The Native Runtime workflow now runs these
standalone tests before image assembly.

The cloud executor cannot run Docker/cgroups or loopback-native acceptance.
No local OCI pass is claimed. Final-head hosted Native Runtime must rebuild the
probe image and pass actual memory/PID/file limits, compiler OOM classification,
restore and lifecycle acceptance, together with all other applicable CI and
independent native review. One hosted success establishes that run, not a
statistical proof that all timing flakes are eliminated.
