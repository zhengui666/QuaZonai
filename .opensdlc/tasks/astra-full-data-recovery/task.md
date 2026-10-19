# Restore catalog-wide Astra research

<a id="task"></a>
## Task

**Task ID:** astra-full-data-recovery

**Source:** Owner requests stopping the empty-running study847, repairing actual computation, then pursuing the original all-data, dynamically selected market, small-capital ensemble objective with openai/gpt-6-astra/max. No promise of attainable returns is made. Study847/submission855 is now STOPPED/unanswered-aborted with zero trials; old4 and six failed trials remain.

**Endpoint:** working real-data execution, applicable checks and required final-Head review/dev merge for source changes, matching installation, then actual research outcomes. Engineering recovery alone is not the financial endpoint.

<a id="intent"></a>
## Intent

Current dataset contains the complete pinned v2 price/volume catalog but no native request or strategy. Copying an earlier fixed10/32/2253 market set would violate the current scope. The existing evaluator also materializes every BAR and forecast and repeats all market identities in requests/results; it is not a usable full-catalog scan for nearly three million series on this host.

<a id="spec"></a>
## Requirements and design

First restore catalog-wide component calculation through a native pooled scan, reading every original BAR series with official Arrow/Nautilus decoding, original time boundaries and causal feature/label rules. One series is retained at a time; pooled IC remains the existing ndarray-stats calculation on original included pairs, not an average of asset correlations. Short/unobserved series are reported as unavailable rather than failing the whole scan or zero-filling. Preserve one shared explicit fuel budget across assets. Return actual coverage and original request without millions of repeated identities or model-facing final values. This does not supply missing simulation definitions, fees, a shared-account curve or qualification; those remain required before claiming ensemble recovery.

Native paths stay trusted local arguments. Existing explicit-selection commands and schemas keep their behavior. Add a distinct scan command/request instead of silently changing empty-selection semantics. Reuse native features, partition censoring, official Wasmi lifecycle and scientific metric code. No new numerical engine, scheduler, data source, dependency fork or truth claims.

<a id="plan"></a>
## Implementation plan

1. Retain CLI stop receipt and inspect full source/catalog, old preparation helpers and current kernel interfaces.
2. Add a native regression that compares a complete-catalog pooled scan with the original explicit two-asset calculation, including clipped iterative windows, unavailable short series, real coverage and original fuel accounting. Establish its pre-fix failure.
3. Add the compact native scan DTO, shared incremental metric accumulator and one-series-at-a-time Arrow/Nautilus adapter/CLI entry. Keep old behavior unchanged and run focused native tests.
4. Integrate only supported research tools/registered input, preserving original stopped studies and remaining allowance; restore native shared-account ensemble execution separately rather than substituting IC.
5. Run applicable final-Head CI/read-only reviews and deliver through remote dev and the official installation. Admit a linked finite study only after real input checks; inspect actual coverage, IC, daily account returns and losing days.

<a id="verification"></a>
## Verification

CLI reconfirmed847/855 STOPPED/unanswered-aborted, zero science trials; six original failed trials remain on4. In the isolated worktree, the distinct native scan DTO/command now uses official Parquet Arrow/Nautilus decoding, original shared feature/partition/metric rules, shared optional fuel and compact coverage. Existing evaluator behavior is retained. Source schema registry and routine `check-science` include the new command/tests.

The new regression first failed before implementation with missing native request/function imports (`/tmp/qz-astra-scan-before.log`). All5 focused tests now pass, including multiple native timeframes for one instrument and the actual stdin/stdout CLI path: original mixed-asset pooled IC/counts/fuel; a globally exhausted fuel allowance; clipped prefix independence and unmetered fuel; short/unobserved coverage and global row-budget failure; original root and positive-price validation. The initial unmetered assertion failed because the test helper supplies explicit fuel; the test explicitly omits fuel rather than changing production behavior.

Local `make check-science`, `make check-ts`, matching native build and `make check-core` pass. The core run executed40 actual native shared-account/durable cases, with no skips. Logs are under `/home/zzy/.local/share/quazonai-development/astra-checks/` (`science.log`, `ts.log`, `native-build-current.log`, `core.log`, `scan-focused.log`). These deterministic checks do not establish real-market quality or the requested research results.

The real installed full catalog readback uses a deliberately constant Wasm model, not a selected factor or profitability test. Inputs are `/tmp/qz-astra-catalog-check/{request.json,constant.wasm}`. Its first invocation was interrupted after30minutes by the tool's outer timeout, with no result; this is NOT a successful full-data check. No heldout performance or financial target is established. The second finite readback completed with exit0, all2937699 registered series observed and all69926278 original BARs processed. It explicitly reports592807warmup-only series; no source series or BAR was replaced by a fixed shortlist. The constant model's three IC values remain null/CONSTANT_SCORE, as expected: this is input validation, not selected alpha or financial attainment. Result, progress and terminal status are `/home/zzy/.local/share/quazonai-development/astra-checks/catalog-{result.json,progress.log,exit-code}`. The process has exited; no second application host owns research storage. Its earlier debug binary predates equivalent root validation extraction, earlier rejection of invalid requests and complete-BarType uniqueness. This registered import uses the original one-minute-only BAR specification; no numeric feature/label/metric rules changed. Original cutoff includes the last native closing BAR via an exclusive end one nanosecond after2026-10-13T00:00:00Z; source date/history/PIT/fees remain unqualified.

The same pinned source's `daily_aligned/2026_04_03.parquet` schema was inspected. It includes actual fee fields but no tick/minimum-order or historical execution-definition fields. Resolution/winner/probability columns are not causal feature inputs. The enriched layer alone therefore does not repair the historical-definition/PIT gap.

[PR220](https://github.com/zhengui666/QuaZonai/pull/220) owns hosted CI and required read-only Codex review. Initial Head `fc200cc4` passed scope/TypeScript/Web/science-core CI. Codex found one P2: an instrument-only uniqueness check rejected valid multiple BAR timeframes. The added1-minute/1-hour regression genuinely failed with `DUPLICATE_CATALOG_INSTRUMENT` before changing uniqueness to the complete native `BarType`; all5 focused cases now pass. The next Head `5216725d` also passed all hosted CI; Codex then identified the missing regenerated native schema. The existing source-export equality test was observed failing before regeneration. Export now runs the retained `cargo run --locked -p contracts --example generate` rather than hand-editing JSON. The existing native preflight report is also registered to preserve its offline schema views; all19 preflight/schema cases and4fuel-schema cases pass. Routine science now executes that existing equality coverage. Regeneration reflects the current native-only exporter, not the retired HTTP schema set (current HTTP contracts remain TypeScript). Final-Head CI and read-only re-review remain required, not replaced by either older Head.

No merge, matching installed-service deployment, research-tool integration, complete execution definitions or restored shared-account study is yet claimed. The main checkout's unrelated changes and original catalogs/configuration/authentication/storage remain untouched.
