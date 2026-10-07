# Original Polymarket observations to Forward preparation

## Scope

Reuse the existing official data-only Polymarket source and its retained NDJSON.
The new `job polymarket-paper record-forward --plan PLAN.json --output NEW.ndjson`
records the explicitly selected business BAR window and stops at its observed end
or cancellation. The plan has `schema_version: 1`, `instrument_id`, canonical
`bar_type` ending in `LAST-EXTERNAL`, decimal-string `first_close_ns`, and integer
`required_bars`. These are business observations, not an execution time/CPU/token/
output budget. Existing `run`, `serve` and hidden `source` Paper behavior remains.
The optional existing `--proxy-env NAME` selects an already-approved variable.
No execution client, credential, account connection or order is created.

The recorder uses the official native TimeBarAggregator through LiveDataEngine,
with timestamp-on-close and no empty/carry-forward BARs. The original initial
partial BAR is retained and replayed, but only complete planned windows after
readiness enter the Forward catalog. Every original trade/close is replayed in
receive order; late venue event clocks are preserved, not manually re-bucketed.
A missing initial close or unknown suppressed/reset state is Unsupported.
Native quotes/trades/BARs retain their timestamps. The original SourceRecord also
records the actual callback observation time. A scheduled native BAR timestamp
is not treated as a receive time. Parsed lifecycle readiness and actual final
drain remain explicit; any gap, change, cancellation or missing BAR is incomplete.
The source scope is observed selected native/parsed events, not all exchange packets.

## Offline operator

Build the operator with the existing `catalog-prepare,polymarket-history` features
and the recorder with `native-paper`. All dependencies remain official pinned
packages; no separate aggregator, catalog or indicator engine is implemented.

`catalog-prepare prepare-forward --recording ORIGINAL.ndjson --plan PREPARE.json
--warmup-catalog ORIGINAL_ROOT --warmup-metadata ORIGINAL_METADATA.json
--warmup-selection ORIGINAL_SELECTION.json --original-features ORIGINAL_PART.json
--output NEW_DIRECTORY` performs local preparation only. Repeat `--original-features`
for the original parts in their original order.

The preparation plan contains `schema_version`, the same recorder plan as `source`,
an explicit local `dataset_revision_id` for the native quality document, the unchanged
`feature_schema`, and a new catalog `declaration`. Its Universe and origin must match
the supplied original warmup metadata. Its PIT must remain UNVERIFIED. Measured
row count, quality and recorded-feature descriptors cannot be caller assertions.
Source identity, license and scientific meaning are not fabricated by preparation.

Only the versioned raw quote source reference
`nautilus-polymarket/0.63.0/quote/INSTRUMENT_ID` with source keys `bid_price`,
`ask_price`, `bid_size` or `ask_size` is implemented. Feature keys, definition
order, availability basis and maximum age stay unchanged. Unknown/derived/textual
feature meanings are Unsupported; they are not renamed to fit this producer.
Observed availability uses the actual retained observation, not import time.

Prepare replays original TradeTick and retained close events through the official
TimeBarAggregator and compares every original native BAR field. It retains the raw
BAR, then changes only its persisted aggregation source to EXTERNAL and its availability
to the actual recorded close observation. MID is never renamed LAST. Missing intervals
are not filled. Changed definitions, ambiguous trades, late revisions, unresolved
transport loss, missing lifecycle evidence and incomplete windows fail closed.

The new catalog preserves the supplied warmup BARs and original feature bytes.
It must be a contiguous data continuation of that warmup and cannot extend its
original Universe or invent settlement. Existing `catalog-prepare` measures quality
and verifies native Parquet readback. Registration time fields use a conservative
microsecond ceiling; all original native/feature/quality nanoseconds remain intact.
The final `forward-preparation.json` is written only after preparation succeeds.
It reports PREPARED_UNREGISTERED, not a Dataset registration or research PASS.

## Acceptance boundaries

This does not accept an Alpha, verify its accepted evaluation report, execute a
model or reset its state. The original Store/CURRENT_DECISION path must still
revalidate its original report/model and replay the warmup prefix. An unrelated
or expired market and a different feature dictionary cannot substitute for them.

Frozen V5's experiment evaluator rejects BinaryOption; that capability correction
is a separate candidate. This source bridge does not remove that or any scientific
gate. Public capture, source registration, actual model continuation, Paper account
relay, PostgreSQL, production deployment and real orders have not run for this slice.
Controlled native fixtures establish only local component and protocol behavior.

A selected complete first BAR additionally requires an earlier actual native BAR
callback observed no later than that window's start. SourceActor `ready` alone
precedes the BAR subscription and cannot prove complete aggregation. The producer
and importer both reject delayed-start or missing-initialization evidence; choose
the explicit business plan early enough to retain this native prewindow evidence.
This is a data completeness requirement, not an execution budget.
