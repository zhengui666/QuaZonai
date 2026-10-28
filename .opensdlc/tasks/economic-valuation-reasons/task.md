# Explain missing daily economic valuations

## Task

The owner authorized a minimal daily valuation diagnostic after an actual primary C01 run had only its first daily equity available. The first known funding cause began later than the first unavailable NAV. Parent commit: `1e6a0b39b4641672569739c69e3e0e5133837c39` (reviewed funding-boundary fix).

Implement locally and obtain independent review first. Do not push or open a PR until parent PR262 merges; then rebase onto actual `dev` and review the final tree. No service change, research admission or historical replay.

## Design

Keep the existing nullable value and daily `unknown_reason` contracts. `value()` accepts an optional reason collector. Scheduling passes `None`; only daily evaluation collects the first encountered failure. Nonzero balance FX failures identify ledger, currency, amount, source/instrument and observation clocks; perpetual PnL failures identify quantity and context/cohort mark clocks. Reporting USDT0 FX and funding unavailability have separate prefixes. Mark causes distinguish missing, future, stale and absent price fields. Only one route candidate is described, not a list of markets.

All diagnostics are formatted only on an already-failed valuation. They read existing state without advancing context readers. Existing conversion rates, valuation arithmetic, evaluation order, missing policy, daily returns and trading behavior are unchanged. Valuation causes are per-day and can recover; existing irreversible funding semantics are retained. Full source data and original results are not rewritten. No Wire field or generated contract changes.

## Verification

- Existing high-precision target reused after its prior writer confirmed idle; no new Cargo target.
- `CARGO_TARGET_DIR=/home/zzy/.cache/quazonai-economic-check CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo test -p job --features high-precision ticks::economic --lib -- --test-threads=1`: 22 passed.
- New small native cash test buys spot, retains a fresh bridge and unused margin collateral, then makes only the nonzero spot balance stale. Daily NAV stays unavailable and the reason identifies currency/instrument/last/at/age. Zero balance and refreshed mark controls remain available; the failure does not become sticky.
- Existing context test now checks source and last timestamp when a stale explicit context still cannot fall back to a fresh trade.
- `cargo fmt --all` and `git diff --check`: passed.
- A local compile first found an unintended extra argument in an unchanged test helper call; corrected before the successful run. No native numeric or model formulas were changed.

Final rebased-head CI and review are pending. No historical economic replay or new trial was executed.
