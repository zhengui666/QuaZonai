# Tick-native historical research input

<a id="task"></a>
## Task

Task ID: `tick-native-research-input`.

Delivery PR: [#243](https://github.com/zhengui666/QuaZonai/pull/243), targeting remote `dev`. Final CI, review and merge receipts are retained on that PR.

Define what QuaZonai should become so historical research can use the configured Hyperliquid trade archive without requiring prebuilt BARs.

Original endpoint: requirements proposal for owner review, without implementation, data conversion or release. The owner subsequently requested the full research, design, development, review and testing workflow and confirmed continuation without project-board synchronization. Release/deployment and full-corpus acquisition remain outside the task.

The owner accepted the complete initial sampling, factor and resource profile, requested the most general implementation, and explicitly required an isolated branch from remote `dev` followed by merge back after development. The [implementation plan](implementation-plan.md) records that profile. Development baseline: remote `dev` at `1ddefc197414f0d2e35340d8d011ccfa26f996dd`, isolated branch `feat/tick-native-research-input`.

Current status: runtime implementation, local native/connected validation and independent source review are complete. Remote final-Head CI, clean GitHub Codex review and the authorized merge are not yet complete. No release or deployment is included.

<a id="intent"></a>
## Intent

QuaZonai currently admits native BAR catalogs to its scientific research interface. The configured Hyperliquid archive contains raw spot and perpetual trades, and NautilusTrader supports native `TradeTick` data. Requiring BAR materialization before every study discards event detail and makes BAR construction an unnecessary prerequisite for tick-native research.

Support two explicit historical research inputs: the existing BAR mode and a native trade-event mode. A user must be able to run a supported study over the original trade events through a registered native TradeTick catalog without first creating a BAR catalog. Existing BAR studies and their contracts retain their meaning.

Initial source scope is the already configured Hyperliquid perpetual and spot trade archive. Outcome contracts, order-book/L2 research, funding settlement simulation, live data and real order execution are excluded.

<a id="spec"></a>
## Requirements

### Expected behavior and observable acceptance

1. **Explicit study input.** New study requests identify native modality (`BAR` or `TradeTick`), registered catalog selection, instrument universe, half-open time range and sampling/target horizon. Service, CLI and Web submit the same contract. No caller infers modality from a catalog path or its contents. Existing versioned BAR requests without a modality retain their historical BAR interpretation.
2. **Direct native tick input.** A registered native TradeTick catalog can be selected and queried directly by the native job. A tick-mode study requires no materialized BARs. Conversion uses the original archive as provenance and writes a separate derived native catalog without overwriting the source or existing catalogs.
3. **Explicit causal semantics.** Each supported tick study defines event ordering, decision cutoff, same-timestamp cohort behavior, feature eligibility and forward-label boundaries. Features cannot observe trades beyond their decision cutoff. Deterministic tie-breaking does not establish trustworthy source sequence or availability. Existing BAR factor behavior and requests remain unchanged.
4. **Exact values and integrity accounting.** TradeTick import and evaluation preserve exact source price, quantity, trade identity, instrument identity and source timestamps. Identity scope is explicit. Missing identities, duplicate/conflicting IDs, rejected rows and uncovered intervals appear in incomplete or rejected reports. No value is rounded, filled or silently dropped to make the input acceptable. Unsupported native precision fails with a specific report. This requirement does not change existing BAR import contracts.
5. **Availability evidence.** Event time and original receive/availability time remain distinct. Missing original availability is represented as an explicitly unverified event-time proxy. Such data can support explicitly unqualified exploratory analysis; it cannot be described as PIT-qualified or independently validated on that basis. Import/conversion time and catalog registration cannot supply missing original availability evidence.
6. **Separate operational and scientific state.** The product distinguishes source archive loaded, native research input prepared and scientific qualification. Mechanical readiness may permit an exploratory run. It never establishes PIT, historical contract/fee correctness, market completeness, profitability or independent evidence. The generic ready flag retains its operational meaning.
7. **Hyperliquid selection and report.** The configured spot/perpetual trade scope can be converted and studied without downloading outcome records or constructing a BAR catalog. Results identify the pinned source revision, selected streams, exact instrument/time coverage, missing paths and definition/availability limitations. They also identify the derived catalog revision, conversion/build format and applicable sampling, feature and target semantics so the selected input is reproducible.
8. **Bounded and resumable operation.** Native import and evaluation use bounded batches over the selected instrument/time range and existing catalog/import conventions. They retain stable identities and deterministic behavior across batch boundaries and resumed work. Changed source revisions or incompatible resume settings fail explicitly; partial preparation cannot appear complete. Use official Nautilus APIs and existing project contracts, storage and execution infrastructure. No copied upstream implementation, alternate scheduler or replacement research ledger is introduced.

### Existing flow and intended change

Keep the BAR catalog reader, BAR factor evaluation, partitioning, independent evaluation, trial ledger and research APIs compatible. Preserve the existing BAR feature ABI, horizon meaning and simulation behavior. Selecting TradeTick research input does not by itself extend portfolio simulation support. Add an explicit TradeTick selection through the existing service → research → native job boundary, using Nautilus catalog queries and the existing source importer.

Extend science requests and native selections from a BAR-only assumption to tagged native input. Tick-aware feature and target semantics must be explicit. Existing bar-based factors are not silently reinterpreted as per-trade factors.

Settings and Research must expose eligible tick catalogs and their modality, selected universe, coverage and preparation limitations. Results must retain the submitted modality and coverage. Reuse current report/metadata patterns to represent operational preparation and scientific evidence separately.

### Compatibility, failure and data semantics

- Existing BAR datasets, requests, stored studies and results remain readable and retain their current meaning. Compatibility does not depend on guessing a catalog's modality.
- Mixed BAR and TradeTick inputs in one study are initially unsupported and fail clearly. There is no implicit join, resampling or conversion.
- Equal event timestamps are processed as one cohort unless the source supplies a trustworthy sequence within the relevant scope. File order, batch size and resumption cannot change cohort semantics. A sequence for one stream does not establish chronology across unrelated streams.
- Availability evidence governs eligibility at a decision cutoff. Neither sorting by event time nor a stable source ID permits a study to observe a trade before its evidenced availability. Event-time proxy studies remain explicitly unverified.
- Sampling/target definitions must state what the half-open range bounds, any warm-up selection, whether the decision event or cohort is visible to features, horizon endpoint rules, and treatment of incomplete forward windows. Any label-support range outside the decision range must be explicit and reported. Label reads cannot enter feature evaluation or cross protected partition/Sealed boundaries.
- Every inspected source row is accounted for as accepted or explicitly reported as rejected, duplicate or conflicting. Resume replay is distinguished from source duplication. Duplicate handling cannot silently discard source provenance or conflicting values.
- Missing archive paths and observed intervals with no trades are reported distinctly. Neither proves venue-wide completeness. The service cannot silently narrow the requested universe or time range to make a run succeed.
- Exploratory admission can permit unverified scientific evidence; it does not waive required identity, precision, format, instrument definitions or selected-partition integrity. Known missing or incomplete selected partitions fail admission with an incomplete-input report. Unverified venue-wide completeness is a separate evidence limitation, and an observed no-trade interval is not a missing partition merely because it is empty.
- Missing instrument definitions, unsupported precision, incomplete partitions and incompatible source/native build formats produce specific failure reports. Original source files and existing catalogs remain intact.
- Current Hyperliquid instrument definitions cannot be represented as historical definitions. Missing fee or contract parameters remain missing, and any exploratory use of definitions with limited provenance remains visible in the result.
- Existing research budgets, Sealed isolation, independent-evaluation rules, trial accounting, licenses and target-only delivery boundaries continue to apply. Completing tick evaluation does not bypass scientific admission or independent evaluation.

### Non-goals

- Replacing BAR research or converting every source into ticks.
- A general event-stream strategy engine, order-book/L2 factor framework, perpetual funding settlement model or live trading path.
- Outcome-record acquisition or outcome-contract research as a prerequisite to the configured Hyperliquid trade scope.
- Inferring missing historical definitions, fees or contract parameters.
- Calling a loaded archive, successful conversion or completed calculation scientifically qualified.

<a id="decisions"></a>
## Accepted first supported profile

1. **Sampling policy and horizon vocabulary.** Explicit `EVENT_TIME_COHORT_V1` decisions after each instrument's complete equal-event-time cohort; `FIXED_TRADE_COHORTS` labels use the quantity-weighted price return at the H-th later cohort. Warm-up stays inside the selected interval; incomplete and cross-partition labels are excluded. Initial event-time-proxy evidence is exploratory and unqualified.
2. **Factor eligibility.** Extend `qz.factor-wasm` with a named tick feature schema through the existing generic feature ABI v2. Preserve its BAR mode and leave volume, ensemble, portfolio and simulation consumers BAR-only. Unsupported combinations fail explicitly.
3. **Local resource envelope.** One active tick unit, 100,000 records per batch/cohort, 1,000,000 selected trades per unit/study, a 2 GiB process address-space limit and an initial 10 GiB aggregate derived-output budget. Limits are explicit and configurable through the trusted operator/worker; exceeding a limit fails instead of sampling or shrinking coverage. Reuse completed immutable import units through existing reports; interrupted units require a new output directory.

The owner accepted these choices before the delivered implementation. Acceptance does not establish source qualification or market completeness. The policy vocabulary remains explicit so later policies cannot silently change existing factor meaning.

<a id="references"></a>
## Relationship to existing requirements

The [current product architecture](../../architecture.md), [Hyperliquid data preparation](../../../runtimes/data/hyperliquid.md), [original preparation task](../hyperliquid-public-history/task.md) and [current data-plugin task](../hyperliquid-history-100ms/task.md) remain the baseline. Their scientific entry preserves the BAR boundary. This proposal supersedes that boundary only with owner acceptance of the tick-native expansion and its explicit sampling, factor and resource choices; it does not retrospectively change completed preparation work or BAR admission semantics.

## Preparation evidence

Current-source research and independent requirements/plan review were completed on the `dev` baseline above. Review corrections distinguish current cohort aggregates from prior causal feature state, restrict missing masks to defined derived missingness, keep resource budgets cumulative across resumed units, and preserve exploratory representation metadata without historical qualification claims.

Pinned `lychee` 0.24.2 checked the initial task and plan: all seven unique local references passed. Repository-wide `make check-links` ran and failed on 63 existing links in 22 unchanged historical/source documents, largely pointing to retired modules. No unrelated historical documents were changed.

The complete TypeScript check passed: 534 checks passed and 21 native/source-fixture-dependent checks explicitly skipped; all Service, Research, Nautilus and CLI typechecks passed. Web's existing 26 checks and production build passed, and the two additional tick evidence-presentation checks passed separately. Deployment helpers passed under the complete official Node 22.19.0 distribution: 228 source, 26 deployment, five package and eight release-helper checks, with one explicit real-release-distribution skip. These helper checks did not perform a release.

An independent reviewer executed 14 matching native tick checks covering exact cohort behavior, forward-label isolation, native file inventory, availability contradictions, physical held-out separation, precision and empty-instrument coverage. Final native checks passed 15/15 in both standard and high-precision profiles; the final source operator passed 17/17. Contracts/domain passed 221 checks, with no failed or ignored checks. Repository Rust formatting passed.

The real dual-format native build completed and retained seven artifacts, including separate standard BAR and high-precision tick executables. The final standard executable performed actual native BAR conversion/preparation/readback and factor execution. The final high-precision executable performed 32-cohort calculation through HTTP → Pi Durable → remote Worker → native kernel → owner-only final results under the default 2 GiB address-space limit. The packaged source operator converted exact controlled TradeTicks without BARs, reused its verified completed receipt, preserved the original input, and failed with an INCOMPLETE report when a required definition was missing; its live process limits showed 2 GiB soft and hard address-space bounds. These are controlled software checks, not fresh archive, model-quality or PIT evidence.

Web's final production build and all 28 unit checks passed. Source/acquisition selection, proxy-clock labeling, separate output budgets, two-binary routing, receipt replay and process cancellation were checked independently. Final immutable-Head CI and GitHub review remain pending.

The final exposure compatibility correction passed all 192 research checks: legacy BAR/BAR uses its original exact BarType and label-time comparisons; BAR/TradeTick comparisons also account conservatively for each shared instrument's underlying BAR interval. Neither changing granularity/modality nor copying a catalog can manufacture a fresh independent tick sample. Stored exposure facts and the original ledger are retained. Focused diagnostic checks also confirm that tick identity, coverage, precision, definition and resource failures keep their specific public codes without exposing private paths or source values.

The final high-precision executable's SHA-256 is `20f004518541b7cc34ce0a098d1d6ffba9a8ee0e8c3bdf73f96ea8c36cb64fa8`; the standard executable is `f069b8883214e57446b7f257b267a6da3115b7c00ad7caa4998d19d9e7cb0fe7`. These identify local controlled acceptance assets, not source-market qualification or a published release.

The original configured Hyperliquid archive bytes are not present in this workspace. Fresh archive access and Playwright Chromium installation are unavailable under the current network policy; the Chromium CDN returned an explicit proxy denial. No new real Hyperliquid acquisition/conversion, real-market qualification, browser acceptance, release or deployment is claimed. Final native/CI/review and merge evidence will be recorded at its actual scope.
