# QuaZonai

QuaZonai is a single-user, self-hosted research and portfolio-construction
control plane. Its current product path is:

```text
Idea Draft → frozen Charter → bounded Research Cycle / Mission DAG
→ PIT-valid Alpha signals → independent evaluation / qualification
→ deterministic multi-Alpha target weights → target-only Package
→ Paper Handoff → Forward Evidence → Live promotion or degradation wake
```

QuaZonai owns research facts, Alpha signals, target weights, Package,
Approval, Handoff and evidence. It does not own broker credentials, orders,
fills, positions, accounts, NAV, execution risk, or downstream runtime
control. A downstream independently consumes a target-only Package.

## Status

Issue #58 replaces the retired preview/single-Alpha workflow. The
published operator surface starts with `IdeaDraft`; direct Program creation and
execution-shaped artifacts are not supported paths.
Release readiness requires the fresh-install, persisted-facts E2E contract and
green independent review; this document is not evidence that those checks have
passed.

## Stack

- Python 3.14, FastAPI, SQLAlchemy 2, PostgreSQL 18, Alembic
- PyArrow / Parquet, NumPy, CVXPY
- Official `openai-codex` SDK with App Server stdio and mission-scoped MCP
- React, TypeScript, Vite, Docker Compose

## Quick start

```bash
cp .env.example .env
# Set a strong PostgreSQL password.
# Set QUAZONAI_MASTER_KEY to base64 encoding of exactly 32 random bytes.
```

Authentication is opt-in. A direct-access deployment must remain loopback-only
or behind an explicitly trusted access boundary:

```dotenv
QUAZONAI_AUTH_ENABLED=false
```

For a new authenticated installation, generate independent master and browser
cookie keys plus a CLI machine token:

```bash
python - <<'PY'
import base64
import secrets

print("QUAZONAI_MASTER_KEY=" + base64.b64encode(secrets.token_bytes(32)).decode())
print("QUAZONAI_AUTH_COOKIE_KEY=" + base64.b64encode(secrets.token_bytes(32)).decode())
print("QUAZONAI_API_TOKEN=" + secrets.token_urlsafe(32))
PY
```

Copy the generated values to `.env` without printing them in chat, logs, or
screenshots:

```dotenv
QUAZONAI_AUTH_ENABLED=true
QUAZONAI_MASTER_KEY=<generated base64 master key>
QUAZONAI_AUTH_COOKIE_KEY=<generated base64 key>
QUAZONAI_API_TOKEN=<generated machine token>
QUAZONAI_AUTH_PUBLIC_ORIGIN=http://127.0.0.1:8000
```

The browser setup is TOTP-only and binds the single `local-operator` on the
first trusted visit. Keep initial setup on loopback, VPN, SSH tunnel, or a
protected proxy. Do not use browser credentials from the CLI. A narrow trusted
proxy CIDR and HTTPS are required before public exposure.

Start the stack:

```bash
docker compose --env-file .env up --build
```

Then open `http://127.0.0.1:8000`. The Web client and `/api/v1/*` share that
origin in the default deployment.

## First research

Create and complete the Draft through the Web workbench, or use the local CLI:

```bash
python -m pip install ./backend
# Required only when QUAZONAI_AUTH_ENABLED=true:
export QUAZONAI_API_TOKEN='<machine token configured for the API>'

quazonai idea create --text "Research liquid equities with point-in-time data"
quazonai idea show <DRAFT_ID>
quazonai idea answer <DRAFT_ID> \
  --expected-revision <REVISION> \
  --answer market_scope="US equities" \
  --answer horizon="one day" \
  --answer data_scope="approved discovery data"
quazonai idea start <DRAFT_ID> --expected-revision <REVISION>
quazonai research graph <PROGRAM_ID>
```

The returned Draft determines the clarification keys. Do not invent
optimizer/model/weight questions, submit a direct Program request, or retry a
write after uncertainty without reading the current resource first.

## Agent Skill

[`skills/quazonai/`](skills/quazonai/) is the portable operator Skill. Install
the entire directory so its authenticated CLI reference and workflows travel
together.

For a user-level Codex installation, symlink it into `$CODEX_HOME/skills`:

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

The Skill may prepare an approval or rejection command, but a human executes
that capital-allocation decision. It never controls a downstream runtime.

## Verification

```bash
make ci
```

Targeted checks should include the affected unit/integration tests, fresh
Alembic migration from an empty database, parser/documentation contract, and
the security/isolation boundary relevant to the change.

## Source of truth

- [DESIGN.md](DESIGN.md): complete product, domain and technical facts.
- [AGENTS.md](AGENTS.md): engineering governance and hard boundaries.
- [OPERATIONS.md](OPERATIONS.md): operator journey.
- [CLI.md](CLI.md): CLI, App Server and MCP contract.
- [skills/quazonai/SKILL.md](skills/quazonai/SKILL.md): portable external
  operator workflow.

## Mobile Web / PWA

Desktop, mobile Web and installed PWA use one responsive client. The PWA
precaches only the static shell; `/api/**` is NetworkOnly and never fabricates
cached research or authentication data. An update needs explicit confirmation;
offline mode states that server data and mutations are unavailable.
