# Continue Hyperliquid research with CryptoHFTData

<a id="task"></a>
## Task

Task ID: `cryptohftdata-hyperliquid-research`.

Source: the user's request to continue chat `01a17fc2-9d02-7270-9ed5-3fe8b0afdf67` here after updating, using the new data-source plugin. The original objective is small-capital Hyperliquid factor research, backtesting and portfolios, seeking Sharpe above 5 and annualized net return above 150% in bull, bear and ranging markets, at a frequency compatible with 0.1–1.5 ms latency jitter.

Endpoint: implement missing capabilities on current remote `dev`, complete review/CI/merge and install the resulting dev release, then continue the finite research through the installed CLI. Financial targets require actual net results; successful software delivery does not answer them.

<a id="intent"></a>
## Intent

The installed release `v3.0.0-dev.20261106142444.42233672808` contains the new `qz.data.cryptohftdata` plugin and the receive-clock/L2 path. The complete published archive now contains 299,670,023,485 bytes. Actual cohort import exposed an incorrect cross-clock ordering check: Nautilus explicitly permits exchange `ts_event` to be ahead of collector `ts_init`. Prepared factor input is not registered yet. The old economic replay is terminal but has incomplete NAV and cannot support the requested performance conclusions.

Use only free published historical files and official upstream components. Retain all original studies, negative results and the 17 consumed attempts of the original 100-attempt budget. Never restart the old deleted-source studies, rewrite exposure history, collect live market data, or use old runtime overlays. Preserve user data and unrelated worktrees.

<a id="spec"></a>
## Requirements and design

- Extend the new plugin with actual published trades; retain exchange event time, collector receive time, price, quantity, trade ID and aggressor side. Reuse the existing native importer and WASM feature ABI.
- Add an explicit receive-time cohort/label mode without changing the legacy event-time mode. Features and labels use only information already received. Preserve raw event bounds for historical exposure comparison.
- Keep both original clocks, including negative exchange/collector offsets. Derived features and prediction timestamps use the receive analysis clock. Each prepared view exports its own actual raw event bounds; exposure checks retain the original receive boundaries and use the larger raw/receive end without relaxing unrelated study histories.
- CryptoHFTData can republish very old trades after reconnects. Before model evaluation, explicitly select published deliveries whose positive source age is at most the declared one-day bound. Fully validate excluded rows and retain their counts, original file identities and row locations in the source report. Negative clock offsets remain eligible; no timestamps are shifted or clamped.
- Declare `OBSERVED_PUBLISHED_FILES` for this source, keeping missing hours, unobserved requested symbols and resolved duplicate counts. This describes the observed published population, not complete exchange history or historical listing dates.
- Use actual complete L2 snapshots in Nautilus for the new economic mode. Retain native fees, cash/margin, IOC, minimum-order and liquidity accounting. Books are sampled observations, not a reconstruction of unseen exchange updates.
- Skip the publisher's hourly carry-forward checkpoint as a fresh L2 observation only when the first complete file frame has its artificial exact-hour receipt and every original `transaction_time` is explicitly null. Validate the complete frame first; retain the previous real mark age and consumed depth. Missing columns are not null, and an asset with only a checkpoint has no fresh executable book.
- Reuse the current shared-account machinery for a genuine multi-factor combination; a sum of separately capitalized factor curves is insufficient.
- Continue the original 3,000 USDT scenario: 60-second decision/holding intent, 1-second base latency plus the requested jitter, tier-0 scenario fees of 4.5 bp perpetual / 7 bp spot and 2 bp extra cost. These are assumptions, not measured network latency or historically qualified metadata.
- Preserve missing marks, FX and settled funding as unknown. Funding uses the oracle price and hourly settled rate; context observations alone do not establish the final paid rate.
- Select calendar partitions before model evaluation, respecting recorded exposures and actual market/regime coverage. Inspecting source structure does not establish scientific independence. No reduced universe or short interval silently replaces the original question.

The public top-20 snapshots are about seconds apart. They can support a frequency-compatible scenario but cannot establish a submillisecond execution advantage. Original instrument definitions survive as an October 28 official snapshot, with explicit historical-parameter limitations.

The old Polymarket scope's namespace was recovered from 909 original native selections and their successful receipts. Their union contains exactly 2,937,699 identities; the actual producer revision `b15da9a1c243bbcdbc2bb7f5b52e0d2333043461` enforces `.POLYMARKET` and native BAR identity equality. The existing immutable catalog/source binding associates these identities with scope `1208:1230`. Store this conservative namespace on the existing cached scope, keeping the original reference and every exposure unchanged. Same or unknown venues still require the original identity comparison.

Before new model evaluation, the calendar proposal is: source warm-up from February 12, analysis from February 13, training ending April 15, validation ending June 2, and independent evaluation ending October 13 (exclusive), all UTC 2026. The previously consumed Hyperliquid iterations end June 1; unrelated recovered Polymarket identities cannot expose Hyperliquid outcomes. October 13 technical samples are outside this proposal. A source-based BTC completed-day mark classification finds bull, bear and range days in the independent calendar; this is a new explicitly declared mark-price classification, not a claim that the former daily-VWAP labels are identical. Actual coverage and registered requests remain to be prepared.

<a id="plan"></a>
## Implementation plan

1. Source adapter: `runtimes/data/cryptohftdata.py`, `source_plugins.py`, source tests and documentation. Confirm one complete published trade file before extending the mapping.
2. Receive-clock factor path: native tick contracts/domain, `apps/job/src/ticks*`, preparation and `packages/contracts/src/tick-input.ts`; update service export, projections and research descriptions. Add concise tests for clock ordering, grouping and labels.
3. Observed economics: existing `apps/job/src/ticks/economic*`, economic contracts and service consumers. Agree the smallest shared-account combination route before implementation; do not build another matching/accounting system.
4. Correct the existing cross-modality exposure comparison to use provably disjoint raw event windows before loading unavailable historical asset scopes. Keep every ledger entry.
5. Run focused checks, independent review and final-head applicable CI; merge and install only the newly published dev artifacts. Then prepare bounded data windows through the new plugin and submit the remaining finite trials with their actual IDs and results.

Work is split between source, native clocks, economics and service integration agents, with independent review of the final changes. Root owns this task record and integration. Local disk and RAM are constrained: no duplicate archive, cold local native build or concurrent unbounded replays.

<a id="verification"></a>
## Verification

The original eight studies are terminal, with 17 consumed attempts retained, including failures. No new study, trial, portfolio or economic replay has run in this continuation. The original 100-attempt total and scientific/financial criteria remain unchanged.

[PR #284](https://github.com/zhengui666/QuaZonai/pull/284) is merged into `dev` as `cc8bb7e42551d3d7209388e1cf21253e1a27e2ea`. Its exact final head `6a2b7f6295f1f28943da5165c9df9bae54b4b8c5` passed all applicable checks and independent/GitHub Codex review. The merge's [push CI](https://github.com/zhengui666/QuaZonai/actions/runs/42233672644) also passed: 1,024 native tests, source Python, TypeScript/deployment, Web, remote Nautilus and mapped-history checks. The matching [dev release](https://github.com/zhengui666/QuaZonai/releases/tag/v3.0.0-dev.20261106142444.42233672808) is complete.

Published data download is complete: 4,845,281 files and 299,670,023,485 bytes, including trades and the uppercase-k book/context supplement. All 206 original perpetual IDs retain their context and official settled-rate coverage; missing observations remain explicit. Native cohort preparation, dataset registration and financial results are unfinished. The fixed calendar, receive clock, scenario and selection rules are recorded in the local research data directory before strategy outcomes are read.

The follow-up also fixes the shared native L2 encoding boundary: source levels use native fixed-point precision, while the official instrument definitions can require fewer decimals. Re-encode only exactly representable raw values using the upstream integer checks; preserve original values, timestamps, numeric increments, minimums and fees. Reject unsupported historical precision rather than rounding depth or changing the instrument rules.

The follow-up targets redundant native L2 work: every observed snapshot currently passes all levels through Nautilus even when no order target exists. Retain the original Quote, UTC clock, funding and outer mark/NAV updates; send complete Clear/Add on an instrument's first frame, an execution target, or an empty/one-sided snapshot. Other idle two-sided frames need only the same actual Quote. The first full frame is required by the official engine's L2 initialization. All orders remain IOC, without native latency or automatic liquidation. Source decoding and the 523-asset universe stay unchanged. Head `58bbba5f` passed all applicable CI, including 1,030 native tests and the precision/idle regressions in both precision builds. The subsequent dual-clock, freshness and checkpoint changes require a new final-head check and review; no full-history throughput result is claimed.

The first actual latest-dev cohort import exited on an AAVE trade whose collector clock was 10.9 ms behind its exchange clock. Both original timestamps are valid under the upstream timestamp contract. A separate partial source inspection also found old reconnect deliveries, so the positive-age population restriction is explicit rather than a claim about the full publisher's latency. No strategy outcome was read. The hourly checkpoint rule follows the publisher's spot and futures wire-format documentation; it must not reset liquidity or make old valuation data appear fresh.

<a id="delivery"></a>
## Delivery

The initial data-source and receive-clock/L2 path is published and installed in `v3.0.0-dev.20261106142444.42233672808`: CLI, user Skill and active application/native containers match the `cc8bb7e4` release. The application, native worker and database are healthy. The raw source archive is mounted read-only into the native worker. The cached Polymarket namespace recovery retained all exposure entries and unrelated roots. The original dirty checkout and all research history/data remain preserved.

The focused performance follow-up uses `codex/observed-book-replay-performance` at `/home/zzy/.codex/worktrees/cryptohftdata-replay-performance/QuaZonai`, based on current `dev` (`cc8bb7e4`). Complete its applicable CI and exact-head independent/GitHub Codex review before merging to `dev`. Install the resulting latest complete release before economic research; successful engineering delivery does not establish the original financial targets.
