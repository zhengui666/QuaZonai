# Searchable research, factor and portfolio tables

## Goal

Replace research's split collection with a full-width table and give research,
factors and portfolio backtests consistent business filters, full-collection
search, server-side sorting and pagination. Long descriptions, original IDs,
model source, reports and execution details belong in independently addressable
detail pages. Preserve list state on return.

Base: remote `dev` at `a27cfb7f95c6b344092400424310845bfc1d3a30`.
The owner selected existing business classifications, independent detail pages
and a small set of reliable metrics. Portfolio tables must show Sharpe,
annualized return and maximum drawdown and support sorting these metrics.

## Implementation

- Add backward-compatible, lightweight `/api/v3/*/list` reads for research,
  factors, portfolios, Tick economic replays and review allocations. Match and
  sort the complete collection before pagination; keep existing CLI and detail
  contracts. Use the official Durable APIs and a rebuildable summary index.
- Keep execution, acceptance, independent evidence and review decisions distinct.
  Use original frozen dataset facts. Research/factors have no reliable original
  creation timestamps and must not acquire invented dates.
- Share URL-backed filters, accessible table sorting, paging, search debounce,
  selection return/focus, and responsive table presentation. Organize portfolio
  subcollections into separate tabs with independent detail routes.
- Project portfolio Sharpe and maximum drawdown from original native shared-account
  statistics. Annualize the complete-day geometric return with the original
  native 252/365-day convention. Keep full native daily returns and complete UTC
  days as separate metric bases. Unknown remains unknown. Display drawdown as a
  positive loss magnitude; retain the original signed statistic in detail.
- Sort original values, not rounded text. Nulls remain last in both directions;
  identity breaks ties. Compare capital only within one selected currency.

## Acceptance

Verify over 100 records, hidden/deleted conversations, combined filters, old
records found without loading more, stable cross-page sorting, decimal capital,
negative Sharpe, zero/missing IC, missing dates and ambiguous metric conventions.
Verify incremental updates and index rebuild using real Durable storage.

Exercise independent detail navigation, deep-link source filters, refresh,
back/forward, failure/retry, deletion, creation and original request recovery.
Check desktop/mobile, light/dark, keyboard and accessibility. Existing source
and qualified evidence boundaries must remain intact.

Run TypeScript/Web checks and the applicable real HTTP/browser regression.
Independent review and final-Head GitHub Codex review plus applicable CI are
required before merging into `dev`. Merge is separate from deployment.

## Evidence

Local validation:

- `make check-ts`: passed. Optional external/native fixture checks retain their
  existing skip conditions; this is not market or live-account evidence.
- `make check-web`: passed (typecheck, unit tests, production build).
- New real HTTP/Durable list checks: 3 passed, including 105 research records,
  full-collection filtering/sorting, precise capital, cache recovery and restart.
- Original native scalar projection checks: 5 passed, including spot CASH report
  ownership, 252/365 conventions, complete loss and missing evidence.
- Real-service browser acceptance: full-collection queries, independent details,
  return state, actual Durable reopen, mobile and axe passed with controlled data.
- Independent reviewers found and verified fixes for source-name initialization,
  failed-read retry, spot-report statistics and original metric detail, old
  research bookmarks, new-tab focus, and long names in detail.

Final browser regressions, final-Head CI and GitHub review are tracked on the PR.
The private project board currently cannot be reached from this environment:
its HTTPS CONNECT request is rejected with HTTP 403 by the outbound proxy.
Board synchronization remains pending; this does not block repository work.
