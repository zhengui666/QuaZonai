# QuaZonai @QUAZONAI_VERSION@

The pinned Node 22.19 application serves the Web console and `/api/v3` at
container port 8080. The service owns the sole official Pi Durable writer and
scheduler; there is no host Worker, separate research server, Rust API or gateway.
The application contains no Nautilus kernel or source-tool executable. Scientific
work connects to a separately deployed Nautilus HTTP instance; paper and live
connections select their own remote instances. CLI archives include JS/dependencies and the official
platform Node runtime, not SEA or a custom bundled executable.

## Install

Use the original non-root Linux x86_64 owner, Docker Engine/Compose 2.20+, Bash
4.4+, curl, tar, awk, GNU coreutils and util-linux. Host Node/npm, Python, Rust and
systemd user units are not required. The CLI also supports macOS Intel/Apple
Silicon and Windows x86_64. Downloads/builds never run on the installed host.

Download `install.sh` from this release, then:

```sh
bash install.sh --version @QUAZONAI_VERSION@
# Remote client only:
bash install.sh --version @QUAZONAI_VERSION@ --cli-only
```

An extracted deployment archive can instead run `bash deploy.sh`. The default
root is `$HOME/.local/share/quazonai`, Web is `http://localhost:8081`, and the
retained PostgreSQL archive port is `127.0.0.1:55432`. Initial `--directory`,
`--port` and `--database-port` select another root/ports. Docker restarts the
application with `unless-stopped`; SIGTERM drains admission and suspends the
original durable storage, not fictional remote cancellation.

## State and providers

`data/service.json` supplies initial `nautilus.backtest`, paper/live `connections`,
datasets, extensions and the original `research.database` path. The browser's
Settings page includes Research, Factors, Portfolios, Observations and Settings;
datasets and data plugins have their own Settings pages, and extensions are managed
in Settings. Instance tokens use a masked password field. Explicitly saved instance
settings live in private `data/instances.json` and override the initial file on restart,
including explicitly cleared connections. `data/research/provider.env` is private: Node alone
reads it using `--env-file`. Use official pi-ai provider environment names; never
put credentials in research inputs, model context, logs or source. Initial
password setup/login happens in the browser. The standalone installed CLI is
`$HOME/.local/bin/quazonai`; use `quazonai login --help`.

Configuration is created only when absent. An existing `data/research/config.json`
or `data/research.json` supplies its original database/modules/provider paths;
only exact former shipped module paths are adjusted. Missing original storage is
an error, not a successful fresh initialization. Private extensions are not
relocated. User paths need to remain available through the original data bind or
explicit `compose.override.yaml` mounts. PostgreSQL is retained for historical
read-only access; this installer does not run or alter its applied SQL ledgers.

## Update and recovery

From the installed `current/deployment` directory:

```sh
bash manage.sh status
bash update.sh @QUAZONAI_VERSION@
bash manage.sh restart
bash manage.sh backup             # leaves the application stopped
bash manage.sh start
bash manage.sh restore /absolute/installation/backups/RECOVERY_POINT
```

Pass `--directory /original/root` for a custom installation. Update stops the
original writer before idle inspection and backup. Unresolved tasks, queued
research submissions and pending/uncertain downstream receipts refuse update;
resume the original release explicitly and reconcile them. Before upgrading an
installation that previously used `jobBinary`, finish its local tasks, remove
that field from the private config, configure the remote backtest instance and
shared catalog paths, then update. Historical
results remain readable; a stored local executable is not silently treated as a
new remote execution. Pending target,
original config, SQLite/WAL, modules, provider env, artifacts, keys, journals,
PostgreSQL and backup data are preserved. Restart/restore reuse the original
receipt identities, not replacement requests. Restore is explicit and requires
the matching original owner, path, database identity and retained release bundle;
pre-restore state is retained in another stopped backup. Protect the entire backup
as it contains secrets and keys. External databases/modules need a separate
owner-managed recovery plan and are not silently omitted or relocated.

### Legacy Rust installations

Automatic schema-2/3 Rust/PG/Worker cutover is **not supported** by this bounded
release. New managers reject it before stopping a service, disabling a unit,
initializing auth/storage, migrating PostgreSQL or replacing original files.
Use the original bundle recorded in `installation.json` to close old admission,
drain/reconcile active science and downstream operations, and preserve its
original database, master key, journals and backups. A reviewed auth/archive/queue
cutover is required before adopting schema 4. A local process exit is not proof
of remote-stop. Do not delete the old Worker or initialize over existing state.

## Remote Nautilus instances

Configure the backtest endpoint independently from each paper/live account
connection. No remote endpoint is inferred from a former `jobBinary`, and a
missing endpoint does not fall back to local execution. The console can start
without one; scientific operations require it. In the browser, open Settings →
Instance connections, configure the backtest/research worker and paper/live account
connections, then save. Enter the token value in the masked password field. Saving configures new work and does not test
remote health or redirect existing tasks. Saved settings survive reload, restart and
the existing stopped data backup/restore. Concurrent edits require loading the latest
revision; uncertain saves reconcile the original update.

For initial file configuration, merge these fields
into the original private `data/service.json`:

```json
{
  "nautilus": {
    "backtest": {
      "id": "research-nautilus",
      "url": "https://research-nautilus.example.com",
      "token": ""
    }
  },
  "connections": [
    {
      "id": "paper-account", "name": "Paper Nautilus", "mode": "paper",
      "url": "https://paper-nautilus.example.com",
      "clientId": "quazonai-paper", "accountId": "paper-account",
      "token": ""
    },
    {
      "id": "live-account", "name": "Live Nautilus", "mode": "live",
      "url": "https://live-nautilus.example.com",
      "clientId": "quazonai-live", "accountId": "live-account",
      "token": ""
    }
  ]
}
```

Replace each empty token value with the instance token in this initial configuration.
The browser Settings page stores edits in `data/instances.json`. Use distinct
connection IDs for distinct account bindings. Existing
observations retain their original connection and receipt identity; changing a
configuration entry does not retarget their pending deliveries.

These URLs must expose the corresponding QuaZonai adapter protocol, not a raw
NautilusTrader Python/Rust library. The shipped remote image serves the scientific
HTTP protocol on port 8090. Paper/live endpoints use the existing target receipt,
account, capability and capital-control protocol. QuaZonai remains target-only;
it does not hold broker credentials or place real orders.

### Deploy the backtest instance separately

The release manifest's `runtime_image` identifies the matching remote Nautilus
image. The local application installer does not pull or start it. Run it on the
chosen execution host with a private environment file containing
`QZ_NAUTILUS_TOKEN`, and a writable directory for durable execution receipts:

```sh
# Example paths: substitute the original absolute paths on both hosts.
mkdir -p /srv/nautilus-state /srv/quazonai/data/catalogs
docker run --detach --name quazonai-nautilus \
  --user "$(id -u):$(id -g)" --read-only \
  --cap-drop ALL --security-opt no-new-privileges \
  --env-file /absolute/private/nautilus.env \
  --env 'QZ_NAUTILUS_CATALOG_ROOTS=["/srv/market-data","/srv/quazonai/data/catalogs"]' \
  --mount type=bind,source=/srv/market-data,target=/srv/market-data,readonly \
  --mount type=bind,source=/srv/quazonai/data/catalogs,target=/srv/quazonai/data/catalogs,readonly \
  --mount type=bind,source=/srv/nautilus-state,target=/data/nautilus \
  --tmpfs /tmp:rw,nosuid,nodev,size=128m \
  --publish 127.0.0.1:8090:8090 \
  REMOTE_NAUTILUS_IMAGE_FROM_RELEASE_MANIFEST
```

Expose the HTTP endpoint to QuaZonai through the chosen private network or HTTPS
proxy. `/health` reports process readiness; authenticated `/v1/capabilities`
reports its supported scientific commands. Configure the app with the address
reachable from its own container, not the app container's `localhost`.

The current dataset contract uses shared filesystems. Original registered
catalogs and the app's `<dataDir>/catalogs` frozen snapshots must be available on
both hosts **at the same absolute paths**, and mounted read-only in the worker.
Only these data directories need sharing; do not mount the app's database,
provider environment or entire private data root into the worker. Keep admitted
catalog revisions immutable. Small generated models and request objects travel
through HTTP, while catalog bytes are not automatically uploaded or copied to
an unrelated remote directory. A catalog path that exists only on one host is
not sufficient.

The remote worker owns its execution receipts under `QZ_NAUTILUS_DATA_DIR`
(default `/data/nautilus`). Preserve and back up this directory separately with
its matching catalogs and image. An application restart, HTTP disconnect or
local task abort does not prove remote cancellation; reconcile the original
execution identity. App backup/restore does not back up or restore remote hosts.

### Source preparation

The remote image contains the exact tested `job`, `source-tools` and retained
Rust/Wasm compiler. Operator helpers are at
`/opt/quazonai/operator/source_plugins.py` in that image. Invoke source preparation
with an explicit entrypoint, read-only inputs and a separate writable output:

```sh
docker run --rm --init --network none --read-only \
  --entrypoint /usr/bin/python3 REMOTE_NAUTILUS_IMAGE_FROM_RELEASE_MANIFEST \
  -E -s -B /opt/quazonai/operator/source_plugins.py plugins
```

For a native CLI operation use `--entrypoint /opt/quazonai/bin/job` on the remote
image; the application no longer has a `job` command. Source download operations
need their explicitly allowed network access. No retired `runtime doctor/serve`,
`worker`, `init-state` or Rust `migrate` command is invoked. A successful backtest
is not independent evidence or permission to trade.
