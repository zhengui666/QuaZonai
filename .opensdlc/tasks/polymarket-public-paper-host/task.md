# Real current Polymarket data to native Paper

The integrated candidate registers `job polymarket-paper run` and its native
modules under the existing native-paper feature. It consumes an original V2
Paper claim, its original execution assumptions, Dataset view and frozen native
metadata. A separate official data-only child retains native instrument, tick,
connection, queue and lifecycle observations before sending bounded NDJSON to
one official event-time Paper simulation in the parent. No venue execution
client, wallet, real financial order, new model, fork or upstream patch is added.
Current public capture is ONLY Paper/connection acceptance data. Existing HF
raw sources remain the research history and are not replaced or relabelled.

All modules and CLI tests are reachable through the existing native-paper
feature. This isolated feature candidate has passed Rust 1.98.1 all-target checks
and 24 focused tests; it does not claim a deployed Paper account is verified.

The source uses official typed message-bus listeners through final node drain,
plus the official public market WebSocket for lifecycle events omitted by the
high-level data-only client. Lifecycle subscriptions are ready before native
ticks begin. Actual disconnect, reconnect, overflow or unresolved drain makes
the segment incomplete. The parent independently deadlines and reaps a stalled
source child, including incomplete-line or continuously buffered output.
Startup failures retain phase/error records, close the producer queue and sync
the writer. Official runtime shutdown is bounded; its timeout is never treated
as proof of native node shutdown or source completeness.

The official L1 book updates on TradeTick even with trade_execution disabled.
This quote-only slice therefore retains raw trades as source evidence but feeds
only original quotes to its official engine; execution coverage ends at the
last completed quote timestamp. The regression first reproduced a wrong 0.80
fill from a Trade following a 0.49/0.51 quote, then verified its rejection.

The host preserves original target/settings/constraints, metadata clocks and
actual native cash-account identity. Public refresh terms must match original
frozen terms; source gaps, late timestamps, changes, unsupported status/close,
source EOF tail execution, incomplete valuation or shutdown remain unavailable
rather than zero. Only an available result exports actual original
PortfolioSnapshot objects and native binding for the existing converter and
account CLI relay; failure retains raw diagnostics without a relay-eligible
binding. No server receipt or scientific qualification is claimed by files.

Real public feed/Paper acceptance and account relay/readback remain unperformed.
The original direct public connection failed DNS resolution. After the explicit
proxy adapter, the same public endpoint reached TLS certificate validation and
failed with a fixed, secret-safe certificate error, retained gap and exit 1.
No ready state or market ticks were observed. TLS validation remains enabled.
No system network or trust settings were changed. Missing inputs are
reported by exact object/path/field; fixture claims, invented fee metadata and
shifted source clocks cannot replace them. Executed protocol tests cover
no-overwrite, incomplete records, original clocks,
queue loss, expired deadlines despite buffered frames, half-line child cleanup,
retention without a running actor, and delayed transport-loss observations.
The existing 14 library and two CLI cases were rerun successfully after changes.
Three additional engineering integrations pass: original quote-driven fills,
nonzero official fee, native terminal valuation and actual snapshot converter;
pending original-latency EOF rejection; and rejection of a trade-only tail.
Fixtures are explicitly synthetic and provide no real market/PIT/claim evidence.
Five proxy tests additionally verify explicit selection/validation, secret-safe errors,
and actual official HTTP factory, data WS and lifecycle WS requests at a local
rejecting proxy, with no direct fallback. All 24 pass without failures or ignored
tests. Seven changed Rust files pass rustfmt. Full-workspace fmt reports
only a pre-existing trailing blank line in Store handoffs.rs, left unchanged.

Public transport selection is explicit through `--proxy-env ENV_NAME` on the
probe, Paper host or source. The source reads that existing variable once and
strictly validates it with the official HTTP(S) proxy API. One configuration
snapshot goes to the official data factory (Gamma/CLOB/Data API/market WS) and
its lifecycle WS. Only the variable name crosses the child argument boundary;
proxy values and credentials are never included in evidence or diagnostics.
Without this option, official defaults remain: HTTP may use environment proxy
detection while WS has no explicit proxy. The adapter does not select a route
automatically or change network settings. Invalid configuration fails without
a direct-connection fallback. The 20-second outer lifecycle guard accommodates
the official unchanged 15-second dial budget; the independent parent deadline
covers bounded startup, capture and shutdown.
