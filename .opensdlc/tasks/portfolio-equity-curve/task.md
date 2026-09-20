# Portfolio equity curve

## Intent and scope

Issue #96: display the adopted historical Alpha portfolio's simulated total equity inside its existing PORTFOLIO evaluation detail. Use Apache ECharts, its React adapter and the existing Ant Design interface. Reuse Nautilus total-equity observations, not target history, cash balances or reconstructed cumulative returns.

Baseline: `92f501ee7b3d1999e4ed397bd4d5825310a6f330`. The product contract is DESIGN.md, section “组合历史回测价值曲线”. DEVELOPMENT.md is absent; CONTRIBUTING.md contains the supported development and verification workflow.

## Ownership

The web assistant authors all source, tests, configuration and exact editing scripts. GitHub Actions executes native formatting, schema generation, dependency locking, builds and tests. GitHub Codex is a reviewer only. No credential-bearing account acceptance or local CodexPro execution is claimed.

## Design

- Rust contracts define an evaluation-bound response and inclusive nanosecond range query.
- Store reuses the published-evaluation authorization and terminal-attempt association, then checks the publication's native-report reference before reading the immutable Study result.
- The domain projection reuses the native simulation binding and Money parser. It sorts exact timestamps, rejects conflicting duplicates and retains exact decimals. It selects actual observations at native, UTC day, ISO week or UTC month resolution, bounded to 10,000 displayed points.
- The API exposes only equity metadata and observations. It holds the existing artifact capacity through native read/parse completion, even if the HTTP waiter disconnects. It does not create runs, evaluations, artifacts, qualification or account state.
- React uses the generated client and response validators, cancels obsolete requests and checks project/candidate/evaluation/run identity. ECharts handles rendering, zoom, tooltip positioning and resize. Ant Design supplies range controls, status and accessible raw observations. Theme colors come from application tokens.
- Scientific rejection or expired qualification is not missing historical evidence. Unsupported evaluation kinds, absent snapshots, unavailable simulations and failed reads remain distinguishable. No fake zeroes, smoothing, return reconstruction or cross-currency addition.

## Implementation plan

1. Define the contract in DESIGN and Rust types.
2. Add the native projection, immutable source lookup and read-only API.
3. Add the lazy chart, exact formatting, interval controls and observation table.
4. Connect the existing candidate evaluation screen and generate contracts and dependency lockfiles.
5. Run native, Store/HTTP, browser, contract, formatting and build checks.
6. Address findings on the same PR; obtain an explicit clean review of its current HEAD before merge.

## Verification matrix

| Layer | Evidence required |
| --- | --- |
| Native execution | Real Nautilus snapshots with fees, positions and intraday data compared with every projected amount |
| Domain | Exact decimals/nanoseconds, duplicates, order, zero/negative values, UTC boundaries, inclusive ranges, million-point bound |
| Store and HTTP | Original admission/publication, source reference and identity, non-PORTFOLIO rejection, read-only behavior, unavailable outcomes |
| Browser | ECharts canvas, raw precision, range/resolution, no fabricated data, wrong-run refusal, offline, keyboard, three viewport sizes and accessibility |
| Integration | Generated OpenAPI/TypeScript/validators, locked package dependencies, full current-HEAD CI |

Controlled protocol/browser fixtures are not real-market results. Native simulation tests exercise the actual upstream engine on explicitly synthetic market fixtures. Neither kind is represented as investment performance.

## Delivery

Implementation and verification are in progress. This document is not a PASS certificate. The PR and GitHub check/review records are the authoritative execution evidence. Completion requires a PR, all applicable checks passing for its latest HEAD, explicit clean `@codex review`, no unresolved review issues, merge and post-merge main checks. Never request GitHub Codex implementation or fixes.

No schema migration, parallel chart engine, account ledger, time-series service, benchmark feed, real trading control, secret store or new authentication mechanism is introduced.
