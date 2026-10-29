# Small-capital tick replay

The existing score-weighted economic replay can repeatedly rebalance small accounts and requires a complete sleeve NAV before it schedules any target, including exits. Add an optional fixed-notional policy for the existing tick economic consumer, leaving the default unchanged.

The policy admits fresh signals above declared round-trip costs, reserves actual available capital, holds the first filled quantity without adding, and schedules exit when the entry-time fast EMA is reached or the holding period expires. Execution remains on later real cohorts with native quantity, minimum, participation, latency and funding constraints. Unknown NAV blocks entries but not closing intent; partial fills and residual positions remain visible. FX bridges and reserve currencies serve conversion, not alpha, under this policy. Preserve legacy requests and results.

This is a research execution model, not a claim of trading profitability or proof of executable quotes, market completeness or historical receiving-time correctness. Historical data gaps remain unknown.
