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

Install or update this exact release, including the native CLI, in one command:

```sh
curl -fsSL https://github.com/zhengui666/QuaZonai/releases/download/@QUAZONAI_VERSION@/install.sh | bash
```

The producer substitutes the tagged version above in every published bundle.
For a Linux CLI-only host, use `bash -s -- --cli-only` in place of `bash`.
The same Release's generated README contains native Windows and macOS commands.
Each release includes SHA256SUMS, all four CLI archives and application, database,
scientific Runtime and Codex image archives. The installer downloads prebuilt
artifacts and verifies checksums; it never checks out source or builds binaries or
images. `--bin-dir` selects the CLI directory; `--directory` selects the cluster.
The existing installation/upgrade recovery and prerequisites below still apply.

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

For a custom installation directory, use its `current/bin` path. The cluster bundle retains its `quazonai` link to the native `server` executable; the one-line installer also places the standalone portable CLI in `$HOME/.local/bin`; existing internal Worker and maintenance paths remain valid. Enter the frontend HTTP or HTTPS address and password in the private login prompt. Explicit HTTP works without an extra development flag; it transmits credentials in cleartext and is intended for trusted networks. Use HTTPS for public connections. Subsequent `quazonai client` commands reuse the saved device login. See the [service Skill connection guide](../../skills/quazonai/references/connection.md). Mission and Downstream credentials retain their original restrictions.

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

### Installed free-source tools

These commands require a completed release that includes the source tools in its
manager and application image; a source-checkout build does not add them to an
older installation. Check the active release's help and plugin inventory first.
The examples below only inspect help, capabilities and an offline HTTP plan;
Coinbase live research acceptance remains subject to the [source-use block](../../runtimes/data/source-plugins.md#source-rights-and-acceptance).
Use the installed manager as the installation owner; no checkout, Python package
installation or Cargo build is needed:

```sh
installation="$HOME/.local/share/quazonai"
python3 "$installation/current/deployment/manage.py" source --help
python3 "$installation/current/deployment/manage.py" source -- plugins
python3 "$installation/current/deployment/manage.py" source -- \
  plan coinbase-candles --instrument BTC-USD --start-seconds 1788220800 \
  --end-seconds 1788220980 --interval-seconds 60
```

Add `--directory /absolute/installation` immediately after `source` for a custom
installation. The active release's immutable image supplies both the helpers and
native binaries. An incomplete application update must finish first. Installed
operations reject `--native-bin` overrides.

File operations need explicit owner-managed mounts. Each `--read-only` input may
be a directory or regular file. A writing operation also requires a separate,
existing `--output-parent`; its `--output` must name a new child. Inputs and output
parents cannot overlap, contain symlinks, expose installation/Codex state, or
replace container system/tool paths. Mount paths support spaces and Unicode but
not control characters, colons, commas, double quotes or backslashes. Original and failed
artifacts are retained. Use another new output name for a retry.

For example, after a supported native conversion and after supplying the original
explicit declaration and native selection:

```sh
python3 "$installation/current/deployment/manage.py" source \
  --read-only /absolute/native/lokima-selection \
  --read-only /absolute/declarations --output-parent /absolute/prepared -- \
  prepare polymarket-capture --native-output /absolute/native/lokima-selection \
  --declaration /absolute/declarations/discovery.json \
  --selection /absolute/declarations/discovery-selection.json \
  --output /absolute/prepared/lokima-discovery
```

The image's capability inventory determines network access: inventory,
archive inspection/freezing, verification, conversion and preparation are offline; only public acquisition
and snapshot planning receive network access. No installation credentials or
Docker socket enter the one-shot source container. The API/Worker are not
restarted. See the [source guide](../../runtimes/data/source-plugins.md) for actual
format capabilities, original input requirements and source-checkout examples;
the installed command accepts the same operation arguments without `--native-bin`.

For an already supplied Binance spot archive, use
`binance-vision-spot-klines` through the same source command. Its offline lifecycle
is `plan`, `inspect`, `freeze`, `verify`, `convert`, then `prepare`; `freeze`,
`convert` and `prepare` each need a separate new output under an explicitly
mounted output parent. The [archive guide](../../runtimes/data/binance-vision.md)
describes original file/definition requirements, explicit unverified or synthetic
receipt clocks, and the ordinary ZIP profile. There is no archive downloader or
terms-acceptance flag. A native result is not upstream permission or research
qualification.

The manager prints a source invocation identity before starting containers. Use
`--invocation-id` with a new 32-character lowercase hexadecimal value when a
caller needs to record that identity before launch. The actual and inventory
containers carry that identity, installation and owner labels, with names
`quazonai-source-<identity>` and `quazonai-source-<identity>-inventory`. A stopped
terminal or lost Docker CLI does not confirm container cancellation: inspect the
matching owned containers before stopping them or removing their mounted files.
Keep diagnostics and partial output when the actual outcome is uncertain.

Preparation returns the original metadata file's byte digest, host paths,
`catalog_registration` and two identity hints. The host paths are mounted at the
same absolute locations in the container, so they can be used verbatim by a
Runtime on that host. Keep source evidence outside the catalog mount. Copy only
the registration fragment into the chosen Runtime configuration through its
normal stopped/configuration-apply process. Service registration still fetches
the actual document from Runtime. Choose the Runtime, grant/permission evidence,
Universe and InputSet explicitly, and perform fresh `DATA_VALIDATE`. Missing
definitions, fees, historical membership or availability are never filled in by
the tools; successful preparation does not qualify research data.

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

The updater downloads the target bundle/image, checks compatibility and idle state, stops this installation, creates a recovery point, explicitly migrates, and activates after API/Worker checks. It preserves PostgreSQL, data, credentials and Codex version. Older versions are rejected; same-version installation is idempotent. Timestamped `v<core>-dev.<UTC timestamp>.<run ID>` Releases are installable here. The separate manual `dev-<sha>-...` image-only channel has no bundle and cannot be used here. Set the stopped gateway configuration to the target manifest's `runtime_image`, restart it using the target `runtime.sh`, and probe capabilities before new research; keep its original state and catalogs.

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

<a id="cli-http"></a>
## Remote HTTP CLI

Local `http://localhost:<web-port>` CLI access needs no extra configuration. To expose a separate HTTP endpoint on a trusted network, keep the existing browser `PUBLIC_URL` and add the exact external origin to the installation's persistent `<installation>/compose.override.yaml` (merge with existing settings; keep a private backup before editing):

```yaml
services:
  app:
    environment:
      CLI_HTTP_ORIGIN: http://research.lan:18080
```

Use your actual host and port. `CLI_HTTP_ORIGIN` contains only an HTTP origin, with no credentials, path, query or fragment. Docker still listens on loopback. Configure the trusted-network reverse proxy to forward that origin to `http://127.0.0.1:<web-port>` while preserving `Host`, `Origin`, `Authorization` and `X-Quazonai-Cli`. The bundled Caddy forwards those headers unchanged and does not enable access logging. Keep passwords, Authorization headers and response bodies out of any external proxy logs. HTTP sends the password and device token without TLS; HTTPS remains available without this option.

Finish/reconcile all Runs and Codex operations, then apply the override using the installed manager's existing maintenance operations:

```sh
python3 - "$HOME/.local/share/quazonai" <<'PY'
import sys
from pathlib import Path
root = Path(sys.argv[1]).resolve()
sys.path.insert(0, str(root / 'current/deployment'))
import manage as m
import codex
m.preflight()
with m.locked(root):
    if (root / 'pending.json').exists():
        raise ValueError('Complete the recorded installation/update first')
    config = m.configuration(root)
    m.compose(config, 'config', '--quiet')
    m.require_idle(config)
    codex.require_stopped(config, recover_created=True)
    try:
        m.configure_app_restarts(config, False)
        m.compose(config, 'stop', 'app')
        m.require_idle(config)
        codex.require_stopped(config, recover_created=True)
        m.run(['systemctl', '--user', 'disable', '--now', m.unit(config)])
        m.require_idle(config)
    except Exception:
        m.resume_existing_services(config)
        m.configure_app_restarts(config, True)
        raise
    m.compose(config, 'up', '--no-start', '--no-deps', 'app')
    m.configure_app_restarts(config, False)
    print('Configuration prepared; starting the original installation', flush=True)
    m.resume_existing_services(config, enable_boot=False)
    m.verify_worker(config)
    m.verify_console(config)
    m.run(['systemctl', '--user', 'enable', m.unit(config)])
    m.configure_app_restarts(config, True)
PY
```

This uses the existing image, database, state and Worker; it performs no build or migration. The installation-root override survives application updates and rollback/retry because every manager Compose call loads it. A startup/check failure remains a failed apply: correct or restore the saved override and repeat after idle checks. If interrupted after the prepared message and either process may be running, use the revision-pinned `runtime-targets` recovery command above to resume without recreation before applying another change.

On the client, run `quazonai client --origin http://research.lan:18080 login`, enter the password in your own terminal, then `quazonai client identity`. Saved connections retain the same origin and native CLI marker. This additional origin accepts only CLI login and cookie-free owner-device (`qzc`) bearer API requests; scoped machine/Mission (`qz2`) credentials retain their existing HTTPS/loopback policy: the CLI rejects remote HTTP before sending them, and the server also rejects them at this origin. It does not permit browser setup/login/session routes or relax MCP/Runtime transport validation.

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
