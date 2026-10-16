# Polymarket factor price-volume recovery

<a id="task"></a>
## Task

**Task ID:** polymarket-research-recovery

**Authorization:** Owner-approved goal “修复 Polymarket 计算输入并恢复研究” (2026-10-16). Restore genuine computation and continue the USD 3000 Polymarket micro/high-frequency arbitrage and strategy-combination study using openai:gpt-6.1-sol / max. Preserve research 4, submission 12 and its five native_request_required failures. The first linked successor has at most 299995 attempts; cumulative admitted attempts remain at most 300000.

**Owner update:** Continue beyond input restoration to find a strategy combination with native shared-account portfolio Sharpe > 5. Do not substitute factor IC for Sharpe or claim completion from a pipeline diagnostic; the original cumulative trial budget remains.

<a id="intent"></a>
## Intent and design

The registered price-volume dataset had no data.request. Adding that request alone still failed: the installed evaluate-factor path required simulation instrument definitions, which the price-volume import deliberately does not contain. Factor statistics need observed BARs, not fabricated fees, tick regimes or historical instruments.

Keep normal forecast, simulation and load_catalog unchanged in their definition requirements. The factor-only path reuses native Nautilus BAR decoding, exact identity/time/order checks, original rows, EMA, Wasmi state/fuel accounting and Pearson calculations. It does not certify PIT, source completeness, execution or profitability. No third-party implementation is modified.

<a id="plan"></a>
## Implementation

Extract shared native catalog opening and BAR decoding; add a factor-only price/volume reader. Share the existing forecast point calculation over borrowed BAR slices. Add regressions proving unchanged factor values/fuel, continued rejection by executable-market loaders, and retained row/time checks.

Local maintenance prepares an explicitly disclosed subset of 32 native one-minute BAR series ranked only by Oct 1–8 training observation counts, with Oct 8–11 validation and Oct 11–13 independent holdout. The primary physical catalog ends at validation; the independent catalog retains the original causal prefix. Original source files remain unchanged. This is a training-active retrospective subset, not the entire dataset or proof of a PIT universe. Normal research submission/observation uses only quazonai CLI. Private configuration changes retain the original durable database and credentials.

<a id="verification"></a>
## Verification

- Before fix, the added price-volume regression failed on the missing simulation definitions. A credential-free installed-kernel evaluation of the real prepared data separately failed with instrument_definitions / NATIVE_CATALOG_METADATA_INVALID.
- `make check-ts` passed for Service, Research and CLI after installing their locked worktree dependencies.
- After fix: `cargo test --locked -p job --lib --test factors --test catalog --test science_cli -- --test-threads=1`: 85 library, 18 factor, 13 catalog and 6 CLI tests passed, including real durable-host/native-factor integration. The first integrated attempt lacked the new worktree's locked Node dependencies; installing them and rerunning resolved it.
- Real-data preparation: 52949 primary BARs / 58007 full BARs, preserving original native schema metadata and readback-equal values. No gaps, clocks, prices or definitions were synthesized.
- Credential-free real-data diagnostic: validation had 8881 observations / 8866 included pairs, non-null IC; heldout had zero observations. This fixed baseline is a pipeline check, not a strategy search or an independent evidence claim.
- Independent native review found no correctness issues in the initial four-file change. Final-Head GitHub Codex review additionally identified undefined native price/quantity sentinels; the added regression failed before the correction and passed after official is_undefined checks were added. Missing prices/volume are rejected, not replaced with zero.
- Initial Head ab6aa7e1 passed all four CI jobs. `make check-core` also passed locally using the CI native job/catalog-prepare and the same-source locally built helper; a CI-built helper embeds its runner source path and cannot be invoked directly on this host. Host-built job binaries require a newer glibc than the installed container; deploy compatible CI outputs instead.
- [PR #200](https://github.com/zhengui666/QuaZonai/pull/200) owns the latest final-Head CI/review and merge evidence. Updated-Head verification, remote dev merge, live successor computation, portfolio Sharpe and final budget/history verification remain pending.

<a id="handoff"></a>
## Boundaries

No live orders, funds, account binding, database edits, trial resets or model substitution. Preserve unrelated workspace changes and old task facts. Do not restart ended studies by mutating history; submit a linked successor with remaining budget. A computed factor is not a qualified strategy or a profitable shared account.
