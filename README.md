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
# Set OPENAI_API_KEY, or provision a Codex login in the persistent codex-data volume.

docker compose --env-file .env up --build
```

Open `http://127.0.0.1:8000`. The production image serves the React workbench and `/api/v1/*` from the same FastAPI origin on that port.

Research Program creation persists a `READY` Mission and durable job. The finite Worker starts the official Codex App Server in an exclusive git worktree; only after App Server admission succeeds does the Mission transition to `RUNNING`. If Codex authentication is unavailable, the job and Mission fail explicitly instead of being left falsely Running.

Registering a Downstream System returns its Bearer service token once. Store that token in the downstream system's secret store; QuaZonai keeps only an AES-GCM encrypted-at-rest copy bound to that Downstream System. Claim, accept, reject, Candidate Package download, and feedback calls require that Bearer token.

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
- [`skills/quazonai/SKILL.md`](skills/quazonai/SKILL.md): thin external/operator Agent workflow.

## Status

`codex/production-rebuild` is the implementation branch for the autonomous research architecture. It is release-ready only when the branch CI is green; documentation alone does not constitute implementation evidence.
