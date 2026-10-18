# Backtest performance chart

<a id="intent"></a>
## Intent

The owner requested deep investigation and a replacement for the tangled backtest equity chart (screenshot supplied in chat). Rebuild the existing consumer, retaining original shared-account evidence, decimal readouts, null gaps, history and observation behavior. No scientific-engine replacement, invented benchmark or deployment is requested. Delivery remains subject to final-head CI and independent review.

<a id="spec"></a>
## Investigation and design

- `nativeCurve` maps canonical Nautilus snapshots without chronological ordering; `equityCoordinates` and ECharts then connect that array in arrival order. A time axis does not reorder line vertices. The retained Rust `portfolio_equity_curve` already sorts clocks but this TS projection does not call it. This explains backward lines in the supplied screenshot; the actual private account data is not accessible without login.
- Existing persisted curves also need display-side ordering. Sort a copy, retain exact sub-millisecond ordering and same-clock observations, and never alter stored raw evidence. Invalid timestamps are disclosed, not plotted. Exact original rows remain available.
- Backtests use explicit original capital for cumulative equity change `(equity - capital) / capital`, not compounded daily returns or annualization. Observation accounts remain equity-only (cash flows are not known). All plotted/calculated values are display projections, not new authoritative metrics.
- Replace point clutter with an unsmoothed line, exact linked tooltip, keyboard-accessible return/equity control, synchronized drawdown and time zoom. Use existing ECharts/Ant Design and theme tokens. Missing values break the line; complete-history drawdown is unavailable when records are missing. Summary values cover the full recorded period, not the zoom window.
- References: [ECharts data sorting](https://echarts.apache.org/handbook/en/concepts/data-transform/), [axis pointer](https://echarts.apache.org/en/option.html#axisPointer), [data zoom](https://echarts.apache.org/en/option.html#dataZoom). Source of account semantics: repository architecture, native projection and retained Rust equity implementation.

<a id="plan"></a>
## Implementation plan

1. Add regression checks for unordered native snapshots, existing persisted curves, duplicate/sub-ms clocks, gaps, return and drawdown semantics; observe failures before changes.
2. Sort `apps/service/src/projections.ts` snapshots by exact native clock; update `apps/web/src/equity-values.ts`, `equity-plot.tsx`, `components.css` and the backtest caller in `portfolio.tsx`. Observation caller stays equity-only. No wire/schema/config/dependency changes.
3. Run scoped service checks, Web unit/types/build and existing browser suite with additional chart coverage; inspect generated desktop/mobile and theme images. Record unavailable checks accurately, then seek final-head CI/review under repository policy.

<a id="verification"></a>
## Verification

- Before fixes: the new Web chronological-coordinate regression failed with a negative 86,400,000 ms interval; the service regression returned the late snapshot first. Both now pass, including exact nanosecond and stable same-clock order.
- `make check-ts`: passed (79 service, 68 research, 4 CLI tests, plus type checks). No native scientific source changed.
- `make check-web`: passed (10 unit tests, type check and production build), repeated after final visual adjustments. Existing large-chunk build warnings remain; no dependencies were added.
- `npm --prefix apps/web run test:e2e`: 33/33 passed, including the existing disposable real HTTP service flow, missing-value handling, observation behavior and 320–1024 layouts. The focused chart test was repeated after subsequent tooltip/zoom assertions and final slider styling and passed. It verifies monotonic rendered x coordinates, exact chronological rows, mouse/keyboard view switching, exact tooltip, actual range drag/reset and both themes at 1440/390px.
- Actual chart-only images from test data: [dark desktop](assets/backtest-dark-desktop.png), [light mobile](assets/backtest-light-mobile.png). These are UI verification images, not the owner's account or profitability evidence.
- `git diff --check`: passed. `make check-links` could not run because `lychee` is not installed.
- `cua.getState()` could not connect to native desktop (DISPLAY/XAUTHORITY absent); no browser/app surfaces were listed. The installed service at port 18080 requires authentication; the owner's original account data and live screenshot reproduction remain unchecked. No installed service, database, scientific calculation, credentials or original records were changed.

<a id="review"></a>
## Review and delivery

[PR #214](https://github.com/zhengui666/QuaZonai/pull/214) targets `dev`. Initial head `e430056f` passed all hosted CI jobs, including science/core. Read-only GitHub Codex review found four concrete display-boundary issues; this follow-up aligns arbitrary decimal scales instead of importing the form's 18-digit limit, keeps the headline as an exact decimal string, exposes every co-located clock in the tooltip, and places invalid raw rows after sorted valid rows. Added unit/browser regressions cover each case. ECharts outer-bound auto adjustment is disabled on both grids so wide labels cannot shift their time axes independently. A failing long-scale regression was observed before this correction.

After correction: `make check-web` passed (11 unit tests, types and build); both focused browser checks passed, including three sub-ms clocks, >18-place decimals, unsafe-integer headline precision, and invalid mixed histories. Updated test-data screenshots are linked above. Final-head hosted checks and explicit clean re-review remain required before merge; no deployment is performed by this task.

<a id="handoff"></a>
## Workspace

Isolated worktree `/home/zzy/projects/QuaZonai-backtest-chart`, branch `fix/backtest-performance-chart`, based on current remote dev `60f369c4`. Original workspace and unrelated changes are untouched.
