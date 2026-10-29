# Running backtest visibility

## Goal

From current remote `dev`, support inspecting running portfolios, recorded progress and current native equity in CLI, service Skill and Web. Reuse the existing equity projection/chart; factor library data displays only dataset names. Merge back into `dev` after final-Head checks and clean review.

## Acceptance

- Portfolio list/get retain queued and running portfolios; reads never resubmit work.
- Native shared-account snapshots reach running portfolio views with recorded phase and actual counters where available. No inferred percentage or synthetic equity.
- Web polls the same portfolio, shows progress and provisional curve using `EquityPlot`; completed output remains authoritative.
- CLI list/get expose progress and curve; watch/wait observe the same identity and detach without cancellation.
- Factor list/detail show registered dataset names without internal dataset IDs or market lists.
- Validate native snapshot transport, partial-to-terminal transitions, timeout/read-only CLI behavior and Web display. Partial protocol tests do not prove profitability or real account behavior.

## Delivery

Branch: `feat/running-backtest-visibility`, based on remote `dev` at `114344eb`.

PR: [#265](https://github.com/zhengui666/QuaZonai/pull/265).

Local verification: Web typecheck, 37 tests and production build; CLI typecheck and 14 protocol tests; native writer isolation tests; real Nautilus simulation subprocess preview before completion and catalog ensemble original-account snapshot comparison. Service full suite: 338 passed, 24 explicitly skipped for unavailable native fixtures/prerequisites, zero failures. Final-Head CI must run the applicable native/core/browser prerequisites before merge. These checks do not establish market profitability or real account authorization.
