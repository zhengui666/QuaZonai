# Structured research, factor library and account-aware capacity

<a id="task"></a>
## Task

**Task ID:** structured-research-capacity

**Source:** Owner request in the current Pi session: (1) replace prose-only research submission/display with a deeply researched structured experience, (2) separate single-factor library from factor-combination backtesting, (3) predict usable capital/capacity for both and automatically adapt paper/live allocation and related parameters to account total value, (4) expose existing partial-capital reservation/withdrawal functionality.

**Endpoint:** exact final-Head applicable CI and GitHub Codex read-only review resolved, merged into remote `dev`, then existing local deployment updated and observed. Owner explicitly authorized continuous execution without questions. This authorizes development/deployment, not an actual real-money withdrawal or brokerage order during validation.

**Role assignment:** Astra parent does research/design and product acceptance only. Exact `gpt-6.1-sol`, maximum supported reasoning, subagent does implementation/tests/code review corrections, never product planning or installed operations. Exact `gpt-6-luna`, low reasoning, subagent does GitHub delivery and installed operations, never product planning or code changes. No silent model fallback. Existing developer/project constraints and original data/identities remain.

<a id="intent"></a>
## Intent and investigation

Users need a readable research mandate, reusable atomic factors, a truthful combined backtest, understandable capital suitability, and account allocation that cannot spend money reserved for withdrawal. Extra controls alone, prompt prettification, relabeling research runs as factors, or showing static capacity numbers would not solve this.

Astra inspected contracts, research host/input/trials/plugins, service workbench/science/native/downstream/projections, Web research/portfolio/observations, scientific liquidity and capital-exit contracts. Initial checkout was behind current dev; implementation base is current remote dev `e936abf3e73211744cec89953bc1f693716c7cfe`, isolated worktree `.pi/worktrees/structured-research-capacity`, branch `feat/structured-research-capacity`. Re-read current-base files before editing; main checkout and other worktrees have unrelated changes.

Findings:
- `ResearchStart` is goal/model/dataset/trial count or native input. `ResearchInput` has goal plus opaque data/policy. Host configures/submits prose, whereas scientific trial facts are already durable. Existing final evaluation, exposure isolation, budgets, continuation/recovery and causal partition logic must stay intact.
- Current dev supports one-factor and multi-member `strategyMembers`, cross-market policies and historical reassessment. A research ID is not an atomic factor ID. Multi-member evidence cannot be attributed to each member as independent performance.
- `PortfolioStart` consumes research IDs and explicit weights; native blending and one Nautilus shared account are reusable. Do not add independent factor curves to manufacture portfolio equity.
- `NativeRollingBarLiquidityPolicyV1`, measured BAR notionals and native participation constraints already exist. They describe historical capacity under stated assumptions, not guaranteed future orderbook depth.
- Native `capital_exit` contracts/owner keep requested/reserved/released/verified/reconciled amounts, command/revision/account bindings, cancellation/reduction phases and retained reservations. Current v3 service only exposes targets/receipts/account snapshots; a native contract existing does not mean v3 withdrawal already works.
- Account refresh currently observes once; browser polling a cached list is not unattended account adaptation. Target expiry must not be extended merely to make old backtests deployable.

<a id="spec"></a>
## Product and contract design (Astra)

### 1. Research mandate, not a chat prompt

New research uses a typed, versioned research specification persisted as the authority alongside captured dataset/policy. A small deterministic adapter may render it for Pi; never parse model prose to recover the specification. Keep legacy goal-only research readable/replayable and original request identity comparisons intact; do not rewrite historical admissions.

Form sections (progressive disclosure, existing UI components):
1. **Research question:** short name, economic question, hypothesis/rationale and optional exclusions. The name drives lists; a large paragraph must not be the title. These are explicit separate fields, not one free-text prompt.
2. **Market and data:** registered dataset, supported market/instrument selection, recorded coverage/currency/source, supported signal input family and prediction/holding horizon where the existing native contract allows it. Show inherited native settings read-only when not configurable. Reject unsupported overrides; do not offer fake knobs. Do not accept browser catalog paths or change partitions to gain data.
3. **Evaluation:** explicit supported objective/metric and direction, validation/independent-assessment scheme, comparison baseline if actually supported, acceptance target where measurable. Bind to real metric keys and enforce thresholds in deterministic result projection/admission rather than trusting an LLM saying it passed. Clearly distinguish computed, criterion met, insufficient evidence and failed. Existing historical reassessment stays labeled as such, not untouched out-of-sample.
4. **Search and stop:** actual maximum attempts, permitted factor complexity/search scope supported by current tools, actual stop criteria, model and thinking settings. Show effective settings before submission. Do not implement speculative research controls unused by tools; lack of support must be explicit.
5. **Capital assumptions:** portfolio/account capital scenarios and explicit execution/participation assumptions for later capacity assessment; absence does not block raw factor research. A factor score has no capacity without a score-to-position/execution policy.

Submission preview presents the named sections. Preserve exact-body request replay after uncertain network outcomes. Validate the typed contract at HTTP/CLI and research boundaries; frozen original scientific inputs, selected model and budget remain the same across recovery/resume.

Detail page is a structured research record: mandate summary; data/period/partition and model; actual task/submission progress; trial table (hypothesis, original ID, state, validation metrics/errors); final selection and independent evidence; suitability/capacity status; links to factor/combination. Avoid raw JSON editors and opaque metric dumps as primary UI. Unknown values are unknown, not zero. Explanatory copy is short and contextual.

### 2. Factor library and portfolio backtests

Separate routes/navigation/command-palette destinations **因子库** and **组合回测** while preserving old links and existing research/settings/data/extensions/observations functionality.

Atomic factor identity binds original research ID + trial ID + member identity/index/model source; derive stable references from existing durable records instead of copying research state into a second database. One factor row is one model, not a research conversation or a multi-factor candidate. Show source, hypothesis, market/data, evidence status, validation/heldout metric provenance and capacity under the explicit strategy policy. A member of an evaluated combination is marked component-only evidence unless independently assessed; do not invent standalone metrics or automatically spend sealed exposure to create them. Existing single-factor selected records can use their existing independent evidence.

Combination creation selects atomic factor references and nonnegative explicit weights (positive selected members summing to one, with cash reserve handled separately), compatible market/currency/evaluation coverage and capital. Reuse native blend/simulation, preserve original model/policy/selection provenance and one shared cash account. Persist factor membership in new combinations; preserve legacy research-ID requests and reads. A one-factor backtest remains valid but is clearly a one-factor configuration, not a second factor-library object. Multi-member legacy research must not be double-counted as both its whole combination and each component.

Factor page: searchable list, evidence/capacity badges, compact detail and add-to-combination action. Use user-facing names/hypotheses and navigable source links in combination membership, not only `r8.t11.m0`-style internal IDs. Translate structured preview enums into the UI's Chinese labels. Hide the unchangeable single-member limit for single-factor scope; describe participation as成交量参与率 with a clear percent example and use a readable label for source freshness. Keep optional advanced capacity fields progressively disclosed. Combination page: members/weights, capital, execution assumptions, equity/drawdown and original metrics, capacity report and target weights. Preserve dark/light palette, Geist/Phosphor/current components; do not redesign branding or add marketing layouts/dependencies.

### 3. Capacity = conditional executable capital, not a promised return

Implement actual native scientific capacity assessment using existing catalog/notional/target data, exact decimal arithmetic and current native interfaces. Reuse measured BAR liquidity and simulation settings; extend the small Rust science interface where necessary rather than implementing a second numerical engine in TypeScript.

A capacity report contains:
- scope (atomic factor + explicit conversion policy, or final combined portfolio), currency, source/evidence/decision clocks, observation window and validity/freshness;
- assumptions: max participation, execution/rebalance horizon, fees/slippage basis actually measured/configured, instrument minimum-order/lot constraints where available;
- lower practical capital bound where measurable, maximum participation-constrained capital, binding instrument/constraint and unallocated cash;
- a short capital scenario table for the user's configured capital and a few comparable sizes: required turnover/order notional, participation, usable allocation, status/reason. Distinguish historical estimate from current account capacity; if impact model/depth is unavailable, do not invent after-cost profitability or confidence intervals.

Definition: at each original decision/asset, liquidity budget = eligible observed traded notional × allowed participation over the explicitly stated execution window. Required trade = capital × absolute target-weight change (or exact target notional minus current observed holdings for live adaptation). Capacity is the tightest supported constraint across relevant decisions/assets. For conservative initial deployment include initial establishment and disclose exit assumption. Zero demand is non-binding, not infinite proven capacity; missing/zero liquidity for nonzero demand is unavailable/zero feasible capacity as appropriate. The estimate must use available-at data before the decision, never future volume. Minimum order sizes bind low capital. Same-market factors share the combined net demand/liquidity budget; never sum standalone capacities. Fees and lot rounding preserve reserve/cash invariants. The model predicts conditional feasibility, not a profit guarantee.

Capacity assumptions are independent assessment inputs, not a prerequisite that must have been entered before the original research. Provide an explicit capacity-assessment form on factor detail and explicit assumptions on new combination/backtest (prefilled from recorded assumptions when available); legacy eligible factors can therefore be assessed without rerunning research or changing original admission. Reuse the same typed assumptions/native operation, retain/show the effective assumptions, and do not silently default scientific values. A combination's own explicitly chosen capacity assumptions govern its joint assessment even when its member research mandates used different scenarios. Missing conversion policy or actual market data remains unavailable. This prevents the capacity feature from becoming permanently unavailable for every existing or initially assumption-free factor.

Existing native limitations remain visible. Unsupported instrument/currency/depth, incomplete source, stale account/market or missing execution policy yield a reasoned unavailable result, never a default million-dollar cap. Report useful historical results even when live usage is unavailable.

### 4. Account-value-driven automatic adaptation

Paper/live account binding offers **按账户净值自动调整** with an explicit selected factor/combination and its supported capacity/execution policy. Existing bindings stay unchanged unless owner enables it. New user-selected automatic mode continuously recomputes on new authoritative account snapshots using official Pi Durable task lifecycle, including restart; not a browser-only calculator or `setInterval` shadow scheduler.

Separate research parameters from execution parameters. Recompute eligible allocation/member scale, target notionals/weights, cash reserve and feasible rebalance size within the preselected evidence-backed factor pool and configured policy. Do NOT silently mutate factor formulas, optimize on new heldout results, change predictive horizons, add leverage or switch strategies. The smallest correct allocation rule is proportional approved weights capped by joint capacity, with residual cash retained; if more elaborate optimization is already available use its original mandate, otherwise do not invent one. Display per-factor applied capital/scale and constrained/unused reasons so the requested factor/combination adjustment is observable.

Available strategy equity = current account equity minus outstanding withdrawal reservations minus explicit cash buffer (avoid double counting funds already withdrawn/reconciled in equity). Bound by verified account/market/execution capability and collective capacity. Handle equity increases/decreases, deposits/withdrawals, tiny accounts, reserve exceeding equity and unchanged repeated snapshots. Exact decimal strings for money, native solver/arithmetic for science, integer sequences/epochs for idempotency.

Capture input snapshot sequence/clock, capital-exit revision/reservation, factor set and capacity assumptions with each effective allocation. Only newer same-account/mode/currency snapshots update it. Old/conflicting/unknown snapshots and unsupported capability keep a visible blocked state, not a made-up executable result. No duplicate delivery on replay/restart; unknown POST outcome reconciles original identity. Account-wide reservations apply across multiple bindings to the same physical account.

Generate target-only adjustments through the existing downstream delivery contract only when the source target/forecast and capacity evidence are fresh and compatible. Historical expired targets must remain expired; no TTL refresh without a new actual evaluation/source. Lack of execution support can still show an account-sizing projection but must explicitly say not delivered. Never call a recommendation 'applied' or ACK 'traded'. Keep current target receipt/provenance visible, and show latest automatic-adjustment time, effective budget, cap, cash/reserved money, changed parameters and reason.

### 5. Partial capital reservation and extraction UI

In each supported paper/live account detail add **资金预留与提取** with total equity, trading allocation, requested/reserved amount, released cash, verified withdrawable amount and reconciled withdrawal. Read and adapt existing capital-exit contracts/state machine/owner via supported account/downstream interface, not a replacement brokerage implementation. If current downstream lacks the operation, expose the actual capability and reason and leave action unavailable; connect a real supported path where available, not only a permanent placeholder.

Implementation discovery resolved by Astra: no existing v3 native-owner HTTP bridge is present. Implement the complete optional capability-advertised downstream bridge using original CapitalExit V1 documents and document its concrete protocol; reuse native-owner contract/tests as well as transport tests. Do not recreate a broker or owner state machine. Installed live capability remains unavailable until an actual compatible downstream is observed. Show supplied existing reservations/progress even if initiation is unavailable; absent reservation evidence is unknown, not zero, for unattended executable allocation.

User flow: amount/currency → actual preview (remaining trading capacity, cancellations/reduction plan, fees/cost bound where supported) → explicit initiate → phase/progress and actions supported by native state (pause/resume/cancel/reconcile externally completed withdrawal). Server binds original observation/control revision/preview identity. The generic development authorization is not a request to initiate a real withdrawal now.

Funds reserved for an exit are excluded immediately from new allocation proposals. Pausing/cancelling an exit does not falsely release reserved funds or resume superseded targets. Follow existing native semantics. Submitted/accepted/requested is not frozen at broker or withdrawn; distinguish local reservation, downstream acknowledgement, released, verified withdrawable, external withdrawal and reconciled completion. Withdrawal is performed externally, not by QZ; expose reconciliation with evidence rather than an invented transfer button. Preserve native money/currency, physical-account scope, request IDs, uncertain-effect reconciliation and state transitions.

### Acceptance cases

1. Create a new structured study, inspect each field in HTTP persisted input, tool-visible mandate and UI; change a real supported constraint and observe enforcement. Exact replay succeeds, changed body conflicts; legacy study/recovery still works.
2. Research detail shows actual trial/evaluation facts with split status and provenance, not chat prose. Factor library has separate stable rows for single models; combo evidence is never falsely standalone.
3. Build a combination from chosen atomic factors; weights/model refs reach real native shared-account simulation and UI. Unsupported/incompatible evidence is explained.
4. Native capacity tests prove monotonic feasibility with increasing capital, binding per-asset constraint, joint overlap, current-holding delta, missing/stale/zero liquidity, exact decimals, tiny lots and currency mismatch. One real catalog native calculation is exercised; controlled protocol tests do not prove live market depth.
5. New account snapshot changes automatic factor/portfolio allocation and parameters, including reserve deduction. Repeated snapshot/restart does not duplicate target effects; stale inputs/expired targets are not delivered. Browser closed does not stop enabled unattended adjustment.
6. Supported capital-exit preview/start/status/actions render original state and reserve amounts and constrain automatic allocation. Unsupported downstream is explicitly unavailable. Unknown response is reconciled; pausing/cancelling does not spend reserved money. Do not test actual real-money withdrawals.
7. Desktop and narrow viewport interaction checks cover new navigation, structured forms/errors, factor selection, capacity table and account controls with keyboard/accessibility and truthful loading/empty/error states.
8. Exact final Head applicable CI and GitHub Codex explicit no actionable issues; remote dev merge verified; local deployment uses merged revision and original data/config, with active studies safely preserved and no replacement empty DB.

<a id="plan"></a>
## Ordered implementation (Astra)

1. Sol: inspect current-base contracts/callers and implement typed research mandate across `packages/contracts`, `apps/research/src/input.ts`, host/recovery/tool-facing input, service workbench/projections and CLI/request-intent. Preserve old input/replay and partition semantics. Add focused checks.
2. Sol: add stable atomic factor projections/endpoints; adapt portfolio admissions/native input to explicit factor refs; preserve legacy requests. Split Web routes, palette/commands and pages using current components; update existing service/CLI/Skill documentation.
3. Sol: implement capacity report and scenario calculation in existing native science/domain, connect factor/portfolio services and display. Reuse catalog/rolling liquidity/target policies. Verify real native math before labeling UI supported.
4. Sol: bridge existing capital-exit capabilities and original state through service/downstream; wire typed forms/status; connect reservations and account snapshot-based durable adaptation, including honest unsupported/freshness handling.
5. Sol: run relevant `make check-ts`, `make check-web`, Web interaction and scientific/core checks from project docs, fix connected callers and tests, update architecture/task evidence. Independent Sol code review uses a separate context; author fixes issues. No added framework, gate/checksum machinery, fake fixtures or extra scheduling engine.
6. Sol: ensure the existing explicit stop/resume workflow can continue an owner-stopped, unselected study after its original active submission becomes terminal from abort, preserving original conversation/input/model/trial budget and previous-stop evidence. Existing resume currently recognizes only `done` STOPPED or `unanswered/model_error`; verify actual official abort status with a focused test and minimally extend the existing recovery path if needed. Do not resume selected/final-assessed studies, exhausted budgets, active predecessors or unrelated failures. Replay and admission-gap recovery must remain exact. This connected lifecycle fix is needed for a reversible maintenance stop under the owner's explicit local-update request, not a new pause scheduler or updater bypass.
7. Luna: push/review/CI orchestration, obtain required exact-Head GitHub Codex read-only review, route fixes to Sol; merge only final tested Head into remote dev. Publish/update existing installation with approved runbook, preserve original data/provider/config and reconcile active work; observe actual revision/health/new API and UI availability. Actual installed updater stops the app then rejects any active durable task/submission, so do not bypass it. If study 3427 is still generation-only with no unresolved external/native effect, perform the now-tested explicit owner stop as a recorded maintenance action, verify terminal/idle, update, then explicitly resume the same study/predecessor using the supported recovery endpoint. Retain the original model/trials/budget and previous-stop record. If those preservation preconditions are not true, do not manufacture idle or cancel remote actions. Do not edit code.

Product ambiguities are resolved by the conditional-capacity, deterministic-approved-pool and target-only design above. A genuine unsupported external provider operation is visible, not simulated. Any implementation discovery that requires a material product change goes back to Astra, not to the user for routine confirmation.

<a id="verification"></a>
## Verification

Initial inventory: Luna confirmed exact model, clean new branch from `e936abf3`, healthy installed revision `e936abf3` at `/home/zzy/.local/share/quazonai-ts` with persistent original data. Subsequent supported CLI inspection confirmed study 3427 has a running generation, zero trials and a placed submission. Deployment must preserve and explicitly continue that original work. Merge and deployment of this task have not run.

### Local author implementation and verification

Sol implemented the approved structured mandate, deterministic criteria/search/stop enforcement, derived atomic library and native shared-account combinations, conditional native capacity/sizing, optional original-V1 capital bridge, explicit Durable automatic adjustment, and narrow owner-aborted continuation. The later usability clarification is included: eligible legacy factors accept read-only post-research assumptions, new combinations have independent policies, and member names/hypotheses/source links and Chinese preview labels are readable. Original research admissions are not rewritten.

Local commands and observed results:
- `make check-ts`: service 83, research 88, CLI 5 tests plus typechecks passed. The stopped-continuation file passes 14 cases; structured service coverage passes 4 cases. Actual installed CLI pack/schema/shared-validator checks also passed, including exact assessment file bytes and invalid participation rejection.
- `make check-web` and `npm --prefix apps/web run test:e2e`: typecheck, 11 unit tests, production build and 36 browser cases passed. Chromium exercised the disposable real HTTP service, light/dark, 320–1024 layouts, axe, exact values/replay and keyboard horizontal scrolling. Desktop and 320px factor-capacity, atomic-combination and capital-control screenshots were inspected. Existing bundle-size/Babel warnings remain.
- Matching serial native build, `CARGO_BUILD_JOBS=1 make check-science`, `make check-core QZ_JOB_BINARY="$PWD/target/structured-bin/job"`, and the explicit native-capital-owner command passed: 216 job-library tests plus retained contract/domain/integration suites; connected native/HTTP 7/7; native capital owner 8/8. Legacy assessment proves both unavailable stale observations and an eligible causal-catalog success without changing original admission/trials/evidence/exposure; joint combinations retain original curves/targets. Native unavailable reports retain requested capital amounts with null metrics, not invented results.
- `make check-deploy`: source Python 227/227, deploy Node 17 passing and one existing skipped optional-environment case, package Python 2/2, release Python 3/3. No container publication or installed-service action is implied.
- Whitespace and limited changed-Markdown relative-target checks passed. `lychee` is absent, so full `make check-links`, fragments and remote URLs remain unverified locally.

The current task-owned logs are `/tmp/qz-structured-native-final5.log`, `/tmp/qz-structured-web-final9.log`, `/tmp/qz-structured-ts-deploy-final7.log`, `/tmp/qz-structured-cli-package-final.log` and the final transaction-replay rerun `/tmp/qz-structured-tx-replay-final.log`. Earlier failing/moving-source logs are superseded, not acceptance evidence. The final transaction-return detachment also passed a fresh complete TypeScript chain and connected bridge case (1/1).

Cua inventory had no DISPLAY/XAUTHORITY and browser APIs were unavailable; Astra accepted actual Playwright continuation and owns providing a configured desktop to the fresh reviewer. No manual cua or installed compatible capital/automatic adapter verification is claimed. Controlled tests do not prove live depth, profit, paid-model quality or long-duration operation.

<a id="review"></a>
## Independent preliminary review

A separate exact `openai/gpt-6.1-sol` / `max` context reviewed the moving worktree read-only at 2026-10-19 03:02 +08:00. Report and reproduction scripts are available to the author at `/tmp/qz-structured-sol-review-result.md`, `/tmp/qz-structured-review-repros.mts` and `/tmp/qz-structured-review-replay.mts`. This is not final-Head acceptance. Six concrete findings require correction and regression checks:
1. Native account sizing must retain actual out-of-rebalance-scope holdings, not spend proceeds from hypothetical sales; otherwise reservations/cash buffer are violated.
2. Disabling automatic mode must stop new offers but preserve pending/uncertain allocation receipt reconciliation; refresh/new-binding admission must use the allocation receipt and physical-account scope.
3. Missing positions must remain unknown for executable account sizing, not become an empty holdings array.
4. A definitively rejected allocation must not become an effective unchanged target that suppresses all later valid offers.
5. Persist the original fully captured native capital-preview request before its first POST so a lost response/retry cannot change observation binding under one request ID.
6. Enforce the structured budget-or-selection stop rule at research_finish; model prose cannot terminate an unselected structured study with remaining budget. Explicit owner stop and legacy behavior remain distinct.

The approved owner-stop/resume maintenance addition and final native/browser checks were still pending at review time. Author corrections and exact final-Head re-review are required before Luna delivery.

### Author remediation (not independent acceptance)

All six findings now have connected corrections/regressions: retain actual scoped holdings; reconcile original uncertain allocation receipts through disable/stop/restart and read-only refresh; reject missing positions/valuations; deduplicate only against ACKed accepted economics/control/member context; capture the complete native preview before POST; and enforce the original structured stop rule. Further regressions corrected same-phase failed-GET journal replay and projection-versus-actual receipt confusion. Transaction outputs are detached before the Chord overlay settles. The verified official owner abort is `unanswered/aborted`; only the original unselected/unassessed `STOPPED/owner_stop` with remaining budget and its exact saved recovery intent can continue, preserving previous-stop history.

Astra must obtain fresh exact-commit independent review and product acceptance. These author checks do not relabel the preliminary moving-worktree review as final approval.

<a id="handoff"></a>
## Local implementation handoff

The implementation is confined to `feat/structured-research-capacity` in the approved worktree. The local commit and precise current evidence are recorded in the accompanying `/tmp/qz-structured-sol-result.md`; Git retains the task/source history. No push, merge, publication, installed-service restart, installed-study operation, brokerage order, real capital reservation or withdrawal was performed by Sol.

Next: Astra reviews the exact implementation commit and resolves any final findings with Sol. Luna then owns the required exact-Head remote CI/additional GitHub read-only evidence, merge into `dev` and approved installed maintenance under the original-data/study-3427 preservation plan above. Required unavailable checks and actual adapter capability must remain explicit; no empty replacement database, manufactured idle state or fictional financial success.
