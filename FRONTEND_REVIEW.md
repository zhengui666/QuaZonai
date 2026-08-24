# QuaZonai Production Web / Integration Review

## Release status

**RELEASE-READY for the implemented QuaZonai Research Intelligence + Portfolio Construction scope.**

The previously recorded release blockers are closed:

- the FastAPI backend now implements the domain resources consumed by the production Web client;
- Playwright lifecycle tests run against real FastAPI + PostgreSQL rather than an HTTP mock harness;
- the legacy QuaZonai-owned deployment/risk/live-supervisor/trading control plane has been removed from the runtime and public API;
- an OpenAPI contract test requires the research/portfolio resources and rejects execution-owned API paths;
- PostgreSQL upgrades from the owned `0001_initial` lineage migrate through `0002_research_boundary`;
- PostgreSQL-specific FK ordering is covered by the backend integration suite;
- `frontend/package-lock.json` is committed and CI installs it with `npm ci`;
- production image build and the research-service Compose smoke test pass.

Installation-specific `RESEARCH_READY`, `PAPER_HANDOFF_READY`, and `LIVE_HANDOFF_READY` remain backend-authoritative preflight states; they are runtime facts, not repository release blockers.

## Architecture

### Web client

- React 19 + TypeScript + Vite.
- React Router for SPA routing.
- TanStack Query owns server state, retries, mutation invalidation, and polling.
- Radix Themes provides accessible interaction primitives.
- TanStack Table + TanStack Virtual provide dense data grids.
- TradingView Lightweight Charts provides financial time-series views.
- Apache ECharts provides analytical charts and matrices.
- React Flow provides Mission DAG and lineage/redundancy graphs.
- Phosphor Icons is the single icon family.
- Geist Variable + Geist Mono Variable are self-hosted through Fontsource packages.

### Backend

The public FastAPI workbench exposes the Research Intelligence / Portfolio Construction domain rather than an execution control plane. Core public resources include:

- Idea preview;
- Research Programs, Missions, and activity;
- Alpha Library;
- Portfolio Mandates, Programs, and immutable Candidates;
- Candidate Approval;
- Candidate Package / Handoff / Forward Evidence lifecycle;
- governed Data Sources, Datasets, Universes, Downstreams, Plugins, readiness, health, and SSE events.

Mutation endpoints use persisted idempotency semantics where applicable. Approval is state-checked and creates one immutable Candidate Package plus an Available Handoff atomically. Downstream-owned claim/accept/reject/package/feedback endpoints require downstream identity and are not exposed as QuaZonai Agent runtime-control tools.

## Product ownership boundary

QuaZonai does **not** own broker credentials, orders, fills, positions, accounts, NAV, TradingNode, execution risk, recovery, downstream stop/undeploy, or live execution runtime control.

The repository release gate enforces that boundary by checking that legacy deployment/risk/integration APIs, deployment/risk persistence models, live runner/supervisor files, the old native execution-risk crate, and Nautilus-owned runtime dependencies are absent.

The Web UI similarly exposes no buy/sell, order, position, close-position, stop/undeploy, broker, wallet, NAV, or downstream runtime-control surface.

## Approval / Handoff semantics

Candidate Approval exposes only governed human decision actions:

- Approve the immutable recommendation to a compatible logical downstream.
- Reject with a DESIGN-aligned reason code and optional note.

The Approval UI cannot change Alpha members, target weights, Mandate, Capital Context, evidence, policy, constraints, risk model, cost model, or capacity model.

A Handoff is revocable by QuaZonai only while it remains unclaimed. Once claimed, downstream ownership is explicit. Claim, accept/reject, package retrieval, and Forward Evidence feedback are downstream-authenticated API operations; QuaZonai does not convert them into execution controls.

## Real browser lifecycle verification

The Frontend GitHub Actions workflow starts a real PostgreSQL 18 service, runs Alembic, seeds deterministic **test-only** domain facts, starts real Uvicorn/FastAPI, installs the committed npm lock with `npm ci`, and then runs the browser suite.

Required Playwright lifecycle coverage passes against that real backend/database stack:

1. Idea → charter preview → create Research Program → Mission appears — **PASS**.
2. Candidate ready → Approve → immutable Candidate Package/Handoff becomes Available; no execution controls — **PASS**.
3. Admin Data Source creation → backend readiness refresh — **PASS**.

Production application code contains no fixture store and does not synthesize missing research/performance evidence. Missing performance, calibration, feature importance, exposure, correlation, or Forward Evidence renders explicit Loading/Empty/Error states.

## Contract and migration verification

Backend integration coverage includes:

- public mutation idempotency;
- Research Program + Mission creation/activity;
- Approval → Candidate Package → Handoff transaction;
- downstream identity isolation;
- claim/package/accept/feedback state transitions;
- governed Data Source → readiness transition;
- OpenAPI required-path and forbidden execution-path gate;
- current and previous owned Alembic revisions in preflight;
- credential and plugin parent/child FK ordering under PostgreSQL.

`0002_research_boundary` is intentionally irreversible because downgrading would recreate execution-owned state that is outside the current product boundary.

## Final verification matrix

### Frontend workflow

- PostgreSQL migration — PASS
- deterministic test-only seed — PASS
- real FastAPI startup — PASS
- `npm ci` from committed `frontend/package-lock.json` — PASS
- ESLint — PASS
- TypeScript — PASS
- Vitest / React Testing Library — PASS
- production Vite build — PASS
- Chromium install — PASS
- Playwright against real FastAPI/PostgreSQL — PASS

### Repository CI

- research ownership boundary gate — PASS
- product runtime install — PASS
- Python compile — PASS
- Ruff — PASS
- MyPy — PASS
- runtime import smoke — PASS
- database preflight + Alembic migration — PASS
- backend Pytest — PASS
- Docker Compose configuration — PASS
- production image build — PASS
- research-service Compose smoke — PASS

## Conclusion

The former frontend/backend integration, execution-ownership, migration, PostgreSQL semantics, and dependency-lock blockers have been resolved. The repository and production Web client are release-ready for the implemented QuaZonai Research Intelligence / Portfolio Construction scope, subject only to normal installation-specific readiness preflight at runtime.
