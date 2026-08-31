# QuaZonai

QuaZonai is a single-user, self-hosted AI quantitative research and governance **Control Plane**. It uses the official OpenAI Codex App Server SDK for finite Missions and a pinned remote NautilusTrader `1.231.0` runtime for canonical market-data catalogs, strategy execution, backtesting, matching, order/fill/position/PnL evidence and sealed evaluation.

The NautilusTrader instances run independently, typically on another host. QuaZonai calls them through a typed HTTP contract and does not import NautilusTrader in the Core API/worker image. QuaZonai still does **not** own broker credentials, real orders, fills, positions, accounts, NAV, TradingNode/LiveNode, execution risk, heartbeat, recovery, or downstream stop/undeploy.

## Product loop

```text
Idea → frozen Research Charter → autonomous Mission DAG → Alpha qualification
→ independent Sealed Evaluation → Alpha Library → Portfolio Mandate
→ Portfolio Candidate → independent portfolio evaluation → human Paper/Live Approval
→ Candidate Package → Handoff Registry → independent downstream feedback
→ Forward Evidence → Degradation Monitoring → research wake-up
```

Normal research has two recurring human actions: propose an Idea and approve/reject the system-recommended Candidate. Setup, optional operator authentication, data authorization, Codex authentication, Mandate/Universe/downstream/plugin configuration and incident response live under Administration.

## Stack

- Python 3.14, FastAPI, SQLAlchemy 2, PostgreSQL 18, Alembic
- RFC 6238 TOTP via PyOTP and AES-256-GCM browser-session protection
- PyArrow / Parquet, Polars, NumPy, Optuna, CVXPY
- Official `openai-codex` SDK with Codex App Server over stdio; mission-scoped MCP broker
- React 19, TypeScript, Vite, TanStack Query, ECharts
- Docker Compose for self-hosted operation

## Quick start

```bash
cp .env.example .env
# Set a strong PostgreSQL password.
# Set QUAZONAI_MASTER_KEY to base64 encoding of exactly 32 random bytes.
```

Operator authentication is explicitly opt-in in every environment. The default retains the existing direct-access behavior:

```dotenv
QUAZONAI_AUTH_ENABLED=false
```

A direct-access deployment should remain loopback-only or sit behind another access boundary you deliberately trust. Enable QuaZonai's own authentication before exposing the workbench through a network/TLS layer.

Generate the TOTP setup secret, browser-cookie key and CLI machine token:

```bash
python - <<'PY'
import base64
import secrets

print("QUAZONAI_AUTH_TOTP_SECRET=" + base64.b32encode(secrets.token_bytes(20)).decode().rstrip("="))
print("QUAZONAI_AUTH_COOKIE_KEY=" + base64.b64encode(secrets.token_bytes(32)).decode())
print("QUAZONAI_API_TOKEN=" + secrets.token_urlsafe(32))
PY
```

Generate `QUAZONAI_AUTH_COOKIE_KEY` independently from `QUAZONAI_MASTER_KEY`; QuaZonai rejects equal decoded 32-byte values. The generated `secrets.token_urlsafe(32)` machine token satisfies the accepted RFC 6750 Bearer `b64token` grammar. Do not add spaces, quotes, line breaks, or other characters to the value.

Copy the generated values into `.env`, then configure the complete authentication group:

```dotenv
QUAZONAI_AUTH_ENABLED=true
QUAZONAI_AUTH_TOTP_SECRET=<generated base32 setup key>
QUAZONAI_AUTH_COOKIE_KEY=<generated base64 key>
QUAZONAI_API_TOKEN=<generated machine token>
QUAZONAI_AUTH_PUBLIC_ORIGIN=http://127.0.0.1:8000
```

Add `QUAZONAI_AUTH_TOTP_SECRET` to Google Authenticator with **Enter a setup key**, account name `QuaZonai`, and **Time based** key type. The browser login then requires only the current 6-digit authenticator code.

`QUAZONAI_ENV` accepts only `development`, `test`, or `production` (case-insensitive and surrounding whitespace is ignored). `QUAZONAI_AUTH_PUBLIC_ORIGIN` is canonicalized with browser-origin semantics before comparison: scheme/host are lower-cased, Unicode hosts use IDNA ASCII, IPv6 is compressed/bracketed, default `:80`/`:443` ports are omitted, and non-default ports are retained. HTTPS origins automatically receive `Secure` browser cookies. When authentication is enabled in `production`, the origin must use HTTPS. A remotely exposed installation should normally set the externally trusted TLS origin, for example `https://quazonai.example.com`.

If a TLS reverse proxy or tunnel connects to the API, set `QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS` only to the exact direct proxy IP/CIDR seen by the API/container. `127.0.0.1/32` is appropriate only for a direct or host-network peer; Compose commonly sees a Docker bridge/gateway address, so configure the actual connecting address instead. Configure that proxy to append its observed peer to `X-Forwarded-For` or overwrite it with a verified client address. QuaZonai ignores forwarding headers from every other peer and falls back to the direct source for login throttling; do not use `/0` or a broad client network. The Compose command explicitly uses `--no-proxy-headers`; manual Uvicorn launches must use that flag too and must not set `FORWARDED_ALLOW_IPS` or pass `--proxy-headers`, so QuaZonai can verify the actual direct peer before parsing the header.

Start QuaZonai:

```bash
docker compose --env-file .env up --build
```

Open `http://127.0.0.1:8000` for the default local deployment. The production image serves the React workbench and `/api/v1/*` from the same FastAPI origin on that port.

### TOTP Operator Authentication and trusted browsers

When `QUAZONAI_AUTH_ENABLED=true`, QuaZonai V1 protects its Web/operator API with one deployment Operator. This is not a multi-user/RBAC system. Normal browser sign-in requires only an RFC 6238 TOTP code compatible with Google Authenticator. The operator identity is fixed to `local-operator`; browser username/password are not login factors or supported settings.

TOTP-only is single-factor authentication and is weaker against online guessing than password + TOTP. Internet-facing deployments should still use HTTPS, narrowly scoped trusted-proxy configuration, and deployment-level network access controls. Non-empty deprecated browser username/password environment settings fail startup closed when authentication is enabled. This migration also invalidates pre-v3 session/trusted-browser cookies, so upgraded browsers must enter one current TOTP once; no database migration or authenticator re-binding is required when the TOTP setup secret is unchanged.

The login form offers **Trust this browser**. When selected, the server stores a long-lived encrypted/authenticated HttpOnly cookie in that browser profile. Once the short session expires, a still-valid trusted-browser credential restores a new session without asking for another TOTP code. The default trusted-browser lifetime is 30 days and can be changed with `QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS`; the short-session default is 12 hours and can be changed with `QUAZONAI_AUTH_SESSION_TTL_SECONDS`.

Only trust a browser profile you control. Signing out deletes both the current session and trusted-browser credential, then leaves an `HttpOnly`/`SameSite=Strict` browser-local logout barrier plus a sealed local issuance epoch. A subsequent TOTP login clears the barrier but binds new cookies to that retained epoch, so a network-reordered stale login or automatic-renewal response cannot restore access. Authenticated logout also advances the current API process's global browser-cookie issuance generation; anonymous/public logout changes only its caller's local state and cannot block other browsers from logging in or renewing. Browser cookies are additionally bound to a fresh random API-runtime issuance epoch, so restarting the API intentionally invalidates every existing browser session and trusted-browser credential rather than letting a reset logout counter revive one. Rotating `QUAZONAI_AUTH_COOKIE_KEY` invalidates every existing browser session and trusted-browser credential immediately. Rotating `QUAZONAI_AUTH_TOTP_SECRET` requires updating the authenticator entry. Rotating `QUAZONAI_API_TOKEN` invalidates the old CLI/automation credential.

When `QUAZONAI_AUTH_ENABLED=false`, the Web/operator API preserves direct access and no login or logout controls are shown. When it is `true`, startup fails closed unless the complete authentication group is valid. Enabled production authentication additionally requires an HTTPS public origin; any HTTPS public origin automatically uses `Secure` cookies.

After startup, open **Administration → Runtime configuration** to configure the Codex model, optional custom OpenAI-compatible Base URL, optional API key, and Worker limits. These values are persisted in PostgreSQL instead of `.env`; the Codex API key is write-only in the Web/API surface and AES-GCM encrypted at rest by `QUAZONAI_MASTER_KEY`. Existing Codex login state in the persistent `codex-data` volume remains supported when no API key/custom provider is required. Runtime changes apply to newly claimed work without rebuilding the Compose stack.

Research Program creation persists a `READY` Mission and durable job. The finite Worker starts the official Codex App Server in an exclusive git worktree; only after App Server admission succeeds does the Mission transition to `RUNNING`. If Codex authentication is unavailable, the job and Mission fail explicitly instead of being left falsely Running.

Registering a Downstream System returns its Bearer service token once. Store that token in the downstream system's secret store; QuaZonai keeps only an AES-GCM encrypted-at-rest copy bound to that Downstream System. Claim, accept, reject, Candidate Package download, and feedback calls require that Bearer token. These downstream credentials remain separate from Operator browser authentication and the CLI machine token.

## Remote Nautilus runtime

Deploy the pinned reference service from [`deploy/Dockerfile.nautilus-runtime`](deploy/Dockerfile.nautilus-runtime). [`deploy/nautilus-runtime.compose.example.yml`](deploy/nautilus-runtime.compose.example.yml) demonstrates separate Research and Sealed instances plus a same-host narrow proxy on the stable `quazonai-core` network. Core Compose keeps the API off the runtime bridge and uses a narrow `nautilus-runtime-proxy` for the two runtime endpoints. Configure the Core deployment with independent endpoints and service tokens:

```dotenv
QUAZONAI_NAUTILUS_RUNTIME_URL=https://research-runtime.example
QUAZONAI_NAUTILUS_RUNTIME_TOKEN=<research-runtime-service-token>
QUAZONAI_NAUTILUS_SEALED_RUNTIME_URL=https://sealed-runtime.example
QUAZONAI_NAUTILUS_SEALED_RUNTIME_TOKEN=<sealed-runtime-service-token>
QUAZONAI_NAUTILUS_VERSION=1.231.0
QUAZONAI_NAUTILUS_CONTRACT_VERSION=2
```

These service tokens are not broker credentials. Keep them at the trusted Core deployment boundary; Codex Mission children never receive them. A governed Dataset Revision must have an immutable Nautilus Catalog binding before a Mission experiment can run. Discovery evidence enters the Search Ledger; Sealed evaluation uses a separate endpoint/catalog and returns controlled disclosure only. Approved output is a Nautilus-native Candidate Bundle.

PMXT Archive is available as the `quazonai-pmxt-archive` historical `DATA_CONNECTOR` plugin for Polymarket v2 and Kalshi. It supports either one fixed hourly Parquet URL plus one target `asset_id`/`market_ticker`, or a generic immutable `ArchiveManifest` for a bounded all-market history range. The manifest path probes the fixed PMXT URL space without bulk downloading and records missing/probe-error gaps; `POST /api/v1/quant-runtime/archive-manifests/{manifest_id}/materialize` then validates exact hourly coverage and known shard sizes before materializing one instrument and a bounded UTC slice into a new immutable Dataset Revision. Materialization uses bounded Parquet batches and isolated runtime memory limits. This path requires no PMXT API key and has no order or execution capability.

The generic plugin runner also uses a per-instance quota-backed staging tmpfs and bounded protocol output, so third-party imports cannot fill the immutable Catalog volume or unboundedly buffer runtime memory.

## Agent Skill

[`skills/quazonai/`](skills/quazonai/) is the portable Agent Skills package for operating a running QuaZonai instance through the local `quazonai` CLI. Install the entire directory, not only `SKILL.md`, so the bundled command reference and workflows remain available.

Install the CLI from the repository root:

```bash
python -m pip install ./backend
# Required only when QUAZONAI_AUTH_ENABLED=true:
export QUAZONAI_API_TOKEN='<same machine token configured for the API>'
quazonai --help
```

When Operator authentication is enabled, the CLI automatically sends `QUAZONAI_API_TOKEN` as its Bearer machine credential. The token must be 32–4096 RFC 6750 `b64token` ASCII characters; whitespace, CR/LF, control characters, non-ASCII, and other punctuation are rejected at API startup. The CLI never reads the browser cookie or TOTP setup secret and remains loopback-only by design.

For a user-level Codex installation, symlink the Skill directory into `$CODEX_HOME/skills`. When `CODEX_HOME` is unset, Codex defaults to `~/.codex`. The subshell safely replaces an existing symlink but refuses to overwrite or nest inside an existing real directory:

```bash
(
  set -eu
  CODEX_HOME="${CODEX_HOME:-${HOME}/.codex}"
  SKILL_SOURCE="$(pwd)/skills/quazonai"
  SKILL_DEST="${CODEX_HOME}/skills/quazonai"

  mkdir -p "${CODEX_HOME}/skills"
  if [ -L "${SKILL_DEST}" ]; then
    rm "${SKILL_DEST}"
  elif [ -e "${SKILL_DEST}" ]; then
    printf 'Refusing to replace existing directory or file: %s\n' "${SKILL_DEST}" >&2
    printf 'Move or remove it explicitly, then run this installer again.\n' >&2
    exit 1
  fi
  ln -s "${SKILL_SOURCE}" "${SKILL_DEST}"
)
```

Then restart or reload Codex and ask it to perform a QuaZonai operation, such as “check QuaZonai readiness” or “show active research programs.” Codex clients that support explicit Skill invocation can use `$quazonai`.

Other Agent Skills-compatible clients may use a different discovery directory, including repository-scoped locations. Follow that client's documentation rather than assuming Codex's user-level path.

The Skill assumes the Core API is running on a local loopback endpoint. It is an external operator workflow, not QuaZonai's built-in per-Mission Codex runtime. When used while the active working directory is inside a validated QuaZonai checkout, the Skill discovers that checkout with Git and defers to its `AGENTS.md`, `DESIGN.md`, `OPERATIONS.md`, and `CLI.md`; when installed standalone elsewhere, its bundled references provide the portable operating baseline. Candidate approval/rejection commands remain human-only and are never executed by an AI Agent.

## Verification

```bash
make test
make lint
make typecheck
make frontend
make build
make compose-smoke
```

GitHub Actions additionally runs PostgreSQL 18 row-lock/idempotency integration, Candidate Package Reference Runtime conformance, frontend typecheck/test/build, backend and production image builds, and a core Compose smoke test that verifies both API health and the served Web client.

## Source of truth

- [`DESIGN.md`](DESIGN.md): complete product/domain/technical source of truth.
- [`AGENTS.md`](AGENTS.md): development governance and hard boundaries.
- [`OPERATIONS.md`](OPERATIONS.md): user operating model.
- [`CLI.md`](CLI.md): CLI, Codex App Server and Mission Tool contracts.
- [`skills/quazonai/SKILL.md`](skills/quazonai/SKILL.md): portable external Agent workflow for the implemented CLI; defers to the repository sources of truth when available.

## Status

The Nautilus-first remote runtime architecture is implemented behind the independent runtime contract. It is release-ready only when CI, real Research/Sealed runtime tests, and independent review are green; documentation alone does not constitute implementation evidence.
