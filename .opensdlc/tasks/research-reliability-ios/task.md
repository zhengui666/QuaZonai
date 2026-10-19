# Research reliability, capacity and warm iOS workbench

<a id="task"></a>
## Task

Task ID: research-reliability-ios. Source: owner's six-part request in this session. Endpoint: implement, verify, obtain exact-head CI and GitHub Codex read-only review, merge to remote dev. Production deployment is separate. Main agent researches/designs; development uses openai/gpt-6.1-sol max; operations uses openai/gpt-6-luna low. No model substitution.

<a id="intent"></a>
## Intent

1. Reduce preventable scientific execution failures and show meaningful causes and remedies instead of a generic scientific_execution_failed.
2. Chinese metric presentation throughout the app, with a circled i explaining every displayed metric on hover, keyboard focus and touch.
3. Clear factor/portfolio titles; long hypotheses, goals and provenance are collapsed but recoverable.
4. Scientifically justified factor and portfolio capacity with explicit scope, assumptions and data limitations.
5. Subtle warm iOS-like UI, preserving workflows and accessibility.
6. Original pixel logo inspired by Zonai ancient technology and stock-market candles; consistent app mark and favicon.

Preserve original studies, trial accounting, isolation, stored money precision, APIs and user data. Do not convert actual failures into success or silently retry a new trial. No broker execution or new datasets authorized.

<a id="spec"></a>
## Requirements and design

### Reliability

Current projections.ts replaces arbitrary native error text with scientific_execution_failed. Correct this shared boundary with a concise diagnostic (stable category/code, Chinese cause and suggested action, operation/stage where known) without exposing host paths/credentials or model source. Preserve original private durable diagnostics. Classify historic records too where identifiable. Inspect shared factor/portfolio execution and fix concrete reproducible root causes, including cancellation/recovery classification if applicable; do not promise zero failures. Unknown exceptions remain honestly unknown, with task/trial identity for diagnosis.

### Capacity research and model

Inspected current apps/job/src/capacity.rs: the existing upper limit is min over target decisions/instruments of participation * last available BAR notional / max(abs(weight change), full exit weight). It is a conditional historical liquidity ceiling, not profitability, live depth or a confidence interval. Reusing a BAR across decisions must not replenish its liquidity budget; missing/reduced targets and exits must not disappear. Shared-account portfolio targets net member demand before capacity; never sum component limits. Pure factor scores have no standalone monetary capacity without a documented strategy mapping.

Primary literature consulted: Toth et al. (2011), [Anomalous price impact and the critical nature of liquidity](https://arxiv.org/abs/1105.1694), supports nonlinear impact and fragmented execution; Zarinelli et al. (2014), [Beyond the square root](https://arxiv.org/abs/1412.2152), shows participation and duration dependence and limitations of universal square-root fits. Full PDFs retrieved directly and relevant model/caveat/conclusion sections inspected. Toth §5 explicitly distinguishes its qualitative mechanism from fully quantitative calibrated predictions. Zarinelli equation (1), §5.4 and §6 distinguish order-size, duration and participation effects; even normalization by daily volume is an assumption, and its US-equity sample cannot calibrate Polymarket or another venue. These results do not calibrate this installation's markets.

Implement a defensible historical participation-capacity calculation, showing entry/rebalance and exit constraints, exact limiting capital/instrument/time, observation coverage, and explicitly assumed volume stress (baseline and reduced-liquidity scenarios). All capital arithmetic stays native decimal. Report the formula/method and missing economic inputs. Profitability capacity requires matched out-of-sample net returns at capital scales, market/venue-calibrated impact, spread/fees, timing and exit constraints: no arbitrary impact coefficient or invented confidence interval. Explicitly mark economic capacity unavailable when those inputs are absent. Preserve existing maximumCapital as the historical executable ceiling consumed by sizing, not an economic optimum. Add optional backward-compatible report diagnostics if needed. Unavailable capacity must identify concrete missing input/native failure, not catch-all text.

### Presentation

Reuse Ant Design, Phosphor and existing components. Central Chinese metric glossary covers native Nautilus statistics, IC, sample counts, capacity and account/equity summaries, with unknown-key fallback '未识别指标' and original key in disclosure rather than raw field as headline. Circled-i tooltips explain definition, units and limits; focus/click supported. Do not change underlying values or manufacture annualization.

Use short deterministic titles: explicit names first, otherwise first meaningful sentence with bounded display and full original content under disclosure. Existing identifiers remain secondary. Collapse hypotheses, long goals, model/provenance and assumptions by default; errors and key numeric results stay visible.

Warm ivory background (#f7f4ef family), near-white grouped surfaces, charcoal text, muted jade accent, subtle warm shadows and translucent navigation only; native Apple/system Chinese font stack and tabular figures. Rounded grouped controls, restrained motion with reduced-motion support, mobile navigation preserved. Pixel logo combines a stepped ancient-circuit loop/Q silhouette and three market candles; no copied game asset, no extra image dependency.

<a id="plan"></a>
## Implementation plan

Isolated integration branch feat/research-reliability-ios from origin/dev 0cadba28. Original dirty checkout is untouched.

Parallel packages:
- Reliability Sol: apps/research native execution/error paths, apps/service projections/science errors and focused tests; no Web or capacity implementation edits. Add diagnostic contract separately if needed, coordinate before touching shared index.ts.
- Capacity Sol: apps/job/src/capacity.rs, crates/contracts/src/capacity.rs, apps/service/src/capacity.ts, corresponding report DTO and tests. No account-sizing behavior changes, Web files or shared projection edits.
- UI Sol: apps/web sources, assets, tests only. Existing report fields first; main agent shares optional new fields after backend contract settles.
- Operations Luna: read-only installed failure/version diagnosis and delivery prerequisites; no restarts, deployment, database writes, study/account actions.

Each developer must reproduce a relevant defect before correcting, inspect callers, keep changes focused, run package checks and commit locally. Main integrates commits, checks cross-module behavior, directs independent review and operations delivery. Reuse installed dependency/native caches, never modify upstream source.

<a id="verification"></a>
## Verification

Implementation checks not run yet. All three developer session headers confirm openai/gpt-6.1-sol / max; operations confirms openai/gpt-6-luna / low. Initial native UI inventory is blocked: cua.getState reports DISPLAY/XAUTHORITY unset and no X11 connection, so no manual visual acceptance is claimed. Required: applicable make check-ts, check-web, existing browser interaction suite and visual artifact inspection; native science/core for capacity changes. Final-head CI and explicit clean Codex read-only review before dev merge. Real installed-model studies and production deployment remain separate from deterministic development checks.

Luna read-only installed CLI diagnosis: exact image revision is 0cadba282dc9508fdf26793b3c88dcda03bf6d53. Existing owner profile was mounted read-only into an ephemeral CLI-only container; no service restart, database mount/writer or study mutation. Of 13 listed studies, latest study 3427 has failed trials 7103/7119/7134, all masked as scientific_execution_failed; generation task 7146 completed and submission 3435 is unanswered/aborted. Other studies contain computed trials alongside generic failures and explicit strategy_market_not_allowed / strategy_instrument_not_allowed / heldout_already_exposed / native_request_required codes. Supported public diagnostics cannot expose the masked native causes, so no particular root cause is claimed for those historical generic failures. New projection diagnostics must classify identifiable original stored failures without altering their facts. GitHub CLI/auth is available at /home/zzy/.local/bin/gh.
