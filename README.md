# QuaZonai

QuaZonai is a single-user, self-hosted autonomous quantitative **research and portfolio construction** workbench. It uses OpenAI Codex App Server for finite research Missions, an independent sealed evaluator for promotion evidence, and downstream-neutral Candidate Packages for Paper/Live handoff.

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
- OpenAI Codex App Server 0.149.0 over stdio; mission-scoped MCP broker
- React 19, TypeScript, Vite, TanStack Query, ECharts
- Docker Compose for self-hosted operation

## Quick start

```bash
cp .env.example .env
# set strong PostgreSQL and downstream service credentials

docker compose --env-file .env up --build
```

Open `http://127.0.0.1:8000`.

The dedicated Agent Worker also needs a valid Codex login in its persistent `codex-home` volume. `RESEARCH_READY` remains false until the Agent Worker heartbeat reports both Codex presence and authenticated status.

## Verification

```bash
make test
make lint
make typecheck
make frontend
make build
make compose-smoke
```

GitHub Actions additionally runs PostgreSQL 18 row-lock/idempotency integration, PyArrow point-in-time/Sealed tests, Codex App Server schema generation, frontend typecheck/test/build, backend and agent image builds, and a core Compose smoke test.

## Source of truth

- [`DESIGN.md`](DESIGN.md): complete product/domain/technical source of truth.
- [`AGENTS.md`](AGENTS.md): development governance and hard boundaries.
- [`OPERATIONS.md`](OPERATIONS.md): user operating model.
- [`CLI.md`](CLI.md): CLI, Codex App Server and Mission Tool contracts.
- [`skills/quazonai/SKILL.md`](skills/quazonai/SKILL.md): thin external/operator Agent workflow.

## Status

`codex/production-rebuild` is the implementation branch for the autonomous research architecture. It is release-ready only when the branch CI is green; documentation alone does not constitute implementation evidence.
