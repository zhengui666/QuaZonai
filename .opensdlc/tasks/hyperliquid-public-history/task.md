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
4. Update [data documentation](../../../runtimes/data/README.md), [architecture](../../architecture.md) and the existing native-history workflow. Keep checks targeted; no stored data fixtures.
5. Verify real source schemas and all-market selections, native readback and adjacent BAR behavior. Record failures and unperformed work below. Deliver only task changes through PR review.

<a id="verification"></a>
## Verification

Observed locally on 2026-10-16:

- Acquired all 184 selected original files for 2026-10-13: 655,551,750 bytes, 178 book files, perpetual/spot/outcome fills, candles, asset contexts and the original README. Every selected file matches its published size and the inventory has no missing selected paths. Interrupted/short responses did not publish a completed manifest; the repeated acquisition reused complete files. This path does not claim content-hash verification.
- The unmodified official Hyperliquid adapter returned 1,471 native definitions: 541 perpetuals, 332 spot pairs and 598 outcome-side contracts, with 853 native currency definitions and their actual current observation time.
- `python3 -I -B -m unittest discover -s runtimes/data -p test_hub.py -v`: 2 checks passed. The earlier Rust invocation passed 51 checks (4 importer, 12 catalog, 10 preparation, 1 instrument-version, 21 managed and 3 native-update), plus the currency-registration check separately. Clippy with `polymarket-history,catalog-prepare,market-history` passed before the real-data corrections below.
- The first real 00:00–00:05 UTC import scanned 10,320,319 rows and returned `INCOMPLETE`, retaining 40,748 read-back-verified native records. It reported 282 records without current mappings, 869 price-precision and 1,597 size-precision failures. Its window contained no book snapshots; file scan messages were not evidence of book import. Inspection found five-decimal outcome prices and fractional spot quantities not representable by current contract parameters, including 231 selected spot fills needing more than nine decimals. None were rounded into compliance.
- The corrected actual 00:10–00:15 UTC import returned `IMPORTED`: 52,053 selected source rows, 89,681 native records with exact readback, no missing identities/rejected rows/partitions. Native output: 38,292 trades across 265 markets; 11,701 supplied BARs across 281; 36,458 snapshot deltas and 890 quotes across all 178 published book symbols; 1,170 funding and 1,170 mark observations across 234. Original parameters remain missing for 173 selected rows across 30 expired outcome symbols; those IDs come from official parsing, not invented definitions. 51,880 rows use later-observed definitions. This is a source-record import, not historical research qualification.
- The real book write initially exposed the official catalog's homogeneous-metadata requirement for Clear/Add batches. QZ now assigns the generated Clear control's unused numeric fields the snapshot encoding precision; native empty/one-sided snapshot write/readback passes without an upstream patch. The new operator's 6 checks and the adjacent history/catalog/preparation/managed regression invocation passed with high precision. Final checks after integration with current remote `dev` are still pending.
- Actual source coverage reveals partial daily streams: books start at 00:09:29.972 UTC and contexts stop at 06:42 UTC on 2026-10-13. The report exposes actual event ranges instead of trusting date directories. An independent raw 00:00–00:05 volume comparison found supplied candle totals equal selected taker totals in 206 of 301 shared symbols, not all of them; preserve published candles as source records, not verified executable volume. Trade-derived BAR verification remains pending.
- `CARGO_NET_OFFLINE=true make check-architecture`: passed. `make check-links` could not execute because `lychee` is not installed; this is not a passing link check.

The source inventory, public first-row schemas, original asset-context CSV header and documentation were inspected through public endpoints. These observations and a successful download are not an independent exchange-wide completeness reconciliation. No formal research qualification or full-corpus acquisition has been performed.

<a id="review"></a>
## Review

Not requested from GitHub yet; no CI or Codex outcome claimed.

<a id="delivery"></a>
## Delivery

Feature branch exists locally. No implementation commit, PR, merge, deployment or live registration yet.

<a id="handoff"></a>
## Handoff

Working branch: `feat/hyperliquid-data`, based on local `dev`. Pre-existing changes to `AGENTS.md`, the deleted license and untracked agent/settings directories must not enter this task's commits. Continue with the approved generic acquisition/native conversion implementation.
