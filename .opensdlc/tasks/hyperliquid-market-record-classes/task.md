# Separate protocol records from market-trade inputs

<a id="task"></a>
## Task

**Task ID:** hyperliquid-market-record-classes

**Source and endpoint:** Continue the owner's full Hyperliquid factor/backtest/portfolio research with 100 initial factor trials. Add only the generic predicate needed to express the verified source-record classification; deliver through reviewed CI and merge to dev. Preserve original fills for future event/cashflow consumers. This does not complete research or prove executable profitability.

<a id="intent"></a>
## Intent

Full-archive inspection shows `crossed=true` includes non-order-book account/protocol records. Treating them as ordinary price/volume ticks can create artificial signals. Current `not_equals` cannot exclude several values of the same field.

The classification scan covered 2,617,960,184 original rows. A semi-join on `(timestamp,coin,tid)` for known protocol markers, excluding Dust, matched 399,698 rows, split into 199,849 rows on each side. It found no protocol-marked maker paired with an apparently ordinary taker lacking the method marker. This is not a complete uniqueness or buy/sell consistency proof.

<a id="spec"></a>
## Requirements and design

- Add optional `not_in` field-to-scalar-list predicates, using existing equality semantics. Any matching listed value excludes the row; separate field conditions remain conjunctive. Existing profiles, scalar conditions, null values and filtered-row accounting retain their meaning.
- Keep arrays/objects unsupported as individual comparison values. Empty lists exclude nothing. No venue-specific code branch is needed.
- Use an explicit spot/perpetual processing view: `crossed=true`, `liquidation_method != backstop`, and direction outside Net Child Vaults, Settlement, Spot Dust Conversion, Backstop Borrow Liquidation and Partial Borrow Liquidation.
- Keep actual market liquidations: 5,712,689 taker rows with method=market are ordinary Close Long/Short records. Do not filter by `is_liquidation=false` or positive order ID; those fields do not distinguish execution from protocol transfers in this source.
- The resulting 1,308,222,597 rows remain candidate market records, not an exchange-completeness, uniqueness, historical-parameter, PIT or fillability proof. Retained raw settlement/backstop data still need separate accounting treatment in a later portfolio model.
- Leave the shipped acquisition profile and all existing raw files/manifests unchanged. New rules require an explicitly separate processing declaration and a matching operator build.

<a id="plan"></a>
## Implementation plan

Reuse the native importer's `Stream::matches` and source report. Add a small mixed-class regression and malformed-list checks; keep existing scalar rejection coverage. Do not add a new downloader, event-join framework or backtest engine.

<a id="verification"></a>
## Verification

Implemented optional `not_in` predicates using the existing scalar comparator. Regression coverage combines `crossed`, scalar method exclusions and direction lists: actual market Close Long/Short and ordinary spot records remain; protocol/Dust and maker rows are filtered. Null values, empty sets, multiple fields and malformed nested comparisons are covered without changing the default profile.

`cargo fmt --all -- --check` and `git diff --check` passed. Native execution has not run locally; CI remains required. Independent data checks found every source identity maps to its original official native type: 206 CryptoPerpetual and 317 CurrencyPair. All taker-side source order IDs were nonzero, including protocol events; maker-wide order-ID checks were not performed. The native raw-record conversion measurements are resource tests, not scientific strategy results.

<a id="handoff"></a>
## Handoff

Worktree: `/home/zzy/.codex/worktrees/hyperliquid-market-selection/QuaZonai`; branch: `fix/hyperliquid-market-selection`, based on PR #245 Head f7f2204. Rust implementation/tests have one writer; the coordinator owns documentation and processing declarations. PR #244/#245 remain unchanged while their native checks execute.
