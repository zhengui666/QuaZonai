# TradingVPS latency investigation

Research date: 2026-10-26. The candidate currently investigated is **TradingVPS.io**;
the similarly named TraderVPS.com is a different brand. Confirm the intended
provider before choosing a region or plan. No VPS was purchased or logged into.

## Evidence

| Source | What it establishes | What remains unknown |
| --- | --- | --- |
| [TradingVPS regions](https://tradingvps.io/) | Advertised Chicago, New York, London, Frankfurt, Amsterdam and Dublin locations; Tokyo/Singapore advertised as planned expansion at inspection time | Actual routing, contention and latency to Hyperliquid from a particular purchased instance |
| [Provider network article](https://tradingvps.io/the-complete-guide-to-network-performance-for-traders/) (2025-10-27) | Provider describes roughly 1ms Chicago pings to nearby Rithmic, Tradovate and IBKR infrastructure | This is neither a Hyperliquid measurement nor a published p50/p95/p99/jitter dataset |
| [Hyperliquid node documentation](https://github.com/hyperliquid-dex/node) | Recommends Tokyo for low-latency node deployment | A node location recommendation is not a measurement of the public API route from TradingVPS |

No reproducible independent TradingVPS → Hyperliquid latency distribution was
found. Public evidence cannot establish a stable 1ms, 10ms or 100ms complete
decision/execution cycle, or a smallest supported period. A ping to another broker
and a probe from this development environment would not answer that question.

## Data precision and execution timing

The historical source's millisecond event timestamp, an offline BAR interval and
an execution host's network RTT are different quantities. Slow transport does not
prevent precise offline millisecond aggregation. Preserve original millisecond
trades and make BAR aggregation configurable; do not infer a sampling period from
an unmeasured network route. A 1ms timestamp does not prove submillisecond event
ordering or original reception time. Historical availability and PIT limitations
in the [source guide](hyperliquid.md) still apply.

The current patch adds historical data, not a live execution service. No measured
TradingVPS performance, real-time execution SLA or strategy profitability is claimed.

## Required on-instance measurement

Before deployment, test each candidate region on the actual plan under the expected
CPU/disk workload. Over a representative multi-day window, separately record:

- TCP/TLS setup and persistent WebSocket ping/pong RTT to the actual Hyperliquid
  endpoint, with UTC sample time, region, plan and connection/reconnect state.
- p50, p95, p99, maximum, adjacent-sample jitter and timeout/reconnect rates. Retain
  raw latency samples so percentiles and failures can be independently recomputed.
- Process scheduling delay and end-to-end receive/compute/publish wall time under
  load. Use monotonic clocks for durations. Do not interpret RTT/2 as measured
  one-way latency or compare unsynchronized exchange/host clocks as network delay.

Then choose an execution deadline that covers the observed tail and workload,
with an explicit policy for stale inputs, missed deadlines and reconnections.
Without those observations there is no defensible numerical minimum period to
substitute. Offline raw-data precision can remain 1ms regardless of that decision.
