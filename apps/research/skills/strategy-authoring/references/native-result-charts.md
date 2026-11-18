# Presenting native research evidence

QuaZonai displays original native result tables and supported charts in the research result page. There is no `render_chart` tool. Do not invent a display call or write a replacement backtest/statistics engine. The examples below specify how to use existing evidence, not fabricated research outcomes or restrictions on strategy design.

## Identity and reading

A research result's `resultTaskId` identifies the original Pi tool request. Its native backtest child is a different Task. Use the native backtest tool output's `taskId` and `evidenceTables` references when reading. Do not substitute the tool ID, infer IDs by arithmetic, or rerun a backtest just to obtain a chart.

For a returned table reference, call the existing tool with the actual values:

```text
native_assessment_read_exposure {
  taskId: <actual native backtest taskId>,
  path: <exact path from evidenceTables.tables>,
  offset: 0,
  pageSize: 200
}
```

Continue at the returned `nextOffset`, not `offset + pageSize`. Byte limits may shorten a page. `nextOffset: null` ends only that table. State which original Task, table and row ranges were actually read; do not claim full review from a reference, a Web chart, or a row count. For a large compact result, follow its existing `$.summary` / `summaryPath` instructions first. An oversized original row remains unread, not zero or discarded evidence.

## Example 1: official daily returns

If the completed original result includes `equity_performance.returns`, use its `equity_returns` table reference. At the top level its path is `$.equity_performance.returns`; nested folds use their returned full path instead.

Each original row contains `ts_event_ns` (nanosecond string) and `value` (number or null). Read the accompanying `equity_performance.status`, `reason`, `currency`, `statistics`, and `notes` through the original result/summary. The Web page can show these rows as a time series. Keep null as missing. Do not replace a missing account-equity return with a position return, recompute CAGR/Sharpe, or call a brief training run independent validation. Preserve the original annualization parameters and the official analyzer's calendar-day carry-forward limitation.

A useful write-up identifies the observed window, the actual number of read rows, the original units and missing rows, and explains the observed pattern without claiming unseen history. It links to the original research result rather than pasting invented values into a chart example.

## Example 2: account equity and valuation limitations

Use the `portfolio_snapshots` reference, normally `$.canonical_result.portfolio_snapshots`. Relevant pinned-native fields are `account_id`, `ts_event`, `total_equity`, `is_stale`, and `unpriced_instruments`.

`total_equity` contains native Money strings: a decimal amount followed by its currency. The Web view keeps accounts and currencies separate; it does not add unlike currencies or substitute cash balances for equity. Original nanosecond clocks and decimal strings remain in the table and tooltip. Same-millisecond records are distinct records, not duplicates to remove. A snapshot with unpriced instruments is not plotted as a valid full equity value; stale/carried valuation remains explicitly identified. Neither an incomplete snapshot series nor final positions establishes peak exposure or margin adequacy.

## Example 3: positions, fills and parameter cases

For positions, the view plots the original `realized_pnl` Money value per position record, not a newly calculated trade return or full-history profitability distribution. Open and snapshot records retain their original meaning. For fills, the pinned canonical structure is `{client_order_id, event: {Filled: {...}}, order_event_ordinal}`; the display adapter uses the original `Filled.ts_event`, `last_px`, `currency`, and `instrument_id`. Fees, quantities and other original fields remain available in the table. Do not infer total PnL from fill prices.

The Web run selector lists original native Tasks, including failed and unfinished cases. A parameter batch requires explicit case selection; a successful survivor is not a selected winner. Do not average review stars, hide failed attempts, claim a parameter case is an independently qualified strategy, or recreate the retired scan workflow to make a plot.

## Display scope and exact values

The current chart describes only the loaded table page. Other pages are not silently merged, sampled or claimed as reviewed. Each page shows the original row interval and total. Missing fields, explicit null, numeric zero and empty tables are different states. Unsupported schemas remain original tables rather than guessed charts. Reading or drawing a page never changes the original Task, account, catalog, or report.

The public Web client uses the existing owner-authenticated service:

- `GET /api/v3/research/:id/native-backtests` lists native children and their `ownerTaskId`; retain its cursor.
- `GET /api/v3/research/:id/native-tables?taskId=...&offset=0&pageSize=200` lists table references.
- Add the returned, URL-encoded `path` to read that table's original rows.

The table API returns `offset`, `endExclusive`, `totalRows`, `nextOffset` and a 50 KiB maximum payload. It accepts pages of 1–1000 rows, but may return fewer. It is read-only and does not access arbitrary workspace files. These HTTP endpoints are the external/Web interface, not additional tools for this internal Agent. Continue to use `native_assessment_read_exposure` for internal evidence reading.
