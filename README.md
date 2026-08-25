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

Normal research has two recurring human actions: propose an Idea and approve/reject the system-recommended Candidate. Setup, data authorization, Codex authentication, Mandate/Universe/downstream/plugin configuration and incident response live under Administration.

## Stack

- Python 3.14, FastAPI, SQLAlchemy 2, PostgreSQL 18, Alembic
- PyArrow / Parquet, Polars, NumPy, Optuna, CVXPY
- Official `openai-codex` SDK with Codex App Server over stdio; mission-scoped MCP broker
- React 19, TypeScript, Vite, TanStack Query, ECharts
- Docker Compose for self-hosted operation

## Quick start

```bash
cp .env.example .env
# Set a strong PostgreSQL password.
# Set QUAZONAI_MASTER_KEY to base64 encoding of exactly 32 random bytes.

docker compose --env-file .env up --build
```

Open `http://127.0.0.1:8000`. The production image serves the React workbench and `/api/v1/*` from the same FastAPI origin on that port.

After startup, open **Administration → Runtime configuration** to configure the Codex model, optional custom OpenAI-compatible Base URL, optional API key, and Worker limits. These values are persisted in PostgreSQL instead of `.env`; the Codex API key is write-only in the Web/API surface and AES-GCM encrypted at rest by `QUAZONAI_MASTER_KEY`. Existing Codex login state in the persistent `codex-data` volume remains supported when no API key/custom provider is required. Runtime changes apply to newly claimed work without rebuilding the Compose stack.

Research Program creation persists a `READY` Mission and durable job. The finite Worker starts the official Codex App Server in an exclusive git worktree; only after App Server admission succeeds does the Mission transition to `RUNNING`. If Codex authentication is unavailable, the job and Mission fail explicitly instead of being left falsely Running.

Registering a Downstream System returns its Bearer service token once. Store that token in the downstream system's secret store; QuaZonai keeps only an AES-GCM encrypted-at-rest copy bound to that Downstream System. Claim, accept, reject, Candidate Package download, and feedback calls require that Bearer token.

## Agent Skill

[`skills/quazonai/`](skills/quazonai/) is the portable Agent Skills package for operating a running QuaZonai instance through the local `quazonai` CLI. Install the entire directory, not only `SKILL.md`, so the bundled command reference and workflows remain available.

Install the CLI from the repository root:

```bash
python -m pip install ./backend
quazonai --help
```

For a user-level Codex installation, copy or symlink the Skill directory:

```bash
mkdir -p "${HOME}/.agents/skills"
ln -sfn "$(pwd)/skills/quazonai" "${HOME}/.agents/skills/quazonai"
```

Then restart or reload the Agent client and ask it to perform a QuaZonai operation, such as “check QuaZonai readiness” or “show active research programs.” Codex clients that support explicit Skill invocation can use `$quazonai`.

The Skill assumes the Core API is running on a local loopback endpoint. It is an external operator workflow, not QuaZonai's built-in per-Mission Codex runtime.

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
- [`skills/quazonai/SKILL.md`](skills/quazonai/SKILL.md): self-contained external Agent workflow for the implemented CLI.

## Status

`codex/production-rebuild` is the implementation branch for the autonomous research architecture. It is release-ready only when the branch CI is green; documentation alone does not constitute implementation evidence.
