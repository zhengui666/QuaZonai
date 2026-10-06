# Date-only HF selection with unknown market membership

## Goal

Permit a source-backed date or explicit-file selection when a partition's
market membership is unknown, without fabricating market identifiers or
weakening complete-file byte budgets.

## Scope

The existing `qz.hf_partitions/1` accepts an additive `market_mapping: UNKNOWN`
marker only with an empty `markets` array. Unmarked nonempty market mappings
remain compatible. A market-filtered request fails if an unknown partition
remains in its date/include candidate set. Python planning, preserved-plan
validation and the offline Rust selection reader retain the same distinction.
Acquisition remains complete-file based; no remote row-group reader, native
instrument inference, registration, research run or source upload is added.

## Verification

- Run the data source Python regression suite, including date/include scopes,
  unknown/empty/pseudo-all mappings, preserved handoffs, cache reuse and budgets
- Replay the existing 42-object archive metadata at fixed inventory revision
  `f0a74ad5a659ab0d1d37af4840d619af6e12ed3c` with network access disabled in the
  test; 42 paths and 21,470,666,882 bytes must be preserved
- A single December 2025 day must still require its 5,132,274,521-byte object
- Run the offline Rust handoff tests for known and unknown mappings; these
  envelope fixtures are not real market data or scientific acceptance
- Independently review the final candidate and rerun affected checks after fixes

## Delivery boundary

No source data upload, HF index publication or real-data acquisition is included.
Metadata replay does not reverify current remote availability, historical
completeness, market coverage or PIT.
