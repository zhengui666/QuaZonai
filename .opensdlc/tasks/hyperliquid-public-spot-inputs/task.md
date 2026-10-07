# Hyperliquid offline source parsing

## Current boundary

The owner's 2026-10-07 instruction prohibits self-collection of Hyper data.
This supersedes the earlier A1 public-observation candidate and its collection
examples. No Hyper metadata refresh, HTTP candle polling, WebSocket recording,
replacement collector or network fallback is exposed by this candidate.

The former standalone binary, operator implementation and source-tools alias are
removed. The job capture operation and its network producer are removed. These
are absent even when all QZ features are enabled; tests exercise both CLI parsers'
rejection without invoking a network client.

The original A1/V3 source, prior real observation, failure and validation evidence
remain historical artifacts. Their previous commands are not current instructions
or permission to collect again. Controlled fixtures remain controlled fixtures;
previous QZ-collected evidence is not relabeled as a third-party public dataset.

## Offline capability

`hyperliquid-offline` enables reading already supplied source sidecars and native
catalogs. It calls the official 0.63.0 `CandleData`/`parse_ws_candle` parser and keeps
native instrument/precision/clock/row equality checks, original source locators,
immutable selection, explicit fee acceptance and UNVERIFIED historical availability.
The existing explicit CASH simulation and catalog preparation paths stay local.

The official adapter has no parser-only feature. Its unchanged crate still has
non-optional network/runtime dependencies and compiled HTTP/WebSocket modules.
QZ uses only its parsing API here; this does not claim the executable contains no
network dependencies. No official implementation is copied, patched or forked.

## Input limitations

The supported legacy sidecar format describes original one-minute Hyper REST
response values and their original observation clocks. Reading it does not create
new observations or establish public-third-party provenance. Missing source
objects, native definitions, original clocks, licensing, fees or coverage remain
missing; they are not manufactured or fetched as a fallback.

No general Hyper CSV/S3/archive adapter is added. Coinbase and Binance importers
remain provider-specific, and cannot relabel their records as Hyper. A future
existing-file adapter needs its own verified source format and scope.

## Verification boundary

Only relevant Rust compilation and offline controlled tests are permitted for this
change. No real Hyper operation, database action, frontend generation, remote Git
publication or deployment is part of it. Earlier capture evidence and test counts
are not a fresh acceptance result for this offline-only candidate.
