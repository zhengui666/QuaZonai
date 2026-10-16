# Native source preparation

## Scope and delivery boundary

Follow-up to the [acquisition foundation](../free-source-acquisition/task.md) in
[PR 138](https://github.com/zhengui666/QuaZonai/pull/138), based on
`a0450ee402a2ea45b83c69f94455587efcc5407b`. This is part of the owner's autonomous
`dev` iteration. The follow-up PR and merge are coordinator-owned and pending.

The [operator guide](../../../runtimes/data/source-plugins.md) is the maintained
capability and usage contract. Deliver a useful source-specific acquisition to
native-artifact path by reusing existing source/native components. Keep operator
preparation separate from service registration, data-use grants and historical
research qualification. No new server/UI, paid API, account, credential, real
order, development agent or dependency fork is introduced.

- A capability registry delegates HTTP acquisition to `acquire.py`, immutable
  archive acquisition to `snapshot.py`, and Polymarket source interpretation to
  the existing `polymarket-history capture/archive` commands. Sampled marks have
  no BAR/trade conversion. Arbitrary Hugging Face files have no invented converter
- `catalog-prepare ingest-candles` consumes frozen Coinbase OHLCV acquisition and
  original native `CurrencyPair` definitions. It rechecks source request bounds,
  clocks, hashes, source rows, derived lineage, exact decimal values, native
  precision/grid/currency identity and definition availability. It writes a new
  catalog, reads original BARs/definitions back, rechecks frozen inputs and
  publishes `import-report.json` last. The old root preparation invocation remains
  available unchanged
- Native values use pinned `Price`, `Quantity`, `Bar`, `InstrumentAny` and
  `ParquetDataCatalog` APIs. Input decimal text is checked for exact representation
  before and after native construction; rounding never repairs invalid source
  precision. Native selection uses BAR event labels, which differ from source
  candle bucket-start selection. Actual retrieval clocks remain `ts_init`
- Failed conversion never deletes or replaces original acquisition/raw evidence.
  Native partial artifacts remain unpublished without the final report. No
  conversion result creates a source permission, PIT or research qualification

## Same-receipt BAR invariant

The pinned official Nautilus 0.63.0 APIs explicitly allow nondecreasing
initialization clocks: `nautilus_model::data::is_monotonically_increasing_by_init`
and `ParquetDataCatalog::check_ascending_timestamps` use `<=`. The pinned package
revision is `a0400251110653b6d8ae6a9b5b89c4543fa85a2d`; see the
[official model API](https://github.com/nautechsystems/nautilus_trader/blob/a0400251110653b6d8ae6a9b5b89c4543fa85a2d/crates/model/src/data/mod.rs)
and [official catalog API](https://github.com/nautechsystems/nautilus_trader/blob/a0400251110653b6d8ae6a9b5b89c4543fa85a2d/crates/persistence/src/backend/catalog.rs). Native
`BacktestEngine::add_data` stably sorts initialization times, and `run` processes
same-time data without advancing its clock between those records. These public
APIs were inspected in the official pinned registry packages, not copied or
modified in this repository.

A pre-change real native regression wrote/read four distinct event-time BARs with
one unchanged receipt timestamp and processed all four through the native engine
without orders. The original QZ `load_catalog` rejected that same batch. Running the final
regression with the original strict predicate produced exit 101 and
`CATALOG_NONUNIQUE_OR_REVISED_BAR`; restoring the approved predicate passed
the same regression and all 13 catalog tests. After
that proof and coordinator approval, only the BAR predicate changes to:

`previous.ts_event < bar.ts_event && previous.ts_init <= bar.ts_init`

The final regression verifies native Parquet, native engine and QZ readback all
retain the same four timestamps. Negative tests retain rejection of duplicate
event times, decreasing availability, event-after-init, decision-cutoff breaches
and instrument-update receipt ties. No instrument/control ordering is relaxed;
no per-row timestamp offsets are fabricated.

This is an inspection/preparation correctness change, not blanket scientific
admission. Forecast output still requires strictly increasing availability and
labels available later than the prediction; those checks are unchanged. A REST
backfill with shared late retrieval clocks cannot become historical prediction
research merely by passing catalog measurement. Current definitions remain late
and source availability remains unverified.

## Verification

Use the pinned compiler, `CARGO_PROFILE_DEV_DEBUG=0`, and two build jobs:

```sh
python3 -B -m unittest discover -s runtimes/data -v
cargo fmt --all -- --check
cargo clippy --locked -p job --features polymarket-history,catalog-prepare \
  --all-targets -- -D warnings
cargo test --locked -p job --features polymarket-history,catalog-prepare \
  --bin polymarket-history --test catalog_prepare --test catalog --test managed \
  --test instrument_versions --test native_instrument_updates \
  --test candle_ingest --test batched_observation
cargo run --locked -q -p job --features catalog-prepare \
  --bin catalog-prepare -- --help
cargo run --locked -q -p job --features catalog-prepare \
  --bin catalog-prepare -- ingest-candles --help
```

The local native matrix passed **83 tests**, including original Polymarket
capture/chain/archive behavior, catalog/partition preparation, managed jobs,
instrument-history handling, equal-receipt BAR ordering, nine new candle importer
checks and the real Python registry → built Rust converter → native Parquet path.
The nine importer tests include unchanged root partition preparation and actual
quality measurement while preserving `FIXTURE`/`UNVERIFIED`. Python discovery
passed **72 tests**. These are controlled local checks, not source qualification
or exact-head hosted CI. Formatting and all-target Clippy with `-D warnings` passed locally. Coordinator
review remains required before publication; exact-head CI and clean independent
review remain required before merge.

## Separate bounded public-source observation

On 2026-09-30 the registry downloaded eight source files totaling **30,900,196
bytes** from
[lokima's fixed public capture](https://huggingface.co/datasets/lokima/polymarket-btc-eth-5m-updown-8h/tree/97d05eac7324a410db4dabb4f06b25cc21aae23f),
retaining its ODC-BY source terms and provenance. Snapshot retrieval time was
`2026-09-30T18:02:11.692736Z`; local `snapshot.json` SHA-256 was
`27ebd2f39f5a58ab8442c435d9e5f5b2b71229b5db265283141cb3a281aa1278`.

The actual registry then called the existing native capture adapter for market
`btc-updown-5m-1785357600`, selection `[1785357600,1785357900)`, and one-second BARs.
The source-bound publication completed at `2026-09-30T18:03:12.592469409Z` with
**1,946 observed trades, 483 BARs, two original payout records and two instrument
definitions**. The registry validated the report, detached original evidence,
counts and native artifacts. This is one bounded recorded market window, not
all-market coverage, a current live trading feed or scientific qualification.

Original source responses, downloaded bytes and native output are retained outside
Git. Coinbase's live API remains unavailable from this executor; crypto conversion
was tested with explicitly synthetic, exact source fixtures and real native
components. No substituted response is described as live Coinbase data.

## Remaining work

- Service/CLI/UI lifecycle integration with actual Runtime catalogs, permissions,
  immutable revisions, frozen InputSets and fresh `DATA_VALIDATE`
- Complete original historical definitions, fees/calendars/universe membership,
  source rights and availability evidence for the intended research scope
- Historical crypto sources with original sequential receive/publication evidence
  for forecast/label eligibility; current REST retrieval is insufficient
- Wider Polymarket history and free-source coverage only where each existing
  adapter's original evidence checks pass; missing source capabilities stay visible

The user's complete usable, admitted research-plugin objective remains open beyond
operator acquisition and native preparation.
