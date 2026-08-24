# Frontend review record

## Reuse decisions

QuaZonai intentionally does not implement its own generic UI infrastructure:

- **Radix Themes** — accessible buttons, forms, dialogs, tabs, selects, switches and table presentation.
- **TanStack Query** — server-state cache, retry and mutation invalidation.
- **TanStack Table** — filtering, sorting and pagination for research/data/admin grids.
- **TradingView Lightweight Charts** — OHLC candlestick + volume rendering.
- **Apache ECharts** — research pulse and analytical time-series/metric visualization.
- **React Flow (`@xyflow/react`)** — Mission DAG, Alpha lineage and portfolio redundancy graphs.
- **Phosphor Icons** — one consistent icon family.
- **React Router** — application routing.

No custom table engine, candlestick renderer, graph canvas, dialog/focus trap, select widget, routing engine or query cache is maintained by QuaZonai.

## Product-boundary review

- Approval views are read-only except Approve/Reject and compatible downstream selection.
- No candidate member, weight, Mandate or evidence editing exists in Approval.
- Claimed Handoffs expose no stop/undeploy/close-position control.
- Hidden chain-of-thought is not represented; only structured events and evidence summaries are shown.
- UI displays backend state instead of deriving authoritative domain transitions locally.
- Data Source / downstream Administration is low-frequency and separate from normal research workflow.

## Taste / visual review

`design-taste-frontend` was used contextually. The skill explicitly says it is not a dashboard template, so its marketing-layout presets were not copied. Relevant rules applied:

- no AI-purple/blue-glow default aesthetic;
- one restrained jade/teal accent;
- Geist-style sans + mono numeric typography;
- no gratuitous glassmorphism or Bento-card repetition;
- CSS Grid for responsive structure;
- `100dvh`, mobile-safe navigation and safe-area padding;
- `focus-visible` and reduced-motion support;
- dense cockpit hierarchy appropriate for quantitative research;
- no emoji UI and no hand-drawn SVG icons.

## Test coverage

- Vitest + React Testing Library: DataTable behavior, Idea → Charter preview/start, Approval compatibility and decision request.
- Playwright: full shell navigation, Research detail + Lightweight Charts rendering, Mission graph rendering, Paper-only downstream selection, claimed-Handoff boundary, responsive mobile navigation.
- CI: clean dependency install, TypeScript typecheck, unit tests, production Vite build, Chromium E2E.

## Review outcome

No intentional UI path violates the QuaZonai ownership boundary in `DESIGN.md`. Remaining release risk is backend wire-shape drift: FastAPI/Pydantic remains the canonical wire contract, so frontend CI should be paired with backend OpenAPI/schema compatibility tests when the backend implementation branch is finalized.
