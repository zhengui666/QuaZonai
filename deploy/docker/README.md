# QuaZonai @QUAZONAI_VERSION@

The pinned Node 22.19 application serves the Web console and `/api/v3` at
container port 8080. The service owns the sole official Pi Durable writer and
scheduler; there is no host Worker, separate research server, Rust API or gateway.
A separate offline scientific image retains the real Rust/Wasm compiler and
Nautilus/Clarabel kernel. CLI archives include JS/dependencies and the official
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

`data/service.json` configures datasets, connections, extensions and the original
`research.database` path. `data/research/provider.env` is private: Node alone
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
resume the original release explicitly and reconcile them. Pending target,
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

## Scientific images and source tools

The application and scientific image reuse the exact same tested `job` output.
The scientific image is offline, accepts explicit immutable `/input` mounts and
writes only to `/output`; actual Docker stop is needed to stop its process.
No retired `runtime doctor/serve`, `worker`, `init-state` or Rust `migrate` command
is invoked. Source preparation remains in the app image at
`/opt/quazonai/operator/source_plugins.py` and uses its matching `source-tools`.
Call it in a disposable `docker run --rm --init --network none --read-only` with
explicit read-only input and writable output mounts. Configure an original
server-side native catalog in `data/service.json`; a successful compilation or
backtest is not independent evidence or permission to trade.
