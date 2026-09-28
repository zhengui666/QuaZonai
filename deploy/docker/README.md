# QuaZonai deployment

The release bundle runs Web/API/Caddy and PostgreSQL 18/PGMQ with Docker Compose. It extracts the same image's Worker and scientific gateway binaries. Codex and scientific jobs use separate prebuilt GHCR images; the native Codex home persists across updates. Scientific Runtimes and catalogs are registered separately.

<a id="prerequisites"></a>
## Prerequisites

Use a non-root Linux x86_64 owner with local Docker Engine, Compose 2.20+, Python 3.10+, Git, systemd (including `systemd-analyze`) and cgroup v2. The host Worker requires glibc 2.36+ and OpenSSL 3. Rust, Node.js and Codex are not needed on the host. The host needs access to GHCR for image downloads.

Docker must use a local Unix socket and a rootful daemon without `userns-remap`. Docker Desktop, remote contexts and rootless/remapped daemons are unsupported. API and Worker use the owner's Docker access; Codex containers do not receive the socket. Keep the application on its default local interface.

Enable persistent user services once:

```sh
loginctl enable-linger "$USER"
```

Run deployment scripts as this owner, not with `sudo`. For a private GHCR package, run `docker login ghcr.io` first; never put registry credentials in the bundle.

<a id="install"></a>
## Install

Download `quazonai-deploy.tar.gz` from a [GitHub Release](https://github.com/zhengui666/QuaZonai/releases) that provides the bundle. Extract it into an empty directory, then run:

```sh
bash deploy.sh
```

The default installation is `$HOME/.local/share/quazonai`, the browser address is **http://localhost:8081**, and PostgreSQL is published at `127.0.0.1:55432`. Web and database ports bind to loopback. The installer pulls the application, scientific job, Codex and database digests in `release.json`, initializes new state, runs migrations and checks API/Worker startup. Repeating installation preserves the original identity, password, key and data.

Optional initial settings:

```sh
bash deploy.sh --directory "$HOME/.local/share/quazonai" \
  --port 8081 --database-port 55432
```

`release.json` supplies the default Codex version. An optional `.env` copied from `.env.example` can select another exact version already published in `ghcr.io/zhengui666/quazonai-codex`; a version existing only on npm is not installable here.

Installation paths may contain spaces, Unicode, percent signs and brackets, but not control characters, colons, double quotes or backslashes. Keep the chosen absolute path and owner after installation. `.env` accepts `CODEX_VERSION=<exact-version>` and comments, not shell code; an existing installation's `.env` takes precedence.

New installations use `<installation>-codex` as the persistent Codex home. `--codex-home /absolute/path` selects another; neither the home nor installation may contain the other, including through symlinks. Do not run unrelated Codex sessions against a shared home during login or version changes.

<a id="login"></a>
## Login and research

Open the browser address and set the QuaZonai instance password on first use (including the first upgrade from password-free access). Later visits require that password. Select **记住本设备 30 天** to keep a browser signed in for 30 days; otherwise the session ends with the browser and lasts at most 12 hours. This password is separate from PostgreSQL and ChatGPT credentials.

**设置 → 鉴权管理** changes the password and lists connected CLI machines. A password change signs browsers out; CLI machines remain connected until individually removed. The installation provides `quazonai` in its current release's `bin` directory:

```sh
export PATH="$HOME/.local/share/quazonai/current/bin:$PATH"
quazonai --version
quazonai client login
```

For a custom installation directory, use its `current/bin` path. `quazonai` links to the same native `server` executable; existing internal Worker and maintenance paths remain valid. Enter the frontend address and password in the private login prompt. Subsequent `quazonai client` commands reuse the saved device login. See the [service Skill connection guide](../../skills/quazonai/references/connection.md). Mission and Downstream credentials retain their original restrictions.

Open **Settings → Codex → ChatGPT Auth → 登录 ChatGPT**. Copy the device code, open the authorization link and complete login on OpenAI's page. QuaZonai refreshes account/model status; researcher and reviewer share the account but have separate model settings. Refreshing the page cannot recover its code: finish in the original page or cancel and restart. Device-code authorization must be allowed by the account/workspace.

A private terminal is an alternative login entry:

```sh
bash "$HOME/.local/share/quazonai/current/deployment/codex-login.sh"
bash "$HOME/.local/share/quazonai/current/deployment/codex-login.sh" --status
```

These commands require idle Runs/sessions and hold the deployment lock. Credentials stay in the native home; application updates do not remove them. [Configure a scientific Runtime](#scientific-runtime) and real data before starting research. API startup and account login do not provision those inputs.

<a id="scientific-runtime"></a>
## Scientific Runtime and data

The installer extracts the scientific gateway from the application image and pulls the matching job image. Configure its catalogs, credential and resource limits before starting the gateway. From this bundle directory, print the setup, configuration-apply and recovery guides pinned to the exact `release.json.revision`:

```sh
python3 - <<'PY'
import json, re
from pathlib import Path
revision = json.loads(Path('release.json').read_text())['revision']
if not re.fullmatch(r'[0-9a-f]{40}', revision):
    raise ValueError('Invalid release revision')
base = f'https://github.com/zhengui666/QuaZonai/blob/{revision}/.opensdlc'
for path in ('operations.md#scientific-runtime',
             'operations.md#runtime-targets', 'operations.md#runtime-recovery',
             'operations.md#access-cutover'):
    print(f'{base}/{path}')
PY
```

For an installed copy, its bundle is `<installation>/current/deployment`. Open the printed revision-specific configuration guide; use `runtime.sh` from that installed bundle and its `release.json.runtime_image` digest. The gateway is a separate host process; scientific jobs run in its registered Docker image. Its loopback listener needs an existing trusted HTTPS reverse proxy reachable from both the API container and host Worker. Container-local `127.0.0.1` does not reach the host.

Follow `runtime-targets` to set the exact HTTPS `origin` and reachable `addresses`, close admissions, preserve configuration and apply both the API and Worker environments using the installed manager. Editing `installation.json` alone or repeating a same-version deployment is insufficient. Then register the matching endpoint and credential in Runtime settings, probe readiness, and register actual catalogs. A successful probe or empty catalog list is not research data.

<a id="codex-update"></a>
## Update Codex

Finish Runs and login sessions, then pull the latest published QuaZonai Codex image:

```sh
bash "$HOME/.local/share/quazonai/current/deployment/codex-update.sh"
```

An exact published image version may be supplied as the first argument. A missing image fails without changing the selected version. The updater pulls the selected GHCR image and checks its native version and sandbox, requires idle execution, then switches the installation's image and `.env`. No application restart or database migration is needed. After an interrupted switch, retry the same explicit version. The native home and old images are retained.

<a id="update"></a>
## Update QuaZonai

Finish all Runs, or cancel them and wait for their actual terminal state. Stop the independent scientific gateway after its jobs finish and preserve its configuration/state. Select an existing published release tag:

```sh
read -r -p 'Published release tag: ' version
bash "$HOME/.local/share/quazonai/current/deployment/update.sh" "$version"
```

The updater downloads the target bundle/image, checks compatibility and idle state, stops this installation, creates a recovery point, explicitly migrates, and activates after API/Worker checks. It preserves PostgreSQL, data, credentials and Codex version. Older versions are rejected; same-version installation is idempotent. Dev-image tags are not release tags and cannot be used here. Set the stopped gateway configuration to the target manifest's `runtime_image`, restart it using the target `runtime.sh`, and probe capabilities before new research; keep its original state and catalogs.

For the first upgrade from a deployment bundle with `release.json.schema_version=1`, use the freshly extracted **target** bundle and run inside that directory:

```sh
python3 manage.py apply-update --directory "$HOME/.local/share/quazonai"
```

The old updater does not accept the version-2 file set. Subsequent updates use the installed `update.sh` above.

<a id="status"></a>
## Status and configuration

```sh
python3 "$HOME/.local/share/quazonai/current/deployment/manage.py" status
```

Before `current` exists, use the extracted bundle's `manage.py status`. Commands accept `--directory /absolute/installation/path` for a non-default installation.

Keep `installation.json`, the data directory, `master.key`, `.env`, original ports and Compose project identity. Runtime/downstream settings use the manifest's `runtime_targets` and `downstream_targets`; a local `compose.override.yaml` survives updates. Runtime targets must be reachable through their configured HTTP transport as described [above](#scientific-runtime); the native gateway does not listen on a Unix HTTP socket. Empty target lists do not create a Runtime.

<a id="recovery"></a>
## Backups and recovery

Before migration, the updater saves `backups/<timestamp>-<id>/` with `database.dump`, `data.tar.gz`, `installation.json` and a separate `master.key`. Copy backups off the installation disk and keep the key separately protected. Back up Codex home separately and use the revision-pinned `runtime-recovery` guide [above](#scientific-runtime) for stopped scientific catalogs/journals and ownership-preserving recovery.

If install/update stops with an error, retain `pending.json`, the original state and recovery point. Correct the cause and retry the **same target version**. A `preparing` failure can restore the old processors; `migrating` may already have changed the schema; `starting` resumes the same candidate without repeating migration. Do not delete the marker, regenerate identity/key material, remove volumes or start an older binary to bypass the failure.

A cold restore requires a matching database, application state, key, installation identity, runtime state and release. Stop all writers and reconcile remote work first; restore original paths/ownership. With API/Worker stopped and old database transactions ended, follow the revision-pinned `access-cutover` guide printed [above](#scientific-runtime). It invokes the matching `<installation>/releases/<version>/bin/server`, privately obtains the restored database's migration-owner `DATABASE_URL`, creates and saves one UUIDv7 using `uuidgen --time-v7`, and checks/saves the native receipt. Unknown outcomes reuse that record; each distinct database restore gets a new record. Reissue needed machine connections and reconcile original remote tasks before resuming. Restoring a database alone is not a complete recovery.

<a id="troubleshooting"></a>
## Troubleshooting

| Symptom | Action |
| --- | --- |
| GHCR pull denied | Check package visibility and the owner's Docker login |
| Missing manifest but existing Compose resources | Restore the original installation identity from backup; do not generate a new password/key for the old database |
| Worker ABI or unit verification fails | Fix host prerequisites/path before retrying; the installer checks the candidate before stopping an existing release |
| Codex sandbox reports `Operation not permitted`, including `bwrap: loopback: Failed RTM_NEWADDR` | On hosts with restrictive AppArmor user-namespace policy, load the included profile below |

For the AppArmor case only, an administrator installs the executable-specific profile:

```sh
sudo install -m 0644 codex.apparmor /etc/apparmor.d/quazonai-codex
sudo apparmor_parser -r /etc/apparmor.d/quazonai-codex
```

Retry the original deployment/Codex update as the installation owner. Do not disable the host policy globally.
