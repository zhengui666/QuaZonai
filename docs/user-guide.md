# Personal hosting and recovery

<a id="audience"></a>
## Audience and prerequisites

This guide packages the real single-user application on a Linux x86_64 host using the existing Rust API, Worker, PostgreSQL/PGMQ and a native Caddy gateway. It does not run the synthetic preview. The separate scientific Runtime, native Codex bindings and licensed data remain necessary for real research.

**Release status:** this is a deployment candidate, not a statement that a production installation or the complete research/delivery/restore contract has passed. Check the [acceptance evidence](architecture/issue-62-execution.md#acceptance) before relying on results. Do not substitute process health, a mock peer or CI for that acceptance.

Prepare the [supported build environment](../CONTRIBUTING.md#set-up-a-checkout), PostgreSQL 18 with PGMQ 1.10.0, an application database identity distinct from the migration owner, and an HTTPS origin you control. Use the official Caddy package/service; the gateway regression uses Caddy 2.11.4. Do not disable the API's HTTPS/origin policy to make remote login work. Native Missions additionally require Linux cgroup v2, `/usr/bin/systemd-run`, `/usr/bin/prlimit`, `/usr/bin/systemctl`, and the service account's running systemd **user** manager with its real `XDG_RUNTIME_DIR`; see [Mission resource prerequisites](../OPERATIONS.md#mission-原生资源前置条件). A system service merely using `User=quazonai` does not establish that execution boundary.

<a id="workflow"></a>
## Install the real application

### 1. Build a reviewed revision

Select a revision whose required checks and review are accepted. Use a dedicated checkout, at its repository root, with no concurrent edits or branch switches. Preserve unfinished work in its existing checkout; do not reset, clean or stash it merely to install a release. Create the dedicated `quazonai` service account using the host's account-management tools, with its own group and home `/var/lib/quazonai`; inspect an existing account before reusing it.

Run this entire block in one dedicated shell. It rejects tracked, staged or untracked source changes before building and checks the same revision and worktree again before installation. An error stops the block without selecting a release. Ignored build output is not source evidence; these checks do not replace independent CI/review or make concurrent editing safe.

```sh
set -eu
worktree_state=$(git status --porcelain=v1 --untracked-files=all)
if [ -n "$worktree_state" ]; then
  printf '%s\n' 'Refusing release: checkout has uncommitted or untracked changes.' >&2
  exit 1
fi
revision=$(git rev-parse --verify HEAD)

rustup run 1.98.1 cargo build --locked --release --target-dir target -p server --bin server
npm --prefix apps/web ci --ignore-scripts --no-audit --no-fund
npm --prefix apps/web run build

built_revision=$(git rev-parse --verify HEAD)
worktree_state=$(git status --porcelain=v1 --untracked-files=all)
if [ "$built_revision" != "$revision" ] || [ -n "$worktree_state" ]; then
  printf '%s\n' 'Refusing release: source changed during the build.' >&2
  exit 1
fi
release="/opt/quazonai/releases/$revision"
sudo install -d -m 0755 /opt/quazonai/releases
sudo mkdir -m 0755 -- "$release"
sudo install -d -m 0755 "$release/bin" "$release/web"
sudo install -m 0755 target/release/server "$release/bin/server"
sudo cp -R apps/web/dist/. "$release/web/"
sudo chmod -R u=rwX,go=rX "$release/web"
printf 'Installed release: %s\n' "$release"
```

`target/release/server` is the shared native CLI/API/Worker executable; `apps/web/dist` is the public static web build. Neither `make demo-preview` nor `vite preview` is the hosted production server. Do not put the source checkout, `.env`, database dumps, Codex home or private state under a web root. The explicit directory modes allow the unprivileged services to traverse the release even when the administrator uses umask `077`. A release directory that already exists is rejected, not reused; inspect any incomplete installation before deciding how to handle it.

For a **first installation only**, in the same successful shell session, select the installed release:

```sh
sudo ln -sT -- "$release" /opt/quazonai/current
```

The command must fail when `current` already exists, whether it is a symlink, directory or file. It does not overwrite the old selection or create a link inside an old release. Use the upgrade procedure below for an existing installation. The selected release and public web files should be owned by the administrator, not the API's writable service account. A copied release is not an activated or verified service.

### 2. Prepare persistent state and database access

Follow [the native initial setup](../OPERATIONS.md#首次启动认证服务) for the database roles, explicit migration and local one-use bootstrap capability. Schema changes are never run automatically by the long-running services.

The state initializer requires a **new, nonexistent** state directory. Create its parent, not the state directory itself:

```sh
sudo install -d -o quazonai -g quazonai -m 0700 /var/lib/quazonai
sudo -u quazonai /opt/quazonai/current/bin/server init-state \
  --state-dir /var/lib/quazonai/state
sudo install -d -o root -g quazonai -m 0750 /etc/quazonai
sudo install -o root -g quazonai -m 0640 deploy/quazonai.env.example \
  /etc/quazonai/quazonai.env
sudoedit /etc/quazonai/quazonai.env
```

Run `init-state` only for a new installation. On an existing installation retain its keys, references and files; never delete the directory to make this command succeed. Back up the master key through the separate procedure in [data and keys](../OPERATIONS.md#数据和密钥).

Fill `DATABASE_URL` locally with the non-owner application identity and set `PUBLIC_URL` to the browser's exact HTTPS origin. The sample intentionally has no database password and cannot start unchanged. The unprivileged `quazonai` user manager must read this file. Keep it administrator-owned and readable by the dedicated group only (`root:quazonai`, mode 0640, directory 0750); root-only 0600 would prevent startup. Do not add unrelated users to that group. Do not `source` it: its syntax is systemd's `EnvironmentFile`, not a shell script.

Prepare Codex profiles, authorized data and the separate Runtime through [OPERATIONS](../OPERATIONS.md) and [the native Runtime guide](../runtimes/native/README.md). For native Missions, configure `CODEX_DEPLOYMENT`, `MISSION_API_ORIGIN` and `MISSION_WORKSPACES` together. The latter must be an existing private directory writable by `quazonai`. The deployment JSON and native Codex homes must be readable by that account, with credentials remaining outside the checkout and web root. Empty integration allowlists or absent Codex bindings do not mean research is ready.

### 3. Install process supervision and the same-origin gateway

These are **user units**, not system units. Install them in the global user-unit directory and enable them only for the dedicated account. Lingering starts that account's manager at boot and keeps it after logout; the manager supplies the correct runtime directory and creates Mission scopes within its delegated user hierarchy.

```sh
sudo install -d -m 0755 /etc/systemd/user
sudo install -m 0644 deploy/systemd/quazonai-api.service \
  deploy/systemd/quazonai-worker.service /etc/systemd/user/
sudo loginctl enable-linger quazonai
sudo systemctl start "user@$(id -u quazonai).service"
sudo systemctl --machine=quazonai@.host --user daemon-reload
sudo systemctl --machine=quazonai@.host --user enable --now \
  quazonai-api.service quazonai-worker.service
```

Use the same `--machine=quazonai@.host --user` selection for subsequent `status`, `stop`, `restart`, `reset-failed` or `disable` commands. Do not put these units in `/etc/systemd/system`, add `User=` to them, hard-code another user's runtime directory or run the Worker as root. Enabling a user unit globally is not required. An existing system-unit installation must be stopped deliberately before switching to prevent duplicate Workers; keep its state and database intact.

These services run the actual foreground commands, use native SIGTERM handling and retry failed startup every 15 seconds, including while a dependency is still starting. They do not initialize keys, migrate the database, grant permissions or manufacture research results. Inspect recurring failures rather than interpreting retries as recovery. An exhausted stop timeout can terminate local processes; it does not prove a remote task was cancelled. Use the existing Attempt/reconciliation procedure after interruption. Caddy remains its separate system service.

Copy [the gateway configuration](../deploy/Caddyfile) to a dedicated file such as `/etc/caddy/quazonai.caddy` and import it from the host's existing Caddyfile. Do not overwrite an unrelated site's configuration. In the installed copy, replace the default `quazonai.example.com` with the same origin host as `PUBLIC_URL`; the default upstream is `127.0.0.1:8080` and the web root is `/opt/quazonai/current/web`. Alternatively set the three `QUAZONAI_*` variables in the **Caddy service's** environment; the API's environment file is not automatically shared with Caddy.

Use the official package's validation and reload commands after checking its service paths:

```sh
sudo caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
sudo systemctl reload caddy
```

Caddy must be able to read only the public web files and obtain or use the host's HTTPS certificate. DNS, certificate issuance, ports and any existing proxy are properties of the selected host and must be verified there. This task does not provision or expose a host automatically.

The gateway preserves the original Host/Origin for the loopback HTTP API. `/api` and `/health` are exclusive backend routes: backend failures remain errors, not the web shell. Missing build files remain 404. Only browser GET/HEAD HTML navigation can fall back to `index.html`; static files revalidate so the existing prompted PWA update mechanism remains in control. Native proxy defaults provide SSE streaming without enabling command retries.

### 4. Verify the installation before using research results

Inspect `sudo systemctl --machine=quazonai@.host --user status quazonai-api quazonai-worker` and the relevant journal locally. Root can select the user-unit journal with `sudo journalctl _UID="$(id -u quazonai)" _SYSTEMD_USER_UNIT=quazonai-worker.service` (use the API unit name for API logs). Confirm a native bounded Mission can actually start under this same account; a running Worker without a usable user manager is insufficient. Open the configured HTTPS origin and complete the one-use initialization/TOTP flow from the native setup instructions. Never post capabilities, TOTP secrets, cookies, account tokens or unsanitized diagnostics into an Issue or chat.

Verify a persistent project through the **real** API and browser, restart the API and reopen it, then check the same project and history. Check login, trusted-device revocation, a Worker restart with a controlled task, SSE reconnect and a database-unavailable response. `/health/live` only proves the API process is listening; it does not certify database, Worker, Codex, data or Runtime readiness.

For research, follow the existing [research startup call chain](architecture.md#structure) and [operation instructions](../OPERATIONS.md): configure the real dependencies, freeze the Brief and input, start the bounded Cycle, observe its original Run, and inspect the resulting evidence. Qualification requires the independent evaluations defined in DESIGN. Target packages require their own approval and Claim/ACK evidence; QZ never places broker orders. Do not mark this installation production-accepted before the complete applicable [acceptance scenarios](architecture/issue-62-execution.md#acceptance), including recovery, have actually run.

<a id="configuration"></a>
## Configuration and ownership

| Location | Purpose | Writable by |
| --- | --- | --- |
| `/opt/quazonai/releases/<revision>` | Reviewed executable and built public files | Administrator during installation |
| `/opt/quazonai/current` | Selected release, not user data | Administrator |
| `/etc/quazonai/quazonai.env` | Local service configuration and application DB identity | Administrator; root:quazonai 0640, read by the dedicated user manager |
| `/var/lib/quazonai/state` | Native keys, private references and durable artifacts | `quazonai`; never a web root |
| Configured Codex home and Mission workspace | Native authentication/session/workspace state | Designated service account |
| PostgreSQL and separate Runtime | Domain records, queue and task recovery state | Their existing dedicated identities |

Caddy terminates HTTPS and serves files; it does not own application sessions or migrations. systemd supervises processes; it does not replace the durable research queue or remote Runtime journal. The service account must not be a database owner. Keep only the services needed by the existing architecture; there is no new proxy service implementation or orchestration engine.

<a id="recovery"></a>
## Upgrade, rollback and failures

**Upgrade:** prepare a second reviewed release, preserve the existing selection, stop the Worker and API using `sudo systemctl --machine=quazonai@.host --user stop quazonai-worker quazonai-api` in a controlled maintenance window, and take the database/artifact/key backups specified in [OPERATIONS](../OPERATIONS.md). Apply only explicitly reviewed migrations with the migration identity. Select the new release using an atomic symlink replacement, restart both services and repeat the real-host checks above. Allow the browser's prompted update or reload after checking outstanding commands. This is a bounded-downtime personal installation, not a claim of zero-downtime deployment.

**Rollback:** selecting the previous executable is only safe when it is compatible with the current schema, state and native protocols. A binary rollback does not undo a database migration or recover data. When restoration is required, use [the existing recovery instructions](../OPERATIONS.md), including access invalidation where specified, and verify the restored application before resuming the Worker. Rehearse this with disposable copies first; never delete a production volume or regenerate a master key as a repair step.

**Rendering failure:** the web root shows a generic recovery message rather than the exception contents. Reload requires confirmation because unsubmitted state can be lost. It neither cancels nor reissues previously sent commands. After recovery, inspect the original record or Run; an unknown result is not permission to create a new request. Rendering recovery cannot repair a failed JavaScript download, an event-handler/network error or a backend failure.

**Repeated 502/503 or startup failure:** inspect the original API/Worker error and required configuration locally. Confirm the upstream bind and the exact public origin; do not point the gateway at the synthetic preview. Do not weaken the origin policy, run the application with the migration identity or silently remove unavailable integrations. Preserve any uncertain-request identity for the existing reconciliation flow.

## Verify changes to the hosting boundary

```sh
node --test deploy/install.test.mjs
CADDY_BIN=/path/to/caddy node --test deploy/proxy.test.mjs
npm --prefix apps/web run test:e2e -- error-boundary.spec.ts
```

The installation regression executes the guide's build/install/selection control flow in a disposable Git checkout using real Git and GNU coreutils, but replaces compilation with explicitly synthetic files and does not use root. It tests directory modes, failure propagation, dirty-source rejection and existing-selection preservation, not compilation, account permissions or deployment. The real builds remain the responsibility of the existing CI/Web checks.

The browser command uses the existing Playwright servers and requires the built web assets, installed dependencies and Chromium described in [CONTRIBUTING](../CONTRIBUTING.md#verify-the-change). The gateway test launches real Caddy with temporary files and an explicitly synthetic HTTP peer; it does not authenticate or execute research. The [hosting workflow](../.github/workflows/deployment.yml) also parses the actual user-unit files with a syntax-only executable stand-in and checks a native user-service → bounded user-scope launch with `prlimit`. The latter uses `/usr/bin/true`, not QZ, Codex or an account; it verifies only the native process-launch prerequisite. A valid unit file and a usable user manager still do not prove the configured QZ services started. The existing Web workflow verifies TypeScript, the browser regressions and its real-API suite. None of these checks is a production deployment.

Implementation references: [Caddy SPA patterns](https://caddyserver.com/docs/caddyfile/patterns), [native proxy behavior](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy), [systemd services](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html), [React error boundaries](https://react.dev/reference/react/Component#catching-rendering-errors-with-an-error-boundary), [Git status](https://git-scm.com/docs/git-status), and [GNU target-directory behavior](https://www.gnu.org/software/coreutils/manual/html_node/Target-directory.html).
