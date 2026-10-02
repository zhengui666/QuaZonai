# Original compiler cgroup failure evidence

## Failure and scope

The postmerge [Native Runtime job](https://github.com/zhengui666/QuaZonai/actions/runs/36919475629/job/110562217678)
at `b58ec6d153d5b211b2941bf0d603ea3a3108a68b` failed the real compiler OOM
assertion: expected `MEMORY_LIMIT`, observed `NATIVE_JOB_FAILED`. The original
container ran for about 96 ms, exited 1 and reported `OOMKilled=false`; the same
facts persisted after five seconds and in its durable journal. The other 17 OCI
tests passed, including normal compilation and the separate memory probe's
actual SIGKILL, fresh local kernel OOM counters and Docker OOM observation.

This failure shape predates this source: the compiler observation follow-up in
[the historical task](../polymarket-public-history/task.md) retained the same
assertion after failures in PR #128. The separate [memory probe correction](../native-isolation-oom-observation/task.md)
does not establish why this compiler attempt failed.

The [production compiler](../../../apps/job/src/managed.rs) runs `rustc --version`
before compilation and discards both commands' stderr. Its child termination
detail is not exposed by the container's exit 1. Neither a specific compiler
failure, local OOM with a missed daemon event, nor PID exhaustion is established
by the retained original evidence. Runtime's classification follows the actual
Docker observation; changing that policy or the pressure fixture is unsupported.

## Bounded diagnostic change

Only [the test](../../../apps/runtime/tests/native_oci.rs) and its
[observer](../../../apps/runtime/tests/support/compiler_memory_observation.rs)
change. Observation starts before submission. It uses the fixture's run label,
checks the exact immutable image, container ID, run, attempt, external ID and
JOB role, then requires a running Docker PID. That PID must name this exact
container's default systemd or cgroupfs cgroup v2 path. PID start time and cgroup
membership are checked before and after every sample; loss ends observation.
No other cgroup is searched, and unsupported layouts remain unavailable.

Within the original 30-second job budget, Docker identity polling uses 10 ms
intervals and the host sampler uses 2 ms intervals. Each kernel-file read is
limited to 4096 bytes. The sampler retains only first/last numeric local
`memory.events.local` max/oom/oom_kill counters, memory peak/limit/swap limit,
`pids.events` max and PID limit, plus count and closed availability/stop reasons.
Missing or invalid optional values remain null. It writes no host path, process
command, credentials, source, free-form error or stderr. Explicit finish and
RAII drop stop and join the owned observer, including assertion unwinding.

These are best-effort samples of the original attempt, not final kernel
accounting or an acceptance oracle. A short-lived cgroup can be missed; flat or
missing samples cannot prove the absence of a resource failure. Fresh local OOM
or PID-limit observations would narrow a future failure's cause, but cannot
identify the compiler phase or recover stderr. No observation is automatically
converted into `MEMORY_LIMIT` or a passing assertion.

The production image, compiler invocation, launch, classification, limits,
deadlines and cleanup remain unchanged. All original failure-class/message,
Docker OOM, zero-output, privacy and idempotence assertions remain. No compile
retry or new permission is introduced. This repairs evidence collection; it
does not claim to fix the underlying intermittent failure.

## Verification and remaining gate

Four standalone Rust 1.98.1 tests pass, including exact cgroup identity and
path rejection, unsigned/complete/unique memory and PID counters, bounded reads,
missing/malformed scalar handling, PID/start-time parsing, rejection of an
unrelated live host PID, the sampler deadline, and owned observer cancellation.
The helper passes standalone Clippy with warnings denied. Workspace formatting
and whitespace checks pass.

Disk constraints prohibit local Cargo compilation/linking, Docker or model
execution. Full-target Clippy and original OCI acceptance have not run here.
Independent review of the exact immutable patch is required before considering
one focused hosted acceptance with the unchanged scientific assertions. A pass
would verify that attempt, not establish the previous failure's root cause or
prove that every timing failure is eliminated.
