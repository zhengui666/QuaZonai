# QuaZonai

QuaZonai is a single-user, self-hosted autonomous quantitative **research and portfolio construction** workbench. It uses the official OpenAI Codex App Server SDK for finite research Missions, an independent sealed evaluator for promotion evidence, and downstream-neutral Candidate Packages for Paper/Live handoff.

QuaZonai does **not** own broker credentials, orders, fills, positions, accounts, NAV, TradingNode, live execution, execution risk, heartbeat, recovery, or downstream stop/undeploy.

## Product loop

```text
Idea → frozen Research Charter → autonomous Mission DAG → Alpha qualification
→ independent Sealed Evaluation → Alpha Library → Portfolio Mandate
→ Portfolio Candidate → independent portfolio evaluation → human Paper/Live Approval
→ Candidate Package → Handoff Registry → independent downstream feedback
→ Forward Evidence → Degradation Monitoring → research wake-up
```

Normal research has two recurring human actions: propose an Idea and approve/reject the system-recommended Candidate. Setup, operator authentication, data authorization, Codex authentication, Mandate/Universe/downstream/plugin configuration and incident response live under Administration.

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

For any deployment that is not deliberately running with local-development authentication disabled, configure the single Operator in `.env`:

```bash
python - <<'PY'
import base64
import secrets

print("QUAZONAI_AUTH_TOTP_SECRET=" + base64.b32encode(secrets.token_bytes(20)).decode().rstrip("="))
print("QUAZONAI_AUTH_COOKIE_KEY=" + base64.b64encode(secrets.token_bytes(32)).decode())
print("QUAZONAI_API_TOKEN=" + secrets.token_urlsafe(32))
PY
```

Copy the generated values into `.env`, then set:

```dotenv
QUAZONAI_AUTH_USERNAME=operator
QUAZONAI_AUTH_PASSWORD=<strong password, at least 12 characters>
QUAZONAI_AUTH_TOTP_SECRET=<generated base32 setup key>
QUAZONAI_AUTH_COOKIE_KEY=<generated base64 key>
QUAZONAI_API_TOKEN=<generated machine token>
QUAZONAI_AUTH_PUBLIC_ORIGIN=http://127.0.0.1:8000
```

Add `QUAZONAI_AUTH_TOTP_SECRET` to Google Authenticator with **Enter a setup key**, account name `QuaZonai`, and **Time based** key type. The browser login then requires username, password, and the current 6-digit authenticator code.

`QUAZONAI_AUTH_PUBLIC_ORIGIN` must exactly match the browser origin, including scheme and non-default port. Production requires HTTPS, so a remotely exposed installation should normally set it to the externally trusted TLS origin, for example `https://quazonai.example.com`.

Start QuaZonai:

```bash
docker compose --env-file .env up --build
```

Open `http://127.0.0.1:8000` for the default local deployment. The production image serves the React workbench and `/api/v1/*` from the same FastAPI origin on that port.

### Operator 2FA and trusted browsers

QuaZonai V1 has one deployment Operator, not a multi-user/RBAC system. Normal browser sign-in requires the `.env` username/password plus an RFC 6238 TOTP code compatible with Google Authenticator.

The login form offers **Trust this browser**. When selected, the server stores a long-lived encrypted/authenticated HttpOnly cookie in that browser profile. Once the short session expires, a still-valid trusted-browser credential restores a new session without asking for either the password or TOTP code. The default trusted-browser lifetime is 30 days and can be changed with `QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS`; the short-session default is 12 hours and can be changed with `QUAZONAI_AUTH_SESSION_TTL_SECONDS`.

Only trust a browser profile you control. Signing out deletes both the current session and trusted-browser credential. Rotating `QUAZONAI_AUTH_COOKIE_KEY` invalidates every existing browser session and trusted-browser credential immediately. Rotating `QUAZONAI_AUTH_TOTP_SECRET` requires updating the authenticator entry. Rotating `QUAZONAI_API_TOKEN` invalidates the old CLI/automation credential.

In `production`, QuaZonai refuses to start unless the complete Operator authentication configuration is present and valid. In `development`/`test`, authentication is disabled only when the entire primary authentication group is absent; partially configured authentication is rejected instead of silently falling back to anonymous access.

After startup, open **Administration → Runtime configuration** to configure the Codex model, optional custom OpenAI-compatible Base URL, optional API key, and Worker limits. These values are persisted in PostgreSQL instead of `.env`; the Codex API key is write-only in the Web/API surface and AES-GCM encrypted at rest by `QUAZONAI_MASTER_KEY`. Existing Codex login state in the persistent `codex-data` volume remains supported when no API key/custom provider is required. Runtime changes apply to newly claimed work without rebuilding the Compose stack.

Research Program creation persists a `READY` Mission and durable job. The finite Worker starts the official Codex App Server in an exclusive git worktree; only after App Server admission succeeds does the Mission transition to `RUNNING`. If Codex authentication is unavailable, the job and Mission fail explicitly instead of being left falsely Running.

Registering a Downstream System returns its Bearer service token once. Store that token in the downstream system's secret store; QuaZonai keeps only an AES-GCM encrypted-at-rest copy bound to that Downstream System. Claim, accept, reject, Candidate Package download, and feedback calls require that Bearer token. These downstream credentials remain separate from Operator browser authentication and the CLI machine token.

## Agent Skill

[`skills/quazonai/`](skills/quazonai/) is the portable Agent Skills package for operating a running QuaZonai instance through the local `quazonai` CLI. Install the entire directory, not only `SKILL.md`, so the bundled command reference and workflows remain available.

Install the CLI from the repository root:

```bash
python -m pip install ./backend
export QUAZONAI_API_TOKEN='<same machine token configured for the API>'
quazonai --help
```

When Operator authentication is enabled, the CLI automatically sends `QUAZONAI_API_TOKEN` as its Bearer machine credential. It never reads the browser cookie, Operator password, or TOTP setup secret. The CLI remains loopback-only by design.

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

`codex/production-rebuild` is the implementation branch for the autonomous research architecture. It is release-ready only when the branch CI is green; documentation alone does not constitute implementation evidence.
