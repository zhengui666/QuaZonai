# Explicit managed spot CASH source wiring

## Current scope

The explicit `simulate` CLI and managed `SimulatePortfolio` retain opt-in
multi-currency CASH simulation over already supplied local source files. The
2026-10-07 instruction prohibits Hyper self-collection. The Hyper network producer
and its commands are removed; `hyperliquid-offline` exposes only existing-file
parsing, source verification and selected-source preservation.

Candidate/Sequence/Study/Experiment and research qualification retain their
existing admission rules. No source, account, real order or missing evidence is
created by the offline consumer.

## Source boundary

The legacy `FrozenSpotCandleSourceV1` format preserves original official instrument
serialization, original parsed Info response values and their original completed
response observation clocks. These are not HTTP wire bytes or socket-arrival
proof. Historical availability remains an enum containing only `UNVERIFIED`.

Each response describes one already closed minute. Original sequence and array
locators, native open-event time, original observation `ts_init`, and independently
bound exclusive close (`T + 1ms`) are preserved. Native data comes from the official
parser, without rewritten clocks, invented source provenance or relabeled fixtures.

The sidecar and Parquet use the same registered snapshot/storage version. The
loader compares complete native instrument and Bar serialization, including
precision, and rejects changed, missing, duplicate or unexplained rows. Prepare
retains only selected unchanged original response objects. It does not fetch
missing files or promote historical PIT.

The CLI ID is the existing `app.dataset_revisions.id`, consistent with
`NativeTaskParametersV1.dataset_revision_id` and `RuntimeInputV1.revision_id`.
It is propagated and bound; the offline CLI does not query the registry to prove
that it exists.

## Retained shared behavior

- Managed dispatch changes only explicit `SimulatePortfolio`; optional resource
  caps and actual output-byte accounting remain intact
- CASH observer, fee scenario, freshness, full-day return and report-consumer rules
  retain their existing gates
- Existing Coinbase/Binance/Polymarket local-file importers remain separate and
  provider-specific; none is presented as a general Hyper history importer
- Original capture/failure evidence remains historical, never a new third-party
  dataset or permission to collect again

## Remaining acceptance

The current image recipe still selects native-paper without the offline Hyper
feature, so installed capability is not claimed. Real registry/materialization,
scientific qualification, historical PIT, full database/native-service recovery,
frontend generation and deployment remain separate acceptance conditions.
Current verification for this restriction is Rust compilation and offline tests
only, with no live Hyper collection.
