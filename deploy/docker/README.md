# QuaZonai deployment

The release bundle runs Web/API/Caddy, the official Pi Durable Node host and PostgreSQL 18/PGMQ with Docker Compose. The application image includes its own Node executable, production npm dependencies, trusted research extensions and matching factor Job binary. Scientific Worker and Runtime binaries remain host-side; OCI scientific jobs use their separate prebuilt image. Durable conversations/tasks/usage persist in the native SQLite database under `data/research`.

<a id="prerequisites"></a>
## Prerequisites

Use a non-root Linux x86_64 owner with local Docker Engine, Compose 2.20+, Bash 4.4+, curl, tar, awk, Git, GNU coreutils, util-linux, systemd (including `systemd-analyze` and `systemd-socket-activate`) and cgroup v2. The host Worker requires glibc 2.36+ and OpenSSL 3. Python, jq, Rust, Node.js and Codex are not needed on the host. Standard Unix tools and the Linux host tools required by the manager must already be installed; no packages or sudo actions are run automatically. The host needs access to GHCR for image downloads.

Docker must use a local Unix socket and a rootful daemon without `userns-remap`. Docker Desktop, remote contexts and rootless/remapped daemons are unsupported. Host scientific Runtime/source helpers retain Docker access; the application container has no Docker socket or supplemental socket group. Keep the application on its default local interface.

Enable persistent user services once:

```sh
loginctl enable-linger "$USER"
```

Run deployment scripts as this owner, not with `sudo`. For a private GHCR package, run `docker login ghcr.io` first; never put registry credentials in the bundle.

<a id="install"></a>
## Install

Install or update this exact release, including the native CLI, in one command:

```sh
sh -c 'f=$(mktemp) || exit; cleanup() { rm -f -- "$f"; }; trap cleanup 0; trap "exit 1" 1 2 3 15; curl --fail --location --proto "=https" --proto-redir "=https" --tlsv1.2 --output "$f" https://github.com/zhengui666/QuaZonai/releases/download/@QUAZONAI_VERSION@/install.sh && bash "$f" "$@"' sh
```

The producer substitutes the tagged version above in every published bundle.
CLI-only installation on Linux/macOS supports Bash 3.2+; full Linux management requires Bash 4.4+. For a Linux CLI-only host, append `--cli-only` to the command. Downloads finish successfully before any script executes; a failed transfer is cleaned up.
The same Release's generated README contains native Windows and macOS commands.
Each release includes all four CLI archives and application, database,
scientific Runtime image archives. The installer downloads prebuilt
artifacts, checks their version and archive structure, and does not perform file checksum verification; it never checks out source or builds binaries or
images. `--bin-dir` selects the CLI directory; `--directory` selects the cluster.
The existing installation/upgrade recovery and prerequisites below still apply.

Download `quazonai-deploy.tar.gz` from a [GitHub Release](https://github.com/zhengui666/QuaZonai/releases) that provides the bundle. Extract it into an empty directory, then run:

```sh
bash deploy.sh
```

The default installation is `$HOME/.local/share/quazonai`, the browser address is **http://localhost:8081**, and PostgreSQL is published at `127.0.0.1:55432`. Web and database ports bind to loopback. The installer pulls the application, scientific job and database images in the schema-3 `release.json`, initializes new state, runs migrations and checks API/Worker/durable-host startup. Repeating installation preserves the original identity, password, key and data.

Optional initial settings:

```sh
bash deploy.sh --directory "$HOME/.local/share/quazonai" \
  --port 8081 --database-port 55432
```

Installation paths may contain spaces, Unicode, percent signs and brackets, but not control characters, colons, double quotes or backslashes. Keep the chosen absolute path and owner after installation. Research configuration and credentials are not stored in `installation.json`.

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

### Native research configuration

The real Bash installer creates private `data/research/config.json` once:

```json
{"database":"/absolute/installation/data/research/research.sqlite","modules":["/opt/quazonai/research/plugins/research.ts","/opt/quazonai/research/plugins/factor-wasm.ts","/opt/quazonai/research/plugins/heldout-wasm.ts"],"port":8092}
```

Use your actual installation path. The Node host runs only on container loopback 8092; browser/device CLI traffic reaches it through Rust's owner-authenticated `/api/v2/research` proxy. Do not publish port 8092. The entrypoint stops all console siblings if Node, Rust or Caddy exits unexpectedly; restart uses the same native SQLite tasks, not a new Agent owner.

Configure a native pi-ai provider through private `data/research/provider.env` using that provider's supported environment variable names (see `.env.example`). Node alone reads it with `--env-file`; the installer, Rust server and host scientific Worker do not source it. Alternatively add `providerModules` with trusted Provider `.ts` module paths to `config.json`. Paths relative to this JSON resolve from its directory. Place user modules under `data/research/plugins`; modules can import packaged dependencies through `/opt/quazonai/research/node_modules`. Add trusted Extension paths to `modules`; optional packaged extensions are `factor-volume.ts` and `validation-compare.ts`. Keep port 8092 for the bundled proxy/health checks.

Provider credentials stay in native provider configuration, never in research input/model JSON, tools, API outputs or model context. Configuration, provider environment and user modules survive updates unchanged. Install/readiness checks perform no paid inference. [Configure scientific Runtime/data](#scientific-runtime) separately; existing catalogs under `data` remain available at their original absolute host paths in the container.

```sh
quazonai client research models
quazonai client research start --file /absolute/research-request.json
quazonai client research list
quazonai client research get 1
quazonai client research stop 1
quazonai client research tasks
quazonai client research usage
```

The start file contains `{requestId:string,input:{goal:string,model:{provider:string,modelId:string},data:object,policy:object}}`. Retain the same file/requestId after an uncertain result. Native numeric IDs/snapshots/status are returned unchanged; an accepted stop is not proof of remote cancellation.

<a id="scientific-runtime"></a>
## Scientific Runtime and data

The installer extracts the scientific gateway from the application image and pulls the matching job image. Configure its catalogs, credential and resource limits before starting the gateway. From this bundle directory, print the setup, configuration-apply and recovery guides pinned to the exact `release.json.revision`:

```sh
bash manage.sh runtime-guides
```

For an installed copy, its bundle is `<installation>/current/deployment`. Open the printed revision-specific configuration guide; use `runtime.sh` from that installed bundle and its `release.json.runtime_image` digest. The gateway is a separate host process; scientific jobs run in its registered Docker image. Its loopback listener needs an existing trusted HTTPS reverse proxy reachable from both the API container and host Worker. Container-local `127.0.0.1` does not reach the host.

Follow `runtime-targets` to set the exact HTTPS `origin` and reachable `addresses`, close admissions, preserve configuration and apply both the API and Worker environments using the installed manager. Editing `installation.json` alone or repeating a same-version deployment is insufficient. Then register the matching endpoint and credential in Runtime settings, probe readiness, and register actual catalogs. A successful probe or empty catalog list is not research data.

### Installed free-source tools

These commands require a completed release that includes the source tools in its
manager and application image; a source-checkout build does not add them to an
older installation. Check the active release's help and plugin inventory first.
The examples below only inspect help, capabilities and an offline HTTP plan;
Coinbase live research acceptance remains subject to the [source-use block](../../runtimes/data/source-plugins.md#source-rights-and-acceptance).
Use the installed manager as the installation owner; no checkout, host Python, package
installation or Cargo build is needed:

```sh
installation="$HOME/.local/share/quazonai"
bash "$installation/current/deployment/manage.sh" source --help
bash "$installation/current/deployment/manage.sh" source -- plugins
bash "$installation/current/deployment/manage.sh" source -- \
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
parents cannot overlap, contain symlinks, expose installation/historical credential state, or
replace container system/tool paths. Mount paths support spaces and Unicode but
not control characters, colons, commas, double quotes or backslashes. Original and failed
artifacts are retained. Use another new output name for a retry.

For example, after a supported native conversion and after supplying the original
explicit declaration and native selection:

```sh
bash "$installation/current/deployment/manage.sh" source \
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

<a id="update"></a>
## Update QuaZonai

Finish/reconcile all scientific Runs and inspect `research tasks`; stop durable research through its public API and wait for its actual native task graph to become idle. Stop the independent scientific gateway after its jobs finish and preserve its configuration/state. Select an existing published release tag:

```sh
read -r -p 'Published release tag: ' version
bash "$HOME/.local/share/quazonai/current/deployment/update.sh" "$version"
```

The updater observes actual Run and durable-task idle state, stops the app (including Node) and host Worker before archiving SQLite/data, creates a recovery point, explicitly migrates, and activates after API/Worker/durable health checks. It preserves PostgreSQL, all historical data/old homes, native SQLite, provider environment, user extensions and configuration. Unfinished legacy Missions reject cutover with original-outcome reconciliation guidance; nothing is marked successful/cancelled to permit upgrade. Older versions are rejected; same-version installation is idempotent. Timestamped `v<core>-dev.<UTC timestamp>.<run ID>` Releases remain installable; the separate manual `dev-<sha>-...` image-only channel has no installable bundle. Keep scientific Runtime state/catalogs and configure its matching `runtime_image` before new science.

For the first upgrade from any Python-based deployment, including schema-version 1 or 2, use the target release’s one-line installer above. It invokes the new shell manager, bypassing the old updater’s fixed bundle file list, while preserving installation state. Alternatively, extract the target bundle into an empty directory and run:

```sh
bash manage.sh apply-update --directory "$HOME/.local/share/quazonai"
```

The old updater does not accept the shell bundle file set; do not invoke it for this transition. Subsequent updates use the installed `update.sh` above.

<a id="status"></a>
## Status and configuration

```sh
bash "$HOME/.local/share/quazonai/current/deployment/manage.sh" status
```

Before `current` exists, use the extracted bundle's `manage.sh status`. Commands accept `--directory /absolute/installation/path` for a non-default installation.

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

Finish/reconcile all Runs and durable research tasks, then apply the override using the installed manager's maintenance operation:

```sh
bash "$HOME/.local/share/quazonai/current/deployment/manage.sh" apply-config
```

This uses the existing image, database, state and Worker; it performs no build or migration. The installation-root override survives application updates and rollback/retry because every manager Compose call loads it. A startup/check failure remains a failed apply: correct or restore the saved override and repeat after idle checks. If interrupted after the prepared message and either process may be running, use the revision-pinned `runtime-targets` recovery command above to resume without recreation before applying another change.

On the client, run `quazonai client --origin http://research.lan:18080 login`, enter the password in your own terminal, then `quazonai client identity`. Saved connections retain the same origin and native CLI marker. This additional origin accepts only CLI login and cookie-free owner-device (`qzc`) bearer API requests; scoped machine/Mission (`qz2`) credentials retain their existing HTTPS/loopback policy: the CLI rejects remote HTTP before sending them, and the server also rejects them at this origin. It does not permit browser setup/login/session routes or relax MCP/Runtime transport validation.

<a id="recovery"></a>
## Backups and recovery

Before migration, the updater stops the app/Node and saves `backups/<timestamp>-<id>/` with `database.dump`, `data.tar.gz` (including native research SQLite, config, user modules and provider environment), `installation.json` and separate `master.key`. Copy backups off disk and protect keys/provider secrets separately. Historical old homes/credentials are never deleted; back them up independently. Use the revision-pinned `runtime-recovery` guide [above](#scientific-runtime) for stopped scientific catalogs/journals.

If install/update stops with an error, retain `pending.json`, the original state and recovery point. Correct the cause and retry the **same target version**. A `preparing` failure can restore the old processors; `migrating` may already have changed the schema; `starting` resumes the same candidate without repeating migration. Do not delete the marker, regenerate identity/key material, remove volumes or start an older binary to bypass the failure.

A cold restore requires a matching PostgreSQL database, native research SQLite (including any WAL/SHM files in the stopped data archive), provider/config/module files, application state/key, installation identity, Runtime state and release. Stop all writers and reconcile remote work first; restore original paths/ownership. With API/Node/Worker stopped and old database transactions ended, follow the revision-pinned `access-cutover` guide printed [above](#scientific-runtime). It invokes the matching `<installation>/releases/<version>/bin/server`, privately obtains the restored database's migration-owner `DATABASE_URL`, saves one UUIDv7 using `uuidgen --time-v7`, and records the native receipt. Unknown outcomes reuse that record; each distinct database restore gets a new record. Reissue needed machine connections and inspect original remote tasks before resuming the same native durable state. Restoring only PostgreSQL is not complete recovery.

<a id="troubleshooting"></a>
## Troubleshooting

| Symptom | Action |
| --- | --- |
| GHCR pull denied | Check package visibility and the owner's Docker login |
| Missing manifest but existing Compose resources | Restore the original installation identity from backup; do not generate a new password/key for the old database |
| Worker ABI or unit verification fails | Fix host prerequisites/path before retrying; the installer checks the candidate before stopping an existing release |
| Research service fails or task status is unknown | Keep the same SQLite/config and inspect app logs/native tasks; do not create a replacement database or infer remote cancellation |
| Unfinished legacy Missions block cutover | Reconcile original remote outcomes using the original release before retrying; old homes and historical rows remain intact |
