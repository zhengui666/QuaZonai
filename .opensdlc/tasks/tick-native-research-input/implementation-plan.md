# Tick-native research implementation proposal

Status: initial profile accepted by the owner; implementation and validation in progress. The owner requested the full development workflow, an isolated branch from remote `dev`, merge back after completion, and no project-board synchronization. Scope remains historical factor research, without release, live acquisition, real execution or full-corpus download.

## Verified baseline

Use remote `dev` at `1ddefc197414f0d2e35340d8d011ccfa26f996dd`, not the older `main` checkout. Current ownership is TypeScript service → research/Pi Durable → remote native job, with shared TypeScript HTTP contracts and retained Rust scientific contracts.

The configured [Hyperliquid profile](../../../runtimes/data/sources/hyperliquid.json) pins `ProbioticFarmer/hyperliquid-augmented-copytrading` at `064bc5ea76a6f0db8dd5c2b66f598e925857784d`, with MIT terms and attribution. Its single `trades` stream includes perpetual, spot and outcome paths. Selecting `--kind trades` currently includes outcomes; it cannot meet the requested scope alone.

The existing `market-history` importer already writes native TradeTicks without BARs when aggregation options are absent. Its exact decimal conversion, native precision, identity accounting, native write/readback and report publication must be reused. The [data plugin](../../../apps/service/src/providers/hyperliquid.ts) currently always requests BAR aggregation and validates BAR output, while returning `ready:false` pending research preparation.

The current factor-only pipeline is separate from optional portfolio simulation. Both registered `qz.factor-wasm` and `qz.factor-volume` use BAR features. A generic feature-model interpreter exists, but its current experiment consumer also produces targets and simulation; that consumer is not the tick-research entry point.

No current real archive path or local installation resource configuration has been supplied in this workspace. Prior source-guide readback is historical evidence, not a new conversion or fresh source verification.

## Accepted initial choices

### Sampling and horizon

Recommend one observation per instrument and complete equal-event-time cohort, with an explicit `EVENT_TIME_COHORT_V1` policy. Within the initial configured archive, availability is the declared event-time proxy and remains unverified. Do not create one arbitrarily ordered prediction per trade within a tied timestamp.

For a cohort at event time `t`, its aggregates use all and only its original trades. Previous-price and EMA feature state may also use earlier eligible cohorts inside the selected range. The prediction is evaluated after the current cohort, before any later cohort. Native IDs provide deterministic storage order only; they are not source chronology. Closing a cohort requires the selected input to satisfy the declared ordering contract, not an invented source sequence.

Use an explicit `FIXED_TRADE_COHORTS` horizon `H >= 1`, counted within the same instrument. The label is the return from the current cohort's quantity-weighted mean price to that of the H-th subsequent cohort. There is no inferred clock, padding, last-trade tie-break price or BAR horizon reinterpretation. Cohort reference prices are derived features, not materialized BARs or replacement source prices.

The selected event range is `[start,end)`. Cohort decisions and all feature/label-support reads stay inside that range and their permitted partitions. Initial warm-up uses selected preceding cohorts only. Incomplete final horizons remain missing; labels crossing a protected partition boundary are excluded. Features never receive future labels. Existing iterative/held-out clipping and exposure admission remain authoritative.

The first supported semantics are exploratory event-proxy cohorts for this archive. Requests for PIT-qualified tick admission fail without original availability evidence and supported causal availability semantics. Original receive clocks with out-of-order event chronology must not be silently forced into this proxy policy; unsupported chronology is rejected. A later availability-aware policy requires its own explicit semantics.

Alternative requiring a different implementation: explicit clock decisions with fixed-duration horizons and separately defined as-of/tie/empty-clock behavior. Neither policy is selected implicitly.

### Factor support

Keep all existing BAR factor registrations and their ABI unchanged. Neither current `qz.factor-wasm` nor `qz.factor-volume` is declared tick-capable merely because it accepts eight floats. Extend `qz.factor-wasm` with an explicit tick mode and named feature schema through the existing generic feature ABI v2; keep `qz.factor-volume`, ensemble, portfolio and simulation consumers BAR-only initially.

Recommend a versioned tick feature contract: current cohort quantity-weighted price, previous cohort quantity-weighted price, fast and slow cohort-price EMA, total quantity, trade count and elapsed event time from the previous cohort. EMA advances once per complete cohort through the official indicator API, with periods explicitly measured in cohorts. Exact original ticks remain unchanged; derived features use documented finite numerical conversion. Masks represent only explicitly defined derived missingness and warm-up. Invalid source identities, values, precision or nonfinite calculations fail explicitly. Derived quantity-weighted marks are statistical features, not executable prices or lossless source encodings.

The tick model must declare this feature contract, so a BAR model cannot be invoked with silently substituted slot meanings. Reuse the bounded Wasmi execution and the existing factor IC/partition reporting infrastructure. No new portfolio target, order or simulation support follows from tick factor registration.

### Initial resource envelope

Recommend a conservative first profile, subject to explicit owner acceptance rather than a claim about installation capacity:

- One active native conversion/study unit at a time within this tick profile through existing execution infrastructure. Existing BAR concurrency is unchanged.
- Retain the existing 100,000-record write batch convention; complete cohorts cannot be split to evade a bound. An oversized cohort fails with a resource report.
- Bound each conversion unit and study selection to 1,000,000 selected trades, with a 2 GiB native-process memory ceiling. There is no implicit sampling when a limit is exceeded.
- Set an explicit initial 10 GiB derived-output budget per original conversion/preparation operation. A preparation operation counts both research and independent catalog copies plus its published metadata; completed-unit resume verifies the existing operation instead of granting another output allowance. Separate manually selected units are separate explicit operations; no cross-request budget ledger or full-archive scheduler is added. Preserve originals and fail unpublished when an operation exceeds its configured budget. Larger/full-archive preparation still requires sufficient storage and bounded resumable units. These are limits, not measured full-archive capacity.
- Use immutable instrument/time-bounded conversion units and existing final import reports. Reuse a completed unit only after its original selection, source revision, native build, coverage and readback identity agree. Retry an interrupted/unpublished unit into a fresh directory; do not claim in-place checkpoint support from the native writer.

Deduplication must remain correct across unit boundaries and retries. The implementation must verify identity scope and source overlap using existing source/import metadata and storage, and reject an unsupported partitioning scheme instead of dropping checks to meet the memory ceiling. Full-archive operation is not accepted merely because one bounded unit succeeds.

## Implementation slices after acceptance

1. Add explicit native input and sampling/horizon contracts in shared TypeScript and Rust scientific sources. Preserve legacy versioned BAR interpretation. Reject mixed modalities and incompatible factors at admission, not after an expensive job.
2. Add generic explicit source stream/path selection to the existing acquisition/import conventions. Perpetual plus spot must be selectable from new downloads and existing mixed archives without downloading outcomes or modifying the original archive/profile history. Reports retain the exact selected paths, missing paths and pinned source revision.
3. Extend the Hyperliquid data plugin with explicit BAR/TradeTick mode. Tick conversion omits BAR flags and checks TradeTick output. Represent archive-loaded, native-prepared and scientific evidence separately, without reinterpreting `ready` as qualification.
4. Extend native catalog registration/preparation and direct tick queries using matching official high-precision builds and file-scoped decoding. Preserve the standard BAR executable and route high-precision tick execution through an explicitly configured companion executable on the same Worker protocol; bind execution receipts to both binary revisions. Enforce half-open event bounds separately from the catalog API's availability-time filtering. Preserve exact source values, IDs, times, currencies and definitions, and reject incompatible native formats. Missing required representation definitions fail; present current definitions with limited historical provenance may serve only as visibly limited representation metadata in exploratory price/quantity research. Do not consume their current fee/contract economics as historical facts or backdate their observation times.
5. Add the explicit tick cohort feature/evaluation branch to the factor-only path. Reuse trial accounting, budget, iterative partition clipping, final independent evaluation and the existing Pi Durable ledger. Preserve source/instrument/time lineage across BAR/TradeTick so switching modality or copying a catalog cannot reset exposure.
6. Surface modality, universe, sampling, horizon, exact coverage and evidence limitations in Settings, Research, CLI and results through the same service contract. Unsupported portfolio/simulation consumers fail explicitly.

## Verification and review plan

- Check backward compatibility with unchanged stored BAR requests, factor ABI, horizon semantics, native outputs and adjacent BAR/import tests.
- Verify native TradeTick write/readback with exact high-precision decimals, original IDs, instruments and timestamps; no BAR files may be required or created by tick mode.
- Verify perp+spot-only acquisition plans and offline mixed-archive selection exclude outcome paths. Missing selected paths fail; known empty intervals remain distinguishable from missing files.
- Use causal counterexamples: tied events across files/batches, a later price that would change an earlier feature if leaked, incomplete and cross-partition labels, repeated IDs/conflicts, future/later definitions, incompatible precision, changed-source resume and oversized cohorts.
- Run the same selection with varied batch boundaries/order and resumed units; compare original records, cohort features, labels, coverage and final reports. Failed or unpublished units cannot become prepared inputs.
- Test the shared HTTP/CLI/Web contract and real Durable restart/retry behavior. Controlled protocol tests establish orchestration behavior only; native tests establish actual computation/readback.
- Apply the current repository's TypeScript, Web, source, native/core and relevant browser checks to the delivered change, then obtain independent exact-Head review and applicable CI. Required clean GitHub Codex review precedes any remote `dev` merge. No deployment is included.
- A real configured-archive acceptance run requires its accessible original files/manifest, matching native executables and sufficient resources. If unavailable, report that acceptance as blocked; do not present synthetic catalogs, historical receipts or compilation as real Hyperliquid research evidence.

## Accepted scope and remaining delivery gates

The owner accepted the three choices and requested implementation, review and testing followed by merge back to remote `dev`. Code completion, passing software checks, real-source availability and scientific qualification remain separate facts. Final-Head CI and clean review precede merge. Release/deployment and full-corpus acquisition remain excluded.
