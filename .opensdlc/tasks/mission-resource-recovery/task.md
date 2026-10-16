# Mission resource recovery and optional CPU/output budgets

Status: candidate, pending final compiler/tests and independent review.

## Current requested behavior

Application wall time, cumulative CPU, token count and output-byte budgets may all be absent. Absence is represented by None, never by zero or a large fake limit. Memory limits, CPU rate control, user cancellation, finite owner credentials and real platform failures remain. RPC/frame/input parsing boundaries are unchanged.

## Resource lifecycle

- One resource ledger entry per exact Run / Attempt / owner epoch / launch UUID
- Current PostgreSQL owner fence authorizes recovery only after an older epoch lost its lease
- Host cgroup cpu.stat and Linux Docker cumulative total_usage include exited descendants
- Known final CPU is recorded while the whole resource is frozen, before destruction
- Missing CPU statistics are NULL with explicit unknown status, never fabricated zero
- With no CPU cap, unknown statistics do not prevent recovery once the exact old resource is confirmed stopped
- A finite CPU cap still refuses unknown historical usage; clean close/reopen retains consumed CPU
- Durable create and execution phases distinguish an unused reservation or unstarted Docker container from an unknown execution
- Previously launched finite Missions without a ledger retain the original finite deadline/rate behavior
- The existing Mission select loop enforces configured cumulative caps even during database/RPC waits; no additional daemon is created
- Unlimited CPU mode records the available final/recovery CPU observation without a metering-driven stop condition
- Absent output caps omit the application-controlled native fsize limit

## Remaining unsupported combination

A finite cumulative CPU cap with absent wall time still requires an independently surviving enforcer. The QZ poller cannot survive QZ SIGKILL; cgroup/Docker quotas are rates and RLIMIT_CPU is per process. Native launch rejects this particular unsupported combination. Explicitly absent CPU plus absent wall is permitted and does not require nonexistent CPU remaining-budget evidence.

Polling for configured finite caps is 200 ms, plus metric, database and termination latency. Checkpoint writes have a 2-second timeout. This is not a zero-overshoot guarantee. No production verification is claimed.

## Verification

Tests cover full identity, aggregate CPU, counter absence/overflow, accounting across reopen, stale-owner rejection, pre-launch cleanup, finite legacy compatibility, and unlimited CPU with unknown history. PostgreSQL tests require the real disposable PostgreSQL/PGMQ fixture. Native host/Docker tests require their real facilities. The development cloud container has no cgroup v2 mount or Docker socket and denies systemd user-bus access; those acceptance tests are not claimed as run.

Compiler/test results accompany the delivery artifacts. No production services, model turns, user computer, remote branch, merge or deployment are modified.

## Verified native capability evidence

The installed cloud tool reports systemd 257. Its official scope implementation accepts RuntimeMaxUSec only while UNIT_STUB; controller disappearance does not itself stop the scope. The timer is based on active-enter monotonic time plus the fixed duration. Live RuntimeMaxSec heartbeat renewal is not a supported solution here:

- https://raw.githubusercontent.com/systemd/systemd/v257/src/core/dbus-scope.c
- https://raw.githubusercontent.com/systemd/systemd/v257/src/core/scope.c
- https://docs.docker.com/reference/dockerfile/#healthcheck
