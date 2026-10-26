# Hyperliquid historical data plugin and 100ms BARs

## Goal

Continue the [existing history integration](../hyperliquid-public-history/task.md)
on a new branch from current remote `dev`, expose it through the current data
plugin framework, and support at least 100ms historical grain. The user clarified
that data must come from existing official, GitHub **or** Hugging Face archives,
including third-party publishers; no live market collection is requested.
The subsequent request prefers millisecond data and asks for TradingVPS latency
and jitter research. Retain source milliseconds and configurable BAR intervals;
do not choose an execution period from unverified latency marketing.

Working branch: `codex/hyperliquid-history-100ms`, based on remote `dev`
`133b8fd0e9b2811007a937fc7f8c1d4a737e551a`. Endpoint is a reviewed PR merged into
remote `dev`, with applicable final-Head CI and explicit clean GitHub Codex review
as required by [AGENTS](../../../AGENTS.md). No deployment or research qualification.

## Change

- Reuse the fixed Hyperliquid profile and `hub.py`; no second downloader, live
  collector, fixed coin list, content hashing or new freezing mechanism.
- Add disabled `qz.data.hyperliquid` in Settings → Data Plugins. Load local history
  offline, optionally download an explicit date range of existing trade files,
  and optionally run the native importer using local definitions.
- Add mutually exclusive native `--bar-milliseconds` / `--bar-seconds`, plus
  supplied `bar_milliseconds` mappings. Official Nautilus builds exact 100ms BARs
  from original millisecond trades, omitting empty intervals and retaining clocks.
- Preserve earlier results on failure and cooperative process cancellation on
  provider disable/service close. Return archive/import evidence with `ready:false`;
  original research preparation and partitions remain explicit.

## Sources and coverage

The [source guide](../../../runtimes/data/hyperliquid.md) compares official history,
official historical GitHub, ProbioticFarmer, Chainticks and node-fill mirrors.
The selected fixed ProbioticFarmer archive has prior 2026-10-16 evidence: 443 dates
of perp/spot fills, 165 dates of outcomes and a native import of 38,292 trades in
265 markets. Those observations are not a fresh verification of this patch or a
claim of complete exchange-wide history. 1s candles/minute books cannot establish
100ms observations. The 100ms path selects trades only with original taker filters.

## Verification and delivery status

- Existing `test_hub.py`: 2 checks passed locally on 2026-10-26, covering all-symbol
  file selection, missing partitions and relative download paths.
- Historical provider tests: 5/5 passed; deployment workflow checks: 5/5 passed.
  The independent reviewer executed plugin/registry/deployment checks 14/14 and
  reported no remaining findings at `e261b259`. These are protocol/deployment
  checks, not fresh real-source or native readback evidence.
- Native millisecond/readback checks and complete TypeScript checks are running.
  Final-Head CI and explicit clean GitHub Codex review remain pending on
  [PR #242](https://github.com/zhengui666/QuaZonai/pull/242), targeting `dev`.
- A fresh source inventory/download is blocked: `huggingface.co` is outside this
  environment's network policy and the proxy returned CONNECT 403. No source
  refresh, real-data 100ms re-import, research qualification or deployment is claimed.
- Private project-board synchronization is pending: its operations reference could
  not be read and no `board_read` / `board_update` tool is available in this session.

Native compilation uses the repository-pinned official Rust 1.98.1 and Nautilus
0.63.0. Protocol tests use controlled operators; only executed native tests count
as actual native decoding/readback evidence. Final evidence belongs in the PR.
