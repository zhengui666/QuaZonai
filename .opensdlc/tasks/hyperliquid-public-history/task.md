# Hyperliquid public history

<a id="task"></a>
## Task

**Task ID:** hyperliquid-public-history

**Original request:** Add complete usable all-market Hyperliquid data from Hugging Face or GitHub through general-purpose code, without special-case logic. The user subsequently required a new branch from `dev`, not development on `dev`.

**Endpoint:** Implement and verify the data integration on `feat/hyperliquid-data`; PR targets `dev`. Final-Head CI and explicit clean GitHub Codex review are required before remote merge. No deployment or live-service registration.

<a id="intent"></a>
## Intent

The user accepted the source's actual published interval rather than genesis-to-present completeness and explicitly selected inclusion of trades and order books. Discover markets from records/native definitions, not a fixed symbol list. Preserve existing user changes and data. Do not add content hashing, new freezing mechanisms, gates or stored data fixtures.

<a id="spec"></a>
## Requirements and design

Use the MIT-declared [Hyperliquid research archive](https://huggingface.co/datasets/ProbioticFarmer/hyperliquid-augmented-copytrading/tree/064bc5ea76a6f0db8dd5c2b66f598e925857784d), inspected on 2026-10-16:

| Records | Actual inventory |
| --- | --- |
| Perpetual and spot fills | 443 dates, 2025-07-28–2026-10-13, no absent dates within that range |
| Outcome fills | 165 dates, 2026-05-02–2026-10-13, same mapped fill fields; use only `outcome_fills/all/`, not duplicated per-outcome copies |
| One-second candles | 440 dates, 2025-07-31–2026-10-13, includes spot despite the card's perpetual-only description |
| Book snapshots | 303 dates, 2025-12-15–2026-10-13, individual symbol files; inspected rows have 20 levels per side |
| Asset contexts | 1,242 daily CSV.lz4 files beginning 2023-05-20; funding-rate observations, prices and open interest |

The card's complete-genesis-history and full-depth claims are not established by these observations. Funding-rate observations are not funding-payment events. Wallet-specific maker/taker fills are not separate market transactions; source mapping selects taker records using `crossed=true`. Preserve all original fields in acquired source files.

[Chainticks](https://huggingface.co/datasets/Chainticks/perp-data) has missing dates. The [official historical-data documentation](https://hyperliquid.gitbook.io/hyperliquid-docs/historical-data) does not guarantee timeliness or completeness. The [official GitHub dataset](https://github.com/hyperliquid-dex/historical_data) excludes some market-maker trades. None establishes complete history of every market from genesis.

Acquisition and conversion use a declarative source profile, independent of venue. Official Nautilus owns Hyperliquid instrument and identity parsing, including spot, builder-deployed perps and expired outcome identities. Identity-only mappings do not invent missing contract definitions. Original native instrument definitions are accepted without rewriting historical observation times. Current metadata does not become historical/PIT evidence. Import observed BARs, trades, snapshot deltas/quotes and funding/price updates into native catalogs; reuse the existing BAR research boundary without adding live trading or an L2 strategy engine. Missing mappings/parameters and unsupported simulations must remain visible.

Genuine ten-decimal spot quantities require the official native high-precision encoding. The optional `high-precision` job feature is enabled by the new operator; existing standard-format catalogs and default scientific builds remain unchanged. New-format consumers must use that feature. `--native-precision` preserves raw values independently of contract tick/lot parameters and reports the distinction; default definition-based precision remains strict.

The corpus is 267,618,687,866 bytes; local free storage was about 108 GB. Support date/type selection and verify manageable real all-market windows, not an uncontrolled whole-corpus download.

<a id="plan"></a>
## Implementation plan

The user approved the implementation plan in this session.

1. Add the declarative source profile and generic standard-library public Hub acquisition entry under [runtimes/data](../../../runtimes/data).
2. Add an optional native `market-history` operator entry and only the required official dependencies in [job](../../../apps/job/Cargo.toml). Parse declared decimal/time/column layouts, stream source files, resolve native raw symbols, retain source scope and publish a read-back-checked import report.
3. Reuse Nautilus native instrument/data serialization, snapshot records, `BarBuilder` where needed, and the existing [catalog reader](../../../apps/job/src/catalog.rs) / [preparation entry](../../../apps/job/src/bin/catalog-prepare.rs). Do not relax scientific/account capability checks.
4. Update the [source guide](../../../runtimes/data/hyperliquid.md), [data index](../../../runtimes/data/README.md), [architecture](../../architecture.md) and existing CI. On the current `dev` layout, the optional-format checks belong in the active science CI; the manual native-source workflow remains unchanged. Keep checks targeted; no stored data fixtures.
5. Verify real source schemas and all-market selections, native readback and adjacent BAR behavior. Record failures and unperformed work below. Deliver only task changes through PR review.

<a id="verification"></a>
## Verification

Observed locally on 2026-10-16:

- Acquired all 184 selected original files for 2026-10-13: 655,551,750 bytes, 178 book files, perpetual/spot/outcome fills, candles, asset contexts and the original README. Every selected file matches its published size and the inventory has no missing selected paths. Interrupted/short responses did not publish a completed manifest; the repeated acquisition reused complete files. This path does not claim content-hash verification.
- The unmodified official Hyperliquid adapter returned 1,471 native definitions: 541 perpetuals, 332 spot pairs and 598 outcome-side contracts, with 853 native currency definitions and their actual current observation time.
- After merging current remote `dev` (`070b0a08`, 255 commits beyond the initial baseline), the targeted native invocation passed 55 checks: 6 importer, 13 catalog, 10 preparation, 1 instrument-version, 22 managed and 3 native-update. The currency-registration check passed separately. A final rerun of the 6 importer checks and 4 existing source-tool checks passed after the equivalent lint corrections. `python3 -I -B -m unittest discover -s runtimes/data -p test_hub.py -v`: 2 checks passed.
- Targeted Clippy passed with `polymarket-history,catalog-prepare,market-history` for the three history/preparation binaries and the five adjacent native integration-test targets, with `-D warnings`. Repository-wide formatting and the task diff check passed. The broader all-target lint attempts exposed pre-existing unused native-capital helpers and unrelated ownership warnings; they did not pass and no broad account-code cleanup was attempted. The incoming forward-preparation remainder check received the equivalent `.is_multiple_of()` correction.
- The first real 00:00–00:05 UTC import scanned 10,320,319 rows and returned `INCOMPLETE`, retaining 40,748 read-back-verified native records. It reported 282 records without current mappings, 869 price-precision and 1,597 size-precision failures. Its window contained no book snapshots; file scan messages were not evidence of book import. Inspection found five-decimal outcome prices and fractional spot quantities not representable by current contract parameters, including 231 selected spot fills needing more than nine decimals. None were rounded into compliance.
- The corrected actual 00:10–00:15 UTC import returned `IMPORTED`, and the integrated-baseline rerun reproduced its result: 52,053 selected source rows, 89,681 native records with exact readback, no missing identities/rejected rows/partitions. Native output: 38,292 trades across 265 markets; 11,701 supplied BARs across 281; 36,458 snapshot deltas and 890 quotes across all 178 published book symbols; 1,170 funding and 1,170 mark observations across 234. Original parameters remain missing for 173 selected rows across 30 expired outcome symbols; those IDs come from official parsing, not invented definitions. 51,880 rows use later-observed definitions. This is a source-record import, not historical research qualification.
- The real book write initially exposed the official catalog's homogeneous-metadata requirement for Clear/Add batches. QZ assigns the generated Clear control's unused numeric fields the snapshot encoding precision; native empty/one-sided snapshot write/readback passes without an upstream patch. Identity-only raw import accepts an empty instrument-definition array without requiring a fabricated instrument.
- The trade-only import selected the same 38,292 actual trades and emitted 1,053 observed one-minute BARs across 265 markets; all 39,345 records read back exactly. A separate inspection of every persisted native trade bucket and BAR matched exact open/high/low/close, summed volume, interval-end labels and availability, with no bars for empty intervals. The narrower acquisition reused hard-linked originals and retained their original retrieval time.
- Actual source coverage reveals partial daily streams: books start at 00:09:29.972 UTC and contexts stop at 06:42 UTC on 2026-10-13. The report exposes actual event ranges instead of trusting date directories. An independent raw 00:00–00:05 volume comparison found supplied candle totals equal selected taker totals in 206 of 301 shared symbols, not all of them; preserve published candles as source records, not verified executable volume.
- Post-review reruns reproduced both real-data reports exactly: all-stream output retained 89,681 exact-readback records, and trade-only output retained 39,345 (38,292 trades plus 1,053 BARs). Separate whole-output OHLCV/availability inspection passed again. The 8 importer checks now include original price/size precision changes, changes back across a batch boundary, complete per-file native history readback, and precision changes within/between BAR intervals.
- The final consumer/equal-availability correction passed 62 targeted checks (9 importer, 13 catalog, 10 preparation, 22 managed, 1 instrument-version, 3 native-update and 4 source-tool), targeted Clippy, formatting and diff checks. Before the fix, the existing catalog round-trip case reproduced an old, unselected file's conflicting price-precision schema; the shared reader now uses file-scoped decoding. A small late-trade check reproduced two distinct BAR intervals sharing one availability timestamp and native filename; both now persist/read back with exact common encoding and unchanged values/event/availability clocks.
- The original baseline's `make check-architecture` passed before integration; current `dev` no longer has that Makefile target, so this is not final-Head proof. `make check-links` cannot execute because `lychee` is not installed; this is not a passing link check. Local documentation targets/anchors were inspected, not relabeled as a full automated link check.

The source inventory, public first-row schemas, original asset-context CSV header and documentation were inspected through public endpoints. These observations and a successful download are not an independent exchange-wide completeness reconciliation. No formal research qualification or full-corpus acquisition has been performed.

<a id="review"></a>
## Review

The separate native review of `e87b74be4c4af3e1ea225099ff19409564b7470a` found one actionable boundary: valid original price/size precision changes were mixed in one native write. Two small inline checks reproduced both that persistence failure and the related mixed-OHLC aggregation panic. The correction splits contiguous native metadata segments, resets the official query session and uses file-scoped readback; it also builds BARs with exact uniform raw values and emits each interval at its maximum observed precision. The 8 importer checks and targeted Clippy passed after correction. No upstream implementation or scientific transition rule was changed.

On `370e1989`, all four CI checks passed, but acceptance remained blocked by two verified findings: Codex identified the remaining directory-scoped shared BAR reader, and the native reviewer identified differently encoded BAR intervals with one availability timestamp/native filename. Both were reproduced before correction. The shared reader now preserves per-file metadata, and complete equal-availability BAR groups share an exact maximum-precision representation without moving clocks or modifying raw values. Existing definition, PIT and capability rules remain unchanged. Final-Head clean reviews and CI must be obtained again.

GitHub Codex explicitly reported no findings for `e87b74be`; that receipt applies only to that old Head, not the correction. Acceptance still requires the separate native reviewer and the user's explicitly authorized GitHub Codex read-only review of the final corrected Head, not Codex implementation or production execution. Exact reviewed revisions, findings, CI and merge outcomes belong in the feature branch's GitHub pull-request evidence.

<a id="delivery"></a>
## Delivery

Implementation commit: `16a5e5ab`; current remote-`dev` integration: `f12321cf`. Corrections and local evidence follow on the same feature branch in [zhengui666/QuaZonai#198](https://github.com/zhengui666/QuaZonai/pull/198), targeting `dev`. Delivery is conditional on the exact final Head's applicable CI and required clean reviews before remote `dev` merge. No deployment, research qualification or live-service registration is included.

<a id="handoff"></a>
## Handoff

Working branch: `feat/hyperliquid-data`, created from local `dev` and integrated with current remote `dev`. Preserve the uncommitted user review rule in `AGENTS.md`, the pre-existing license deletion, untracked agent/settings directories and retained stash/backups. Do not stage them with task changes. The generic import and real-data verification are implemented; the delivery boundary remains exact-Head CI and independent/GitHub reviews, not the import status.
