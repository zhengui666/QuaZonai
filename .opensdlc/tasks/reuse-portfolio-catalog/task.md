# Reuse the selected portfolio catalog

## Request and scope

The owner authorized questioning QuaZonai's Rust architecture and removing evidenced duplicate work without changing the research business model or third-party implementations. Integration target: [PR #166](https://github.com/zhengui666/QuaZonai/pull/166), based on `6139e5421cfb45aebfd56d5d65510103ddf6c428`.

Only [portfolio.rs](../../../apps/job/src/portfolio.rs) and [study.rs](../../../apps/job/src/study.rs) change. The release-transaction stack fix and Rust-only CI changes are separate work.

## Evidence and change

For rolling liquidity, Portfolio Build loaded a native catalog inside `prepare`, then loaded the same selection again for `last_bar_notionals`. Study repeated both loads at each cutoff. Each load discovers Parquet files, reads their footers, queries/decodes native BARs and validates the selected market.

Callers now load one `NativeMarketData` and borrow it into internal preparation. The same market supplies the rolling-liquidity references. It is explicitly dropped before allocation/solver input construction so decoded BARs do not survive into that phase. Non-rolling work still loads once. The independent simulation replay keeps its own catalog load. A new selection is loaded at every Study cutoff; there is no cross-cutoff or process-wide cache.

Forecasts, calibration, fuel accounting, native solving and output/domain validation are unchanged. Source authorization, event windows and availability cutoffs still come from the original request and native catalog loader.

## Verification boundary

Existing regression entrypoints include `cargo test --locked -p job --test study --test allocation --test managed`. The managed Build rolling-liquidity case covers original policy binding, missing/wrong-role inputs, expiration, injected liquidity and output tampering. The Study rolling-liquidity case covers changing per-cutoff notionals, availability/staleness rejection, policy identity, zero liquidity and infeasible allocations. Allocation tests cover the solver inputs; they do not by themselves establish Build pipeline execution. No tests are removed.

This cloud workspace has no Rust compiler/Cargo. Compilation and these tests remain unexecuted locally and require final-source Rust CI after authorized publication. Eliminating duplicate calls is a source-level observation; no timing, throughput, memory percentage or runtime-performance claim is made.
