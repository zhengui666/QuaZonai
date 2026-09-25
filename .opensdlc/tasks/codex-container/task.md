# Containerized Codex

<a id="task"></a>
## Task

**Task ID:** codex-container.

**Request and endpoint:** The owner requested a separately built Codex container,
ChatGPT authentication persisted across replacement, a version configured in
`.env`, and an update command accepting a version or defaulting to latest. Develop
from current main, verify, open a PR and merge after current-head checks and review.
The owner explicitly instructed this session to ignore AGENTS.md.

<a id="intent"></a>
## Intent

Build from a general-purpose base image, install the official Codex distribution,
and preserve native authentication without copying credentials into an image or
source control. Keep the existing dirty checkout and installed services untouched.

<a id="spec"></a>
## Requirements and design

Use the existing Debian/npm multi-stage packaging pattern and native device-code
login. Persist the complete writable `CODEX_HOME`; Codex owns token storage and
refresh. Resolve `latest` to an exact published version, build and check the
candidate before changing `.env`. Failed builds must retain the previous version
and authentication storage.

The owner confirmed replacement of QuaZonai's Codex backend. Both API account/model
requests and Worker Missions use the same independently built image through the
existing Bollard Docker dependency. Each App Server owns a container with a real
identity; Missions retain independent native resource limits and remaining wall
time. Use host networking for the existing loopback MCP contract and mount only
the native home, workspace and required MCP executable. The Codex container never
receives the Docker socket. The trusted API/Worker receive that socket directly.

<a id="plan"></a>
## Implementation plan

Inspected `deploy/docker`, native `Launch`, `MissionProcess`, profile discovery,
Mission MCP configuration and container CI. Reuse Docker Compose, official npm
packages, native authentication and existing GitHub Actions. Implement the Rust
transport/lifecycle, deployment image/update scripts and container acceptance in
separate file scopes; integrate documentation and CI. Verify failure preservation,
real protocol/container behavior, limits and cancellation, then obtain independent
review and current-head CI before merging.

<a id="verification"></a>
## Verification

Base: `a187c2eb4629da438ce1b40a24effb4ab3c218f8` from fetched `origin/main`.
The worktree is isolated at `/home/zzy/.codex/worktrees/codex-container/QuaZonai`.
Local Docker access is denied; `sudo -n` is unavailable under this session's
no-new-privileges restriction. Actual image execution must run in GitHub Actions.
Local checks passed: `cargo check --locked -p server --tests --features codex-container`,
the container-test Clippy check with `-D warnings`, 15 `codex_native::` unit tests,
2 deployment discovery tests, and Rust formatting. Web type checking and all 528
existing frontend tests passed. `python3 -B -m unittest discover -s deploy/docker
-p '*_test.py'` passed 65 tests; shell syntax, workflow YAML parsing and
`git diff --check` passed. These checks do not establish Docker execution or a
successful ChatGPT authorization.

The Container workflow exercises fresh-home native App Server sessions, persistent
home replacement, actual cgroup limits, cancellation/deadline/start-interruption
cleanup, and native Mission sandbox execution. Its model response fixture is
explicitly test-only and validates actual tool output, not echoed command text.
The deployment smoke additionally checks the real API probe, independent version
switching, preserved home, refusal while a container is active and the mounted
MCP executable's ABI. CI and review results belong to the containing PR.

<a id="review"></a>
## Review

Independent read-only reviews found and prompted fixes for Docker API negotiation,
orphaned CREATED-container recovery, host Git prerequisites, non-Mission data
mounts, old deployment-bundle migration, and release gating. Reuse the deployment
lock for the short startup critical section; do not create a new coordinator.
Exact-head GitHub review and real Docker CI remain required before merge.

The first real-container CI built and verified Codex 0.157.0, then exposed Ubuntu's
AppArmor restriction on capabilities inside an unprivileged user namespace.
Reuse the repository's existing narrowly attached `userns` profile pattern and
add the same native sandbox preflight before candidate image switching. Keep
non-root execution, dropped capabilities and no-new-privileges unchanged.

<a id="delivery"></a>
## Delivery

The PR containing this task is the authority for CI, review and merge state.
The delivery endpoint is merge to main, without installing a production release
or initiating the owner's private ChatGPT login. Login and independent version
updates are documented in [the deployment guide](../../../deploy/docker/README.md).

<a id="handoff"></a>
## Handoff

Branch: `codex/standalone-codex-container`. No production operations are authorized
by this development PR. Initial ChatGPT authorization requires the owner's browser;
CI must use a fresh home and never consume production credentials.
