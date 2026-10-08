# Pure-shell deployment and updates

## Request

Replace the one-command installer and deployment/update runtime with ordinary
shell scripts. The host must not need Python, Node.js or jq, and installation
must continue to use prebuilt release binaries and GHCR images only.

Delivery is a separate follow-on to the r9 candidate in [PR #178](https://github.com/zhengui666/QuaZonai/pull/178).

## Preserved behavior

- Original installation JSON, owner, absolute path, Compose identity, credentials,
  volumes, independent Codex version/home and operator configuration
- Exclusive deployment lock; durable preparing/migrating/starting markers;
  complete backup before migration; exact-processor startup retries
- Prebuilt image/revision validation, Linux x86_64/systemd prerequisites,
  stopped/idle gates and fail-closed recovery without old-code schema rollback
- Installed Runtime, source tools, private native login, configuration apply and
  matching-release access-cutover commands

## Implementation and checks

The Linux manager is `deploy/docker/manage.sh`, with a strict bundled POSIX awk
JSON reader. Codex uses `codex.sh`. The standalone installer also supports the
macOS native CLI, without a Linux stack. Release Python remains CI-side only;
no Python file is shipped in the 13-file deployment bundle.

Run `node --test deploy/install.test.mjs deploy/docker/manage_shell.test.mjs` and
`bash deploy/docker/codex-shell.test.sh` for isolated command-boundary checks.
On macOS, the CLI release job supplies `QUAZONAI_TEST_LINUX_BASH` from Homebrew
for Linux-stack dispatch fixtures only. Native CLI, catalog and documented
bootstrap fixtures remain on `/bin/bash` 3.2 with the host awk; this is not a
Mac installer dependency. Catalog checks reject raw and escaped NUL before
BSD awk can erase bytes or confuse property names.
The Container acceptance harness invokes the shipped shell implementation for
real installation, upgrade, interruption and recovery checks.

Local tests do not establish a real Docker/systemd installation, macOS execution,
registry publication or a completed release. Those remain separate acceptance
and publication steps.
