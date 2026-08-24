# QuaZonai Frontend Production Review

## Review status

**Frontend artifact: RELEASE-READY against the `DESIGN.md` frontend/domain API contract.**

The React workbench is production-built, type-safe, lint-clean, unit-tested, browser-tested, responsive, accessibility-aware, and contains no trading/execution control surface. Production application code does not include fixture data or synthetic fallback performance.

**Whole-product integration: BLOCKED by backend contract drift.** The current FastAPI application mounted on this PR still exposes the legacy router set (`research`, `deployments`, `risk`, `strategies`, etc.) rather than the resource contract in `DESIGN.md` section 40 (`research-programs`, `alpha-library`, `portfolio-*`, `approvals`, `handoffs`, and related resources). The browser lifecycle tests therefore validate the frontend against the documented HTTP contract through a stateful Playwright contract harness; they are not evidence that the current legacy backend implements those endpoints.

This distinction is intentional. Frontend release readiness is not used to claim repository-wide DESIGN conformance.

## Architecture

- React 19 + TypeScript + Vite.
- React Router for SPA routing.
- TanStack Query owns server state, retries, mutation invalidation, and polling. No Redux server-state mirror exists.
- Radix Themes provides buttons, dialogs, selects, dropdown menus, tabs, switches, text fields, text areas, and accessibility/focus primitives.
- TanStack Table provides filtering, sorting, pagination, and column visibility.
- TanStack Virtual provides row virtualization for large table result sets.
- TradingView Lightweight Charts provides candlestick, line, area, volume, crosshair, and tooltip-capable financial series.
- Apache ECharts provides analytical bars, lines, calibration views, exposure charts, and correlation heatmaps.
- React Flow provides Mission DAG, Alpha lineage, and redundancy/common-source graphs.
- Phosphor Icons is the single icon family.
- Geist Variable + Geist Mono Variable are self-hosted through Fontsource packages.
- CSS variables and compact layout primitives provide the cockpit shell; no custom design-system Button/Dialog/Select or custom chart/table engine is maintained.

## Routes and pages

| Route | Surface | Production behavior |
| --- | --- | --- |
| `/` | Dashboard | Research Pulse; Active Programs; Running Missions; Alpha discovery/evaluation; Portfolio readiness; Approval/Handoff counts; Agent Worker, Codex, and data readiness; material SSE events. |
| `/ideas` | Idea Composer | Natural-language Idea, charter preview, overlap/clarification, Start Research. |
| `/research` | Research Observatory | Program table with sorting/filtering/pagination/column visibility/virtualization. |
| `/research/:id` | Research detail | Frozen Charter, Branch summaries, Mission DAG, experiment/evidence ledger, optional real OHLC context, structured Agent activity. |
| `/alpha` | Alpha Library | Alpha, Universe, Horizon, Role, Qualification, Health, Evidence; immutable detail navigation. |
| `/alpha/:id` | Alpha detail | Performance/benchmark, drawdown/degradation, calibration, feature importance, lineage, scope/evidence. Charts render only returned API evidence. |
| `/portfolio` | Portfolio Lab | Mandates, Portfolio Programs, read-only Alpha allocation, risk exposure, correlation matrix, portfolio/benchmark series when returned by the API. |
| `/portfolio/candidates/:id` | Candidate detail | Immutable members, target contribution/weight view, redundancy graph, frozen policy/risk/cost/capacity/constraint/evaluation contract. No editing. |
| `/approval` | Candidate Approval | One immutable recommendation per card; rationale, evidence, risk, cost, capacity, changes, Capital Context, compatible downstream selector, Approve/Reject only. |
| `/handoff` | Handoff Center | Available/Claimed/Accepted/Feedback states, contracts, unclaimed revoke boundary, optional API-provided Forward Evidence; no runtime stop/undeploy/order/position controls after claim. |
| `/admin` | Administration | Readiness, service health, Mandates, Data Sources, Datasets, Universes, read-only frozen Capital Context snapshots, Downstreams, Plugins. |

Legacy aliases `/alphas`, `/approvals`, and `/handoffs` remain route-compatible while canonical navigation uses `/alpha`, `/approval`, and `/handoff`.

## Server-state and API behavior

All production state is read from `/api/v1/*`. There is no production mock store and no static demo dataset.

Mutations use the documented backend resources and explicit expected state where applicable. TanStack Query invalidates dependent caches after writes; unit tests specifically verify Data Source invalidation of data/readiness/health and Approval invalidation of approvals/handoffs.

Charts parse optional structured evidence fields. Missing performance, calibration, feature importance, exposure, correlation, or Forward Evidence produces an explicit Empty State instead of fabricated values.

Capital Context is deliberately read-only in the current frontend. `DESIGN.md` does not define a public Capital Context mutation endpoint; the Administration surface therefore shows Capital Context frozen in Approval snapshots rather than inventing unsupported CRUD.

## Approval and execution boundary

Candidate Approval exposes only governed decision actions:

- Approve the immutable recommendation to a compatible logical downstream.
- Reject with a DESIGN-aligned reason code and optional note.

The page does not allow changing Alpha members, target weights, Mandate, Capital Context, evidence, policy, constraints, risk model, cost model, or capacity model.

The Handoff surface permits revocation only while the offer is unclaimed. Once claimed, the UI states that the downstream owns runtime behavior and provides no stop, undeploy, close-position, broker, order, fill, account, NAV, wallet, or TradingNode control.

## Design and accessibility review

- Dense technical-investor cockpit hierarchy; no marketing hero, Bento marketing grid, or landing-page layout.
- Neutral zinc/slate-like surfaces with one restrained emerald/jade accent.
- No AI-purple gradient and no gratuitous glassmorphism.
- Geist + Geist Mono numeric typography.
- Consistent Phosphor icon family; no emoji UI.
- Keyboard-focus styling via `:focus-visible`.
- Radix interaction primitives provide keyboard/focus/ARIA behavior for dialogs, selects, dropdowns, tabs, switches, and buttons.
- Tables expose labels and sorting state.
- Charts include accessible role/labels while visual hover/crosshair detail remains supplemental.
- `prefers-reduced-motion` disables nonessential animation.
- Desktop sidebar, compact mobile primary nav, and a Radix mobile overflow menu keep every workbench route reachable at narrow widths.

## Verification result

The final implementation has passed the repository GitHub Actions verification chain:

- `npm run lint` — PASS.
- `npm run typecheck` — PASS.
- `npm run test` — PASS, including component, page interaction, and TanStack Query mutation/cache-invalidation coverage.
- `npm run build` — PASS.
- `npm run e2e` — PASS.

Required Playwright lifecycle coverage:

1. Idea → charter preview → create Research Program → Mission appears — PASS.
2. Candidate ready → Approve → Handoff becomes Available; no execution controls — PASS.
3. Admin Data Source creation → readiness refresh — PASS.

Repository verification on the same implementation also passed Python compile/Ruff/MyPy, database preflight/migration, backend Pytest, Docker Compose configuration, production image build, Compose service smoke, and Rust format/clippy/test.

## Known limitations / release gates outside the frontend

### 1. Backend HTTP contract drift — blocking whole-product release

The current FastAPI app on this PR still mounts the legacy API architecture and does not implement the complete `DESIGN.md` section 40 resource set consumed by this frontend. Until the backend supplies those real resources and OpenAPI/schema compatibility is tested, the frontend cannot be called production-integrated with this branch's backend.

Recommended backend release gate:

- implement the documented Idea/Research Program/Mission/Alpha/Portfolio/Approval/Handoff/Admin resources;
- generate or validate the frontend wire types against FastAPI/Pydantic OpenAPI;
- run the three browser lifecycle flows against the real API/database rather than only the frontend contract harness.

### 2. Repository architecture drift — blocking repository-wide DESIGN conformance

The current repository still contains execution-oriented backend/runtime artifacts such as legacy deployment/risk/live-supervisor paths. This frontend neither links to nor controls them, but their presence means the repository as a whole should not be described as conforming to the stated QuaZonai non-execution ownership boundary until backend cleanup is completed.

### 3. Installation readiness remains environment-specific

A built frontend cannot make an installation `RESEARCH_READY`, `PAPER_HANDOFF_READY`, or `LIVE_HANDOFF_READY`. Those statuses remain backend-authoritative and depend on Codex authentication, governed data, evaluator availability, downstream registration, and installation preflight.

### 4. Dependency lock hardening

All direct frontend dependency versions are exact-pinned and CI materializes a deterministic npm lock artifact for the verified install. The generated lockfile is not currently committed in `frontend/`; committing that generated lock and switching CI to `npm ci` remains a reproducibility hardening item rather than a functional UI blocker.

## Conclusion

The `frontend/` implementation is suitable to release as the production QuaZonai Web client **against the documented new domain API contract**. It is not a demo and it does not depend on production fake data.

Do not mark the entire QuaZonai product/repository release-ready until the backend contract and execution-ownership drift above are resolved and the required browser flows pass against the real FastAPI/PostgreSQL stack.
