# Native instrument updates

This is the published `nautilus-backtest 0.63.0` package, from upstream commit
`a0400251110653b6d8ae6a9b5b89c4543fa85a2d`, directory `crates/backtest`.
The original [license](LICENSE), copyright headers, manifest, source and tests are
retained. `upstream-sha256.json` records the original published files before the
QuaZonai changes; it is not a checksum of the modified package. Cargo's local
download marker and registry checksum file are omitted.

The 2026-09-27 patch adds `InstrumentUpdate` as typed native `Data::Custom`, routes
it through the existing backtest data iterator at its original `ts_init`, updates
the same exchange matching engine, and publishes the definition through the
native DataEngine/cache. The exchange checks version time and unchanged expiry.
Matching, order compatibility, queue resets, fills, account state and settlement
remain in the unmodified `nautilus-execution` crate. There is no extra replay loop.
The scientific job additionally restricts accepted changes to its validated
tick-only definition history. Equal-time ordering follows native insertion order;
QZ rejects ambiguous update/market-data timestamp ties before replay.

Modified upstream code files are `src/lib.rs`, `src/engine.rs`, and `src/exchange.rs`;
`src/instrument_update.rs` is added. All changes are marked and remain under
LGPL-3.0-only. Root `Cargo.toml` uses a source patch at the same locked version.
Two package-external links in `benches/BENCHMARKS.md` point to the immutable
upstream source instead of nonexistent files in the consuming repository.
Remove this override when an upstream version exposes the same verified path;
an upstream `Data::Instrument` variant alone does not prove exchange forwarding.

The original package is available from
[crates.io](https://crates.io/crates/nautilus-backtest/0.63.0), and source from
[the immutable upstream revision](https://github.com/nautechsystems/nautilus_trader/tree/a0400251110653b6d8ae6a9b5b89c4543fa85a2d/crates/backtest).
