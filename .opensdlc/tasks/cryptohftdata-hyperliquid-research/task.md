# Continue Hyperliquid research with CryptoHFTData

<a id="task"></a>
## Task

Task ID: `cryptohftdata-hyperliquid-research`.

Source: the user's request to continue chat `01a17fc2-9d02-7270-9ed5-3fe8b0afdf67` here after updating, using the new data-source plugin. The original objective is small-capital Hyperliquid factor research, backtesting and portfolios, seeking Sharpe above 5 and annualized net return above 150% in bull, bear and ranging markets, at a frequency compatible with 0.1–1.5 ms latency jitter.

Endpoint: implement missing capabilities on current remote `dev`, complete review/CI/merge and install the resulting dev release, then continue the finite research through the installed CLI. Financial targets require actual net results; successful software delivery does not answer them.

<a id="intent"></a>
## Intent

The installed release `v3.0.0-dev.20261106100442.42199853123` contains the new `qz.data.cryptohftdata` plugin and the completed 279,118,890,859-byte published archive. It does not yet supply prepared factor input. Its books and context preserve source timestamps, whereas the old factor path requires identical event/availability timestamps. The old economic replay is terminal but has incomplete NAV and cannot support the requested performance conclusions.

Use only free published historical files and official upstream components. Retain all original studies, negative results and the 17 consumed attempts of the original 100-attempt budget. Never restart the old deleted-source studies, rewrite exposure history, collect live market data, or use old runtime overlays. Preserve user data and unrelated worktrees.

<a id="spec"></a>
## Requirements and design

- Extend the new plugin with actual published trades; retain exchange event time, collector receive time, price, quantity, trade ID and aggressor side. Reuse the existing native importer and WASM feature ABI.
- Add an explicit receive-time cohort/label mode without changing the legacy event-time mode. Features and labels use only information already received. Preserve raw event bounds for historical exposure comparison.
- Declare `OBSERVED_PUBLISHED_FILES` for this source, keeping missing hours, unobserved requested symbols and resolved duplicate counts. This describes the observed published population, not complete exchange history or historical listing dates.
- Use actual complete L2 snapshots in Nautilus for the new economic mode. Retain native fees, cash/margin, IOC, minimum-order and liquidity accounting. Books are sampled observations, not a reconstruction of unseen exchange updates.
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

Work is split between source, native clocks, economics and service integration agents. Root owns this task record and integration. Local disk has about 37 GiB free and RAM is constrained: no duplicate 279 GB archive, cold local native build or concurrent unbounded replays.

<a id="verification"></a>
## Verification

At resume, the goal is active; the new worktree is based on `ed0c48fb`. Current installed app/native containers are healthy and use that release without source overlays. All eight old studies are terminal, there are 17 recorded factors/trials and no portfolios. No new trials or replay have been started for this task.

Pending: source conversion, changed-path tests, final review/CI, deployment and all financial research results.

Implemented: original receipt-clock grouping and labels; bounded shared raw-book decoding; observed L2 IOC and native initial currency conversion; fixed-weight multi-factor signals through one original account; source selection without raw copies; the conservative namespace comparison. Actual BBO derived from the same complete source snapshot feeds Nautilus's official risk-price cache; only the L2 book supplies fills.

Focused source Python tests (55), service export/admission/exposure tests (22), model/replay protocol tests (11), and Web clock presentation tests (5) passed. Service/research type checks passed. The native schema was generated from the changed contracts using the repository generator. Independent review found the native preparation clock mismatch; it is corrected with a cross-partition regression. Native compilation/tests and final review are pending CI. No new scientific trials or replays have run.

The first [PR #284 CI](https://github.com/zhengui666/QuaZonai/actions/runs/42218591254) built the native programs and passed TS/source/deployment and Web/e2e. The science suite stopped at a union test missing its new receive-time target sample; the sample is added. Review corrections cover cumulative L2 cash cost, the actual entry-notional cap, one-sided exits and local source-file version changes. File selection records the existing local device/inode/size/mtime/ctime identity and checks it around reading; no new content-hash procedure is introduced.

Receive-time final reservation now distinguishes disjoint causal windows within the same original prepared study, after checking its original primary/sealed binding. Other studies, modalities, old records and existing finals retain the raw-event comparison. Gap metadata groups missing hours without dropping their exact count, and new-source coverage responses reference the full catalog report instead of repeating million-file arrays in every trial.

The same plugin can load the exported research configuration through `preparedConfig`. Public trade inventory is complete: 1,418,278 files, 14,438,943,369 bytes for the original supported universe. Trade and official historical funding downloads are running; source statistics, data preparation, installation of this continuation and financial evaluation remain unfinished.

<a id="delivery"></a>
## Delivery

In progress on `codex/cryptohftdata-research`, worktree `/home/zzy/.codex/worktrees/cryptohftdata-research/QuaZonai`. The original dirty checkout is preserved. The prior plugin release is already installed; this continuation has not yet been merged or published.
